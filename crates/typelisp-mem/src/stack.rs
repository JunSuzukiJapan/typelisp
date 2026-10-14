//! Growing the stack where a walk over source recurses.
//!
//! The reader, the checker and the compiler's passes each recurse once per
//! level of nesting in what they walk, and a program (or a macro's expansion)
//! can nest deeper than any fixed thread stack holds. Each wraps its one
//! recursion point in [`grow_stack`], so how deep a program may nest is not
//! decided by the size of the thread it happens to run on.

/// What is left on the stack before it is grown: more than whatever runs
/// between two levels of a walk needs — a reader macro or a macro's
/// expansion among it, which are interpreted or compiled code of their own.
const RED_ZONE: usize = 1024 * 1024;

/// How much stack each growth adds.
const GROWTH: usize = 8 * 1024 * 1024;

/// Run `f`, on a fresh stack segment if this one is close to running out.
#[inline]
pub fn grow_stack<R>(f: impl FnOnce() -> R) -> R {
    stacker::maybe_grow(RED_ZONE, GROWTH, f)
}
