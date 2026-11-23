//! What the runtime asks of the machine it runs on: standard input, output
//! and error, ending the process, the time, sleeping, and the few facts about
//! the machine every target has an answer to (the local time zone, the CPU's
//! name, how many threads could run at once).
//!
//! Three implementations, one per kind of machine, each in a module of its
//! own with the same items:
//!
//! - [`os`]: `std` over an operating system — everywhere but the two below,
//!   WASI included.
//! - `js`: `wasm32-unknown-unknown`. Its `std` throws standard output and
//!   error away, panics on asking the time or sleeping, and traps on exiting,
//!   so the page or program that loaded the module provides them, as
//!   functions it imports from the module `typelisp` (what `typl -c --target
//!   wasm32-unknown-unknown` writes beside the module says how).
//! - [`board`]: without std (a microcontroller). The executable provides
//!   them as C functions (`typelisp_host_*`): the board's own way to read,
//!   write, stop and keep time.
//!
//! What only some targets have at all (files, sockets, OS threads) is not
//! here but in the runtime crates that bring it (`typelisp-rt-sys`,
//! `typelisp-rt-posix`). What is here every target answers, if only with
//! "unknown".
//!
//! In this crate, the lowest the runtime has, so that every crate above can
//! report through it.

#[cfg(all(feature = "std", not(all(target_family = "wasm", target_os = "unknown"))))]
pub mod os;
#[cfg(all(feature = "std", not(all(target_family = "wasm", target_os = "unknown"))))]
pub use os::*;

#[cfg(all(feature = "std", target_family = "wasm", target_os = "unknown"))]
mod js;
#[cfg(all(feature = "std", target_family = "wasm", target_os = "unknown"))]
pub use js::*;

#[cfg(not(feature = "std"))]
pub mod board;
#[cfg(not(feature = "std"))]
pub use board::*;

#[cfg(any(not(feature = "std"), all(target_family = "wasm", target_os = "unknown")))]
mod clock;
#[cfg(all(feature = "std", target_family = "wasm", target_os = "unknown"))]
use js::now;
#[cfg(not(feature = "std"))]
use board::now;

/// `eprintln!` through [`write_stderr`]: one line, and a failure to write it
/// ignored, as `eprintln!`'s own is not (it panics) — what is reported here
/// is already a failure being reported.
pub fn eprintln(args: core::fmt::Arguments) {
    let line = alloc::format!("{}\n", args);
    let _ = write_stderr(line.as_bytes());
}
