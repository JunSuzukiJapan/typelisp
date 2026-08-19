//! `Error` now lives in the `typelisp-mem` crate (see its doc comment for
//! why) — re-exported here unchanged so every existing `crate::Error`/
//! `crate::errors::Error` reference elsewhere in this crate keeps working.
pub use typelisp_mem::Error;
