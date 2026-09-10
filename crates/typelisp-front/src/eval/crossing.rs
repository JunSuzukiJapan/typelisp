//! The interpreter's side of the boundary with compiled code: how a failure
//! that starts on one side becomes a failure on the other.
//!
//! Two directions, and they are not symmetric:
//!
//! - the interpreter calls compiled code, which panics/throws
//!   ([`catch_compiled_panic`]);
//! - compiled code calls back into the interpreter, which fails
//!   ([`unwind_interpreted_failure`]).
//!
//! Nothing here knows about LLVM. It is the *evaluator's* boundary, phrased in
//! `EvalError` and the three payload types `typelisp-rt` raises, which is why
//! it sits here rather than in `crate::compile` where it used to.

use super::value::EvalError;

/// Runs `call` — a call into compiled code — and turns a failure that unwound
/// out of it back into an ordinary `Err` instead of letting it keep going.
///
/// This is the boundary that makes compiled failures recoverable, and the
/// reason the process no longer aborts on them. Every place the interpreter
/// enters compiled code has to go through it, or the unwind continues into
/// interpreter frames that never expected it. Three payloads arrive here, from
/// the two directions the boundary is crossed:
///
/// - [`typelisp_rt::CompiledPanic`] — a `(panic ...)` in compiled code.
///   Rebuilt as `EvalError::Panic`, matching what the interpreted path
///   produces for the same form.
/// - [`typelisp_rt::CompiledThrow`] — a `(throw 'tag v)` in compiled code that
///   no compiled `catch` claimed. Rebuilt as `EvalError::Throw`, so an
///   *interpreted* `(catch 'tag ...)` further up catches it exactly as it
///   would an interpreted throw.
/// - [`typelisp_rt::InterpretedUnwind`] — compiled code called *back* into the
///   interpreter (`rt_apply_any`/`rt_dyn_call`) and the interpreter failed.
///   The original `EvalError` travels whole, so a `Break`, an `Internal` and a
///   `Panic` stay distinguishable.
///
/// **Anything else is re-raised unchanged.** A payload from neither source is
/// a genuine bug in the runtime or in LLVM-generated code, and swallowing one
/// would turn a crash into a plausible-looking `EvalError` and hide it.
///
/// `AssertUnwindSafe` is sound here for the same reason the call is: the
/// `Heap` compiled code mutates is reached through `runtime::active_heap`'s
/// raw pointer, not through anything captured by this closure, and the caller
/// repairs the root stack afterwards.
///
/// The caller is responsible for the GC root stack. Compiled code pushes
/// roots as it runs and pops them on the way out; an unwind skips every one
/// of those pops, so a caller that catches here must truncate the root stack
/// back to the depth it recorded before the call
/// (`Heap::root_count`/`Heap::truncate_roots` — the same pair a compiled
/// `break`/`return` already unwinds through).
pub fn catch_compiled_panic<R>(call: impl FnOnce() -> R) -> Result<R, EvalError> {
    // The frames entering calls published and their drivers never took: an
    // unwind travels through that window, so it leaves them behind. The
    // compiled tier's own catcher (`typelisp_rt`'s `protected`) restores this
    // for the same reason — see `truncate_current_frames`.
    let frames = typelisp_abi::call_state::current_frame_depth();
    let payload = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)) {
        Ok(v) => return Ok(v),
        Err(payload) => payload,
    };
    typelisp_abi::call_state::truncate_current_frames(frames);
    let payload = match payload.downcast::<typelisp_rt::CompiledPanic>() {
        Ok(p) => return Err(EvalError::Panic(p.message)),
        Err(other) => other,
    };
    let payload = match payload.downcast::<typelisp_rt::CompiledThrow>() {
        // SAFETY: the heap the throw's value belongs to is the one registered
        // for the call that just unwound, and it is still registered here —
        // this runs before the caller repairs anything.
        Ok(_) => match unsafe { typelisp_rt::take_throw() } {
            Some((tag, value)) => return Err(EvalError::Throw(tag, Box::new(value))),
            None => {
                return Err(EvalError::Internal("a compiled throw arrived with nothing parked for it".to_string()))
            }
        },
        Err(other) => other,
    };
    match payload.downcast::<typelisp_rt::InterpretedUnwind>() {
        Ok(_) => Err(take_interpreted_error()
            .unwrap_or_else(|| EvalError::Internal("an interpreted unwind arrived with no error parked for it".to_string()))),
        Err(other) => std::panic::resume_unwind(other),
    }
}

thread_local! {
    /// Where an interpreted callee's error waits while the unwind that
    /// announces it travels out through the compiled frames in between.
    ///
    /// Thread-local rather than a panic payload because `EvalError` is not
    /// `Send` — see `typelisp_rt::InterpretedUnwind`. A thread-local is not a
    /// weaker guarantee here but the exact one: the unwind that carries the
    /// announcement runs on this same thread, between the `park` below and
    /// the `take` in [`catch_compiled_panic`], and cannot be observed
    /// anywhere else.
    static INTERPRETED_ERROR: std::cell::RefCell<Option<EvalError>> = const { std::cell::RefCell::new(None) };
}

/// Parks `error` for the [`catch_compiled_panic`] that will catch the unwind
/// about to be raised for it, and returns so the caller can raise it.
///
/// Only `Interp`'s `rt_apply_any`/`rt_dyn_call` hooks call this, immediately
/// before `typelisp_rt::unwind_interpreted_error`. Nesting is fine: each
/// compiled call has its own catch, so the innermost one takes what the
/// innermost failure parked.
pub fn park_interpreted_error(error: EvalError) {
    INTERPRETED_ERROR.with(|cell| *cell.borrow_mut() = Some(error));
}

/// Takes back what [`park_interpreted_error`] left, clearing the slot.
fn take_interpreted_error() -> Option<EvalError> {
    INTERPRETED_ERROR.with(|cell| cell.borrow_mut().take())
}

/// Announces an interpreted callee's failure to the compiled frames that
/// called it, by unwinding — the one way back out, since a compiled caller has
/// no `Result` channel to return an `EvalError` through.
///
/// A `throw` takes the *throw* channel rather than this one. It has to: a
/// compiled `catch` further up asks `rt_throw_matches` whether the unwind in
/// flight carries its tag, and an interpreted throw announced as a generic
/// interpreted error would answer no and travel straight past a `catch` that
/// should have claimed it. Parking it as a throw is what makes "the throw
/// crossed an interpreted frame on its way" invisible to the catcher, which is
/// the whole point of a dynamic exit.
///
/// Only `Interp`'s `rt_apply_any`/`rt_dyn_call` hooks call this.
///
/// # Safety
///
/// A `Heap` must be registered on this thread — the one `error`'s values
/// belong to.
pub unsafe fn unwind_interpreted_failure(error: EvalError) -> ! {
    match error {
        EvalError::Throw(tag, value) => {
            typelisp_rt::park_throw(tag, *value);
            typelisp_rt::unwind_throw()
        }
        other => {
            park_interpreted_error(other);
            typelisp_rt::unwind_interpreted_error()
        }
    }
}
