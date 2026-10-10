#!/bin/sh
# Run inside the matching FreeBSD VM after the host packages the release archive.
set -eu

target=$1
version=$2
test "$(uname -s)" = FreeBSD
directory=$(mktemp -d)
trap 'rm -rf "$directory"' 0
tar -xzf "dist/trache-v$version-$target.tar.gz" -C "$directory"
binary="$directory/trache-v$version-$target/trache"
test "$("$binary" --version)" = "trache $version"
"$binary" --help

# Keep the entire trash/restore/purge exercise in this temporary data directory.
export XDG_DATA_HOME="$directory/data"
mkdir -p "$XDG_DATA_HOME" "$directory/work"
file="$directory/work/sample.txt"
pattern="full:path:string:$file"
printf 'original contents\n' > "$directory/expected"
cp "$directory/expected" "$file"
"$binary" --trash-dry-run "$file"
cmp "$directory/expected" "$file"
"$binary" "$file"
test ! -e "$file"
"$binary" --trash-list | grep -F sample.txt
"$binary" --trash-undo "$pattern"
cmp "$directory/expected" "$file"
"$binary" "$file"
"$binary" --trash-purge "$pattern"
test ! -e "$file"
test -z "$(find "$XDG_DATA_HOME/Trash/files" -type f)"
echo "FreeBSD $target archive smoke checks passed"
