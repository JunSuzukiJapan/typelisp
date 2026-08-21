//! The type checker: read `Sexpr` -> cons-cell core IR, with checking against
//! the built-in `Option<T>` / `Sexpr` data types and user-defined ones.
//!
//! See [`checker::Checker`] for the entry point. The checker produces a
//! cons-cell core IR ([`core`]) that the interpreter (step 4) walks.

pub mod resolved;
pub mod core;
pub mod forms;
pub mod repr;
pub mod registry;
pub mod checker;
pub mod loop_dsl;
pub mod locate;
pub mod semantic;

pub use resolved::{CompileTarget, Pattern, Ref};
pub use registry::{AdtDef, AdtKind, AssocFn, DefLocs, Docs, FnSig, MacroDef, Namespace, Registry, VarInfo, Variant};
pub use checker::{Checker, MacroExpander, MacroLambda, RedefPolicy, TopLevelForm, MONO_BUNDLE_MODULE};
pub use locate::{
    completion_candidates, completion_locals, definition_target, doc_for, hover_text, locate_node, CompletionCandidate,
    CompletionKind,
};
