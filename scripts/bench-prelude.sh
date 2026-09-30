#!/usr/bin/env bash
# Measures the precompiled prelude's effect: the same workloads run against a
# prelude with its native bodies installed and against a purely interpreted
# one, in a single process. See src/bin/bench_prelude.rs.
#
# Release build, deliberately: a debug build's interpreter overhead swamps the
# difference this is trying to show, and nobody ships debug.
#
#   scripts/bench-prelude.sh
#
# Delegates to with-llvm-env.sh for the LLVM 22 toolchain prefix (never
# hardcode it — see that script).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
exec "$here/with-llvm-env.sh" cargo run --quiet --release --bin typl-bench-prelude
