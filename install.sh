#!/bin/sh
# Installs typl and typl-lsp from a GitHub release of typelisp:
#   curl -fsSL https://raw.githubusercontent.com/JunSuzukiJapan/typelisp/main/install.sh | sh
#
# TYPELISP_VERSION   the version to install, like 0.1.0 (default: the latest release)
# TYPELISP_HOME      where to install (default: ~/.typelisp); the executables go
#                    into its bin/, and typl keeps the library compiled programs
#                    link in its lib/
#
# The archives are the ones scripts/dist/build.sh makes, one per CPU, each with
# its SHA-256 beside it; the download is checked against it before anything is
# installed.
#
# Everything is inside `main`, called on the last line, so a download cut off
# halfway through runs nothing.
set -eu

repo="JunSuzukiJapan/typelisp"

err() {
    echo "typelisp install: $*" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || err "$1 is needed and was not found"
}

main() {
    [ "$(uname -s)" = Darwin ] || err "prebuilt binaries are for macOS only; elsewhere, build from source with 'cargo install --locked typelisp'"
    need curl
    need tar
    need shasum

    # `uname -m` answers x86_64 in a shell running under Rosetta; the CPU
    # itself is what decides which build runs natively.
    if [ "$(sysctl -n hw.optional.arm64 2>/dev/null || true)" = 1 ]; then
        arch=arm64
    else
        arch=x86_64
    fi

    version="${TYPELISP_VERSION:-}"
    if [ -z "$version" ]; then
        latest="$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")" ||
            err "could not reach https://github.com/$repo/releases/latest"
        case "${latest##*/}" in
            v*) version="${latest##*/v}" ;;
            *) err "could not tell the latest release from $latest" ;;
        esac
    fi

    if [ -n "${TYPELISP_HOME:-}" ]; then
        home="$TYPELISP_HOME"
    elif [ -n "${HOME:-}" ]; then
        home="$HOME/.typelisp"
    else
        err "neither TYPELISP_HOME nor HOME is set; set TYPELISP_HOME to where typelisp should go"
    fi

    name="typelisp-$version-$arch"
    base="https://github.com/$repo/releases/download/v$version"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT

    echo "downloading $name"
    curl -fsSL -o "$tmp/$name.tar.gz" "$base/$name.tar.gz" ||
        err "could not download $base/$name.tar.gz (is there a release v$version with a build for $arch?)"
    curl -fsSL -o "$tmp/$name.tar.gz.sha256" "$base/$name.tar.gz.sha256" ||
        err "could not download $base/$name.tar.gz.sha256"
    (cd "$tmp" && shasum -a 256 -c "$name.tar.gz.sha256" >/dev/null 2>&1) ||
        err "$name.tar.gz does not match its SHA-256; nothing was installed"
    tar -C "$tmp" -xzf "$tmp/$name.tar.gz"

    # Each executable is copied beside its final name and renamed over it, so
    # a typl that is running keeps its file and an interrupted install leaves
    # the old one whole.
    mkdir -p "$home/bin"
    for exe in typl typl-lsp; do
        [ -f "$tmp/$name/$exe" ] || err "$name.tar.gz holds no $exe"
        cp "$tmp/$name/$exe" "$home/bin/.$exe.new"
        chmod 755 "$home/bin/.$exe.new"
        mv -f "$home/bin/.$exe.new" "$home/bin/$exe"
    done
    "$home/bin/typl" --version >/dev/null 2>&1 ||
        err "the installed typl does not start; this macOS may be older than the one it was built for ($(sw_vers -productVersion))"
    echo "installed $("$home/bin/typl" --version) into $home/bin"

    case ":$PATH:" in
        *":$home/bin:"*) ;;
        *)
            echo
            echo "Add $home/bin to PATH, for example in ~/.zshrc:"
            echo "  export PATH=\"$home/bin:\$PATH\""
            ;;
    esac
    if ! xcrun --find cc >/dev/null 2>&1; then
        echo
        echo "typl -c links executables with the Xcode Command Line Tools, which are not installed:"
        echo "  xcode-select --install"
    fi
}

main "$@"
