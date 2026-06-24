//! The type checker: read `Sexpr` -> typed AST, with checking against the
//! built-in `Option<T>` / `Sexpr` data types and (later) user-defined ones.
//!
//! See [`checker::Checker`] for the entry point. The checker produces a
//! [`ast::Typed`] tree that the interpreter (step 4) walks.

pub mod ast;
pub mod registry;
pub mod checker;

pub use ast::{Arm, Expr, Pattern, QuotedSexpr, Typed};
pub use registry::{AdtDef, AdtKind, AssocFn, FnSig, MacroDef, Namespace, Registry, VarInfo, Variant};
pub use checker::{Checker, MacroExpander, RedefPolicy, TopLevel};
