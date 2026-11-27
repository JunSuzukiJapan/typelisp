//! An Arm Cortex-M core: `cortex-m-rt` gives the vector table and `Reset`,
//! which readies `.data` and `.bss` and calls the entry here. Interrupts are
//! masked and unmasked in the NVIC.
//!
//! The runtime numbers a vector-table entry as the core does: the core's
//! exceptions from 0 to 15, then interrupt `n` at 16 + `n`.

use crate::start::{arrived, die, hex};

// Named for what it links in and nothing here calls: the one core's critical
// section (`critical-section-single-core`), which the allocator takes.
use cortex_m as _;

/// How many interrupts the NVIC numbers.
const INTERRUPTS: u16 = 240;

/// Runs the program and ends with what it answers.
#[cortex_m_rt::entry]
fn start() -> ! {
    crate::start::run()
}

/// Nothing to do: the core takes the interrupts the NVIC lets through from
/// reset on.
pub(crate) fn interrupts_may_come() {}

/// A fault (a bad address, an undefined instruction, the stack run into the
/// heap) ends the program with the code a panic does, instead of
/// `cortex-m-rt`'s default of spinning forever.
#[cortex_m_rt::exception]
unsafe fn HardFault(frame: &cortex_m_rt::ExceptionFrame) -> ! {
    die(&[b"hard fault at pc ", &hex(frame.pc())])
}

/// Every other exception and interrupt: the program's handler for it, or,
/// when it has none, the end of the program the way a fault ends it. `irqn`
/// is the entry's number in the vector table less 16 (an exception's is
/// negative, down to -14).
#[cortex_m_rt::exception]
unsafe fn DefaultHandler(irqn: i16) {
    arrived((i32::from(irqn) + 16) as usize, b"exception or interrupt", i32::from(irqn))
}

/// An interrupt, by its number, as the NVIC takes one.
#[derive(Clone, Copy)]
struct Irq(u16);

// SAFETY: the number is the interrupt's own, which the program named and the
// runtime checked is below `INTERRUPTS`.
unsafe impl cortex_m::interrupt::InterruptNumber for Irq {
    fn number(self) -> u16 {
        self.0
    }
}

/// Masking and unmasking an interrupt, for the runtime's interrupt handlers.
/// All interrupts keep the priority they reset to, the same for all, so that
/// a handler is never interrupted by another.
pub(crate) static CONTROLLER: typelisp_rt::interrupt::Controller = typelisp_rt::interrupt::Controller {
    interrupts: INTERRUPTS,
    mask: |n| cortex_m::peripheral::NVIC::mask(Irq(n)),
    // SAFETY: unmasking breaks no critical section: the runtime holds none,
    // and an interrupt that comes then waits for the runtime.
    unmask: |n| unsafe { cortex_m::peripheral::NVIC::unmask(Irq(n)) },
};
