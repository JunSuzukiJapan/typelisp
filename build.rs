// llvm-sys puts `<llvm@17>/lib` on the link search path, and Homebrew's
// llvm@17 ships `libunwind.dylib` there. When ld resolves libSystem's
// re-export of `/usr/lib/system/libunwind.dylib` it searches `-L` first, so it
// picks up Homebrew's copy, warns that the copy "couldn't be matched with any
// parent library", and links it directly — the binaries then unwind through
// Homebrew's libunwind instead of the system one. Searching the SDK's
// `usr/lib/system` ahead of it makes the re-export resolve to the system stub.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=SDKROOT");
    if std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() != Ok("apple") {
        return;
    }
    let out = std::process::Command::new("xcrun")
        .arg("--show-sdk-path")
        .output()
        .expect("build.rs: failed to run `xcrun --show-sdk-path`");
    assert!(
        out.status.success(),
        "build.rs: `xcrun --show-sdk-path` failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sdk = String::from_utf8(out.stdout).expect("build.rs: SDK path is not UTF-8");
    println!("cargo:rustc-link-search=native={}/usr/lib/system", sdk.trim());

    deployment_target();
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
