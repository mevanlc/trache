//! Read-only inventory. Put Back records are display hints, never item identities.

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
use std::ptr::NonNull;

use objc2_foundation::{NSHomeDirectory, NSString};

use super::ds_store::{self, PutBack};

#[derive(Debug)]
pub(crate) struct Inventory {
    pub groups: Vec<Group>,
    pub issues: Vec<Issue>,
    pub discovery_complete: bool,
}

#[derive(Debug)]
pub(crate) struct Group {
    pub path: PathBuf,
    pub items: Vec<Item>,
    pub complete: bool,
}

#[derive(Debug)]
pub(crate) struct Item {
    pub path: PathBuf,
    pub hint: Option<PutBack>,
    pub bytes: u64,
    pub size_complete: bool,
}

#[derive(Debug)]
pub(crate) struct Issue {
    pub path: PathBuf,
    pub error: io::Error,
}

#[derive(Clone, Copy)]
pub(crate) struct Options {
    pub measure: bool,
    pub hints: bool,
}

struct Candidate {
    base: PathBuf,
    components: Vec<OsString>,
    home: bool,
}

impl Candidate {
    fn path(&self) -> PathBuf {
        self.components
            .iter()
            .fold(self.base.clone(), |p, c| p.join(c))
    }

    fn open(&self) -> io::Result<Option<Root>> {
        // Home and mount paths may have legitimate aliases. Pin that trusted base,
        // then reject symlinks in every Trash-specific component.
        let base = std::fs::canonicalize(&self.base)?;
        let mut parent = Dir::open(&base)?;
        let mut ancestors = Vec::new();
        for (index, name) in self.components.iter().enumerate() {
            let expected = match stat_at(parent.fd(), name) {
                Ok(stat) => stat,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error),
            };
            let dir = parent.child(name, &expected)?;
            if index + 1 == self.components.len() {
                return Ok(Some(Root {
                    ancestors,
                    parent,
                    name: name.clone(),
                    dir,
                }));
            }
            ancestors.push((parent, name.clone()));
            parent = dir;
        }
        Err(io::Error::other("empty Trash path"))
    }
}

struct Root {
    ancestors: Vec<(Dir, OsString)>,
    parent: Dir,
    name: OsString,
    dir: Dir,
}

/// Owns the descriptor transferred to fdopendir; closedir closes it exactly once.
struct Dir {
    stream: NonNull<libc::DIR>,
    initial: libc::stat,
}

impl Dir {
    fn open(path: &Path) -> io::Result<Self> {
        let path = cstring(path.as_os_str())?;
        // SAFETY: path is NUL terminated, and the returned fd is uniquely owned.
        let fd = unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        Self::from_fd(fd)
    }

    fn from_fd(fd: RawFd) -> io::Result<Self> {
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the successful open returned a uniquely owned descriptor.
        let file = unsafe { File::from_raw_fd(fd) };
        let initial = stat_fd(fd)?;
        // SAFETY: fd is a live directory; ownership transfers only on success.
        let stream =
            NonNull::new(unsafe { libc::fdopendir(fd) }).ok_or_else(io::Error::last_os_error)?;
        std::mem::forget(file);
        Ok(Self { stream, initial })
    }

    fn fd(&self) -> RawFd {
        // SAFETY: the stream remains live until Drop.
        unsafe { libc::dirfd(self.stream.as_ptr()) }
    }

    fn child(&self, name: &OsStr, expected: &libc::stat) -> io::Result<Self> {
        let name = cstring(name)?;
        // SAFETY: name is a single entry from readdir, relative to a live fd.
        let fd = unsafe {
            libc::openat(
                self.fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        let child = Self::from_fd(fd)?;
        if identity(&child.initial) != identity(expected) {
            return Err(io::Error::other("directory replaced while opening"));
        }
        Ok(child)
    }

    fn next(&mut self) -> io::Result<Option<OsString>> {
        loop {
            // SAFETY: errno is thread-local, and this stream has one owner.
            let entry = unsafe {
                *libc::__error() = 0;
                libc::readdir(self.stream.as_ptr())
            };
            if entry.is_null() {
                let error = io::Error::last_os_error();
                return if error.raw_os_error() == Some(0) {
                    Ok(None)
                } else {
                    Err(error)
                };
            }
            // SAFETY: readdir's NUL-terminated name is copied before the next call.
            let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if bytes != b"." && bytes != b".." {
                return Ok(Some(OsString::from_vec(bytes.to_vec())));
            }
        }
    }

    fn check_unchanged(&self) -> io::Result<()> {
        let now = stat_fd(self.fd())?;
        if !unchanged(&self.initial, &now) {
            return Err(io::Error::other("directory changed during enumeration"));
        }
        Ok(())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        // SAFETY: this instance uniquely owns the stream and its descriptor.
        unsafe { libc::closedir(self.stream.as_ptr()) };
    }
}

fn cstring(value: &OsStr) -> io::Result<CString> {
    CString::new(value.as_bytes()).map_err(|_| io::Error::other("path contains NUL"))
}

fn stat_fd(fd: RawFd) -> io::Result<libc::stat> {
    let mut stat = std::mem::MaybeUninit::uninit();
    // SAFETY: fstat initializes stat on success.
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { stat.assume_init() })
}

fn stat_at(fd: RawFd, name: &OsStr) -> io::Result<libc::stat> {
    let name = cstring(name)?;
    let mut stat = std::mem::MaybeUninit::uninit();
    // SAFETY: name is NUL terminated; fstatat initializes stat on success.
    if unsafe {
        libc::fstatat(
            fd,
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { stat.assume_init() })
}

fn identity(stat: &libc::stat) -> (libc::dev_t, libc::ino_t, i64, i64) {
    (
        stat.st_dev,
        stat.st_ino,
        stat.st_birthtime,
        stat.st_birthtime_nsec,
    )
}

fn unchanged(before: &libc::stat, after: &libc::stat) -> bool {
    identity(before) == identity(after)
        && before.st_size == after.st_size
        && (before.st_mtime, before.st_mtime_nsec) == (after.st_mtime, after.st_mtime_nsec)
        && (before.st_ctime, before.st_ctime_nsec) == (after.st_ctime, after.st_ctime_nsec)
}

fn is_dir(stat: &libc::stat) -> bool {
    stat.st_mode & libc::S_IFMT == libc::S_IFDIR
}

fn mounted_paths() -> io::Result<Vec<PathBuf>> {
    for _ in 0..3 {
        // SAFETY: a null buffer requests only the count; no static storage is used.
        let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        let capacity = count as usize + 16;
        let bytes = capacity
            .checked_mul(std::mem::size_of::<libc::statfs>())
            .and_then(|bytes| i32::try_from(bytes).ok())
            .ok_or_else(|| io::Error::other("mount table too large"))?;
        let mut mounts = Vec::<std::mem::MaybeUninit<libc::statfs>>::with_capacity(capacity);
        // SAFETY: the allocation holds capacity statfs values; only initialized
        // entries reported by getfsstat are subsequently read.
        let actual =
            unsafe { libc::getfsstat(mounts.as_mut_ptr().cast(), bytes, libc::MNT_NOWAIT) };
        if actual < 0 {
            return Err(io::Error::last_os_error());
        }
        if actual as usize >= capacity {
            continue;
        }
        let mut paths = Vec::new();
        for index in 0..actual as usize {
            let mount = unsafe { &*mounts.as_ptr().add(index).cast::<libc::statfs>() };
            let path = unsafe { CStr::from_ptr(mount.f_mntonname.as_ptr()) };
            paths.push(PathBuf::from(OsString::from_vec(path.to_bytes().to_vec())));
        }
        return Ok(paths);
    }
    Err(io::Error::other("mount table kept changing"))
}

pub(crate) fn collect(options: Options) -> Inventory {
    let home = PathBuf::from(NSHomeDirectory().to_string());
    let mut candidates = vec![Candidate {
        base: home,
        components: vec![".Trash".into()],
        home: true,
    }];
    let mut discovery_issues = Vec::new();
    match mounted_paths() {
        Ok(paths) => {
            // SAFETY: getuid has no arguments or memory preconditions.
            let uid = unsafe { libc::getuid() }.to_string();
            candidates.extend(paths.into_iter().map(|base| Candidate {
                base,
                components: vec![".Trashes".into(), uid.clone().into()],
                home: false,
            }));
        }
        Err(error) => discovery_issues.push(Issue {
            path: PathBuf::from("/"),
            error,
        }),
    }
    let mut inventory = collect_candidates(candidates, options);
    inventory.discovery_complete = discovery_issues.is_empty();
    inventory.issues.extend(discovery_issues);
    inventory
}

fn collect_candidates(candidates: Vec<Candidate>, options: Options) -> Inventory {
    let mut inventory = Inventory {
        groups: Vec::new(),
        issues: Vec::new(),
        discovery_complete: true,
    };
    let mut seen = HashSet::new();
    for candidate in candidates {
        let path = candidate.path();
        let mut group = Group {
            path: path.clone(),
            items: Vec::new(),
            complete: true,
        };
        let root = match candidate.open() {
            Ok(Some(root)) => root,
            Ok(None) => {
                if candidate.home {
                    inventory.groups.push(group);
                }
                continue;
            }
            Err(error) => {
                group.complete = false;
                inventory.issues.push(Issue { path, error });
                inventory.groups.push(group);
                continue;
            }
        };
        if !seen.insert((root.dir.initial.st_dev, root.dir.initial.st_ino)) {
            continue;
        }
        scan_root(root, &mut group, options, &mut inventory.issues);
        inventory.groups.push(group);
    }
    inventory.groups.sort_by(|a, b| a.path.cmp(&b.path));
    inventory
}

fn scan_root(mut root: Root, group: &mut Group, options: Options, issues: &mut Vec<Issue>) {
    let hints = options.hints.then(|| read_hints(&root.dir)).flatten();
    loop {
        let name = match root.dir.next() {
            Ok(Some(name)) => name,
            Ok(None) => break,
            Err(error) => {
                group.complete = false;
                issues.push(Issue {
                    path: group.path.clone(),
                    error,
                });
                break;
            }
        };
        if name == ".DS_Store" || name == ".localized" {
            continue;
        }
        let path = group.path.join(&name);
        let mut item = Item {
            path: path.clone(),
            hint: None,
            bytes: 0,
            size_complete: true,
        };
        match stat_at(root.dir.fd(), &name) {
            Ok(stat) => {
                item.hint = name
                    .to_str()
                    .and_then(|name| hints.as_ref()?.get(&normalized(name))?.as_ref())
                    .map(|hint| PutBack {
                        name: hint.name.clone(),
                        location: hint.location.clone(),
                    });
                if options.measure {
                    let issue_count = issues.len();
                    item.bytes = measure(&root.dir, &name, &stat, &path, issues);
                    item.size_complete = issues.len() == issue_count;
                }
            }
            Err(error) => {
                item.size_complete = false;
                issues.push(Issue { path, error });
            }
        }
        group.items.push(item);
    }
    let check = root.dir.check_unchanged().and_then(|()| {
        for (index, (ancestor, name)) in root.ancestors.iter().enumerate() {
            let child = root
                .ancestors
                .get(index + 1)
                .map(|(dir, _)| dir)
                .unwrap_or(&root.parent);
            if identity(&stat_at(ancestor.fd(), name)?) != identity(&child.initial) {
                return Err(io::Error::other(
                    "Trash ancestor replaced during enumeration",
                ));
            }
        }
        let current = stat_at(root.parent.fd(), &root.name)?;
        if identity(&current) != identity(&root.dir.initial) {
            Err(io::Error::other(
                "Trash directory replaced during enumeration",
            ))
        } else {
            Ok(())
        }
    });
    if let Err(error) = check {
        group.complete = false;
        issues.push(Issue {
            path: group.path.clone(),
            error,
        });
    }
    group.items.sort_by(|a, b| a.path.cmp(&b.path));
}

fn measure(
    parent: &Dir,
    name: &OsStr,
    stat: &libc::stat,
    path: &Path,
    issues: &mut Vec<Issue>,
) -> u64 {
    if !is_dir(stat) {
        return stat.st_size.max(0) as u64;
    }
    let first = match parent.child(name, stat) {
        Ok(dir) => dir,
        Err(error) => {
            issues.push(Issue {
                path: path.to_owned(),
                error,
            });
            return 0;
        }
    };
    let device = parent.initial.st_dev;
    let mut visited = HashSet::new();
    let mut pending = vec![(first, path.to_owned(), name.to_owned())];
    let mut total = 0u64;
    // Iterative DFS: no recursive Rust stack, and each live fd has a Drop owner.
    visited.insert(identity(stat));
    if stat.st_dev != device {
        issues.push(Issue {
            path: path.to_owned(),
            error: io::Error::other("filesystem boundary"),
        });
        return 0;
    }
    while let Some((dir, path, _)) = pending.last_mut() {
        let name = match dir.next() {
            Ok(Some(name)) => name,
            result => {
                if let Err(error) = result {
                    issues.push(Issue {
                        path: path.clone(),
                        error,
                    });
                }
                let (dir, path, name) = pending.pop().expect("live traversal frame");
                let parent = pending.last().map(|frame| &frame.0).unwrap_or(parent);
                let check = dir.check_unchanged().and_then(|()| {
                    if identity(&stat_at(parent.fd(), &name)?) == identity(&dir.initial) {
                        Ok(())
                    } else {
                        Err(io::Error::other("directory replaced during enumeration"))
                    }
                });
                if let Err(error) = check {
                    issues.push(Issue { path, error });
                }
                continue;
            }
        };
        let child_path = path.join(&name);
        match stat_at(dir.fd(), &name) {
            Ok(stat) if is_dir(&stat) => {
                if stat.st_dev != device || !visited.insert(identity(&stat)) {
                    issues.push(Issue {
                        path: child_path,
                        error: io::Error::other("filesystem boundary or repeated directory"),
                    });
                    continue;
                }
                match dir.child(&name, &stat) {
                    Ok(child) => pending.push((child, child_path, name)),
                    Err(error) => issues.push(Issue {
                        path: child_path,
                        error,
                    }),
                }
            }
            Ok(stat) => match total.checked_add(stat.st_size.max(0) as u64) {
                Some(sum) => total = sum,
                None => {
                    issues.push(Issue {
                        path: child_path,
                        error: io::Error::other("trash size exceeds u64"),
                    });
                    return total;
                }
            },
            Err(error) => issues.push(Issue {
                path: child_path,
                error,
            }),
        }
    }
    total
}

type Hints = HashMap<String, Option<PutBack>>;

fn normalized(value: &str) -> String {
    NSString::from_str(value)
        .precomposedStringWithCanonicalMapping()
        .to_string()
}

fn read_hints(dir: &Dir) -> Option<Hints> {
    read_hints_after_read(dir, || {})
}

fn read_hints_after_read(dir: &Dir, after_read: impl FnOnce()) -> Option<Hints> {
    let name = OsStr::new(".DS_Store");
    let expected = stat_at(dir.fd(), name).ok()?;
    if expected.st_mode & libc::S_IFMT != libc::S_IFREG {
        return None;
    }
    let name_c = cstring(name).ok()?;
    // SAFETY: read-only open relative to the pinned Trash directory; nonblocking
    // prevents a replacement FIFO from hanging before its type is checked.
    let fd = unsafe {
        libc::openat(
            dir.fd(),
            name_c.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return None;
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let before = stat_fd(fd).ok()?;
    if !unchanged(&expected, &before)
        || before.st_mode & libc::S_IFMT != libc::S_IFREG
        || before.st_size < 0
        || before.st_size as u64 > ds_store::MAX_BYTES as u64
    {
        return None;
    }
    let mut bytes = Vec::new();
    (&file)
        .take(ds_store::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    after_read();
    if !unchanged(&before, &stat_fd(fd).ok()?)
        || !unchanged(&before, &stat_at(dir.fd(), name).ok()?)
    {
        return None;
    }
    Some(index_hints(ds_store::parse(&bytes).ok()?))
}

fn index_hints(records: std::collections::BTreeMap<String, PutBack>) -> Hints {
    let mut hints = Hints::new();
    for (name, hint) in records {
        let valid = hint
            .name
            .as_deref()
            .zip(hint.location.as_deref())
            .is_some_and(|(name, folder)| {
                !name.is_empty()
                    && !name.contains('\0')
                    && !folder.contains('\0')
                    && Path::new(name).components().count() == 1
                    && matches!(
                        Path::new(name).components().next(),
                        Some(Component::Normal(_))
                    )
                    && !Path::new(folder)
                        .components()
                        .any(|c| c == Component::ParentDir)
            });
        hints
            .entry(normalized(&name))
            .and_modify(|value| *value = None)
            .or_insert(if valid { Some(hint) } else { None });
    }
    hints
}

pub(crate) fn write_hint(writer: &mut impl Write, hint: Option<&PutBack>) -> io::Result<()> {
    if let Some(PutBack {
        name: Some(name),
        location: Some(location),
    }) = hint
    {
        writeln!(writer, "      recorded folder (unverified): {location:?}")?;
        writeln!(writer, "      recorded name (unverified): {name:?}")
    } else {
        writeln!(writer, "      original location: unknown")
    }
}

impl Inventory {
    pub(crate) fn write_list(&self, writer: &mut impl Write) -> io::Result<()> {
        let mut count = 0;
        for group in &self.groups {
            for item in &group.items {
                writeln!(writer, "{:?}", item.path)?;
                write_hint(writer, item.hint.as_ref())?;
                count += 1;
            }
        }
        if !self.discovery_complete
            || !self.issues.is_empty()
            || self.groups.iter().any(|g| !g.complete)
        {
            writeln!(writer, "Trash listing is incomplete.")?;
        } else if count == 0 {
            writeln!(writer, "Trash is empty.")?;
        }
        Ok(())
    }

    pub(crate) fn finish(&self, stderr: &mut impl Write) -> io::Result<()> {
        let mut permission_denied = false;
        for issue in &self.issues {
            writeln!(
                stderr,
                "trache: {:?}: {:?}",
                issue.path,
                issue.error.to_string()
            )?;
            permission_denied |=
                matches!(issue.error.raw_os_error(), Some(libc::EACCES | libc::EPERM));
        }
        if permission_denied {
            writeln!(
                stderr,
                "trache: Trash access may require Full Disk Access for your terminal or invoking application in System Settings > Privacy & Security."
            )?;
        }
        if self.issues.is_empty() {
            Ok(())
        } else {
            Err(io::Error::other(
                "Trash scan incomplete; accessible results are shown above",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};

    const OPTIONS: Options = Options {
        measure: true,
        hints: true,
    };
    const STORE: &[u8] = include_bytes!("../../tests/fixtures/put_back.ds_store");

    fn candidate(base: &Path, home: bool) -> Candidate {
        Candidate {
            base: base.to_owned(),
            components: vec![".Trash".into()],
            home,
        }
    }

    fn inventory(base: &Path, options: Options) -> Inventory {
        collect_candidates(vec![candidate(base, true)], options)
    }

    fn render_list(inventory: &Inventory) -> String {
        let mut output = Vec::new();
        inventory.write_list(&mut output).unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn measures_payloads_without_following_links_or_opening_special_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".Trash");
        fs::create_dir_all(root.join("directory/nested")).unwrap();
        fs::write(root.join("directory/a"), b"1234").unwrap();
        fs::hard_link(root.join("directory/a"), root.join("directory/hardlink")).unwrap();
        fs::write(root.join("directory/nested/.DS_Store"), b"123").unwrap();
        fs::write(root.join(".DS_Store"), b"corrupt metadata").unwrap();
        fs::write(root.join(".localized"), b"not a payload").unwrap();
        fs::write(root.join(".hidden"), b"ab").unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("large"), vec![0; 100_000]).unwrap();
        symlink(&outside, root.join("directory/link")).unwrap();
        symlink("missing", root.join("dangling")).unwrap();
        let fifo = cstring(root.join("fifo").as_os_str()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let _socket = std::os::unix::net::UnixListener::bind(root.join("socket")).unwrap();
        let before = fs::read(root.join(".DS_Store")).unwrap();
        let result = inventory(temp.path(), OPTIONS);
        assert!(result.issues.is_empty(), "{:?}", result.issues);
        let items = &result.groups[0].items;
        assert_eq!(items.len(), 5);
        let directory = items
            .iter()
            .find(|i| i.path.ends_with("directory"))
            .unwrap();
        assert_eq!(
            directory.bytes,
            11 + outside.as_os_str().as_bytes().len() as u64
        );
        assert!(directory.size_complete);
        assert_eq!(
            items
                .iter()
                .find(|i| i.path.ends_with("dangling"))
                .unwrap()
                .bytes,
            7
        );
        assert_eq!(fs::read(root.join(".DS_Store")).unwrap(), before);
        assert_eq!(fs::metadata(outside.join("large")).unwrap().len(), 100_000);
    }

    #[test]
    fn lists_actual_items_and_labels_stale_matching_records_as_unverified() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".Trash");
        fs::create_dir(&root).unwrap();
        fs::write(root.join(".DS_Store"), STORE).unwrap();
        // The fixture describes another file's origin. Its name match is not proof.
        fs::write(root.join("renamed.txt"), b"different file").unwrap();
        let unusual = OsString::from_vec(b"bad\n\x1bname".to_vec());
        fs::write(root.join(&unusual), b"x").unwrap();
        let result = inventory(temp.path(), OPTIONS);
        assert!(result.issues.is_empty());
        assert_eq!(result.groups[0].items.len(), 2); // Fixture's snow-name record is orphaned.
        let output = render_list(&result);
        assert!(output.contains("recorded folder (unverified): \"Users/example/work/\""));
        assert!(output.contains("recorded name (unverified): \"original.txt\""));
        assert!(output.contains("original location: unknown"));
        assert!(!output.contains('\u{1b}'));
        assert!(!output.contains("bad\n"));
        assert!(output.contains("bad\\n\\u{1b}name"));
        assert_eq!(fs::read(root.join(".DS_Store")).unwrap(), STORE);
        fs::write(root.join(".DS_Store"), b"invalid").unwrap();
        let invalid = inventory(temp.path(), OPTIONS);
        assert!(invalid.issues.is_empty());
        assert!(invalid.groups[0].items.iter().all(|i| i.hint.is_none()));
        fs::remove_file(root.join(".DS_Store")).unwrap();
        assert!(inventory(temp.path(), OPTIONS).issues.is_empty());
        assert!(!root.join(".DS_Store").exists());
    }

    #[test]
    fn rejects_changed_replaced_and_nonregular_metadata() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(".DS_Store"), STORE).unwrap();
        let dir = Dir::open(temp.path()).unwrap();
        assert!(read_hints(&dir).is_some());
        assert!(
            read_hints_after_read(&dir, || {
                fs::write(temp.path().join(".DS_Store"), b"changed").unwrap();
            })
            .is_none()
        );
        fs::write(temp.path().join(".DS_Store"), STORE).unwrap();
        assert!(
            read_hints_after_read(&dir, || {
                fs::rename(temp.path().join(".DS_Store"), temp.path().join("old")).unwrap();
                fs::write(temp.path().join(".DS_Store"), STORE).unwrap();
            })
            .is_none()
        );
        fs::remove_file(temp.path().join(".DS_Store")).unwrap();
        symlink(temp.path().join("old"), temp.path().join(".DS_Store")).unwrap();
        assert!(read_hints(&dir).is_none());
        fs::remove_file(temp.path().join(".DS_Store")).unwrap();
        let fifo = cstring(temp.path().join(".DS_Store").as_os_str()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(read_hints(&dir).is_none());
    }

    #[test]
    fn escapes_non_utf8_paths_even_on_filesystems_that_reject_them() {
        let path = PathBuf::from(OsString::from_vec(b"/Trash/bad\xff\n\x1bname".to_vec()));
        let result = Inventory {
            groups: vec![Group {
                path: PathBuf::from("/Trash"),
                items: vec![Item {
                    path,
                    hint: None,
                    bytes: 0,
                    size_complete: true,
                }],
                complete: true,
            }],
            issues: Vec::new(),
            discovery_complete: true,
        };
        let output = render_list(&result);
        assert!(output.contains("\\xFF") || output.contains("\\xff"));
        assert!(!output.contains('\u{1b}'));
        assert!(!output.contains("bad\n"));
    }

    #[test]
    fn suppresses_ambiguous_and_invalid_hints_without_case_folding() {
        let hint = || PutBack {
            name: Some("file".into()),
            location: Some("folder".into()),
        };
        let records = std::collections::BTreeMap::from([
            ("é".into(), hint()),
            ("e\u{301}".into(), hint()),
            ("Case".into(), hint()),
            ("case".into(), hint()),
            ("missing".into(), PutBack::default()),
            (
                "traversal".into(),
                PutBack {
                    name: Some("../file".into()),
                    location: Some("folder".into()),
                },
            ),
        ]);
        let hints = index_hints(records);
        assert!(hints["é"].is_none());
        assert!(hints["Case"].is_some() && hints["case"].is_some());
        assert!(hints["missing"].is_none() && hints["traversal"].is_none());
    }

    #[test]
    fn deduplicates_aliases_and_keeps_missing_roots_absent() {
        let temp = tempfile::tempdir().unwrap();
        let missing = inventory(temp.path(), OPTIONS);
        assert_eq!(render_list(&missing), "Trash is empty.\n");
        assert!(!temp.path().join(".Trash").exists());
        fs::create_dir(temp.path().join(".Trash")).unwrap();
        fs::write(temp.path().join(".Trash/file"), b"a").unwrap();
        let alias = temp.path().join("alias");
        symlink(temp.path(), &alias).unwrap();
        let result = collect_candidates(
            vec![candidate(temp.path(), true), candidate(&alias, false)],
            OPTIONS,
        );
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].items.len(), 1);
        let second = tempfile::tempdir().unwrap();
        fs::create_dir(second.path().join(".Trash")).unwrap();
        let result = collect_candidates(
            vec![
                candidate(second.path(), false),
                candidate(temp.path(), true),
            ],
            OPTIONS,
        );
        assert_eq!(result.groups.len(), 2);
    }

    #[test]
    fn external_roots_select_only_the_requested_user_and_detect_ancestor_replacement() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".Trashes/501")).unwrap();
        fs::create_dir_all(temp.path().join(".Trashes/502")).unwrap();
        fs::write(temp.path().join(".Trashes/501/own"), b"own").unwrap();
        fs::write(temp.path().join(".Trashes/502/other"), b"other").unwrap();
        let candidate = || Candidate {
            base: temp.path().to_owned(),
            components: vec![".Trashes".into(), "501".into()],
            home: false,
        };
        let result = collect_candidates(vec![candidate()], OPTIONS);
        assert!(result.issues.is_empty());
        assert_eq!(result.groups[0].items.len(), 1);
        assert!(result.groups[0].items[0].path.ends_with("own"));
        let root = candidate().open().unwrap().unwrap();
        fs::rename(temp.path().join(".Trashes"), temp.path().join("old")).unwrap();
        fs::create_dir_all(temp.path().join(".Trashes/501")).unwrap();
        let mut group = Group {
            path: candidate().path(),
            items: Vec::new(),
            complete: true,
        };
        let mut issues = Vec::new();
        scan_root(root, &mut group, OPTIONS, &mut issues);
        assert!(!group.complete);
        assert!(
            issues
                .iter()
                .any(|issue| issue.error.to_string().contains("ancestor replaced"))
        );
        fs::remove_dir_all(temp.path().join(".Trashes")).unwrap();
        symlink(temp.path().join("old"), temp.path().join(".Trashes")).unwrap();
        assert!(candidate().open().is_err());
    }

    #[test]
    fn rejects_root_symlinks_and_directory_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), vec![0; 100]).unwrap();
        symlink(outside.path(), temp.path().join(".Trash")).unwrap();
        let result = inventory(temp.path(), OPTIONS);
        assert!(!result.issues.is_empty());
        assert!(result.groups[0].items.is_empty());
        assert!(render_list(&result).contains("incomplete"));
        assert!(!render_list(&result).contains("Trash is empty"));
        fs::remove_file(temp.path().join(".Trash")).unwrap();
        fs::create_dir(temp.path().join(".Trash")).unwrap();
        let parent = Dir::open(temp.path()).unwrap();
        let expected = stat_at(parent.fd(), OsStr::new(".Trash")).unwrap();
        fs::rename(temp.path().join(".Trash"), temp.path().join("moved")).unwrap();
        symlink(outside.path(), temp.path().join(".Trash")).unwrap();
        let mut issues = Vec::new();
        assert_eq!(
            measure(
                &parent,
                OsStr::new(".Trash"),
                &expected,
                &temp.path().join(".Trash"),
                &mut issues
            ),
            0
        );
        assert!(!issues.is_empty());
        fs::remove_file(temp.path().join(".Trash")).unwrap();
        // A different real directory also fails the expected identity check.
        fs::create_dir(temp.path().join(".Trash")).unwrap();
        assert!(parent.child(OsStr::new(".Trash"), &expected).is_err());
        let root = candidate(temp.path(), true).open().unwrap().unwrap();
        fs::rename(temp.path().join(".Trash"), temp.path().join("replaced")).unwrap();
        fs::create_dir(temp.path().join(".Trash")).unwrap();
        let mut group = Group {
            path: temp.path().join(".Trash"),
            items: Vec::new(),
            complete: true,
        };
        scan_root(root, &mut group, OPTIONS, &mut issues);
        assert!(!group.complete);
    }

    #[test]
    fn disappearing_directories_and_filesystem_boundaries_are_partial() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("dir")).unwrap();
        let mut parent = Dir::open(temp.path()).unwrap();
        let stat = stat_at(parent.fd(), OsStr::new("dir")).unwrap();
        let mut issues = Vec::new();
        parent.initial.st_dev = parent.initial.st_dev.wrapping_add(1);
        assert_eq!(
            measure(&parent, OsStr::new("dir"), &stat, temp.path(), &mut issues),
            0
        );
        assert_eq!(issues.len(), 1);
        fs::remove_dir(temp.path().join("dir")).unwrap();
        measure(&parent, OsStr::new("dir"), &stat, temp.path(), &mut issues);
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn denied_reads_preserve_measured_bytes_and_fail_after_output() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".Trash");
        let denied = root.join("directory/denied");
        fs::create_dir_all(&denied).unwrap();
        fs::write(root.join("directory/readable"), b"1234").unwrap();
        fs::set_permissions(&denied, fs::Permissions::from_mode(0o0)).unwrap();
        let result = inventory(temp.path(), OPTIONS);
        let list = inventory(
            temp.path(),
            Options {
                measure: false,
                hints: true,
            },
        );
        assert!(list.issues.is_empty()); // Listing does not descend.
        fs::set_permissions(&denied, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(result.groups[0].items[0].bytes, 4);
        assert!(!result.groups[0].items[0].size_complete);
        let mut errors = Vec::new();
        assert!(result.finish(&mut errors).is_err());
        assert!(
            String::from_utf8(errors)
                .unwrap()
                .contains("Full Disk Access")
        );
        fs::set_permissions(&root, fs::Permissions::from_mode(0o0)).unwrap();
        let result = inventory(temp.path(), OPTIONS);
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(!result.groups[0].complete);
        assert!(!render_list(&result).contains("Trash is empty"));
    }
}
