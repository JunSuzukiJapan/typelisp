//! An AVR microcontroller (`avr-atmega328p`, the Arduino Uno's): emitting the
//! module as an AVR object and linking it with `ld.lld` into an ELF
//! executable.
//!
//! The target has no runtime: an executable is the program's own code and
//! the start-up code in [`START`], so a program that needs the runtime is
//! refused ([`needs_runtime`],
//! [`check_reaches_no_runtime`]). What the program does not reach is taken out of
//! the module before code is generated for it.
//!
//! Where flash and RAM are is the linker script's `MEMORY`: the user's
//! (`--linker-script`), or the chip's own ([`ATMEGA328P`]). RAM is at
//! `0x800000` and up, as the AVR tools have it: data and code have address
//! spaces of their own, and that is how an ELF file tells them apart.

use std::fs;
use std::process::Command;

use inkwell::module::Module;
use inkwell::targets::{CodeModel, FileType, RelocMode, Target, TargetMachine, TargetTriple};
use inkwell::builder::Builder;
use inkwell::values::{FunctionValue, IntValue, PointerValue};

use super::embedded::{sections_of_their_own, Board, BOARD_MAIN};
use super::{board_linker, reach, BuildOptions, ENTRY_POINT_NAME};
use crate::CompileTarget;

/// The ATmega328P: 32 KiB of flash, of which the last 512 bytes are the
/// Uno's boot loader, and 2 KiB of RAM after the registers.
pub const ATMEGA328P: Board = Board {
    cpu: "atmega328p",
    qemu_machine: "uno",
    qemu_memory: "\
MEMORY
{
  FLASH : ORIGIN = 0, LENGTH = 32K - 512
  RAM : ORIGIN = 0x800100, LENGTH = 2K
}
",
};

/// Where the sections go. The start-up code's parts run one into the next in
/// the order they are in here; the one that readies RAM is there only when the
/// program has something there ([`DATA`]).
const SECTIONS: &str = "\
ENTRY(__typelisp_vectors)
SECTIONS
{
  .text :
  {
    KEEP(*(.vectors))
    KEEP(*(.init.1))
    KEEP(*(.init.2))
    KEEP(*(.init.3))
    KEEP(*(.init.4))
    *(.text .text.*)
    . = ALIGN(2);
  } > FLASH
  .data :
  {
    __data_start = .;
    *(.rodata .rodata.* .data .data.*)
    __data_end = .;
  } > RAM AT> FLASH
  __data_load = LOADADDR(.data);
  .bss (NOLOAD) :
  {
    __bss_start = .;
    *(.bss .bss.* COMMON)
    __bss_end = .;
  } > RAM
  __stack = ORIGIN(RAM) + LENGTH(RAM) - 1;
  ASSERT(SIZEOF(.data) + SIZEOF(.bss) == 0 || DEFINED(__typelisp_ready_data), \"internal error: data nothing readies\")
  ASSERT(__bss_end <= __stack, \"the program's data does not fit in RAM\")
}
";

/// Where a program goes that an error ends: a division with no answer.
pub(super) const FAULT: &str = "__typelisp_stop";

/// How many entries the vector table has, the first of them reset's.
const VECTORS: u16 = 26;

/// The vector-table entry of an interrupt `0` as a handler's attribute has it
/// (`typeck::attribute`): a Cortex-M's exceptions come before it.
const FIRST_INTERRUPT: u16 = 16;

/// The status register's address, and its bit that lets interrupts come.
const SREG: u64 = 0x5f;
const INTERRUPTS: u64 = 0x80;

/// The interrupt the handler `path` is for, its attribute having named the
/// entry `vector`: the number the chip's tables count from reset's 0.
pub(super) fn interrupt_number(target: super::Target, vector: u16, path: &crate::types::Path) -> Result<u16, String> {
    let last = VECTORS - 1;
    match vector.checked_sub(FIRST_INTERRUPT) {
        None => Err(format!("`{}` handles a core exception, and --target {} has none", path, target.name())),
        Some(n) if (1..=last).contains(&n) => Ok(n),
        Some(n) => Err(format!("`{}` handles interrupt {}: the interrupts of --target {} are 1 to {}", path, n, target.name(), last)),
    }
}

/// The function the vector `n` jumps to.
fn vector_symbol(n: u16) -> String {
    format!("__typelisp_vector_{}", n)
}

/// Makes what the interrupt `n` enters: `handler`, between saving and putting
/// back every register it changes, and a return that lets interrupts come
/// again.
pub(super) fn vector(module: &Module<'static>, n: u16, handler: FunctionValue<'static>) -> Result<(), String> {
    let ctx = crate::compile::llvm_context();
    let f = module.add_function(&vector_symbol(n), ctx.void_type().fn_type(&[], false), None);
    // `CallingConv::AVR_SIGNAL`, and the attribute that goes with it.
    f.set_call_conventions(85);
    f.add_attribute(inkwell::attributes::AttributeLoc::Function, ctx.create_string_attribute("signal", ""));
    let b = ctx.create_builder();
    b.position_at_end(ctx.append_basic_block(f, "entry"));
    b.build_call(handler, &[], "").map_err(|e| format!("failed to call an interrupt handler: {}", e))?;
    b.build_return(None).map_err(|e| format!("failed to build a vector's return: {}", e))?;
    Ok(())
}

fn status_register() -> PointerValue<'static> {
    let ctx = crate::compile::llvm_context();
    crate::compile::carriers::int(16).const_int(SREG, false).const_to_pointer(ctx.ptr_type(inkwell::AddressSpace::default()))
}

/// The status register's value.
pub(crate) fn status(b: &Builder<'static>) -> Result<IntValue<'static>, String> {
    let v = b.build_load(crate::compile::carriers::int(8), status_register(), "status").map_err(|e| e.to_string())?.into_int_value();
    let i = v.as_instruction().ok_or("internal error: a load is an instruction")?;
    i.set_volatile(true).map_err(|e| format!("the status register: {:?}", e))?;
    Ok(v)
}

/// Sets the status register.
pub(crate) fn set_status(b: &Builder<'static>, v: IntValue<'static>) -> Result<(), String> {
    let i = b.build_store(status_register(), v).map_err(|e| e.to_string())?;
    i.set_volatile(true).map_err(|e| format!("the status register: {:?}", e))
}

/// Lets interrupts come.
pub(super) fn enable_interrupts(b: &Builder<'static>) -> Result<(), String> {
    set_status(b, crate::compile::carriers::int(8).const_int(INTERRUPTS, false))
}

/// The interrupt vectors, `handled` of them the program's.
fn vector_table(handled: &[u16]) -> String {
    let mut table = String::from(
        "
    .section .vectors,\"ax\",@progbits
    .globl __typelisp_vectors
__typelisp_vectors:
    jmp __typelisp_reset
",
    );
    for n in 1..VECTORS {
        let to = if handled.contains(&n) { vector_symbol(n) } else { "__typelisp_stop".to_string() };
        table.push_str(&format!("    jmp {}\n", to));
    }
    table
}

/// What runs at reset: the register the compiled code takes to be zero
/// (`r1`), the stack, RAM ([`DATA`]), `main`. A `main` that answers stops the
/// processor, and so do an interrupt with no handler and an error ([`FAULT`]).
const START: &str = "
    .section .init.1,\"ax\",@progbits
__typelisp_reset:
    clr r1
    out 0x3f, r1
    ldi r28, lo8(__stack)
    ldi r29, hi8(__stack)
    out 0x3e, r29
    out 0x3d, r28

    .section .init.4,\"ax\",@progbits
    call typelisp_main
    .globl __typelisp_stop
__typelisp_stop:
    cli
    sleep
    rjmp __typelisp_stop
";

// LLVM's AVR back end, which inkwell has no name for.
unsafe extern "C" {
    fn LLVMInitializeAVRTargetInfo();
    fn LLVMInitializeAVRTarget();
    fn LLVMInitializeAVRTargetMC();
    fn LLVMInitializeAVRAsmPrinter();
    fn LLVMInitializeAVRAsmParser();
}

/// Registers the back end with LLVM. Again does nothing.
fn initialize() {
    // SAFETY: each registers a static of LLVM's own, under `COMPILE_LOCK`.
    unsafe {
        LLVMInitializeAVRTargetInfo();
        LLVMInitializeAVRTarget();
        LLVMInitializeAVRTargetMC();
        LLVMInitializeAVRAsmPrinter();
        LLVMInitializeAVRAsmParser();
    }
}

/// What runs at reset, between the rest of [`START`], in a program that has
/// something in RAM: `.data` out of flash, and `.bss` cleared.
const DATA: &str = "
    .section .init.2,\"ax\",@progbits
    .globl __typelisp_ready_data
__typelisp_ready_data:
    .globl __do_copy_data
__do_copy_data:
    ldi r26, lo8(__data_start)
    ldi r27, hi8(__data_start)
    ldi r30, lo8(__data_load)
    ldi r31, hi8(__data_load)
    ldi r24, lo8(__data_end)
    ldi r25, hi8(__data_end)
    rjmp .Ldata_test
.Ldata_copy:
    lpm r0, Z+
    st X+, r0
.Ldata_test:
    cp r26, r24
    cpc r27, r25
    brne .Ldata_copy

    .section .init.3,\"ax\",@progbits
    .globl __do_clear_bss
__do_clear_bss:
    ldi r26, lo8(__bss_start)
    ldi r27, hi8(__bss_start)
    ldi r24, lo8(__bss_end)
    ldi r25, hi8(__bss_end)
    rjmp .Lbss_test
.Lbss_clear:
    st X+, r1
.Lbss_test:
    cp r26, r24
    cpc r27, r25
    brne .Lbss_clear
";

/// What the code generator calls for what the processor has no instructions
/// for, under the names and the calling conventions of `libgcc`: the product
/// of two 32-bit integers (`__mulsi3`), and the quotient and the remainder of
/// two integers of each width (`__udivmodqi4`, `__udivmodhi4`,
/// `__udivmodsi4`, and the signed ones). Each is linked only when it is
/// called.
/// Floating-point arithmetic is not here: `--link` a `libgcc.a` for it.
const HELPERS: &str = "
    .section .text.__mulsi3,\"ax\",@progbits
    .globl __mulsi3
__mulsi3:
    mul r22, r18
    movw r26, r0
    mul r23, r19
    movw r30, r0
    mul r22, r19
    add r27, r0
    adc r30, r1
    clr r1
    adc r31, r1
    mul r23, r18
    add r27, r0
    adc r30, r1
    clr r1
    adc r31, r1
    mul r22, r20
    add r30, r0
    adc r31, r1
    mul r24, r18
    add r30, r0
    adc r31, r1
    mul r22, r21
    add r31, r0
    mul r23, r20
    add r31, r0
    mul r24, r19
    add r31, r0
    mul r25, r18
    add r31, r0
    clr r1
    movw r22, r26
    movw r24, r30
    ret

    .section .text.__udivmodqi4,\"ax\",@progbits
    .globl __udivmodqi4
__udivmodqi4:
    sub r25, r25
    ldi r23, 9
    rjmp 2f
1:  rol r25
    cp r25, r22
    brcs 2f
    sub r25, r22
2:  rol r24
    dec r23
    brne 1b
    com r24
    ret

    .section .text.__divmodqi4,\"ax\",@progbits
    .globl __divmodqi4
__divmodqi4:
    bst r24, 7
    mov r0, r24
    eor r0, r22
    sbrc r24, 7
    neg r24
    sbrc r22, 7
    neg r22
    call __udivmodqi4
    brtc 1f
    neg r25
1:  sbrc r0, 7
    neg r24
    ret

    .section .text.__udivmodhi4,\"ax\",@progbits
    .globl __udivmodhi4
__udivmodhi4:
    clr r26
    sub r27, r27
    ldi r21, 17
    rjmp 2f
1:  rol r26
    rol r27
    cp r26, r22
    cpc r27, r23
    brcs 2f
    sub r26, r22
    sbc r27, r23
2:  rol r24
    rol r25
    dec r21
    brne 1b
    com r24
    com r25
    movw r22, r24
    movw r24, r26
    ret

    .section .text.__divmodhi4,\"ax\",@progbits
    .globl __divmodhi4
__divmodhi4:
    bst r25, 7
    mov r0, r23
    brtc 1f
    com r0
    rcall __typelisp_negate_dividend
1:  sbrs r23, 7
    rjmp 2f
    rcall __typelisp_negate_divisor
2:  call __udivmodhi4
    sbrs r0, 7
    rjmp 3f
    rcall __typelisp_negate_divisor
3:  brtc 4f
    rcall __typelisp_negate_dividend
4:  ret
__typelisp_negate_dividend:
    com r25
    neg r24
    sbci r25, 0xff
    ret
__typelisp_negate_divisor:
    com r23
    neg r22
    sbci r23, 0xff
    ret

    .section .text.__udivmodsi4,\"ax\",@progbits
    .globl __udivmodsi4
__udivmodsi4:
    push r16
    ldi r16, 33
    clr r26
    clr r27
    clr r30
    sub r31, r31
    rjmp 2f
1:  rol r26
    rol r27
    rol r30
    rol r31
    cp r26, r18
    cpc r27, r19
    cpc r30, r20
    cpc r31, r21
    brcs 2f
    sub r26, r18
    sbc r27, r19
    sbc r30, r20
    sbc r31, r21
2:  rol r22
    rol r23
    rol r24
    rol r25
    dec r16
    brne 1b
    com r22
    com r23
    com r24
    com r25
    movw r18, r22
    movw r20, r24
    movw r22, r26
    movw r24, r30
    pop r16
    ret

    .section .text.__divmodsi4,\"ax\",@progbits
    .globl __divmodsi4
__divmodsi4:
    mov r0, r25
    eor r0, r21
    bst r25, 7
    brtc 1f
    rcall __typelisp_negate_low
1:  sbrs r21, 7
    rjmp 2f
    com r21
    com r20
    com r19
    neg r18
    sbci r19, 0xff
    sbci r20, 0xff
    sbci r21, 0xff
2:  call __udivmodsi4
    brtc 3f
    rcall __typelisp_negate_low
3:  bst r0, 7
    brtc 4f
    com r21
    com r20
    com r19
    neg r18
    sbci r19, 0xff
    sbci r20, 0xff
    sbci r21, 0xff
4:  ret
__typelisp_negate_low:
    com r25
    com r24
    com r23
    neg r22
    sbci r23, 0xff
    sbci r24, 0xff
    sbci r25, 0xff
    ret
";

fn target_machine(cpu: &str) -> Result<TargetMachine, String> {
    initialize();
    let triple = TargetTriple::create("avr-unknown-none");
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    target
        .create_target_machine(&triple, cpu, "", crate::compile::optimize::codegen_level(), RelocMode::Static, CodeModel::Default)
        .ok_or_else(|| format!("failed to create a target machine for {}", cpu))
}

/// The error a build for `target` stops with when the program needs the
/// runtime, for each of `whys`.
pub(super) fn needs_runtime(target: super::Target, whys: &[String]) -> String {
    whys.iter().fold(needs_runtime_header(target), |msg, why| format!("{}\n  {}", msg, why))
}

fn needs_runtime_header(target: super::Target) -> String {
    format!("--target {}: the target has no runtime, and the program needs it:", target.name())
}

/// Refuses a program whose `entry` reaches a runtime function.
pub(super) fn check_reaches_no_runtime(module: &Module<'static>, target: super::Target, entry: FunctionValue<'static>) -> Result<(), String> {
    let uses = reach::uses(module, entry, |name| {
        match crate::compile::externs::BOARD_ONLY_SHIMS.iter().find(|(shim, _)| *shim == name) {
            Some((_, what)) => Some(format!("{}, which is for an interrupt controller the chip has not: the device's own register enables its interrupt", what)),
            None => super::is_runtime(module, name).then(|| format!("the runtime function `{}`", name)),
        }
    });
    if uses.is_empty() { Ok(()) } else { Err(reach::report(&needs_runtime_header(target), &uses)) }
}

/// Takes everything `main` and the vectors `handled` do not reach out of
/// `module`.
fn keep_only_entered(module: &Module<'static>, handled: &[u16], tm: &TargetMachine) -> Result<(), String> {
    let kept: String = handled.iter().map(|n| format!(";preserve-gv={}", vector_symbol(*n))).collect();
    module
        .run_passes(
            &format!("internalize<preserve-gv={}{}>,globaldce", BOARD_MAIN, kept),
            tm,
            inkwell::passes::PassBuilderOptions::create(),
        )
        .map_err(|e| format!("internal error: pruning the module failed: {}", e))
}

/// Emits `module` as an AVR object and links it, with the start-up code and
/// what `options` say, into an ELF executable at `output_path`. `handled` are
/// the interrupts [`vector`] was made for. Must be
/// called with [`crate::compile::COMPILE_LOCK`] held.
pub(super) fn write_executable(
    module: &Module<'static>,
    output_path: &str,
    items: &[CompileTarget],
    handled: &[u16],
    options: &BuildOptions,
) -> Result<(), String> {
    let board = options.target.board().ok_or("internal error: an AVR target is a board")?;
    let linker = board_linker()?;
    for file in &options.link {
        if !file.is_file() {
            return Err(format!("--link: {}: no such file", file.display()));
        }
    }
    let memory = match options.linker_script.as_deref() {
        Some(path) => fs::read_to_string(path).map_err(|e| format!("--linker-script: {}: {}", path.display(), e))?,
        None => board.qemu_memory.to_string(),
    };
    let tm = target_machine(board.cpu)?;
    module.set_triple(&tm.get_triple());
    module.set_data_layout(&tm.get_target_data().get_data_layout());
    module
        .get_function(ENTRY_POINT_NAME)
        .ok_or("internal error: the module has no `main`")?
        .as_global_value()
        .set_name(BOARD_MAIN);
    keep_only_entered(module, handled, &tm)?;
    crate::compile::codegen_prep::prepare(module)?;
    crate::compile::optimize::run_llvm_passes(module, &tm)?;
    sections_of_their_own(module);
    // What is linked in besides may have something in RAM too.
    let data = if module.get_first_global().is_some() || !options.link.is_empty() { DATA } else { "" };
    module.set_inline_assembly(&format!("{}{}{}{}", vector_table(handled), START, data, HELPERS));

    // The object and the script, beside the executable while it links.
    let scratch = std::path::PathBuf::from(format!("{}.ld", output_path));
    fs::create_dir_all(&scratch).map_err(|e| format!("failed to create {}: {}", scratch.display(), e))?;
    let linked = (|| {
        let script = scratch.join("typelisp.x");
        fs::write(&script, format!("{}{}", memory, SECTIONS)).map_err(|e| format!("failed to write {}: {}", script.display(), e))?;
        let object_path = scratch.join("program.o");
        tm.write_to_file(module, FileType::Object, &object_path).map_err(|e| format!("failed to emit object file: {}", e))?;
        for warning in crate::compile::codegen_prep::take_frame_warnings(items) {
            eprintln!("{}", warning);
        }
        let status = Command::new(&linker)
            .args(["-flavor", "gnu", "--gc-sections", "-T"])
            .arg(&script)
            .arg(&object_path)
            .args(&options.link)
            .arg("-o")
            .arg(output_path)
            .status()
            .map_err(|e| format!("failed to invoke {}: {}", linker.display(), e))?;
        if !status.success() {
            return Err(format!("ld.lld failed with status {}", status));
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&scratch);
    linked
}
