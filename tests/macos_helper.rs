#![cfg(target_os = "macos")]

use std::fs;
use std::io::{Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[path = "support/macos.rs"]
mod support;

fn trache() -> Command {
    Command::new(env!("CARGO_BIN_EXE_trache"))
}

fn send(writer: &mut impl Write, bytes: &[u8]) {
    writer
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    writer.write_all(bytes).unwrap();
    writer.flush().unwrap();
}

fn receive(reader: &mut impl Read) -> Vec<u8> {
    let mut length = [0; 4];
    reader.read_exact(&mut length).unwrap();
    let length = u32::from_be_bytes(length) as usize;
    assert!(length <= 1024 * 1024);
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).unwrap();
    bytes
}

struct Worker(Child);
impl Worker {
    fn start() -> Self {
        // Direct protocol tests also give us a waitable PID. Production creates its
        // own session in the parent's spawn path, which is checked separately below.
        let mut child = trache()
            .arg("--__trache-macos-trash-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        assert_eq!(receive(child.stdout.as_mut().unwrap()), b"trache-trash/1");
        Self(child)
    }

    fn delete(&mut self, path: &Path) -> Vec<u8> {
        let mut message = vec![b'D'];
        message.extend_from_slice(path.as_os_str().as_encoded_bytes());
        send(self.0.stdin.as_mut().unwrap(), &message);
        receive(self.0.stdout.as_mut().unwrap())
    }

    fn finish(&mut self) -> Duration {
        self.0.stdin.take();
        self.0.stdout.take();
        let start = Instant::now();
        loop {
            if self.0.try_wait().unwrap().is_some() {
                return start.elapsed();
            }
            assert!(start.elapsed() < Duration::from_secs(12));
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.0.stdin.take();
        self.0.stdout.take();
        if self.0.try_wait().unwrap().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

struct Files {
    temp: tempfile::TempDir,
    prefix: String,
}
impl Files {
    fn new() -> Self {
        support::wait_for_helpers();
        let temp = tempfile::tempdir().unwrap();
        let prefix = format!(
            "trache-helper-{}-",
            temp.path().file_name().unwrap().to_string_lossy()
        );
        Self { temp, prefix }
    }
    fn file(&self, suffix: &str) -> PathBuf {
        let path = self.temp.path().join(format!("{}{suffix}", self.prefix));
        fs::write(&path, &self.prefix).unwrap();
        path
    }
    fn trash(&self) -> PathBuf {
        PathBuf::from(std::env::var_os("HOME").unwrap()).join(".Trash")
    }
    fn destinations(&self) -> Vec<PathBuf> {
        fs::read_dir(self.trash())
            .unwrap()
            .filter_map(|e| {
                let e = e.unwrap();
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&self.prefix)
                    .then(|| e.path())
            })
            .collect()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        support::wait_for_helpers();
        for path in self.destinations() {
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.file_type().is_symlink() {
                fs::remove_file(path).unwrap();
            } else {
                assert_eq!(fs::read_to_string(&path).unwrap(), self.prefix);
                fs::remove_file(path).unwrap();
            }
        }
    }
}

#[test]
fn foreground_returns_with_pipe_eof_while_its_detached_helper_lives() {
    let files = Files::new();
    let paths: Vec<_> = (0..3).map(|i| files.file(&format!("{i}.txt"))).collect();
    let start = Instant::now();
    let output = Command::new("/bin/sh")
        .args(["-c", "exec \"$@\" 3>&1", "test-shell"])
        .arg(env!("CARGO_BIN_EXE_trache"))
        .args(&paths)
        .output()
        .unwrap();
    let foreground_elapsed = start.elapsed();
    assert!(output.status.success());
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    assert!(paths.iter().all(|p| !p.exists()));
    let helpers = support::helper_pids();
    assert_eq!(
        helpers.len(),
        1,
        "parent waited for the helper: {:?}",
        foreground_elapsed
    );
    assert_eq!(
        unsafe { libc::getsid(helpers[0] as i32) },
        helpers[0] as i32
    );
    support::wait_for_helpers();
    let store =
        support::ds_store::parse(&fs::read(files.trash().join(".DS_Store")).unwrap()).unwrap();
    let complete = files
        .destinations()
        .iter()
        .filter(|p| {
            store
                .get(p.file_name().unwrap().to_str().unwrap())
                .is_some_and(|r| r.name.is_some() && r.location.is_some())
        })
        .count();
    eprintln!(
        "foreground returned in {:?}; Put Back records: {complete}/3 (best effort)",
        foreground_elapsed
    );
    assert_eq!(files.destinations().len(), 3);
}

#[test]
fn worker_handles_partial_failure_collision_and_broken_reply_pipe() {
    let files = Files::new();
    let path = files.file("collision.txt");
    let mut worker = Worker::start();
    assert_eq!(worker.delete(&path), b"S");
    assert!(
        worker
            .delete(&files.temp.path().join("missing"))
            .starts_with(b"E")
    );
    fs::write(&path, &files.prefix).unwrap();
    assert_eq!(worker.delete(&path), b"S");
    let last = files.file("last.txt");
    // Drop the reader before sending the last request, as if the foreground process died.
    worker.0.stdout.take();
    let mut message = vec![b'D'];
    message.extend_from_slice(last.as_os_str().as_encoded_bytes());
    send(worker.0.stdin.as_mut().unwrap(), &message);
    let elapsed = worker.finish();
    assert!(elapsed >= Duration::from_millis(2900));
    assert!(!last.exists());
    assert_eq!(files.destinations().len(), 3);
}

#[test]
fn parent_cancellation_does_not_leave_queued_work() {
    let files = Files::new();
    let first = files.file("first.txt");
    let second = files.file("second.txt");
    let mut parent = trache()
        .arg("-iv")
        .args([&first, &second])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    parent.stdin.as_mut().unwrap().write_all(b"y\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while first.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(unsafe { libc::kill(parent.id() as i32, libc::SIGINT) }, 0);
    assert_eq!(parent.wait().unwrap().signal(), Some(libc::SIGINT));
    assert!(second.exists());
    assert!(!support::helper_pids().is_empty());
    support::wait_for_helpers();
}

#[test]
fn overlapping_invocations_return_operation_results_independently() {
    let files = Files::new();
    let first = files.file("parallel-a.txt");
    let second = files.file("parallel-b.txt");
    let mut a = trache()
        .arg(first)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut b = trache()
        .arg(second)
        .arg(files.temp.path().join("missing"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    assert!(a.wait().unwrap().success());
    assert_eq!(b.wait().unwrap().code(), Some(1));
    assert_eq!(files.destinations().len(), 2);
    support::wait_for_helpers();
}

#[test]
fn a_dead_helper_reports_an_error_instead_of_delivering_sigpipe() {
    let files = Files::new();
    let first = files.file("before-crash.txt");
    let second = files.file("after-crash.txt");
    let mut parent = trache()
        .arg("-i")
        .args([&first, &second])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    parent.stdin.as_mut().unwrap().write_all(b"y\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while first.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let helpers = support::helper_pids();
    assert_eq!(helpers.len(), 1);
    assert_eq!(unsafe { libc::kill(helpers[0] as i32, libc::SIGKILL) }, 0);
    support::wait_for_helpers();
    // The helper's read end is now definitely closed; the next write must yield EPIPE.
    parent.stdin.as_mut().unwrap().write_all(b"y\n").unwrap();
    let output = parent.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("outcome is uncertain"));
    assert!(second.exists());
}

struct Volume {
    _temp: tempfile::TempDir,
    mount: PathBuf,
}

impl Volume {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let image = temp.path().join("test.dmg");
        let mount = temp.path().join("mount");
        fs::create_dir(&mount).unwrap();
        assert!(
            Command::new("hdiutil")
                .args(["create", "-quiet", "-size", "32m", "-fs", "HFS+"])
                .arg(&image)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("hdiutil")
                .args(["attach", "-quiet", "-nobrowse", "-mountpoint"])
                .arg(&mount)
                .arg(&image)
                .status()
                .unwrap()
                .success()
        );
        Self { _temp: temp, mount }
    }
}

impl Drop for Volume {
    fn drop(&mut self) {
        support::wait_for_helpers();
        assert!(
            Command::new("hdiutil")
                .args(["detach", "-quiet"])
                .arg(&self.mount)
                .status()
                .unwrap()
                .success()
        );
    }
}

#[test]
fn one_helper_handles_home_and_mounted_volume_with_binary_names_and_symlinks() {
    use std::os::unix::ffi::OsStringExt;
    let files = Files::new();
    let volume = Volume::new();
    let home_file = files.file("home.txt");
    let directory = volume.mount.join(format!("{}directory", files.prefix));
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("child"), &files.prefix).unwrap();
    let link = volume.mount.join(format!("{}link", files.prefix));
    std::os::unix::fs::symlink("missing-target", &link).unwrap();
    let mut binary_name = files.prefix.as_bytes().to_vec();
    binary_name.extend_from_slice(b"binary-\x80");
    let binary = volume.mount.join(std::ffi::OsString::from_vec(binary_name));
    fs::write(&binary, &files.prefix).unwrap();
    let output = trache()
        .current_dir(&volume.mount)
        .arg("-r")
        .arg(&home_file)
        .arg(directory.file_name().unwrap())
        .arg(link.file_name().unwrap())
        .arg(binary.file_name().unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for source in [&home_file, &directory, &link, &binary] {
        assert!(fs::symlink_metadata(source).is_err());
    }
    assert_eq!(support::helper_pids().len(), 1);
    support::wait_for_helpers();
    let external_trash = volume
        .mount
        .join(".Trashes")
        .join(unsafe { libc::getuid() }.to_string());
    let external: Vec<_> = fs::read_dir(&external_trash)
        .unwrap()
        .filter_map(|e| {
            let e = e.unwrap();
            e.file_name()
                .to_string_lossy()
                .starts_with(&files.prefix)
                .then(|| e.path())
        })
        .collect();
    assert_eq!(external.len(), 3);
    let records =
        support::ds_store::parse(&fs::read(external_trash.join(".DS_Store")).unwrap()).unwrap();
    let locations: Vec<_> = records
        .into_iter()
        .filter(|(name, _)| name.starts_with(&files.prefix))
        .map(|(_, record)| record.location)
        .collect();
    eprintln!("Mounted-volume Put Back locations (best effort): {locations:?}");
}
