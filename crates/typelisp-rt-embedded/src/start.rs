//! The start of the program, and the memory the runtime allocates from: the
//! same on every core.
//!
//! The core's module (`cortex_m`, `riscv`) enters the program, once its
//! startup crate has readied `.data` and `.bss`, by calling [`run`]. The
//! memory between the end of `.bss` (`__sheap`) and the stack is the heap:
//! the stack takes `_typelisp_stack_size` bytes below the top of RAM
//! (`_stack_start`), a symbol the linker script typl links with defines.
//! Both startup crates' linker scripts name the end of `.bss` and the top of
//! the stack so.

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicPtr, Ordering};

use embedded_alloc::LlffHeap;

#[cfg(target_arch = "arm")]
use crate::cortex_m as this_core;
#[cfg(target_arch = "riscv32")]
use crate::riscv as this_core;

static HEAP: LlffHeap = LlffHeap::empty();

extern "C" {
    /// The end of `.bss`, where the heap starts.
    static __sheap: u8;
    /// The top of the stack, at the end of RAM.
    static _stack_start: u8;
    /// How much of RAM the stack takes; only its address means anything.
    static _typelisp_stack_size: u8;
    /// The program: the `main` typl compiles, under this name.
    fn typelisp_main() -> i32;
}

/// Runs the program and ends with what it answers. What the core's entry
/// calls.
pub(crate) fn run() -> ! {
    // SAFETY: the program typl compiled, which takes nothing.
    let code = unsafe { typelisp_main() };
    typelisp_mem::host::exit(code)
}

/// Gives the heap the memory between `.bss` and the stack, and the runtime
/// the interrupt controller. The first thing the program does, when it uses
/// the runtime at all: one that only drives registers has neither.
///
/// # Safety
///
/// Called once, before anything allocates.
#[no_mangle]
unsafe extern "C" fn typelisp_board_start() {
    // Only the symbols' addresses are taken, which the linker script gives.
    let (heap_start, stack_top, stack_size) = (
        core::ptr::addr_of!(__sheap) as usize,
        core::ptr::addr_of!(_stack_start) as usize,
        core::ptr::addr_of!(_typelisp_stack_size) as usize,
    );
    let heap_end = match stack_top.checked_sub(stack_size) {
        Some(end) if end > heap_start => end,
        _ => {
            let (size, top, bss) = (hex(stack_size as u32), hex(stack_top as u32), hex(heap_start as u32));
            die(&[b"no room for a heap: the stack takes ", &size, b" bytes below ", &top, b", and .bss ends at ", &bss])
        }
    };
    // SAFETY: once, before anything allocates, with memory nothing else uses.
    unsafe { HEAP.init(heap_start, heap_end - heap_start) };
    typelisp_mem::memory::set_capacity(heap_end - heap_start);
    typelisp_rt::interrupt::set_controller(&this_core::CONTROLLER);
    TYPELISP_ARRIVED.store(typelisp_rt::interrupt::arrived as *mut (), Ordering::SeqCst);
    this_core::interrupts_may_come();
}

/// `typelisp_rt::interrupt::arrived`, once the program has started the
/// runtime: a program that starts none has no handler for anything, and the
/// runtime's handlers are not linked into it. Under a name of its own, so
/// that the compiler does not conclude from this crate being the only one to
/// store it that it can only ever be that one function, and name the
/// function in the handler after all.
#[no_mangle]
static TYPELISP_ARRIVED: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// The core took the exception or interrupt at the runtime's vector-table
/// entry `vector` (`typelisp_rt::interrupt`): the program's handler for it
/// runs, or, when it has none, the program ends the way a fault ends it,
/// saying `what` and `number`.
pub(crate) fn arrived(vector: usize, what: &[u8], number: i32) {
    let arrived = TYPELISP_ARRIVED.load(Ordering::SeqCst);
    if !arrived.is_null() {
        // SAFETY: only ever `typelisp_rt::interrupt::arrived`.
        let arrived: fn(usize) -> bool = unsafe { core::mem::transmute(arrived) };
        if arrived(vector) {
            return;
        }
    }
    let sign: &[u8] = if number < 0 { b"-" } else { b"" };
    let digits = decimal(number.unsigned_abs().min(999) as u16);
    let shown = digits.iter().position(|d| *d != b'0').unwrap_or(2);
    die(&[b"unhandled ", what, b" ", sign, digits.get(shown..).unwrap_or(&digits)])
}

/// Ends the program with the code a panic ends it with, having written
/// `parts` and a newline on standard error. Not through `core::fmt`, which
/// every program would carry for it.
pub(crate) fn die(parts: &[&[u8]]) -> ! {
    for part in parts {
        let _ = typelisp_mem::host::write_stderr(part);
    }
    let _ = typelisp_mem::host::write_stderr(b"\n");
    typelisp_mem::host::exit(typelisp_rt::EXIT_CODE_PANIC as i32)
}

/// `n` as `0x` and eight hexadecimal digits.
pub(crate) fn hex(n: u32) -> [u8; 10] {
    let mut text = *b"0x00000000";
    let mut n = n;
    for digit in text.iter_mut().skip(2).rev() {
        let d = (n & 0xf) as u8;
        *digit = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
        n >>= 4;
    }
    text
}

/// `n`, below 1000, as three decimal digits.
fn decimal(n: u16) -> [u8; 3] {
    [b'0' + (n / 100 % 10) as u8, b'0' + (n / 10 % 10) as u8, b'0' + (n % 10) as u8]
}

/// `size` bytes aligned to `align` from the heap, or null when there are
/// none left.
#[no_mangle]
unsafe extern "C" fn typelisp_host_alloc(size: usize, align: usize) -> *mut u8 {
    // SAFETY: the runtime's allocator passes a size and alignment that came
    // from a `Layout`.
    unsafe { HEAP.alloc(Layout::from_size_align_unchecked(size, align)) }
}

/// Gives back what [`typelisp_host_alloc`] returned for the same `size` and
/// `align`.
#[no_mangle]
unsafe extern "C" fn typelisp_host_dealloc(ptr: *mut u8, size: usize, align: usize) {
    // SAFETY: as above, and `ptr` came from `typelisp_host_alloc`.
    unsafe { HEAP.dealloc(ptr, Layout::from_size_align_unchecked(size, align)) }
}
