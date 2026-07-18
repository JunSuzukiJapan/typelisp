#!/usr/bin/env bash
# Regenerates the committed compiler-island AOT artifacts
# (src/compiler_island.bc + src/compiler_island.fasl) from compiler.rs's
# SOURCE. Run this whenever you edit that SOURCE; the
# `island_artifacts_are_fresh` test fails until the artifacts match.
# Interp-closure removal Stage 3.
#
#   scripts/regen-compiler-island.sh
#
# Delegates to with-llvm-env.sh for the LLVM 17 toolchain prefix (never
# hardcode it — see that script). The bootstrap binary resolves the output
# paths from CARGO_MANIFEST_DIR itself, so no paths are passed here.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
exec "$here/with-llvm-env.sh" cargo run --quiet --bin typl-bootstrap-island
