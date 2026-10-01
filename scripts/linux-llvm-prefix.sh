#!/usr/bin/env bash
# Prints the prefix of the LLVM 22 installed by the Linux distribution's
# package manager — what LLVM_SYS_221_PREFIX names. The macOS counterpart is
# `brew --prefix llvm@22`, used by setup-cargo-env.sh and with-llvm-env.sh.
#
# Debian and Ubuntu (apt.llvm.org) name the tool `llvm-config-22`; Fedora and
# Arch ship one LLVM and call it `llvm-config`. Either is taken only if it
# reports version 22; anything else is an error, not a guess.
set -euo pipefail

if [ "$(uname -s)" != Linux ]; then
    echo "error: this script is for Linux; this is $(uname -s)" >&2
    exit 1
fi

for tool in llvm-config-22 llvm-config; do
    command -v "$tool" >/dev/null 2>&1 || continue
    version="$("$tool" --version)"
    case "$version" in
        22.*) "$tool" --prefix; exit 0 ;;
        *) echo "error: $tool is LLVM $version, not 22" >&2; exit 1 ;;
    esac
done
echo "error: no llvm-config-22 or llvm-config on PATH; install LLVM 22 (and its development files)" >&2
exit 1
