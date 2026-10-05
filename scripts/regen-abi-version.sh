#!/usr/bin/env bash
# Writes a new ABI version when what compiled code assumes about the runtime
# archive has changed: docs/dev/api_version/history/api_<N>.md,
# latest_api_signature.md, and typelisp-abi's abi_version! literal. Prints
# that nothing was written when the description is unchanged. The
# abi_version_test test fails until this has been run.
#
#   scripts/regen-abi-version.sh          # only when the description changed
#   scripts/regen-abi-version.sh --bump   # a new version regardless, for a
#                                         # change the description cannot show
#
# Delegates to with-llvm-env.sh for the LLVM 22 toolchain prefix (never
# hardcode it — see that script).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
exec "$here/with-llvm-env.sh" cargo run --quiet --features dev-tools --bin typl-regen-abi-version -- "$@"
