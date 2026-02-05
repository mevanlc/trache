use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn trache() -> Command {
    cargo_bin_cmd!("trache")
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();

    trache()
        .arg("-d")
        .arg(&dir)
        .assert()
        .success();

    assert!(!dir.exists());
}

#[test]
fn test_nonempty_dir_with_d_flag_fails() {
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("file.txt"), "content").unwrap();

    trache()
        .arg("-r")
        .arg(&dir)
        .assert()
        .success();

    assert!(!dir.exists());
}

#[test]
fn test_nonempty_dir_with_capital_r_flag() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("file.txt"), "content").unwrap();

    trache()
        .arg("-R")
        .arg(&dir)
        .assert()
        .success();

    assert!(!dir.exists());
}

#[test]
fn test_recursive_long_flag() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("file.txt"), "content").unwrap();

    trache()
        .arg("--recursive")
        .arg(&dir)
        .assert()
        .success();

    assert!(!dir.exists());
}

#[test]
fn test_dir_long_flag() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();

    trache()
        .arg("--dir")
        .arg(&dir)
        .assert()
        .success();

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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // -i -f: force wins (last flag)
    trache()
        .arg("-i")
        .arg("-f")
        .arg(&file)
        .assert()
        .success();

    assert!(!file.exists());
}

#[test]
fn test_interactive_overrides_force() {
    let tmp = TempDir::new().unwrap();
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
        .stdout(predicate::str::contains("trache 0.1.0"));
}

#[test]
fn test_verbose_flag() {
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
fn test_preserve_root_blocks_root() {
    // Attempting to trash / should fail by default
    trache()
        .arg("-r")
        .arg("/")
        .assert()
        .failure()
        .stderr(predicate::str::contains("dangerous to operate recursively on '/'"));
}

#[test]
fn test_preserve_root_explicit() {
    // --preserve-root=yes should also block /
    trache()
        .arg("-r")
        .arg("--preserve-root=yes")
        .arg("/")
        .assert()
        .failure()
        .stderr(predicate::str::contains("dangerous to operate recursively on '/'"));
}

#[test]
fn test_no_preserve_root_flag_accepted() {
    // --no-preserve-root should be accepted (but we test with a safe file)
    let tmp = TempDir::new().unwrap();
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
    let tmp = TempDir::new().unwrap();
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
fn test_one_file_system_short_flag() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // -x should be accepted and work on regular files
    trache()
        .arg("-x")
        .arg(&file)
        .assert()
        .success();

    assert!(!file.exists());
}

#[test]
fn test_one_file_system_long_flag() {
    let tmp = TempDir::new().unwrap();
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
fn test_one_file_system_with_recursive() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("subdir");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("file.txt"), "content").unwrap();

    // -rx should work on directories
    trache()
        .arg("-r")
        .arg("-x")
        .arg(&dir)
        .assert()
        .success();

    assert!(!dir.exists());
}

// Phase 6: Pattern type and compat flags

#[test]
fn test_match_type_flag_accepted() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // -T with a valid match type should be accepted
    trache()
        .arg("-T")
        .arg("glob")
        .arg(&file)
        .assert()
        .success();

    assert!(!file.exists());
}

#[test]
fn test_compat_p_flag_ignored() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // -P should be silently ignored (4.4BSD-Lite2 compat)
    trache()
        .arg("-P")
        .arg(&file)
        .assert()
        .success();

    assert!(!file.exists());
}

#[test]
fn test_compat_p_flag_combines_with_other_flags() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("mydir");
    fs::create_dir(&dir).unwrap();
    let file = dir.join("inner.txt");
    fs::write(&file, "hello").unwrap();

    // -P combined with -r should still work (P is a no-op)
    trache()
        .arg("-rP")
        .arg(&dir)
        .assert()
        .success();

    assert!(!dir.exists());
}

#[test]
fn test_compat_w_flag_errors() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("test.txt");
    fs::write(&file, "hello").unwrap();

    // -W should error with helpful message
    trache()
        .arg("-W")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("use -u/--undo"));

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
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("-weird-name.txt");
    fs::write(&file, "hello").unwrap();

    // -- should allow files starting with -
    trache()
        .arg("--")
        .arg(&file)
        .assert()
        .success();

    assert!(!file.exists());
}

// macOS Finder/AppleScript has permission issues trashing symlinks in temp dirs
#[test]
#[cfg_attr(target_os = "macos", ignore)]
fn test_symlink_removes_link_not_target() {
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("target.txt");
    let link = tmp.path().join("link.txt");

    fs::write(&target, "hello").unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&target, &link).unwrap();

    trache()
        .arg(&link)
        .assert()
        .success();

    assert!(!link.exists()); // Link should be gone
    assert!(target.exists()); // Target should still exist
}
