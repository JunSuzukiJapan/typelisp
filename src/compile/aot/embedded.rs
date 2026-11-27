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
//! (`--linker-script`), or the QEMU board's ([`Board::qemu_memory`], its RAM
//! the size `--ram` gives). The rest of what the runtime asks
//! of the board (`typelisp_host_*`) the runtime gives over semihosting, or
//! the board as C functions in the objects and libraries the link is given
//! (`--board-functions`, `--link`).
//!
//! An interrupt handler runs at once only while the program is in its own
//! compiled code, never part way through a runtime function
//! (`typelisp_rt::interrupt`): every call the module makes into the runtime
//! is made through a function that marks the runtime busy around it
//! ([`busy_around_runtime_calls`]).
//!
//! Unless `--no-stack-check` is given, every function the module defines
//! starts by asking the runtime whether the stack has room left
//! ([`stack_checked`]): a board has no operating system to stop a stack
//! that runs into the heap.

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
    /// The core exceptions `#[exception(NAME)]` may name on it, of the ones
    /// a Cortex-M has (`typelisp_front::EXCEPTIONS`).
    pub exceptions: &'static [&'static str],
    /// The QEMU that emulates the core, and the arguments that pick the
    /// board (`-M ...`) before `-kernel`.
    pub qemu: (&'static str, &'static [&'static str]),
    /// The QEMU board's memory, which a program is linked for when the user
    /// gives no linker script.
    pub qemu_memory: Layout,
}

/// Where a board's memory is: what its `memory.x` says ([`Layout::memory_x`]).
pub struct Layout {
    /// Where code goes, its origin and length, if not in RAM.
    pub flash: Option<(u64, u64)>,
    /// RAM, its origin and length.
    pub ram: (u64, u64),
    /// How much of the RAM the stack takes (`_typelisp_stack_size`), where the
    /// runtime places it; the rest above `.bss` is the heap. On the QEMU
    /// boards 1 MiB, so that recursion goes deep in the tests.
    pub stack: Option<u64>,
}

impl Layout {
    /// The layout as a `memory.x` for `core`, with `ram` bytes of RAM for
    /// the data, the heap and the stack in place of the board's (`--ram`),
    /// and then a stack of at most a quarter of it. On a board whose code is
    /// in RAM too (no flash), the code keeps the rest of the board's RAM,
    /// below the `ram` bytes at its end.
    pub(super) fn memory_x(&self, core: Core, ram: Option<u64>) -> String {
        let (board_origin, board_ram) = self.ram;
        let (flash, ram_origin, ram_length) = match (self.flash, ram) {
            (None, Some(ram)) if ram < board_ram => (Some((board_origin, board_ram - ram)), board_origin + board_ram - ram, ram),
            (flash, ram) => (flash, board_origin, ram.unwrap_or(board_ram)),
        };
        let mut text = String::from("MEMORY\n{\n");
        if let Some((origin, length)) = flash {
            text.push_str(&format!("  FLASH : ORIGIN = {:#x}, LENGTH = {}\n", origin, length));
        }
        text.push_str(&format!("  RAM : ORIGIN = {:#x}, LENGTH = {}\n}}\n", ram_origin, ram_length));
        // `riscv-rt` places each kind of section in the region its alias
        // names: code and constants in flash where there is one.
        if core == Core::Riscv32 {
            let code = if flash.is_some() { "FLASH" } else { "RAM" };
            for (region, memory) in [("TEXT", code), ("RODATA", code), ("DATA", "RAM"), ("BSS", "RAM"), ("HEAP", "RAM"), ("STACK", "RAM")] {
                text.push_str(&format!("REGION_ALIAS(\"REGION_{}\", {});\n", region, memory));
            }
        }
        if let Some(stack) = self.stack {
            let stack = if ram.is_some() { stack.min(ram_length / 4) } else { stack };
            text.push_str(&format!("_typelisp_stack_size = {};\n", stack));
        }
        text
    }
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
            None => {
                let name = typelisp_front::EXCEPTIONS.iter().find(|(_, v)| *v == vector).map_or("?", |(name, _)| *name);
                match self.exceptions {
                    [] => Err(format!("`{}` handles a core exception, and --target {} has none", path, target.name())),
                    has if has.contains(&name) => Ok(vector),
                    has => Err(format!(
                        "`{}` handles the exception {}, which --target {} has not got (its exceptions are {})",
                        path,
                        name,
                        target.name(),
                        has.join(", ")
                    )),
                }
            }
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

/// The exceptions of an ARMv7-M or ARMv8-M Mainline core (Cortex-M3, M4, M7,
/// M33): all a program may handle.
const ARMV7M_EXCEPTIONS: [&str; 8] =
    ["NonMaskableInt", "MemoryManagement", "BusFault", "UsageFault", "SVCall", "DebugMonitor", "PendSV", "SysTick"];

/// The exceptions of an ARMv6-M core (Cortex-M0, M0+), which has no fault
/// but HardFault and no debug monitor.
const ARMV6M_EXCEPTIONS: [&str; 4] = ["NonMaskableInt", "SVCall", "PendSV", "SysTick"];

/// `thumbv6m-none-eabi`, a Cortex-M0+: no floating-point unit, no
/// compare-and-swap (`typelisp_mem::sync`), and 32 interrupts. QEMU has no
/// Cortex-M0+ board, and its Cortex-M0 one (`microbit`) has 16 KiB of RAM,
/// less than the runtime takes, so the core stands on `mps2-an385` (Arm's
/// AN385, a Cortex-M3, which runs every ARMv6-M instruction as an M0+
/// does): 4 MiB of code memory at 0 and 4 MiB of RAM at `0x20000000`.
pub const CORTEX_M0PLUS: Board = Board {
    core: Core::CortexM,
    llvm_triple: "thumbv6m-none-eabi",
    cpu: "cortex-m0plus",
    features: "",
    interrupts: (0, 31),
    exceptions: &ARMV6M_EXCEPTIONS,
    qemu: ("qemu-system-arm", &["-M", "mps2-an385"]),
    qemu_memory: Layout { flash: Some((0x0, 4 << 20)), ram: (0x20000000, 4 << 20), stack: Some(1 << 20) },
};

/// `thumbv7em-none-eabihf`, on QEMU's `mps2-an386` (Arm's AN386, a
/// Cortex-M4 on an MPS2 FPGA board): 4 MiB of code memory at 0 and 4 MiB of
/// RAM at `0x20000000`.
pub const CORTEX_M4: Board = Board {
    core: Core::CortexM,
    llvm_triple: "thumbv7em-none-eabihf",
    cpu: "cortex-m4",
    features: "",
    interrupts: (0, 239),
    exceptions: &ARMV7M_EXCEPTIONS,
    qemu: ("qemu-system-arm", &["-M", "mps2-an386"]),
    qemu_memory: Layout { flash: Some((0x0, 4 << 20)), ram: (0x20000000, 4 << 20), stack: Some(1 << 20) },
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
    exceptions: &ARMV7M_EXCEPTIONS,
    qemu: ("qemu-system-arm", &["-M", "mps2-an505"]),
    qemu_memory: Layout { flash: Some((0x10000000, 4 << 20)), ram: (0x38000000, 4 << 20), stack: Some(1 << 20) },
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
    exceptions: &[],
    qemu: ("qemu-system-riscv32", &["-M", "virt", "-bios", "none"]),
    qemu_memory: Layout { flash: None, ram: (0x80000000, 16 << 20), stack: Some(1 << 20) },
};

/// How much of RAM the stack takes when the user's `memory.x` does not say
/// (`_typelisp_stack_size`).
const DEFAULT_STACK: &str = "PROVIDE(_typelisp_stack_size = 64K);\n";

/// What says where the board's flash and RAM are: the user's linker script
/// (`--linker-script`), or the QEMU board's, with the RAM `--ram` gives. Refuses a `--link` file that is
/// not there, before anything is emitted.
pub(super) fn board_memory(board: &Board, options: &BuildOptions) -> Result<String, String> {
    for file in &options.link {
        if !file.is_file() {
            return Err(format!("--link: {}: no such file", file.display()));
        }
    }
    match options.linker_script.as_deref() {
        Some(path) => fs::read_to_string(path).map_err(|e| format!("--linker-script: {}: {}", path.display(), e)),
        None => Ok(board.qemu_memory.memory_x(board.core, options.ram.map(|r| r as u64))),
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

/// Makes every function `module` defines call `typelisp_stack_check` first,
/// as GCC's `-fstack-limit-symbol` does: the runtime ends the program when
/// the stack is all but used up, or has run past its end (the guard words at
/// its bottom are not as the runtime wrote them), before the function's
/// frame is any deeper. A call, not a comparison here, so that no block is
/// split and the entry block keeps its allocas. Before
/// [`busy_around_runtime_calls`], so that its brackets have none.
fn stack_checked(module: &Module<'static>) -> Result<(), String> {
    let ctx = module.get_context();
    let check = module
        .get_function("typelisp_stack_check")
        .unwrap_or_else(|| module.add_function("typelisp_stack_check", ctx.void_type().fn_type(&[], false), None));
    let defined: Vec<_> = module.get_functions().filter(|f| f.count_basic_blocks() > 0).collect();
    let builder = ctx.create_builder();
    for f in defined {
        let entry = f.get_first_basic_block().expect("a defined function has a block");
        match entry.get_first_instruction() {
            Some(first) => builder.position_before(&first),
            None => builder.position_at_end(entry),
        }
        builder.build_call(check, &[], "").map_err(|e| format!("failed to build a stack check: {}", e))?;
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
    if !options.no_stack_check {
        stack_checked(module)?;
    }
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

#[cfg(test)]
mod tests {
    use super::{Core, CORTEX_M0PLUS, RISCV32_VIRT};

    /// `--ram` gives the RAM's length, at the board's address, and a stack
    /// of at most a quarter of it.
    #[test]
    fn ram_replaces_the_board_s_and_bounds_the_stack() {
        let x = CORTEX_M0PLUS.qemu_memory.memory_x(Core::CortexM, Some(16 << 10));
        assert!(x.contains("RAM : ORIGIN = 0x20000000, LENGTH = 16384\n"), "{}", x);
        assert!(x.contains("_typelisp_stack_size = 4096;"), "{}", x);
        let x = CORTEX_M0PLUS.qemu_memory.memory_x(Core::CortexM, None);
        assert!(x.contains("LENGTH = 4194304\n") && x.contains("_typelisp_stack_size = 1048576;"), "{}", x);
        let x = RISCV32_VIRT.qemu_memory.memory_x(Core::Riscv32, Some(64 << 10));
        assert!(x.contains("FLASH : ORIGIN = 0x80000000, LENGTH = 16711680\n"), "{}", x);
        assert!(x.contains("RAM : ORIGIN = 0x80ff0000, LENGTH = 65536\n") && x.contains("REGION_ALIAS(\"REGION_TEXT\", FLASH);"), "{}", x);
        let x = RISCV32_VIRT.qemu_memory.memory_x(Core::Riscv32, None);
        assert!(!x.contains("FLASH") && x.contains("REGION_ALIAS(\"REGION_TEXT\", RAM);"), "{}", x);
    }

    /// Every exception a board lists is one an attribute can name.
    #[test]
    fn a_board_s_exceptions_are_named_by_the_attribute() {
        for target in super::super::Target::BOARDS {
            for name in target.board().expect("a board").exceptions {
                assert!(typelisp_front::EXCEPTIONS.iter().any(|(n, _)| n == name), "{}: {}", target.name(), name);
            }
        }
    }
}
