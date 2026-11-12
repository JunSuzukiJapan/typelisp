//! The type checker: read `Sexpr` -> cons-cell core IR, with checking against
//! the built-in `Option<T>` / `Sexpr` data types and user-defined ones.
//!
//! See [`analyze::Checker`] for the entry point. A form is analyzed
//! ([`analyze`]), checked (`verify`), and lowered (`lower`) to the cons-cell
//! core IR ([`core`]) that the interpreter walks.

pub mod resolved;
pub mod core;
pub mod freevars;
pub mod forms;
pub mod repr;
mod info;
pub(crate) mod lower;
pub(crate) mod verify;
pub mod analyze;
pub(crate) mod fold;
pub mod loop_dsl;
pub mod locate;
pub mod semantic;

pub(crate) use verify::{bound_message, bound_name, not_dyn};
pub use resolved::CompileTarget;
pub use analyze::{Checker, MacroExpander, MacroLambda, RedefPolicy, TestCase, TestForms, TestReturn, TopLevelForm, MONO_BUNDLE_MODULE, SPECIAL_FORM_HEADS};
pub use locate::{
    completion_locals, definition_target, doc_for, hover_text, locate_node, CompletionCandidate,
    CompletionKind, DefLocs, Defined, DefinedKind, Docs, Imported,
};
