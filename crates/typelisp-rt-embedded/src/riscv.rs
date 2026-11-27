//! A 32-bit RISC-V core in machine mode: `riscv-rt` gives `_start` and the
//! trap entry, which readies `.data` and `.bss` and calls the entry here,
//! and hands every trap to the two dispatchers here. Interrupts are masked
//! and unmasked in `mie`, a bit for each.
//!
//! The runtime numbers a vector-table entry as a Cortex-M does, so that both
//! cores share it: interrupt `n` — the code `mcause` gives it, 3 for the
//! machine's software interrupt, 7 for its timer, 11 for its external
//! interrupts — at 16 + `n`. An exception is a fault, which ends the
//! program.

use core::arch::asm;

use crate::start::{arrived, die, hex};

// Named for what it links in and nothing here calls: the one hart's critical
// section (`critical-section-single-hart`), which the allocator takes.
use riscv as _;

/// How many interrupts `mie` has a bit for.
const INTERRUPTS: u16 = 32;

/// Runs the program and ends with what it answers.
#[riscv_rt::entry]
fn start() -> ! {
    crate::start::run()
}

/// Lets the interrupts `mie` lets through come: `mstatus.MIE`, which is
/// clear from reset. `mie` itself is clear, so none comes until the program
/// enables one.
pub(crate) fn interrupts_may_come() {
    // SAFETY: sets one bit of `mstatus`.
    unsafe { asm!("csrsi mstatus, 8") }
}

/// An interrupt (`riscv-rt`'s feature `custom-interrupts`): the program's
/// handler for it, or, when it has none, the end of the program the way a
/// fault ends it.
#[no_mangle]
extern "C" fn _dispatch_core_interrupt(code: usize) {
    arrived(16 + code, b"interrupt", code as i32)
}

/// An exception (`riscv-rt`'s feature `custom-exceptions`) — a bad address,
/// an illegal instruction, an `ecall` — ends the program with the code a
/// panic does, instead of `riscv-rt`'s default of spinning forever.
#[no_mangle]
extern "C" fn _dispatch_exception(_frame: &riscv_rt::TrapFrame, code: usize) -> ! {
    let pc: usize;
    // SAFETY: reads `mepc`, where the trap was taken.
    unsafe { asm!("csrr {0}, mepc", out(reg) pc) };
    die(&[b"exception ", &hex(code as u32), b" at pc ", &hex(pc as u32)])
}

/// Masking and unmasking an interrupt, for the runtime's interrupt handlers.
/// A handler runs with `mstatus.MIE` clear, as the core enters a trap, so
/// that it is never interrupted by another.
pub(crate) static CONTROLLER: typelisp_rt::interrupt::Controller = typelisp_rt::interrupt::Controller {
    interrupts: INTERRUPTS,
    // SAFETY: clears and sets one bit of `mie`, below `INTERRUPTS`, which the
    // runtime checked.
    mask: |n| unsafe { asm!("csrc mie, {0}", in(reg) 1usize << n) },
    unmask: |n| unsafe { asm!("csrs mie, {0}", in(reg) 1usize << n) },
};
