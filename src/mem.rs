//! The GC-managed cons heap now lives in the `typelisp-mem` crate (see its
//! doc comment for why) — re-exported here unchanged so every existing
//! `crate::mem::*`/`crate::Heap`/`crate::Value`/... reference elsewhere in
//! this crate keeps working.
pub use typelisp_mem::*;
