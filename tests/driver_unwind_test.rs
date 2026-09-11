//! What the driver does with an unwind, in miniature.
//!
//! `tests/compiled_unwind_test.rs` established that a Rust panic can travel
//! *through* JIT frames. Stopping one is the other half, and it cannot be done
//! with an LLVM landing pad: consuming a Rust panic means freeing its
//! `_Unwind_Exception` and decrementing `panic_count`, and only
//! `std::panic::catch_unwind` does either (the object's own `exception_cleanup`
//! aborts with "Rust panics must be rethrown", and `panic_count::decrease` is
//! called from nowhere else). A landing pad that swallowed one would leak the
//! allocation and leave `std::thread::panicking()` true for the rest of the
//! process.
//!
//! Until C4 the catching Rust frame went at every *call* made inside a
//! protected region (nine `rt_protected_*` trampolines, and this file used to
//! build that shape). It goes in one place now: the driver, at the boundary
//! where it enters a compiled activation. It can, because a Lisp call is a
//! driver round trip and not a machine call, so an unwind raised in compiled
//! code cannot get past the activation that raised it — and where it goes
//! next is frame bookkeeping, not unwinder business.
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

use typelisp_abi::{call_state, encode, set_active_heap, FRAME_HANDLER_SLOT, FRAME_VALUE_SLOT};
use typelisp_abi::{STATUS_CALL, STATUS_RETURN, STATUS_UNWIND};
use typelisp_mem::{Heap, Value};
use typelisp_rt::coroutine::{CoroutineFn, FrameStack, Paused};

/// What [`plain`] answers with.
const NO_THROW_RESULT: i64 = 999;

/// The value [`raiser`] throws.
const THROWN: i64 = 7;

/// [`guarded`]'s resume ids: `1` is its pad, `2` is after the call it makes.
const PAD_PC: i64 = 1;
const AFTER_CALL_PC: i64 = 2;

/// One-slot argument arrays, since every `rt_*` entry point takes
/// `(*const i64, u32)`.
unsafe fn rt1(f: unsafe extern "C" fn(*const i64, u32) -> i64, a: i64) -> i64 {
    f([a].as_ptr(), 1)
}

/// The prologue every compiled body emits: on first entry make a frame, tell
/// the driver about it (it is made *inside* the callee, so there is no other
/// moment), and root it; on a later entry the driver hands the same frame
/// back. Answers with the frame and where to resume.
unsafe fn prologue(param: i64, nslots: i64) -> (i64, i64) {
    if param != 0 {
        return (param, rt1(typelisp_rt::rt_frame_pc, param));
    }
    let frame = rt1(typelisp_rt::rt_frame_new, nslots);
    rt1(typelisp_rt::rt_frame_entered, frame);
    rt1(typelisp_rt::rt_push_sexpr_root, frame);
    (frame, 0)
}

/// Writes one slot of a frame, the way `store-arg` on a slot pointer does.
unsafe fn set_slot(frame: i64, idx: usize, word: i64) {
    let data = rt1(typelisp_rt::rt_frame_data, frame) as usize as *mut i64;
    *data.add(idx) = word;
}

unsafe fn slot(frame: i64, idx: usize) -> i64 {
    let data = rt1(typelisp_rt::rt_frame_data, frame) as usize as *mut i64;
    *data.add(idx)
}

/// A body that just answers. Two slots, because the protocol owns both.
unsafe extern "C-unwind" fn plain(param: i64) -> i64 {
    let (frame, _) = prologue(param, 2);
    set_slot(frame, FRAME_VALUE_SLOT, NO_THROW_RESULT);
    typelisp_rt::rt_pop_sexpr_root(std::ptr::null(), 0);
    STATUS_RETURN
}

/// A body that throws the tag it was passed. Declares no handler, so the
/// driver has nothing to ask it.
unsafe extern "C-unwind" fn raiser(param: i64) -> i64 {
    let (_frame, _) = prologue(param, 2);
    let tag = call_state::pending_arg(0).expect("raiser was passed a tag");
    let args = [tag, encode(Value::Int(THROWN))];
    typelisp_rt::rt_throw(args.as_ptr(), 2);
    unreachable!("rt_throw does not return")
}

/// A body with a region: it calls [`raiser`] with its handler slot naming its
/// own pad, and the pad decides whether the throw in flight is its.
///
/// Slot 2 holds the tag the pad claims. It has to be a frame slot rather than
/// a local: the pad runs in a *later activation* than the code that read the
/// argument.
unsafe extern "C-unwind" fn guarded(param: i64) -> i64 {
    let (frame, pc) = prologue(param, 3);
    if pc == 0 {
        let caught = call_state::pending_arg(0).expect("guarded was passed a tag to claim");
        let thrown = call_state::pending_arg(1).expect("guarded was passed a tag to throw");
        set_slot(frame, 2, caught);
        // From here an unwind reaching this frame lands at `PAD_PC`.
        set_slot(frame, FRAME_HANDLER_SLOT, PAD_PC);
        let call = [raiser as usize as i64, thrown];
        typelisp_rt::rt_frame_call(call.as_ptr(), 2);
        let set_pc = [frame, AFTER_CALL_PC];
        typelisp_rt::rt_frame_set_pc(set_pc.as_ptr(), 2);
        return STATUS_CALL;
    }
    if pc == PAD_PC {
        // The region is over the moment its pad runs, either way it goes.
        set_slot(frame, FRAME_HANDLER_SLOT, 0);
        if rt1(typelisp_rt::rt_throw_matches, slot(frame, 2)) != 0 {
            let claimed = typelisp_rt::rt_throw_take_value(std::ptr::null(), 0);
            set_slot(frame, FRAME_VALUE_SLOT, claimed);
            typelisp_rt::rt_pop_sexpr_root(std::ptr::null(), 0);
            return STATUS_RETURN;
        }
        // Not ours. The driver goes on looking below; it cuts this frame's
        // roots back itself, so there is no pop here.
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
fn drive(heap: &mut Heap, f: CoroutineFn, args: &[i64]) -> Result<i64, Paused> {
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
    assert_eq!(drive(&mut heap, plain, &[]), Ok(NO_THROW_RESULT));
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
    assert_eq!(parked_tag, "boom");
    assert_eq!(parked_value, Value::Int(THROWN));
    unsafe { typelisp_rt::clear_throw() };
}

#[test]
fn a_handler_two_frames_down_claims_the_throw() {
    let mut heap = heap();
    let before = heap.root_count();
    let tag = encode(heap.intern_symbol("boom"));
    assert_eq!(drive(&mut heap, guarded, &[tag, tag]), Ok(encode(Value::Int(THROWN))));
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
    assert_eq!(parked_tag, "outer");
    assert_eq!(parked_value, Value::Int(THROWN));
    unsafe { typelisp_rt::clear_throw() };
}

#[test]
fn claiming_repeats() {
    let mut heap = heap();
    let tag = encode(heap.intern_symbol("boom"));
    for _ in 0..3 {
        assert_eq!(drive(&mut heap, guarded, &[tag, tag]), Ok(encode(Value::Int(THROWN))));
    }
}
