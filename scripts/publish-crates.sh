#!/usr/bin/env bash
# Publishes the workspace's crates to crates.io, the ones others depend on
# first (docs/dev/development.md, "リリースの手順" 5.). `cargo publish` waits
# for each crate to appear in the index before it returns, so the next one
# finds it. A crate whose version is already on crates.io is skipped, which
# makes a run that stopped half way restartable as it is.
#
#   scripts/publish-crates.sh --dry-run   # only typelisp-mem can be packaged
#                                         # before it is on crates.io
#   scripts/publish-crates.sh
#
# Credentials are cargo's own (`cargo login`).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

dry_run=()
case "${1:-}" in
    "") ;;
    --dry-run) dry_run=(--dry-run) ;;
    *) echo "usage: scripts/publish-crates.sh [--dry-run]" >&2; exit 2 ;;
esac

crates=(typelisp-mem typelisp-abi typelisp-print typelisp-read typelisp-rt typelisp-front typelisp)

version="$(awk '/^\[workspace\.package\]/ { s = 1; next } /^\[/ { s = 0 } s && $1 == "version" { gsub(/"/, "", $3); print $3; exit }' Cargo.toml)"
[ -n "$version" ] || { echo "error: no version in [workspace.package]" >&2; exit 1; }

git rev-parse -q --verify "refs/tags/v$version" >/dev/null ||
    { echo "error: tag v$version does not exist; release first" >&2; exit 1; }
git diff --quiet HEAD || { echo "error: uncommitted changes" >&2; exit 1; }

# 200 is published, 404 is not; anything else (rate limit, outage) stops here.
on_crates_io() {
    local code
    code="$(curl -s -o /dev/null -w '%{http_code}' \
        -A "typelisp publish-crates.sh (https://github.com/JunSuzukiJapan/typelisp)" \
        "https://crates.io/api/v1/crates/$1/$version")"
    case "$code" in
        200) return 0 ;;
        404) return 1 ;;
        *) echo "error: crates.io answered $code for $1 $version" >&2; exit 1 ;;
    esac
}

for crate in "${crates[@]}"; do
    if on_crates_io "$crate"; then
        echo "== $crate $version: already on crates.io, skipped"
        continue
    fi
    echo "== $crate $version: cargo publish ${dry_run[*]:-}"
    scripts/with-llvm-env.sh cargo publish -p "$crate" ${dry_run[@]+"${dry_run[@]}"}
    # The crates after it cannot be packaged until this one is on crates.io.
    if [ ${#dry_run[@]} -gt 0 ]; then
        echo "dry run: stopped after $crate; the rest need it on crates.io"
        break
    fi
done
