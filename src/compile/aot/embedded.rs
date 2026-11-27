//! A board without an operating system (`thumbv7em-none-eabihf`,
//! `thumbv8m.main-none-eabihf`, `riscv32imac-unknown-none-elf`): emitting the
//! module as an object for the board's core and linking it with `ld.lld`
//! into an ELF executable.
//!
//! What a board is ([`Board`]) is data: the core, how LLVM names it, the
//! interrupts a program may handle, and the QEMU board that stands for it.
//! What is done with one — the target machine, the memory, the module made
//! ready for the board, the link — is the same for every board, AVR's too
//! (`avr.rs`), which adds only what having no runtime asks.
//!
//! The module is the one a native executable is made from, emitted with the
//! target's word and given the target's data layout before the passes run, as
//! for WebAssembly (see `wasm.rs`).
//!
//! The runtime (`crates/typelisp-rt-embedded`) starts the program: the
//! core's startup crate (`cortex-m-rt`'s vector table and `Reset`,
//! `riscv-rt`'s `_start`) readies RAM and calls the runtime's entry, which
//! gives the heap its memory and calls the module's `main` under the name
//! [`BOARD_MAIN`]. Where flash and RAM are is the linker script `memory.x`,
//! which that crate's `link.x` includes: the user's
//! (`--linker-script`), or the QEMU board's ([`Board::qemu_memory`]). The rest of what the runtime asks
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
use std::path::Path;
use std::process::Command;

use inkwell::module::{Linkage, Module};
use inkwell::values::AsValueRef;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple};
use inkwell::OptimizationLevel;

use super::{embedded_linker, embedded_runtime_dir, BuildOptions, ENTRY_POINT_NAME};
use crate::CompileTarget;

/// What the runtime's entry calls: `main` taking nothing and answering the
/// exit code.
pub(super) const BOARD_MAIN: &str = "typelisp_main";

/// The family of cores a board's is: what LLVM's back end for it is, and
/// what starts a program and takes its interrupts (`typelisp-rt-embedded`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Core {
    /// An Arm Cortex-M (`cortex-m-rt`, the NVIC).
    CortexM,
    /// A 32-bit RISC-V core in machine mode (`riscv-rt`, `mie`).
    Riscv32,
    /// An AVR microcontroller, which has no runtime ([`super::avr`]).
    Avr,
}

impl Core {
    /// Registers LLVM's back end for the core. Again does nothing.
    fn initialize(self) {
        let config = InitializationConfig::default();
        match self {
            Core::CortexM => Target::initialize_arm(&config),
            Core::Riscv32 => Target::initialize_riscv(&config),
            Core::Avr => super::avr::initialize(),
        }
    }
}

/// A target that is a board: the core its code is for, the interrupts a
/// program may handle on it, and the QEMU board that stands for it, whose
/// memory a program is linked for when the user gives no linker script.
pub struct Board {
    /// The family of the core.
    pub core: Core,
    /// The target, as LLVM names it (normalized before use).
    pub llvm_triple: &'static str,
    /// The core, as LLVM names it.
    pub cpu: &'static str,
    /// The core's extensions LLVM is to use, as LLVM spells them.
    pub features: &'static str,
    /// The interrupts `#[interrupt(N)]` may name, the lowest and the highest.
    pub interrupts: (u16, u16),
    /// Whether the core has the exceptions `#[exception(NAME)]` names (a
    /// Cortex-M's).
    pub exceptions: bool,
    /// The QEMU that emulates the core, and the arguments that pick the
    /// board (`-M ...`) before `-kernel`.
    pub qemu: (&'static str, &'static [&'static str]),
    /// The QEMU board's memory, as a `memory.x`. The stack takes 1 MiB of
    /// the RAM, so that recursion goes deep in the tests; the rest above
    /// `.bss` is the heap.
    pub qemu_memory: &'static str,
}

impl Board {
    /// The machine the board's core is, whose code runs from wherever it was
    /// linked to, its code generated at `level`.
    ///
    /// The triple is normalized first (`thumbv7em-unknown-none-eabihf`):
    /// read as written, `eabihf` would be the third part, the operating
    /// system, and the backend would not know the target for an EABI one —
    /// it then calls the GNU names of the floating-point helpers
    /// (`__divdf3`) with the base calling convention, which the runtime's,
    /// built for the hard-float ABI, read their arguments from the wrong
    /// registers in.
    pub(super) fn target_machine(&self, level: OptimizationLevel) -> Result<TargetMachine, String> {
        self.core.initialize();
        let triple = TargetMachine::normalize_triple(&TargetTriple::create(self.llvm_triple));
        let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
        target
            .create_target_machine(&triple, self.cpu, self.features, level, RelocMode::Static, CodeModel::Default)
            .ok_or_else(|| format!("failed to create a target machine for {}", triple))
    }

    /// The interrupt the handler `path` is for on `target`, its attribute
    /// having named the vector-table entry `vector` (`typeck::attribute`: a
    /// Cortex-M's exceptions, then interrupt `n` at 16 + `n`), refused when
    /// the core has no such interrupt. A core exception's is the entry
    /// itself.
    pub(super) fn handled(&self, target: super::Target, vector: u16, path: &crate::types::Path) -> Result<u16, String> {
        let (lowest, highest) = self.interrupts;
        match vector.checked_sub(FIRST_INTERRUPT) {
            None if self.exceptions => Ok(vector),
            None => Err(format!("`{}` handles a core exception, and --target {} has none", path, target.name())),
            Some(n) if (lowest..=highest).contains(&n) => Ok(n),
            Some(n) => Err(format!(
                "`{}` handles interrupt {}: the interrupts of --target {} are {} to {}",
                path,
                n,
                target.name(),
                lowest,
                highest
            )),
        }
    }
}

/// The vector-table entry of interrupt `0`, as a handler's attribute has it
/// (`typeck::attribute`): a Cortex-M's exceptions come before it.
const FIRST_INTERRUPT: u16 = 16;

/// `thumbv7em-none-eabihf`, on QEMU's `mps2-an386` (Arm's AN386, a
/// Cortex-M4 on an MPS2 FPGA board): 4 MiB of code memory at 0 and 4 MiB of
/// RAM at `0x20000000`.
pub const CORTEX_M4: Board = Board {
    core: Core::CortexM,
    llvm_triple: "thumbv7em-none-eabihf",
    cpu: "cortex-m4",
    features: "",
    interrupts: (0, 239),
    exceptions: true,
    qemu: ("qemu-system-arm", &["-M", "mps2-an386"]),
    qemu_memory: "\
MEMORY
{
  FLASH : ORIGIN = 0x00000000, LENGTH = 4M
  RAM : ORIGIN = 0x20000000, LENGTH = 4M
}
_typelisp_stack_size = 1M;
",
};

/// `thumbv8m.main-none-eabihf`, on QEMU's `mps2-an505` (Arm's AN505, a
/// Cortex-M33 with TrustZone): the core starts in the secure state, so the
/// program is linked at the secure addresses of the 4 MiB of code memory
/// (`0x10000000`) and of the 4 MiB of RAM (`0x38000000`).
pub const CORTEX_M33: Board = Board {
    core: Core::CortexM,
    llvm_triple: "thumbv8m.main-none-eabihf",
    cpu: "cortex-m33",
    features: "",
    interrupts: (0, 239),
    exceptions: true,
    qemu: ("qemu-system-arm", &["-M", "mps2-an505"]),
    qemu_memory: "\
MEMORY
{
  FLASH : ORIGIN = 0x10000000, LENGTH = 4M
  RAM : ORIGIN = 0x38000000, LENGTH = 4M
}
_typelisp_stack_size = 1M;
",
};

/// `riscv32imac-unknown-none-elf`, on QEMU's `virt` board with a 32-bit
/// core and no firmware (`-bios none`): RAM at `0x80000000`, where the core
/// starts, and the program loaded into it, code and data alike. `riscv-rt`'s
/// `link.x` places each kind of section in the region its `REGION_*` alias
/// names. The interrupts are the 32 `mie` has a bit for.
pub const RISCV32_VIRT: Board = Board {
    core: Core::Riscv32,
    llvm_triple: "riscv32-unknown-none-elf",
    cpu: "generic-rv32",
    features: "+m,+a,+c",
    interrupts: (0, 31),
    exceptions: false,
    qemu: ("qemu-system-riscv32", &["-M", "virt", "-bios", "none"]),
    qemu_memory: "\
MEMORY
{
  RAM : ORIGIN = 0x80000000, LENGTH = 16M
}
REGION_ALIAS(\"REGION_TEXT\", RAM);
REGION_ALIAS(\"REGION_RODATA\", RAM);
REGION_ALIAS(\"REGION_DATA\", RAM);
REGION_ALIAS(\"REGION_BSS\", RAM);
REGION_ALIAS(\"REGION_HEAP\", RAM);
REGION_ALIAS(\"REGION_STACK\", RAM);
_typelisp_stack_size = 1M;
",
};

/// How much of RAM the stack takes when the user's `memory.x` does not say
/// (`_typelisp_stack_size`).
const DEFAULT_STACK: &str = "PROVIDE(_typelisp_stack_size = 64K);\n";

/// What says where the board's flash and RAM are: the user's linker script
/// (`--linker-script`), or the QEMU board's. Refuses a `--link` file that is
/// not there, before anything is emitted.
pub(super) fn board_memory(board: &Board, options: &BuildOptions) -> Result<String, String> {
    for file in &options.link {
        if !file.is_file() {
            return Err(format!("--link: {}: no such file", file.display()));
        }
    }
    match options.linker_script.as_deref() {
        Some(path) => fs::read_to_string(path).map_err(|e| format!("--linker-script: {}: {}", path.display(), e)),
        None => Ok(board.qemu_memory.to_string()),
    }
}

/// Gives `module` the machine `tm`'s triple and data layout, and its `main`
/// the name the board's start calls ([`BOARD_MAIN`]).
pub(super) fn for_board(module: &Module<'static>, tm: &TargetMachine) -> Result<(), String> {
    module.set_triple(&tm.get_triple());
    module.set_data_layout(&tm.get_target_data().get_data_layout());
    module
        .get_function(ENTRY_POINT_NAME)
        .ok_or("internal error: the module has no `main`")?
        .as_global_value()
        .set_name(BOARD_MAIN);
    Ok(())
}

/// Runs `link` with a folder of its own beside the executable at
/// `output_path`, for the object and the scripts while the executable links,
/// and takes the folder away after.
pub(super) fn beside_output<T>(output_path: &str, link: impl FnOnce(&Path) -> Result<T, String>) -> Result<T, String> {
    let scratch = std::path::PathBuf::from(format!("{}.ld", output_path));
    fs::create_dir_all(&scratch).map_err(|e| format!("failed to create {}: {}", scratch.display(), e))?;
    let linked = link(&scratch);
    let _ = fs::remove_dir_all(&scratch);
    linked
}

/// Writes `module` as an object at `path`, and the warnings its frames gave
/// about `items` once it has been.
pub(super) fn write_object(tm: &TargetMachine, module: &Module<'static>, path: &Path, items: &[CompileTarget]) -> Result<(), String> {
    tm.write_to_file(module, FileType::Object, path).map_err(|e| format!("failed to emit object file: {}", e))?;
    for warning in crate::compile::codegen_prep::take_frame_warnings(items) {
        eprintln!("{}", warning);
    }
    Ok(())
}

/// Runs `ld.lld` (`linker`, as the GNU linker), dropping the sections
/// nothing uses, with `args`.
pub(super) fn run_linker(linker: &Path, args: impl FnOnce(&mut Command) -> &mut Command) -> Result<(), String> {
    let mut command = Command::new(linker);
    command.args(["-flavor", "gnu", "--gc-sections"]);
    let status = args(&mut command).status().map_err(|e| format!("failed to invoke {}: {}", linker.display(), e))?;
    if !status.success() {
        return Err(format!("ld.lld failed with status {}", status));
    }
    Ok(())
}

/// Puts every function and global `module` defines in a section of its own,
/// as `-ffunction-sections -fdata-sections` do, so that the linker's
/// `--gc-sections` drops each one `main` does not reach: by itself the
/// backend writes them all into one `.text` and one `.data`, which stay
/// whole, and with them the calls the program never makes into shims the
/// board's runtime has not got. The names are the ones `link.x` gathers
/// (`.text.*`, `.rodata.*`, `.data.*`, `.bss.*`).
pub(super) fn sections_of_their_own(module: &Module<'static>) {
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

/// Emits `module` as an object for the board's core and links it, with the
/// board's runtime and what `options` say, into an ELF executable at
/// `output_path`. Must be called with [`crate::compile::COMPILE_LOCK`] held.
pub(super) fn write_executable(
    module: &Module<'static>,
    output_path: &str,
    items: &[CompileTarget],
    options: &BuildOptions,
) -> Result<(), String> {
    let triple = options.target.name();
    let board = options.target.board().ok_or_else(|| format!("internal error: {} is not a board", triple))?;
    let runtime = embedded_runtime_dir(triple)?;
    let linker = embedded_linker(&runtime)?;
    let memory_x = board_memory(board, options)?;
    let tm = board.target_machine(crate::compile::optimize::codegen_level())?;
    for_board(module, &tm)?;
    crate::compile::codegen_prep::prepare(module)?;
    busy_around_runtime_calls(module)?;
    crate::compile::optimize::run_llvm_passes(module, &tm)?;
    sections_of_their_own(module);

    beside_output(output_path, |scripts| {
        fs::write(scripts.join("memory.x"), &memory_x).map_err(|e| format!("failed to write memory.x: {}", e))?;
        fs::write(scripts.join("typelisp.x"), DEFAULT_STACK).map_err(|e| format!("failed to write typelisp.x: {}", e))?;
        let object_path = scripts.join("program.o");
        write_object(&tm, module, &object_path, items)?;
        run_linker(&linker, |c| {
            // `link.x` includes `memory.x`, which is found on the search
            // path.
            c.arg("-L")
                .arg(scripts)
                .arg("-T")
                .arg(runtime.join("link.x"))
                .arg("-T")
                .arg(scripts.join("typelisp.x"))
                .arg(&object_path)
                // lld takes a symbol from an archive wherever on the line the
                // archive is, so the runtime calling the board's functions
                // finds them whatever the order.
                .args(&options.link)
                .arg(runtime.join(options.board_functions.archive()))
                .arg("-o")
                .arg(output_path)
        })
    })
}
