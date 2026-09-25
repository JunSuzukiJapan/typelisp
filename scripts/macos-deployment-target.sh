#!/usr/bin/env bash
# Prints the minimum macOS version the Rust toolchain's own standard library
# was built for — the value `MACOSX_DEPLOYMENT_TARGET` must have so that
# rustc, the `cc` crate (ring's C and assembly) and `compile-file`'s link all
# agree with the std objects every binary contains (see build.rs).
#
# Read from the objects in the sysroot's `libstd` rather than from
# `rustc --print deployment-target`: rustc's default can be lower than what
# its prebuilt std claims (10.12 against 15.0 on x86_64 with Rust 1.98), and
# linking for the lower one makes ld warn about every std object.
#
# When it cannot say — no single libstd, an object without a version, objects
# that disagree — it stops and says how to go on: a person picks the value
# (`scripts/setup-cargo-env.sh --deployment-target X`, or exporting
# `MACOSX_DEPLOYMENT_TARGET` before `scripts/with-llvm-env.sh`). It never picks
# one itself.
#
# Used by scripts/setup-cargo-env.sh and scripts/with-llvm-env.sh.
set -euo pipefail

how_to_choose() {
    cat >&2 <<'MSG'
To go on with a value you choose:
  scripts/setup-cargo-env.sh --deployment-target <version>
  MACOSX_DEPLOYMENT_TARGET=<version> scripts/with-llvm-env.sh cargo ...
MSG
}

sysroot="$(rustc --print sysroot)"
host="$(rustc -vV | awk '/^host:/ { print $2 }')"
libdir="$sysroot/lib/rustlib/$host/lib"
shopt -s nullglob
rlibs=("$libdir/"libstd-*.rlib)
if [ "${#rlibs[@]}" -ne 1 ]; then
    echo "error: expected one libstd rlib in $libdir, found ${#rlibs[@]}" >&2
    if [ "${#rlibs[@]}" -eq 0 ]; then
        echo "Check that the toolchain rustc runs is installed for $host:" >&2
        echo "  rustup show active-toolchain; rustup component add rust-std" >&2
    else
        printf '  %s\n' "${rlibs[@]}" >&2
        echo "A toolchain directory should hold one libstd; reinstall it:" >&2
        echo "  rustup toolchain list; rustup toolchain install --force <toolchain>" >&2
    fi
    how_to_choose
    exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
(cd "$work" && ar x "${rlibs[0]}")
objs=("$work"/*.o)
if [ "${#objs[@]}" -eq 0 ]; then
    echo "error: ${rlibs[0]} holds no object files" >&2
    echo "Reinstall the toolchain: rustup toolchain install --force <toolchain>" >&2
    how_to_choose
    exit 1
fi

versions=""
for obj in "${objs[@]}"; do
    # LC_BUILD_VERSION carries `minos`, the older LC_VERSION_MIN_MACOSX `version`.
    v="$(otool -l "$obj" | awk '$1 == "minos" || ($1 == "version" && prev ~ /cmdsize/) { print $2; exit } { prev = $1 }')"
    if [ -z "$v" ]; then
        echo "error: $(basename "$obj") in ${rlibs[0]} names no minimum macOS version" >&2
        echo "(otool -l shows neither LC_BUILD_VERSION nor LC_VERSION_MIN_MACOSX for it.)" >&2
        echo "Look at what the other objects name and choose that:" >&2
        echo "  otool -l <object> | grep -A4 -E 'LC_BUILD_VERSION|LC_VERSION_MIN'" >&2
        how_to_choose
        exit 1
    fi
    versions="$versions$v"$'\n'
done
unique="$(printf '%s' "$versions" | sort -u)"
if [ "$(printf '%s\n' "$unique" | wc -l)" -ne 1 ]; then
    echo "error: the objects of ${rlibs[0]} do not name one minimum macOS version:" >&2
    printf '%s' "$versions" | sort | uniq -c | sort -rn | awk '{ printf "  %5d x %s\n", $1, $2 }' >&2
    highest="$(printf '%s\n' "$unique" | sort -t. -k1,1n -k2,2n -k3,3n | tail -1)"
    echo "Usually the highest one is right — an object built for a newer macOS than" >&2
    echo "the binary makes ld warn — so: $highest. Or check the toolchain first:" >&2
    echo "  rustup show active-toolchain; rustup update" >&2
    how_to_choose
    exit 1
fi
echo "$unique"
