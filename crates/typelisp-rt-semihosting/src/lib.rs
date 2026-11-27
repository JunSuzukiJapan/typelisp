//! The runtime of an executable for a board without an operating system,
//! with the board's functions (`typelisp_host_*` but the allocator's) over
//! semihosting: the host a debugger or QEMU (`-semihosting`) is. What `typl
//! -c --board-functions semihosting` links, and what the tests run with.
//!
//! The runtime is `typelisp-rt-embedded`, named here so that this library
//! carries it; a board that gives the functions as C functions links that
//! one instead.
//!
//! The calls are the same on every core ([`semihosting`]); only the
//! instruction that makes one is the core's ([`call`]).
//!
//! Only for a target without an operating system (`target_os = "none"`)
//! does the crate have anything in it; built for any other (the host, when
//! the whole workspace is built), it is an empty library with std.

#![cfg_attr(target_os = "none", no_std)]

#[cfg(target_os = "none")]
extern crate typelisp_rt_embedded;

/// The instruction that asks the host: what the semihosting specification
/// gives each core, the operation in the first argument register and the
/// address of its arguments (or the one argument) in the second, the answer
/// in the first.
#[cfg(target_os = "none")]
mod call {
    use core::arch::asm;

    /// Asks the host for operation `op` with `arg`.
    ///
    /// # Safety
    ///
    /// `arg` must be what `op` reads: the address of the words it takes,
    /// alive while the call lasts, or the one word.
    #[cfg(target_arch = "arm")]
    pub(crate) unsafe fn syscall1(op: usize, arg: usize) -> usize {
        let mut r0 = op;
        // SAFETY: the caller's; the host reads only what `op` names.
        unsafe { asm!("bkpt #0xab", inout("r0") r0, in("r1") arg, options(nostack, preserves_flags)) };
        r0
    }

    /// Asks the host for operation `op` with `arg`: the three instructions
    /// in a row, uncompressed, that tell a semihosting call from a
    /// breakpoint, aligned so that no page boundary falls between them.
    ///
    /// # Safety
    ///
    /// As for an Arm core.
    #[cfg(target_arch = "riscv32")]
    pub(crate) unsafe fn syscall1(op: usize, arg: usize) -> usize {
        let mut a0 = op;
        // SAFETY: the caller's; the host reads only what `op` names.
        unsafe {
            asm!(
                ".balign 16",
                ".option push",
                ".option norvc",
                "slli x0, x0, 0x1f",
                "ebreak",
                "srai x0, x0, 0x7",
                ".option pop",
                inout("a0") a0,
                in("a1") arg,
                options(nostack, preserves_flags),
            )
        };
        a0
    }

    /// Asks the host for operation `op` with the words `args`.
    ///
    /// # Safety
    ///
    /// `args` must be the words `op` reads.
    pub(crate) unsafe fn syscall(op: usize, args: &[usize]) -> usize {
        // SAFETY: the caller's; `args` lives while the call lasts.
        unsafe { syscall1(op, args.as_ptr() as usize) }
    }

    /// Waits for an interrupt, which a host that has ended the program never
    /// gives.
    pub(crate) fn wait() {
        // SAFETY: stops the core until an interrupt.
        unsafe { asm!("wfi", options(nomem, nostack, preserves_flags)) }
    }
}

/// The operations, as the semihosting specification numbers them.
#[cfg(target_os = "none")]
mod nr {
    pub(crate) const OPEN: usize = 0x01;
    pub(crate) const WRITE: usize = 0x05;
    pub(crate) const READ: usize = 0x06;
    pub(crate) const TIME: usize = 0x11;
    pub(crate) const ELAPSED: usize = 0x30;
    pub(crate) const TICKFREQ: usize = 0x31;

    /// `SYS_OPEN`'s modes.
    pub(crate) mod open {
        pub(crate) const R: usize = 0;
        pub(crate) const W_TRUNC: usize = 4;
        pub(crate) const W_APPEND: usize = 8;
    }
}

#[cfg(target_os = "none")]
mod semihosting {
    use core::cell::Cell;
    use core::sync::atomic::{AtomicUsize, Ordering};

    use critical_section::Mutex;

    use crate::call::{syscall, syscall1};
    use crate::nr;

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
            // Not one of the board's: the runtime names no other. Ended, not
            // a panic, whose message every program would carry `core::fmt`
            // for.
            _ => typelisp_host_exit(134),
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
            crate::call::wait();
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
        let (unix, clock) = critical_section::with(|cs| {
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
