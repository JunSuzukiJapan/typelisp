#!/usr/bin/env bash
# Runs the whole test suite one binary at a time.
#
# `cargo test` launches the compiled test executables in parallel, and several
# of them (compile_test, compile_file_test, ...) drive LLVM/JIT compilation.
# Multiple LLVM-using *processes* running at once can crash with SIGSEGV — a
# known artifact of concurrent LLVM use, not a real test failure. Each binary
# on its own is stable (its in-process parallel threads are fine), so this
# script serializes across binaries to get a reliable green signal.
#
# Usage:
#   scripts/test-serial.sh                # lib + every integration test
#   scripts/test-serial.sh compile_test   # only the named integration test(s)
#
# Any extra `cargo test` flags after a `--` are forwarded, e.g.
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

failed=()
for t in "${targets[@]}"; do
    if [ "$t" = "--lib" ]; then
        echo "=== cargo test --lib ==="
        sel=(--lib)
    else
        echo "=== cargo test --test $t ==="
        sel=(--test "$t")
    fi
    if [ "${#passthrough[@]}" -gt 0 ]; then
        cargo test "${sel[@]}" -- "${passthrough[@]}" || failed+=("$t")
    else
        cargo test "${sel[@]}" || failed+=("$t")
    fi
done

echo
if [ "${#failed[@]}" -ne 0 ]; then
    echo "FAILED: ${failed[*]}"
    exit 1
fi
echo "ALL TESTS PASSED (serial)"
