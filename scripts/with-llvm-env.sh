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

# macOS: one minimum OS version for rustc, the `cc` crate (ring) and
# `compile-file`'s link, the one the toolchain's std was built for — see
# build.rs and scripts/macos-deployment-target.sh. A value already exported
# is the caller's choice and is kept.
if [ "$(uname -s)" = Darwin ]; then
    if [ -n "${MACOSX_DEPLOYMENT_TARGET:-}" ]; then
        if ! [[ "$MACOSX_DEPLOYMENT_TARGET" =~ ^[0-9]+(\.[0-9]+){0,2}$ ]]; then
            echo "error: MACOSX_DEPLOYMENT_TARGET \`$MACOSX_DEPLOYMENT_TARGET\` is not a version like 15.0" >&2
            exit 1
        fi
    else
        MACOSX_DEPLOYMENT_TARGET="$("$(dirname "${BASH_SOURCE[0]}")/macos-deployment-target.sh")"
    fi
    export MACOSX_DEPLOYMENT_TARGET
fi
exec "$@"
