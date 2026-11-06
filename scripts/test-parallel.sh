#!/usr/bin/env bash
# Runs the test suite several binaries at a time, each binary multi-threaded.
#
# The same target set as scripts/test-serial.sh (`--lib` plus every
# `tests/*.rs`), but through `xargs -P` instead of one at a time: binaries
# run concurrently, and each binary runs its own tests on libtest's default
# thread count. Safe because separate processes share no LLVM `Context`, and
# `COMPILE_LOCK` serializes whole compiles within one binary — see
# `src/compile/mod.rs`. `test-serial.sh` stays for when a failure needs a
# deterministic, unloaded re-run.
#
# Usage:
#   scripts/test-parallel.sh                # lib + every integration test
#   scripts/test-parallel.sh compile_test ffi_test
#   scripts/test-parallel.sh -j 4           # at most 4 binaries at once
#   scripts/test-parallel.sh --shard 2/4    # the second quarter of them
#   scripts/test-parallel.sh -- --test-threads=4
#
# `-j N` (`TEST_JOBS`, default 6) caps how many test binaries run at once.
# It does not cap the threads *within* a binary — that is libtest's own
# `--test-threads`, passed after a `--` like in `test-serial.sh`. Flags
# after `--` are forwarded to every `cargo test` invocation; they must not
# contain spaces.
#
# A shard is every target whose position in the list (lib first, then
# tests/*.rs in name order) is congruent to it modulo the count, so the shards
# of one count cover the list exactly once between them — the same rule
# `test-each.sh` uses. A shard cannot be combined with named targets.
#
# EACH TARGET GETS A TIMEOUT (`TEST_TIMEOUT`, default 3600s) and its own log
# under `target/test-parallel-logs/<target>.log`. Stdout carries one
# `RESULT <status> <target>` line per target as they finish, and a summary
# at the end; the script fails if any target did. A target with no result
# at all (its worker never reported back) counts as failed, not skipped.
#
# **If you pipe this script, pass `grep --line-buffered` (or `stdbuf -oL`).**
# grep buffers by block when its output is not a terminal, so a plain
# `scripts/test-parallel.sh | grep ...` hides which targets are done.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

logdir="target/test-parallel-logs"

# Worker mode: run one target, report back through its `.rc` file. Invoked
# by the parent below as `bash $0 --worker <target> [passthrough...]`.
if [ "${1:-}" = "--worker" ]; then
    target="$2"
    shift 2
    if [ "$target" = "--lib" ]; then
        key="lib"
    else
        key="$target"
    fi
    mkdir -p "$logdir"
    # Resolved by the parent and inherited; recomputed here so a worker
    # started by hand still refuses rather than running unbounded.
    timeout_bin="${TIMEOUT_BIN:-}"
    if [ -z "$timeout_bin" ]; then
        for candidate in timeout gtimeout; do
            if command -v "$candidate" >/dev/null 2>&1; then timeout_bin="$candidate"; break; fi
        done
    fi
    if [ -z "$timeout_bin" ]; then
        echo "RESULT 125 $target (no timeout on PATH)"
        echo "125" > "$logdir/$key.rc"
        exit 0
    fi
    : "${TEST_TIMEOUT:=3600}"
    if [ "$target" = "--lib" ]; then
        # See test-serial.sh for why `--lib` needs `--no-fail-fast`. The
        # trailing `--` separates cargo's own flags from libtest's.
        "$timeout_bin" -k 10 "$TEST_TIMEOUT" cargo test --lib --no-fail-fast -- "$@" > "$logdir/$key.log" 2>&1
    else
        "$timeout_bin" -k 10 "$TEST_TIMEOUT" cargo test --test "$target" -- "$@" > "$logdir/$key.log" 2>&1
    fi
    rc=$?
    echo "$rc" > "$logdir/$key.rc"
    echo "RESULT $rc $target"
    exit 0
fi

# Pin LLVM 22 even if .cargo/config.toml wasn't generated (see
# scripts/setup-cargo-env.sh) and the shell points LLVM_SYS_221_PREFIX
# elsewhere.
if command -v brew >/dev/null 2>&1; then
    prefix="$(brew --prefix llvm@22 2>/dev/null || true)"
    if [ -n "$prefix" ] && [ -x "$prefix/bin/llvm-config" ]; then
        export LLVM_SYS_221_PREFIX="$prefix"
    fi
fi

# No `RUST_MIN_STACK` here: see test-serial.sh for why the default is enough.

jobs="${TEST_JOBS:-6}"
targets=()
named=0
passthrough=()
shard_spec=""
seen_dashes=0
usage="usage: scripts/test-parallel.sh [-j N] [--shard <index>/<count>] [targets...] [-- cargo-test-flags...]"
while [ $# -gt 0 ]; do
    if [ "$seen_dashes" -eq 1 ]; then
        passthrough+=("$1")
    elif [ "$1" = "--" ]; then
        seen_dashes=1
    elif [ "$1" = "-j" ] || [ "$1" = "--jobs" ]; then
        [ $# -ge 2 ] || { echo "$usage" >&2; exit 2; }
        jobs="$2"
        shift
    elif [ "$1" = "--shard" ] || [[ "$1" == --shard=* ]]; then
        if [ "$1" = "--shard" ]; then
            [ $# -ge 2 ] || { echo "$usage" >&2; exit 2; }
            shard_spec="$2"
            shift
        else
            shard_spec="${1#--shard=}"
        fi
    elif [ "$1" = "-h" ] || [ "$1" = "--help" ]; then
        sed -n '2,/^set -uo pipefail$/p' "${BASH_SOURCE[0]}" | sed 's/^#//;s/^ //'
        exit 0
    else
        targets+=("$1")
        named=1
    fi
    shift
done

# Default target set: the lib unit tests plus every integration test in tests/.
if [ "${#targets[@]}" -eq 0 ]; then
    targets=("--lib")
    for f in tests/*.rs; do
        targets+=("$(basename "$f" .rs)")
    done
fi

# A shard of the default set (never of named targets): `test-each.sh`'s own
# rule, so the two scripts agree on what shard 2/4 is.
if [ -n "$shard_spec" ]; then
    if [ "$named" -eq 1 ]; then
        echo "error: --shard cannot be combined with named targets" >&2
        exit 2
    fi
    if ! [[ "$shard_spec" =~ ^([0-9]+)/([0-9]+)$ ]] || [ "${BASH_REMATCH[1]}" -lt 1 ] ||
        [ "${BASH_REMATCH[1]}" -gt "${BASH_REMATCH[2]}" ]; then
        echo "error: --shard takes <index>/<count> with 1 <= index <= count, got: $shard_spec" >&2
        exit 2
    fi
    shard="${BASH_REMATCH[1]}"
    count="${BASH_REMATCH[2]}"
    all=("${targets[@]}")
    targets=()
    for i in "${!all[@]}"; do
        if [ $((i % count + 1)) -eq "$shard" ]; then
            targets+=("${all[$i]}")
        fi
    done
    echo "shard $shard/$count: ${#targets[@]} of ${#all[@]} target(s)"
fi

# The AOT tests (`compile_file_test`) link `target/debug/libtypelisp_front.a`
# into every executable they build (`compile::aot::link_archive`), and
# `cargo test` does *not* produce it: it builds `typelisp-front`'s rlib, which is
# a different target from its staticlib. Without this the AOT tests silently
# link whatever `.a` was last left on disk, so a newly added `rt_*` shim fails
# with an undefined symbol while every JIT test passes.
# GNU coreutils' `timeout`, which macOS does not ship: Homebrew installs it as
# `timeout` when coreutils is linked with default names and as `gtimeout`
# otherwise. Resolved by name (never a hardcoded prefix) and **refused rather
# than skipped** if it is missing — running unbounded is the failure mode this
# exists to remove, so silently doing it anyway would be worse than stopping.
timeout_bin=""
for candidate in timeout gtimeout; do
    if command -v "$candidate" >/dev/null 2>&1; then timeout_bin="$candidate"; break; fi
done
if [ -z "$timeout_bin" ]; then
    echo "FAILED: no \`timeout\` on PATH (macOS does not ship one: \`brew install coreutils\`)."
    echo "        This script will not run the suite unbounded — see its header comment."
    exit 1
fi
export TIMEOUT_BIN="$timeout_bin"
: "${TEST_TIMEOUT:=3600}"
export TEST_TIMEOUT

echo "=== cargo build -p typelisp-front (staticlib for the AOT tests) ==="
cargo build -p typelisp-front || { echo "FAILED: building typelisp-front's staticlib"; exit 1; }

mkdir -p "$logdir"
# Fresh verdicts: without this, a target whose worker never ran would be
# judged by the `.rc` file a previous run left behind.
for t in "${targets[@]}"; do
    if [ "$t" = "--lib" ]; then key="lib"; else key="$t"; fi
    rm -f "$logdir/$key.rc"
done
# The passthrough flags, quoted once for the trip through xargs (which does
# its own quote processing, the way a shell would). They must not contain
# spaces — see the header comment.
quoted=""
if [ "${#passthrough[@]}" -gt 0 ]; then
    for arg in "${passthrough[@]}"; do
        quoted="$quoted $(printf %q "$arg")"
    done
fi
# shellcheck disable=SC2086
printf '%s\n' "${targets[@]}" | xargs -P "$jobs" -I{} bash "$0" --worker {} $quoted

echo
failed=()
timedout=()
missing=()
for t in "${targets[@]}"; do
    if [ "$t" = "--lib" ]; then key="lib"; else key="$t"; fi
    if [ ! -f "$logdir/$key.rc" ]; then
        missing+=("$t")
    else
        rc="$(cat "$logdir/$key.rc")"
        if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
            timedout+=("$t")
        elif [ "$rc" -ne 0 ]; then
            failed+=("$t")
        fi
    fi
done
if [ "${#timedout[@]}" -ne 0 ]; then
    echo "TIMEDOUT: ${timedout[*]}"
fi
if [ "${#missing[@]}" -ne 0 ]; then
    echo "NO RESULT: ${missing[*]}"
fi
if [ "${#failed[@]}" -ne 0 ]; then
    echo "FAILED: ${failed[*]}"
fi
if [ "${#timedout[@]}" -ne 0 ] || [ "${#missing[@]}" -ne 0 ] || [ "${#failed[@]}" -ne 0 ]; then
    echo "(per-target logs under $logdir/)"
    exit 1
fi
if [ -n "$shard_spec" ]; then
    echo "ALL ${#targets[@]} TARGET(S) PASSED (parallel, -j$jobs, shard $shard_spec)"
else
    echo "ALL TESTS PASSED (parallel, -j$jobs)"
fi
