#!/usr/bin/env bash
# Builds `typl` and `typl-lsp` for distribution, for the CPU of the Mac this
# runs on, and leaves them in target/dist/typelisp-<version>-<arch>.tar.gz.
# Run it once on an Intel Mac and once on an Apple Silicon Mac, then give both
# archives to scripts/dist/package.sh on the Mac that holds the signing
# certificate:
#   scripts/dist/build.sh
#
# What differs from `cargo build --release`:
#
# - zstd is linked statically. Homebrew's LLVM is built with zstd, and
#   llvm-sys links it as `-lzstd`; the linker takes Homebrew's dylib when it
#   finds one, and a Mac without Homebrew's zstd could not start that typl.
#   A folder holding only `libzstd.a`, searched first, makes the linker take
#   the archive instead. Everything else typl links is in /usr/lib, and the
#   build stops if that is not so.
# - The minimum macOS version is the highest one the linked code was built
#   for: the toolchain's std, Homebrew's LLVM and Homebrew's zstd. A binary
#   that claims a lower one would start on a macOS its code was not built for.
#
# A cargo of its own in target/dist/build, so the ordinary release build in
# target/release is left as it was.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

if [ "$(uname -s)" != Darwin ]; then
    echo "error: this script builds the macOS distribution; this is $(uname -s)" >&2
    exit 1
fi
for tool in brew lipo otool ar; do
    command -v "$tool" >/dev/null 2>&1 || { echo "error: $tool not found" >&2; exit 1; }
done

llvm_prefix="$(brew --prefix llvm@22)"
zstd_prefix="$(brew --prefix zstd)"
[ -x "$llvm_prefix/bin/llvm-config" ] || { echo "error: llvm@22 not found at $llvm_prefix; run 'brew install llvm@22'" >&2; exit 1; }
[ -f "$zstd_prefix/lib/libzstd.a" ] || { echo "error: $zstd_prefix/lib/libzstd.a not found; run 'brew install zstd'" >&2; exit 1; }

arch="$(uname -m)"
dist="$repo_root/target/dist"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The highest minimum macOS version the objects of one archive name.
archive_minos() {
    local dir="$work/minos-$(basename "$1")"
    mkdir -p "$dir"
    (cd "$dir" && ar x "$1")
    local versions=""
    for obj in "$dir"/*.o; do
        versions="$versions$(otool -l "$obj" | awk '$1 == "minos" { print $2; exit }')"$'\n'
    done
    printf '%s' "$versions" | grep -v '^$' | sort -t. -k1,1n -k2,2n -k3,3n | tail -1
}

std_minos="$(scripts/macos-deployment-target.sh)"
llvm_minos="$(archive_minos "$llvm_prefix/lib/libLLVMSupport.a")"
zstd_minos="$(archive_minos "$zstd_prefix/lib/libzstd.a")"
minos="$(printf '%s\n%s\n%s\n' "$std_minos" "$llvm_minos" "$zstd_minos" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)"
echo "minimum macOS: $minos (std $std_minos, LLVM $llvm_minos, zstd $zstd_minos)"

static_zstd="$work/static-zstd"
mkdir -p "$static_zstd"
ln -s "$zstd_prefix/lib/libzstd.a" "$static_zstd/libzstd.a"

export LLVM_SYS_221_PREFIX="$llvm_prefix"
export MACOSX_DEPLOYMENT_TARGET="$minos"
for bin in typl typl-lsp; do
    cargo rustc --locked --release --target-dir "$dist/build" --bin "$bin" -- -L "native=$static_zstd"
done

typl="$dist/build/release/typl"
lsp="$dist/build/release/typl-lsp"
for exe in "$typl" "$lsp"; do
    foreign="$(otool -L "$exe" | tail -n +2 | awk '{ print $1 }' | grep -v -e '^/usr/lib/' -e '^/System/Library/' || true)"
    if [ -n "$foreign" ]; then
        echo "error: $(basename "$exe") links libraries outside the OS:" >&2
        printf '  %s\n' $foreign >&2
        exit 1
    fi
done

# The binary works: the JIT, and an executable compiled with it.
export TYPELISP_HOME="$work/home"
cat > "$work/jit.typl" <<'SRC'
(defun fib ((n int)) int (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
(compile fib)
(println "~a" (fib 20))
SRC
[ "$("$typl" "$work/jit.typl")" = 6765 ] || { echo "error: the JIT check failed" >&2; exit 1; }
printf '(defun main () i32 (println "ok") 0)\n' > "$work/aot.typl"
"$typl" -c "$work/aot.typl" -o "$work/aot"
[ "$("$work/aot")" = ok ] || { echo "error: the AOT check failed" >&2; exit 1; }

version="$("$typl" --version | awk '{ print $2 }')"
name="typelisp-$version-$arch"
rm -rf "${dist:?}/$name" "$dist/$name.tar.gz"
mkdir -p "$dist/$name"
cp "$typl" "$lsp" "$dist/$name/"
tar -C "$dist" -czf "$dist/$name.tar.gz" "$name"
echo "wrote $dist/$name.tar.gz"
