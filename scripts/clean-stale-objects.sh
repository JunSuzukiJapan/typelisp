#!/usr/bin/env bash
# Removes the stale split-debuginfo object files cargo leaves behind in
# `target/*/deps`.
#
# WHY THIS EXISTS: on macOS the dev profile's default is
# `split-debuginfo = "unpacked"` — rustc leaves each codegen unit's `.o` next
# to the artifact and the linked binary keeps a debug map pointing at it, which
# is what gives a backtrace file/line numbers. Cargo never removes the ones
# belonging to *previous* builds, so `target/debug/deps` accumulates one set
# per compilation of every crate, for as long as the checkout lives.
#
# It is not a disk-space nuisance. Measured 2026-09-04 on this repository, with
# 1,774,287 `.o` files (80 GB) in `target/debug/deps`:
#
#   the same 51 MB test binary, `--list` (runs no test code at all)
#     from target/debug/deps/ ...... 21-40 s   (0.03 s CPU: the process sits
#                                               in an uninterruptible wait)
#     from target/debug/ ........... 0.03 s
#
# Every test binary in the suite is exec'd out of that directory, so the whole
# of `scripts/test-serial.sh` pays it once per binary. `editor_keyword_sync_test`
# looked like a 40-second test whose own six tests take 0.85 s.
#
# WHAT IT KEEPS: every `.o` younger than `--days` (2 by default), and every
# file that is not an `.o` at all, so the binaries built in the current session
# keep their debug maps. Older objects belong to artifacts that have long since
# been relinked; deleting them recompiles nothing — cargo's fingerprints do not
# track these files — and at worst an old binary still lying around loses the
# line numbers in its backtraces.
#
# SUPERSEDED EXECUTABLES GO TOO. Every test binary and every `[[bin]]` lands
# in `deps` as `<name>-<16 hex digits>`, and a change to the code or the
# features it was built with gives it a new hash, leaving the old one behind
# (well over 100 MB apiece for a test binary here; they were most of a 53 GB
# `target/debug/deps`). One is deleted, with its `.d`, when a newer one of the
# same name exists *and* it is older than `--days`. The age test is not for
# safety — cargo builds a missing one again — but for cost: two builds of one
# name can both be in use (a binary with and without `--features dev-tools`),
# and the one used yesterday should not have to be rebuilt tomorrow.
#
# HOW, AND WHY NOT `find -delete`: unlinking out of a directory this large is
# itself the slow operation (measured: ~11,000 deletions in the first three
# minutes, i.e. hours for the backlog). So the survivors are *hard-linked* into
# a fresh directory, which is then swapped into place — one rename, and the
# next `cargo test` is already fast. The old directory is deleted afterwards,
# at whatever pace the filesystem manages, with nothing waiting on it. That
# step is interruptible: a leftover `deps.stale.*` is picked up and finished by
# the next run.
#
# Not while a cargo or a test run is going: `deps` is swapped out from under
# it.
#
# Usage:
#   scripts/clean-stale-objects.sh               # keep the last 2 days
#   scripts/clean-stale-objects.sh --days 7
#   scripts/clean-stale-objects.sh --dry-run     # count, change nothing
#   scripts/clean-stale-objects.sh --leave-stale # swap, but don't spend the
#                                                # time deleting the old set
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

days=2
dry_run=0
leave_stale=0
while [ $# -gt 0 ]; do
    case "$1" in
        --days)
            days="${2:?--days needs a number}"
            shift 2
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        --leave-stale)
            leave_stale=1
            shift
            ;;
        -h | --help)
            sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//;$d'
            exit 0
            ;;
        *)
            echo "error: unknown argument: $1 (try --help)" >&2
            exit 2
            ;;
    esac
done

case "$days" in
    '' | *[!0-9]*)
        echo "error: --days takes a whole number of days, got: $days" >&2
        exit 2
        ;;
esac

if [ ! -d target ]; then
    echo "nothing to do: no target/ directory"
    exit 0
fi

# The two halves of one partition, spelled so that they cannot overlap or leave
# a gap: `find`'s `-mtime` compares whole days, so a file aged between `days`
# and `days + 1` matches neither `+days` nor `-days`. "Keep" is therefore the
# literal negation of "stale" rather than a second time test.
stale_test=(-type f -name '*.o' -mtime "+$days")

# Cargo's executables in `deps`: `<name>-<16 hex digits>`, no extension.
hex16='[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]'
executable_test=(-type f -perm -100 -name "*-$hex16" ! -name '*.*')

# `<mtime> <path>` for each file named: BSD stat (macOS) or GNU stat.
if stat -f '%m' . >/dev/null 2>&1; then
    mtime_of=(stat -f '%m %N')
else
    mtime_of=(stat -c '%Y %n')
fi

# The executables in $1 that a newer one of the same name supersedes and that
# are older than `--days`, one file name per line.
superseded() {
    (cd "$1" && {
        find . -maxdepth 1 "${executable_test[@]}" -exec "${mtime_of[@]}" {} + | sed 's|^\([0-9]*\) \./|all \1 |'
        find . -maxdepth 1 "${executable_test[@]}" -mtime "+$days" -print | sed 's|^\./|old |'
    }) | awk '
        function name_of(f) { sub(/-[0-9a-f]+$/, "", f); return f }
        $1 == "all" { mtime[$3] = $2; n = name_of($3); if ($2 > newest[n]) newest[n] = $2; next }
        $1 == "old" { old[$2] = 1 }
        END { for (f in old) if (mtime[f] < newest[name_of(f)]) print f }'
}

for deps in target/*/deps; do
    [ -d "$deps" ] || continue
    parent="$(dirname "$deps")"

    # An earlier run that was interrupted during its delete, finished now.
    for stale in "$parent"/deps.stale.*; do
        [ -d "$stale" ] || continue
        if [ "$dry_run" -eq 1 ]; then
            echo "$stale: left over from an earlier run, would be deleted"
        else
            echo "$stale: left over from an earlier run, deleting"
            rm -rf "$stale"
        fi
    done

    # The partition, as two lists of names: what goes, and everything else.
    # `LC_ALL=C` on both sorts so `comm` sees one order.
    gone="$(cd "$parent" && pwd)/deps.gone.$$"
    trap 'rm -f "$gone"' EXIT
    {
        (cd "$deps" && find . -maxdepth 1 "${stale_test[@]}" -print | sed 's|^\./||')
        superseded "$deps" | while read -r f; do
            echo "$f"
            if [ -e "$deps/$f.d" ]; then echo "$f.d"; fi
        done
    } | LC_ALL=C sort -u > "$gone"
    n_obj=$(grep -c '\.o$' "$gone" || true)
    n_exe=$(superseded "$deps" | wc -l | tr -d ' ')
    if [ "$n_obj" -eq 0 ] && [ "$n_exe" -eq 0 ]; then
        rm -f "$gone"
        echo "$deps: nothing older than $days day(s)"
        continue
    fi
    what="$n_obj object file(s) and $n_exe superseded executable(s)"
    keep_names() {
        (cd "$deps" && find . -maxdepth 1 -mindepth 1 -print | sed 's|^\./||') | LC_ALL=C sort | LC_ALL=C comm -23 - "$gone"
    }
    if [ "$dry_run" -eq 1 ]; then
        kept=$(keep_names | wc -l | tr -d ' ')
        size=$( (cd "$deps" && tr '\n' '\0' < "$gone" | xargs -0 du -ck) | tail -1 | awk '{ printf "%.1f GB", $1 / 1048576 }')
        rm -f "$gone"
        echo "$deps: would delete $what ($size), keeping $kept entr(ies)"
        continue
    fi

    keep_dir="$parent/deps.keep.$$"
    rm -rf "$keep_dir"
    mkdir "$keep_dir"
    keep_abs="$(cd "$keep_dir" && pwd)"
    # Hard links, so this costs one directory entry per survivor and no copying
    # — and so an interrupted run has changed nothing yet: `$deps` is still
    # whole until the rename below. `-J` batches several hundred names into one
    # `ln`, which is what keeps this to a few dozen processes. `-P` links a
    # symlink as a symlink: cargo puts none here, but the default (`-L`, link
    # what it points at) turns a dangling one into a failed run.
    if ! (keep_names | (cd "$deps" && while read -r f; do [ -d "$f" ] || echo "$f"; done) | tr '\n' '\0' |
        (cd "$deps" && xargs -0 -n 500 -J % ln -P -- % "$keep_abs/")); then
        rm -rf "$keep_dir" "$gone"
        echo "error: could not link the survivors; $deps left untouched" >&2
        exit 1
    fi
    # A directory cannot be hard-linked, so one is moved instead — the
    # temporary folders a rustc that was killed leaves behind (`rustcXXXXXX`).
    # Moved last, so an interruption before here still leaves `$deps` whole.
    keep_names | (cd "$deps" && while read -r f; do if [ -d "$f" ]; then mv -- "$f" "$keep_abs/"; fi; done)
    rm -f "$gone"

    stale_dir="$parent/deps.stale.$$"
    mv "$deps" "$stale_dir"
    mv "$keep_dir" "$deps"
    kept=$(find "$deps" -maxdepth 1 -mindepth 1 -print | wc -l | tr -d ' ')
    echo "$deps: $kept entr(ies) kept, $what moved to $stale_dir"

    if [ "$leave_stale" -eq 1 ]; then
        echo "  (left in place; the next run of this script deletes it)"
    else
        echo "  deleting $stale_dir — slow, and safe to interrupt"
        rm -rf "$stale_dir"
    fi
done
