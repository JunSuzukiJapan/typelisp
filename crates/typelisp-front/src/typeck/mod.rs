//! The rebuilt type checker (`docs/dev/checker-rebuild-plan.md`).
//!
//! It is built beside the current one (`crate::check`) and replaces it once
//! every test passes through it. Each module does one thing and talks to the
//! others only through [`crate::types::Type`] and the types it exports.
//!
//! - [`toplevel`]: the entry point, one top-level form at a time.
//! - [`expr`]: which kind of form an expression is.
//! - [`literal`]: the atoms that are their own value, and their types.
//! - [`tree`]: the typed tree checking produces.
//! - [`lower`]: the typed tree to core IR.

pub(crate) mod expr;
pub(crate) mod literal;
pub(crate) mod lower;
pub(crate) mod toplevel;
pub(crate) mod tree;
