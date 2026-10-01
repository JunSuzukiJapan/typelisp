#!/usr/bin/env bash
# Joins the per-CPU builds of scripts/dist/build.sh into one universal binary,
# signs it with a Developer ID, checks that the signed binaries still work,
# and has Apple notarize the zip it writes to target/dist:
#   TYPELISP_SIGN_IDENTITY="Developer ID Application: ..." \
#   TYPELISP_NOTARY_PROFILE=<profile> \
#   scripts/dist/package.sh target/dist/typelisp-0.1.0-x86_64.tar.gz target/dist/typelisp-0.1.0-arm64.tar.gz
#
# TYPELISP_NOTARY_PROFILE names credentials stored once with
#   xcrun notarytool store-credentials <profile>
#
# `--no-notarize` stops before notarization. With it, TYPELISP_SIGN_IDENTITY may
# be `-` (ad-hoc), which checks the signing and the entitlements without a
# certificate; such a zip is not for distribution.
#
# Signed with the hardened runtime, which notarization requires. The JIT and
# the FFI need exceptions to it, given in the .entitlements files beside this
# script; the checks below run the signed binaries to show they are enough.
#
# Notarization cannot be stapled to a zip or a bare executable, so Gatekeeper
# looks the ticket up online the first time the downloaded typl starts.
set -euo pipefail

usage="usage: scripts/dist/package.sh [--no-notarize] ARCHIVE..."
notarize=1
archives=()
for arg in "$@"; do
    case "$arg" in
        --no-notarize) notarize=0 ;;
        -*) echo "$usage" >&2; exit 1 ;;
        *) archives+=("$arg") ;;
    esac
done
[ "${#archives[@]}" -ge 1 ] || { echo "$usage" >&2; exit 1; }

identity="${TYPELISP_SIGN_IDENTITY:-}"
[ -n "$identity" ] || { echo "error: set TYPELISP_SIGN_IDENTITY to a \"Developer ID Application: ...\" identity (security find-identity -v -p codesigning)" >&2; exit 1; }
if [ "$identity" = - ] && [ "$notarize" = 1 ]; then
    echo "error: an ad-hoc signature (TYPELISP_SIGN_IDENTITY=-) cannot be notarized; add --no-notarize" >&2
    exit 1
fi
profile="${TYPELISP_NOTARY_PROFILE:-}"
if [ "$notarize" = 1 ] && [ -z "$profile" ]; then
    echo "error: set TYPELISP_NOTARY_PROFILE to a profile stored with 'xcrun notarytool store-credentials', or pass --no-notarize" >&2
    exit 1
fi

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$here/../.." && pwd)"
dist="$repo_root/target/dist"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Each archive holds typelisp-<version>-<arch>/{typl,typl-lsp} for one CPU.
version=""
arches=()
typls=()
lsps=()
for i in "${!archives[@]}"; do
    archive="${archives[$i]}"
    [ -f "$archive" ] || { echo "error: $archive not found" >&2; exit 1; }
    mkdir -p "$work/in-$i"
    tar -C "$work/in-$i" -xzf "$archive"
    dir="$(find "$work/in-$i" -mindepth 1 -maxdepth 1 -type d -name 'typelisp-*')"
    [ -x "$dir/typl" ] && [ -x "$dir/typl-lsp" ] || { echo "error: $archive does not hold typl and typl-lsp" >&2; exit 1; }
    arch="$(lipo -archs "$dir/typl")"
    case " ${arches[*]-} " in *" $arch "*) echo "error: two archives are for $arch" >&2; exit 1 ;; esac
    this_version="$(basename "$dir" | sed -E 's/^typelisp-(.*)-[^-]+$/\1/')"
    if [ -n "$version" ] && [ "$this_version" != "$version" ]; then
        echo "error: the archives are for different versions ($version and $this_version)" >&2
        exit 1
    fi
    version="$this_version"
    arches+=("$arch")
    typls+=("$dir/typl")
    lsps+=("$dir/typl-lsp")
done

if [ "${#arches[@]}" -eq 1 ]; then
    flavor="${arches[0]}"
else
    flavor=universal
fi
name="typelisp-$version"
stage="$work/$name"
mkdir -p "$stage"
lipo -create "${typls[@]}" -output "$stage/typl"
lipo -create "${lsps[@]}" -output "$stage/typl-lsp"
cp "$repo_root/LICENSE-APACHE" "$repo_root/LICENSE-MIT" "$repo_root/LICENSE-EXCEPTION" "$stage/"
echo "architectures: $(lipo -archs "$stage/typl")"

sign() {
    local timestamp=(--timestamp)
    [ "$identity" = - ] && timestamp=()
    codesign --force --options runtime "${timestamp[@]}" --entitlements "$here/$2.entitlements" --sign "$identity" "$1"
    codesign --verify --strict --verbose=2 "$1"
}
sign "$stage/typl" typl
sign "$stage/typl-lsp" typl-lsp

# The signed binaries work under the hardened runtime: the JIT, an executable
# compiled with typl, a C library from outside the OS through the FFI, and
# typl-lsp answering a document (it handles messages one at a time, in order,
# so the document is checked before the shutdown that follows it is read).
# Without the entitlements the first is killed by the kernel and the third
# cannot open the library.
export TYPELISP_HOME="$work/home"
typl="$stage/typl"
cat > "$work/jit.typl" <<'SRC'
(defun fib ((n int)) int (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
(compile fib)
(println "~a" (fib 20))
SRC
[ "$("$typl" "$work/jit.typl")" = 6765 ] || { echo "error: the JIT check failed" >&2; exit 1; }
printf '(defun main () i32 (println "ok") 0)\n' > "$work/aot.typl"
"$typl" -c "$work/aot.typl" -o "$work/aot"
[ "$("$work/aot")" = ok ] || { echo "error: the AOT check failed" >&2; exit 1; }
zstd_dylib="$(brew --prefix zstd)/lib/libzstd.dylib"
cat > "$work/ffi.typl" <<SRC
(defffi (zstd-version "ZSTD_versionNumber") () i32 :library "$zstd_dylib")
(println "~a" (> (unsafe (zstd-version)) 0))
SRC
[ "$("$typl" "$work/ffi.typl")" = true ] || { echo "error: the FFI check failed (loading $zstd_dylib)" >&2; exit 1; }
lsp_message() { printf 'Content-Length: %d\r\n\r\n%s' "${#1}" "$1"; }
printf '(defmacro twice (x) `(+ ,x ,x))\n(defun f () int (twice 1))\n' > "$work/doc.typl"
{
    lsp_message '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}'
    lsp_message '{"jsonrpc":"2.0","method":"initialized","params":{}}'
    lsp_message "{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{\"textDocument\":{\"uri\":\"file://$work/doc.typl\",\"languageId\":\"typelisp\",\"version\":1,\"text\":\"(defmacro twice (x) \`(+ ,x ,x))\\n(defun f () int (twice 1))\\n\"}}}"
    lsp_message '{"jsonrpc":"2.0","id":2,"method":"shutdown"}'
    lsp_message '{"jsonrpc":"2.0","method":"exit"}'
} | "$stage/typl-lsp" > "$work/lsp.out"
grep -q '"diagnostics":\[\]' "$work/lsp.out" || { echo "error: the typl-lsp check failed:" >&2; cat "$work/lsp.out" >&2; exit 1; }
echo "checks passed: JIT, AOT, FFI, typl-lsp"

zip="$dist/$name-macos-$flavor.zip"
mkdir -p "$dist"
rm -f "$zip"
ditto -c -k --keepParent "$stage" "$zip"

if [ "$notarize" = 1 ]; then
    echo "notarizing (this waits for Apple's answer)..."
    out="$(xcrun notarytool submit "$zip" --keychain-profile "$profile" --wait 2>&1)" || true
    echo "$out"
    if ! printf '%s\n' "$out" | grep -q '^ *status: Accepted'; then
        id="$(printf '%s\n' "$out" | awk '$1 == "id:" { print $2; exit }')"
        echo "error: notarization was not accepted${id:+; see: xcrun notarytool log $id --keychain-profile $profile}" >&2
        rm -f "$zip"
        exit 1
    fi
fi

echo "wrote $zip"
shasum -a 256 "$zip"
if [ "$notarize" = 1 ]; then
    echo "to attach it to the release: gh release upload v$version $zip"
fi
