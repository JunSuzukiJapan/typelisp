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
pub mod registry;
mod info;
mod lower;
mod verify;
pub mod analyze;
mod fold;
pub mod loop_dsl;
pub mod locate;
pub mod semantic;

pub use resolved::{CompileTarget, Pattern};
pub use registry::{AdtDef, AdtKind, AssocFn, CField, CStructDef, DefLocs, Docs, FnSig, MacroDef, Namespace, Registry, TypeAlias, VarInfo, Variant};
pub use analyze::{Checker, MacroExpander, MacroLambda, RedefPolicy, TestCase, TestForms, TestReturn, TopLevelForm, MONO_BUNDLE_MODULE, SPECIAL_FORM_HEADS};
pub use locate::{
    completion_candidates, completion_locals, definition_target, doc_for, hover_text, locate_node, CompletionCandidate,
    CompletionKind,
};
