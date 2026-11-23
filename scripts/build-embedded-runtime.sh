#!/usr/bin/env bash
# Builds what `typl -c` will link an executable for a board without an
# operating system with, into <target dir>/embedded-runtime/<profile>/<triple>/:
#
#   libtypelisp_rt_embedded.a  the runtime (crates/typelisp-rt-embedded), built
#                              for the triple; it leaves only the board's
#                              functions (`typelisp_host_*`) undefined
#
#   scripts/build-embedded-runtime.sh            # the debug profile
#   scripts/build-embedded-runtime.sh release    # the release profile
#
# Needs a rustup toolchain with the thumbv7em-none-eabihf target
# (`rustup target add thumbv7em-none-eabihf`): $TYPELISP_EMBEDDED_TOOLCHAIN, or
# rustup's default one when that is not set. A Rust that is not rustup's (a
# package manager's) has no such target to add, so the toolchain is asked of
# rustup rather than taken from PATH.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
profile="${1:-debug}"
case "$profile" in
  debug) profile_flag=() ;;
  release) profile_flag=(--release) ;;
  *) echo "build-embedded-runtime.sh: unknown profile '$profile' (debug or release)" >&2; exit 1 ;;
esac

if [ -n "${TYPELISP_EMBEDDED_TOOLCHAIN:-}" ]; then
  toolchain_flag=(--toolchain "$TYPELISP_EMBEDDED_TOOLCHAIN")
else
  toolchain_flag=()
fi
cargo_bin="$(rustup which ${toolchain_flag[@]+"${toolchain_flag[@]}"} cargo)"
bin_dir="$(dirname "$cargo_bin")"
sysroot="$("$bin_dir/rustc" --print sysroot)"

target_dir="${CARGO_TARGET_DIR:-$root/target}"
build_dir="$target_dir/embedded"

for target in thumbv7em-none-eabihf; do
  if [ ! -d "$sysroot/lib/rustlib/$target" ]; then
    echo "build-embedded-runtime.sh: the toolchain at $sysroot has no $target target; run: rustup target add $target" >&2
    exit 1
  fi
  # rustup's own cargo and rustc, ahead of any other on PATH. Built by
  # itself (`-p`), so that no other member of the workspace turns on the
  # runtime crates' `std`.
  (cd "$root" && PATH="$bin_dir:$PATH" CARGO_TARGET_DIR="$build_dir" \
    "$cargo_bin" build -p typelisp-rt-embedded --target "$target" ${profile_flag[@]+"${profile_flag[@]}"})

  out="$target_dir/embedded-runtime/$profile/$target"
  mkdir -p "$out"
  cp "$build_dir/$target/$profile/libtypelisp_rt_embedded.a" "$out/libtypelisp_rt_embedded.a"
  echo "build-embedded-runtime.sh: $out"
done
