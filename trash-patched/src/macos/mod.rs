use std::{
    ffi::{CString, OsString},
    os::raw::c_char,
    path::{Path, PathBuf},
    process::Command,
    ptr::NonNull,
};

use log::trace;
use objc2::rc::Retained;
use objc2_foundation::{
    NSFileManager, NSSearchPathDirectory, NSSearchPathDomainMask, NSString, NSURL,
};

use crate::{into_unknown, Error, TrashContext};

#[derive(Copy, Clone, Debug)]
/// There are 2 ways to trash files: via the ≝Finder app or via the OS NsFileManager call
///
///   | <br>Feature            |≝<br>Finder     |<br>NsFileManager |
///   |:-----------------------|:--------------:|:----------------:|
///   |Undo via "Put back"     | ✓              | ✗                |
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
    /// - Does *not* show the "Put Back" option on some systems (the file may be restored by for
    ///   example dragging out from the Trash folder). This is a macOS bug. Read more about it
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
    trace!("Starting delete_using_file_mgr");
    let file_mgr = NSFileManager::defaultManager();
    for path in full_paths {
        let original_path = path.as_ref();

        trace!("Starting delete_url_for_path");
        let url = delete_url_for_path(original_path);
        trace!("Finished delete_url_for_path");

        let trash_check_url = file_url_for_path(trash_availability_check_path(original_path))?;
        ensure_volume_trash_available(&file_mgr, original_path, &trash_check_url)?;

        trace!("Calling trashItemAtURL");
        let res = file_mgr.trashItemAtURL_resultingItemURL_error(&url, None);
        trace!("Finished trashItemAtURL");

        if let Err(err) = res {
            return Err(Error::Unknown {
                description: format!(
                    "While deleting '{}', `trashItemAtURL` failed: {err}",
                    original_path.display()
                ),
            });
        }
    }
    Ok(())
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
                })
            }

            Some(code) => {
                return Err(Error::Os {
                    code,
                    description: format!("The AppleScript exited with error. stderr: {}", stderr),
                })
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
