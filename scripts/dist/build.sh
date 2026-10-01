#!/usr/bin/env bash
# Builds `typl` and `typl-lsp` for distribution, for the CPU of the Mac this
# runs on, and leaves them in target/dist/typelisp-<version>-<arch>.tar.gz
# with its SHA-256 beside it (.tar.gz.sha256). Run it once on an Intel Mac and
# once on an Apple Silicon Mac, and attach all four files to the GitHub
# release; install.sh at the repository root downloads them:
#   scripts/dist/build.sh
#   gh release upload v<version> target/dist/typelisp-<version>-<arch>.tar.gz target/dist/typelisp-<version>-<arch>.tar.gz.sha256
#
# The binaries are not signed with a Developer ID. A file curl downloads gets
# no quarantine attribute, so Gatekeeper does not look at it; an Apple Silicon
# binary needs only the ad-hoc signature the linker gives it.
#
# Where LLVM and zstd come from depends on the CPU:
#
# - arm64: LLVM's own release build (LLVM-<version>-macOS-ARM64.tar.xz from
#   GitHub), not Homebrew's. Homebrew has bottles for the newest macOS only,
#   so a typl linked with them starts on that macOS and no older one. The
#   release build is made for an older macOS (14.0 for 22.1.8), but its
#   archives hold LLVM bitcode for ThinLTO, not machine code, and neither
#   linker takes them: its own ld64.lld cannot read a newer SDK's .tbd files,
#   and Apple's ld given its libLTO.dylib leaves the C++ runtime's symbols
#   undefined. So each object is compiled to machine code with the clang that
#   comes with it, for the macOS its llvm-config was built for, and the
#   archives are put together again. zstd is built from source for the same
#   macOS: Homebrew's is built for the newest one too. Both are kept in
#   target/dist/cache and made again only when that is removed.
# - x86_64: Homebrew's llvm@22 and zstd. LLVM has no release build for an
#   Intel Mac.
#
# What differs from `cargo build --release`:
#
# - zstd is linked statically. LLVM is built with zstd, and llvm-sys links it
#   as `-lzstd`; the linker takes a dylib when it finds one (Homebrew's), and
#   a Mac without Homebrew's zstd could not start that typl. A folder holding
#   only `libzstd.a`, searched first, makes the linker take the archive
#   instead. Everything else typl links is in /usr/lib, and the build stops if
#   that is not so.
# - The minimum macOS version is the highest one the linked code was built
#   for: the toolchain's std, LLVM and zstd. A binary that claims a lower one
#   would start on a macOS its code was not built for.
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
arch="$(uname -m)"
case "$arch" in
    arm64) tools="otool ar curl shasum tar make" ;;
    x86_64) tools="otool ar brew" ;;
    *) echo "error: no distribution build for $arch" >&2; exit 1 ;;
esac
for tool in $tools; do
    command -v "$tool" >/dev/null 2>&1 || { echo "error: $tool not found" >&2; exit 1; }
done

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

# Downloads $1 to $3, and stops unless its SHA-256 is $2.
fetch() {
    curl -fL --retry 3 -o "$3" "$1"
    echo "$2  $3" | shasum -a 256 -c - >/dev/null || { echo "error: $1 is not the file expected (SHA-256)" >&2; exit 1; }
}

# The release build of LLVM, its archives compiled to machine code, in $1:
# bin/llvm-config, include/ and the archives llvm-config names, nothing else.
official_llvm() {
    local prefix="$1" version="$2" sha256="$3"
    local name="LLVM-$version-macOS-ARM64"
    echo "preparing LLVM $version from its release build (once; kept in $prefix)"
    fetch "https://github.com/llvm/llvm-project/releases/download/llvmorg-$version/$name.tar.xz" "$sha256" "$work/$name.tar.xz"
    tar -C "$work" -xf "$work/$name.tar.xz"
    rm "$work/$name.tar.xz"
    local src="$work/$name" tmp="$prefix.tmp"
    local target
    target="$(otool -l "$src/bin/llvm-config" | awk '$1 == "minos" { print $2; exit }')"
    rm -rf "$tmp"
    mkdir -p "$tmp/bin" "$tmp/lib" "$tmp/objects"
    cp "$src/bin/llvm-config" "$tmp/bin/"
    cp -R "$src/include" "$tmp/include"
    local lib
    for lib in $("$src/bin/llvm-config" --link-static --libnames); do
        mkdir -p "$tmp/objects/$lib"
        (cd "$tmp/objects/$lib" && ar x "$src/lib/$lib")
        # The order the archive had, without its symbol table.
        ar t "$src/lib/$lib" | grep -v '^__\.SYMDEF' > "$tmp/objects/$lib.members"
    done
    export clang="$src/bin/clang" target
    find "$tmp/objects" -name '*.o' -print0 | xargs -0 -P "$(sysctl -n hw.ncpu)" -I{} sh -c \
        '"$clang" -c -x ir -O2 -target "arm64-apple-macos$target" -w "$1" -o "$1.native" && mv "$1.native" "$1"' _ {}
    for lib in $("$src/bin/llvm-config" --link-static --libnames); do
        (cd "$tmp/objects/$lib" && xargs ar rcs "$tmp/lib/$lib" < "$tmp/objects/$lib.members")
    done
    rm -rf "$tmp/objects" "$src"
    rm -rf "$prefix"
    mv "$tmp" "$prefix"
}

# zstd's libzstd.a, built from source for macOS $3, in $1.
source_zstd() {
    local dir="$1" version="$2" target="$3" sha256="$4"
    echo "building zstd $version for macOS $target (once; kept in $dir)"
    fetch "https://github.com/facebook/zstd/releases/download/v$version/zstd-$version.tar.gz" "$sha256" "$work/zstd.tar.gz"
    tar -C "$work" -xzf "$work/zstd.tar.gz"
    MACOSX_DEPLOYMENT_TARGET="$target" make -C "$work/zstd-$version/lib" -j "$(sysctl -n hw.ncpu)" libzstd.a >/dev/null
    mkdir -p "$dir"
    cp "$work/zstd-$version/lib/libzstd.a" "$dir/libzstd.a.tmp"
    mv "$dir/libzstd.a.tmp" "$dir/libzstd.a"
}

case "$arch" in
    arm64)
        llvm_version=22.1.8
        llvm_prefix="$dist/cache/llvm-$llvm_version-arm64"
        [ -x "$llvm_prefix/bin/llvm-config" ] ||
            official_llvm "$llvm_prefix" "$llvm_version" f260f4f7c0d430828a81ae8a3826a1d63fc0963ec2459489308cc23b1f7eab4f
        llvm_target="$(otool -l "$llvm_prefix/bin/llvm-config" | awk '$1 == "minos" { print $2; exit }')"
        zstd_version=1.5.7
        zstd_lib="$dist/cache/zstd-$zstd_version-macos$llvm_target/libzstd.a"
        [ -f "$zstd_lib" ] ||
            source_zstd "$(dirname "$zstd_lib")" "$zstd_version" "$llvm_target" eb33e51f49a15e023950cd7825ca74a4a2b43db8354825ac24fc1b7ee09e6fa3
        ;;
    x86_64)
        llvm_prefix="$(brew --prefix llvm@22)"
        zstd_lib="$(brew --prefix zstd)/lib/libzstd.a"
        [ -x "$llvm_prefix/bin/llvm-config" ] || { echo "error: llvm@22 not found at $llvm_prefix; run 'brew install llvm@22'" >&2; exit 1; }
        [ -f "$zstd_lib" ] || { echo "error: $zstd_lib not found; run 'brew install zstd'" >&2; exit 1; }
        ;;
esac

std_minos="$(scripts/macos-deployment-target.sh)"
llvm_minos="$(archive_minos "$llvm_prefix/lib/libLLVMSupport.a")"
zstd_minos="$(archive_minos "$zstd_lib")"
minos="$(printf '%s\n%s\n%s\n' "$std_minos" "$llvm_minos" "$zstd_minos" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)"
echo "minimum macOS: $minos (std $std_minos, LLVM $llvm_minos, zstd $zstd_minos)"

static_zstd="$work/static-zstd"
mkdir -p "$static_zstd"
ln -s "$zstd_lib" "$static_zstd/libzstd.a"

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
rm -rf "${dist:?}/$name" "$dist/$name.tar.gz" "$dist/$name.tar.gz.sha256"
mkdir -p "$dist/$name"
cp "$typl" "$lsp" LICENSE-APACHE LICENSE-MIT LICENSE-EXCEPTION "$dist/$name/"
tar -C "$dist" -czf "$dist/$name.tar.gz" "$name"
(cd "$dist" && shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256")
echo "wrote $dist/$name.tar.gz and $name.tar.gz.sha256"
