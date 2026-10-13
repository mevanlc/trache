# trache - rm-compatible CLI trash and recycle bin access

`trache` (træʃ, like cache) is a fully compatible drop-in replacement for `rm` that sends things to Recycle Bin (Windows) or Trash (macOS/Linux/BSD).

## Motivation
Because...
* ...I can't stop automatically typing `rm -rf` (and other classic `rm` flags) but none of the existing trash CLIs accept rm-compatible flags.
* ...has `rm -W` ever worked for anyone? Probably not?
* ...the only advice the `rm` man-page gives you about data recovery is that "you might be able to recover the data after using `rm` on it, so consider using `shred` instead."

## Install

Download an archive from [GitHub Releases](https://github.com/mevanlc/trache/releases)
and put `trache` on your PATH. The new release workflow builds Linux (static musl),
macOS, Windows, and FreeBSD for x64 and ARM64, plus Android ARM64. Android execution
is not covered by hosted CI; FreeBSD downloads are tested on 15.1.

Install from crates.io with `cargo install trache --locked`. For a checkout:

```sh
cargo install --path . --locked
```

See [RELEASING.md](RELEASING.md) for CI, publishing, checksums, and release recovery.
Versions come from release tags; checkout builds report `0.0.0`.

## Usage

Put it on your PATH and alias it to `rm`. Or don't put it on your PATH and use it like `trash`. I'm not your life-coach.

## License

Octuply licensed under MIT, WTFPL, Unlicense, GLWTPL, Careware, JSON.org's DBE clause, Beerware, and DBAD. Because we believe you should have license to take license.

## Issues, PRs, etc. 

...are welcome. Thanks and enjoy! 

## Oh Yeah, the Usage...

```
Move files to trash. Manage trashed items.

Usage: trache [OPTIONS] [FILES]...

Arguments:
  [FILES]...  Files to trash

Options:
      --trash-list              List items in trash
      --trash-empty             Empty the entire trash
      --trash-stats             Show trash item counts and sizes; -v lists items
      --trash-undo <PATTERN>    Restore items matching pattern from trash (see --help)
      --trash-purge <PATTERN>   Permanently delete items matching pattern from trash (see --help)
      --trash-dry-run           Show what would be done without doing it
  -d, --dir                     Remove empty directories
  -r, --recursive               Remove directories and their contents recursively [aliases: -R]
  -i                            Prompt before every removal; also prompts during --trash-undo
  -I                            Prompt once before removing >3 files or recursively; remember first choice during --trash-undo
      --interactive [<WHEN>]    Prompt according to WHEN: never, once, or always; also affects --trash-undo (see --help) [possible values: never, once, always]
  -f, --force                   Ignore nonexistent files, never prompt
  -v, --verbose                 Explain what is being done
      --preserve-root [<MODE>]  Do not remove '/'; 'all' also rejects arguments on separate devices [possible values: no, yes, all]
      --no-preserve-root        Do not treat '/' specially
  -x, --one-file-system         Skip directories on different file systems
  -h, --help                    Print help (see more with '--help')
  -V, --version                 Print version
```

# Limitations

On macOS, trache keeps a temporary copy of itself running after trashing files so
Apple's delayed Finder **Put Back** metadata can finish writing. Your prompt returns
as soon as the moves finish. The helper reads the actual Trash directories' `.DS_Store`
files without modifying them, waits at least three seconds after the last move, and
checks for up to ten seconds after finishing its work. Put Back remains best effort;
missing or unreadable metadata does not change the command's exit status. If the
helper cannot start, trache performs the work and wait in the foreground.

On macOS, `--trash-list` and `--trash-stats` inspect the current user's home Trash
and per-user Trash directories on mounted volumes without creating directories,
changing `.DS_Store`, or invoking Finder. Listing shows actual Trash paths; verbose
statistics show these paths too. Available recorded folders and names are labeled
**unverified**: stale Put Back records can belong to a previous file with the same
Trash name. Missing or unreadable records show an unknown original location and
do not hide items or fail the command. Recorded folders may be relative to a volume;
trache preserves them as recorded and does not infer deletion timestamps.

macOS statistics count top-level items and recursively sum logical sizes without
following symlinks or crossing into another filesystem. Hard links count once per
directory entry; totals are not estimates of reclaimable disk space. Root-level
`.DS_Store` and `.localized` are excluded, while other hidden payloads are included.
Results are a best-effort scan, not an atomic snapshot. Incomplete scans print
accessible results and explicit lower bounds, report affected paths, and exit
nonzero. Reading Trash may require Full Disk Access for the terminal or invoking app.

Restoration through `--trash-undo` and selective deletion through `--trash-purge`
remain unsupported on macOS. `--trash-empty` delegates to Finder; Finder's Put Back
is separate from trache's undo command.

On Windows, `--trash-stats` and `--trash-empty --trash-dry-run` report exact sizes for top-level
files. Directory sizes are not recursively measured yet, so groups containing directories or
metadata failures are reported as explicit `at least` lower bounds. Volume names in these reports
describe each item's original path; they are not physical `$Recycle.Bin` directory paths.
