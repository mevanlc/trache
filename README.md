# trache - rm-compatible CLI trash and recycle bin access

`trache` (træʃ, like cache) is a fully compatible drop-in replacement for `rm` that sends things to Recycle Bin (Windows) or Trash (macOS/Linux/BSD).

## Motivation
Because...
* ...I can't stop automatically typing `rm -rf` (and other classic `rm` flags) but none of the existing trash CLIs accept rm-compatible flags.
* ...has `rm -W` ever worked for anyone? Probably not?
* ...the only advice the `rm` man-page gives you about data recovery is that "you might be able to recover the data after using `rm` on it, so consider using `shred` instead."

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

Trash restoration is unsupported on macOS. PRs welcome.
