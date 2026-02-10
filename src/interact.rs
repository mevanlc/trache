// Platform-independent interaction primitives and naming helpers.
// Used by platform-specific restore code; not all platforms support it yet.
#![allow(dead_code)]

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

// --- Types ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TwinChoice {
    All,
    Some(Vec<usize>), // 1-indexed selections
    None,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionChoice {
    Overwrite,
    KeepBoth,
    None,
    Quit,
}

#[derive(Debug, Clone)]
pub struct TwinInfo {
    pub name: String,
    pub timestamp: String,
}

// --- Semantic prompt functions ---

pub fn prompt_yes(input: &mut dyn BufRead, prompt: &str) -> bool {
    eprint!("{}", prompt);
    io::stderr().flush().ok();

    let mut line = String::new();
    if input.read_line(&mut line).is_err() {
        return false;
    }

    let response = line.trim().to_lowercase();
    matches!(response.as_str(), "y" | "yes")
}

pub fn prompt_collision(
    input: &mut dyn BufRead,
    path: &Path,
    keep_name: &Path,
    once: bool,
) -> CollisionChoice {
    eprintln!("\n{} already exists.", path.display());
    eprintln!("(o) Overwrite: replace existing file");
    eprintln!("(k) Keep both: restore as {}", keep_name.display());
    eprintln!("(n) None: skip this file");
    eprintln!("(q) Quit");
    if once {
        eprintln!("(this choice will apply to all future 'path already exists' conflicts)");
    }

    loop {
        eprint!("Choice: ");
        io::stderr().flush().ok();

        let mut line = String::new();
        if input.read_line(&mut line).unwrap_or(0) == 0 {
            return CollisionChoice::Quit; // EOF
        }

        match line.trim().to_lowercase().chars().next() {
            Some('o') => return CollisionChoice::Overwrite,
            Some('k') => return CollisionChoice::KeepBoth,
            Some('n') => return CollisionChoice::None,
            Some('q') => return CollisionChoice::Quit,
            _ => eprintln!("Invalid choice."),
        }
    }
}

pub fn prompt_twins(
    input: &mut dyn BufRead,
    path: &Path,
    twins: &[TwinInfo],
    range_desc: &str,
    once: bool,
) -> TwinChoice {
    let count = twins.len();

    loop {
        eprintln!("\nThe following path was trashed {count} times:");
        eprintln!("  {}", path.display());
        eprintln!("(a) All: restore as {range_desc}");
        eprintln!("(s) Some: select versions to restore");
        eprintln!("(l) List: show details");
        eprintln!("(n) None: skip");
        eprintln!("(q) Quit");
        if once {
            eprintln!("(this choice will apply to all future twin conflicts)");
        }

        eprint!("Choice: ");
        io::stderr().flush().ok();

        let mut line = String::new();
        if input.read_line(&mut line).unwrap_or(0) == 0 {
            return TwinChoice::Quit; // EOF
        }

        match line.trim().to_lowercase().chars().next() {
            Some('l') => {
                for (i, twin) in twins.iter().enumerate() {
                    eprintln!("  {}: {} ({})", i + 1, twin.name, twin.timestamp);
                }
                continue;
            }
            Some('a') => return TwinChoice::All,
            Some('n') => return TwinChoice::None,
            Some('q') => return TwinChoice::Quit,
            Some('s') => {
                // Show numbered list for selection
                for (i, twin) in twins.iter().enumerate() {
                    eprintln!("  {}: {} ({})", i + 1, twin.name, twin.timestamp);
                }
                match prompt_selection(input, count) {
                    Some(sel) => return TwinChoice::Some(sel),
                    Option::None => return TwinChoice::None, // EOF during selection
                }
            }
            _ => eprintln!("Invalid choice."),
        }
    }
}

pub fn prompt_selection(input: &mut dyn BufRead, count: usize) -> Option<Vec<usize>> {
    loop {
        eprint!("Select items (e.g. 1,3-5): ");
        io::stderr().flush().ok();

        let mut line = String::new();
        if input.read_line(&mut line).unwrap_or(0) == 0 {
            return None; // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match parse_selection(trimmed, count) {
            Ok(sel) if !sel.is_empty() => return Some(sel),
            Ok(_) => eprintln!("No items selected."),
            Err(e) => eprintln!("Invalid selection: {e}"),
        }
    }
}

// --- Naming helpers ---

pub fn untrash_name(path: &Path, n: usize) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    if let Some(ext) = path.extension() {
        parent.join(format!("{stem}-untrash_{n}.{}", ext.to_string_lossy()))
    } else {
        parent.join(format!("{stem}-untrash_{n}"))
    }
}

pub fn find_untrash_range(path: &Path, count: usize) -> usize {
    let mut start = 1;
    'outer: loop {
        for i in start..start + count {
            if untrash_name(path, i).exists() {
                start = i + 1;
                continue 'outer;
            }
        }
        return start;
    }
}

pub fn format_untrash_range(path: &Path, start: usize, end: usize) -> String {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy();
        if start == end {
            format!("{stem}-untrash_{start}.{ext}")
        } else {
            format!("{stem}-untrash_{{{start}..{end}}}.{ext}")
        }
    } else if start == end {
        format!("{stem}-untrash_{start}")
    } else {
        format!("{stem}-untrash_{{{start}..{end}}}")
    }
}

pub fn collision_choice_name(c: CollisionChoice) -> &'static str {
    match c {
        CollisionChoice::Overwrite => "overwrite",
        CollisionChoice::KeepBoth => "keep both",
        CollisionChoice::None => "none",
        CollisionChoice::Quit => "quit",
    }
}

pub fn parse_selection(input: &str, max: usize) -> Result<Vec<usize>, String> {
    let mut result = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let start: usize = a.trim().parse().map_err(|_| format!("invalid number: '{}'", a.trim()))?;
            let end: usize = b.trim().parse().map_err(|_| format!("invalid number: '{}'", b.trim()))?;
            if start > end {
                return Err(format!("invalid range: {start}-{end}"));
            }
            for i in start..=end {
                if i < 1 || i > max {
                    return Err(format!("selection {i} out of range (1-{max})"));
                }
                result.push(i);
            }
        } else {
            let n: usize = part.parse().map_err(|_| format!("invalid number: '{part}'"))?;
            if n < 1 || n > max {
                return Err(format!("selection {n} out of range (1-{max})"));
            }
            result.push(n);
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;

    // --- parse_selection tests ---

    #[test]
    fn test_parse_selection_single() {
        assert_eq!(parse_selection("3", 5).unwrap(), vec![3]);
    }

    #[test]
    fn test_parse_selection_range() {
        assert_eq!(parse_selection("2-4", 5).unwrap(), vec![2, 3, 4]);
    }

    #[test]
    fn test_parse_selection_mixed() {
        assert_eq!(parse_selection("1,3-5,7", 7).unwrap(), vec![1, 3, 4, 5, 7]);
    }

    #[test]
    fn test_parse_selection_dedup() {
        assert_eq!(parse_selection("1,1,2", 3).unwrap(), vec![1, 2]);
    }

    #[test]
    fn test_parse_selection_out_of_range() {
        assert!(parse_selection("0", 5).is_err());
        assert!(parse_selection("6", 5).is_err());
    }

    #[test]
    fn test_parse_selection_invalid() {
        assert!(parse_selection("abc", 5).is_err());
        assert!(parse_selection("5-3", 5).is_err());
    }

    #[test]
    fn test_parse_selection_whitespace() {
        assert_eq!(parse_selection(" 1 , 3 - 5 ", 5).unwrap(), vec![1, 3, 4, 5]);
    }

    // --- untrash_name tests ---

    #[test]
    fn test_untrash_name_with_ext() {
        let p = Path::new("/home/user/foo.txt");
        assert_eq!(untrash_name(p, 1), PathBuf::from("/home/user/foo-untrash_1.txt"));
        assert_eq!(untrash_name(p, 42), PathBuf::from("/home/user/foo-untrash_42.txt"));
    }

    #[test]
    fn test_untrash_name_no_ext() {
        let p = Path::new("/home/user/Makefile");
        assert_eq!(untrash_name(p, 1), PathBuf::from("/home/user/Makefile-untrash_1"));
    }

    #[test]
    fn test_untrash_name_dotfile() {
        let p = Path::new("/home/user/.gitignore");
        assert_eq!(untrash_name(p, 1), PathBuf::from("/home/user/.gitignore-untrash_1"));
    }

    #[test]
    fn test_untrash_name_multiple_dots() {
        let p = Path::new("/home/user/archive.tar.gz");
        assert_eq!(untrash_name(p, 1), PathBuf::from("/home/user/archive.tar-untrash_1.gz"));
    }

    // --- find_untrash_range tests ---

    #[test]
    fn test_find_untrash_range_no_conflicts() {
        let tmp = tempfile::TempDir::new().unwrap();
        let p = tmp.path().join("foo.txt");
        assert_eq!(find_untrash_range(&p, 3), 1);
    }

    #[test]
    fn test_find_untrash_range_with_conflicts() {
        let tmp = tempfile::TempDir::new().unwrap();
        let p = tmp.path().join("foo.txt");
        fs::write(tmp.path().join("foo-untrash_1.txt"), "").unwrap();
        fs::write(tmp.path().join("foo-untrash_2.txt"), "").unwrap();
        assert_eq!(find_untrash_range(&p, 3), 3);
    }

    #[test]
    fn test_find_untrash_range_gap() {
        let tmp = tempfile::TempDir::new().unwrap();
        let p = tmp.path().join("foo.txt");
        fs::write(tmp.path().join("foo-untrash_1.txt"), "").unwrap();
        fs::write(tmp.path().join("foo-untrash_3.txt"), "").unwrap();
        assert_eq!(find_untrash_range(&p, 1), 2);
        assert_eq!(find_untrash_range(&p, 2), 4);
    }

    // --- prompt_yes tests ---

    #[test]
    fn test_prompt_yes_y() {
        let mut input = Cursor::new(b"y\n");
        assert!(prompt_yes(&mut input, "proceed? "));
    }

    #[test]
    fn test_prompt_yes_yes() {
        let mut input = Cursor::new(b"yes\n");
        assert!(prompt_yes(&mut input, "proceed? "));
    }

    #[test]
    fn test_prompt_yes_no() {
        let mut input = Cursor::new(b"n\n");
        assert!(!prompt_yes(&mut input, "proceed? "));
    }

    #[test]
    fn test_prompt_yes_eof() {
        let mut input = Cursor::new(b"");
        assert!(!prompt_yes(&mut input, "proceed? "));
    }

    // --- prompt_collision tests ---

    #[test]
    fn test_prompt_collision_overwrite() {
        let mut input = Cursor::new(b"o\n");
        let path = Path::new("/home/user/foo.txt");
        let keep = Path::new("/home/user/foo-untrash_1.txt");
        assert_eq!(prompt_collision(&mut input, path, keep, false), CollisionChoice::Overwrite);
    }

    #[test]
    fn test_prompt_collision_keep_both() {
        let mut input = Cursor::new(b"k\n");
        let path = Path::new("/home/user/foo.txt");
        let keep = Path::new("/home/user/foo-untrash_1.txt");
        assert_eq!(prompt_collision(&mut input, path, keep, false), CollisionChoice::KeepBoth);
    }

    #[test]
    fn test_prompt_collision_none() {
        let mut input = Cursor::new(b"n\n");
        let path = Path::new("/home/user/foo.txt");
        let keep = Path::new("/home/user/foo-untrash_1.txt");
        assert_eq!(prompt_collision(&mut input, path, keep, false), CollisionChoice::None);
    }

    #[test]
    fn test_prompt_collision_quit() {
        let mut input = Cursor::new(b"q\n");
        let path = Path::new("/home/user/foo.txt");
        let keep = Path::new("/home/user/foo-untrash_1.txt");
        assert_eq!(prompt_collision(&mut input, path, keep, false), CollisionChoice::Quit);
    }

    #[test]
    fn test_prompt_collision_invalid_then_valid() {
        let mut input = Cursor::new(b"x\no\n");
        let path = Path::new("/home/user/foo.txt");
        let keep = Path::new("/home/user/foo-untrash_1.txt");
        assert_eq!(prompt_collision(&mut input, path, keep, false), CollisionChoice::Overwrite);
    }

    #[test]
    fn test_prompt_collision_eof() {
        let mut input = Cursor::new(b"");
        let path = Path::new("/home/user/foo.txt");
        let keep = Path::new("/home/user/foo-untrash_1.txt");
        assert_eq!(prompt_collision(&mut input, path, keep, false), CollisionChoice::Quit);
    }

    // --- prompt_twins tests ---

    fn sample_twins() -> Vec<TwinInfo> {
        vec![
            TwinInfo { name: "foo.txt".into(), timestamp: "2024-01-15 10:30".into() },
            TwinInfo { name: "foo.txt".into(), timestamp: "2024-01-16 11:45".into() },
            TwinInfo { name: "foo.txt".into(), timestamp: "2024-01-17 09:00".into() },
        ]
    }

    #[test]
    fn test_prompt_twins_all() {
        let mut input = Cursor::new(b"a\n");
        let twins = sample_twins();
        let choice = prompt_twins(&mut input, Path::new("/tmp/foo.txt"), &twins, "foo-untrash_{1..3}.txt", false);
        assert_eq!(choice, TwinChoice::All);
    }

    #[test]
    fn test_prompt_twins_none() {
        let mut input = Cursor::new(b"n\n");
        let twins = sample_twins();
        let choice = prompt_twins(&mut input, Path::new("/tmp/foo.txt"), &twins, "foo-untrash_{1..3}.txt", false);
        assert_eq!(choice, TwinChoice::None);
    }

    #[test]
    fn test_prompt_twins_quit() {
        let mut input = Cursor::new(b"q\n");
        let twins = sample_twins();
        let choice = prompt_twins(&mut input, Path::new("/tmp/foo.txt"), &twins, "foo-untrash_{1..3}.txt", false);
        assert_eq!(choice, TwinChoice::Quit);
    }

    #[test]
    fn test_prompt_twins_list_then_all() {
        let mut input = Cursor::new(b"l\na\n");
        let twins = sample_twins();
        let choice = prompt_twins(&mut input, Path::new("/tmp/foo.txt"), &twins, "foo-untrash_{1..3}.txt", false);
        assert_eq!(choice, TwinChoice::All);
    }

    #[test]
    fn test_prompt_twins_some_single() {
        let mut input = Cursor::new(b"s\n2\n");
        let twins = sample_twins();
        let choice = prompt_twins(&mut input, Path::new("/tmp/foo.txt"), &twins, "foo-untrash_{1..3}.txt", false);
        assert_eq!(choice, TwinChoice::Some(vec![2]));
    }

    #[test]
    fn test_prompt_twins_some_range() {
        let mut input = Cursor::new(b"s\n1,3\n");
        let twins = sample_twins();
        let choice = prompt_twins(&mut input, Path::new("/tmp/foo.txt"), &twins, "foo-untrash_{1..3}.txt", false);
        assert_eq!(choice, TwinChoice::Some(vec![1, 3]));
    }

    #[test]
    fn test_prompt_twins_some_invalid_then_valid() {
        let mut input = Cursor::new(b"s\nabc\n2\n");
        let twins = sample_twins();
        let choice = prompt_twins(&mut input, Path::new("/tmp/foo.txt"), &twins, "foo-untrash_{1..3}.txt", false);
        assert_eq!(choice, TwinChoice::Some(vec![2]));
    }

    #[test]
    fn test_prompt_twins_eof() {
        let mut input = Cursor::new(b"");
        let twins = sample_twins();
        let choice = prompt_twins(&mut input, Path::new("/tmp/foo.txt"), &twins, "foo-untrash_{1..3}.txt", false);
        assert_eq!(choice, TwinChoice::Quit);
    }

    #[test]
    fn test_prompt_twins_some_eof_during_selection() {
        let mut input = Cursor::new(b"s\n");
        let twins = sample_twins();
        let choice = prompt_twins(&mut input, Path::new("/tmp/foo.txt"), &twins, "foo-untrash_{1..3}.txt", false);
        assert_eq!(choice, TwinChoice::None);
    }

    #[test]
    fn test_prompt_twins_invalid_then_valid() {
        let mut input = Cursor::new(b"x\nz\na\n");
        let twins = sample_twins();
        let choice = prompt_twins(&mut input, Path::new("/tmp/foo.txt"), &twins, "foo-untrash_{1..3}.txt", false);
        assert_eq!(choice, TwinChoice::All);
    }
}
