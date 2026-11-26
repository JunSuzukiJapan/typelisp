//! What the driver does with an unwind, in miniature.
//!
//! An unwind raised in compiled code is parked and the shim returns
//! (`typelisp_abi::raise`); the compiled code checks (`rt_unwinding`) and
//! leaves its activation with `STATUS_UNWIND`. It cannot get past that
//! activation, because a Lisp call is a driver round trip and not a machine
//! call — and where it goes next is the driver's frame bookkeeping.
//!
//! The bodies here are written in Rust rather than emitted as IR, because
//! what is being pinned down is the *protocol*: which frame the driver asks,
//! what it does with the answer, and what it leaves behind. Every body obeys
//! the coroutine ABI by hand — make a frame on entry, publish it, dispatch on
//! `pc` — so a failure names the driver rather than the island that emits the
//! same shape.
//!
//! What it pins down:
//!
//! 1. A body that returns hands its value out, and leaves the chain and the
//!    GC root stack as it found them.
//! 2. An unwind that no frame declares a handler for empties the chain,
//!    restores the root stack, and leaves the throw itself intact for the
//!    boundary above.
//! 3. A frame whose handler slot names a pad is re-entered *there*, two frames
//!    down from where the throw was raised, and can claim the value.
//! 4. A handler that declines keeps the throw travelling.

use typelisp_mem::Word;
use typelisp_abi::{call_state, encode, set_active_heap, FRAME_HANDLER_SLOT, FRAME_PC_SLOT, FRAME_RESERVED_SLOTS, FRAME_VALUE_SLOT};
use typelisp_abi::{STATUS_CALL, STATUS_RETURN, STATUS_UNWIND};
use typelisp_mem::{Heap, Value};
use typelisp_rt::coroutine::{CoroutineFn, FrameStack, Paused};

/// What [`plain`] answers with.
const NO_THROW_RESULT: Word = 999;

/// The value [`raiser`] throws.
const THROWN: i64 = 7;

/// [`guarded`]'s resume ids: `1` is its pad, `2` is after the call it makes.
const PAD_PC: Word = 1;
const AFTER_CALL_PC: Word = 2;

/// The prologue every compiled body emits: on first entry open a frame, which
/// `rt_frame_open` also tells the driver about (it is made *inside* the
/// callee, so there is no other moment); on a later entry the driver hands
/// back the address of the same frame's words. Answers with that address and
/// where to resume.
unsafe fn prologue(param: Word, nslots: usize) -> (*mut Word, Word) {
    if param != 0 {
        let data = param as usize as *mut Word;
        return (data, *data.add(FRAME_PC_SLOT));
    }
    let open = [nslots as Word, 0];
    let data = typelisp_rt::rt_frame_open(open[0], open[1]) as usize as *mut Word;
    (data, 0)
}

/// Writes one slot of a frame, the way `slot-set` does.
unsafe fn set_slot(data: *mut Word, idx: usize, word: Word) {
    *data.add(idx) = word;
}

unsafe fn slot(data: *mut Word, idx: usize) -> Word {
    *data.add(idx)
}

/// A body that just answers, with only the slots the protocol owns.
unsafe extern "C-unwind" fn plain(param: Word) -> Word {
    let (frame, _) = prologue(param, FRAME_RESERVED_SLOTS);
    set_slot(frame, FRAME_VALUE_SLOT, NO_THROW_RESULT);
    STATUS_RETURN
}

/// A body that throws the tag it was passed. Declares no handler, so the
/// driver has nothing to ask it.
unsafe extern "C-unwind" fn raiser(param: Word) -> Word {
    let (_frame, _) = prologue(param, FRAME_RESERVED_SLOTS);
    let tag = call_state::with(|s| s.pending_arg(0)).expect("raiser was passed a tag");
    let args = [tag, encode(Value::Int(THROWN))];
    typelisp_rt::rt_throw(args[0], args[1]);
    // What generated code does after a call that raised.
    assert_ne!(typelisp_rt::rt_unwinding(), 0, "rt_throw raised nothing");
    STATUS_UNWIND
}

/// A body with a region: it calls [`raiser`] with its handler slot naming its
/// own pad, and the pad decides whether the throw in flight is its.
///
/// The first slot past the protocol's holds the tag the pad claims. It has to
/// be a frame slot rather than a local: the pad runs in a *later activation*
/// than the code that read the argument.
unsafe extern "C-unwind" fn guarded(param: Word) -> Word {
    let (frame, pc) = prologue(param, FRAME_RESERVED_SLOTS + 1);
    if pc == 0 {
        let caught = call_state::with(|s| s.pending_arg(0)).expect("guarded was passed a tag to claim");
        let thrown = call_state::with(|s| s.pending_arg(1)).expect("guarded was passed a tag to throw");
        set_slot(frame, FRAME_RESERVED_SLOTS, caught);
        // From here an unwind reaching this frame lands at `PAD_PC`.
        set_slot(frame, FRAME_HANDLER_SLOT, PAD_PC);
        let call = [raiser as *const () as usize as Word, thrown];
        typelisp_rt::rt_frame_call(call.as_ptr(), 2);
        set_slot(frame, FRAME_PC_SLOT, AFTER_CALL_PC);
        return STATUS_CALL;
    }
    if pc == PAD_PC {
        // The region is over the moment its pad runs, either way it goes.
        set_slot(frame, FRAME_HANDLER_SLOT, 0);
        if typelisp_rt::rt_throw_matches(slot(frame, FRAME_RESERVED_SLOTS)) != 0 {
            let claimed = typelisp_rt::rt_throw_take_value();
            set_slot(frame, FRAME_VALUE_SLOT, claimed);
            return STATUS_RETURN;
        }
        // Not ours. The driver goes on looking below; it cuts this frame's
        // roots back itself, closing it.
        return STATUS_UNWIND;
    }
    panic!("guarded was resumed at {}, which only a returning callee reaches and none does", pc)
}

/// A heap registered as the active one, the way any compiled call needs.
fn heap() -> Box<Heap> {
    let mut heap = Box::new(Heap::with_capacity(1 << 12));
    set_active_heap(heap.as_mut() as *mut Heap);
    heap
}

/// Runs `f` with `args`, with the panic hook silenced — a throw in flight
/// prints nothing useful and every one of these tests raises at least one.
fn drive(heap: &mut Heap, f: CoroutineFn, args: &[Word]) -> Result<i64, Paused> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut stack = FrameStack::new();
    let outcome = stack.run(heap, f, args);
    std::panic::set_hook(previous);
    assert_eq!(stack.depth(), 0, "the driver left frames standing");
    outcome
}

#[test]
fn a_body_that_returns_hands_its_value_out() {
    let mut heap = heap();
    let before = heap.root_count();
    assert_eq!(drive(&mut heap, plain, &[]), Ok(NO_THROW_RESULT as i64));
    assert_eq!(heap.root_count(), before, "the body's own frame root outlived it");
}

#[test]
fn an_unwind_with_no_handler_empties_the_chain() {
    let mut heap = heap();
    let before = heap.root_count();
    let tag = encode(heap.intern_symbol("boom"));
    assert_eq!(drive(&mut heap, raiser, &[tag]), Err(Paused::Unwinding));
    assert_eq!(heap.root_count(), before, "the unwound frame's roots were left behind");
    // Still travelling: the boundary above is what turns it into an error.
    let (parked_tag, parked_value) = unsafe { typelisp_rt::take_throw() }.expect("the throw is still in flight");
    assert_eq!(Value::Symbol(parked_tag), heap.intern_symbol("boom"));
    assert_eq!(parked_value, Value::Int(THROWN));
    unsafe { typelisp_rt::clear_throw() };
}

#[test]
fn a_handler_two_frames_down_claims_the_throw() {
    let mut heap = heap();
    let before = heap.root_count();
    let tag = encode(heap.intern_symbol("boom"));
    assert_eq!(drive(&mut heap, guarded, &[tag, tag]), Ok(encode(Value::Int(THROWN)) as i64));
    assert_eq!(heap.root_count(), before);
}

#[test]
fn a_handler_that_declines_keeps_the_throw_travelling() {
    let mut heap = heap();
    let before = heap.root_count();
    let caught = encode(heap.intern_symbol("inner"));
    let thrown = encode(heap.intern_symbol("outer"));
    assert_eq!(drive(&mut heap, guarded, &[caught, thrown]), Err(Paused::Unwinding));
    assert_eq!(heap.root_count(), before);
    let (parked_tag, parked_value) = unsafe { typelisp_rt::take_throw() }.expect("the throw is still in flight");
    assert_eq!(Value::Symbol(parked_tag), heap.intern_symbol("outer"));
    assert_eq!(parked_value, Value::Int(THROWN));
    unsafe { typelisp_rt::clear_throw() };
}

#[test]
fn claiming_repeats() {
    let mut heap = heap();
    let tag = encode(heap.intern_symbol("boom"));
    for _ in 0..3 {
        assert_eq!(drive(&mut heap, guarded, &[tag, tag]), Ok(encode(Value::Int(THROWN)) as i64));
    }
}
