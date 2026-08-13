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
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Pin LLVM 17 even if .cargo/config.toml wasn't generated (see
# scripts/setup-cargo-env.sh) and the shell points LLVM_SYS_170_PREFIX
# elsewhere.
if command -v brew >/dev/null 2>&1; then
    prefix="$(brew --prefix llvm@17 2>/dev/null || true)"
    if [ -n "$prefix" ] && [ -x "$prefix/bin/llvm-config" ]; then
        export LLVM_SYS_170_PREFIX="$prefix"
    fi
fi

# `Checker::check_if`/`Interp::eval`'s `if`-chain handling recurses per
# nesting level (never loopified the way `compile-value`'s own dispatch was,
# per the "interp if chain stack overflow" note in
# docs/dev/implementation-log.md) — checking/running the self-hosted
# compiler's own deeply-nested dispatch functions (`compile-sexpr-field` and
# friends) against the default ~2MB test-thread stack is already close to
# the edge, so an unrelated, modest addition elsewhere in the binary
# (observed: a handful of new top-level functions in `src/eval/interp.rs`,
# with no new recursion of their own) can tip a specific test over —
# `compile_dispatches_bignum_comparisons_and_agrees_with_the_interpreter`
# hit exactly this in 2026-07-15's compiled-global work. Not a correctness
# bug in the tipping change itself; widening every test thread's stack is
# the same trade-off `docs/dev/implementation-log.md` already accepts for
# this whole class of issue, cheaper than loopifying the checker's `if`
# handling for a one-test margin.
export RUST_MIN_STACK=$((32 * 1024 * 1024))

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

# The AOT tests (`compile_file_test`) link `target/<profile>/libtypelisp_rt.a`
# into every executable they build (`compile::aot::staticlib_path`), and
# `cargo test` does *not* produce it: it builds `typelisp-rt`'s rlib, which is
# a different target from its staticlib. Without this the AOT tests silently
# link whatever `.a` was last left on disk, so a newly added `rt_*` shim fails
# with an undefined symbol while every JIT test passes.
echo "=== cargo build -p typelisp-rt (staticlib for the AOT tests) ==="
cargo build -p typelisp-rt || { echo "FAILED: building typelisp-rt's staticlib"; exit 1; }

failed=()
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
    if [ "${#passthrough[@]}" -gt 0 ]; then
        cargo test "${sel[@]}" -- --test-threads=1 "${passthrough[@]}" || failed+=("$t")
    else
        cargo test "${sel[@]}" -- --test-threads=1 || failed+=("$t")
    fi
done

echo
if [ "${#failed[@]}" -ne 0 ]; then
    echo "FAILED: ${failed[*]}"
    exit 1
fi
echo "ALL TESTS PASSED (serial)"
