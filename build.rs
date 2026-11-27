// The runtimes for a board, which `bundled_embedded` builds with `-p`: named
// so that they are dependencies that are used. Both are empty for the host.
extern crate typelisp_rt_embedded;
extern crate typelisp_rt_semihosting;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(typelisp_bundled_runtime)");
    println!("cargo::rustc-check-cfg=cfg(typelisp_bundled_wasm)");
    println!("cargo::rustc-check-cfg=cfg(typelisp_bundled_embedded)");
    let deployment = deployment_target();
    bundled_runtime(deployment.as_deref());
    profile_dir();
}

// `<target dir>/<profile>`, where this build's artifacts go — the debug `typl`
// links `typelisp-front`'s staticlib from there (`compile::aot::link_archive`).
// Read from `OUT_DIR` rather than put together from `CARGO_MANIFEST_DIR`, so a
// `CARGO_TARGET_DIR` or a `--target` is followed. Cargo does not document its
// shape, and it has had two: `<target dir>/<profile>/build/<pkg>-<hash>/out`,
// and `<target dir>/<profile>/build/<pkg>/<hash>/out` from Rust 1.100. Anything
// else stops the build instead of naming a folder the archive is not in.
fn profile_dir() {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    fn is_build(p: Option<&std::path::Path>) -> Option<&std::path::Path> {
        p.filter(|p| p.file_name() == Some("build".as_ref()))
    }
    let unit = out_dir.parent().filter(|_| out_dir.file_name() == Some("out".as_ref()));
    let build = unit
        .and_then(|u| is_build(u.parent()).or_else(|| is_build(u.parent().and_then(|p| p.parent()))));
    let dir = match build {
        Some(build) => build.parent().expect("`build` has a parent"),
        None => panic!(
            "OUT_DIR `{}` is neither `<target dir>/<profile>/build/<pkg>-<hash>/out` nor `<target dir>/<profile>/build/<pkg>/<hash>/out`",
            out_dir.display()
        ),
    };
    println!("cargo:rustc-env=TYPELISP_PROFILE_DIR={}", dir.display());
}

// A release `typl` carries the static library every AOT executable links
// (`compile::aot::link_archive`) inside itself, and writes it out to
// `$TYPELISP_HOME/lib/<build id>/` the first time it links. `cargo install`
// builds in a temporary directory and keeps only the executables, so the
// archive it built along the way is gone by the time `typl -c` runs; the one
// copy that survives is the one inside `typl`.
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
//
// `deployment` is the minimum macOS version `deployment_target` chose. The
// inner cargo is given it even when the outer one was not: this archive is
// what `typl -c` links, so its objects are the ones that must agree with the
// executable.
//
// `TYPELISP_LINK_TREE_RUNTIME` set to anything non-empty makes a release build
// do what a debug one does: no inner cargo, and `typl` links the
// `target/release` archive the outer build makes anyway (the workspace's
// `default-members`). The inner build compiles every runtime crate a second
// time in a target directory of its own; this is for working on the tree,
// where that second build is the wait. A `typl` built this way cannot leave
// the tree, so it is not something to install.
fn bundled_runtime(deployment: Option<&str>) {
    if std::env::var("PROFILE").as_deref() != Ok("release") {
        return;
    }
    println!("cargo:rerun-if-env-changed=TYPELISP_LINK_TREE_RUNTIME");
    if std::env::var_os("TYPELISP_LINK_TREE_RUNTIME").is_some_and(|v| !v.is_empty()) {
        println!(
            "cargo:warning=TYPELISP_LINK_TREE_RUNTIME is set: this typl links target/release/libtypelisp_front.a \
             instead of carrying it, so do not install it"
        );
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
    let mut build = std::process::Command::new(&cargo);
    // As `--config` as well as in the environment: this cargo reads the
    // repository's `.cargo/config.toml` too, and the value
    // `scripts/setup-cargo-env.sh` writes there is `force = true`, which wins
    // over the environment. A `--config` wins over the file.
    if let Some(version) = deployment {
        build.env("MACOSX_DEPLOYMENT_TARGET", version);
        build.arg(format!("--config=env.MACOSX_DEPLOYMENT_TARGET.value=\"{}\"", version));
        build.arg("--config=env.MACOSX_DEPLOYMENT_TARGET.force=true");
    }
    let status = build
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
    // The name of the folder `typl` writes the archive to: a hash of its
    // bytes (and of the WebAssembly runtimes', when it carries them), so a
    // `typl` built again never links an archive another build wrote. Only
    // compared for equality, within one machine, so `std`'s hasher is enough —
    // the value is fixed here and carried in the binary.
    let bytes = std::fs::read(&archive).unwrap_or_else(|e| panic!("build.rs: failed to read {}: {}", archive.display(), e));
    let mut hasher = std::hash::DefaultHasher::new();
    std::hash::Hasher::write(&mut hasher, &bytes);
    bundled_wasm(&cargo, &manifest_dir, &target_dir, &mut hasher);
    bundled_embedded(&cargo, &manifest_dir, &target_dir, &mut hasher);
    println!("cargo:rustc-env=TYPELISP_BUNDLED_STATICLIB_ID={:016x}", std::hash::Hasher::finish(&hasher));
    println!("cargo:rustc-cfg=typelisp_bundled_runtime");
}

// With the `wasm` feature, a release `typl` also carries what `typl -c
// --target <triple>` links a WebAssembly module with: the runtime built for
// each of the two triples, and for wasm32-wasip1 WASI's start file and C
// library, from the toolchain building this. Without the feature it carries
// none of it — nothing is built and nothing is asked of the toolchain.
//
// The linker is not carried: `rust-lld` is linked against the toolchain's own
// `libLLVM`, so a copy of it alone does not run. `typl` runs the one in the
// rustup toolchain it finds when it links (`compile::aot::wasm_linker`) — a
// toolchain with these targets is what building with the feature needed.
fn bundled_wasm(cargo: &str, manifest_dir: &std::path::Path, target_dir: &std::path::Path, hasher: &mut std::hash::DefaultHasher) {
    if std::env::var_os("CARGO_FEATURE_WASM").is_none() {
        return;
    }
    let rustc = std::env::var("RUSTC").expect("cargo sets RUSTC");
    let sysroot = std::process::Command::new(&rustc)
        .args(["--print", "sysroot"])
        .output()
        .unwrap_or_else(|e| panic!("build.rs: failed to run {} --print sysroot: {}", rustc, e));
    assert!(sysroot.status.success(), "build.rs: {} --print sysroot failed", rustc);
    let sysroot = std::path::PathBuf::from(String::from_utf8_lossy(&sysroot.stdout).trim());
    for (triple, key) in [("wasm32-wasip1", "WASIP1"), ("wasm32-unknown-unknown", "UNKNOWN")] {
        assert!(
            sysroot.join("lib").join("rustlib").join(triple).is_dir(),
            "build.rs: the `wasm` feature needs the {} target in the toolchain at {}: run `rustup target add {}`",
            triple,
            sysroot.display(),
            triple
        );
        let status = std::process::Command::new(cargo)
            .arg("build")
            .arg("--release")
            .arg("--locked")
            .args(["-p", "typelisp-front"])
            .arg("--manifest-path")
            .arg(manifest_dir.join("Cargo.toml"))
            .arg("--target")
            .arg(triple)
            .arg("--target-dir")
            .arg(target_dir)
            .status()
            .unwrap_or_else(|e| panic!("build.rs: failed to run {} to build the {} runtime: {}", cargo, triple, e));
        assert!(status.success(), "build.rs: building the {} runtime failed ({})", triple, status);
        let archive = target_dir.join(triple).join("release").join("libtypelisp_front.a");
        bundle_file(&format!("TYPELISP_BUNDLED_WASM_{}_STATICLIB", key), &archive, hasher);
    }
    let self_contained = sysroot.join("lib").join("rustlib").join("wasm32-wasip1").join("lib").join("self-contained");
    bundle_file("TYPELISP_BUNDLED_WASIP1_CRT1", &self_contained.join("crt1-command.o"), hasher);
    bundle_file("TYPELISP_BUNDLED_WASIP1_LIBC", &self_contained.join("libc.a"), hasher);
    println!("cargo:rustc-cfg=typelisp_bundled_wasm");
}

// With the `embedded` feature, a release `typl` also carries what `typl -c
// --target <board>` links a board's executable with, for each board's triple
// (`BOARDS`): the runtime built for the triple twice, for a board that gives
// its functions as C functions (`typelisp-rt-embedded`) and with them over
// semihosting (`typelisp-rt-semihosting`), and the linker script `link.x` of
// the core's startup crate, which that build writes where the crate's build
// script runs (`out_dir`, which the build's messages say). Without the
// feature it carries none of it. What it carries, the crate is told by
// `$OUT_DIR/bundled_boards.rs`: a function that answers the three files'
// bytes for a triple (`compile::aot::archive`).
//
// The linker is not carried, for the reason `bundled_wasm` gives.
fn bundled_embedded(cargo: &str, manifest_dir: &std::path::Path, target_dir: &std::path::Path, hasher: &mut std::hash::DefaultHasher) {
    if std::env::var_os("CARGO_FEATURE_EMBEDDED").is_none() {
        return;
    }
    let mut arms = String::new();
    for (triple, startup) in BOARDS {
        let [link_x, c, semihosting] = bundled_board(cargo, manifest_dir, target_dir, triple, startup, hasher);
        arms.push_str(&format!(
            "        {:?} => Some([include_bytes!({:?}), include_bytes!({:?}), include_bytes!({:?})]),\n",
            triple,
            link_x.display().to_string(),
            c.display().to_string(),
            semihosting.display().to_string()
        ));
    }
    let table = format!(
        "/// The files a board's executable links with, carried for `triple`:\n\
         /// `link.x`, the runtime for C board functions, the runtime over semihosting.\n\
         fn bundled_board(triple: &str) -> Option<[&'static [u8]; 3]> {{\n    match triple {{\n{}        _ => None,\n    }}\n}}\n",
        arms
    );
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    std::fs::write(out_dir.join("bundled_boards.rs"), table).expect("build.rs: failed to write bundled_boards.rs");
    println!("cargo:rustc-cfg=typelisp_bundled_embedded");
}

/// The triples of the boards `typl -c --target` builds for with a runtime
/// (`compile::aot::Target::BOARDS`), each with the crate that starts the
/// program on its core and writes `link.x`.
const BOARDS: [(&str, &str); 4] = [
    ("thumbv7em-none-eabihf", "cortex-m-rt"),
    ("thumbv8m.main-none-eabihf", "cortex-m-rt"),
    ("thumbv6m-none-eabi", "cortex-m-rt"),
    ("riscv32imac-unknown-none-elf", "riscv-rt"),
];

/// What [`bundled_embedded`] carries for the board `triple`, whose startup
/// crate is `startup`: the paths of `link.x` and of the two runtimes.
fn bundled_board(
    cargo: &str,
    manifest_dir: &std::path::Path,
    target_dir: &std::path::Path,
    triple: &str,
    startup: &str,
    hasher: &mut std::hash::DefaultHasher,
) -> [std::path::PathBuf; 3] {
    let rustc = std::env::var("RUSTC").expect("cargo sets RUSTC");
    let sysroot = std::process::Command::new(&rustc)
        .args(["--print", "sysroot"])
        .output()
        .unwrap_or_else(|e| panic!("build.rs: failed to run {} --print sysroot: {}", rustc, e));
    assert!(sysroot.status.success(), "build.rs: {} --print sysroot failed", rustc);
    let sysroot = std::path::PathBuf::from(String::from_utf8_lossy(&sysroot.stdout).trim());
    assert!(
        sysroot.join("lib").join("rustlib").join(triple).is_dir(),
        "build.rs: the `embedded` feature needs the {} target in the toolchain at {}: run `rustup target add {}`",
        triple,
        sysroot.display(),
        triple
    );
    let mut link_x = None;
    let mut archives = Vec::new();
    for package in ["typelisp-rt-embedded", "typelisp-rt-semihosting"] {
        // Each in a target directory of its own, so that the two do not
        // rebuild each other.
        let dir = target_dir.join(package);
        let out = std::process::Command::new(cargo)
            .arg("build")
            .arg("--release")
            .arg("--locked")
            .args(["-p", package])
            .arg("--manifest-path")
            .arg(manifest_dir.join("Cargo.toml"))
            .arg("--target")
            .arg(triple)
            .arg("--target-dir")
            .arg(&dir)
            .arg("--message-format=json-render-diagnostics")
            .stderr(std::process::Stdio::inherit())
            .output()
            .unwrap_or_else(|e| panic!("build.rs: failed to run {} to build {}: {}", cargo, package, e));
        assert!(out.status.success(), "build.rs: building {} for {} failed ({})", package, triple, out.status);
        let messages = String::from_utf8_lossy(&out.stdout);
        let startup_id = format!("#{}@", startup);
        let out_dir = messages
            .lines()
            .filter(|l| l.contains("\"reason\":\"build-script-executed\"") && l.contains(&startup_id))
            .filter_map(|l| l.split("\"out_dir\":\"").nth(1).and_then(|rest| rest.split('"').next()))
            .last()
            .unwrap_or_else(|| panic!("build.rs: building {} did not say where {} wrote link.x", package, startup));
        link_x = Some(std::path::PathBuf::from(out_dir).join("link.x"));
        let archive = dir.join(triple).join("release").join(format!("lib{}.a", package.replace('-', "_")));
        bundle_hash(&archive, hasher);
        archives.push(archive);
    }
    let link_x = link_x.expect("two runtimes were built");
    bundle_hash(&link_x, hasher);
    let [c, semihosting]: [std::path::PathBuf; 2] = archives.try_into().expect("two runtimes were built");
    [link_x, c, semihosting]
}

/// Adds the bytes of `path`, a file the build carries, to `hasher`.
fn bundle_hash(path: &std::path::Path, hasher: &mut std::hash::DefaultHasher) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("build.rs: failed to read {}: {}", path.display(), e));
    std::hash::Hasher::write(hasher, &bytes);
}

fn bundle_file(name: &str, path: &std::path::Path, hasher: &mut std::hash::DefaultHasher) {
    bundle_hash(path, hasher);
    println!("cargo:rustc-env={}={}", name, path.display());
}

// Four things name a minimum macOS version, and they only agree when
// `MACOSX_DEPLOYMENT_TARGET` is set to the one the toolchain's prebuilt std
// was built for: rustc links for its own default (10.12 on x86_64, below the
// 15.0 its std objects claim with Rust 1.98), the `cc` crate — which compiles
// ring's C and assembly for rustls — uses the installed SDK's version (26.2 on
// an SDK newer than the OS), and the `cc` that `compile-file` links with uses
// Apple clang's default. Objects then claim a newer OS than the binary they
// are linked into, and the linker says so: unset, every `typl -c` printed 27
// warnings, one per ring object. Nothing stops working — measured on macOS 15
// with an SDK for 26.2, `typl` and its executables ran, TLS included.
//
// So a value is not required. One a person sets is used everywhere
// (`scripts/setup-cargo-env.sh` / `scripts/with-llvm-env.sh` set it from
// `scripts/macos-deployment-target.sh`, which reaches rustc and the `cc`
// crate for the whole build). Without one, this script reads std's the same
// way and uses it where it can still reach: the inner cargo that builds the
// archive `typl -c` links (`bundled_runtime`), and `compile-file`'s own link.
// Out of reach is the outer build, already under way — `typl`'s own link
// disagrees with std, but cargo does not show link warnings. When std's
// cannot be read, the build goes on without a value and says why.
//
// Returns the version chosen, for `bundled_runtime`.
fn deployment_target() -> Option<String> {
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return None;
    }
    let version = match std::env::var("MACOSX_DEPLOYMENT_TARGET") {
        Ok(version) => {
            assert!(is_version(&version), "build.rs: MACOSX_DEPLOYMENT_TARGET `{}` is not a version like 10.12", version);
            version
        }
        Err(_) => match std_deployment_target() {
            Ok(version) => version,
            Err(why) => {
                println!(
                    "cargo:warning=MACOSX_DEPLOYMENT_TARGET is not set and std's minimum macOS version \
                     could not be read ({}); executables `typl -c` builds may be linked with warnings. \
                     Set MACOSX_DEPLOYMENT_TARGET to choose one",
                    why
                );
                return None;
            }
        },
    };
    println!("cargo:rustc-env=TYPELISP_MACOSX_DEPLOYMENT_TARGET={}", version);
    Some(version)
}

fn is_version(version: &str) -> bool {
    !version.is_empty() && version.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

// The minimum macOS version the objects of the toolchain's `libstd` name —
// `scripts/macos-deployment-target.sh` in Rust. Read from the objects rather
// than from `rustc --print deployment-target`, which answers rustc's own
// default (10.12) and not what its prebuilt std was built for.
fn std_deployment_target() -> Result<String, String> {
    let rustc = std::env::var("RUSTC").map_err(|_| "cargo did not set RUSTC".to_string())?;
    let target = std::env::var("TARGET").map_err(|_| "cargo did not set TARGET".to_string())?;
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").map_err(|_| "cargo did not set OUT_DIR".to_string())?);
    let sysroot = run(std::process::Command::new(&rustc).args(["--print", "sysroot"]))?;
    let libdir = std::path::Path::new(sysroot.trim()).join("lib").join("rustlib").join(&target).join("lib");
    let rlibs: Vec<_> = std::fs::read_dir(&libdir)
        .map_err(|e| format!("cannot read {}: {}", libdir.display(), e))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("libstd-") && n.ends_with(".rlib")))
        .collect();
    let [rlib] = rlibs.as_slice() else {
        return Err(format!("expected one libstd rlib in {}, found {}", libdir.display(), rlibs.len()));
    };
    let work = out_dir.join("std-objects");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("cannot create {}: {}", work.display(), e))?;
    run(std::process::Command::new("ar").arg("x").arg(rlib).current_dir(&work))?;
    let mut versions = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(&work).map_err(|e| format!("cannot read {}: {}", work.display(), e))? {
        let path = entry.map_err(|e| format!("cannot read {}: {}", work.display(), e))?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("o") {
            continue;
        }
        let load_commands = run(std::process::Command::new("otool").arg("-l").arg(&path))?;
        versions.insert(minimum_version(&load_commands).ok_or_else(|| format!("{} names no minimum macOS version", path.display()))?);
    }
    let _ = std::fs::remove_dir_all(&work);
    match versions.len() {
        1 => Ok(versions.into_iter().next().expect("one version")),
        0 => Err(format!("{} holds no object files", rlib.display())),
        _ => Err(format!("the objects of {} name more than one version: {:?}", rlib.display(), versions)),
    }
}

// `otool -l`'s answer for one object: `minos` of `LC_BUILD_VERSION`, or
// `version` of the older `LC_VERSION_MIN_MACOSX` (the line after `cmdsize`).
fn minimum_version(load_commands: &str) -> Option<String> {
    let mut prev = "";
    for line in load_commands.lines() {
        let mut words = line.split_whitespace();
        let (Some(key), Some(value)) = (words.next(), words.next()) else { continue };
        if key == "minos" || (key == "version" && prev == "cmdsize") {
            return Some(value.to_string()).filter(|v| is_version(v));
        }
        prev = key;
    }
    None
}

fn run(command: &mut std::process::Command) -> Result<String, String> {
    let output = command.output().map_err(|e| format!("cannot run {:?}: {}", command.get_program(), e))?;
    if !output.status.success() {
        return Err(format!("{:?} failed ({})", command.get_program(), output.status));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("{:?} printed something that is not UTF-8", command.get_program()))
}
