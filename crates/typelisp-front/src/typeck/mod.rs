//! The rebuilt type checker (`docs/dev/checker-rebuild-plan.md`).
//!
//! It is built beside the current one (`crate::check`) and replaces it once
//! every test passes through it. Each module does one thing and talks to the
//! others only through [`crate::types::Type`] and the types it exports.
//!
//! - [`toplevel`]: the entry point, one top-level form at a time.
//! - [`defun`]: defining a function.
//! - [`defstruct`]: defining a struct type and its accessors.
//! - [`defenum`]: defining a sum type.
//! - [`defmethod`]: defining a method outside an `impl`.
//! - [`impl_block`]: an inherent `impl`, the methods of one type.
//! - [`generic`]: specializations of generic definitions.
//! - [`expr`]: which kind of form an expression is, and bodies.
//! - [`literal`]: the atoms that are their own value, and their types.
//! - [`binding`]: `let` and `progn`.
//! - [`control`]: `if`.
//! - [`matching`]: `match`.
//! - [`pattern`]: a `match` arm's pattern.
//! - [`exhaustive`]: whether a `match`'s arms cover every value.
//! - [`call`]: calls to functions.
//! - [`method`]: calls to methods.
//! - [`path_call`]: calls whose head is a `::` path.
//! - [`instance`]: `make-instance`, and reading a struct's field.
//! - [`variant`]: a sum type's value.
//! - [`use_form`]: `use`, and what it registers for reading.
//! - [`resolve`]: what a name written in a module means.
//! - [`locals`]: local variables.
//! - [`scope`]: which type parameters a name means where a type is written.
//! - [`type_syntax`]: reading a written type.
//! - [`subst`]: replacing declared type parameters with types.
//! - [`unify`]: inference holes and the unification that solves them.
//! - [`registry`]: what the program has defined, kept only when a form checks.
//! - [`bridge`]: the current checker's registry copied in, while the two
//!   checkers stand side by side.
//! - [`tree`]: the typed tree checking produces.
//! - [`lower`]: the typed tree to core IR.

pub(crate) mod binding;
pub(crate) mod bridge;
pub(crate) mod call;
pub(crate) mod control;
pub(crate) mod defenum;
pub(crate) mod defmethod;
pub(crate) mod defstruct;
pub(crate) mod defun;
pub(crate) mod exhaustive;
pub(crate) mod expr;
pub(crate) mod generic;
pub(crate) mod impl_block;
pub(crate) mod instance;
pub(crate) mod literal;
pub(crate) mod locals;
pub(crate) mod lower;
pub(crate) mod matching;
pub(crate) mod method;
pub(crate) mod path_call;
pub(crate) mod pattern;
pub(crate) mod registry;
pub(crate) mod resolve;
pub(crate) mod scope;
pub(crate) mod subst;
pub(crate) mod toplevel;
pub(crate) mod tree;
pub(crate) mod type_syntax;
pub(crate) mod unify;
pub(crate) mod use_form;
pub(crate) mod variant;
