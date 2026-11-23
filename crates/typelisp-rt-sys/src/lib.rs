//! The part of typelisp's runtime that needs files and a process environment:
//! what WASI also has, and a microcontroller has not
//! (`docs/dev/runtime-split-plan.md`).
//!
//! - [`file`]: a file as a stream, in `typelisp_rt`'s stream table.
//! - [`fs_builtin`]: `open` and the `file-*` builtins.
//! - [`env`]: the command line, environment variables, the home directory.
//! - [`dribble`]: the file `dribble` copies the session into.
//!
//! Each builtin has one implementation, which the interpreter calls and the
//! compiled-code shims ([`shims`]) call.

pub mod dribble;
pub mod env;
pub mod file;
pub mod fs_builtin;
mod shims;

pub use shims::*;
