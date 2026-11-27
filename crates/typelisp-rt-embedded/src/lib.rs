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
//! What differs between the cores is in a module of each: how the program
//! is entered, how a fault or an interrupt arrives, and how one interrupt is
//! masked ([`cortex_m`] for an Arm Cortex-M, [`riscv`] for a 32-bit RISC-V
//! core). Everything else is [`start`], the same for all.
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

#[cfg(all(target_os = "none", target_arch = "arm"))]
pub mod cortex_m;
#[cfg(all(target_os = "none", target_arch = "riscv32"))]
pub mod riscv;
#[cfg(target_os = "none")]
pub mod start;

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
    #[cfg(target_arch = "arm")]
    #[no_mangle]
    extern "C" fn __aeabi_unwind_cpp_pr0() -> ! {
        unwound()
    }

    fn unwound() -> ! {
        let _ = writeln!(Stderr, "an unwind started, and panics abort on this target");
        typelisp_mem::host::abort()
    }
}
