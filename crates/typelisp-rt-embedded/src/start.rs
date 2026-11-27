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
//!
//! Nothing stops the stack at its bottom: below it is the heap, which a stack
//! that grows past it overwrites. The bottom's first words are guard words
//! ([`GUARD`]), written before the program starts; each function the
//! program compiled to starts by calling [`typelisp_stack_check`] (unless
//! `--no-stack-check`), which ends the program when the stack is all but
//! used up or the guard words are not as written. The runtime's own
//! functions are not checked on entry: one that runs the stack past its
//! bottom is found by the guard words at the next check, or when the program
//! panics, faults or ends, which then says so.

use core::alloc::{GlobalAlloc, Layout};
use typelisp_mem::sync::atomic::{AtomicPtr, Ordering};

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

/// The words at the bottom of the stack, written before the program starts:
/// a stack that has grown past its bottom has overwritten them.
const GUARD: [u32; 4] = [0x5354_4143, 0x4b20_454e, 0x4420_4845, 0x5245_2121];

/// What the program says when the stack has run past its bottom.
const OVERFLOWED: &[u8] = b"stack overflow: the stack has run past its bottom into the heap";

/// The least room on the stack a function may start with: what is below is
/// for the runtime functions it calls and for ending the program.
const STACK_RESERVE: usize = 512;

/// The bottom of the stack, where the heap ends: `_typelisp_stack_size`
/// bytes below the top of RAM. Ends the program when that leaves no room
/// for a heap after `.bss`.
fn stack_bottom() -> usize {
    // Only the symbols' addresses are taken, which the linker script gives.
    let (heap_start, stack_top, stack_size) = (
        core::ptr::addr_of!(__sheap) as usize,
        core::ptr::addr_of!(_stack_start) as usize,
        core::ptr::addr_of!(_typelisp_stack_size) as usize,
    );
    match stack_top.checked_sub(stack_size) {
        Some(bottom) if bottom > heap_start => bottom,
        _ => {
            let (size, top, bss) = (hex(stack_size as u32), hex(stack_top as u32), hex(heap_start as u32));
            end(&[b"no room for a heap: the stack takes ", &size, b" bytes below ", &top, b", and .bss ends at ", &bss])
        }
    }
}

/// The guard words' place: the first word of the stack from its bottom up.
fn guard() -> *mut [u32; 4] {
    ((stack_bottom() + 3) & !3) as *mut [u32; 4]
}

/// Whether the stack has grown past its bottom: the guard words are not as
/// [`run`] wrote them.
pub(crate) fn overflowed() -> bool {
    // SAFETY: the bottom of the stack, which is RAM, on a word.
    unsafe { core::ptr::read_volatile(guard()) != GUARD }
}

/// Ends the program when the stack has less than [`STACK_RESERVE`] bytes
/// left above its bottom, or has run past it. What every function the
/// program compiled to calls first.
#[no_mangle]
extern "C" fn typelisp_stack_check() {
    let bottom = stack_bottom();
    if this_core::stack_pointer() < bottom + STACK_RESERVE || overflowed() {
        let size = hex(core::ptr::addr_of!(_typelisp_stack_size) as usize as u32);
        end(&[b"stack overflow: the program needs more than the ", &size, b" bytes of the stack (_typelisp_stack_size)"])
    }
}

/// Runs the program and ends with what it answers, or, when the stack has
/// run past its bottom on the way, with the code a panic ends it with: what
/// it answers was worked out with a heap the stack wrote over. What the
/// core's entry calls.
pub(crate) fn run() -> ! {
    // SAFETY: the bottom of the stack, far below the frames there are yet.
    unsafe { core::ptr::write_volatile(guard(), GUARD) };
    // SAFETY: the program typl compiled, which takes nothing.
    let code = unsafe { typelisp_main() };
    if overflowed() {
        end(&[OVERFLOWED])
    }
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
    let (heap_start, heap_end) = (core::ptr::addr_of!(__sheap) as usize, stack_bottom());
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
/// `parts` and a newline on standard error, after saying so when the stack
/// has run past its bottom: the fault may be what that did to the heap.
pub(crate) fn die(parts: &[&[u8]]) -> ! {
    say_if_overflowed();
    end(parts)
}

/// Writes on standard error that the stack has run past its bottom, when it
/// has.
pub(crate) fn say_if_overflowed() {
    if overflowed() {
        let _ = typelisp_mem::host::write_stderr(OVERFLOWED);
        let _ = typelisp_mem::host::write_stderr(b"\n");
    }
}

/// [`die`], without looking at the stack. Not through `core::fmt`, which
/// every program would carry for it.
fn end(parts: &[&[u8]]) -> ! {
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
