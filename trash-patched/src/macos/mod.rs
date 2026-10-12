use std::{
    ffi::{CStr, CString, OsString},
    fs::{self, Metadata},
    os::macos::fs::MetadataExt as _,
    os::raw::c_char,
    os::unix::{ffi::OsStringExt, fs::MetadataExt as _},
    path::{Path, PathBuf},
    process::Command,
    ptr::NonNull,
};

use log::trace;
use objc2::rc::Retained;
use objc2_foundation::{
    NSFileManager, NSSearchPathDirectory, NSSearchPathDomainMask, NSString, NSURL,
};

use crate::{Error, TrashContext, into_unknown};

#[derive(Copy, Clone, Debug)]
/// There are 2 ways to trash files: via the ≝Finder app or via the OS NsFileManager call
///
///   | <br>Feature            |≝<br>Finder     |<br>NsFileManager |
///   |:-----------------------|:--------------:|:----------------:|
///   |Undo via "Put back"     | ✓              | Best effort; delayed flush |
///   |Speed                   | ✗<br>Slower    | ✓<br>Faster      |
///   |No sound                | ✗              | ✓                |
///   |No extra permissions    | ✗              | ✓                |
///
pub enum DeleteMethod {
    /// Use an `osascript`, asking the Finder application to delete the files.
    ///
    /// - Might ask the user to give additional permissions to the app
    /// - Produces the sound that Finder usually makes when deleting a file
    /// - Shows the "Put Back" option in the context menu, when using the Finder application
    ///
    /// This is the default.
    Finder,

    /// Use `trashItemAtURL` from the `NSFileManager` object to delete the files.
    ///
    /// - Somewhat faster than the `Finder` method
    /// - Does *not* require additional permissions
    /// - Does *not* produce the sound that Finder usually makes when deleting a file
    /// - Put Back metadata can be flushed asynchronously. Short-lived callers must keep the
    ///   process alive after deletion; trache does this in a temporary helper. Metadata is
    ///   still best effort on affected macOS versions. Read more about it
    ///   at:
    ///   - <https://github.com/sindresorhus/macos-trash/issues/4>
    ///   - <https://github.com/ArturKovacs/trash-rs/issues/14>
    NsFileManager,
}
impl DeleteMethod {
    /// Returns `DeleteMethod::Finder`
    pub const fn new() -> Self {
        DeleteMethod::Finder
    }
}
impl Default for DeleteMethod {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone, Default, Debug)]
pub struct PlatformTrashContext {
    delete_method: DeleteMethod,
}
impl PlatformTrashContext {
    pub const fn new() -> Self {
        Self {
            delete_method: DeleteMethod::new(),
        }
    }
}
pub trait TrashContextExtMacos {
    fn set_delete_method(&mut self, method: DeleteMethod);
    fn delete_method(&self) -> DeleteMethod;
}
impl TrashContextExtMacos for TrashContext {
    fn set_delete_method(&mut self, method: DeleteMethod) {
        self.platform_specific.delete_method = method;
    }
    fn delete_method(&self) -> DeleteMethod {
        self.platform_specific.delete_method
    }
}
impl TrashContext {
    pub(crate) fn delete_all_canonicalized(&self, full_paths: Vec<PathBuf>) -> Result<(), Error> {
        match self.platform_specific.delete_method {
            DeleteMethod::Finder => delete_using_finder(&full_paths),
            DeleteMethod::NsFileManager => delete_using_file_mgr(&full_paths),
        }
    }
}

fn delete_using_file_mgr<P: AsRef<Path>>(full_paths: &[P]) -> Result<(), Error> {
    let file_mgr = NSFileManager::defaultManager();
    for path in full_paths {
        delete_one_with_file_manager(&file_mgr, path.as_ref())?;
    }
    Ok(())
}

/// Filesystem identity, including birth time to distinguish reused inode numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u64,
    pub birth_seconds: i64,
    pub birth_nanoseconds: i64,
}

impl FileIdentity {
    pub fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            birth_seconds: metadata.st_birthtime(),
            birth_nanoseconds: metadata.st_birthtime_nsec(),
        }
    }
}

/// A successful FileManager move and the information needed to observe Put Back metadata.
/// Missing optional information does not mean the move failed.
#[derive(Clone, Debug)]
pub struct TrashReceipt {
    pub original_path: PathBuf,
    pub original_parent_identity: Option<FileIdentity>,
    pub volume_root: Option<PathBuf>,
    pub trashed_path: Option<PathBuf>,
    pub trashed_identity: Option<FileIdentity>,
}

/// Trash one item with FileManager, returning its actual destination (including renames).
/// This does not wait for Apple's asynchronous Put Back metadata flush.
pub fn delete_with_file_manager(path: impl AsRef<Path>) -> Result<TrashReceipt, Error> {
    let paths = crate::canonicalize_paths([path])?;
    objc2::rc::autoreleasepool(|_| {
        delete_one_with_file_manager(&NSFileManager::defaultManager(), &paths[0])
    })
}

fn delete_one_with_file_manager(
    file_mgr: &NSFileManager,
    original_path: &Path,
) -> Result<TrashReceipt, Error> {
    let parent = original_path.parent().ok_or(Error::TargetedRoot)?;
    let original_parent_identity = fs::metadata(parent)
        .ok()
        .as_ref()
        .map(FileIdentity::from_metadata);
    let volume_root = volume_root(parent);
    let url = delete_url_for_path(original_path);
    let trash_check_url = file_url_for_path(trash_availability_check_path(original_path))?;
    ensure_volume_trash_available(file_mgr, original_path, &trash_check_url)?;
    let mut destination = None;
    trace!("Calling trashItemAtURL");
    file_mgr
        .trashItemAtURL_resultingItemURL_error(&url, Some(&mut destination))
        .map_err(|err| Error::Unknown {
            description: format!(
                "While deleting '{}', `trashItemAtURL` failed: {err}",
                original_path.display()
            ),
        })?;
    let trashed_path = destination.map(|url| {
        // The file URL owns this NUL-terminated representation; copy it before releasing it.
        let bytes = unsafe { CStr::from_ptr(url.fileSystemRepresentation().as_ptr()) }.to_bytes();
        PathBuf::from(OsString::from_vec(bytes.to_vec()))
    });
    let trashed_identity = trashed_path
        .as_ref()
        .and_then(|p| fs::symlink_metadata(p).ok())
        .as_ref()
        .map(FileIdentity::from_metadata);
    Ok(TrashReceipt {
        original_path: original_path.to_owned(),
        original_parent_identity,
        volume_root,
        trashed_path,
        trashed_identity,
    })
}

fn volume_root(path: &Path) -> Option<PathBuf> {
    let path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut info = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // statfs initializes the structure on success, including its NUL-terminated mount path.
    if unsafe { libc::statfs(path.as_ptr(), info.as_mut_ptr()) } != 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    let bytes = unsafe { CStr::from_ptr(info.f_mntonname.as_ptr()) }.to_bytes();
    Some(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

fn delete_url_for_path(path: &Path) -> Retained<NSURL> {
    let path_bytes = path.as_os_str().as_encoded_bytes();
    let path = match std::str::from_utf8(path_bytes) {
        Ok(path_utf8) => NSString::from_str(path_utf8), // utf-8 path, use as is
        Err(_) => NSString::from_str(&percent_encode(path_bytes)), // binary path, %-encode it
    };

    NSURL::fileURLWithPath(&path)
}

fn file_url_for_path(path: &Path) -> Result<Retained<NSURL>, Error> {
    let c_path = CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| Error::Unknown {
        description: format!("Path contains an interior nul byte: '{}'", path.display()),
    })?;
    let path_ptr =
        NonNull::new(c_path.as_ptr() as *mut c_char).expect("CString pointer is non-null");
    let is_dir = path.symlink_metadata().map(|m| m.is_dir()).unwrap_or(false);

    // Foundation copies the filesystem representation while constructing the URL.
    Ok(unsafe {
        NSURL::fileURLWithFileSystemRepresentation_isDirectory_relativeToURL(path_ptr, is_dir, None)
    })
}

fn trash_availability_check_path(path: &Path) -> &Path {
    let mut candidate = path;
    loop {
        if std::str::from_utf8(candidate.as_os_str().as_encoded_bytes()).is_ok() {
            return candidate;
        }

        candidate = match candidate.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("/"),
        };
    }
}

fn ensure_volume_trash_available(
    file_mgr: &NSFileManager,
    path: &Path,
    url: &NSURL,
) -> Result<(), Error> {
    file_mgr
        .URLForDirectory_inDomain_appropriateForURL_create_error(
            NSSearchPathDirectory::TrashDirectory,
            NSSearchPathDomainMask::UserDomainMask,
            Some(url),
            true,
        )
        .map(|_| ())
        .map_err(|err| Error::UnsupportedTrashVolume {
            path: path.to_path_buf(),
            reason: format!("{err}"),
        })
}

fn delete_using_finder<P: AsRef<Path>>(full_paths: &[P]) -> Result<(), Error> {
    // AppleScript command to move files (or directories) to Trash looks like
    //   osascript -e 'tell application "Finder" to delete { POSIX file "file1", POSIX "file2" }'
    // The `-e` flag is used to execute only one line of AppleScript.
    let mut command = Command::new("osascript");
    let posix_files = full_paths
        .iter()
        .map(|p| {
            let path_b = p.as_ref().as_os_str().as_encoded_bytes();
            match std::str::from_utf8(path_b) {
                Ok(path_utf8) => format!(r#"POSIX file "{}""#, esc_quote(path_utf8)), // utf-8 path, escape \"
                Err(_) => format!(r#"POSIX file "{}""#, esc_quote(&percent_encode(path_b))), // binary path, %-encode it and escape \"
            }
        })
        .collect::<Vec<String>>()
        .join(", ");
    let script = format!("tell application \"Finder\" to delete {{ {posix_files} }}");

    let argv: Vec<OsString> = vec!["-e".into(), script.into()];
    command.args(argv);

    // Execute command
    let result = command.output().map_err(into_unknown)?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        match result.status.code() {
            None => {
                return Err(Error::Unknown {
                    description: format!("The AppleScript exited with error. stderr: {}", stderr),
                });
            }

            Some(code) => {
                return Err(Error::Os {
                    code,
                    description: format!("The AppleScript exited with error. stderr: {}", stderr),
                });
            }
        };
    }
    Ok(())
}

/// std's from_utf8_lossy, but non-utf8 byte sequences are %-encoded instead of being replaced by a special symbol.
/// Valid utf8, including `%`, are not escaped.
use std::borrow::Cow;
fn percent_encode(input: &[u8]) -> Cow<'_, str> {
    use percent_encoding::percent_encode_byte as b2pc;

    let mut iter = input.utf8_chunks().peekable();
    if let Some(chunk) = iter.peek() {
        if chunk.invalid().is_empty() {
            return Cow::Borrowed(chunk.valid());
        }
    } else {
        return Cow::Borrowed("");
    };

    let mut res = String::with_capacity(input.len());
    for chunk in iter {
        res.push_str(chunk.valid());
        let invalid = chunk.invalid();
        if !invalid.is_empty() {
            for byte in invalid {
                res.push_str(b2pc(*byte));
            }
        }
    }
    Cow::Owned(res)
}

/// Escapes `"` or `\` with `\` for use in AppleScript text
fn esc_quote(s: &str) -> Cow<'_, str> {
    if s.contains(['"', '\\']) {
        let mut r = String::with_capacity(s.len());
        let chars = s.chars();
        for c in chars {
            match c {
                '"' | '\\' => {
                    r.push('\\');
                    r.push(c);
                } // escapes quote/escape char
                _ => {
                    r.push(c);
                } // no escape required
            }
        }
        Cow::Owned(r)
    } else {
        Cow::Borrowed(s)
    }
}

#[cfg(test)]
mod tests;
