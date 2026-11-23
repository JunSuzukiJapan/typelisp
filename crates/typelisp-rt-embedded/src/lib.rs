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
//! allocates from ([`start`]), what happens on a panic or a fault, and with
//! the feature `semihosting` the rest of the board's functions, over
//! semihosting ([`semihosting`]).
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

    /// Any other exception or interrupt nothing handles, the same way.
    #[cortex_m_rt::exception]
    unsafe fn DefaultHandler(irqn: i16) {
        let _ = writeln!(Stderr, "unhandled exception or interrupt {}", irqn);
        typelisp_mem::host::exit(typelisp_rt::EXIT_CODE_PANIC as i32)
    }
}

/// The board's functions but the allocator's, over semihosting: the host a
/// debugger or QEMU (`-semihosting`) is. What the tests run with.
#[cfg(all(target_os = "none", feature = "semihosting"))]
pub mod semihosting {
    use core::cell::Cell;
    use core::sync::atomic::{AtomicUsize, Ordering};

    use cortex_m::interrupt::{self, Mutex};

    use cortex_m_semihosting::{nr, syscall, syscall1};

    /// `SYS_EXIT_EXTENDED`: ends with an exit code, where `SYS_EXIT` on a
    /// 32-bit core can say only success or failure.
    const EXIT_EXTENDED: usize = 0x20;

    /// `ADP_Stopped_ApplicationExit`: the reason an ending program gives.
    const APPLICATION_EXIT: usize = 0x20026;

    /// A handle not opened yet.
    const UNOPENED: usize = usize::MAX;

    /// The host's handles for descriptors 0, 1 and 2, opened on first use.
    static HANDLES: [AtomicUsize; 3] = [AtomicUsize::new(UNOPENED), AtomicUsize::new(UNOPENED), AtomicUsize::new(UNOPENED)];

    /// The host's handle for descriptor `fd`: its console (`:tt`), opened
    /// for reading for 0, writing for 1 and appending for 2, as the
    /// semihosting specification has standard input, output and error.
    fn handle(fd: i32) -> usize {
        let (slot, mode) = match fd {
            0 => (0, nr::open::R),
            1 => (1, nr::open::W_TRUNC),
            2 => (2, nr::open::W_APPEND),
            other => panic!("descriptor {} is not one of the board's", other),
        };
        let open = HANDLES[slot].load(Ordering::Relaxed);
        if open != UNOPENED {
            return open;
        }
        let name = b":tt\0";
        // SAFETY: `SYS_OPEN` reads the name, its length without the NUL, and
        // the mode.
        let opened = unsafe { syscall(nr::OPEN, &[name.as_ptr() as usize, mode, name.len() - 1]) };
        HANDLES[slot].store(opened, Ordering::Relaxed);
        opened
    }

    #[no_mangle]
    extern "C" fn typelisp_host_write(fd: i32, ptr: *const u8, len: usize) {
        let h = handle(fd);
        let (mut ptr, mut len) = (ptr, len);
        while len > 0 {
            // SAFETY: `SYS_WRITE` reads `len` bytes at `ptr`, which the
            // caller keeps while the call lasts. It answers how many it did
            // not write.
            let left = unsafe { syscall(nr::WRITE, &[h, ptr as usize, len]) };
            if left >= len {
                return;
            }
            // SAFETY: within the caller's bytes.
            ptr = unsafe { ptr.add(len - left) };
            len = left;
        }
    }

    #[no_mangle]
    extern "C" fn typelisp_host_read(fd: i32, ptr: *mut u8, len: usize) -> usize {
        let h = handle(fd);
        // SAFETY: `SYS_READ` writes at most `len` bytes at `ptr`. It answers
        // how many it did not read.
        let left = unsafe { syscall(nr::READ, &[h, ptr as usize, len]) };
        len.saturating_sub(left)
    }

    #[no_mangle]
    extern "C" fn typelisp_host_exit(code: i32) -> ! {
        // SAFETY: `SYS_EXIT_EXTENDED` reads the reason and the code.
        unsafe { syscall(EXIT_EXTENDED, &[APPLICATION_EXIT, code as usize]) };
        // A host that answers semihosting does not come back from it.
        loop {
            cortex_m::asm::wfi();
        }
    }

    /// The host's clock that never goes back, in its ticks.
    fn elapsed_ticks() -> u64 {
        let mut words = [0u32; 2];
        // SAFETY: `SYS_ELAPSED` writes the 64-bit count, low word first,
        // into the two words.
        unsafe { syscall1(nr::ELAPSED, words.as_mut_ptr() as usize) };
        u64::from(words[0]) | (u64::from(words[1]) << 32)
    }

    #[no_mangle]
    extern "C" fn typelisp_host_now_us() -> u64 {
        // SAFETY: `SYS_TICKFREQ` takes nothing.
        let per_second = unsafe { syscall1(nr::TICKFREQ, 0) } as u128;
        (u128::from(elapsed_ticks()) * 1_000_000 / per_second.max(1)) as u64
    }

    /// The Unix time in microseconds when [`typelisp_host_unix_us`] was first
    /// asked, and the monotonic clock then: `SYS_TIME` says whole seconds,
    /// and the clock gives the rest.
    static UNIX_BASE: Mutex<Cell<Option<(u64, u64)>>> = Mutex::new(Cell::new(None));

    #[no_mangle]
    extern "C" fn typelisp_host_unix_us() -> i64 {
        let now = typelisp_host_now_us();
        let (unix, clock) = interrupt::free(|cs| {
            let base = UNIX_BASE.borrow(cs);
            if let Some(known) = base.get() {
                return known;
            }
            // SAFETY: `SYS_TIME` takes nothing.
            let secs = unsafe { syscall1(nr::TIME, 0) } as u64;
            let known = (secs * 1_000_000, now);
            base.set(Some(known));
            known
        });
        (unix + (now - clock)) as i64
    }

    #[no_mangle]
    extern "C" fn typelisp_host_sleep_us(us: u64) {
        let until = typelisp_host_now_us().saturating_add(us);
        while typelisp_host_now_us() < until {}
    }
}
