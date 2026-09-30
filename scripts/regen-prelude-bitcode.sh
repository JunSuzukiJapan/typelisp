#!/usr/bin/env bash
# Regenerates the committed prelude dump
# (crates/typelisp-front/src/prelude.typld: checked state + bitcode) from
# prelude.rs's SOURCE. Run this whenever you edit that SOURCE; the `prelude_artifacts_are_fresh` test fails until the
# artifact matches.
#
# ORDER MATTERS: the prelude is compiled *by* the compiler island, so a
# regenerated island produces a different prelude artifact. Whenever both need
# regenerating, run the island first:
#
#   scripts/regen-compiler-island.sh
#   scripts/regen-prelude-bitcode.sh
#
# (The reverse dependency does not exist: the island's own generator loads the
# prelude interpreted, so it never reads this artifact.)
#
# Also run this after changing an `llvm-*` builder (eval_llvm_builtin_method)
# or the compile bridge: those change the emitted IR without changing SOURCE.
# The freshness hash covers SOURCE only and won't notice;
# `the_committed_prelude_matches_a_fresh_build` compares the built bytes.
#
# Delegates to with-llvm-env.sh for the LLVM 22 toolchain prefix (never
# hardcode it — see that script). The bootstrap binary resolves the output
# path from CARGO_MANIFEST_DIR itself, so no paths are passed here.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
exec "$here/with-llvm-env.sh" cargo run --quiet --bin typl-bootstrap-prelude
