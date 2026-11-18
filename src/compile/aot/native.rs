//! The host machine: its target, and emitting an object file and linking it
//! into an executable with the system `cc`.

use std::fs;
use std::process::Command;
use inkwell::module::Module;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple};
use crate::CompileTarget;
use super::link_archive;

/// A [`TargetMachine`] for the machine this process is running on.
///
/// Shared by the two things that turn a module into machine code:
/// [`write_executable`], which writes an object file and links it, and
/// `driver::disassemble_function`, which writes assembly text and shows it to
/// a person. One target machine and one set of options, so what
/// `disassemble` prints is the code `compile-file` would actually emit —
/// including the optimization level ([`crate::compile::optimize`]), which at
/// `0` is why the assembly reads the way the IR does. Its callers prepare the
/// module first ([`crate::compile::codegen_prep::prepare`]) and then run the
/// level's passes over it, as the JIT does.
fn host_target_machine() -> Result<TargetMachine, String> {
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| format!("failed to initialize native target: {}", e))?;
    let triple = host_triple()?;
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    target
        .create_target_machine(
            &triple,
            &TargetMachine::get_host_cpu_name().to_string(),
            &TargetMachine::get_host_cpu_features().to_string(),
            crate::compile::optimize::codegen_level(),
            reloc_mode(),
            CodeModel::Default,
        )
        .ok_or_else(|| "failed to create a target machine for the host triple".to_string())
}

/// The host triple, with the minimum macOS version the executable is linked
/// for (see [`macos_version_min`]). LLVM's default names the running OS
/// (`x86_64-apple-darwin24.6.0`), which would make the object claim a newer
/// macOS than the static library beside it and the executable it goes into;
/// it is used only when this build has no version to name.
#[cfg(target_os = "macos")]
fn host_triple() -> Result<TargetTriple, String> {
    let default = TargetMachine::get_default_triple();
    let default = default.as_str().to_string_lossy();
    let arch = default
        .split_once("-apple-")
        .map(|(arch, _)| arch)
        .ok_or_else(|| format!("internal error: the host triple `{}` is not an Apple one", default))?;
    match macos_version_min() {
        Some(version) => Ok(TargetTriple::create(&format!("{}-apple-macosx{}", arch, version))),
        None => Ok(TargetMachine::get_default_triple()),
    }
}

#[cfg(not(target_os = "macos"))]
fn host_triple() -> Result<TargetTriple, String> {
    Ok(TargetMachine::get_default_triple())
}

/// Linux: position-independent code. `cc` links a PIE there by default, and
/// LLVM's default for an ELF target is static code, which the linker can put
/// into a PIE only by leaving relocations in `.text` (`DT_TEXTREL`).
#[cfg(target_os = "linux")]
fn reloc_mode() -> RelocMode {
    RelocMode::PIC
}

#[cfg(not(target_os = "linux"))]
fn reloc_mode() -> RelocMode {
    RelocMode::Default
}

/// The system libraries the static library needs, which `cc` does not add
/// by itself on Linux — rustc's `--print native-static-libs` for
/// `typelisp-front`, without the `-lc` `cc` adds anyway. macOS needs none:
/// `cc` links libSystem, which holds them all.
#[cfg(target_os = "linux")]
const SYSTEM_LIBRARIES: &[&str] = &["-lgcc_s", "-lutil", "-lrt", "-lpthread", "-lm", "-ldl"];

#[cfg(not(target_os = "linux"))]
const SYSTEM_LIBRARIES: &[&str] = &[];

/// The minimum macOS version this build's static library was compiled for,
/// which is what its objects claim — `compile-file` links for the same one
/// rather than Apple clang's default (`build.rs` says why). `None` when
/// `build.rs` had none: neither set nor readable from std. The link then
/// takes the tools' defaults and may warn, but still works.
#[cfg(target_os = "macos")]
fn macos_version_min() -> Option<&'static str> {
    option_env!("TYPELISP_MACOSX_DEPLOYMENT_TARGET")
}

/// `module` as host assembly text — `(disassemble name)`'s answer.
///
/// Emitted to memory rather than to a file: the caller is showing it to a
/// person, not linking it, so there is nothing for a temporary file to be
/// for. Must be called with [`crate::compile::COMPILE_LOCK`] held, like every
/// other use of the shared LLVM context.
pub(crate) fn assembly_of(module: &Module<'static>) -> Result<String, String> {
    crate::compile::codegen_prep::prepare(module)?;
    let tm = host_target_machine()?;
    crate::compile::optimize::run_llvm_passes(module, &tm)?;
    let buf = tm
        .write_to_memory_buffer(module, FileType::Assembly)
        .map_err(|e| format!("failed to emit assembly: {}", e))?;
    // Already said when the function was compiled; showing its code is not a
    // second occasion to.
    crate::compile::codegen_prep::take_frame_warnings(&[]);
    String::from_utf8(buf.as_slice().to_vec()).map_err(|e| format!("the assembler emitted invalid UTF-8: {}", e))
}

/// Emits `module` to an object file and links it into a native executable
/// at `output_path` via the system `cc`. Must be called with
/// [`crate::compile::COMPILE_LOCK`] held.
pub(super) fn write_executable(module: &Module<'static>, output_path: &str, libraries: &[String], items: &[CompileTarget]) -> Result<(), String> {
    // The whole program's IR, for reading. An AOT module is the one the
    // island's output is hardest to see: it is neither `disassemble`'s
    // single function nor a committed dump, and by the time anything is
    // wrong it is machine code. `TYPELISP_AOT_IR=<path>` writes it here.
    if let Ok(path) = std::env::var("TYPELISP_AOT_IR") {
        std::fs::write(&path, module.print_to_string().to_string())
            .map_err(|e| format!("failed to write {}: {}", path, e))?;
    }
    crate::compile::codegen_prep::prepare(module)?;
    let target_machine = host_target_machine()?;
    crate::compile::optimize::run_llvm_passes(module, &target_machine)?;

    // Named from `output_path` (not e.g. the process id) so it can't
    // collide across concurrently-running `cargo test` threads in the same
    // process — every caller already needs a distinct `output_path` of its
    // own, for the same reason.
    let object_path = format!("{}.o", output_path);
    target_machine
        .write_to_file(module, FileType::Object, std::path::Path::new(&object_path))
        .map_err(|e| format!("failed to emit object file: {}", e))?;
    for warning in crate::compile::codegen_prep::take_frame_warnings(items) {
        eprintln!("{}", warning);
    }

    // `-l` for every library a `defffi` named. Nothing is needed for a
    // declaration without one: what it reaches is already linked (libc comes
    // with `cc`, and the rest is in the static library beside it).
    let archive = match link_archive() {
        Ok(archive) => archive,
        Err(e) => {
            let _ = fs::remove_file(&object_path);
            return Err(e);
        }
    };
    let mut link = Command::new("cc");
    #[cfg(target_os = "macos")]
    if let Some(version) = macos_version_min() {
        link.arg(format!("-mmacosx-version-min={}", version));
    }
    let status = link
        .arg(&object_path)
        .arg(&archive)
        .args(libraries.iter().map(|l| format!("-l{}", l)))
        .args(SYSTEM_LIBRARIES)
        .arg("-o")
        .arg(output_path)
        .status()
        .map_err(|e| format!("failed to invoke the system linker (`cc`): {}", e));
    let _ = fs::remove_file(&object_path);
    let status = status?;

    if !status.success() {
        // The linker's own message names only an undefined symbol; say what
        // it means when the archive is of another ABI version. Read only on
        // this path, where the cost of reading the archive does not matter.
        let marker = typelisp_abi::ABI_SYMBOL.as_bytes();
        if fs::read(&archive).is_ok_and(|bytes| !bytes.windows(marker.len()).any(|w| w == marker)) {
            return Err(format!(
                "linker failed with status {}: {} is not of ABI version {} (it does not define {}), so it was built \
                 for another typl",
                status,
                archive.display(),
                typelisp_abi::ABI_VERSION,
                typelisp_abi::ABI_SYMBOL
            ));
        }
        return Err(format!("linker failed with status {}", status));
    }
    Ok(())
}
