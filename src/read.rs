//! The reader now lives in the `typelisp-read` crate (see its doc comment for
//! why) — re-exported here unchanged so every existing `crate::read::…` /
//! `crate::Reader` reference elsewhere in this crate keeps working.
pub use typelisp_read::reader;

pub use reader::*;
