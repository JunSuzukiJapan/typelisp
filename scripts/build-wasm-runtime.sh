#!/usr/bin/env bash
# Builds what `typl -c --target <triple>` links a WebAssembly module with, for
# both triples, into <target dir>/wasm-runtime/<profile>/<triple>/:
#
#   libtypelisp_front.a  the runtime, built for the triple
#   wasm-ld              the linker (rust-lld) of the Rust toolchain that built it
#   crt1-command.o       wasm32-wasip1 only: WASI's start file  } from that
#   libc.a               wasm32-wasip1 only: WASI's C library   } toolchain
#
#   scripts/build-wasm-runtime.sh            # the debug profile
#   scripts/build-wasm-runtime.sh release    # the release profile
#
# Needs a rustup toolchain with the wasm32-wasip1 and wasm32-unknown-unknown
# targets (`rustup target add wasm32-wasip1 wasm32-unknown-unknown`):
# $TYPELISP_WASM_TOOLCHAIN, or rustup's default one when that is not set. A
# Rust that is not rustup's (a package manager's) has no wasm target to add,
# so the toolchain is asked of rustup rather than taken from PATH.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
profile="${1:-debug}"
case "$profile" in
  debug) profile_flag=() ;;
  release) profile_flag=(--release) ;;
  *) echo "build-wasm-runtime.sh: unknown profile '$profile' (debug or release)" >&2; exit 1 ;;
esac

if [ -n "${TYPELISP_WASM_TOOLCHAIN:-}" ]; then
  toolchain_flag=(--toolchain "$TYPELISP_WASM_TOOLCHAIN")
else
  toolchain_flag=()
fi
cargo_bin="$(rustup which ${toolchain_flag[@]+"${toolchain_flag[@]}"} cargo)"
bin_dir="$(dirname "$cargo_bin")"
sysroot="$("$bin_dir/rustc" --print sysroot)"
host="$("$bin_dir/rustc" -vV | sed -n 's/^host: //p')"
lld="$sysroot/lib/rustlib/$host/bin/rust-lld"
if [ ! -x "$lld" ]; then
  echo "build-wasm-runtime.sh: the toolchain at $sysroot has no rust-lld at $lld" >&2
  exit 1
fi

target_dir="${CARGO_TARGET_DIR:-$root/target}"
build_dir="$target_dir/wasm"

for target in wasm32-wasip1 wasm32-unknown-unknown; do
  if [ ! -d "$sysroot/lib/rustlib/$target" ]; then
    echo "build-wasm-runtime.sh: the toolchain at $sysroot has no $target target; run: rustup target add $target" >&2
    exit 1
  fi
  # rustup's own cargo and rustc, ahead of any other on PATH.
  (cd "$root" && PATH="$bin_dir:$PATH" CARGO_TARGET_DIR="$build_dir" \
    "$cargo_bin" build -p typelisp-front --target "$target" ${profile_flag[@]+"${profile_flag[@]}"})

  out="$target_dir/wasm-runtime/$profile/$target"
  mkdir -p "$out"
  cp "$build_dir/$target/$profile/libtypelisp_front.a" "$out/libtypelisp_front.a"
  if [ "$target" = wasm32-wasip1 ]; then
    self_contained="$sysroot/lib/rustlib/$target/lib/self-contained"
    cp "$self_contained/crt1-command.o" "$self_contained/libc.a" "$out/"
  fi
  # lld picks its flavour from the name it is run under.
  ln -sf "$lld" "$out/wasm-ld"
  echo "build-wasm-runtime.sh: $out"
done
