//! A task that is a compiled chain and nothing else.

use typelisp_mem::Word;
use typelisp_mem::prelude::*;
use typelisp_mem::{Heap, Value};
use super::{pending_wait, wake_word, ChanOp, Progress, SchedError, TaskBody, Waiting, Wake};

// ---- a task that is a compiled chain and nothing else -------------------------

/// How a compiled-only task fails — the answer a program's own failure gets
/// in an executable, split the way the interpreter's `EvalError` splits it so
/// the two front ends say the same things about the same program.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskFailure {
    /// A `(panic ...)`, a runtime error, or a scheduler refusal: `panic: msg`.
    Panic(String),
    /// A `throw` no `catch` in the task claimed. Carries the tag.
    Throw(typelisp_mem::SymRef),
    /// The runtime disagreeing with itself — every task blocked, a handle of
    /// the wrong shape.
    Internal(String),
}

impl From<SchedError> for TaskFailure {
    fn from(e: SchedError) -> TaskFailure {
        match e {
            SchedError::Panic(m) => TaskFailure::Panic(m),
            SchedError::Internal(m) => TaskFailure::Internal(m),
            SchedError::Interrupted => TaskFailure::Panic("interrupted".to_string()),
        }
    }
}

impl core::fmt::Display for TaskFailure {
    /// The line an executable prints on the way out — the same wording
    /// `EvalError`'s `Display` uses for the same failures.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TaskFailure::Panic(m) => write!(f, "panic: {}", m),
            TaskFailure::Throw(tag) => write!(f, "throw: no enclosing (catch '{}) for this throw", tag.spelling()),
            TaskFailure::Internal(m) => write!(f, "error: {}", m),
        }
    }
}

/// A task that is a compiled chain and nothing else — every task an
/// ahead-of-time compiled executable has, since there is no interpreter in it
/// to have a continuation stack.
///
/// Where the interpreter's task keeps a continuation stack *and* a chain,
/// this keeps the chain alone: the executable's `main` enters a body, a task
/// `task` started applies a closure, and from then on every step is a drive of
/// the standing chain. What it needs from the heap is two state slots at
/// `sbase`, for the same reason the interpreter's task has them — a value
/// on its way in (a wake value, the closure to apply, a parked `send`'s
/// offer) is a Rust local until the chain takes it, and nothing else roots
/// it.
///
/// **Every answer is a tagged word.** The closure a `task` site builds tags
/// its result before returning (`core_bridge::translate_spawn_call`), and an
/// executable's `main` answers an `int` (tagged) or unit (the word `0`, a
/// fixnum). So the task's result is `decode(word)` with no representation
/// to consult — which is what lets this crate hold the scheduler at all.
///
/// **`Send`, now that `Value` is** (`docs/dev/os-threads-design.md` §1/§3):
/// `Value::Cons` used to be the one raw pointer standing in the way (into
/// the `Heap`'s cons arena), and nothing else here is tied to a thread — no
/// `Rc`, no thread-local handle, no machine frame — so a scheduler that
/// spreads tasks over OS threads finds its wall in the heap and not in the
/// task, exactly as this comment predicted before `Value: Send` landed.
/// `assert_send::<CompiledTask>()` is a real test now (`tests/sched_send_test.rs`),
/// not just a thing that was tried once.
pub struct CompiledTask {
    chain: crate::coroutine::FrameStack,
    state: CompiledState,
    sbase: usize,
    /// How the chain reads what it was waiting for out of its value slot,
    /// recorded when it suspended.
    wake: Wake,
    /// Whether the body's final word is *not* a tagged value — a global
    /// initialiser answers with `rt_global_new`'s raw storage id, which the
    /// body has already stored and nothing reads back. Decoding it would be
    /// reading bits that were never a `Value`.
    discard_answer: bool,
    /// This task's own share of `typelisp_rt`'s in-flight-unwind
    /// thread-locals (`IN_FLIGHT_TAG`/`CAUGHT_UNWIND`), installed by `step`
    /// before it drives the chain and taken back once it stops — see
    /// `docs/dev/os-threads-design.md` §3. Empty exactly when this task is
    /// not in the middle of an unwind.
    parked_unwind: crate::ParkedUnwind,
    /// How the chain's final word becomes the task's result, when it is not
    /// the tagged word every site-built closure answers with — see
    /// [`CompiledTask::enter_with`].
    answer: Option<AnswerDecoder>,
    /// Whether an interpreted callee is somebody else's to run: the task is
    /// an interpreter's (`TaskBody::runs_anywhere` of its body), and a chain
    /// that applies one stops there with [`Progress::NeedsMain`] and its
    /// application in [`CompiledState::HandedOff`], for the interpreter's
    /// task body to take over ([`Self::hand_off`]). An executable's task
    /// applies one on this machine frame instead, with the interpreter an
    /// `eval`-carrying executable has (`apply_on_this_frame`).
    hands_off: bool,
}

/// Reads a compiled body's final word back as a value, by the body's
/// declared return representation — which this crate cannot name, so the
/// front end that can hands the reading over. Fails the way a crossing that
/// disagrees with itself does, as the message.
pub type AnswerDecoder = Box<dyn Fn(&mut Heap, i64) -> Result<Value, String> + Send>;

/// What [`CompiledTask::enter_with`] enters.
pub enum Entry {
    /// A compiled body, by its coroutine-ABI entry.
    Body(crate::coroutine::CoroutineFn),
    /// A compiled closure, tagged; rooted in the task's first state slot.
    Closure(Value),
}

/// What a [`CompiledTask`] that stopped for an interpreter hands over to the
/// task body that has one: its chain, standing, and the application the
/// chain is waiting on.
pub struct HandedOff {
    /// The standing chain. Its frames are rooted on the task's root stack —
    /// the closure and every argument of the application with them (a
    /// call's operands live in the caller's frame slots).
    pub chain: crate::coroutine::FrameStack,
    /// Where the task's roots begin; see [`TaskBody::sbase`].
    pub sbase: usize,
    /// The task's in-flight unwind state, as it stopped.
    pub parked_unwind: crate::ParkedUnwind,
    /// What the chain waits on: `Paused::Applying` or `Paused::ApplyingDyn`
    /// for an interpreted callee, or `Paused::Suspended` for the
    /// interpreter's thread itself (`Waiting::Main`) — resumed with unit.
    pub pending: crate::coroutine::Paused,
}

enum CompiledState {
    /// Enter this body with no arguments — the executable's `main`.
    Entry(crate::coroutine::CoroutineFn),
    /// Apply this closure to no arguments — a task `task` started. Rooted in
    /// the first state slot.
    Start(Value),
    /// Enter this with these argument words — an interpreted `thread`'s
    /// task. What the words point at is rooted in the state slots until the
    /// entry's prologue has copied them into its frame.
    EnterWith(Entry, Vec<Word>),
    /// Re-enter the standing chain with this word in its value slot.
    Resume(i64),
    /// Hand the standing chain the unwind parked for it (`deliver` of an
    /// error) and keep driving.
    Raise,
    /// Waiting. The scheduler moves it out of this with `deliver`.
    Blocked(Waiting),
    /// Stopped on an interpreted callee, for the interpreter to run — see
    /// `hands_off`. Nothing moves it out: [`CompiledTask::hand_off`] takes the
    /// task apart.
    HandedOff(crate::coroutine::Paused),
}

impl CompiledTask {
    /// A task that enters `f` with no arguments — what the executable's
    /// `main` is to the scheduler. Built on the task's own root stack.
    pub fn entry(heap: &mut Heap, f: crate::coroutine::CoroutineFn) -> CompiledTask {
        let sbase = heap.root_count();
        heap.push_root(Value::Empty);
        heap.push_root(Value::Empty);
        CompiledTask {
            chain: crate::coroutine::FrameStack::for_task(),
            state: CompiledState::Entry(f),
            sbase,
            wake: Wake::Unit,
            discard_answer: false,
            parked_unwind: crate::ParkedUnwind::default(),
            answer: None,
            hands_off: false,
        }
    }

    /// An interpreter's task (`hands_off`) that enters `entry` with `args` —
    /// words already in the entry's own representations — and answers with
    /// what `answer` reads the final word as. What an interpreted `thread`
    /// starts: the call's callee and arguments were evaluated by the task
    /// that ran the `thread`, and only the call itself moves.
    ///
    /// `held` is whatever keeps the heap objects among `args` alive — the
    /// arguments as values — rooted in the second state slot until the first
    /// step (a closure entry is rooted in the first). Built on the task's own
    /// root stack.
    pub fn enter_with(heap: &mut Heap, entry: Entry, args: Vec<Word>, held: Value, answer: AnswerDecoder) -> CompiledTask {
        let sbase = heap.root_count();
        heap.push_root(match &entry {
            Entry::Closure(c) => *c,
            Entry::Body(_) => Value::Empty,
        });
        heap.push_root(held);
        CompiledTask {
            chain: crate::coroutine::FrameStack::for_task(),
            state: CompiledState::EnterWith(entry, args),
            sbase,
            wake: Wake::Unit,
            discard_answer: false,
            parked_unwind: crate::ParkedUnwind::default(),
            answer: Some(answer),
            hands_off: true,
        }
    }

    /// [`TaskBody::start_closure`] for an interpreter's task: one that stops
    /// on an interpreted callee rather than applying it (`hands_off`).
    pub fn start_closure_handing_off(heap: &mut Heap, closure: Value) -> CompiledTask {
        CompiledTask { hands_off: true, ..<CompiledTask as TaskBody>::start_closure(heap, closure) }
    }

    /// Takes apart a task that answered [`Progress::NeedsMain`].
    pub fn hand_off(self) -> HandedOff {
        let CompiledState::HandedOff(pending) = self.state else {
            unreachable!("only a task that stopped on an interpreted callee is handed off");
        };
        HandedOff { chain: self.chain, sbase: self.sbase, parked_unwind: self.parked_unwind, pending }
    }

    /// [`Self::entry`] for a body whose answer is nobody's — a `defvar`
    /// initialiser (see `discard_answer`). The task finishes with unit.
    pub fn initialiser(heap: &mut Heap, f: crate::coroutine::CoroutineFn) -> CompiledTask {
        CompiledTask { discard_answer: true, ..Self::entry(heap, f) }
    }

    /// Writes what the next state carries into the state slots.
    fn set_slots(&self, heap: &mut Heap) {
        let held = match &self.state {
            CompiledState::Start(closure) => *closure,
            CompiledState::Blocked(Waiting::Chan(ChanOp::Send(_, v)))
            | CompiledState::Blocked(Waiting::Spawn(v, _))
            | CompiledState::Blocked(Waiting::SpawnThread(v, _)) => *v,
            _ => Value::Empty,
        };
        heap.set_root(self.sbase, held);
        heap.set_root(self.sbase + 1, Value::Empty);
    }

    /// What an interpreted callee applied on this machine frame answered:
    /// the chain is resumed with the value, or — when it raised — the raise
    /// is parked for the next step to hand the chain.
    fn after_apply(&mut self, heap: &mut Heap, answer: Result<i64, typelisp_abi::Raised>) -> Progress<TaskFailure> {
        self.state = match answer {
            Ok(v) => CompiledState::Resume(v),
            Err(_raised) => {
                crate::coroutine::park_raised();
                CompiledState::Raise
            }
        };
        self.set_slots(heap);
        Progress::Running
    }

    /// What a drive that stopped means for the task.
    fn after_drive(&mut self, heap: &mut Heap, outcome: Result<i64, crate::coroutine::Paused>) -> Progress<TaskFailure> {
        match outcome {
            Ok(_) if self.discard_answer => Progress::Done(Ok(Value::Empty)),
            Ok(word) => match &self.answer {
                Some(answer) => Progress::Done(answer(heap, word).map_err(TaskFailure::Internal)),
                None => Progress::Done(Ok(typelisp_abi::decode(word as Word))),
            },
            // The chain stays exactly as it is, rooted by the prologues that
            // built it; only the answer's shape has to be remembered.
            Err(crate::coroutine::Paused::Suspended) => match pending_wait(heap) {
                // Only an interpreter's task has an interpreter's thread to
                // move to; there it stops, and the task body that takes it
                // over resumes the chain (`Paused::Suspended` as `pending`).
                Ok((Waiting::Main, _)) if self.hands_off => {
                    self.state = CompiledState::HandedOff(crate::coroutine::Paused::Suspended);
                    Progress::NeedsMain
                }
                // Anywhere else there is nowhere to move to, and nothing to
                // wait for: go on at once, without giving up the turn.
                Ok((Waiting::Main, _)) => {
                    self.state = CompiledState::Resume(0);
                    self.set_slots(heap);
                    Progress::Running
                }
                Ok((w, wake)) => {
                    self.wake = wake;
                    self.state = CompiledState::Blocked(w.clone());
                    self.set_slots(heap);
                    Progress::Blocked(w)
                }
                Err(e) => Progress::Done(Err(e.into())),
            },
            // A compiled frame applied an interpreted function value. Only an
            // interpreter can run it, and the one an `eval`-carrying
            // executable has runs it on this machine frame — which cannot
            // suspend, the rule `Interp::apply` has always had. An executable
            // with no interpreter has nothing to run it with, and says so.
            Err(pending @ (crate::coroutine::Paused::Applying { .. } | crate::coroutine::Paused::ApplyingDyn { .. }))
                if self.hands_off =>
            {
                self.state = CompiledState::HandedOff(pending);
                Progress::NeedsMain
            }
            Err(crate::coroutine::Paused::Applying { closure, args }) => {
                // SAFETY: the closure and its arguments came from a compiled
                // `apply` site in the heap registered on this thread.
                let answer = unsafe { crate::apply_on_this_frame(closure, &args, "a task") };
                self.after_apply(heap, answer)
            }
            Err(crate::coroutine::Paused::ApplyingDyn { vtable, slot, args }) => {
                // SAFETY: as above, for a `:dyn` site's slot in a vtable this
                // heap's program registered.
                let answer = unsafe {
                    crate::reify_dyn_slot(vtable, slot, "a :dyn call").and_then(|closure| crate::apply_on_this_frame(closure, &args, "a task"))
                };
                self.after_apply(heap, answer)
            }
            // The chain is gone — every frame popped, its roots cut back —
            // and what was unwinding is parked. A program's own failure is
            // this task's failure; anything else is a bug in the runtime and
            // keeps unwinding untouched.
            Err(crate::coroutine::Paused::Unwinding) => {
                let payload = crate::take_activation_unwind()
                    .unwrap_or_else(|| typelisp_abi::fatal("a task's chain reported an unwind but none is being carried"));
                // SAFETY: a `Heap` is registered on this thread — the chain
                // just ran on it.
                Progress::Done(Err(unsafe { unwind_failure(payload) }))
            }
        }
    }
}

/// The program-level failure a parked unwind payload stands for, or the
/// payload back if it is nobody's — a real bug, which must keep unwinding
/// rather than come back as a plausible-looking typelisp error.
///
/// # Safety
///
/// A `Heap` must be registered on this thread (a throw's value lives there).
unsafe fn unwind_failure(payload: Box<dyn core::any::Any + Send>) -> TaskFailure {
    let payload = match payload.downcast::<typelisp_abi::CompiledPanic>() {
        Ok(p) => return TaskFailure::Panic(p.message),
        Err(other) => other,
    };
    match payload.downcast::<typelisp_abi::CompiledThrow>() {
        Ok(_) => {
            let (tag, _) = crate::take_throw()
                .unwrap_or_else(|| typelisp_abi::fatal("a compiled throw unwound with no throw in flight"));
            TaskFailure::Throw(tag)
        }
        Err(_) => typelisp_abi::fatal("a task was left by an unwind that is neither a panic nor a throw"),
    }
}

impl TaskBody for CompiledTask {
    type Cx = ();
    type Error = TaskFailure;

    fn start_closure(heap: &mut Heap, closure: Value) -> CompiledTask {
        let sbase = heap.root_count();
        heap.push_root(closure);
        heap.push_root(Value::Empty);
        CompiledTask {
            chain: crate::coroutine::FrameStack::for_task(),
            state: CompiledState::Start(closure),
            sbase,
            wake: Wake::Unit,
            discard_answer: false,
            parked_unwind: crate::ParkedUnwind::default(),
            answer: None,
            hands_off: false,
        }
    }

    fn sbase(&self) -> usize {
        self.sbase
    }

    fn step(&mut self, heap: &mut Heap, _cx: &()) -> Progress<TaskFailure> {
        // Install what this task itself left in `IN_FLIGHT_TAG`/`CAUGHT_UNWIND`
        // the last time it stopped — empty unless it is resuming into a
        // `catch`'s dispatch or an `unwind-protect`'s cleanup mid-unwind. See
        // `docs/dev/os-threads-design.md` §3: another task may have run its
        // own throw through these same thread-locals since then, so nothing
        // here may be assumed left over from last time except what this call
        // installs.
        crate::restore_parked_unwind(core::mem::take(&mut self.parked_unwind));
        // Taken out to be consumed; every path below either puts the next
        // state back or reports `Done`, after which nothing reads it.
        let state = core::mem::replace(&mut self.state, CompiledState::Resume(0));
        let outcome = match state {
            CompiledState::Entry(f) => self.chain.run(heap, f, &[]),
            CompiledState::Start(closure) => self.chain.run_closure(heap, typelisp_abi::encode(closure), &[]),
            CompiledState::EnterWith(Entry::Body(f), args) => self.chain.run(heap, f, &args),
            CompiledState::EnterWith(Entry::Closure(closure), args) => {
                self.chain.run_closure(heap, typelisp_abi::encode(closure), &args)
            }
            CompiledState::Resume(word) => {
                self.chain.set_top_value(heap, word);
                self.chain.resume(heap, 0)
            }
            CompiledState::Raise => self.chain.raise(heap, 0),
            // Only the scheduler moves a task out of this, by delivering what
            // it was waiting for.
            CompiledState::Blocked(w) => {
                self.state = CompiledState::Blocked(w.clone());
                self.parked_unwind = crate::take_parked_unwind();
                return Progress::Blocked(w);
            }
            CompiledState::HandedOff(_) => unreachable!("a task handed off is not stepped again"),
        };
        let progress = self.after_drive(heap, outcome);
        // Save whatever this call leaves in flight back onto the task, and
        // clear the thread-locals so a different task stepped next sees a
        // clean slate rather than this one's leftovers.
        self.parked_unwind = crate::take_parked_unwind();
        progress
    }

    fn deliver(&mut self, heap: &mut Heap, answer: Result<Value, SchedError>) {
        match answer {
            // Encoded through the shape the suspension recorded, for the
            // reason every crossing obeys: the word's meaning is in the type,
            // and the value cannot say. A unit wake is a word the collector
            // never follows; a typed one crosses tagged.
            Ok(v) => {
                let word = wake_word(self.wake, v);
                // The word is a Rust local until the chain's value slot takes
                // it on the next step, and other tasks allocate in between.
                heap.set_root(self.sbase, v);
                heap.set_root(self.sbase + 1, Value::Empty);
                self.state = CompiledState::Resume(word);
            }
            // The chain is asked for a handler the way a compiled `panic`
            // asks: the payload is parked where the pad looks, and the next
            // step raises it into the innermost frame.
            Err(e) => {
                let message = match e {
                    SchedError::Panic(m) | SchedError::Internal(m) => m,
                    SchedError::Interrupted => "interrupted".to_string(),
                };
                // Parked on the task directly rather than through
                // `park_activation_unwind`'s live thread-locals: `deliver`
                // runs between two tasks' steps (`sched::deliver_to`), with
                // no guarantee that *this* task's saved state is what is
                // currently installed on the thread — writing to the
                // thread-locals here would hand the panic to whichever task
                // steps next instead of to this one. `step` installs it when
                // this task runs again, before raising it into the chain.
                self.parked_unwind = crate::ParkedUnwind::panic(message);
                self.state = CompiledState::Raise;
                self.set_slots(heap);
                // A newer exit replaces whatever throw this task's own
                // cleanup might already have had in flight — CLHS's rule for
                // an exit raised while another is still travelling. Safe to
                // touch directly (not through the task's own saved state):
                // `deliver_to` has already switched `heap`'s current root
                // stack to this task's, so this clears only this task's own
                // slot.
                heap.set_in_flight_throw(None);
            }
        }
    }

    /// A `throw` that leaves a task has no catch to reach — a tag does not
    /// cross a task boundary — and a panic is not recoverable by definition.
    /// Either way the program stops, which is Go's rule for an unrecovered
    /// panic.
    /// None: a thrown value is not kept past the task (`TaskFailure::Throw`
    /// carries the tag alone).
    fn failure_values(_e: &TaskFailure) -> Vec<Value> {
        Vec::new()
    }

    fn failure_left_task(e: TaskFailure) -> TaskFailure {
        match e {
            TaskFailure::Throw(tag) => TaskFailure::Panic(format!("`throw` of `{}` left its task", tag.spelling())),
            other => other,
        }
    }
}
