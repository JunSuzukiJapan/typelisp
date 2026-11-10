//! The rebuilt type checker (`docs/dev/checker-rebuild-plan.md`).
//!
//! It is built beside the current one (`crate::check`) and replaces it once
//! every test passes through it. Each module does one thing and talks to the
//! others only through [`crate::types::Type`] and the types it exports.
//!
//! - [`toplevel`]: the entry point, one top-level form at a time.
//! - [`define`]: whether a definition may take its name.
//! - [`defun`]: defining a function.
//! - [`defvar`]: global variables.
//! - [`defstruct`]: defining a struct type and its accessors.
//! - [`defenum`]: defining a sum type.
//! - [`defmethod`]: defining a method outside an `impl`.
//! - [`deftrait`]: defining a trait.
//! - [`impl_block`]: an inherent `impl`, the methods of one type.
//! - [`trait_impl`]: an `impl` of a trait for a type.
//! - [`generic`]: specializations of generic definitions.
//! - [`expr`]: which kind of form an expression is, and bodies.
//! - [`literal`]: the atoms that are their own value, and their types.
//! - [`binding`]: `let` and `progn`.
//! - [`control`]: `if`.
//! - [`matching`]: `match`.
//! - [`the`]: `the`.
//! - [`widen`]: a value where another type is expected, widened.
//! - [`quote`]: `quote`.
//! - [`looping`]: `loop`, `break` and `return`.
//! - [`setf`]: `setf`, assigning a place.
//! - [`format`]: `format`, `print` and `println`.
//! - [`sexpr_value`]: a value held as a `Sexpr`, encoded by its type.
//! - [`pattern`]: a `match` arm's pattern.
//! - [`exhaustive`]: whether a `match`'s arms cover every value.
//! - [`macros`]: macro calls, expanded.
//! - [`call`]: calls to functions.
//! - [`method`]: calls to methods.
//! - [`path_call`]: calls whose head is a `::` path.
//! - [`instance`]: `make-instance`, and reading a struct's field.
//! - [`variant`]: a sum type's value.
//! - [`use_form`]: `use`, and what it registers for reading.
//! - [`module`]: `module`, forms checked in a module.
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

pub(crate) mod args;
pub(crate) mod binding;
pub(crate) mod bounds;
pub(crate) mod bridge;
pub(crate) mod call;
pub(crate) mod cast;
pub(crate) mod collection_literal;
pub(crate) mod control;
pub(crate) mod define;
pub(crate) mod defenum;
pub(crate) mod defmacro;
pub(crate) mod defmethod;
pub(crate) mod defstruct;
pub(crate) mod deftrait;
pub(crate) mod defun;
pub(crate) mod defvar;
pub(crate) mod dyn_trait;
pub(crate) mod exhaustive;
pub(crate) mod format;
pub(crate) mod expr;
pub(crate) mod generic;
pub(crate) mod impl_block;
pub(crate) mod instance;
pub(crate) mod lambda;
pub(crate) mod lambda_list;
pub(crate) mod library;
pub(crate) mod list_form;
pub(crate) mod literal;
pub(crate) mod locals;
pub(crate) mod looping;
pub(crate) mod lower;
pub(crate) mod macros;
pub(crate) mod matching;
pub(crate) mod method;
pub(crate) mod modify;
pub(crate) mod module;
pub(crate) mod path_call;
pub(crate) mod pattern;
pub(crate) mod quote;
pub(crate) mod registry;
pub(crate) mod resolve;
pub(crate) mod scope;
pub(crate) mod select;
pub(crate) mod setf;
pub(crate) mod spawn;
pub(crate) mod sexpr_value;
pub(crate) mod subst;
pub(crate) mod sugar;
pub(crate) mod the;
pub(crate) mod tools;
pub(crate) mod toplevel;
pub(crate) mod trait_impl;
pub(crate) mod tree;
pub(crate) mod type_syntax;
pub(crate) mod unify;
pub(crate) mod use_form;
pub(crate) mod variant;
pub(crate) mod widen;
pub(crate) mod wire;
