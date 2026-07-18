# Plan: Windows trash statistics and empty-trash previews

## Summary

Add Windows support for the reporting behavior shared by:

- `trache --trash-stats [-v|--verbose]`
- `trache --trash-empty --trash-dry-run [-v|--verbose]`

Reuse one platform-neutral report model and renderer. Keep platform-specific code responsible only for discovering trash groups and measuring their items.

The initial Windows implementation will provide exact sizes for top-level files and explicit lower bounds when a directory cannot yet be measured recursively. It must never present a partial byte count as an exact total.

## Current behavior

### Freedesktop platforms

`--trash-stats` and the empty-trash dry run already share a recursive collector and renderer in `src/main.rs`.

The collector:

- obtains items through `trash::os_limited::list()`;
- groups items by Freedesktop trash directory;
- derives each payload path from its `.trashinfo` identifier;
- recursively sums apparent sizes;
- measures symlinks without following them; and
- produces exact per-trash and overall byte totals.

`--trash-stats` ends with `Total:`, while the empty-trash dry run ends with `Would permanently delete:`.

### Windows

The patched `trash` backend already:

- enumerates the unified Windows Recycle Bin with `FOLDERID_RecycleBinFolder`;
- returns each item's original parent, name, deletion time, and Shell parsing identifier;
- returns `TrashItemSize::Bytes` for top-level files; and
- returns `TrashItemSize::Entries` for directories by enumerating their immediate children.

The Windows backend does not currently return recursive directory byte sizes. Consequently:

- `--trash-stats` returns an unsupported-platform error; and
- the empty-trash dry run reports only the unified scope, item paths under `-v`, and an item count.

## Goals

1. Support both reporting commands on Windows.
2. Reuse the same section ordering and action-oriented final labels as the Freedesktop implementation.
3. Show exact file sizes already available through the Windows Shell API.
4. Represent unsized directories and metadata failures explicitly.
5. Produce mathematically valid lower-bound rollups whenever any item is unsized.
6. Group items by original Windows volume when it can be identified reliably.
7. Preserve the behavior of the real, non-dry-run empty operation.
8. Keep output deterministic and testable without requiring destructive access to a developer's actual Recycle Bin.

## Non-goals for the initial implementation

- Do not recursively enumerate Windows directory contents to calculate exact directory byte sizes.
- Do not inspect `$Recycle.Bin` directly with `std::fs`; Windows Shell parsing identifiers are not guaranteed to be ordinary filesystem paths.
- Do not change the semantics of trashing, restoring, purging, or actually emptying the Recycle Bin.
- Do not claim that known top-level file bytes approximate the full size. They are a lower bound.
- Do not add macOS statistics support as part of this work.

## Output contract

### Exact group

When every item in a group has a byte size:

```text
Trash summary:
  Recycle Bin [C:]: 3 item(s), 4.2 GiB
Total: 3 item(s), 4.2 GiB
```

### Group containing unsized items

Use `at least` and report the number of unsized items:

```text
Trash summary:
  Recycle Bin [C:]: 5 item(s), at least 4.2 GiB (2 unsized directories)
Total: 5 item(s), at least 4.2 GiB (2 unsized directories)
```

If an unsized item is not known to be a directory, say `unsized items` rather than `unsized directories`.

### Verbose statistics

Repeat the compact summary after the item listing so that it remains visible when the command completes:

```text
Trash items:
  Recycle Bin [C:]: 5 item(s), at least 4.2 GiB (2 unsized directories)
       4.0 GiB  C:\images\old.iso
       200 MiB  C:\downloads\archive.zip
             ?  C:\projects\old-tree (directory, 324 immediate entries)
             ?  C:\backup (directory, 18 immediate entries)
             0 B  C:\empty.txt

Trash summary:
  Recycle Bin [C:]: 5 item(s), at least 4.2 GiB (2 unsized directories)
Total: 5 item(s), at least 4.2 GiB (2 unsized directories)
```

### Empty-trash dry run

Use the same report, changing only the final label:

```text
Trash summary:
  Recycle Bin [C:]: 5 item(s), at least 4.2 GiB (2 unsized directories)
Would permanently delete: 5 item(s), at least 4.2 GiB (2 unsized directories)
```

With `-v`, include the same `Trash items:` section shown above.

### Multiple volumes

Sort groups by volume label and print an overall lower-bound total:

```text
Trash summary:
  Recycle Bin [C:]: 5 item(s), at least 4.2 GiB (2 unsized directories)
  Recycle Bin [D:]: 7 item(s), 900 MiB
Total: 12 item(s), at least 5.1 GiB (2 unsized directories)
```

Use `Windows Recycle Bin (unclassified)` when a usable original volume cannot be derived. If the Recycle Bin is empty and therefore has no item-derived volumes, use `Windows Recycle Bin (all volumes)` with zero items.

## Proposed report model

Move formatting out of the Freedesktop-only function and introduce a platform-neutral model. Names are illustrative:

```rust
struct TrashReport {
    groups: Vec<TrashGroupReport>,
}

struct TrashGroupReport {
    label: String,
    items: Vec<TrashItemReport>,
}

struct TrashItemReport {
    original_path: PathBuf,
    size: ReportedSize,
}

enum ReportedSize {
    Exact(u64),
    Unavailable(UnsizedReason),
}

enum UnsizedReason {
    Directory { immediate_entries: usize },
    MetadataError { message: String },
}
```

Group and overall rollups should be derived from items rather than stored independently. A rollup consists of:

- item count;
- sum of all exact bytes, using checked addition;
- number of unavailable sizes; and
- number of unavailable sizes known to be directories.

Formatting rules:

- no unavailable sizes: render the byte sum normally;
- one or more unavailable sizes: render `at least <known bytes>` plus an unsized-item annotation;
- an unavailable-only group: render `at least 0 B`, not `0 B`;
- use `bytesize` IEC formatting on every platform; and
- retain the existing `Trash items`, `Trash summary`, and final-label ordering.

The renderer should accept:

```rust
fn print_trash_report(report: &TrashReport, verbose: bool, total_label: &str)
```

The renderer must not contain `cfg(target_os = ...)` branches.

## Collector design

### Freedesktop collector

Refactor the existing implementation into a collector that returns `TrashReport`:

1. Call `list()` and `trash_folders()`.
2. Derive each payload path from its `.trashinfo` ID.
3. Calculate recursive apparent size exactly.
4. Use the trash directory path as the group label.
5. Preserve empty discovered trash directories as zero-item groups.

Expected Freedesktop output must remain unchanged after the refactor.

### Windows collector

1. Call `trash::os_limited::list()` once.
2. For each item, call `trash::os_limited::metadata()` once.
3. Convert `TrashItemSize::Bytes(n)` to `ReportedSize::Exact(n)`.
4. Convert `TrashItemSize::Entries(n)` to `ReportedSize::Unavailable(UnsizedReason::Directory { immediate_entries: n })`.
5. Treat a metadata error as `ReportedSize::Unavailable(UnsizedReason::MetadataError { ... })` instead of discarding the entire report. Keep its message short and suitable for verbose output.
6. Group the item from the prefix of `item.original_path()`:
   - drive and verbatim-drive prefixes become `Recycle Bin [C:]`, etc.;
   - UNC prefixes use a stable UNC label if encountered; and
   - missing or unsupported prefixes use `Windows Recycle Bin (unclassified)`.
7. Sort items by original path within each group.
8. Sort groups by label.

The volume label is derived from the original path, not from the Shell parsing ID. Document this as a grouping label rather than the physical `$Recycle.Bin` directory path.

## Command wiring

### `--trash-stats [-v|--verbose]`

- Freedesktop: collect exact recursive sizes, then render with `total_label = "Total"`.
- Windows: collect exact file sizes and unavailable directory sizes, then render with `total_label = "Total"`.
- macOS/iOS: retain the current unsupported behavior.

### `--trash-empty --trash-dry-run [-v|--verbose]`

- Freedesktop: use the same exact collector as `--trash-stats`.
- Windows: use the same lower-bound collector as `--trash-stats`.
- Render with `total_label = "Would permanently delete"`.
- Never invoke `purge_all()`.

### `--trash-empty` without `--trash-dry-run`

Keep the existing path unchanged:

1. Call `list()`.
2. If empty, report that the trash is already empty.
3. Call `purge_all(items)`.
4. Report the deleted item count.

Do not collect sizes before a real empty operation; the report cost belongs only to explicit inspection and dry-run commands.

## Error and race handling

Recycle Bin contents may change while the report is being collected.

- Failure to list the Recycle Bin remains a command error.
- Failure to read one item's metadata should make that item unsized, not fail the whole report.
- Checked addition must prevent byte-count overflow.
- A disappeared item should be shown as unsized if it was returned by `list()` but fails during metadata lookup.
- The dry-run report is a snapshot, not a guarantee that the subsequent real empty operation will affect the identical set.
- Avoid printing raw COM debug representations in user-facing output.

## Implementation sequence

1. Introduce the platform-neutral report types and rollup logic.
2. Extract the current layout into a platform-neutral renderer.
3. Port the Freedesktop collector to the new model and verify byte-for-byte output compatibility where practical.
4. Move the `bytesize::ByteSize` import out of the Freedesktop-only configuration so the renderer builds on Windows.
5. Add a Windows volume-label helper using Windows `Path` prefix components.
6. Add the Windows collector using `list()` and `metadata()`.
7. Replace the Windows `trash_stats()` unsupported stub.
8. Route the Windows empty-trash dry run through the shared collector and renderer.
9. Update CLI documentation and platform limitations.
10. Add platform-neutral, Windows-specific, and command-level tests.

## Testing strategy

### Platform-neutral tests

These tests should run on Termux and Windows using synthetic reports:

- one exact group;
- multiple exact groups;
- mixed exact and unavailable sizes;
- an unavailable-only group renders `at least 0 B`;
- overall lower-bound aggregation;
- deterministic group and item ordering;
- non-verbose output omits item lines;
- verbose output prints items first and repeats the summary last;
- `Total:` versus `Would permanently delete:` final labels; and
- checked-addition overflow behavior.

Prefer rendering into a `Write` target rather than capturing process stdout so formatting tests remain isolated.

### Windows collector tests

Separate collection from COM metadata lookup with a small injected function or trait so most tests can use synthetic `TrashItem` and `TrashItemMetadata` values.

Cover:

- drive-prefix grouping for at least `C:` and `D:`;
- fallback grouping for an unclassified path;
- exact file sizes;
- directories with immediate-entry details;
- metadata lookup failure;
- mixed exact and unavailable items; and
- stable path ordering.

### Windows integration tests

Run on a native Windows CI worker:

- `--trash-stats` succeeds instead of returning unsupported;
- a uniquely named trashed file reports its exact size;
- `--trash-stats -v` reports its original path;
- `--trash-empty --trash-dry-run` reports it but does not remove it;
- the verbose dry run has `Trash items` before `Trash summary`;
- the final dry-run label is `Would permanently delete:`; and
- cleanup purges only uniquely named test items.

Tests that manipulate the real Recycle Bin must use unique names, execute serially, and clean up on both success and failure where possible. Do not empty the entire Recycle Bin from a test.

### Build verification

- `cargo fmt -- --check`
- `git diff --check`
- `cargo test` on the existing Termux/Linux environment
- `cargo check --target x86_64-pc-windows-msvc` where the target is available
- native Windows tests in CI for COM and Shell behavior

## Acceptance criteria

- `--trash-stats` no longer returns unsupported on Windows.
- Both Windows reporting commands use the same report model and renderer as Freedesktop.
- Top-level Windows files contribute exact bytes.
- Unsized directories cause `at least` rollups and are visibly counted.
- No partial total is formatted as exact.
- `-v` prints per-item sizes or `?`, then repeats the summary at the end.
- Multiple original volumes receive separate, deterministically ordered groups.
- Empty-trash dry run never calls `purge_all()`.
- Real empty-trash behavior and output remain unchanged.
- Existing Freedesktop output and tests continue to pass.

## Follow-up: exact Windows directory sizes

A later change may recursively traverse directory `IShellItem` children through `BHID_EnumItems` and sum `PKEY_Size` for leaf items. That work should:

- avoid recursion depth limits by using an explicit work queue;
- define behavior for inaccessible or disappearing descendants;
- preserve lower-bound reporting whenever traversal is incomplete;
- measure links/reparse points without following them unexpectedly; and
- benchmark large directory trees before replacing the initial lower-bound implementation.

The report model proposed above already supports this upgrade: successfully traversed directories become `Exact(bytes)`, while incomplete traversal remains unavailable or can later be extended to carry a byte lower bound.
