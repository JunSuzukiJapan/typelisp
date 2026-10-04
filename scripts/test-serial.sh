#!/usr/bin/env bash
# Runs the whole test suite one binary at a time, each binary single-threaded.
#
# `cargo test` launches the compiled test executables in parallel, and several
# of them (compile_test, compile_file_test, ...) drive LLVM/JIT compilation
# against the one process-wide `LLVMContext` (`src/compile/mod.rs`). Concurrent
# LLVM use — whether across *processes* or across *threads within one test
# binary* — can crash with SIGSEGV: `COMPILE_LOCK` guards each individual LLVM
# builtin, but IR building and JIT execution still interleave on the shared
# Context across threads, which LLVM does not support. It's rare and
# load-dependent (a single binary passes on its own most of the time), but not
# zero, so this script both serializes across binaries *and* forces
# `--test-threads=1` within each for a reliable green signal.
#
# Usage:
#   scripts/test-serial.sh                # lib + every integration test
#   scripts/test-serial.sh compile_test   # only the named integration test(s)
#
# Any extra `cargo test` flags after a `--` are forwarded (and win over the
# default `--test-threads=1`, since libtest takes the last occurrence), e.g.
#   scripts/test-serial.sh -- --nocapture
#
# EACH TARGET GETS A TIMEOUT (`TEST_TIMEOUT`, default 3600s). Nothing else in
# this repo bounds a test's running time: libtest has no per-test timeout on
# stable, an evaluated (or compiled) Lisp loop has no fuel, and the executables
# `compile_file_test` builds are waited on with `Command::status()`. Without
# this, one runaway costs the whole run: on 2026-09-11 a malformed `loop` in a
# hand-written core-IR test span for **nine hours** at 100% CPU on the *first*
# target, and the run reported nothing at all — not even which file it was in.
# `COMPILE_LOCK` is the other way to hang here: it is not reentrant, so taking
# it where it is already held waits forever. A timed-out target is named in
# `TIMEDOUT:` at the end and is a failure, not a skip.
#
# 3600s is about twice the slowest targets measured on the slowest machines the
# suite runs on: compile_file_test, ~30 minutes in an x86_64 Linux VM (colima),
# where each of its tests links an executable against the debug
# `libtypelisp_front.a`, and compile_test, ~19 minutes on an Intel GitHub
# runner. On a recent Mac neither takes more than a few minutes. Raise it with
# `TEST_TIMEOUT=7200 scripts/...` on a slower machine rather than removing it.
#
# **If you pipe this script, pass `grep --line-buffered` (or `stdbuf -oL`).**
# grep buffers by block when its output is not a terminal, so a plain
# `scripts/test-serial.sh | grep ...` hides which target is running — which is
# how the nine hours went by unnoticed.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Pin LLVM 22 even if .cargo/config.toml wasn't generated (see
# scripts/setup-cargo-env.sh) and the shell points LLVM_SYS_221_PREFIX
# elsewhere.
if command -v brew >/dev/null 2>&1; then
    prefix="$(brew --prefix llvm@22 2>/dev/null || true)"
    if [ -n "$prefix" ] && [ -x "$prefix/bin/llvm-config" ]; then
        export LLVM_SYS_221_PREFIX="$prefix"
    fi
fi

# No `RUST_MIN_STACK` here: the default (~2MB per test thread) is enough.
#
# It was 32MB until 2026-09-08. Two things recursed per `if`-nesting level —
# `Checker::check_if` and the evaluator — and checking/running the self-hosted
# compiler's own deeply-nested dispatch functions (`compile-sexpr-field` and
# friends) sat close to the edge, so an unrelated, modest addition elsewhere in
# the binary could tip a single test over (that is what
# `compile_dispatches_bignum_comparisons_and_agrees_with_the_interpreter` hit in
# 2026-07-15's compiled-global work).
#
# The evaluator half is gone: it now runs on a continuation stack in the heap
# (`crates/typelisp-front/src/eval/interp/core_cps.rs`), so a Lisp recursion no
# longer costs Rust frames. Measured with the whole suite at the default stack:
# 117 files, 129 test results, all green. `Checker::check_if` still recurses,
# so if this class of failure comes back it is the checker's `if` handling that
# has to be loopified — widening the stack again would only move the edge.

# Split args into target names (before `--`) and passthrough flags (after).
targets=()
passthrough=()
seen_dashes=0
for arg in "$@"; do
    if [ "$seen_dashes" -eq 1 ]; then
        passthrough+=("$arg")
    elif [ "$arg" = "--" ]; then
        seen_dashes=1
    else
        targets+=("$arg")
    fi
done

# Default target set: the lib unit tests plus every integration test in tests/.
if [ "${#targets[@]}" -eq 0 ]; then
    targets=("--lib")
    for f in tests/*.rs; do
        targets+=("$(basename "$f" .rs)")
    done
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
: "${TEST_TIMEOUT:=3600}"

echo "=== cargo build -p typelisp-front (staticlib for the AOT tests) ==="
cargo build -p typelisp-front || { echo "FAILED: building typelisp-front's staticlib"; exit 1; }

failed=()
timedout=()
for t in "${targets[@]}"; do
    if [ "$t" = "--lib" ]; then
        echo "=== cargo test --lib ==="
        sel=(--lib)
    else
        echo "=== cargo test --test $t ==="
        sel=(--test "$t")
    fi
    # `--test-threads=1` first so a user-supplied one in `passthrough` (which
    # comes after) wins — libtest honors the last occurrence. The `if` guard
    # avoids expanding an empty `passthrough` array under `set -u` (an error on
    # the bash 3.2 macOS ships).
    #
    # `-k 10`: SIGTERM first, then SIGKILL ten seconds later. A test binary
    # spinning in a loop dies on the TERM, but the child processes an AOT test
    # spawned may not, and a target that is not dead is a target that still
    # holds cargo's build lock.
    if [ "${#passthrough[@]}" -gt 0 ]; then
        "$timeout_bin" -k 10 "$TEST_TIMEOUT" cargo test "${sel[@]}" -- --test-threads=1 "${passthrough[@]}"
    else
        "$timeout_bin" -k 10 "$TEST_TIMEOUT" cargo test "${sel[@]}" -- --test-threads=1
    fi
    rc=$?
    # 124 is `timeout`'s own "the command was still running"; 137 is a SIGKILL,
    # which after `-k` means it ignored the TERM as well.
    if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
        echo "=== TIMED OUT after ${TEST_TIMEOUT}s: $t ==="
        timedout+=("$t")
    elif [ "$rc" -ne 0 ]; then
        failed+=("$t")
    fi
done

echo
if [ "${#timedout[@]}" -ne 0 ]; then
    echo "TIMEDOUT: ${timedout[*]}"
fi
if [ "${#failed[@]}" -ne 0 ]; then
    echo "FAILED: ${failed[*]}"
fi
if [ "${#timedout[@]}" -ne 0 ] || [ "${#failed[@]}" -ne 0 ]; then
    exit 1
fi
echo "ALL TESTS PASSED (serial)"
