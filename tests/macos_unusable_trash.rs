#![cfg(target_os = "macos")]

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use tempfile::TempDir;

fn trache() -> Command {
    cargo_bin_cmd!("trache")
}

#[test]
fn test_force_does_not_ignore_unusable_volume_trash() {
    let (_volume, tmp) = create_hfs_volume();
    let mount = tmp.path().join("mnt");
    fs::write(mount.join(".Trashes"), "").unwrap();

    let dir = mount.join("no-trash-dir");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("file.txt"), "content").unwrap();

    trache()
        .arg("-rf")
        .arg(&dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("usable Trash"));

    assert!(dir.exists());
}

struct MountedVolume {
    mount_point: PathBuf,
}

impl Drop for MountedVolume {
    fn drop(&mut self) {
        let _ = ProcessCommand::new("hdiutil")
            .arg("detach")
            .arg(&self.mount_point)
            .status();
    }
}

fn create_hfs_volume() -> (MountedVolume, TempDir) {
    let tmp = TempDir::new().unwrap();
    let dmg_file = tmp.path().join("fs.dmg");
    let mount_point = tmp.path().join("mnt");
    fs::create_dir(&mount_point).unwrap();

    let created = ProcessCommand::new("hdiutil")
        .args(["create", "-quiet", "-size", "32m", "-fs", "HFS+"])
        .arg(&dmg_file)
        .status()
        .unwrap();
    assert!(created.success());

    let attached = ProcessCommand::new("hdiutil")
        .args(["attach", "-quiet", "-nobrowse", "-mountpoint"])
        .arg(&mount_point)
        .arg(&dmg_file)
        .status()
        .unwrap();
    assert!(attached.success());

    (MountedVolume { mount_point }, tmp)
}
