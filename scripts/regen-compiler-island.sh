#!/usr/bin/env bash
# Regenerates the committed compiler-island AOT artifact
# (src/compiler_island.bc) from compiler.rs's SOURCE. Run this whenever you
# edit that SOURCE; the `island_artifacts_are_fresh` test fails until the
# artifact matches. Interp-closure removal Stage 3.
#
# Also run it after changing an `llvm-*` builder (eval_llvm_builtin_method):
# that changes the emitted IR without changing SOURCE, and no test detects it
# — the freshness hash covers SOURCE only.
#
#   scripts/regen-compiler-island.sh
#
# Delegates to with-llvm-env.sh for the LLVM 17 toolchain prefix (never
# hardcode it — see that script). The bootstrap binary resolves the output
# paths from CARGO_MANIFEST_DIR itself, so no paths are passed here.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
exec "$here/with-llvm-env.sh" cargo run --quiet --bin typl-bootstrap-island
