//! A function handed to C as a callback — a `defffi` argument whose declared
//! type is `(fn (T...) R)`.
//!
//! C holds nothing but a code address and calls it with exactly the declared
//! arguments, so the function it is given can have no environment: there is
//! nowhere for C to pass one. Three spellings of such a function are
//! accepted, all written directly at the argument:
//!
//! * the name of a top-level function (a generic one is specialized from the
//!   declared type, as any function value is),
//! * a `lambda` with no free variables,
//! * the name of a `labels` function that, together with every sibling it
//!   reaches, has no free variables — siblings share one captured
//!   environment, so a sibling's capture is this function's too.
//!
//! A variable holding a function is refused: which function it holds, and so
//! whether it has an environment, is not known until it runs.
//!
//! The last two are **lifted**: a new top-level function carries the body
//! (for `labels`, a copy of the block that calls the function), so every
//! callback reaches the backend as a named top-level function and the entry
//! C calls never needs an environment. The lifted definitions travel ahead of
//! the form being checked, in the same bundle a generic's instantiations do
//! (`Checker::check_form_at`).

use super::*;
use crate::ffi_callback::CallbackSig;

impl Checker {
    /// Every callback entry the checked code asks for, in the order it first
    /// asked. Read by `compile::aot::compile_file`, which emits one entry per
    /// callback and registers it at startup.
    pub fn ffi_callbacks(&self) -> Vec<CallbackSig> {
        self.rebuilt.ffi_callbacks()
    }

}

