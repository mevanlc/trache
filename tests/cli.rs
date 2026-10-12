use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

#[cfg(target_os = "macos")]
#[path = "support/macos.rs"]
mod macos_support;

fn trache() -> Command {
    cargo_bin_cmd!("trache")
}

/// Owns a unique test directory and removes any of its items left in the real
/// Recycle Bin/Trash before tempfile removes the live directory.
struct TestDir {
    _temp: TempDir,
    path: PathBuf,
}

impl TestDir {
    fn new() -> std::io::Result<Self> {
        let temp = TempDir::new()?;
        let path = test_dir_path(temp.path())?;
        Ok(Self { _temp: temp, path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(target_os = "windows")]
fn test_dir_path(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = path.canonicalize()?;
    let canonical = canonical.to_string_lossy();
    let shell_path = if let Some(rest) = canonical.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        canonical
            .strip_prefix(r"\\?\")
            .unwrap_or(&canonical)
            .to_owned()
    };
    Ok(PathBuf::from(shell_path))
}

#[cfg(not(target_os = "windows"))]
fn test_dir_path(path: &Path) -> std::io::Result<PathBuf> {
    Ok(path.to_owned())
}

#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"))
))]
impl Drop for TestDir {
    fn drop(&mut self) {
        let root = regex::escape(&self.path().to_string_lossy());
        let _ = trache()
            .arg("--trash-purge")
            .arg(format!(r"full:path:regex:{root}(?:[\\/].*)?"))
            .output();
    }
}

#[cfg(target_os = "macos")]
impl Drop for TestDir {
    fn drop(&mut self) {
        macos_support::cleanup(self.path());
    }
}

fn exact_path_pattern(path: &Path) -> String {
    format!("full:path:string:{}", path.display())
}

#[test]
fn test_help() {
    trache()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Move files to trash"));
}

#[test]
fn test_trash_file() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    trache()
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty()); // Silent by default (like rm)

    assert!(!file.exists());
}

#[test]
fn test_nonexistent_file_fails() {
    trache()
        .arg("/nonexistent/path/to/file.txt")
        .assert()
        .failure();
}

// Phase 1: Directory handling tests

#[test]
fn test_dir_without_flag_fails() {
    let tmp = TestDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();

    trache()
        .arg(&dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("directory"));
}

#[test]
fn test_empty_dir_with_d_flag() {
    let tmp = TestDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();

    trache().arg("-d").arg(&dir).assert().success();

    assert!(!dir.exists());
}

#[test]
fn test_nonempty_dir_with_d_flag_fails() {
    let tmp = TestDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("file.txt"), "content").unwrap();

    trache()
        .arg("-d")
        .arg(&dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not empty"));
}

#[test]
fn test_nonempty_dir_with_r_flag() {
    let tmp = TestDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("file.txt"), "content").unwrap();

    trache().arg("-r").arg(&dir).assert().success();

    assert!(!dir.exists());
}

#[test]
fn test_nonempty_dir_with_capital_r_flag() {
    let tmp = TestDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("file.txt"), "content").unwrap();

    trache().arg("-R").arg(&dir).assert().success();

    assert!(!dir.exists());
}

#[test]
fn test_recursive_long_flag() {
    let tmp = TestDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("file.txt"), "content").unwrap();

    trache().arg("--recursive").arg(&dir).assert().success();

    assert!(!dir.exists());
}

#[test]
fn test_dir_long_flag() {
    let tmp = TestDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();

    trache().arg("--dir").arg(&dir).assert().success();

    assert!(!dir.exists());
}

// Phase 2: Prompting tests

#[test]
fn test_force_ignores_nonexistent() {
    trache()
        .arg("-f")
        .arg("/nonexistent/path/to/file.txt")
        .assert()
        .success();
}

#[test]
fn test_interactive_always_yes() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    trache()
        .arg("-i")
        .arg(&file)
        .write_stdin("y\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("remove regular file"));

    assert!(!file.exists());
}

#[test]
fn test_interactive_always_no() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    trache()
        .arg("-i")
        .arg(&file)
        .write_stdin("n\n")
        .assert()
        .success();

    assert!(file.exists()); // File should still exist
}

#[test]
fn test_interactive_long_form() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    trache()
        .arg("--interactive=always")
        .arg(&file)
        .write_stdin("y\n")
        .assert()
        .success();

    assert!(!file.exists());
}

#[test]
fn test_interactive_never() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // --interactive=never should not prompt
    trache()
        .arg("--interactive=never")
        .arg(&file)
        .assert()
        .success();

    assert!(!file.exists());
}

#[test]
fn test_prompt_once_with_many_files() {
    let tmp = TestDir::new().unwrap();
    let files: Vec<_> = (0..5)
        .map(|i| {
            let f = tmp.path().join(format!("file{}.txt", i));
            fs::write(&f, "content").unwrap();
            f
        })
        .collect();

    // -I should prompt once for >3 files
    let mut cmd = trache();
    cmd.arg("-I");
    for f in &files {
        cmd.arg(f);
    }
    cmd.write_stdin("y\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("remove 5 argument(s)?"));

    for f in &files {
        assert!(!f.exists());
    }
}

#[test]
fn test_prompt_once_declined() {
    let tmp = TestDir::new().unwrap();
    let files: Vec<_> = (0..5)
        .map(|i| {
            let f = tmp.path().join(format!("file{}.txt", i));
            fs::write(&f, "content").unwrap();
            f
        })
        .collect();

    let mut cmd = trache();
    cmd.arg("-I");
    for f in &files {
        cmd.arg(f);
    }
    cmd.write_stdin("n\n").assert().success();

    // All files should still exist
    for f in &files {
        assert!(f.exists());
    }
}

#[test]
fn test_force_overrides_interactive() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // -i -f: force wins (last flag)
    trache().arg("-i").arg("-f").arg(&file).assert().success();

    assert!(!file.exists());
}

#[test]
fn test_interactive_overrides_force() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // -f -i: interactive wins (last flag)
    trache()
        .arg("-f")
        .arg("-i")
        .arg(&file)
        .write_stdin("y\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("remove regular file"));

    assert!(!file.exists());
}

// Phase 3: Verbose and version tests

#[test]
fn test_version() {
    trache()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(concat!(
            "trache ",
            env!("CARGO_PKG_VERSION")
        )));
}

#[test]
fn test_verbose_flag() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    trache()
        .arg("-v")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("trashed"));

    assert!(!file.exists());
}

#[test]
fn test_verbose_long_flag() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    trache()
        .arg("--verbose")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("trashed"));

    assert!(!file.exists());
}

#[test]
fn test_silent_by_default() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    trache()
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    assert!(!file.exists());
}

// Phase 4: Root protection tests

#[test]
#[cfg(unix)]
fn test_preserve_root_blocks_root() {
    // Attempting to trash / should fail by default
    trache()
        .arg("-r")
        .arg("/")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "dangerous to operate recursively on '/'",
        ));
}

#[test]
#[cfg(unix)]
fn test_preserve_root_explicit() {
    // --preserve-root=yes should also block /
    trache()
        .arg("-r")
        .arg("--preserve-root=yes")
        .arg("/")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "dangerous to operate recursively on '/'",
        ));
}

#[test]
#[cfg(target_os = "windows")]
fn test_windows_preserve_root_blocks_drive_root_but_allows_safe_dry_run_override() {
    let current = std::env::current_dir().unwrap();
    let root = current.ancestors().last().unwrap();
    let dangerous = format!("dangerous to operate recursively on '{}'", root.display());

    trache()
        .arg("-r")
        .arg("--trash-dry-run")
        .arg(root)
        .assert()
        .failure()
        .stderr(predicate::str::contains(&dangerous));

    trache()
        .arg("-r")
        .arg("--preserve-root=yes")
        .arg("--trash-dry-run")
        .arg(root)
        .assert()
        .failure()
        .stderr(predicate::str::contains(dangerous));

    trache()
        .arg("-r")
        .arg("--no-preserve-root")
        .arg("--trash-dry-run")
        .arg(root)
        .assert()
        .success()
        .stdout(predicate::str::contains("would trash"));
}

#[test]
fn test_no_preserve_root_flag_accepted() {
    // --no-preserve-root should be accepted (but we test with a safe file)
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    trache()
        .arg("--no-preserve-root")
        .arg(&file)
        .assert()
        .success();

    assert!(!file.exists());
}

#[test]
fn test_preserve_root_all_flag_accepted() {
    // --preserve-root=all should be accepted
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    trache()
        .arg("--preserve-root=all")
        .arg(&file)
        .assert()
        .success();

    assert!(!file.exists());
}

// Phase 5: Filesystem boundaries tests

#[test]
#[cfg(unix)]
fn test_one_file_system_short_flag() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // -x should be accepted and work on regular files
    trache().arg("-x").arg(&file).assert().success();

    assert!(!file.exists());
}

#[test]
#[cfg(unix)]
fn test_one_file_system_long_flag() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // --one-file-system should be accepted
    trache()
        .arg("--one-file-system")
        .arg(&file)
        .assert()
        .success();

    assert!(!file.exists());
}

#[test]
#[cfg(unix)]
fn test_one_file_system_with_recursive() {
    let tmp = TestDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("file.txt"), "content").unwrap();

    // -rx should work on directories
    trache().arg("-r").arg("-x").arg(&dir).assert().success();

    assert!(!dir.exists());
}

#[test]
#[cfg(target_os = "windows")]
fn test_windows_one_file_system_is_rejected_without_touching_the_file() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("one-file-system.txt");
    fs::write(&file, "keep me").unwrap();

    trache()
        .arg("--one-file-system")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--one-file-system is not supported on this platform",
        ));

    assert!(file.exists());
}

// Phase 6: Pattern type and compat flags

#[test]
fn test_compat_p_flag_ignored() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // -P should be silently ignored (BSD compat)
    trache().arg("-P").arg(&file).assert().success();

    assert!(!file.exists());
}

#[test]
fn test_compat_p_flag_combines_with_other_flags() {
    let tmp = TestDir::new().unwrap();
    let dir = tmp.path().join("mydir");
    fs::create_dir(&dir).unwrap();
    let file = dir.join("inner.txt");
    fs::write(&file, "hello").unwrap();

    // -P combined with -r should still work (P is a no-op)
    trache().arg("-rP").arg(&dir).assert().success();

    assert!(!dir.exists());
}

#[test]
fn test_compat_w_flag_errors() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // -W should error with helpful message
    trache()
        .arg("-W")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("use --trash-undo"));

    assert!(file.exists()); // File should still exist
}

// Phase 7: Edge cases

#[test]
fn test_reject_dot() {
    trache()
        .arg("-r")
        .arg(".")
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to remove '.' or '..'"));
}

#[test]
fn test_reject_dotdot() {
    trache()
        .arg("-r")
        .arg("..")
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to remove '.' or '..'"));
}

#[test]
fn test_double_dash_separator() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("-weird-name.txt");
    fs::write(&file, "hello").unwrap();

    // -- should allow files starting with -
    trache().arg("--").arg(&file).assert().success();

    assert!(!file.exists());
}

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_interactive_with_trash_undo_flag_accepted() {
    // -i combined with --trash-undo should parse without error
    // (will find no matching items, but the flag combination is valid)
    trache()
        .arg("-i")
        .arg("--trash-undo")
        .arg("nonexistent_pattern_xyz_12345")
        .assert()
        .success();
}

// Phase 8: Trash management system tests (require real freedesktop trash — Linux/Windows only)

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_trash_list_shows_trashed_item() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("systest_list.txt");
    fs::write(&file, "hello").unwrap();
    let original_path = file.to_string_lossy().into_owned();

    trache().arg(&file).assert().success();

    trache()
        .arg("--trash-list")
        .assert()
        .success()
        .stdout(predicate::str::contains(original_path));

    // cleanup
    trache()
        .arg("--trash-purge")
        .arg(exact_path_pattern(&file))
        .assert()
        .success();
}

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_trash_undo_restores_file() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("systest_undo.txt");
    fs::write(&file, "restore me").unwrap();

    trache().arg(&file).assert().success();
    assert!(!file.exists());

    trache()
        .arg("--trash-undo")
        .arg(exact_path_pattern(&file))
        .assert()
        .success()
        .stdout(predicate::str::contains("Restoring"));

    assert!(file.exists());
    assert_eq!(fs::read_to_string(&file).unwrap(), "restore me");
}

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_trash_undo_no_match() {
    trache()
        .arg("--trash-undo")
        .arg("full:nonexistent_xyz_99999.txt")
        .assert()
        .success()
        .stdout(predicate::str::contains("No items matching"));
}

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_trash_purge_removes_item() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("systest_purge.txt");
    fs::write(&file, "delete me").unwrap();
    let original_path = file.to_string_lossy().into_owned();

    trache().arg(&file).assert().success();

    trache()
        .arg("--trash-purge")
        .arg(exact_path_pattern(&file))
        .assert()
        .success()
        .stdout(predicate::str::contains("Purging"));

    // verify gone from list
    trache()
        .arg("--trash-list")
        .assert()
        .success()
        .stdout(predicate::str::contains(original_path).not());
}

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_trash_dry_run_undo() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("systest_dryrun.txt");
    fs::write(&file, "hello").unwrap();

    trache().arg(&file).assert().success();
    assert!(!file.exists());

    trache()
        .arg("--trash-dry-run")
        .arg("--trash-undo")
        .arg(exact_path_pattern(&file))
        .assert()
        .success()
        .stdout(predicate::str::contains("would restore"));

    assert!(!file.exists()); // not actually restored

    // cleanup
    trache()
        .arg("--trash-purge")
        .arg(exact_path_pattern(&file))
        .assert()
        .success();
}

#[test]
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios")))]
fn test_trash_dry_run_empty_shows_summary_and_verbose_items() {
    let tmp = TestDir::new().unwrap();
    let data_home = tmp.path().join("data");
    let file = tmp.path().join("systest_dryrun_empty.txt");
    fs::write(&file, "keep me").unwrap();
    let original_path = file.to_string_lossy().into_owned();
    let trash_folder = data_home.join("Trash").to_string_lossy().into_owned();
    let trash_payload = data_home.join("Trash/files/systest_dryrun_empty.txt");

    trache()
        .env("XDG_DATA_HOME", &data_home)
        .arg(&file)
        .assert()
        .success();
    assert!(!file.exists());
    assert!(trash_payload.exists());

    trache()
        .env("XDG_DATA_HOME", &data_home)
        .arg("--trash-empty")
        .arg("--trash-dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Trash items:").not())
        .stdout(predicate::str::contains("Trash summary:"))
        .stdout(predicate::str::contains(trash_folder.clone()))
        .stdout(predicate::str::contains("1 item(s), 7 B"))
        .stdout(predicate::str::contains(original_path.clone()).not())
        .stdout(predicate::str::contains(
            "Would permanently delete: 1 item(s), 7 B",
        ));
    assert!(trash_payload.exists());

    trache()
        .env("XDG_DATA_HOME", &data_home)
        .arg("--trash-empty")
        .arg("--trash-dry-run")
        .arg("--verbose")
        .assert()
        .success()
        .stdout(predicate::str::contains("Trash items:"))
        .stdout(predicate::str::contains("Trash summary:"))
        .stdout(predicate::str::contains(trash_folder))
        .stdout(predicate::str::contains("7 B"))
        .stdout(predicate::str::contains(original_path))
        .stdout(
            predicate::str::is_match(
                r"(?s)^Trash items:.*\n\nTrash summary:.*\nWould permanently delete:",
            )
            .unwrap(),
        );
    assert!(trash_payload.exists());

    trache()
        .env("XDG_DATA_HOME", &data_home)
        .arg("--trash-list")
        .assert()
        .success()
        .stdout(predicate::str::contains("systest_dryrun_empty.txt"));
}

#[test]
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios")))]
fn test_trash_stats_shows_rollups_and_verbose_items() {
    let tmp = TestDir::new().unwrap();
    let data_home = tmp.path().join("data");
    let file = tmp.path().join("stats-file.bin");
    let dir = tmp.path().join("stats-dir");
    fs::write(&file, vec![0; 4096]).unwrap();
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("nested.bin"), vec![0; 8192]).unwrap();
    let original_path = file.to_string_lossy().into_owned();
    let original_dir = dir.to_string_lossy().into_owned();
    let trash_folder = data_home.join("Trash").to_string_lossy().into_owned();

    trache()
        .env("XDG_DATA_HOME", &data_home)
        .arg(&file)
        .assert()
        .success();
    trache()
        .env("XDG_DATA_HOME", &data_home)
        .arg("-r")
        .arg(&dir)
        .assert()
        .success();
    assert!(!file.exists());
    assert!(!dir.exists());

    trache()
        .env("XDG_DATA_HOME", &data_home)
        .arg("--trash-stats")
        .assert()
        .success()
        .stdout(predicate::str::contains("Trash items:").not())
        .stdout(predicate::str::contains("Trash summary:"))
        .stdout(predicate::str::contains(trash_folder.clone()))
        .stdout(predicate::str::contains("2 item(s), 12.0 KiB"))
        .stdout(predicate::str::contains("Total: 2 item(s), 12.0 KiB"))
        .stdout(predicate::str::contains(original_path.clone()).not())
        .stdout(predicate::str::contains(original_dir.clone()).not());

    trache()
        .env("XDG_DATA_HOME", &data_home)
        .arg("--trash-stats")
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains("Trash items:"))
        .stdout(predicate::str::contains("Trash summary:"))
        .stdout(predicate::str::contains(trash_folder))
        .stdout(predicate::str::contains("4.0 KiB"))
        .stdout(predicate::str::contains("8.0 KiB"))
        .stdout(predicate::str::contains(original_path))
        .stdout(predicate::str::contains(original_dir))
        .stdout(
            predicate::str::is_match(r"(?s)^Trash items:.*\n\nTrash summary:.*\nTotal:").unwrap(),
        );

    assert!(!file.exists());
    assert!(!dir.exists());
}

#[test]
#[cfg(target_os = "windows")]
fn test_windows_recycle_bin_lifecycle_handles_unicode_file_and_directory() {
    let tmp = TestDir::new().unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("trache stats ☃-{}-{nonce}.bin", std::process::id());
    let dir_name = format!("trache dir 目录-{}-{nonce}", std::process::id());
    let file = tmp.path().join(&name);
    let dir = tmp.path().join(&dir_name);
    fs::write(&file, vec![0; 4096]).unwrap();
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("one.txt"), "one").unwrap();
    fs::write(dir.join("two.txt"), "two").unwrap();
    let original_path = file.to_string_lossy().into_owned();
    let original_dir = dir.to_string_lossy().into_owned();
    let exact_item_pattern = format!(r"4\.0 KiB  {}", regex::escape(&original_path));

    trache().arg(&file).assert().success();
    trache().arg("-r").arg(&dir).assert().success();
    assert!(!file.exists());
    assert!(!dir.exists());

    trache()
        .arg("--trash-stats")
        .assert()
        .success()
        .stdout(predicate::str::contains("Trash summary:"))
        .stdout(predicate::str::contains("Total:"));

    trache()
        .arg("--trash-stats")
        .arg("--verbose")
        .assert()
        .success()
        .stdout(predicate::str::contains("Trash items:"))
        .stdout(predicate::str::is_match(exact_item_pattern).unwrap())
        .stdout(predicate::str::contains(format!(
            "{} (directory, 2 immediate entries)",
            original_dir
        )))
        .stdout(
            predicate::str::is_match(r"(?s)^Trash items:.*\n\nTrash summary:.*\nTotal:").unwrap(),
        );

    trache()
        .arg("--trash-empty")
        .arg("--trash-dry-run")
        .arg("--verbose")
        .assert()
        .success()
        .stdout(predicate::str::contains(&original_path))
        .stdout(predicate::str::contains(&original_dir))
        .stdout(
            predicate::str::is_match(
                r"(?s)^Trash items:.*\n\nTrash summary:.*\nWould permanently delete:",
            )
            .unwrap(),
        );

    trache()
        .arg("--trash-list")
        .assert()
        .success()
        .stdout(predicate::str::contains(&name))
        .stdout(predicate::str::contains(&dir_name));

    trache()
        .arg("--trash-undo")
        .arg(exact_path_pattern(&file))
        .assert()
        .success();
    assert_eq!(fs::read(&file).unwrap(), vec![0; 4096]);

    trache()
        .arg("--trash-purge")
        .arg(exact_path_pattern(&dir))
        .assert()
        .success();

    trache()
        .arg("--trash-list")
        .assert()
        .success()
        .stdout(predicate::str::contains(&original_path).not())
        .stdout(predicate::str::contains(&original_dir).not());
}

// Interactive undo: collision cases

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_trash_undo_collision_overwrite() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("systest_col_ow.txt");
    fs::write(&file, "original").unwrap();

    trache().arg(&file).assert().success();
    fs::write(&file, "blocker").unwrap();

    trache()
        .arg("-i")
        .arg("--trash-undo")
        .arg(exact_path_pattern(&file))
        .write_stdin("o\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Overwritten"));

    assert_eq!(fs::read_to_string(&file).unwrap(), "original");
}

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_trash_undo_collision_keep_both() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("systest_col_kb.txt");
    fs::write(&file, "original").unwrap();

    trache().arg(&file).assert().success();
    fs::write(&file, "blocker").unwrap();

    trache()
        .arg("-i")
        .arg("--trash-undo")
        .arg(exact_path_pattern(&file))
        .write_stdin("k\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Restored as"));

    assert_eq!(fs::read_to_string(&file).unwrap(), "blocker");
    let renamed = tmp.path().join("systest_col_kb-untrash_1.txt");
    assert!(renamed.exists());
    assert_eq!(fs::read_to_string(&renamed).unwrap(), "original");
}

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_trash_undo_collision_skip() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("systest_col_skip.txt");
    fs::write(&file, "original").unwrap();

    trache().arg(&file).assert().success();
    fs::write(&file, "blocker").unwrap();

    trache()
        .arg("-i")
        .arg("--trash-undo")
        .arg(exact_path_pattern(&file))
        .write_stdin("n\n")
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&file).unwrap(), "blocker");

    // cleanup — item still in trash
    trache()
        .arg("--trash-purge")
        .arg(exact_path_pattern(&file))
        .assert()
        .success();
}

// Interactive undo: twin cases

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_trash_undo_twins_all() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("systest_tw_all.txt");

    fs::write(&file, "v1").unwrap();
    trache().arg(&file).assert().success();
    fs::write(&file, "v2").unwrap();
    trache().arg(&file).assert().success();
    assert!(!file.exists());

    trache()
        .arg("-i")
        .arg("--trash-undo")
        .arg(exact_path_pattern(&file))
        .write_stdin("a\n")
        .assert()
        .success();

    let r1 = tmp.path().join("systest_tw_all-untrash_1.txt");
    let r2 = tmp.path().join("systest_tw_all-untrash_2.txt");
    assert!(r1.exists());
    assert!(r2.exists());
}

#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_trash_undo_twins_none() {
    let tmp = TestDir::new().unwrap();
    let file = tmp.path().join("systest_tw_none.txt");

    fs::write(&file, "v1").unwrap();
    trache().arg(&file).assert().success();
    fs::write(&file, "v2").unwrap();
    trache().arg(&file).assert().success();

    trache()
        .arg("-i")
        .arg("--trash-undo")
        .arg(exact_path_pattern(&file))
        .write_stdin("n\n")
        .assert()
        .success();

    assert!(!file.exists());

    // cleanup
    trache()
        .arg("--trash-purge")
        .arg(exact_path_pattern(&file))
        .assert()
        .success();
}

// macOS Finder/AppleScript has permission issues trashing symlinks in temp dirs
#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_symlink_removes_link_not_target() {
    let tmp = TestDir::new().unwrap();
    let target = tmp.path().join("target.txt");
    let link = tmp.path().join("link.txt");

    fs::write(&target, "hello").unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&target, &link).unwrap();

    trache().arg(&link).assert().success();

    assert!(!link.exists()); // Link should be gone
    assert!(target.exists()); // Target should still exist
}
