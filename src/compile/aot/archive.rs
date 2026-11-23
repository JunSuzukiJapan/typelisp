//! Where the runtime archive an executable links against comes from: the
//! workspace's build, or the copy a release `typl` carries and writes out on
//! first use, and removing those copies.

use std::fs;

/// The `typelisp-rt` crate's `staticlib` artifact for one link, which
/// exports the `#[no_mangle]` runtime shims in [`crate::compile::runtime`]
/// (`rt_ping`/`rt_heap_init`/`rt_heap_live_count`, and from Stage 3 onward
/// `rt_cons`/`rt_car`/...) as plain C symbols. Linked into every AOT
/// executable below so calls to those shims resolve the same way a call to
/// another `defun` in the file does — see `runtime`'s module doc comment.
///
/// Deliberately `typelisp-front`'s artifact, not this (`typelisp`) crate's —
/// `typelisp` embeds all of LLVM via `inkwell`, and linking *that* into a
/// tiny AOT executable drags in LLVM's entire system-library footprint
/// (`libc++`, zlib, libffi, terminfo, ...) for no benefit; nothing at or
/// below `typelisp-front` names LLVM at all.
///
/// It is `typelisp-front`'s and not `typelisp-rt`'s only because `cc` takes
/// *one* archive and `typelisp-front` sits above `typelisp-rt`, so its
/// artifact is the one that holds both. Front's own objects — the checker,
/// the interpreter, the prelude source — are what `rt_eval` needs and
/// nothing else does, and the linker leaves every one of them out of a
/// program that does not call it: measured at +784 bytes and zero
/// `typelisp_front` symbols for `(defun main () i32 42)` against linking
/// `typelisp-rt` alone. That is the whole reason the front end is a crate;
/// see its doc comment, and `Cargo.toml`'s `profile.dev.package` entries for
/// the one thing that quietly breaks it.
///
/// Where the archive comes from depends on how `typl` was built, and is
/// fixed when it is built — never chosen at link time:
///
/// - A release build carries the archive inside itself (`build.rs`'s
///   `bundled_runtime`) and writes it out once, at the first link that needs
///   it, to `$TYPELISP_HOME/lib/<build id>/` ([`bundled_archive_dir`]); every
///   later link uses that file. `cargo install` keeps nothing of the tree it
///   built in, so this is what makes an installed `typl` able to compile at
///   all.
/// - A debug build links the one beside its own artifacts,
///   `<target dir>/debug/` — `CARGO_TARGET_DIR` when that is set. The
///   workspace shares one target dir, so this crate's is every member's;
///   `build.rs`'s `profile_dir` reads it from `OUT_DIR`.
/// - A release build made with `TYPELISP_LINK_TREE_RUNTIME` set does what a
///   debug one does, with `<target dir>/release/`.
///
/// Whichever it is, the linker refuses an archive of another ABI version
/// ([`typelisp_abi::ABI_SYMBOL`], referenced from [`build_main_wrapper`]).
///
/// **In a debug build this artifact is not built by the `cargo` invocation
/// that runs an AOT test.** `cargo test` builds `typelisp-front`'s *rlib* (the
/// dependency this crate links) and never its `staticlib` target, so what is
/// on disk here is whatever the last `cargo build -p typelisp-front` / `cargo
/// build --workspace` left. Adding an `rt_*` shim therefore links against a
/// runtime that predates it and fails with an undefined symbol — observed
/// adding `rt_apply_any`, where the JIT tests all passed and only AOT broke.
/// `scripts/test-serial.sh` builds the staticlib first for exactly this
/// reason; a bare `cargo test --test compile_file_test` needs it built by
/// hand.
///
/// `typl --lib-dir DIR` replaces both with `DIR/`[`STATICLIB_NAME`]
/// ([`set_lib_dir`]).
pub(super) fn link_archive() -> Result<std::path::PathBuf, String> {
    if let Some(dir) = LIB_DIR.get() {
        return Ok(dir.join(STATICLIB_NAME));
    }
    default_archive()
}

/// The archive `build.rs` built and this binary carries; see [`link_archive`].
#[cfg(typelisp_bundled_runtime)]
static BUNDLED_STATICLIB: &[u8] = include_bytes!(env!("TYPELISP_BUNDLED_STATICLIB"));

#[cfg(typelisp_bundled_runtime)]
fn default_archive() -> Result<std::path::PathBuf, String> {
    write_archive_once(&bundled_archive_dir()?, BUNDLED_STATICLIB)
}

#[cfg(not(typelisp_bundled_runtime))]
fn default_archive() -> Result<std::path::PathBuf, String> {
    Ok(std::path::PathBuf::from(env!("TYPELISP_PROFILE_DIR")).join(STATICLIB_NAME))
}

/// `$TYPELISP_HOME/lib/<build id>`, where this build's archive is written.
///
/// The build id is a hash of the archive's bytes (`build.rs`), because an
/// archive works only with the `typl` built together with it: a `typl` built
/// again gets a folder of its own, and an existing file can be trusted
/// without looking inside it.
#[cfg(typelisp_bundled_runtime)]
fn bundled_archive_dir() -> Result<std::path::PathBuf, String> {
    Ok(typelisp_home()?.join("lib").join(env!("TYPELISP_BUNDLED_STATICLIB_ID")))
}

/// `TYPELISP_HOME`, or `~/.typelisp` when it is not set — the same shape as
/// `CARGO_HOME` and `~/.cargo`.
fn typelisp_home() -> Result<std::path::PathBuf, String> {
    if let Some(home) = std::env::var_os("TYPELISP_HOME").filter(|h| !h.is_empty()) {
        return Ok(std::path::PathBuf::from(home));
    }
    match std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        Some(home) => Ok(std::path::PathBuf::from(home).join(".typelisp")),
        None => Err("neither TYPELISP_HOME nor HOME is set, so there is nowhere to put the library \
                     compiled executables link; set TYPELISP_HOME or pass --lib-dir"
            .to_string()),
    }
}

/// `dir/`[`STATICLIB_NAME`], written from `bytes` unless it is already there.
///
/// Written to a file of its own in the same folder and renamed into place, so
/// a `typl` that dies mid-write leaves no half-written archive under the
/// final name, and two writing at once each rename a whole one.
#[cfg(any(typelisp_bundled_runtime, test))]
pub(super) fn write_archive_once(dir: &std::path::Path, bytes: &[u8]) -> Result<std::path::PathBuf, String> {
    write_file_once(dir, STATICLIB_NAME, bytes)
}

/// [`write_archive_once`] for any of the files a build carries: `dir/name`
/// holding `bytes`, written once.
#[cfg(any(typelisp_bundled_runtime, test))]
pub(super) fn write_file_once(dir: &std::path::Path, name: &str, bytes: &[u8]) -> Result<std::path::PathBuf, String> {
    let path = dir.join(name);
    if path.is_file() {
        return Ok(path);
    }
    fs::create_dir_all(dir).map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
    static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let partial = dir.join(format!(
        ".{}.{}.{}",
        name,
        std::process::id(),
        WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    if let Err(e) = fs::write(&partial, bytes) {
        let _ = fs::remove_file(&partial);
        return Err(format!("failed to write {}: {}", partial.display(), e));
    }
    fs::rename(&partial, &path).map_err(|e| {
        let _ = fs::remove_file(&partial);
        format!("failed to move {} to {}: {}", partial.display(), path.display(), e)
    })?;
    Ok(path)
}

/// Which of the archives release `typl`s wrote `typl --remove-lib` removes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoveLib {
    /// This build's (`--remove-lib`).
    This,
    /// Every build's but this one's (`--remove-lib --others`): what is left
    /// behind by `typl`s installed before this one.
    Others,
    /// Every build's (`--remove-lib --all`).
    All,
}

/// `typl --remove-lib`: removes the build-id folders under
/// `$TYPELISP_HOME/lib` that `which` names, and answers with the ones it
/// removed.
///
/// Only folders named like a build id are touched; anything else a person put
/// under `lib` is not `typl`'s to remove. A debug build has no build id of its
/// own, so only [`RemoveLib::All`] means anything to it.
pub fn remove_written_archives(which: RemoveLib) -> Result<Vec<std::path::PathBuf>, String> {
    let own = own_build_id();
    if own.is_none() && which != RemoveLib::All {
        return Err("this typl is a debug build: it links the library in the tree it was built in, \
                    has written none and has no build id of its own; --all removes every build's"
            .to_string());
    }
    remove_archives_under(&typelisp_home()?.join("lib"), own, which)
}

/// [`remove_written_archives`] on the folder `root` for a build whose id is
/// `own`.
pub(super) fn remove_archives_under(
    root: &std::path::Path,
    own: Option<&str>,
    which: RemoveLib,
) -> Result<Vec<std::path::PathBuf>, String> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("failed to read {}: {}", root.display(), e)),
    };
    let mut removed = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("failed to read {}: {}", root.display(), e))?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|n| is_build_id(n)) else { continue };
        let is_own = own == Some(name);
        let chosen = match which {
            RemoveLib::This => is_own,
            RemoveLib::Others => !is_own,
            RemoveLib::All => true,
        };
        let path = entry.path();
        if !chosen || !path.is_dir() {
            continue;
        }
        fs::remove_dir_all(&path).map_err(|e| format!("failed to remove {}: {}", path.display(), e))?;
        removed.push(path);
    }
    removed.sort();
    Ok(removed)
}

#[cfg(typelisp_bundled_runtime)]
fn own_build_id() -> Option<&'static str> {
    Some(env!("TYPELISP_BUNDLED_STATICLIB_ID"))
}

#[cfg(not(typelisp_bundled_runtime))]
fn own_build_id() -> Option<&'static str> {
    None
}

/// The shape `build.rs` gives a build id: 16 lowercase hex digits.
fn is_build_id(name: &str) -> bool {
    name.len() == 16 && name.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The file name of the archive every AOT executable links; see
/// [`link_archive`].
pub const STATICLIB_NAME: &str = "libtypelisp_front.a";

/// The folder `typl --lib-dir` named, for the rest of the process.
static LIB_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Makes every later `compile-file` in this process link `dir`'s
/// [`STATICLIB_NAME`] instead of the default one ([`link_archive`]).
///
/// Refuses a folder that does not hold the archive: the default one is not
/// consulted once a folder has been named, so a mistyped one would otherwise
/// surface only as a linker error at the first `compile-file`.
pub fn set_lib_dir(dir: std::path::PathBuf) -> Result<(), String> {
    if !dir.join(STATICLIB_NAME).is_file() {
        return Err(format!("{} has no {}", dir.display(), STATICLIB_NAME));
    }
    LIB_DIR.set(dir).map_err(|_| "the library folder is already set".to_string())
}

/// The folder holding what a WebAssembly module for `triple` links: the
/// runtime archive built for that target, and for `wasm32-wasip1` WASI's C
/// library and start file from the same Rust toolchain.
///
/// Where it is depends on how `typl` was built, as for the native archive
/// ([`link_archive`]):
///
/// - A release build with the `wasm` feature carries them and writes them
///   out once, to `$TYPELISP_HOME/lib/<build id>/<triple>/`.
/// - A release build without it carries none, and `--target` is refused.
/// - A debug build links the ones `scripts/build-wasm-runtime.sh` built in its
///   tree, `<target dir>/wasm-runtime/<profile>/<triple>/`, beside which the
///   script also links the toolchain's linker ([`wasm_linker`]).
pub(super) fn wasm_runtime_dir(triple: &str) -> Result<std::path::PathBuf, String> {
    #[cfg(typelisp_bundled_wasm)]
    {
        let dir = bundled_archive_dir()?.join(triple);
        match triple {
            "wasm32-wasip1" => {
                write_file_once(&dir, STATICLIB_NAME, include_bytes!(env!("TYPELISP_BUNDLED_WASM_WASIP1_STATICLIB")))?;
                write_file_once(&dir, "crt1-command.o", include_bytes!(env!("TYPELISP_BUNDLED_WASIP1_CRT1")))?;
                write_file_once(&dir, "libc.a", include_bytes!(env!("TYPELISP_BUNDLED_WASIP1_LIBC")))?;
            }
            "wasm32-unknown-unknown" => {
                write_file_once(&dir, STATICLIB_NAME, include_bytes!(env!("TYPELISP_BUNDLED_WASM_UNKNOWN_STATICLIB")))?;
            }
            _ => return Err(format!("internal error: no WebAssembly runtime for {}", triple)),
        }
        Ok(dir)
    }
    #[cfg(all(typelisp_bundled_runtime, not(typelisp_bundled_wasm)))]
    {
        let _ = triple;
        Err("this typl was built without WebAssembly support: --target needs one built with `--features wasm` \
             (`cargo install --features wasm ...`)"
            .to_string())
    }
    #[cfg(not(typelisp_bundled_runtime))]
    {
        let profile_dir = std::path::Path::new(env!("TYPELISP_PROFILE_DIR"));
        let (Some(target_dir), Some(profile)) = (profile_dir.parent(), profile_dir.file_name()) else {
            return Err(format!("internal error: {} is not <target dir>/<profile>", profile_dir.display()));
        };
        let dir = target_dir.join("wasm-runtime").join(profile).join(triple);
        if !dir.join(STATICLIB_NAME).is_file() {
            return Err(format!(
                "{} has no WebAssembly runtime; build it with scripts/build-wasm-runtime.sh {}",
                dir.display(),
                profile.to_string_lossy()
            ));
        }
        Ok(dir)
    }
}

/// The folder `typl -c --target <triple>` links an executable for a board
/// from: the runtime built for the triple (`libtypelisp_rt_embedded.a`),
/// `cortex-m-rt`'s linker script (`link.x`), and the linker (`ld.lld`).
///
/// A debug build links the ones `scripts/build-embedded-runtime.sh` built in
/// its tree, `<target dir>/embedded-runtime/<profile>/<triple>/`. A release
/// build carries none, and `--target` is refused.
pub(super) fn embedded_runtime_dir(triple: &str) -> Result<std::path::PathBuf, String> {
    #[cfg(typelisp_bundled_runtime)]
    {
        let _ = triple;
        Err("this typl carries no runtime for a board: --target thumbv7em-none-eabihf needs a typl built in its \
             source tree, with scripts/build-embedded-runtime.sh run"
            .to_string())
    }
    #[cfg(not(typelisp_bundled_runtime))]
    {
        let profile_dir = std::path::Path::new(env!("TYPELISP_PROFILE_DIR"));
        let (Some(target_dir), Some(profile)) = (profile_dir.parent(), profile_dir.file_name()) else {
            return Err(format!("internal error: {} is not <target dir>/<profile>", profile_dir.display()));
        };
        let dir = target_dir.join("embedded-runtime").join(profile).join(triple);
        for file in [EMBEDDED_STATICLIB_NAME, "link.x", "ld.lld"] {
            if !dir.join(file).exists() {
                return Err(format!(
                    "{} has no {}; build the runtime for a board with scripts/build-embedded-runtime.sh {}",
                    dir.display(),
                    file,
                    profile.to_string_lossy()
                ));
            }
        }
        Ok(dir)
    }
}

/// The runtime archive in [`embedded_runtime_dir`].
pub(super) const EMBEDDED_STATICLIB_NAME: &str = "libtypelisp_rt_embedded.a";

/// The linker for a WebAssembly module whose runtime is in `runtime`
/// ([`wasm_runtime_dir`]), run with `-flavor wasm`.
///
/// `rust-lld`, which no build carries: it is linked against its toolchain's
/// own `libLLVM` and does not run apart from it. A debug build's runtime
/// folder has the one of the toolchain that built the runtime beside it
/// (`scripts/build-wasm-runtime.sh`); a release build's has none, and the
/// one of rustup's default toolchain is run — a toolchain with these targets
/// is what building with the `wasm` feature took.
pub(super) fn wasm_linker(runtime: &std::path::Path) -> Result<std::path::PathBuf, String> {
    if !cfg!(typelisp_bundled_runtime) {
        return Ok(runtime.join("wasm-ld"));
    }
    let run = |cmd: &mut std::process::Command, what: &str| -> Result<String, String> {
        let out = cmd.output().map_err(|e| format!("--target needs rustup to find its linker (rust-lld): {}: {}", what, e))?;
        if !out.status.success() {
            return Err(format!("--target needs rustup to find its linker (rust-lld): {} failed: {}", what, String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let rustc = run(std::process::Command::new("rustup").args(["which", "rustc"]), "rustup which rustc")?;
    let sysroot = run(std::process::Command::new(&rustc).args(["--print", "sysroot"]), "rustc --print sysroot")?;
    let host = run(std::process::Command::new(&rustc).args(["--print", "host-tuple"]), "rustc --print host-tuple")?;
    let lld = std::path::Path::new(&sysroot).join("lib").join("rustlib").join(&host).join("bin").join("rust-lld");
    if !lld.is_file() {
        return Err(format!("the rustup toolchain at {} has no linker for WebAssembly ({})", sysroot, lld.display()));
    }
    Ok(lld)
}
