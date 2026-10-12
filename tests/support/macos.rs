use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

#[path = "../../src/macos_trash/ds_store.rs"]
pub mod ds_store;

pub fn helper_pids() -> Vec<u32> {
    let binary = Path::new(env!("CARGO_BIN_EXE_trache"))
        .canonicalize()
        .unwrap();
    let pattern = format!(
        "^{} --__trache-macos-trash-worker$",
        regex::escape(binary.to_str().unwrap())
    );
    let output = Command::new("/usr/bin/pgrep")
        .args(["-f", &pattern])
        .output()
        .unwrap();
    assert!(matches!(output.status.code(), Some(0 | 1)));
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| s.parse().unwrap())
        .collect()
}

pub fn wait_for_helpers() {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !helper_pids().is_empty() {
        assert!(
            Instant::now() < deadline,
            "trash helpers exceeded their lifetime limit"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Existing CLI fixtures can have ordinary basenames. Only remove items whose
/// active metadata identifies this test's uniquely owned original directory.
#[allow(dead_code)] // Shared by integration binaries; only CLI fixtures need this cleanup.
pub fn cleanup(root: &Path) {
    wait_for_helpers();
    let Ok(root) = root.canonicalize() else {
        return;
    };
    let trash = std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".Trash");
    let Ok(bytes) = fs::read(trash.join(".DS_Store")) else {
        return;
    };
    let Ok(records) = ds_store::parse(&bytes) else {
        return;
    };
    for (name, record) in records {
        let Some(location) = record.location else {
            continue;
        };
        let location = location.trim_start_matches('/');
        let location = location
            .strip_prefix("System/Volumes/Data/")
            .unwrap_or(location);
        let parent = Path::new("/").join(location);
        if !parent.starts_with(&root)
            || name == ".DS_Store"
            || Path::new(&name).file_name() != Some(std::ffi::OsStr::new(&name))
        {
            continue;
        }
        let item = trash.join(name);
        if let Ok(metadata) = fs::symlink_metadata(&item) {
            if metadata.is_dir() {
                fs::remove_dir_all(item).unwrap();
            } else {
                fs::remove_file(item).unwrap();
            }
        }
    }
}
