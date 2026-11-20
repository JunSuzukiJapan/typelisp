#!/usr/bin/env bash
# Builds what `typl -c --target wasm32-wasip1` links a module with, into
# <target dir>/wasm-runtime/<profile>/wasm32-wasip1/:
#
#   libtypelisp_front.a  the runtime, built for wasm32-wasip1
#   crt1-command.o       WASI's start file  } from the Rust toolchain that
#   libc.a               WASI's C library   } built the runtime
#   wasm-ld              that toolchain's linker (rust-lld)
#
#   scripts/build-wasm-runtime.sh            # the debug profile
#   scripts/build-wasm-runtime.sh release    # the release profile
#
# Needs a rustup toolchain with the wasm32-wasip1 target
# (`rustup target add wasm32-wasip1`): $TYPELISP_WASM_TOOLCHAIN, or rustup's
# default one when that is not set. A Rust that is not rustup's (a package
# manager's) has no wasm target to add, so the toolchain is asked of rustup
# rather than taken from PATH.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
profile="${1:-debug}"
case "$profile" in
  debug) profile_flag=() ;;
  release) profile_flag=(--release) ;;
  *) echo "build-wasm-runtime.sh: unknown profile '$profile' (debug or release)" >&2; exit 1 ;;
esac
target=wasm32-wasip1

if [ -n "${TYPELISP_WASM_TOOLCHAIN:-}" ]; then
  toolchain_flag=(--toolchain "$TYPELISP_WASM_TOOLCHAIN")
else
  toolchain_flag=()
fi
cargo_bin="$(rustup which ${toolchain_flag[@]+"${toolchain_flag[@]}"} cargo)"
bin_dir="$(dirname "$cargo_bin")"
sysroot="$("$bin_dir/rustc" --print sysroot)"
host="$("$bin_dir/rustc" -vV | sed -n 's/^host: //p')"
self_contained="$sysroot/lib/rustlib/$target/lib/self-contained"
lld="$sysroot/lib/rustlib/$host/bin/rust-lld"
if [ ! -d "$self_contained" ]; then
  echo "build-wasm-runtime.sh: the toolchain at $sysroot has no $target target; run: rustup target add $target" >&2
  exit 1
fi
if [ ! -x "$lld" ]; then
  echo "build-wasm-runtime.sh: the toolchain at $sysroot has no rust-lld at $lld" >&2
  exit 1
fi

target_dir="${CARGO_TARGET_DIR:-$root/target}"
build_dir="$target_dir/wasm"
out="$target_dir/wasm-runtime/$profile/$target"

# rustup's own cargo and rustc, ahead of any other on PATH.
(cd "$root" && PATH="$bin_dir:$PATH" CARGO_TARGET_DIR="$build_dir" \
  "$cargo_bin" build -p typelisp-front --target "$target" ${profile_flag[@]+"${profile_flag[@]}"})

mkdir -p "$out"
cp "$build_dir/$target/$profile/libtypelisp_front.a" "$out/libtypelisp_front.a"
cp "$self_contained/crt1-command.o" "$self_contained/libc.a" "$out/"
# lld picks its flavour from the name it is run under.
ln -sf "$lld" "$out/wasm-ld"
echo "build-wasm-runtime.sh: $out"
