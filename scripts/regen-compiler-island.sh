#!/usr/bin/env bash
# Regenerates the committed compiler-island AOT artifact
# (src/compiler_island.bc) from compiler.rs's SOURCE. Run this whenever you
# edit that SOURCE; the `island_artifacts_are_fresh` test fails until the
# artifact matches. Interp-closure removal Stage 3.
#
# Also run it after changing an `llvm-*` builder (eval_llvm_builtin_method):
# that changes the emitted IR without changing SOURCE. The freshness hash
# covers SOURCE only and won't notice, but
# `the_committed_island_matches_a_fresh_build` compares the built bytes.
#
# RUN IT TWICE if the change altered what the island *emits* — a compile-*
# function's own IR output, not just its bookkeeping. The build compiles the
# new SOURCE with the *previously committed* island, so one pass produces code
# in the old shape; a second pass, now driven by the new island, converges.
# `the_committed_island_matches_a_fresh_build` is what tells you: if it fails
# right after a regen, run this again rather than hunting for the difference.
# See src/compile/bootstrap.rs's module doc for the worked example.
#
#   scripts/regen-compiler-island.sh
#
# Delegates to with-llvm-env.sh for the LLVM 17 toolchain prefix (never
# hardcode it — see that script). The bootstrap binary resolves the output
# paths from CARGO_MANIFEST_DIR itself, so no paths are passed here.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
exec "$here/with-llvm-env.sh" cargo run --quiet --bin typl-bootstrap-island
