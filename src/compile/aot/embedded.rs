//! A board without an operating system (`thumbv7em-none-eabihf`): emitting
//! the module as an ARM object and linking it with `ld.lld` into an ELF
//! executable.
//!
//! The module is the one a native executable is made from, given the
//! target's data layout before the passes run, as for WebAssembly (see
//! `wasm.rs`): a word is an `i64` everywhere, and the tables whose words
//! hold an address are rewritten the same way ([`super::wasm::split_address_words`]).
//!
//! The runtime (`crates/typelisp-rt-embedded`) starts the program: its
//! `cortex-m-rt` vector table and `Reset` ready RAM and call the runtime's
//! entry, which gives the heap its memory and calls the module's `main`
//! under the name [`BOARD_MAIN`]. Where flash and RAM are is the linker
//! script `memory.x`, which `link.x` includes: the user's
//! (`--linker-script`), or [`QEMU_MEMORY`]. The rest of what the runtime asks
//! of the board (`typelisp_host_*`) the runtime gives over semihosting, or
//! the board as C functions in the objects and libraries the link is given
//! (`--board-functions`, `--link`).
//!
//! An interrupt handler runs at once only while the program is in its own
//! compiled code, never part way through a runtime function
//! (`typelisp_rt::interrupt`): every call the module makes into the runtime
//! is made through a function that marks the runtime busy around it
//! ([`busy_around_runtime_calls`]).

use std::fs;
use std::process::Command;

use inkwell::module::{Linkage, Module};
use inkwell::values::AsValueRef;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple};

use super::{embedded_linker, embedded_runtime_dir, BuildOptions, ENTRY_POINT_NAME};
use crate::CompileTarget;

/// What the runtime's entry calls: `main` taking nothing and answering the
/// exit code.
const BOARD_MAIN: &str = "typelisp_main";

/// The memory of the board QEMU's `mps2-an386` is (Arm's AN386, a Cortex-M4
/// on an MPS2 FPGA board): 4 MiB of code memory at 0 and 4 MiB of RAM at
/// `0x20000000`. The stack takes 1 MiB of the RAM, so that recursion goes
/// deep in the tests; the rest above `.bss` is the heap.
const QEMU_MEMORY: &str = "\
MEMORY
{
  FLASH : ORIGIN = 0x00000000, LENGTH = 4M
  RAM : ORIGIN = 0x20000000, LENGTH = 4M
}
_typelisp_stack_size = 1M;
";

/// How much of RAM the stack takes when the user's `memory.x` does not say
/// (`_typelisp_stack_size`).
const DEFAULT_STACK: &str = "PROVIDE(_typelisp_stack_size = 64K);\n";

/// The machine the triple names: a Cortex-M4 with its single-precision FPU,
/// whose code runs from wherever it was linked to.
///
/// The triple is normalized first (`thumbv7em-unknown-none-eabihf`): read as
/// written, `eabihf` would be the third part, the operating system, and the
/// backend would not know the target for an EABI one — it then calls the
/// GNU names of the floating-point helpers (`__divdf3`) with the base calling
/// convention, which the runtime's, built for the hard-float ABI, read their
/// arguments from the wrong registers in.
fn target_machine(triple: &str) -> Result<TargetMachine, String> {
    Target::initialize_arm(&InitializationConfig::default());
    let triple = TargetMachine::normalize_triple(&TargetTriple::create(triple));
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    target
        .create_target_machine(
            &triple,
            "cortex-m4",
            "",
            crate::compile::optimize::codegen_level(),
            RelocMode::Static,
            CodeModel::Default,
        )
        .ok_or_else(|| "failed to create a target machine for thumbv7em-none-eabihf".to_string())
}

/// Puts every function and global `module` defines in a section of its own,
/// as `-ffunction-sections -fdata-sections` do, so that the linker's
/// `--gc-sections` drops each one `main` does not reach: by itself the
/// backend writes them all into one `.text` and one `.data`, which stay
/// whole, and with them the calls the program never makes into shims the
/// board's runtime has not got. The names are the ones `link.x` gathers
/// (`.text.*`, `.rodata.*`, `.data.*`, `.bss.*`).
fn sections_of_their_own(module: &Module<'static>) {
    for f in module.get_functions() {
        if f.count_basic_blocks() > 0 {
            let name = f.get_name().to_string_lossy().into_owned();
            set_section(f.as_value_ref(), &format!(".text.{}", name));
        }
    }
    let mut g = module.get_first_global();
    while let Some(global) = g {
        if let Some(init) = global.get_initializer() {
            let name = global.get_name().to_string_lossy().into_owned();
            let kind = if global.is_constant() {
                "rodata"
            // SAFETY: a live constant of `module`.
            } else if unsafe { inkwell::llvm_sys::core::LLVMIsNull(init.as_value_ref()) } != 0 {
                "bss"
            } else {
                "data"
            };
            set_section(global.as_value_ref(), &format!(".{}.{}", kind, name));
        }
        g = global.get_next_global();
    }
}

/// Makes every call `module` makes into the runtime (a function it declares
/// whose name starts with `rt_`) a call of a function of its own that calls
/// `typelisp_busy_enter`, the runtime function and `typelisp_busy_leave`, so
/// that an interrupt handler waits while the runtime works
/// (`typelisp_rt::interrupt`). A use that is not a call — an address handed
/// to the runtime — takes the same function, which behaves the same.
fn busy_around_runtime_calls(module: &Module<'static>) -> Result<(), String> {
    let ctx = module.get_context();
    let bracket = ctx.void_type().fn_type(&[], false);
    let declared = |name: &str| module.get_function(name).unwrap_or_else(|| module.add_function(name, bracket, None));
    let (enter, leave) = (declared("typelisp_busy_enter"), declared("typelisp_busy_leave"));
    let shims: Vec<_> = module
        .get_functions()
        .filter(|f| f.count_basic_blocks() == 0 && f.get_name().to_bytes().starts_with(b"rt_"))
        .collect();
    let builder = ctx.create_builder();
    let failed = |e: inkwell::builder::BuilderError| format!("failed to build a runtime call's busy bracket: {}", e);
    for shim in shims {
        let name = shim.get_name().to_string_lossy().into_owned();
        let busy = module.add_function(&format!("{}.busy", name), shim.get_type(), Some(Linkage::Internal));
        shim.as_global_value().as_pointer_value().replace_all_uses_with(busy.as_global_value().as_pointer_value());
        builder.position_at_end(ctx.append_basic_block(busy, "entry"));
        builder.build_call(enter, &[], "").map_err(failed)?;
        let args: Vec<_> = busy.get_param_iter().map(|p| p.into()).collect();
        let answer = builder.build_call(shim, &args, "answer").map_err(failed)?;
        builder.build_call(leave, &[], "").map_err(failed)?;
        match answer.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => builder.build_return(Some(&v)).map_err(failed)?,
            inkwell::values::ValueKind::Instruction(_) => builder.build_return(None).map_err(failed)?,
        };
    }
    Ok(())
}

/// Names `value`'s section `name`, as it is. Not inkwell's `set_section`,
/// which on a macOS host puts a comma in front of a name without one (Mach-O
/// spells a section `segment,section`) whatever the module's target is.
fn set_section(value: inkwell::llvm_sys::prelude::LLVMValueRef, name: &str) {
    let name = std::ffi::CString::new(name).expect("a symbol name has no NUL in it");
    // SAFETY: `value` is a live global of the module; LLVM copies the name.
    unsafe { inkwell::llvm_sys::core::LLVMSetSection(value, name.as_ptr()) }
}

/// Emits `module` as an ARM object and links it, with the board's runtime
/// and what `options` say, into an ELF executable at `output_path`. Must be
/// called with [`crate::compile::COMPILE_LOCK`] held.
pub(super) fn write_executable(
    module: &Module<'static>,
    output_path: &str,
    items: &[CompileTarget],
    options: &BuildOptions,
) -> Result<(), String> {
    let triple = super::Target::Thumbv7em.name();
    let runtime = embedded_runtime_dir(triple)?;
    let linker = embedded_linker(&runtime)?;
    for file in &options.link {
        if !file.is_file() {
            return Err(format!("--link: {}: no such file", file.display()));
        }
    }
    let memory_x = match options.linker_script.as_deref() {
        Some(path) => fs::read_to_string(path).map_err(|e| format!("--linker-script: {}: {}", path.display(), e))?,
        None => QEMU_MEMORY.to_string(),
    };
    let tm = target_machine(triple)?;
    module.set_triple(&tm.get_triple());
    module.set_data_layout(&tm.get_target_data().get_data_layout());
    module
        .get_function(ENTRY_POINT_NAME)
        .ok_or("internal error: the module has no `main`")?
        .as_global_value()
        .set_name(BOARD_MAIN);
    super::wasm::split_address_words(module)?;
    crate::compile::codegen_prep::prepare(module)?;
    busy_around_runtime_calls(module)?;
    crate::compile::optimize::run_llvm_passes(module, &tm)?;
    sections_of_their_own(module);

    // The object and the two scripts, beside the executable while it links.
    let scripts = std::path::PathBuf::from(format!("{}.ld", output_path));
    fs::create_dir_all(&scripts).map_err(|e| format!("failed to create {}: {}", scripts.display(), e))?;
    let linked = (|| {
        fs::write(scripts.join("memory.x"), &memory_x).map_err(|e| format!("failed to write memory.x: {}", e))?;
        fs::write(scripts.join("typelisp.x"), DEFAULT_STACK).map_err(|e| format!("failed to write typelisp.x: {}", e))?;
        let object_path = scripts.join("program.o");
        tm.write_to_file(module, FileType::Object, &object_path)
            .map_err(|e| format!("failed to emit object file: {}", e))?;
        for warning in crate::compile::codegen_prep::take_frame_warnings(items) {
            eprintln!("{}", warning);
        }
        let status = Command::new(&linker)
            .args(["-flavor", "gnu"])
            .arg("--gc-sections")
            // `link.x` includes `memory.x`, which is found on the search path.
            .arg("-L")
            .arg(&scripts)
            .arg("-T")
            .arg(runtime.join("link.x"))
            .arg("-T")
            .arg(scripts.join("typelisp.x"))
            .arg(&object_path)
            // lld takes a symbol from an archive wherever on the line the
            // archive is, so the runtime calling the board's functions finds
            // them whatever the order.
            .args(&options.link)
            .arg(runtime.join(options.board_functions.archive()))
            .arg("-o")
            .arg(output_path)
            .status()
            .map_err(|e| format!("failed to invoke {}: {}", linker.display(), e))?;
        if !status.success() {
            return Err(format!("ld.lld failed with status {}", status));
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&scripts);
    linked
}
