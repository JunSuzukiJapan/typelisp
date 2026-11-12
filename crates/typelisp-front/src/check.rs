//! What the checker (`crate::Checker`) builds and reports, apart from the
//! checking itself: the cons-cell core IR it lowers each form to ([`core`],
//! [`forms`], `lower`, [`repr`]), the messages for what it refuses
//! (`verify`), and what the editor reads of a checked session ([`locate`],
//! [`semantic`]).

pub mod resolved;
pub mod core;
pub mod freevars;
pub mod forms;
pub mod repr;
pub(crate) mod lower;
pub(crate) mod verify;
pub(crate) mod fold;
pub mod loop_dsl;
pub mod locate;
pub mod semantic;

pub(crate) use verify::{bound_message, bound_name, not_dyn};
pub use resolved::CompileTarget;
pub use locate::{
    completion_locals, definition_target, doc_for, hover_text, locate_node, CompletionCandidate,
    CompletionKind, DefLocs, Defined, DefinedKind, Docs, Imported,
};
