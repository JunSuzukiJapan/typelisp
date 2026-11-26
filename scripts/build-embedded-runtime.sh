#!/usr/bin/env bash
# Builds what `typl -c --target <triple>` links an executable for a board
# without an operating system with, into
# <target dir>/embedded-runtime/<profile>/<triple>/:
#
#   libtypelisp_rt_embedded.a    the runtime (crates/typelisp-rt-embedded)
#                                built for the triple, for a board that gives
#                                its functions as C functions
#                                (`--board-functions c`)
#   libtypelisp_rt_semihosting.a the same with them over semihosting, which
#                                QEMU and a debugger answer
#                                (crates/typelisp-rt-semihosting,
#                                `--board-functions semihosting`)
#   link.x                     cortex-m-rt's linker script, which places the
#                              vector table and the sections in the memory
#                              that `memory.x` describes
#   ld.lld                     the linker (rust-lld) of the Rust toolchain
#                              that built it
#
# and the linker in <target dir>/embedded-runtime/<profile>/ too, for a board
# that has no runtime (AVR).
#
#   scripts/build-embedded-runtime.sh            # the debug profile
#   scripts/build-embedded-runtime.sh release    # the release profile
#
# Needs a rustup toolchain with the boards' targets (`rustup target add
# thumbv7em-none-eabihf thumbv8m.main-none-eabihf`): $TYPELISP_EMBEDDED_TOOLCHAIN, or
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
host="$("$bin_dir/rustc" -vV | sed -n 's/^host: //p')"
lld="$sysroot/lib/rustlib/$host/bin/rust-lld"
if [ ! -x "$lld" ]; then
  echo "build-embedded-runtime.sh: the toolchain at $sysroot has no rust-lld at $lld" >&2
  exit 1
fi

target_dir="${CARGO_TARGET_DIR:-$root/target}"
build_dir="$target_dir/embedded"
mkdir -p "$target_dir/embedded-runtime/$profile"
ln -sf "$lld" "$target_dir/embedded-runtime/$profile/ld.lld"

for target in thumbv7em-none-eabihf thumbv8m.main-none-eabihf; do
  if [ ! -d "$sysroot/lib/rustlib/$target" ]; then
    echo "build-embedded-runtime.sh: the toolchain at $sysroot has no $target target; run: rustup target add $target" >&2
    exit 1
  fi
  out="$target_dir/embedded-runtime/$profile/$target"
  mkdir -p "$out"
  for package in typelisp-rt-embedded typelisp-rt-semihosting; do
    # rustup's own cargo and rustc, ahead of any other on PATH. Built by
    # itself (`-p`), so that no other member of the workspace turns on the
    # runtime crates' `std`; each in a target directory of its own, so that
    # the two do not rebuild each other. The messages on standard output say
    # where each build script wrote (`out_dir`), which is where cortex-m-rt
    # put `link.x`.
    messages="$(cd "$root" && PATH="$bin_dir:$PATH" CARGO_TARGET_DIR="$build_dir/$package" \
      "$cargo_bin" build -p "$package" --target "$target" \
      --message-format=json-render-diagnostics ${profile_flag[@]+"${profile_flag[@]}"})"
    rt_out="$(printf '%s\n' "$messages" | grep '"reason":"build-script-executed"' | grep '#cortex-m-rt@' |
      sed -n 's/.*"out_dir":"\([^"]*\)".*/\1/p' | tail -n 1)"
    if [ -z "$rt_out" ] || [ ! -f "$rt_out/link.x" ]; then
      echo "build-embedded-runtime.sh: cortex-m-rt's build script did not say where it wrote link.x" >&2
      exit 1
    fi
    archive="lib$(printf '%s' "$package" | tr - _).a"
    cp "$build_dir/$package/$target/$profile/$archive" "$out/$archive"
  done
  cp "$rt_out/link.x" "$out/link.x"
  # lld picks its flavour from the name it is run under.
  ln -sf "$lld" "$out/ld.lld"
  echo "build-embedded-runtime.sh: $out"
done
