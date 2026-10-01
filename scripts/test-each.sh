#!/usr/bin/env bash
# Runs the test targets one at a time through scripts/test-serial.sh, deleting
# each target's test binary once it has run. For machines with less disk than
# the whole suite needs: linked, the test binaries come to more than 15 GB,
# and this keeps one of them at a time.
#
# Usage:
#   scripts/test-each.sh                # lib + every integration test
#   scripts/test-each.sh --shard 2/4    # the second quarter of them
#
# A shard is every target whose position in the list (lib first, then
# tests/*.rs in name order) is congruent to it modulo the count, so the shards
# of one count cover the list exactly once between them.
#
# The verdict is each target's exit status, not its output: a test binary that
# dies on a signal (SIGSEGV) prints no `test result:` line, so a count of
# failed result lines misses it. One line per target, `RESULT <status>
# <target>`, and a summary at the end; the script fails if any target did.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

usage="usage: scripts/test-each.sh [--shard <index>/<count>]"
shard=1
count=1
while [ $# -gt 0 ]; do
    case "$1" in
        --shard)
            [ $# -ge 2 ] || { echo "$usage" >&2; exit 2; }
            spec="$2"
            shift 2
            ;;
        --shard=*)
            spec="${1#--shard=}"
            shift
            ;;
        *)
            echo "$usage" >&2
            exit 2
            ;;
    esac
    if ! [[ "$spec" =~ ^([0-9]+)/([0-9]+)$ ]] || [ "${BASH_REMATCH[1]}" -lt 1 ] ||
        [ "${BASH_REMATCH[1]}" -gt "${BASH_REMATCH[2]}" ]; then
        echo "error: --shard takes <index>/<count> with 1 <= index <= count, got: $spec" >&2
        exit 2
    fi
    shard="${BASH_REMATCH[1]}"
    count="${BASH_REMATCH[2]}"
done

all=(--lib)
for f in tests/*.rs; do
    all+=("$(basename "$f" .rs)")
done
targets=()
for i in "${!all[@]}"; do
    if [ $((i % count + 1)) -eq "$shard" ]; then
        targets+=("${all[$i]}")
    fi
done
echo "shard $shard/$count: ${#targets[@]} of ${#all[@]} target(s)"

failed=()
for t in "${targets[@]}"; do
    scripts/test-serial.sh "$t"
    rc=$?
    echo "RESULT $rc $t"
    if [ "$rc" -ne 0 ]; then
        failed+=("$t")
    fi
    # `<name>-<16 hex digits>` is how cargo names a test binary in deps; the
    # lib's is the crate's own name and other targets still need it.
    if [ "$t" != --lib ]; then
        rm -f target/debug/deps/"$t"-[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]
    fi
done

echo
if [ "${#failed[@]}" -ne 0 ]; then
    echo "FAILED: ${failed[*]}"
    exit 1
fi
echo "ALL ${#targets[@]} TARGET(S) PASSED (shard $shard/$count)"
