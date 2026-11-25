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
    struct Stderr;

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
    use core::sync::atomic::{AtomicPtr, Ordering};

    use embedded_alloc::LlffHeap;

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

    /// Runs the program and ends with what it answers.
    #[cortex_m_rt::entry]
    fn start() -> ! {
        // SAFETY: the program typl compiled, which takes nothing.
        let code = unsafe { typelisp_main() };
        typelisp_mem::host::exit(code)
    }

    /// Gives the heap the memory between `.bss` and the stack, and the
    /// runtime the interrupt controller. The first thing the program does,
    /// when it uses the runtime at all: one that only drives registers has
    /// neither.
    ///
    /// # Safety
    ///
    /// Called once, before anything allocates.
    #[no_mangle]
    unsafe extern "C" fn typelisp_board_start() {
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
                let (size, top, bss) = (hex(stack_size as u32), hex(stack_top as u32), hex(heap_start as u32));
                die(&[b"no room for a heap: the stack takes ", &size, b" bytes below ", &top, b", and .bss ends at ", &bss])
            }
        };
        // SAFETY: once, before anything allocates, with memory nothing else
        // uses.
        unsafe { HEAP.init(heap_start, heap_end - heap_start) };
        typelisp_mem::memory::set_capacity(heap_end - heap_start);
        typelisp_rt::interrupt::set_controller(&NVIC);
        TYPELISP_ARRIVED.store(typelisp_rt::interrupt::arrived as *mut (), Ordering::SeqCst);
    }

    /// `typelisp_rt::interrupt::arrived`, once the program has started the
    /// runtime: a program that starts none has no handler for anything, and
    /// the runtime's handlers are not linked into it. Under a name of its
    /// own, so that the compiler does not conclude from this crate being the
    /// only one to store it that it can only ever be that one function, and
    /// name the function in the handler after all.
    #[no_mangle]
    static TYPELISP_ARRIVED: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

    /// Ends the program with the code a panic ends it with, having written
    /// `parts` and a newline on standard error. Not through `core::fmt`,
    /// which every program would carry for it.
    fn die(parts: &[&[u8]]) -> ! {
        for part in parts {
            let _ = typelisp_mem::host::write_stderr(part);
        }
        let _ = typelisp_mem::host::write_stderr(b"\n");
        typelisp_mem::host::exit(typelisp_rt::EXIT_CODE_PANIC as i32)
    }

    /// `n` as `0x` and eight hexadecimal digits.
    fn hex(n: u32) -> [u8; 10] {
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
        die(&[b"hard fault at pc ", &hex(frame.pc())])
    }

    /// Every other exception and interrupt: the program's handler for it
    /// (`typelisp_rt::interrupt`), or, when it has none, the end of the
    /// program the way a fault ends it. `irqn` is the entry's number in the
    /// vector table less 16.
    #[cortex_m_rt::exception]
    unsafe fn DefaultHandler(irqn: i16) {
        let arrived = TYPELISP_ARRIVED.load(Ordering::SeqCst);
        if !arrived.is_null() {
            // SAFETY: only ever `typelisp_rt::interrupt::arrived`.
            let arrived: fn(usize) -> bool = unsafe { core::mem::transmute(arrived) };
            if arrived((i32::from(irqn) + 16) as usize) {
                return;
            }
        }
        // An exception's number is negative, down to -14.
        let sign: &[u8] = if irqn < 0 { b"-" } else { b"" };
        let digits = decimal(irqn.unsigned_abs());
        let shown = digits.iter().position(|d| *d != b'0').unwrap_or(2);
        die(&[b"unhandled exception or interrupt ", sign, digits.get(shown..).unwrap_or(&digits)])
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
