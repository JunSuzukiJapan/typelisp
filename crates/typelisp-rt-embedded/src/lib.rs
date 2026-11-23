//! The runtime of an executable for a board without an operating system:
//! `typelisp-rt` and the crates below it, as one static library.
//!
//! The crates are named here so that the library carries them: the shims
//! compiled code calls are theirs (`typelisp_rt`'s, and the printer's and the
//! reader's), and nothing in this crate calls them.
//!
//! What a board has not got is not here: files and the environment
//! (`typelisp-rt-sys`), sockets and OS threads (`typelisp-rt-posix`), and
//! `eval` (`typelisp-front`).
//!
//! What it adds of its own is the start of the program and the memory it
//! allocates from ([`start`]), and what happens on a panic or a fault. The
//! rest of what the runtime asks of the board (`typelisp_host_*`) the board
//! gives as C functions, or `typelisp-rt-semihosting`, which is this library
//! with them over semihosting.
//!
//! Only for a target without an operating system (`target_os = "none"`)
//! does the crate have anything in it; built for any other (the host, when
//! the whole workspace is built), it is an empty library with std.

#![cfg_attr(target_os = "none", no_std)]

#[cfg(target_os = "none")]
extern crate typelisp_abi;
#[cfg(target_os = "none")]
extern crate typelisp_mem;
#[cfg(target_os = "none")]
extern crate typelisp_print;
#[cfg(target_os = "none")]
extern crate typelisp_read;
#[cfg(target_os = "none")]
extern crate typelisp_rt;
// Named for what it links in and nothing here calls: the one core's
// critical section (`critical-section-single-core`), which the allocator
// takes.
#[cfg(target_os = "none")]
extern crate cortex_m;

#[cfg(target_os = "none")]
mod panic {
    use core::fmt::Write;

    /// Standard error as a `fmt::Write` that allocates nothing: a panic may
    /// be an allocation failing.
    pub(crate) struct Stderr;

    impl Write for Stderr {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let _ = typelisp_mem::host::write_stderr(s.as_bytes());
            Ok(())
        }
    }

    /// Reports a Rust panic of the runtime's own and ends the program with
    /// the code a typelisp panic ends it with.
    #[panic_handler]
    fn panic(info: &core::panic::PanicInfo) -> ! {
        let _ = writeln!(Stderr, "{}", info);
        typelisp_mem::host::exit(typelisp_rt::EXIT_CODE_PANIC as i32)
    }

    /// Where an unwind out of a call would start. The runtime calls compiled
    /// code through `extern "C-unwind"` pointers, and the frames that do keep
    /// a landing pad that stops an unwind, which names this function. Panics
    /// abort here, so nothing unwinds and nothing calls it; an unwind that
    /// did come would have nowhere to go but the end of the program.
    #[no_mangle]
    extern "C" fn rust_eh_personality() -> ! {
        unwound()
    }

    /// The same for the ARM exception tables' own entry for a frame
    /// (`.ARM.exidx`), which every function's table entry names.
    #[no_mangle]
    extern "C" fn __aeabi_unwind_cpp_pr0() -> ! {
        unwound()
    }

    fn unwound() -> ! {
        let _ = writeln!(Stderr, "an unwind started, and panics abort on this target");
        typelisp_mem::host::abort()
    }
}

/// The start of the program, and the memory the runtime allocates from.
///
/// `cortex-m-rt` gives the vector table and `Reset`, which readies `.data`
/// and `.bss` and calls [`start`](start::start). The memory between the end of
/// `.bss` (`__sheap`) and the stack is the heap: the stack takes
/// `_typelisp_stack_size` bytes below the top of RAM (`_stack_start`), a
/// symbol the linker script typl links with defines.
#[cfg(target_os = "none")]
pub mod start {
    use core::alloc::{GlobalAlloc, Layout};
    use core::fmt::Write;

    use embedded_alloc::LlffHeap;

    use crate::panic::Stderr;

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

    /// Gives the heap the memory between `.bss` and the stack, runs the
    /// program, and ends with what it answers.
    #[cortex_m_rt::entry]
    fn start() -> ! {
        // Only the symbols' addresses are taken, which the linker script
        // gives.
        let (heap_start, stack_top, stack_size) = (
            core::ptr::addr_of!(__sheap) as usize,
            core::ptr::addr_of!(_stack_start) as usize,
            core::ptr::addr_of!(_typelisp_stack_size) as usize,
        );
        let heap_end = match stack_top.checked_sub(stack_size) {
            Some(end) if end > heap_start => end,
            _ => {
                let _ = writeln!(
                    Stderr,
                    "no room for a heap: the stack takes {} bytes below {:#x}, and .bss ends at {:#x}",
                    stack_size, stack_top, heap_start
                );
                typelisp_mem::host::exit(typelisp_rt::EXIT_CODE_PANIC as i32)
            }
        };
        // SAFETY: once, before anything allocates, with memory nothing else
        // uses.
        unsafe { HEAP.init(heap_start, heap_end - heap_start) };
        typelisp_rt::interrupt::set_controller(&NVIC);
        // SAFETY: the program typl compiled, which takes nothing.
        let code = unsafe { typelisp_main() };
        typelisp_mem::host::exit(code)
    }

    /// `size` bytes aligned to `align` from the heap, or null when there are
    /// none left.
    #[no_mangle]
    unsafe extern "C" fn typelisp_host_alloc(size: usize, align: usize) -> *mut u8 {
        // SAFETY: the runtime's allocator passes a size and alignment that
        // came from a `Layout`.
        unsafe { HEAP.alloc(Layout::from_size_align_unchecked(size, align)) }
    }

    /// Gives back what [`typelisp_host_alloc`] returned for the same `size`
    /// and `align`.
    #[no_mangle]
    unsafe extern "C" fn typelisp_host_dealloc(ptr: *mut u8, size: usize, align: usize) {
        // SAFETY: as above, and `ptr` came from `typelisp_host_alloc`.
        unsafe { HEAP.dealloc(ptr, Layout::from_size_align_unchecked(size, align)) }
    }

    /// A fault (a bad address, an undefined instruction, the stack run into
    /// the heap) ends the program with the code a panic does, instead of
    /// `cortex-m-rt`'s default of spinning forever.
    #[cortex_m_rt::exception]
    unsafe fn HardFault(frame: &cortex_m_rt::ExceptionFrame) -> ! {
        let _ = writeln!(Stderr, "hard fault at pc {:#010x}", frame.pc());
        typelisp_mem::host::exit(typelisp_rt::EXIT_CODE_PANIC as i32)
    }

    /// Every other exception and interrupt: the program's handler for it
    /// (`typelisp_rt::interrupt`), or, when it has none, the end of the
    /// program the way a fault ends it. `irqn` is the entry's number in the
    /// vector table less 16.
    #[cortex_m_rt::exception]
    unsafe fn DefaultHandler(irqn: i16) {
        if typelisp_rt::interrupt::arrived((i32::from(irqn) + 16) as usize) {
            return;
        }
        let _ = writeln!(Stderr, "unhandled exception or interrupt {}", irqn);
        typelisp_mem::host::exit(typelisp_rt::EXIT_CODE_PANIC as i32)
    }

    /// An interrupt, by its number, as the NVIC takes one.
    #[derive(Clone, Copy)]
    struct Irq(u16);

    // SAFETY: the number is the interrupt's own, which the program named and
    // the runtime checked is below 240.
    unsafe impl cortex_m::interrupt::InterruptNumber for Irq {
        fn number(self) -> u16 {
            self.0
        }
    }

    /// Masking and unmasking an interrupt, for the runtime's interrupt
    /// handlers. All interrupts keep the priority they reset to, the same
    /// for all, so that a handler is never interrupted by another.
    static NVIC: typelisp_rt::interrupt::Controller = typelisp_rt::interrupt::Controller {
        mask: |n| cortex_m::peripheral::NVIC::mask(Irq(n)),
        // SAFETY: unmasking breaks no critical section: the runtime holds
        // none, and an interrupt that comes then waits for the runtime.
        unmask: |n| unsafe { cortex_m::peripheral::NVIC::unmask(Irq(n)) },
    };
}
