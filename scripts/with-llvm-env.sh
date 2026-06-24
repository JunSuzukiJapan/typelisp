#!/usr/bin/env bash
# Resolves the LLVM 17 toolchain prefix at run time via `brew --prefix` —
# never hardcode this path, Homebrew's install prefix differs across
# machines (Intel vs Apple Silicon, Linuxbrew, ...). Used as:
#   scripts/with-llvm-env.sh cargo build
set -euo pipefail

if ! command -v brew >/dev/null 2>&1; then
    echo "error: brew not found; install LLVM 17 and set LLVM_SYS_170_PREFIX yourself" >&2
    exit 1
fi

export LLVM_SYS_170_PREFIX="$(brew --prefix llvm@17)"
exec "$@"
