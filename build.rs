fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(typelisp_bundled_runtime)");
    deployment_target();
    bundled_runtime();
}

// A release `typl` carries the static library every AOT executable links
// (`compile::aot::staticlib_path`) inside itself. `cargo install` builds in a
// temporary directory and keeps only the executables, so the archive it built
// along the way is gone by the time `typl -c` runs; the one copy that survives
// is the one inside `typl`.
//
// The archive is `typelisp-front`'s `staticlib`, and a build script cannot
// reach a dependency's artifacts, so it is built here by a cargo of its own.
// That cargo gets a target directory of its own under `OUT_DIR`: the outer one
// holds the lock on `target/` until this script returns. `--locked` keeps it
// from rewriting `Cargo.lock` in the source tree it builds from.
//
// Release only. A debug build links the archive in the tree it was built in,
// as before: the debug archive is over 100MB, and every test binary that links
// this crate would carry a copy.
fn bundled_runtime() {
    if std::env::var("PROFILE").as_deref() != Ok("release") {
        return;
    }
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let target = std::env::var("TARGET").expect("cargo sets TARGET");
    let cargo = std::env::var("CARGO").expect("cargo sets CARGO");
    for input in ["crates", "Cargo.toml", "Cargo.lock"] {
        println!("cargo:rerun-if-changed={}", manifest_dir.join(input).display());
    }
    let target_dir = out_dir.join("runtime-target");
    let status = std::process::Command::new(&cargo)
        .arg("build")
        .arg("--release")
        .arg("--locked")
        .args(["-p", "typelisp-front"])
        .arg("--manifest-path")
        .arg(manifest_dir.join("Cargo.toml"))
        .arg("--target")
        .arg(&target)
        .arg("--target-dir")
        .arg(&target_dir)
        .status()
        .unwrap_or_else(|e| panic!("build.rs: failed to run {} to build typelisp-front's static library: {}", cargo, e));
    assert!(status.success(), "build.rs: building typelisp-front's static library failed ({})", status);
    let archive = target_dir.join(&target).join("release").join("libtypelisp_front.a");
    assert!(archive.is_file(), "build.rs: {} was not built", archive.display());
    println!("cargo:rustc-env=TYPELISP_BUNDLED_STATICLIB={}", archive.display());
    println!("cargo:rustc-cfg=typelisp_bundled_runtime");
}

// Four things name a minimum macOS version, and they only agree when
// `MACOSX_DEPLOYMENT_TARGET` is set to the one the toolchain's prebuilt std
// was built for: rustc links for its own default (10.12 on x86_64, below the
// 15.0 its std objects claim with Rust 1.98), the `cc` crate — which compiles
// ring's C and assembly for rustls — uses the installed SDK's version (26.2 on
// an SDK newer than the OS), and the `cc` that `compile-file` links with uses
// Apple clang's default. Objects then claim a newer OS than the binary they
// are linked into. `scripts/setup-cargo-env.sh` / `scripts/with-llvm-env.sh`
// set the variable from `scripts/macos-deployment-target.sh`, which makes rustc
// and the `cc` crate use it; `compile-file` reads it from here, so an
// executable is built for the same OS as the static library it links, however
// `typl` itself was started.
fn deployment_target() {
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let version = std::env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| {
        panic!(
            "build.rs: MACOSX_DEPLOYMENT_TARGET is not set. Run scripts/setup-cargo-env.sh once \
             (or build through scripts/with-llvm-env.sh): without it the `cc` crate builds ring \
             for the SDK's macOS version while rustc builds for its own, and the objects claim a \
             newer OS than the binaries they are linked into"
        )
    });
    assert!(
        !version.is_empty() && version.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "build.rs: MACOSX_DEPLOYMENT_TARGET `{}` is not a version like 10.12",
        version
    );
    println!("cargo:rustc-env=TYPELISP_MACOSX_DEPLOYMENT_TARGET={}", version);
}
