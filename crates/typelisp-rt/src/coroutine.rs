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
use typelisp_abi::call_state::Pending;
use typelisp_abi::{decode, STATUS_CALL, STATUS_RETURN, STATUS_SUSPEND, STATUS_UNWIND};
use typelisp_mem::{BoxId, Heap, Value};

// The value slot holds a **raw word**, and the driver never looks inside it.
// Whether those bits are a tagged reference or a native `i32`/`f64` is decided
// by the function's declared return representation, which lives in the type
// system and nowhere else — the same rule every other crossing obeys. So this
// hands the word straight through and lets the caller, who knows the type,
// decode it (`decode_compiled_return`).

/// A coroutine-ABI compiled function: takes a frame (or `0` on first entry),
/// returns a status word.
///
/// `"C-unwind"`, like every other compiled-body pointer type in the tree
/// (`CompiledSignature`, `ApplyInterpretedFn`, `DynSlotClosureFn`): a body
/// can reach `rt_panic`/`rt_throw`, which unwind, and a `"C"` pointer
/// promises the caller they cannot. This said `"C"` from C2c until C4 and
/// happened to work — the call site LLVM emits for a `nounwind` callee still
/// has a walkable frame — but it was a promise the callee did not keep, and
/// `catch_unwind` at the driver's boundary is only meaningful if the unwind
/// is allowed to get there.
pub type CoroutineFn = unsafe extern "C-unwind" fn(i64) -> i64;

/// Why a drive stopped short of an answer.
#[derive(Debug, PartialEq, Eq)]
pub enum Paused {
    /// A frame suspended. It is still on the stack, and driving again resumes
    /// it — this is what a scheduler switches away on.
    Suspended,
    /// A frame applied a function value that turned out to be
    /// **interpreted**, so only whoever owns a continuation stack can run it.
    ///
    /// The chain stays standing, exactly as for a suspension: the answer goes
    /// into the top frame's value slot ([`FrameStack::set_top_value`]) and
    /// [`FrameStack::resume`] picks the chain back up. A driver whose caller
    /// is a machine frame resolves it on that frame instead
    /// ([`FrameStack::run_to_end`]), which is the one thing it can do — and
    /// the reason such a call still cannot suspend.
    Applying { closure: i64, args: Vec<i64> },
    /// An unwind passed the outermost frame this driver owned without
    /// finding a handler. The chain is gone — every frame of it has been
    /// popped and its roots cut back — and what is unwinding is parked for
    /// whoever drove to carry on with ([`resume_unwinding`]).
    Unwinding,
}

/// The frames of one task's compiled call chain, innermost last.
///
/// Holding them here rather than on the machine stack is what lets a task be
/// put down mid-call: everything the chain needs is a `Vec` of heap objects,
/// and the collector already traces each of them.
pub struct FrameStack {
    frames: Vec<Entry>,
}

/// One frame of the chain, and what the driver has to know about it that the
/// frame itself does not say.
struct Entry {
    /// The body to re-enter to run this frame on.
    body: CoroutineFn,
    /// The frame object.
    frame: Value,
    /// How deep the GC root stack was when this frame was entered.
    ///
    /// Read only on the unwinding path. A frame pushes roots as it runs — its
    /// own frame object in the prologue, temporaries around calls — and an
    /// unwind skips every matching pop, so popping the frame means cutting
    /// the root stack back to here. On the ordinary path there is nothing to
    /// repair: the single `return` pops what the prologue pushed.
    roots: usize,
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
    /// caller's frame does not point at the callee's.
    ///
    /// **Not what keeps a suspended chain alive today.** Each frame is rooted
    /// by the prologue that made it (`coroutine-begin`'s
    /// `rt_push_sexpr_root`), on the root stack of the task that owns the
    /// chain, and `Heap::gc` marks every root stack rather than only the
    /// running one — so a chain put down mid-call stays alive with nothing
    /// extra registered. This exists for a driver that has no task behind it
    /// to ask, which is what C5's boundaries will need.
    pub fn roots(&self) -> impl Iterator<Item = Value> + '_ {
        self.frames.iter().map(|e| e.frame)
    }

    /// Begin a call: the function is entered with no frame, so its prologue
    /// makes one and copies `args` out of `typelisp_abi::call_state`, and
    /// publishes it through `rt_frame_entered` — the only moment the driver
    /// can learn of a frame that is made inside the callee.
    ///
    /// Pushes the frame it published, including when the entry ends in an
    /// unwind: a function that raises inside its own `catch` published its
    /// frame first, and that frame is the one that catches. Only an entry
    /// that unwound *before* publishing leaves nothing to record.
    fn begin(&mut self, heap: &mut Heap, f: CoroutineFn, args: &[i64], env: &[i64]) -> i64 {
        let roots = heap.root_count();
        let published = call_state::current_frame_depth();
        call_state::set_pending_args(args);
        call_state::set_pending_env(env);
        // Nothing may allocate between here and the prologue's copy — see
        // `call_state`'s doc comment.
        let status = unsafe { activation(f, 0) };
        if call_state::current_frame_depth() > published {
            self.frames.push(Entry { body: f, frame: take_current_frame(), roots });
        } else if status & 0b11 != STATUS_UNWIND {
            panic!("a coroutine function did not publish its frame on entry");
        }
        // An unwind travels between the publish and the take, so anything a
        // nested entry left above this point is owned by nobody.
        call_state::truncate_current_frames(published);
        status
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

    /// Hands a value to the innermost frame, where a returning callee would
    /// have left it.
    ///
    /// What a suspension is waiting for arrives this way: the frame reads its
    /// value slot on resuming, and it cannot tell — and must not be able to
    /// tell — whether the word came from a callee or from a scheduler.
    pub fn set_top_value(&self, heap: &mut Heap, word: i64) {
        let frame = match self.frames.last() {
            Some(top) => top.frame,
            None => panic!("set_top_value: there is no suspended chain to deliver to"),
        };
        set_frame_value(heap, frame, word);
    }

    /// Hands `value` to the frame waiting for it and runs it again — what a
    /// returning callee's answer does, for a callee that had no frame of its
    /// own to pop.
    fn hand_back(&mut self, heap: &mut Heap, value: i64) -> i64 {
        let (body, frame) = {
            let caller = self.frames.last().expect("a frame is waiting for the answer");
            (caller.body, caller.frame)
        };
        set_frame_value(heap, frame, value);
        unsafe { activation(body, typelisp_abi::encode(frame)) }
    }

    /// Raises the unwind already parked into this chain: the innermost frame
    /// is asked for a handler, and the walk goes on from there.
    ///
    /// What an *interpreted* callee's non-local exit does when the call came
    /// from compiled code. Until C5 that travelled as a Rust panic through
    /// the machine frames in between; there are none left to travel through,
    /// so the exit is handed to the chain as the status it would have become
    /// anyway.
    pub fn raise(&mut self, heap: &mut Heap, base: usize) -> Result<i64, Paused> {
        self.drive(heap, base, STATUS_UNWIND)
    }

    /// [`Self::run_with_env`] for a caller that is a machine frame: an
    /// interpreted `apply` underneath is run right here, on that frame.
    ///
    /// The one thing a driver with no continuation stack behind it can do
    /// with [`Paused::Applying`]. It keeps the boundaries that still work
    /// this way (`rt_drive_body`, `rt_drive_entry`, the C FFI thunk) behaving as
    /// they did, at the cost the plan's B6 names: a call made through one of
    /// them cannot suspend.
    pub fn run_to_end(
        &mut self,
        heap: &mut Heap,
        f: CoroutineFn,
        args: &[i64],
        env: &[i64],
        apply: impl Fn(i64, &[i64]) -> i64,
    ) -> Result<i64, Paused> {
        let base = self.frames.len();
        let mut outcome = self.run_with_env(heap, f, args, env);
        loop {
            match outcome {
                Err(Paused::Applying { closure, args }) => {
                    let v = apply(closure, &args);
                    self.set_top_value(heap, v);
                    outcome = self.resume(heap, base);
                }
                // A loop's back edge offered a turn (C7). This driver has
                // nowhere to put the chain down, but it does not have to:
                // nothing is being waited for, so declining the offer and
                // resuming is the whole of honouring it. The value slot is
                // set because a resume block reads it like any other, and
                // the island discards this one.
                Err(Paused::Suspended) => match call_state::take_pending_suspend() {
                    Some((call_state::SUSPEND_SAFEPOINT, _, _)) => {
                        self.set_top_value(heap, 0);
                        outcome = self.resume(heap, base);
                    }
                    // A real wait, which this driver cannot honour. Put it
                    // back so the caller's error can still say what it was.
                    other => {
                        if let Some((kind, first, second)) = other {
                            call_state::set_pending_suspend_2(kind, first, second);
                        }
                        return Err(Paused::Suspended);
                    }
                },
                other => return other,
            }
        }
    }

    /// Pick a suspended chain back up: re-enter its innermost frame.
    ///
    /// The counterpart of [`Self::run`] for a stack that is already standing.
    /// The frame says where to resume (its `pc`), so entering it is the same
    /// call as entering it the first time — which is the whole reason the ABI
    /// has exactly one parameter.
    ///
    /// `base` is how many frames below this segment belong to somebody else.
    /// It is zero for all but one case, and that case is C5's: a compiled
    /// frame applies an interpreted closure, the closure calls a compiled
    /// function, and the task now has two runs of compiled frames in one
    /// stack with interpreted frames between them. Each is resumed down to
    /// its own base.
    ///
    /// Whatever the resumed frame was waiting for must already be in its value
    /// slot ([`set_frame_value`]) — the same place a returning callee leaves
    /// its answer, because from the resuming function's side those are the
    /// same thing.
    pub fn resume(&mut self, heap: &mut Heap, base: usize) -> Result<i64, Paused> {
        let top = match self.frames.last() {
            Some(top) => (top.body, top.frame),
            None => panic!("resume: there is no suspended chain to resume"),
        };
        let status = unsafe { activation(top.0, typelisp_abi::encode(top.1)) };
        self.drive(heap, base, status)
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
        let status = self.begin(heap, f, args, env);
        self.drive(heap, base, status)
    }

    /// The protocol loop, shared by entering ([`Self::run_with_env`]) and
    /// picking a chain back up ([`Self::resume`]).
    ///
    /// A `RETURN` pops and hands its value to the frame below (or out, if
    /// there is none below `base`); a `CALL` pushes; a `SUSPEND` leaves the
    /// chain standing and gets out of the way, because who runs next is a
    /// scheduling question, not a calling one; an `UNWIND` pops frames,
    /// cutting the root stack back as it goes.
    ///
    /// `base` is how many frames belonged to somebody else when this drive
    /// began — nonzero only for a nested drive, whose caller is waiting on a
    /// machine frame.
    fn drive(&mut self, heap: &mut Heap, base: usize, mut status: i64) -> Result<i64, Paused> {
        loop {
            match status & 0b11 {
                STATUS_RETURN => {
                    let done = self.frames.pop().expect("a returning frame is on the stack");
                    let value = frame_value(heap, done.frame);
                    if self.frames.len() == base {
                        return Ok(value);
                    }
                    // Hand the answer to the waiting frame and resume it.
                    let (caller_body, caller_frame) = {
                        let caller = self.frames.last().expect("a waiting frame");
                        (caller.body, caller.frame)
                    };
                    set_frame_value(heap, caller_frame, value);
                    status = unsafe { activation(caller_body, typelisp_abi::encode(caller_frame)) };
                }
                STATUS_CALL => match take_pending_call() {
                    Pending::Direct { target, args, env } => {
                        // SAFETY: the address came from `build-fn-address`
                        // over a function declared under the coroutine ABI,
                        // the provenance every indirect compiled call relies
                        // on.
                        let f: CoroutineFn = unsafe { std::mem::transmute::<usize, CoroutineFn>(target) };
                        status = self.begin(heap, f, &args, &env);
                    }
                    // SAFETY: the word came from a compiled `apply` site,
                    // whose callee the checker typed as an `Fn`.
                    Pending::Apply { closure, args } => match unsafe { crate::resolve_closure(closure) } {
                        crate::Callee::Coroutine { f, env } => status = self.begin(heap, f, &args, &env),
                        // Runs to completion by construction, so there is no
                        // frame to push — only an answer to hand back. The
                        // catch is `activation`'s, for its reason: this is a
                        // compiled activation too.
                        crate::Callee::Classic { f, env } => {
                            let call = std::panic::AssertUnwindSafe(|| unsafe {
                                f(args.as_ptr(), args.len() as u32, env.as_ptr(), env.len() as u32)
                            });
                            status = match std::panic::catch_unwind(call) {
                                Ok(v) => self.hand_back(heap, v),
                                Err(payload) => {
                                    unsafe { crate::park_activation_unwind(payload) };
                                    STATUS_UNWIND
                                }
                            };
                        }
                        crate::Callee::Interpreted => return Err(Paused::Applying { closure, args }),
                    },
                    Pending::Dyn { vtable, slot, args } => {
                        let (ptr, abi) = crate::vtable_slot_addr(vtable as usize, slot as usize);
                        if ptr == 0 {
                            // Nothing compiled behind the slot: the concrete
                            // type's method is ordinary interpreted code. Ask
                            // the interpreter for the closure it stands for,
                            // and from here it is an ordinary apply.
                            let closure = unsafe { crate::reify_dyn_slot(vtable, slot, "a :dyn call") };
                            return Err(Paused::Applying { closure, args });
                        }
                        if abi == typelisp_abi::BODY_ABI_COROUTINE {
                            // SAFETY: a coroutine-ABI vtable entry is the
                            // address of a body declared under
                            // `coroutine_fn_type`, which is what the slot's
                            // recorded ABI says.
                            let f: CoroutineFn = unsafe { std::mem::transmute::<usize, CoroutineFn>(ptr) };
                            status = self.begin(heap, f, &args, &[]);
                        } else {
                            // SAFETY: as above, under the plain
                            // `compiled_fn_type` signature.
                            let f: crate::ClassicMethodFn = unsafe { std::mem::transmute(ptr) };
                            let call = std::panic::AssertUnwindSafe(|| unsafe {
                                f(args.as_ptr(), args.len() as u32)
                            });
                            status = match std::panic::catch_unwind(call) {
                                Ok(v) => self.hand_back(heap, v),
                                Err(payload) => {
                                    unsafe { crate::park_activation_unwind(payload) };
                                    STATUS_UNWIND
                                }
                            };
                        }
                    }
                },
                STATUS_SUSPEND => return Err(Paused::Suspended),
                _ => match self.unwind(heap, base) {
                    Some(resumed) => status = resumed,
                    None => return Err(Paused::Unwinding),
                },
            }
        }
    }
}

impl FrameStack {
    /// Walks the chain for a frame that wants the unwind in flight, and
    /// resumes it there.
    ///
    /// `Some(status)` means a frame's handler was entered and the protocol
    /// goes on from that status; `None` means the unwind passed every frame
    /// this drive owned, and the chain above `base` is gone.
    ///
    /// A frame either declares a handler in its
    /// [`FRAME_HANDLER_SLOT`](typelisp_abi::FRAME_HANDLER_SLOT) — the `pc` of
    /// the `catch`/`unwind-protect` region it is currently inside — or it does
    /// not, and then it leaves: nothing in it wants to run on the way out, so
    /// there is nothing to enter it for. Each pop cuts the GC root stack back
    /// to that frame's entry depth, which is the repair the `rt_protected_*`
    /// trampolines used to make at every protected call, made once per frame
    /// instead.
    ///
    /// The handler is **not** cleared here. What protects the frame from the
    /// handler onwards is whatever encloses that region, and only the emitted
    /// code knows what that is — so the pad installs it (the island's
    /// `install-handler`), exactly as it does on the ordinary way out.
    fn unwind(&mut self, heap: &mut Heap, base: usize) -> Option<i64> {
        loop {
            let (body, frame) = {
                let top = self.frames.last().expect("an unwinding frame is on the stack");
                (top.body, top.frame)
            };
            let handler = frame_handler(heap, frame);
            if handler != 0 {
                heap.set_frame_pc(frame_id(frame), handler as u32);
                return Some(unsafe { activation(body, typelisp_abi::encode(frame)) });
            }
            let leaving = self.frames.pop().expect("a frame above the base");
            heap.truncate_roots(leaving.roots);
            if self.frames.len() == base {
                return None;
            }
        }
    }
}

/// Where an unwind reaching this frame resumes, or `0` for nowhere.
fn frame_handler(heap: &Heap, f: Value) -> i64 {
    let id = frame_id(f);
    if heap.frame_len(id) <= typelisp_abi::FRAME_HANDLER_SLOT {
        panic!(
            "a compiled frame has {} slot(s) and so no handler slot: it was built before the slot \
             was reserved, which means an artifact (the island dump or the prelude bitcode) needs \
             regenerating",
            heap.frame_len(id)
        );
    }
    heap.frame_word(id, typelisp_abi::FRAME_HANDLER_SLOT)
}

/// Runs one compiled activation with the driver's catch around it.
///
/// This is where raising an unwind meets the driver protocol. Compiled code
/// raises the way it always did — `rt_throw`/`rt_panic` unwind, and so does a
/// runtime error or an interpreted callee reached on a machine frame —
/// but the unwind now travels no further than the one activation that raised
/// it, because Lisp calls are driver round trips and not machine calls. The
/// driver catches it here and turns it into a status, so the *travelling*
/// part of an unwind is frame bookkeeping rather than a Rust panic crossing
/// frames it knows nothing about.
///
/// A payload that is none of the runtime's three deliberate kinds keeps
/// unwinding untouched (`park_activation_unwind`): a genuine bug in the
/// runtime or in generated code must not come back as a plausible-looking
/// typelisp condition.
unsafe fn activation(f: CoroutineFn, arg: i64) -> i64 {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(arg))) {
        Ok(status) => status,
        Err(payload) => {
            crate::park_activation_unwind(payload);
            STATUS_UNWIND
        }
    }
}

/// Re-raises the unwind a drive carried out as [`Paused::Unwinding`].
///
/// Every driver whose caller is a machine frame ends up here: the unwind was
/// travelling towards a `catch` that is not in this chain, and the boundary
/// above — `catch_compiled_panic`, another `activation`, the interpreter's
/// own hook — is the one that knows where it goes next.
pub fn resume_unwinding() -> ! {
    crate::resume_activation_unwind()
}

/// The frame a just-entered function allocated for itself, as the prologue
/// published it through `rt_frame_entered`.
fn take_current_frame() -> Value {
    let word = call_state::take_current_frame()
        .unwrap_or_else(|| panic!("a coroutine function did not publish its frame on entry"));
    decode(word)
}

fn take_pending_call() -> Pending {
    call_state::take_pending_call()
        .unwrap_or_else(|| panic!("a frame returned STATUS_CALL without naming a callee"))
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

// ---- asking to be put down ------------------------------------------------
//
// A suspending builtin is not a call that blocks: it is two facts handed to
// the driver. The shim records *what* the task is waiting for, and the
// compiled code then sets its resume point and returns `STATUS_SUSPEND` —
// which is the `coroutine-call` shape with the callee left out.
//
// So these shims never block, and there is nothing here for an AOT executable
// with no scheduler to get wrong: the driver on the other side decides whether
// a suspension is something it can honour.

/// `(yield)` for compiled code: give up the rest of this task's turn.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary).
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_yield(_args: *const i64, _argc: u32) -> i64 {
    call_state::set_pending_suspend(call_state::SUSPEND_YIELD, 0);
    0
}

thread_local! {
    /// Iterations left before the next compiled loop back edge offers the
    /// driver a turn. See [`rt_loop_safepoint`].
    static SAFEPOINT_COUNTDOWN: std::cell::Cell<u32> = const { std::cell::Cell::new(SAFEPOINT_PERIOD) };
}

/// How many back-edge polls pass between offers.
///
/// **An arbitrary number, and it has to be.** What it trades is how long a
/// tight loop can hold the only thread against how often a loop that holds it
/// briefly pays a driver round trip — and this project does not measure
/// running time, so there is no measurement to pick it by. What matters for
/// the reason the safepoint exists is only that the bound is *finite*: a
/// collector that has to stop every thread needs every thread to reach a
/// point where it can be stopped.
const SAFEPOINT_PERIOD: u32 = 256;

/// The back edge of a compiled `loop` asking whether to hand control back
/// (Phase C7) — returns non-zero when it should, having already recorded
/// [`call_state::SUSPEND_SAFEPOINT`].
///
/// **A loop with no call in it is the one place a driver never sees.** Every
/// other way out of a compiled activation is a round trip: C2 made a Lisp
/// call one, C3 a suspension, C4 an unwind, C5 an `apply` and a `:dyn`
/// dispatch. A `loop` whose body calls nothing at all has none of those, so
/// between its entry and its exit the driver — and therefore the scheduler,
/// and therefore a collector that needs every thread parked — gets no turn.
///
/// The decision is made here rather than emitted inline because it is policy,
/// not code generation: the island emits the poll and branches on the answer,
/// which is the same division `rt_suspend_*` already draws between *deciding*
/// what a task waits for and *ending* the activation.
///
/// # Safety
///
/// `args`/`argc` are unused.
#[no_mangle]
pub unsafe extern "C" fn rt_loop_safepoint(_args: *const i64, _argc: u32) -> i64 {
    SAFEPOINT_COUNTDOWN.with(|c| {
        let left = c.get().saturating_sub(1);
        if left == 0 {
            c.set(SAFEPOINT_PERIOD);
            call_state::set_pending_suspend(call_state::SUSPEND_SAFEPOINT, 0);
            1
        } else {
            c.set(left);
            0
        }
    })
}

/// `(sleep secs)` for compiled code: `args[0]` is an `f64`'s raw bit pattern.
///
/// The bits travel as the payload untouched — this shim does not sleep, and
/// must not: with tasks, `sleep` stops **the task**, and only the scheduler
/// knows whether there is anything else to run while it does. `rt_sleep`, the
/// thread-sleeping shim this replaces, was the reason `sleep` could not be
/// compiled: the same source meant two different things on the two sides.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_sleep(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        crate::fatal("rt_suspend_sleep: expected 1 argument");
    }
    call_state::set_pending_suspend(call_state::SUSPEND_SLEEP, *args);
    0
}

/// `(net-wait h interest)` for compiled code: `args[0]` is the stream
/// handle, `args[1]` the interest code — both plain integers, which is how
/// they travel. Records the wait; the activation then returns
/// `STATUS_SUSPEND` and the scheduler parks the task on the descriptor.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_io(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        crate::fatal("rt_suspend_io: expected 2 arguments");
    }
    call_state::set_pending_suspend_2(call_state::SUSPEND_IO, *args, *args.add(1));
    0
}

/// `(net-wait-for h interest secs)` for compiled code: the handle and the
/// interest packed into one word (handle above, interest in the low bit)
/// because the seconds' bits need a whole word of their own.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_io_for(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        crate::fatal("rt_suspend_io_for: expected 3 arguments");
    }
    let packed = (*args << 1) | (*args.add(1) & 1);
    call_state::set_pending_suspend_2(call_state::SUSPEND_IO_FOR, packed, *args.add(2));
    0
}

/// `(wait t)` for compiled code: `args[0]` is the `Task<T>` handle.
///
/// The handle goes through as the payload **still tagged**. Reading the
/// scheduler id out of it is the front end's job, not this crate's: the box is
/// a `BoxedObj::Struct` whose one field is a `TaskId`, and `TaskId` belongs to
/// the scheduler, which lives above here.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_wait(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        crate::fatal("rt_suspend_wait: expected 1 argument");
    }
    call_state::set_pending_suspend(call_state::SUSPEND_WAIT, *args);
    0
}

/// The channel operations for compiled code.
///
/// All six are suspensions, and only two of them can really wait. The table
/// of channels belongs to the scheduler — see
/// [`typelisp_abi::call_state::SUSPEND_CHAN_NEW`] for why it cannot live down
/// here — so every one of them is a question for the driver, answered on the
/// way back through the frame's value slot.
///
/// Each shim only *records*; none of them touches a channel, and none of them
/// blocks. That is the same division the other `rt_suspend_*` shims draw.
///
/// # Safety
///
/// `argc` must be at least as large as the number of words each reads, and
/// `args` must point to that many valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_chan_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        crate::fatal("rt_suspend_chan_new: expected 1 argument");
    }
    call_state::set_pending_suspend(call_state::SUSPEND_CHAN_NEW, *args);
    0
}

/// `(len ch)` for compiled code. See [`rt_suspend_chan_new`].
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_chan_len(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        crate::fatal("rt_suspend_chan_len: expected 1 argument");
    }
    call_state::set_pending_suspend(call_state::SUSPEND_CHAN_LEN, *args);
    0
}

/// `(cap ch)` for compiled code. See [`rt_suspend_chan_new`].
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_chan_cap(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        crate::fatal("rt_suspend_chan_cap: expected 1 argument");
    }
    call_state::set_pending_suspend(call_state::SUSPEND_CHAN_CAP, *args);
    0
}

/// `(close ch)` for compiled code. See [`rt_suspend_chan_new`].
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_chan_close(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        crate::fatal("rt_suspend_chan_close: expected 1 argument");
    }
    call_state::set_pending_suspend(call_state::SUSPEND_CHAN_CLOSE, *args);
    0
}

/// `(send ch v)` for compiled code. See [`rt_suspend_chan_new`].
///
/// `args[1]` arrives **already tagged**: the suspension site tagged it with
/// the element's own `Repr::field_kind`, because a raw machine word does not
/// say whether it is an integer or a pointer and the driver has only the word.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_chan_send(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        crate::fatal("rt_suspend_chan_send: expected 2 arguments");
    }
    call_state::set_pending_suspend_2(call_state::SUSPEND_CHAN_SEND, *args, *args.add(1));
    0
}

/// `(select ...)` for compiled code: the whole descriptor, copied out.
///
/// `args` is `[n, has-else, kind, chan, extra]...`, every word tagged — the
/// array is a compiled frame's slots and the collector walks those, so a raw
/// integer in one would be read as a pointer. The driver untags.
///
/// Copied rather than pointed at because the frame those slots live in is put
/// down the moment this returns.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to `argc` valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_chan_select(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        crate::fatal("rt_suspend_chan_select: expected at least 2 words");
    }
    let words = std::slice::from_raw_parts(args, argc as usize);
    call_state::set_pending_select(words);
    call_state::set_pending_suspend(call_state::SUSPEND_CHAN_SELECT, 0);
    0
}

/// `(recv ch)` for compiled code. See [`rt_suspend_chan_new`].
///
/// `args[1]` is the `Option<T>` type key the answer is built with — a string
/// the bridge appended at the call site, the same way `vector-op`'s `pop`
/// carries one. Nothing here can derive it: a channel's buffer holds tagged
/// words and a word does not name its type.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_chan_recv(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        crate::fatal("rt_suspend_chan_recv: expected 2 arguments");
    }
    call_state::set_pending_suspend_2(call_state::SUSPEND_CHAN_RECV, *args, *args.add(1));
    0
}
