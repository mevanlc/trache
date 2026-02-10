use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Component, Path, PathBuf};

use clap::{ArgGroup, Parser, ValueEnum};

#[derive(Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
enum InteractiveMode {
    /// Never prompt
    #[default]
    Never,
    /// Prompt once before removing more than three files, or when removing recursively
    Once,
    /// Prompt before every removal
    Always,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
enum PreserveRoot {
    /// Do not treat '/' specially
    No,
    /// Do not remove '/' (default)
    #[default]
    Yes,
    /// Also reject arguments on a separate device from their parent
    All,
}

#[derive(Clone, Copy, Default)]
enum PatternTarget {
    #[default]
    Name,
    Path,
}

#[allow(dead_code)]
enum CompiledMatcher {
    Glob(globset::GlobMatcher),
    Regex(regex::Regex, bool),
    String(String, bool),
}

#[allow(dead_code)]
impl CompiledMatcher {
    fn is_match(&self, haystack: &str) -> bool {
        match self {
            Self::Glob(g) => g.is_match(haystack),
            Self::Regex(r, full) => {
                if *full {
                    r.find(haystack)
                        .map(|m| m.start() == 0 && m.end() == haystack.len())
                        .unwrap_or(false)
                } else {
                    r.is_match(haystack)
                }
            }
            Self::String(s, full) => {
                if *full {
                    haystack == s.as_str()
                } else {
                    haystack.contains(s.as_str())
                }
            }
        }
    }
}

struct ParsedPattern<'a> {
    pattern: &'a str,
    match_type: &'a str,
    full: bool,
    target: PatternTarget,
}

fn parse_pattern(raw: &str) -> ParsedPattern<'_> {
    let mut match_type = "glob";
    let mut full = false;
    let mut target = PatternTarget::Name;
    let mut rest = raw;

    loop {
        if let Some(after) = rest.strip_prefix("glob:") {
            match_type = "glob";
            rest = after;
        } else if let Some(after) = rest.strip_prefix("regex:") {
            match_type = "regex";
            rest = after;
        } else if let Some(after) = rest.strip_prefix("string:") {
            match_type = "string";
            rest = after;
        } else if let Some(after) = rest.strip_prefix("full:") {
            full = true;
            rest = after;
        } else if let Some(after) = rest.strip_prefix("partial:") {
            full = false;
            rest = after;
        } else if let Some(after) = rest.strip_prefix("name:") {
            target = PatternTarget::Name;
            rest = after;
        } else if let Some(after) = rest.strip_prefix("path:") {
            target = PatternTarget::Path;
            rest = after;
        } else {
            break;
        }
    }

    ParsedPattern {
        pattern: rest,
        match_type,
        full,
        target,
    }
}

fn compile_matcher(pattern: &str, kind: &str, full: bool) -> Result<CompiledMatcher, String> {
    let matcher = match kind {
        "glob" => {
            let (glob_pattern, literal_sep) = if full {
                (pattern.to_string(), true)
            } else {
                (format!("*{pattern}*"), false)
            };
            let glob = globset::GlobBuilder::new(&glob_pattern)
                .literal_separator(literal_sep)
                .build()
                .map_err(|e| format!("invalid glob pattern: {e}"))?
                .compile_matcher();
            CompiledMatcher::Glob(glob)
        }
        "regex" => {
            let re = regex::Regex::new(pattern)
                .map_err(|e| format!("invalid regex: {e}"))?;
            CompiledMatcher::Regex(re, full)
        }
        "string" => {
            CompiledMatcher::String(pattern.to_string(), full)
        }
        _ => return Err(format!("unknown match type: '{kind}'")),
    };

    Ok(matcher)
}

/// Options for trash operations
struct TrashOptions {
    dir: bool,
    recursive: bool,
    force: bool,
    interactive: InteractiveMode,
    verbose: bool,
    dry_run: bool,
    preserve_root: PreserveRoot,
    one_file_system: bool,
}

#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
))]
use chrono::{DateTime, Local};
#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
))]
use trash::os_limited::{list, purge_all, restore_all};

#[derive(Parser)]
#[command(name = "trache")]
#[command(version)]
#[command(about = "Move files to trash. Manage trashed items.", long_about = None)]
#[command(group(
    ArgGroup::new("mode")
        .args(["list", "empty", "undo", "purge"])
))]
struct Cli {
    /// List items in trash
    #[arg(long = "trash-list")]
    list: bool,

    /// Empty the entire trash
    #[arg(long = "trash-empty")]
    empty: bool,

    /// Restore items matching pattern from trash (see --help)
    #[arg(
        long = "trash-undo",
        value_name = "PATTERN",
        long_help = "Restore items matching PATTERN from trash.\n\n\
            PATTERN may include optional prefixes to control matching:\n\
            \n\
            \x20 [glob:|regex:|string:|full:|partial:|name:|path:]*PATTERN\n\
            \n\
            Match type (default: glob):\n\
            \x20 glob:     glob pattern (see https://docs.rs/globset)\n\
            \x20 regex:    regular expression\n\
            \x20 string:   literal string\n\
            \n\
            Match extent (default: partial):\n\
            \x20 partial:  pattern matches a substring of the name/path\n\
            \x20 full:     pattern must match the entire name/path\n\
            \n\
            Match target (default: name):\n\
            \x20 name:  match against file basename\n\
            \x20 path:  match against original full path\n\
            \n\
            Prefixes can be stacked; rightmost wins per group.\n\
            \n\
            Examples:\n\
            \x20 --trash-undo foo            names containing \"foo\"\n\
            \x20 --trash-undo 'full:*.txt'   names matching *.txt exactly\n\
            \x20 --trash-undo 'regex:^foo'   names with regex match\n\
            \x20 --trash-undo 'string:a.txt' names containing \"a.txt\" literally\n\
            \x20 --trash-undo 'path:/tmp'    paths containing \"/tmp\""
    )]
    undo: Option<String>,

    /// Permanently delete items matching pattern from trash (see --help)
    #[arg(
        long = "trash-purge",
        value_name = "PATTERN",
        long_help = "Permanently delete items matching PATTERN from trash.\n\n\
            PATTERN may include optional prefixes to control matching:\n\
            \n\
            \x20 [glob:|regex:|string:|full:|partial:|name:|path:]*PATTERN\n\
            \n\
            Match type (default: glob):\n\
            \x20 glob:     glob pattern (see https://docs.rs/globset)\n\
            \x20 regex:    regular expression\n\
            \x20 string:   literal string\n\
            \n\
            Match extent (default: partial):\n\
            \x20 partial:  pattern matches a substring of the name/path\n\
            \x20 full:     pattern must match the entire name/path\n\
            \n\
            Match target (default: name):\n\
            \x20 name:  match against file basename\n\
            \x20 path:  match against original full path\n\
            \n\
            Prefixes can be stacked; rightmost wins per group.\n\
            \n\
            Examples:\n\
            \x20 --trash-purge foo            names containing \"foo\"\n\
            \x20 --trash-purge 'full:*.txt'   names matching *.txt exactly\n\
            \x20 --trash-purge 'regex:^foo'   names with regex match\n\
            \x20 --trash-purge 'string:a.txt' names containing \"a.txt\" literally\n\
            \x20 --trash-purge 'path:/tmp'    paths containing \"/tmp\""
    )]
    purge: Option<String>,

    /// Show what would be done without doing it
    #[arg(long = "trash-dry-run")]
    dry_run: bool,

    // --- rm-compatible flags ---
    /// Remove empty directories
    #[arg(short = 'd', long = "dir")]
    dir: bool,

    /// Remove directories and their contents recursively
    #[arg(short = 'r', visible_short_alias = 'R', long)]
    recursive: bool,

    /// Prompt before every removal
    #[arg(short = 'i', overrides_with_all = ["force", "prompt_once", "interactive"])]
    prompt_always: bool,

    /// Prompt once before removing more than three files, or when removing recursively
    #[arg(short = 'I', overrides_with_all = ["force", "prompt_always", "interactive"])]
    prompt_once: bool,

    /// Prompt according to WHEN: never, once, or always
    #[arg(long = "interactive", value_name = "WHEN", default_missing_value = "always", num_args = 0..=1, overrides_with_all = ["force", "prompt_always", "prompt_once"])]
    interactive: Option<InteractiveMode>,

    /// Ignore nonexistent files, never prompt
    #[arg(short = 'f', long, overrides_with_all = ["prompt_always", "prompt_once", "interactive"])]
    force: bool,

    /// Explain what is being done
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Do not remove '/'; 'all' also rejects arguments on separate devices
    #[arg(long = "preserve-root", value_name = "MODE", default_missing_value = "yes", num_args = 0..=1, overrides_with = "no_preserve_root")]
    preserve_root: Option<PreserveRoot>,

    /// Do not treat '/' specially
    #[arg(long = "no-preserve-root", overrides_with = "preserve_root")]
    no_preserve_root: bool,

    /// Skip directories on different file systems
    #[arg(short = 'x', long = "one-file-system")]
    one_file_system: bool,

    /// This flag has no effect.  It is kept only for backwards compatibility with BSD.
    #[arg(short = 'P', hide = true)]
    _compat_p: bool,

    /// Unsupported (use --trash-undo instead)
    #[arg(short = 'W', hide = true)]
    compat_w: bool,

    /// Files to trash
    #[arg(required_unless_present = "mode")]
    files: Vec<PathBuf>,
}

fn main() {
    // Reset SIGPIPE to default behavior (terminate silently) so piping to
    // tools like `head` or `grep` doesn't cause a panic
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let cli = Cli::parse();

    if cli.compat_w {
        eprintln!("trache: -W is not supported; use --trash-undo <pattern> to restore from trash");
        std::process::exit(1);
    }

    let dry_run = cli.dry_run;

    let result = if cli.list {
        list_trash()
    } else if cli.empty {
        if dry_run {
            println!("would empty trash");
            Ok(())
        } else {
            empty_trash()
        }
    } else if let Some(ref raw) = cli.undo {
        let parsed = parse_pattern(raw);
        let matcher = compile_matcher(parsed.pattern, parsed.match_type, parsed.full)
            .unwrap_or_else(|e| {
                eprintln!("trache: {e}");
                std::process::exit(1);
            });
        restore_items(parsed.pattern, &matcher, parsed.target, dry_run)
    } else if let Some(ref raw) = cli.purge {
        let parsed = parse_pattern(raw);
        let matcher = compile_matcher(parsed.pattern, parsed.match_type, parsed.full)
            .unwrap_or_else(|e| {
                eprintln!("trache: {e}");
                std::process::exit(1);
            });
        purge_items(parsed.pattern, &matcher, parsed.target, dry_run)
    } else {
        let interactive = if cli.force {
            InteractiveMode::Never
        } else if cli.prompt_always {
            InteractiveMode::Always
        } else if cli.prompt_once {
            InteractiveMode::Once
        } else if let Some(mode) = cli.interactive {
            mode
        } else {
            InteractiveMode::Never
        };

        let preserve_root = if cli.no_preserve_root {
            PreserveRoot::No
        } else if let Some(mode) = cli.preserve_root {
            mode
        } else {
            PreserveRoot::Yes // default
        };

        let opts = TrashOptions {
            dir: cli.dir,
            recursive: cli.recursive,
            force: cli.force,
            interactive,
            verbose: cli.verbose,
            dry_run: cli.dry_run,
            preserve_root,
            one_file_system: cli.one_file_system,
        };

        trash_files(&cli.files, &opts)
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn trash_files(files: &[PathBuf], opts: &TrashOptions) -> Result<(), Box<dyn std::error::Error>> {
    // Check -x/--one-file-system support on this platform
    #[cfg(not(unix))]
    if opts.one_file_system {
        return Err("--one-file-system is not supported on this platform".into());
    }

    let mut had_error = false;

    // -I: prompt once if >3 files or recursive
    let prompt_once_triggered =
        opts.interactive == InteractiveMode::Once && (files.len() > 3 || opts.recursive);

    if prompt_once_triggered {
        let msg = if opts.recursive {
            format!("trache: remove {} argument(s) recursively? ", files.len())
        } else {
            format!("trache: remove {} argument(s)? ", files.len())
        };
        if !prompt_yes(&msg) {
            return Ok(());
        }
    }

    for file in files {
        // Reject paths ending in . or ..
        match file.components().last() {
            Some(Component::CurDir) | Some(Component::ParentDir) => {
                eprintln!(
                    "trache: refusing to remove '.' or '..' directory: skipping '{}'",
                    file.display()
                );
                had_error = true;
                continue;
            }
            _ => {}
        }

        // Check preserve-root
        if let Err(e) = check_preserve_root(file, opts.preserve_root) {
            eprintln!("trache: {}", e);
            had_error = true;
            continue;
        }

        // Check one-file-system
        if opts.one_file_system && let Err(e) = check_one_file_system(file) {
            eprintln!("trache: {}", e);
            had_error = true;
            continue;
        }

        if let Err(e) = trash_single(file, opts, prompt_once_triggered)
            && (!opts.force || file.symlink_metadata().is_ok())
        {
            eprintln!("trache: cannot remove '{}': {}", file.display(), e);
            had_error = true;
        }
    }

    if had_error {
        Err("some files could not be removed".into())
    } else {
        Ok(())
    }
}

fn trash_single(
    file: &PathBuf,
    opts: &TrashOptions,
    already_prompted: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = match file.symlink_metadata() {
        Ok(m) => m,
        Err(e) => {
            if opts.force && e.kind() == io::ErrorKind::NotFound {
                return Ok(()); // -f ignores nonexistent files
            }
            return Err(e.into());
        }
    };

    // Prompt if -i (always) and we haven't already done a bulk prompt
    let should_prompt = opts.interactive == InteractiveMode::Always && !already_prompted;

    if metadata.is_dir() {
        if opts.recursive {
            if should_prompt {
                let prompt =
                    format!("trache: remove directory '{}' recursively? ", file.display());
                if !prompt_yes(&prompt) {
                    return Ok(());
                }
            }
            if opts.dry_run {
                println!("would trash '{}'", file.display());
            } else {
                trash::delete(file)?;
                if opts.verbose {
                    println!("trashed '{}'", file.display());
                }
            }
        } else if opts.dir {
            if is_dir_empty(file)? {
                if should_prompt {
                    let prompt = format!("trache: remove directory '{}'? ", file.display());
                    if !prompt_yes(&prompt) {
                        return Ok(());
                    }
                }
                if opts.dry_run {
                    println!("would trash '{}'", file.display());
                } else {
                    trash::delete(file)?;
                    if opts.verbose {
                        println!("trashed '{}'", file.display());
                    }
                }
            } else {
                return Err("Directory not empty".into());
            }
        } else {
            return Err("Is a directory".into());
        }
    } else {
        if should_prompt {
            let file_type = if metadata.is_symlink() {
                "symbolic link"
            } else {
                "regular file"
            };
            let prompt = format!("trache: remove {} '{}'? ", file_type, file.display());
            if !prompt_yes(&prompt) {
                return Ok(());
            }
        }
        if opts.dry_run {
            println!("would trash '{}'", file.display());
        } else {
            trash::delete(file)?;
            if opts.verbose {
                println!("trashed '{}'", file.display());
            }
        }
    }

    Ok(())
}

fn is_dir_empty(path: &PathBuf) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(fs::read_dir(path)?.next().is_none())
}

fn prompt_yes(prompt: &str) -> bool {
    eprint!("{}", prompt);
    io::stderr().flush().ok();

    let stdin = io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_err() {
        return false;
    }

    let response = line.trim().to_lowercase();
    matches!(response.as_str(), "y" | "yes")
}

fn check_preserve_root(path: &Path, mode: PreserveRoot) -> Result<(), String> {
    if mode == PreserveRoot::No {
        return Ok(());
    }

    // Normalize the path to check for root
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // Check if it's the root directory
    if canonical == Path::new("/") {
        return Err("it is dangerous to operate recursively on '/'\n\
             use --no-preserve-root to override this failsafe"
            .to_string());
    }

    // For --preserve-root=all, also check if path is on a different device than its parent
    if mode == PreserveRoot::All && let Err(e) = check_same_device_as_parent(&canonical) {
        return Err(format!(
            "'{}' is on a different device from its parent; refusing to operate\n{}",
            path.display(),
            e
        ));
    }

    Ok(())
}

#[cfg(unix)]
fn check_same_device_as_parent(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;

    let path_meta = path.symlink_metadata().map_err(|e| e.to_string())?;

    if let Some(parent) = path.parent() {
        if parent.as_os_str().is_empty() {
            return Ok(()); // No parent to compare
        }
        let parent_meta = parent.symlink_metadata().map_err(|e| e.to_string())?;

        if path_meta.dev() != parent_meta.dev() {
            return Err("use --no-preserve-root to override this failsafe".to_string());
        }
    }

    Ok(())
}

#[cfg(not(unix))]
fn check_same_device_as_parent(_path: &Path) -> Result<(), String> {
    // On non-Unix platforms, skip the device check
    Ok(())
}

#[cfg(unix)]
fn check_one_file_system(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;

    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let path_meta = canonical.symlink_metadata().map_err(|e| e.to_string())?;

    if let Some(parent) = canonical.parent() {
        if parent.as_os_str().is_empty() {
            return Ok(()); // No parent to compare
        }
        let parent_meta = parent.symlink_metadata().map_err(|e| e.to_string())?;

        if path_meta.dev() != parent_meta.dev() {
            return Err(format!(
                "skipping '{}', since it's on a different file system",
                path.display()
            ));
        }
    }

    Ok(())
}

#[cfg(not(unix))]
fn check_one_file_system(_path: &Path) -> Result<(), String> {
    // This shouldn't be called on non-Unix - we error earlier
    Ok(())
}

#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
))]
fn list_trash() -> Result<(), Box<dyn std::error::Error>> {
    let items = list()?;

    if items.is_empty() {
        println!("Trash is empty.");
        return Ok(());
    }

    for item in items {
        let time = format_timestamp(item.time_deleted);
        println!(
            "{} {} {}",
            time,
            item.name.to_string_lossy(),
            item.original_path().display()
        );
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
fn list_trash() -> Result<(), Box<dyn std::error::Error>> {
    Err("Listing trash is not supported on this platform".into())
}

#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
))]
fn format_timestamp(time_deleted: i64) -> String {
    DateTime::from_timestamp(time_deleted, 0)
        .map(|t| t.with_timezone(&Local))
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "????-??-?? ??:??".to_string())
}

#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
))]
/// Build a map of original_path -> count for duplicate detection.
fn path_counts(items: &[trash::TrashItem]) -> std::collections::HashMap<PathBuf, usize> {
    let mut counts = std::collections::HashMap::new();
    for item in items {
        *counts.entry(item.original_path()).or_insert(0) += 1;
    }
    counts
}

#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
))]
/// Print each item with disambiguation when multiple items share the same original path.
fn print_items(items: &[trash::TrashItem], prefix: &str) {
    let counts = path_counts(items);
    let mut seen: std::collections::HashMap<PathBuf, usize> = std::collections::HashMap::new();

    for item in items {
        let path = item.original_path();
        let total = counts[&path];
        if total > 1 {
            let idx = seen.entry(path.clone()).or_insert(0);
            *idx += 1;
            let ts = format_timestamp(item.time_deleted);
            println!("{prefix} ({}/{total}, {ts}): {}", *idx, path.display());
        } else {
            println!("{prefix}: {}", path.display());
        }
    }
}

#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
))]
fn restore_items(pattern: &str, matcher: &CompiledMatcher, target: PatternTarget, dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
    let items = list()?;
    let matching: Vec<_> = items
        .into_iter()
        .filter(|item| {
            let haystack = match target {
                PatternTarget::Name => item.name.to_string_lossy().into_owned(),
                PatternTarget::Path => item.original_path().to_string_lossy().into_owned(),
            };
            matcher.is_match(&haystack)
        })
        .collect();

    if matching.is_empty() {
        println!("No items matching '{pattern}' found in trash.");
        return Ok(());
    }

    let prefix = if dry_run { "would restore" } else { "Restoring" };
    print_items(&matching, prefix);

    if !dry_run {
        restore_all(matching)?;
        println!("Restored item(s).");
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
fn restore_items(_pattern: &str, _matcher: &CompiledMatcher, _target: PatternTarget, _dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
    Err("Restoring from trash is not supported on this platform".into())
}

#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
))]
fn purge_items(pattern: &str, matcher: &CompiledMatcher, target: PatternTarget, dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
    let items = list()?;
    let matching: Vec<_> = items
        .into_iter()
        .filter(|item| {
            let haystack = match target {
                PatternTarget::Name => item.name.to_string_lossy().into_owned(),
                PatternTarget::Path => item.original_path().to_string_lossy().into_owned(),
            };
            matcher.is_match(&haystack)
        })
        .collect();

    if matching.is_empty() {
        println!("No items matching '{pattern}' found in trash.");
        return Ok(());
    }

    let prefix = if dry_run { "would purge" } else { "Purging" };
    print_items(&matching, prefix);

    if !dry_run {
        purge_all(matching)?;
        println!("Permanently deleted item(s).");
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
fn purge_items(_pattern: &str, _matcher: &CompiledMatcher, _target: PatternTarget, _dry_run: bool) -> Result<(), Box<dyn std::error::Error>> {
    Err("Purging trash is not supported on this platform".into())
}

#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))
))]
fn empty_trash() -> Result<(), Box<dyn std::error::Error>> {
    let items = list()?;

    if items.is_empty() {
        println!("Trash is already empty.");
        return Ok(());
    }

    let count = items.len();
    purge_all(items)?;
    println!("Permanently deleted {count} item(s).");
    Ok(())
}

#[cfg(target_os = "macos")]
fn empty_trash() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"Finder\" to empty trash")
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("osascript failed: {stderr}").into());
    }
    println!("Trash emptied.");
    Ok(())
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn empty_trash() -> Result<(), Box<dyn std::error::Error>> {
    Err("Emptying trash is not supported on this platform".into())
}
