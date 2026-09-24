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
}
