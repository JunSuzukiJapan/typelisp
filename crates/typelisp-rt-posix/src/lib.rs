//! The part of typelisp's runtime that needs a POSIX system: what WASI has
//! not (`docs/dev/runtime-split-plan.md`).
//!
//! - [`os`]: the C library calls `std` has no wrapper for.
//! - [`net`], [`net_builtin`], [`x509`]: sockets, TLS and name resolution, as
//!   backends in `typelisp_rt`'s stream table.
//! - [`threads`]: starting the OS threads the scheduler steps tasks from, and
//!   the program entries of an executable whose tasks run on them.
//! - [`sys`]: CPU time, the host's name and OS release, a file's owner,
//!   starting an editor.
//!
//! Each builtin has one implementation, which the interpreter calls and the
//! compiled-code shims ([`shims`]) call.

extern crate alloc;

pub mod net;
pub mod net_builtin;
pub mod os;
mod shims;
pub mod sys;
pub mod threads;
pub mod x509;

pub use shims::*;
pub use sys::{rt_ed_open, rt_file_owner_name, rt_get_internal_run_time, rt_machine_instance, rt_software_version};
pub use threads::{rt_run_program_threads, rt_run_program_threads_int};
