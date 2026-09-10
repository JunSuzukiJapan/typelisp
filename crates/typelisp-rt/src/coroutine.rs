//! The driver that runs coroutine-ABI compiled code (Phase C2).
//!
//! # What changes, and why it has to
//!
//! Under the old ABI a compiled function was `i64 f(const i64 *args, u32
//! argc)`: it ran to completion and returned its value. There was no way for
//! it to stop in the middle, because the rest of its work lived on the machine
//! stack and nothing could pick that up again afterwards. That is the whole of
//! why `sleep`/`yield`/`wait` could not be compiled — not a missing feature,
//! a missing *shape*.
//!
//! The coroutine ABI is `i64 f(i64 frame)`, and what comes back is a **status
//! word** (`STATUS_RETURN` and friends) saying why the function stopped. Its
//! value, when it has one, is in the frame — Phase C1 already moved every live
//! local there, so a frame is now the entire state of an activation. Put those
//! two together and "stop here, come back later" becomes sayable: the frame is
//! the work, the status is why it paused.
//!
//! # The stack is data
//!
//! [`FrameStack`] is a `Vec` of frames, and a call is a push. That is the
//! second thing this buys, after suspension: a Lisp recursion no longer costs
//! Rust frames, exactly as Phase A did for the interpreter. The depth limit
//! becomes the heap rather than the OS stack.
//!
//! # Where the arguments go
//!
//! A callee's frame is allocated by the callee, not the caller — only the
//! callee knows how many slots it needs, and with mutual recursion the caller
//! may be compiled before the callee exists. So the first entry to a function
//! passes `0` in place of a frame, meaning "make your own", and the arguments
//! wait in `typelisp_abi::call_state` for its prologue to copy in.
//!
//! That is one thread-local rather than an argument because the ABI has
//! exactly one parameter, and it must, so that resuming and entering are the
//! same call. A second parameter would make them two shapes, and every
//! indirect call — a closure, a `:dyn` method — would have to know which it
//! was making.

use typelisp_abi::call_state;
use typelisp_abi::{decode, STATUS_CALL, STATUS_RETURN, STATUS_SUSPEND};
use typelisp_mem::{BoxId, Heap, Value};

// The value slot holds a **raw word**, and the driver never looks inside it.
// Whether those bits are a tagged reference or a native `i32`/`f64` is decided
// by the function's declared return representation, which lives in the type
// system and nowhere else — the same rule every other crossing obeys. So this
// hands the word straight through and lets the caller, who knows the type,
// decode it (`decode_compiled_return`).

/// A coroutine-ABI compiled function: takes a frame (or `0` on first entry),
/// returns a status word.
pub type CoroutineFn = unsafe extern "C" fn(i64) -> i64;

/// Why a drive stopped short of an answer.
#[derive(Debug, PartialEq, Eq)]
pub enum Paused {
    /// A frame suspended. It is still on the stack, and driving again resumes
    /// it — this is what a scheduler switches away on.
    Suspended,
    /// A frame is unwinding. Phase C4 gives this a payload; for now it only
    /// says that no compiled function produces one yet.
    Unwinding,
}

/// The frames of one task's compiled call chain, innermost last.
///
/// Holding them here rather than on the machine stack is what lets a task be
/// put down mid-call: everything the chain needs is a `Vec` of heap objects,
/// and the collector already traces each of them.
pub struct FrameStack {
    frames: Vec<(CoroutineFn, Value)>,
}

impl Default for FrameStack {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameStack {
    pub fn new() -> Self {
        FrameStack { frames: Vec::new() }
    }

    /// How deep the chain is. Zero means finished or not yet started.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Every frame in the chain, for the collector to trace.
    ///
    /// A frame reached only from here is reachable from nowhere else — the
    /// caller's frame does not point at the callee's — so this is the whole of
    /// what keeps a suspended chain alive.
    pub fn roots(&self) -> impl Iterator<Item = Value> + '_ {
        self.frames.iter().map(|(_, f)| *f)
    }

    /// Begin a call: the function is entered with no frame, so its prologue
    /// makes one and copies `args` out of `typelisp_abi::call_state`.
    fn enter(&mut self, f: CoroutineFn, args: &[i64], env: &[i64]) -> i64 {
        call_state::set_pending_args(args);
        call_state::set_pending_env(env);
        // Nothing may allocate between here and the prologue's copy — see
        // `call_state`'s doc comment.
        unsafe { f(0) }
    }

    /// Run `f(args)` to an answer, or to the point where it stops being this
    /// driver's business.
    ///
    /// The loop is the whole protocol. A `RETURN` pops and hands its value to
    /// the frame below (or out, if there is none); a `CALL` pushes; a
    /// `SUSPEND` or `UNWIND` leaves the chain standing and gets out of the
    /// way, because who runs next is a scheduling question, not a calling one.
    pub fn run(&mut self, heap: &mut Heap, f: CoroutineFn, args: &[i64]) -> Result<i64, Paused> {
        self.run_with_env(heap, f, args, &[])
    }

    /// [`run`](Self::run) for a callee that also has captures — a closure
    /// body, entered from outside the compiled world.
    pub fn run_with_env(
        &mut self,
        heap: &mut Heap,
        f: CoroutineFn,
        args: &[i64],
        env: &[i64],
    ) -> Result<i64, Paused> {
        let base = self.frames.len();
        let mut status = self.enter(f, args, env);
        // The frame the entered function made for itself, so the driver can
        // reach it on the way back.
        let mut current = take_current_frame();
        self.frames.push((f, current));

        loop {
            match status & 0b11 {
                STATUS_RETURN => {
                    let (_, done) = self.frames.pop().expect("a returning frame is on the stack");
                    let value = frame_value(heap, done);
                    if self.frames.len() == base {
                        return Ok(value);
                    }
                    // Hand the answer to the waiting frame and resume it.
                    let (caller_fn, caller_frame) = *self.frames.last().expect("a waiting frame");
                    set_frame_value(heap, caller_frame, value);
                    status = unsafe { caller_fn(typelisp_abi::encode(caller_frame)) };
                }
                STATUS_CALL => {
                    let (callee, callee_args, callee_env) = take_pending_call();
                    status = self.enter(callee, &callee_args, &callee_env);
                    current = take_current_frame();
                    self.frames.push((callee, current));
                }
                STATUS_SUSPEND => return Err(Paused::Suspended),
                _ => return Err(Paused::Unwinding),
            }
        }
    }
}

/// The frame a just-entered function allocated for itself, as the prologue
/// published it through `rt_frame_entered`.
fn take_current_frame() -> Value {
    let word = call_state::take_current_frame()
        .unwrap_or_else(|| panic!("a coroutine function did not publish its frame on entry"));
    decode(word)
}

fn take_pending_call() -> (CoroutineFn, Vec<i64>, Vec<i64>) {
    let (target, args, env) = call_state::take_pending_call()
        .unwrap_or_else(|| panic!("a frame returned STATUS_CALL without naming a callee"));
    // SAFETY: the address came from `build-fn-address` over a function this
    // module declared under the coroutine ABI, the same provenance every
    // indirect compiled call already relies on.
    let f: CoroutineFn = unsafe { std::mem::transmute::<usize, CoroutineFn>(target) };
    (f, args, env)
}

fn frame_id(f: Value) -> BoxId {
    match f {
        Value::Boxed(id) => id,
        other => panic!("expected a frame, got {:?}", other),
    }
}

/// Reads the value slot the protocol reserves, as the raw word it holds.
pub fn frame_value(heap: &Heap, f: Value) -> i64 {
    heap.frame_word(frame_id(f), typelisp_abi::FRAME_VALUE_SLOT)
}

/// Writes the value slot the protocol reserves.
pub fn set_frame_value(heap: &mut Heap, f: Value, w: i64) {
    heap.set_frame_word(frame_id(f), typelisp_abi::FRAME_VALUE_SLOT, w);
}
