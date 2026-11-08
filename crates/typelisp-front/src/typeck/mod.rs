//! The rebuilt type checker (`docs/dev/checker-rebuild-plan.md`).
//!
//! It is built beside the current one (`crate::check`) and replaces it once
//! every test passes through it. Each module does one thing and talks to the
//! others only through [`crate::types::Type`] and the types it exports.
//!
//! - [`toplevel`]: the entry point, one top-level form at a time.
//! - [`defun`]: defining a function.
//! - [`expr`]: which kind of form an expression is, and bodies.
//! - [`literal`]: the atoms that are their own value, and their types.
//! - [`binding`]: `let` and `progn`.
//! - [`control`]: `if`.
//! - [`call`]: calls to the functions the program defined.
//! - [`locals`]: local variables.
//! - [`type_syntax`]: reading a written type.
//! - [`registry`]: what the program has defined, kept only when a form checks.
//! - [`tree`]: the typed tree checking produces.
//! - [`lower`]: the typed tree to core IR.

pub(crate) mod binding;
pub(crate) mod call;
pub(crate) mod control;
pub(crate) mod defun;
pub(crate) mod expr;
pub(crate) mod literal;
pub(crate) mod locals;
pub(crate) mod lower;
pub(crate) mod registry;
pub(crate) mod toplevel;
pub(crate) mod tree;
pub(crate) mod type_syntax;
