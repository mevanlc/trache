use bytesize::ByteSize;
use std::io::{self, Write};
use std::path::PathBuf;

#[cfg(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"), not(target_os = "ios"))
))]
use std::collections::BTreeMap;
#[cfg(target_os = "windows")]
use std::path::{Component, Path, Prefix};
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios")))]
use std::{fs, path::Path};

#[derive(Debug)]
pub(crate) struct TrashReport {
    groups: Vec<TrashGroupReport>,
}

#[derive(Debug)]
struct TrashGroupReport {
    label: String,
    items: Vec<TrashItemReport>,
}

#[derive(Debug)]
struct TrashItemReport {
    original_path: PathBuf,
    size: ReportedSize,
}

#[derive(Debug)]
enum ReportedSize {
    Exact(u64),
    Unavailable(UnsizedReason),
}

#[derive(Debug)]
enum UnsizedReason {
    Directory { immediate_entries: usize },
    MetadataError { message: String },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TrashRollup {
    item_count: usize,
    known_bytes: u64,
    unavailable_count: usize,
    unavailable_directories: usize,
}

impl TrashReport {
    fn new(mut groups: Vec<TrashGroupReport>) -> Self {
        groups.sort_by(|left, right| left.label.cmp(&right.label));
        for group in &mut groups {
            group
                .items
                .sort_by(|left, right| left.original_path.cmp(&right.original_path));
        }
        Self { groups }
    }

    fn rollup(&self) -> io::Result<TrashRollup> {
        rollup_items(self.groups.iter().flat_map(|group| &group.items))
    }
}

impl TrashGroupReport {
    fn rollup(&self) -> io::Result<TrashRollup> {
        rollup_items(&self.items)
    }
}

fn rollup_items<'a>(
    items: impl IntoIterator<Item = &'a TrashItemReport>,
) -> io::Result<TrashRollup> {
    let mut rollup = TrashRollup::default();

    for item in items {
        rollup.item_count = rollup
            .item_count
            .checked_add(1)
            .ok_or_else(|| io::Error::other("trash item count exceeds usize"))?;
        match &item.size {
            ReportedSize::Exact(bytes) => {
                rollup.known_bytes = rollup
                    .known_bytes
                    .checked_add(*bytes)
                    .ok_or_else(|| io::Error::other("trash size exceeds u64"))?;
            }
            ReportedSize::Unavailable(reason) => {
                rollup.unavailable_count += 1;
                if matches!(reason, UnsizedReason::Directory { .. }) {
                    rollup.unavailable_directories += 1;
                }
            }
        }
    }

    Ok(rollup)
}

fn format_size(bytes: u64) -> String {
    ByteSize::b(bytes).display().iec().to_string()
}

fn format_rollup(rollup: TrashRollup) -> String {
    if rollup.unavailable_count == 0 {
        return format!(
            "{} item(s), {}",
            rollup.item_count,
            format_size(rollup.known_bytes)
        );
    }

    let (count, noun) = if rollup.unavailable_directories == rollup.unavailable_count {
        (
            rollup.unavailable_directories,
            if rollup.unavailable_directories == 1 {
                "unsized directory"
            } else {
                "unsized directories"
            },
        )
    } else {
        (
            rollup.unavailable_count,
            if rollup.unavailable_count == 1 {
                "unsized item"
            } else {
                "unsized items"
            },
        )
    };

    format!(
        "{} item(s), at least {} ({count} {noun})",
        rollup.item_count,
        format_size(rollup.known_bytes)
    )
}

fn write_item<W: Write>(writer: &mut W, item: &TrashItemReport) -> io::Result<()> {
    match &item.size {
        ReportedSize::Exact(bytes) => writeln!(
            writer,
            "    {:>10}  {}",
            format_size(*bytes),
            item.original_path.display()
        ),
        ReportedSize::Unavailable(UnsizedReason::Directory { immediate_entries }) => {
            let noun = if *immediate_entries == 1 {
                "entry"
            } else {
                "entries"
            };
            writeln!(
                writer,
                "    {:>10}  {} (directory, {} immediate {noun})",
                "?",
                item.original_path.display(),
                immediate_entries
            )
        }
        ReportedSize::Unavailable(UnsizedReason::MetadataError { message }) => writeln!(
            writer,
            "    {:>10}  {} (size unavailable: {message})",
            "?",
            item.original_path.display()
        ),
    }
}

fn write_trash_report<W: Write>(
    writer: &mut W,
    report: &TrashReport,
    verbose: bool,
    total_label: &str,
) -> io::Result<()> {
    if report.groups.is_empty() {
        writeln!(writer, "No trash directories found.")?;
        return Ok(());
    }

    // Calculate everything before writing so an overflow never leaves a partial report behind.
    let group_rollups = report
        .groups
        .iter()
        .map(TrashGroupReport::rollup)
        .collect::<io::Result<Vec<_>>>()?;
    let total_rollup = report.rollup()?;

    if verbose {
        writeln!(writer, "Trash items:")?;
        for (group, rollup) in report.groups.iter().zip(&group_rollups) {
            writeln!(writer, "  {}: {}", group.label, format_rollup(*rollup))?;
            for item in &group.items {
                write_item(writer, item)?;
            }
        }
        writeln!(writer)?;
    }

    writeln!(writer, "Trash summary:")?;
    for (group, rollup) in report.groups.iter().zip(&group_rollups) {
        writeln!(writer, "  {}: {}", group.label, format_rollup(*rollup))?;
    }
    writeln!(writer, "{total_label}: {}", format_rollup(total_rollup))
}

pub(crate) fn print_trash_report(
    report: &TrashReport,
    verbose: bool,
    total_label: &str,
) -> io::Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    write_trash_report(&mut stdout, report, verbose, total_label)
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios")))]
fn freedesktop_item_paths(item: &trash::TrashItem) -> io::Result<(PathBuf, PathBuf)> {
    let info_file = Path::new(&item.id);
    let trash_folder = info_file
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid trash item ID"))?;
    let name_in_trash = info_file
        .file_stem()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid trash item ID"))?;
    let payload = trash_folder.join("files").join(name_in_trash);
    Ok((trash_folder.to_owned(), payload))
}

/// Return the apparent size of a path and all descendants. Symlinks are measured but not followed.
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios")))]
fn apparent_size(path: &Path) -> io::Result<u64> {
    let mut total = 0u64;
    let mut pending = vec![path.to_owned()];

    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        } else {
            total = total
                .checked_add(metadata.len())
                .ok_or_else(|| io::Error::other("trash size exceeds u64"))?;
        }
    }

    Ok(total)
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios")))]
fn collect_freedesktop_trash_report(
    items: Vec<trash::TrashItem>,
) -> Result<TrashReport, Box<dyn std::error::Error>> {
    let mut by_folder: BTreeMap<PathBuf, Vec<TrashItemReport>> = BTreeMap::new();

    if let Ok(folders) = trash::os_limited::trash_folders() {
        for folder in folders {
            by_folder.entry(folder).or_default();
        }
    }

    for item in items {
        let original_path = item.original_path();
        let (folder, payload) = freedesktop_item_paths(&item)?;
        let size = ReportedSize::Exact(apparent_size(&payload)?);
        by_folder.entry(folder).or_default().push(TrashItemReport {
            original_path,
            size,
        });
    }

    let groups = by_folder
        .into_iter()
        .map(|(folder, items)| TrashGroupReport {
            label: folder.display().to_string(),
            items,
        })
        .collect();
    Ok(TrashReport::new(groups))
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios")))]
pub(crate) fn collect_trash_report() -> Result<TrashReport, Box<dyn std::error::Error>> {
    collect_freedesktop_trash_report(trash::os_limited::list()?)
}

#[cfg(target_os = "windows")]
const WINDOWS_UNCLASSIFIED_LABEL: &str = "Windows Recycle Bin (unclassified)";

/// Return a stable grouping label based on an item's original volume.
///
/// This describes the original path, not a physical `$Recycle.Bin` directory.
#[cfg(target_os = "windows")]
fn windows_volume_label(path: &Path) -> String {
    let Some(Component::Prefix(component)) = path.components().next() else {
        return WINDOWS_UNCLASSIFIED_LABEL.to_owned();
    };

    match component.kind() {
        Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => {
            format!("Recycle Bin [{}:]", char::from(drive).to_ascii_uppercase())
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => format!(
            "Recycle Bin [\\\\{}\\{}]",
            server.to_string_lossy(),
            share.to_string_lossy()
        ),
        _ => WINDOWS_UNCLASSIFIED_LABEL.to_owned(),
    }
}

#[cfg(target_os = "windows")]
fn compact_metadata_error(error: &trash::Error) -> String {
    let message = match error {
        trash::Error::Unknown { description } | trash::Error::Os { description, .. } => {
            description.as_str()
        }
        _ => "metadata lookup failed",
    };
    let message = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let message = if message.is_empty() {
        "metadata lookup failed".to_owned()
    } else {
        message
    };

    const MAX_CHARS: usize = 160;
    if message.chars().count() <= MAX_CHARS {
        message
    } else {
        let mut shortened = message.chars().take(MAX_CHARS - 3).collect::<String>();
        shortened.push_str("...");
        shortened
    }
}

#[cfg(target_os = "windows")]
fn collect_windows_trash_report_with<F>(
    items: Vec<trash::TrashItem>,
    mut metadata_for: F,
) -> TrashReport
where
    F: FnMut(&trash::TrashItem) -> Result<trash::TrashItemMetadata, String>,
{
    let mut by_volume: BTreeMap<String, Vec<TrashItemReport>> = BTreeMap::new();

    for item in items {
        let original_path = item.original_path();
        let label = windows_volume_label(&original_path);
        let size = match metadata_for(&item) {
            Ok(metadata) => match metadata.size {
                trash::TrashItemSize::Bytes(bytes) => ReportedSize::Exact(bytes),
                trash::TrashItemSize::Entries(immediate_entries) => {
                    ReportedSize::Unavailable(UnsizedReason::Directory { immediate_entries })
                }
            },
            Err(message) => ReportedSize::Unavailable(UnsizedReason::MetadataError { message }),
        };
        by_volume.entry(label).or_default().push(TrashItemReport {
            original_path,
            size,
        });
    }

    if by_volume.is_empty() {
        by_volume.insert("Windows Recycle Bin (all volumes)".to_owned(), Vec::new());
    }

    TrashReport::new(
        by_volume
            .into_iter()
            .map(|(label, items)| TrashGroupReport { label, items })
            .collect(),
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn collect_trash_report() -> Result<TrashReport, Box<dyn std::error::Error>> {
    let items = trash::os_limited::list()?;
    Ok(collect_windows_trash_report_with(items, |item| {
        trash::os_limited::metadata(item).map_err(|error| compact_metadata_error(&error))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &str, size: ReportedSize) -> TrashItemReport {
        TrashItemReport {
            original_path: PathBuf::from(path),
            size,
        }
    }

    fn group(label: &str, items: Vec<TrashItemReport>) -> TrashGroupReport {
        TrashGroupReport {
            label: label.to_owned(),
            items,
        }
    }

    fn render(report: &TrashReport, verbose: bool, total_label: &str) -> io::Result<String> {
        let mut output = Vec::new();
        write_trash_report(&mut output, report, verbose, total_label)?;
        Ok(String::from_utf8(output).expect("report is UTF-8"))
    }

    #[test]
    fn renders_one_exact_group() {
        let report = TrashReport::new(vec![group(
            "Recycle Bin [C:]",
            vec![item("C:/old.bin", ReportedSize::Exact(4096))],
        )]);

        assert_eq!(
            render(&report, false, "Total").unwrap(),
            "Trash summary:\n  Recycle Bin [C:]: 1 item(s), 4.0 KiB\nTotal: 1 item(s), 4.0 KiB\n"
        );
    }

    #[test]
    fn renders_multiple_exact_groups_and_overall_total() {
        let report = TrashReport::new(vec![
            group("B", vec![item("b", ReportedSize::Exact(2048))]),
            group("A", vec![item("a", ReportedSize::Exact(1024))]),
        ]);
        let output = render(&report, false, "Total").unwrap();

        assert!(output.find("  A:").unwrap() < output.find("  B:").unwrap());
        assert!(output.contains("Total: 2 item(s), 3.0 KiB"));
    }

    #[test]
    fn renders_mixed_sizes_as_a_lower_bound() {
        let report = TrashReport::new(vec![group(
            "A",
            vec![
                item("exact", ReportedSize::Exact(2048)),
                item(
                    "directory",
                    ReportedSize::Unavailable(UnsizedReason::Directory {
                        immediate_entries: 4,
                    }),
                ),
                item(
                    "missing",
                    ReportedSize::Unavailable(UnsizedReason::MetadataError {
                        message: "item disappeared".to_owned(),
                    }),
                ),
            ],
        )]);
        let output = render(&report, false, "Total").unwrap();

        assert!(output.contains("3 item(s), at least 2.0 KiB (2 unsized items)"));
        assert!(!output.contains("unsized directories"));
    }

    #[test]
    fn renders_an_unavailable_only_group_as_at_least_zero() {
        let report = TrashReport::new(vec![group(
            "A",
            vec![item(
                "directory",
                ReportedSize::Unavailable(UnsizedReason::Directory {
                    immediate_entries: 0,
                }),
            )],
        )]);
        let output = render(&report, false, "Total").unwrap();

        assert!(output.contains("1 item(s), at least 0 B (1 unsized directory)"));
    }

    #[test]
    fn overall_rollup_remains_a_lower_bound() {
        let report = TrashReport::new(vec![
            group("A", vec![item("a", ReportedSize::Exact(1024))]),
            group(
                "B",
                vec![
                    item("b", ReportedSize::Exact(2048)),
                    item(
                        "dir",
                        ReportedSize::Unavailable(UnsizedReason::Directory {
                            immediate_entries: 2,
                        }),
                    ),
                ],
            ),
        ]);
        let output = render(&report, false, "Total").unwrap();

        assert!(output.contains("Total: 3 item(s), at least 3.0 KiB (1 unsized directory)"));
    }

    #[test]
    fn sorts_groups_and_items_deterministically() {
        let report = TrashReport::new(vec![
            group(
                "B",
                vec![
                    item("z", ReportedSize::Exact(0)),
                    item("a", ReportedSize::Exact(0)),
                ],
            ),
            group("A", vec![item("m", ReportedSize::Exact(0))]),
        ]);
        let output = render(&report, true, "Total").unwrap();

        assert!(output.find("  A:").unwrap() < output.find("  B:").unwrap());
        assert!(output.find("  a\n").unwrap() < output.find("  z\n").unwrap());
    }

    #[test]
    fn verbose_output_lists_items_then_repeats_the_summary() {
        let report = TrashReport::new(vec![group(
            "A",
            vec![
                item("exact", ReportedSize::Exact(12)),
                item(
                    "directory",
                    ReportedSize::Unavailable(UnsizedReason::Directory {
                        immediate_entries: 1,
                    }),
                ),
                item(
                    "missing",
                    ReportedSize::Unavailable(UnsizedReason::MetadataError {
                        message: "item disappeared".to_owned(),
                    }),
                ),
            ],
        )]);
        let output = render(&report, true, "Would permanently delete").unwrap();

        assert!(output.starts_with("Trash items:\n"));
        assert!(output.contains("?  directory (directory, 1 immediate entry)"));
        assert!(output.contains("?  missing (size unavailable: item disappeared)"));
        assert!(output.find("Trash items:").unwrap() < output.find("Trash summary:").unwrap());
        assert!(
            output.ends_with(
                "Would permanently delete: 3 item(s), at least 12 B (2 unsized items)\n"
            )
        );
    }

    #[test]
    fn non_verbose_output_omits_item_paths() {
        let report = TrashReport::new(vec![group(
            "A",
            vec![item("secret-path", ReportedSize::Exact(1))],
        )]);

        assert!(
            !render(&report, false, "Total")
                .unwrap()
                .contains("secret-path")
        );
    }

    #[test]
    fn renderer_uses_the_requested_final_label() {
        let report = TrashReport::new(vec![group("A", Vec::new())]);

        assert!(
            render(&report, false, "Total")
                .unwrap()
                .contains("Total: 0 item(s), 0 B")
        );
        assert!(
            render(&report, false, "Would permanently delete")
                .unwrap()
                .contains("Would permanently delete: 0 item(s), 0 B")
        );
    }

    #[test]
    fn checked_addition_rejects_size_overflow_before_writing() {
        let report = TrashReport::new(vec![group(
            "A",
            vec![
                item("max", ReportedSize::Exact(u64::MAX)),
                item("one", ReportedSize::Exact(1)),
            ],
        )]);
        let mut output = Vec::new();

        let error = write_trash_report(&mut output, &report, true, "Total").unwrap_err();
        assert_eq!(error.to_string(), "trash size exceeds u64");
        assert!(output.is_empty());
    }

    #[cfg(target_os = "windows")]
    fn windows_item(parent: &str, name: &str) -> trash::TrashItem {
        trash::TrashItem {
            id: format!("id-{name}").into(),
            name: name.into(),
            original_parent: PathBuf::from(parent),
            time_deleted: 0,
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_volume_labels_cover_drives_unc_and_fallback() {
        assert_eq!(
            windows_volume_label(Path::new(r"c:\old\file")),
            "Recycle Bin [C:]"
        );
        assert_eq!(
            windows_volume_label(Path::new(r"\\?\D:\old\file")),
            "Recycle Bin [D:]"
        );
        assert_eq!(
            windows_volume_label(Path::new(r"\\server\share\old\file")),
            r"Recycle Bin [\\server\share]"
        );
        assert_eq!(
            windows_volume_label(Path::new(r"relative\file")),
            WINDOWS_UNCLASSIFIED_LABEL
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_collector_converts_metadata_failures_and_sorts_items() {
        let items = vec![
            windows_item(r"C:\old", "z-file"),
            windows_item(r"D:\old", "missing"),
            windows_item(r"C:\old", "a-directory"),
        ];
        let mut calls = 0;
        let report = collect_windows_trash_report_with(items, |item| {
            calls += 1;
            match item.name.to_string_lossy().as_ref() {
                "z-file" => Ok(trash::TrashItemMetadata {
                    size: trash::TrashItemSize::Bytes(4096),
                }),
                "a-directory" => Ok(trash::TrashItemMetadata {
                    size: trash::TrashItemSize::Entries(3),
                }),
                _ => Err("item disappeared".to_owned()),
            }
        });
        let output = render(&report, true, "Total").unwrap();

        assert_eq!(calls, 3);
        assert!(
            output.contains("Recycle Bin [C:]: 2 item(s), at least 4.0 KiB (1 unsized directory)")
        );
        assert!(output.contains("Recycle Bin [D:]: 1 item(s), at least 0 B (1 unsized item)"));
        assert!(output.find("a-directory").unwrap() < output.find("z-file").unwrap());
        assert!(output.contains("directory, 3 immediate entries"));
        assert!(output.contains("size unavailable: item disappeared"));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn empty_windows_report_uses_the_all_volumes_group() {
        let report = collect_windows_trash_report_with(Vec::new(), |_| {
            unreachable!("an empty report has no metadata lookups")
        });

        assert_eq!(
            render(&report, false, "Total").unwrap(),
            "Trash summary:\n  Windows Recycle Bin (all volumes): 0 item(s), 0 B\nTotal: 0 item(s), 0 B\n"
        );
    }
}
