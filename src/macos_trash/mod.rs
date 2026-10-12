//! A short-lived, re-executed copy of trache owns FileManager's asynchronous flushes.

mod ds_store;
mod observer;
mod protocol;

use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::{ffi::OsStringExt, process::CommandExt};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use observer::Batch;
use protocol::{DELETE, ERROR, READY, SUCCESS, invalid, read_frame, write_frame};

const WORKER_ARG: &str = "--__trache-macos-trash-worker";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug)]
pub struct TransportError(io::Error);

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "lost contact with trash helper; the last operation's outcome is uncertain and was not retried: {}",
            self.0
        )
    }
}

impl std::error::Error for TransportError {}

#[derive(Default)]
enum Mode {
    #[default]
    Unstarted,
    Remote(Client),
    Local,
}

#[derive(Default)]
pub struct Backend {
    mode: Mode,
    local: Batch,
}

impl Backend {
    pub fn delete(&mut self, path: &Path) -> Result<()> {
        self.delete_with_launcher(path, Client::start)
    }

    fn delete_with_launcher(
        &mut self,
        path: &Path,
        start: impl FnOnce() -> io::Result<Client>,
    ) -> Result<()> {
        if matches!(self.mode, Mode::Unstarted) {
            self.mode = match start() {
                Ok(client) => Mode::Remote(client),
                Err(error) => {
                    log::debug!("Trash helper startup failed; using foreground fallback: {error}");
                    Mode::Local
                }
            };
        }
        match &mut self.mode {
            Mode::Remote(client) => client.delete(path),
            Mode::Local => self.local.delete(path).map_err(Into::into),
            Mode::Unstarted => unreachable!(),
        }
    }

    /// Close IPC without waiting for the remote process, or settle the local fallback once.
    pub fn finish(self) {
        drop(self.mode);
        self.local.settle();
    }
}

struct Client {
    // Dropping Child deliberately does not kill or wait for the settling helper.
    _child: Child,
    input: ChildStdin,
    output: ChildStdout,
}

impl Client {
    fn start() -> io::Result<Self> {
        Self::spawn(Command::new(std::env::current_exe()?), STARTUP_TIMEOUT)
    }

    fn spawn(mut command: Command, timeout: Duration) -> io::Result<Self> {
        // Preserve only the explicit standard-stream mappings across exec. In particular,
        // a caller's `3>&1` must not keep a shell pipeline open during the grace period.
        // Enumerate before fork; the child only uses async-signal-safe syscalls.
        let inherited_fds: Vec<i32> = std::fs::read_dir("/dev/fd")?
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
            .filter(|fd| *fd > 2)
            .collect();
        command
            .arg(WORKER_ARG)
            .current_dir("/")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // Never use Foundation or allocate in this closure.
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                for &fd in &inherited_fds {
                    if libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) < 0 {
                        let error = io::Error::last_os_error();
                        if error.raw_os_error() != Some(libc::EBADF) {
                            return Err(error);
                        }
                    }
                }
                Ok(())
            });
        }
        let mut child = command.spawn()?;
        let input = child.stdin.take().expect("piped stdin");
        let mut output = child.stdout.take().expect("piped stdout");
        // Darwin's <sys/fcntl.h> defines F_SETNOSIGPIPE as 73 (libc does not expose
        // it on Apple targets). Suppress SIGPIPE only on IPC, preserving the CLI's
        // normal SIGPIPE behavior when the user closes a stdout pipeline.
        let ready = if unsafe { libc::fcntl(input.as_raw_fd(), 73, 1) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            read_frame(&mut DeadlineReader {
                inner: &mut output,
                deadline: Instant::now() + timeout,
            })
        };
        if !matches!(ready, Ok(Some(ref message)) if message == READY) {
            // No request has been sent, so killing/reaping and falling back is safe.
            let _ = child.kill();
            let _ = child.wait();
            return Err(ready
                .err()
                .unwrap_or_else(|| invalid("invalid trash helper readiness message")));
        }
        Ok(Self {
            _child: child,
            input,
            output,
        })
    }

    fn delete(&mut self, path: &Path) -> Result<()> {
        let path = std::path::absolute(path)?;
        let mut request = vec![DELETE];
        request.extend_from_slice(path.as_os_str().as_encoded_bytes());
        write_frame(&mut self.input, &request).map_err(TransportError)?;
        let reply = read_frame(&mut self.output)
            .map_err(TransportError)?
            .ok_or_else(|| TransportError(invalid("helper closed its reply pipe")))?;
        if reply == SUCCESS {
            return Ok(());
        }
        if reply.first() == Some(&ERROR) {
            let error = std::str::from_utf8(&reply[1..])
                .map_err(|_| TransportError(invalid("invalid helper error")))?;
            return Err(error.into());
        }
        Err(TransportError(invalid("invalid helper reply")).into())
    }
}

struct DeadlineReader<'a, T> {
    inner: &'a mut T,
    deadline: Instant,
}

impl<T: Read + AsRawFd> Read for DeadlineReader<'_, T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let mut pollfd = libc::pollfd {
                fd: self.inner.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let millis = remaining
                .as_millis()
                .saturating_add(1)
                .min(i32::MAX as u128) as i32;
            // One valid pollfd, borrowed for the duration of the syscall.
            let result = unsafe { libc::poll(&mut pollfd, 1, millis) };
            if result > 0 {
                return self.inner.read(bytes);
            }
            if result == 0 {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}

/// Dispatch before ordinary CLI parsing. The protocol is private, not a user option.
pub fn run_if_worker() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(WORKER_ARG)) {
        return;
    }
    if args.next().is_some() {
        std::process::exit(2);
    }
    // Broken IPC means settle completed moves, not die before their dispatch timer fires.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    // Worker owns the private pipes installed by Command; closing them releases all IPC.
    let mut input = unsafe { File::from_raw_fd(libc::STDIN_FILENO) };
    let mut output = unsafe { File::from_raw_fd(libc::STDOUT_FILENO) };
    let mut batch = Batch::default();
    let result = serve(&mut input, &mut output, &mut batch);
    drop(input);
    drop(output);
    // Reserve the standard descriptor numbers so a later open by a framework or the
    // observer cannot turn a stray stdio write into a write to some unrelated file.
    let null = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")
        .ok();
    if let Some(null) = &null {
        for fd in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
            unsafe {
                libc::dup2(null.as_raw_fd(), fd);
            }
        }
    }
    batch.settle();
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}

fn serve(input: &mut impl Read, output: &mut impl io::Write, batch: &mut Batch) -> io::Result<()> {
    write_frame(output, READY)?;
    while let Some(request) = read_frame(input)? {
        if request.first() != Some(&DELETE) || request.len() == 1 || request[1..].contains(&0) {
            return Err(invalid("invalid trash request"));
        }
        let path = OsString::from_vec(request[1..].to_vec());
        let path = Path::new(&path);
        if !path.is_absolute() {
            return Err(invalid("trash request must be absolute"));
        }
        match batch.delete(path) {
            Ok(()) => write_frame(output, SUCCESS)?,
            Err(error) => {
                let mut reply = vec![ERROR];
                reply.extend_from_slice(error.to_string().as_bytes());
                write_frame(output, &reply)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn shell(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn startup_rejects_wrong_versions_and_times_out_partial_frames() {
        assert!(
            Client::spawn(
                shell("printf '\\000\\000\\000\\001x'"),
                Duration::from_secs(1)
            )
            .is_err()
        );
        let start = Instant::now();
        let error = Client::spawn(
            shell("printf '\\000'; exec sleep 60"),
            Duration::from_millis(50),
        )
        .err()
        .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn lost_reply_is_fatal_even_if_the_source_disappeared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("disposable");
        fs::write(&path, "owned test file").unwrap();
        let mut command = shell(
            "printf '\\000\\000\\000\\016trache-trash/1'; dd bs=1 count=1 >/dev/null 2>&1; /bin/rm -- \"$1\"",
        );
        command.arg("fake-worker").arg(&path);
        let client = Client::spawn(command, Duration::from_secs(1)).unwrap();
        let mut backend = Backend {
            mode: Mode::Remote(client),
            local: Batch::default(),
        };
        let error = backend
            .delete_with_launcher(&path, || panic!("must not retry startup"))
            .unwrap_err();
        assert!(error.downcast_ref::<TransportError>().is_some());
        assert!(!path.exists());
        if let Mode::Remote(client) = &mut backend.mode {
            client._child.wait().unwrap();
        }
        backend.finish();
    }

    #[test]
    fn foreground_fallback_is_selected_once_and_keeps_success_receipts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!(
            "trache-fallback-{}",
            dir.path().file_name().unwrap().to_string_lossy()
        ));
        fs::write(&path, "owned fallback test file").unwrap();
        let mut backend = Backend::default();
        backend
            .delete_with_launcher(&path, || Err(io::ErrorKind::NotFound.into()))
            .unwrap();
        let missing = dir.path().join("missing");
        assert!(
            backend
                .delete_with_launcher(&missing, || panic!("fallback must stay local"))
                .is_err()
        );
        let destination = backend.local.receipts()[0].trashed_path.clone().unwrap();
        let start = Instant::now();
        backend.finish();
        assert!(start.elapsed() >= Duration::from_millis(2900));
        assert!(start.elapsed() < Duration::from_secs(11));
        assert_eq!(
            fs::read_to_string(&destination).unwrap(),
            "owned fallback test file"
        );
        fs::remove_file(destination).unwrap();
    }
}
