#!/bin/bash
# Compare rm vs trache behavior
# Run inside OrbStack Linux: orb -m trache-test bash systests/compare-rm.sh

set -e

TRACHE="/tmp/trache-build/release/trache"
TESTDIR="/tmp/trache-systest-$$"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

pass() { echo -e "${GREEN}PASS${NC}: $1"; }
fail() { echo -e "${RED}FAIL${NC}: $1"; exit 1; }
skip() { echo -e "${YELLOW}SKIP${NC}: $1"; }

setup() {
    rm -rf "$TESTDIR"
    mkdir -p "$TESTDIR"
    cd "$TESTDIR"
}

cleanup() {
    cd /
    rm -rf "$TESTDIR"
    # Empty trash for clean state
    "$TRACHE" --empty 2>/dev/null || true
}

trap cleanup EXIT

echo "=== trache vs rm comparison tests ==="
echo "Test dir: $TESTDIR"
echo ""

# Test 1: Basic file removal
setup
echo "Test 1: Basic file removal"
echo "hello" > file.txt
"$TRACHE" file.txt
[ ! -f file.txt ] && pass "file removed" || fail "file still exists"

# Test 2: Undo (restore from trash)
setup
echo "Test 2: Undo/restore"
echo "restore me" > restore.txt
"$TRACHE" restore.txt
"$TRACHE" --undo restore
[ -f restore.txt ] && pass "file restored" || fail "file not restored"
[ "$(cat restore.txt)" = "restore me" ] && pass "content intact" || fail "content changed"

# Test 3: -f on nonexistent (should exit 0)
setup
echo "Test 3: -f on nonexistent"
"$TRACHE" -f nonexistent.txt && pass "exit 0 with -f" || fail "exit non-zero with -f"
rm -f nonexistent.txt && pass "rm also exits 0" || fail "rm exits non-zero"

# Test 4: No -d/-r on directory
setup
echo "Test 4: Directory without -d/-r"
mkdir testdir
"$TRACHE" testdir 2>/dev/null && fail "should have failed" || pass "correctly rejected"
rm testdir 2>/dev/null && fail "rm should have failed" || pass "rm also rejected"
rmdir testdir

# Test 5: -d on empty directory
setup
echo "Test 5: -d on empty directory"
mkdir emptydir
"$TRACHE" -d emptydir
[ ! -d emptydir ] && pass "empty dir removed" || fail "empty dir still exists"

# Test 6: -d on non-empty directory (should fail)
setup
echo "Test 6: -d on non-empty directory"
mkdir nonempty
touch nonempty/file
"$TRACHE" -d nonempty 2>/dev/null && fail "should have failed" || pass "correctly rejected"
rm -r nonempty

# Test 7: -r recursive
setup
echo "Test 7: -r recursive"
mkdir -p deep/nested/dir
echo "file" > deep/nested/dir/file.txt
"$TRACHE" -r deep
[ ! -d deep ] && pass "recursive removed" || fail "recursive still exists"

# Test 8: Reject . and ..
setup
echo "Test 8: Reject . and .."
"$TRACHE" -r . 2>/dev/null && fail "should reject ." || pass "rejected ."
"$TRACHE" -r .. 2>/dev/null && fail "should reject .." || pass "rejected .."

# Test 9: -v verbose
setup
echo "Test 9: -v verbose"
echo "test" > verbose.txt
output=$("$TRACHE" -v verbose.txt 2>&1)
echo "$output" | grep -q "verbose.txt" && pass "verbose shows filename" || fail "verbose silent"

# Test 10: -- separator for files starting with -
setup
echo "Test 10: -- separator"
echo "dash" > "./-dashfile.txt"
"$TRACHE" -- -dashfile.txt
[ ! -f "-dashfile.txt" ] && pass "dash file removed" || fail "dash file still exists"

# Test 11: Multiple files
setup
echo "Test 11: Multiple files"
touch a.txt b.txt c.txt
"$TRACHE" a.txt b.txt c.txt
[ ! -f a.txt ] && [ ! -f b.txt ] && [ ! -f c.txt ] && pass "all removed" || fail "some remain"

# Test 12: List trash
setup
echo "Test 12: List trash"
echo "listme" > listme.txt
"$TRACHE" listme.txt
"$TRACHE" --list | grep -q "listme" && pass "shows in list" || fail "not in list"

# Test 13: Purge from trash
setup
echo "Test 13: Purge specific item"
echo "purgeme" > purgeme.txt
"$TRACHE" purgeme.txt
"$TRACHE" --purge purgeme
"$TRACHE" --list | grep -q "purgeme" && fail "still in trash" || pass "purged"

# Test 14: Empty trash
setup
echo "Test 14: Empty trash"
touch trash1.txt trash2.txt
"$TRACHE" trash1.txt trash2.txt
"$TRACHE" --empty
count=$("$TRACHE" --list 2>&1 | grep -c "trash" || echo "0")
[ "$count" = "0" ] || "$TRACHE" --list | grep -q "empty" && pass "trash emptied" || fail "trash not empty"

echo ""
echo "=== All tests passed ==="
