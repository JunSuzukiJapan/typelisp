//! Non-local exits: match failure, `panic`, and `catch`/`throw`.

use typelisp_mem::prelude::*;
use core::cell::RefCell;
use typelisp_abi::{active_heap, decode, encode, fatal, raise, CompiledPanic, CompiledThrow, InterpretedUnwind};
use typelisp_mem::Value;

/// `compiler.rs`'s `compile-match-arms` calls this once every arm's
/// pattern has failed to match — provably unreachable for a well-typed
/// program (the checker's own exhaustiveness check already guarantees one
/// of `nil`/`int`/`float`/`char`/`bool`/`sym`/`str`/`cons` always matches a
/// `Sexpr` scrutinee), so this is a trap for a `compiler.rs`/checker bug,
/// not a normal/recoverable runtime condition — there is nothing
/// meaningful to return.
///
/// # Safety
///
/// None beyond the ordinary compiled-function-ABI contract — unlike every
/// other `rt_*` function here, this one never touches the active `Heap`.
#[no_mangle]
pub unsafe extern "C" fn rt_match_fail() -> i64 {
    fatal("match: no pattern arm matched (the checker should have guaranteed exhaustiveness)")
}

/// `(panic msg)` for compiled code (`core_bridge`'s `panic` arm /
/// `compiler.rs`'s `compile-panic`): raises a [`CompiledPanic`] carrying
/// `msg`, so the boundary that entered compiled code can turn it back into
/// the same recoverable `EvalError::Panic` the interpreted path produces.
///
/// # Safety
///
/// , `message` must decode to a `Value::Str`; a `Heap`
/// must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_panic(message: i64) -> i64 {
    let message = match decode(message) {
        Value::Str(id) => active_heap().string(id).to_string(),
        _ => fatal("rt_panic: argument is not a Str"),
    };
    raise(message).into_word()
}

/// Whether an unwind was raised and is waiting (`typelisp_abi::raise`) —
/// what generated code asks right after a call that may have raised, before
/// it uses the answer (`compile::raise_check`). `1` sends it out of its
/// activation; the driver takes the unwind.
///
/// # Safety
///
/// None: it reads a thread-local and touches no argument.
#[no_mangle]
pub unsafe extern "C" fn rt_unwinding() -> i64 {
    i64::from(typelisp_abi::is_raised())
}

// ---- catch / throw / unwind-protect --------------------------------------
//
// A `throw` is raised like any other exit (`typelisp_abi::raise_payload`):
// its tag and value are parked here, a [`CompiledThrow`] says so, and the
// shim returns. The compiled code that called it leaves its activation with
// `STATUS_UNWIND`, and the driver (`coroutine::activation`) takes the
// payload. It can travel no further than that one activation, because a Lisp
// call is a driver round trip and not a machine call.
//
// Where it goes next is frame bookkeeping: the driver asks each frame,
// innermost first, whether it declares a handler
// (`typelisp_abi::FRAME_HANDLER_SLOT`), and resumes the first that does.

typelisp_mem::thread_local! {
    /// The tag of the throw currently in flight *for whichever task's `step`
    /// is running on this thread right now*. Its *value* lives in
    /// `Heap::set_in_flight_throw` instead, where the collector can see it —
    /// see [`CompiledThrow`].
    ///
    /// **Not this thread's for the whole time a task is merely parked.** A
    /// task whose `unwind-protect` cleanup suspends (`(sleep)`/`(recv)`/…)
    /// leaves this set while it is `Blocked`, and cooperative scheduling can
    /// run a *different* task's own throw on this same thread before it
    /// resumes — so every `TaskBody::step` saves this thread-local into the
    /// task itself before returning and restores it before running again
    /// ([`take_parked_unwind`]/[`restore_parked_unwind`]), the same way a
    /// task's continuation stack is put down and picked back up.
    static IN_FLIGHT_TAG: RefCell<Option<typelisp_mem::SymRef>> = const { RefCell::new(None) };
    /// The panic payload the driver caught and has not yet handed on: to a
    /// `catch` that claims it ([`rt_throw_take_value`], which drops it), to
    /// the interpreter as an `EvalError` ([`take_activation_unwind`]), or back
    /// to the unwinder ([`resume_activation_unwind`]) when the drive's caller
    /// is a machine frame.
    ///
    /// Saved and restored around each `step` exactly like `IN_FLIGHT_TAG`,
    /// for the same reason.
    static CAUGHT_UNWIND: RefCell<Option<Box<dyn core::any::Any + Send>>> = const { RefCell::new(None) };
}

/// What [`IN_FLIGHT_TAG`]/[`CAUGHT_UNWIND`] hold for one task, moved off the
/// live thread-locals between that task's steps — `docs/dev/os-threads-design.md`
/// §3.
///
/// The throw's *value* is deliberately not here: it needs a GC root, and each
/// task's own `RootStack` already gives it one that survives exactly as long
/// as this does (`Heap::in_flight_throw`/`set_in_flight_throw`). Keeping the
/// two halves in separate places is safe only because both are switched in
/// lockstep — whoever installs a `ParkedUnwind` before running a task has
/// also made that task's own root stack the current one first.
#[derive(Default)]
pub struct ParkedUnwind {
    tag: Option<typelisp_mem::SymRef>,
    caught: Option<Box<dyn core::any::Any + Send>>,
}

impl ParkedUnwind {
    /// A `ParkedUnwind` for a `panic` that arrived with no throw of its
    /// own — what [`sched::CompiledTask::deliver`](crate::sched::CompiledTask)
    /// parks for its own next `step` to install, instead of writing straight
    /// to the live thread-locals the way [`park_activation_unwind`] does (see
    /// that function's caller for why: `deliver` can run while a *different*
    /// task's state is the one installed on this thread).
    pub fn panic(message: String) -> ParkedUnwind {
        ParkedUnwind { tag: None, caught: Some(Box::new(CompiledPanic { message })) }
    }
}

/// Takes this thread's in-flight-unwind state, leaving both thread-locals
/// empty — half of the pair a task's `step` calls around itself, the other
/// half being [`restore_parked_unwind`]. Whether called at a `step`'s start
/// (to make room for this task's own saved state) or its end (to save what
/// it leaves behind), the shape is the same: nothing is left on the thread
/// for a task that is not the one about to run, or that just stopped
/// running, to see.
pub fn take_parked_unwind() -> ParkedUnwind {
    ParkedUnwind { tag: IN_FLIGHT_TAG.with(|c| c.borrow_mut().take()), caught: CAUGHT_UNWIND.with(|c| c.borrow_mut().take()) }
}

/// Installs a previously [`take_parked_unwind`]'s state onto this thread —
/// what a task's `step` does before running, so code that reads
/// `IN_FLIGHT_TAG`/`CAUGHT_UNWIND` (a `catch`'s dispatch, an
/// `unwind-protect`'s resume) finds what *this* task itself last left there,
/// not another task's leftovers or another task's clean slate.
pub fn restore_parked_unwind(p: ParkedUnwind) {
    IN_FLIGHT_TAG.with(|c| *c.borrow_mut() = p.tag);
    CAUGHT_UNWIND.with(|c| *c.borrow_mut() = p.caught);
}

/// Parks a throw's tag and value for the unwind about to be raised for it,
/// rooting the value for the flight.
///
/// `pub` because the `typelisp` crate needs it too: an interpreted callee
/// that throws produces an `EvalError::Throw`, and the boundary hook turns it
/// into this same parked pair so a *compiled* `catch` further up sees one kind
/// of in-flight throw, not two.
///
/// # Safety
///
/// A `Heap` must be registered on this thread, and it must be the heap `value`
/// belongs to.
pub unsafe fn park_throw(tag: typelisp_mem::SymRef, value: Value) {
    IN_FLIGHT_TAG.with(|cell| *cell.borrow_mut() = Some(tag));
    active_heap().set_in_flight_throw(Some(value));
}

/// Takes back what [`park_throw`] left. `None` when no throw is in flight.
///
/// Clears the tag but **leaves the value's GC root standing**: taking the pair
/// here does not necessarily end the flight. The boundary that turns a
/// compiled throw back into an `EvalError::Throw` takes it and the throw keeps
/// travelling, now through interpreted frames, which can still run
/// `unwind-protect` cleanups that allocate. The root is released where the
/// value actually stops — the `catch` that claims it, on either side
/// ([`rt_throw_take_value`], or `Op::Catch` in the interpreter).
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
pub unsafe fn take_throw() -> Option<(typelisp_mem::SymRef, Value)> {
    let tag = IN_FLIGHT_TAG.with(|cell| cell.borrow_mut().take())?;
    // `park_throw` sets the two together, and every path that clears the
    // value clears the tag with it.
    let value = active_heap()
        .in_flight_throw()
        .unwrap_or_else(|| fatal(&format!("a throw to '{} is in flight without its value", tag.spelling())));
    Some((tag, value))
}

/// Ends the flight: no throw is travelling any more, so the value it carried
/// no longer needs a root of its own.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
pub unsafe fn clear_throw() {
    IN_FLIGHT_TAG.with(|cell| *cell.borrow_mut() = None);
    active_heap().set_in_flight_throw(None);
}

/// Parks an exit raised in *interpreted* code so a standing compiled chain
/// can catch it — the same two channels an unwinding call uses, minus the
/// unwinding.
///
/// Since C5 an interpreted callee reached from compiled code runs on the
/// task's continuation stack rather than on a machine frame inside the
/// compiled activation, so its non-local exit has no Rust frames to travel
/// through. It is handed to the chain as a status instead
/// (`coroutine::FrameStack::raise`), and this is what the chain's `catch`
/// then finds: the tag and value where `rt_throw_matches` looks for them, and
/// the payload kind where the driver's own catch leaves it.
///
/// # Safety
///
/// A `Heap` must be registered on this thread, and it must be the heap
/// `value` belongs to.
pub unsafe fn park_throw_for_chain(tag: typelisp_mem::SymRef, value: Value) {
    park_throw(tag, value);
    CAUGHT_UNWIND.with(|cell| *cell.borrow_mut() = Some(Box::new(CompiledThrow)));
}

/// [`park_throw_for_chain`] for an exit that is not a throw — a `panic`, a
/// runtime error, a `break` that escaped. The `EvalError` itself waits on the
/// front end's side (`crossing::park_interpreted_error`); this is the marker
/// that says which side to ask.
///
/// Clears any throw still parked, for `park_activation_unwind`'s reason:
/// anything that is not a throw *replaces* what was in flight, and leaving
/// the old tag parked would let a `catch` claim this as its own.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
pub unsafe fn park_error_for_chain() {
    clear_throw();
    CAUGHT_UNWIND.with(|cell| *cell.borrow_mut() = Some(Box::new(InterpretedUnwind)));
}

/// Raises the unwind for a throw already parked by [`park_throw`].
///
/// The counterpart of [`typelisp_abi::unwind_interpreted_error`] for the
/// throw channel; the `typelisp` crate calls it after parking an interpreted
/// `EvalError::Throw`.
pub fn unwind_throw() -> typelisp_abi::Raised {
    typelisp_abi::raise_payload(Box::new(CompiledThrow))
}

/// `(throw 'tag value)` for compiled code (`compiler.rs`'s `compile-throw`):
/// parks the tag and value and unwinds, so the nearest dynamically enclosing
/// `catch` on that tag — compiled or interpreted, in this function or twenty
/// frames up — produces the value in its own place.
///
/// # Safety
///
/// `tag` must decode to a `Value::Symbol` and
/// `value` to the thrown value already in its tagged form (the island
/// converts by the node's baked-in representation kind before calling). A
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_throw(tag: i64, value: i64) -> i64 {
    let tag = match decode(tag) {
        Value::Symbol(id) => id,
        _ => fatal("rt_throw: tag is not a symbol"),
    };
    let value = decode(value);
    park_throw(tag, value);
    unwind_throw().into_word()
}

/// Whether the throw currently in flight carries tag `tag` — a `catch`'s
/// dispatch block asking "is this one mine?". Returns the compiled `bool`
/// encoding (`1`/`0`), like every `rt_*` predicate.
///
/// False when nothing is in flight at all, which is the case for a caught
/// `CompiledPanic`/`InterpretedUnwind`: those belong to no tag, so every
/// `catch` declines them and only `unwind-protect` cleanups see them pass.
///
/// A throw of this `catch`'s symbol carrying another type
/// (`throw_tag_mismatch`) is an error: the throw in flight is replaced by a
/// panic saying so, and the answer is false.
///
/// # Safety
///
/// `tag` must decode to a `Value::Symbol`; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_throw_matches(tag: i64) -> i64 {
    let tag = match decode(tag) {
        Value::Symbol(id) => id,
        _ => fatal("rt_throw_matches: tag is not a symbol"),
    };
    let thrown = IN_FLIGHT_TAG.with(|cell| *cell.borrow());
    match thrown {
        Some(t) if t == tag => 1,
        // A throw of this symbol carrying another type: it becomes a panic,
        // which this `catch` declines like any other.
        Some(t) => match typelisp_mem::symbols::throw_tag_mismatch(t, tag, &typelisp_print::shared::name_view()) {
            Some(message) => {
                clear_throw();
                CAUGHT_UNWIND.with(|cell| *cell.borrow_mut() = Some(Box::new(CompiledPanic { message })));
                0
            }
            None => 0,
        },
        None => 0,
    }
}

/// Consumes the in-flight throw, returning its value in tagged form — the
/// other half of a `catch` claiming one, called only after
/// [`rt_throw_matches`] said yes.
///
/// Dropping the parked payload here is what makes the claim final: nothing
/// is left to carry on, which is exactly right, since control has rejoined
/// the catching function's ordinary flow.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_throw_take_value() -> i64 {
    CAUGHT_UNWIND.with(|cell| *cell.borrow_mut() = None);
    match take_throw() {
        Some((_, value)) => {
            clear_throw();
            encode(value)
        }
        None => fatal("rt_throw_take_value: no throw is in flight"),
    }
}

/// Classifies an unwind a compiled activation raised (`coroutine::activation`)
/// and parks it for the driver to carry on with.
///
/// Anything that is not one of the runtime's three deliberate payloads is
/// fatal: a genuine bug in the runtime must not come back as a
/// plausible-looking typelisp condition.
///
/// A throw keeps its parked tag and value — its flight is not over, it is
/// being carried by the driver instead of by the unwinder. Anything else
/// *replaces* what was in flight: a cleanup that panics while a throw travels
/// wins, and CLHS says so. Leaving the old tag parked would let a `catch`
/// further up claim this panic as if it were its own throw.
///
/// Unlike [`protected`] this repairs nothing. The GC root stack and the
/// published-frame stack are cut back by the driver, which knows the depths
/// per frame rather than per call.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
pub(crate) unsafe fn park_activation_unwind(payload: Box<dyn core::any::Any + Send>) {
    let ours = payload.is::<CompiledThrow>() || payload.is::<CompiledPanic>() || payload.is::<InterpretedUnwind>();
    if !ours {
        fatal("an unwind was raised whose payload the runtime does not raise");
    }
    if !payload.is::<CompiledThrow>() {
        clear_throw();
    }
    CAUGHT_UNWIND.with(|cell| *cell.borrow_mut() = Some(payload));
}

/// Takes what [`park_activation_unwind`] parked, for a caller that wants the
/// payload itself rather than a re-raise — the interpreter's task driver,
/// which turns it into an `EvalError` and unwinds its own continuation stack
/// through it instead of the machine stack.
pub fn take_activation_unwind() -> Option<Box<dyn core::any::Any + Send>> {
    CAUGHT_UNWIND.with(|cell| cell.borrow_mut().take())
}

/// Raises again what [`park_activation_unwind`] parked — what a driver whose
/// caller is a machine frame does when the unwind passed its outermost frame.
///
/// It is `fatal` when nothing is carried, because reaching it with an empty
/// slot means the driver reported an unwind that never was.
pub(crate) fn resume_activation_unwind() -> typelisp_abi::Raised {
    match take_activation_unwind() {
        Some(payload) => typelisp_abi::raise_payload(payload),
        None => fatal("resume_activation_unwind: the driver reported an unwind but none is being carried"),
    }
}
