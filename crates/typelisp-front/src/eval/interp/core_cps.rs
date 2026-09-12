//! The evaluator as an explicit state machine over a continuation stack.
//!
//! The evaluation loop lives here; [`core_eval`](super::core_eval) holds the
//! tag vocabulary, the leaves, and the readers this calls into. What makes it
//! a machine rather than a walk is *where the rest of the computation lives*:
//! a non-tail subexpression is not a recursive Rust call but a `Frame` on a
//! `Vec`. The Rust stack stays flat and the continuation becomes data — which
//! is what a task (goroutine) needs in order to be suspended and resumed.
//!
//! The design, and the mapping from every `Op` to the frames it needs, is in
//! `docs/dev/cps-evaluator-design.md`.
//!
//! # Roots
//!
//! Two slots at the bottom hold whatever the *current state* carries, and are
//! overwritten in place (`set_root`) rather than pushed, which is what keeps
//! a tail loop from growing the root stack without bound. Above them, each
//! frame roots its own values when pushed and drops
//! them with `truncate_roots` when unwound:
//!
//! ```text
//! roots: [ ..., state-env, state-form, frame0's values.., frame1's values.., ... ]
//!               ^ sbase                ^ frame0's base
//! ```
//!
//! A frame's subexpression never needs a root of its own: it is reachable
//! from the form the frame already roots. Only a *computed* value does, which
//! is why `State::Apply` carries one through the state slots.
//!
//! The one window that must not allocate is between `truncate_roots` (which
//! drops a frame's values) and the `set_state` that re-roots what the next
//! state carries. Nothing in between touches the heap.

use typelisp_mem::{Heap, RootScope, RootStackId, SymRef, Value};

use crate::check::core;
use crate::check::repr::Repr;
use crate::eval::value::EvalError;

use super::core_eval::{
    bool_field, construct_sexpr_core, env_lookup, extend_env, heap_err, int_field, match_core_pattern, path_field,
    param_reprs, repr_list, str_field, sym_field, tail_after, tail_after_value, Op, StepCmd,
};
use super::{option_value, FnDef, Interp};

use std::rc::Rc;

/// What one step of the machine produced.
enum State {
    /// Evaluate this form in this environment.
    Eval(Value, Value),
    /// A value is ready. Hand it to the top frame, or return it if there is none.
    Apply(Value),
    /// Begin by completing a call whose callee and arguments are already
    /// evaluated. **Only a task `go` started starts here**: the form that ran
    /// the `go` did the evaluating, and this is what it handed over.
    ///
    /// `argv` is a heap list rather than a `Vec` so the two state slots can
    /// root it — a `Vec<Value>` in a Rust local is invisible to the collector.
    Enter { form: Value, argv: Value, kind: ArgsKind },
    /// Suspended until something else happens. The scheduler owns this task
    /// from here; whatever it was waiting for arrives as the `Apply` that
    /// replaces this state.
    Blocked(Waiting),
    /// Enter a coroutine-ABI compiled body, with its arguments already
    /// marshaled.
    ///
    /// **Not a call.** The chain of frames the body builds belongs to the
    /// task (`Task::compiled`), so the body can stop in the middle and the
    /// task can be put down with it — which is the whole of Phase C3. A
    /// *classic* body still goes through `Interp::call_compiled` and is as
    /// atomic as it always was: it has nowhere to keep its state.
    CompiledEnter { argv: Value, start: DriveStart },
    /// Keep driving a chain that is already standing — after a suspension, or
    /// after a call it made was answered.
    ///
    /// `wake` is what the task was waiting for, to be written into the
    /// innermost frame's value slot before it is re-entered: the same place a
    /// returning callee leaves its answer, because from the resuming
    /// function's side those are the same thing.
    CompiledResume { drive: DriveCtx, wake: Option<(Value, Repr)> },
    /// Hand an exit to a chain that is already standing, and keep driving.
    ///
    /// What an *interpreted* callee's non-local exit becomes when the call
    /// came from compiled code. Since C5 that callee runs on this task's own
    /// continuation stack, so its exit travels up as an ordinary
    /// `State::Unwind` until it reaches the `Frame::DriveCompiled` that
    /// stands for the compiled frame waiting underneath — and from there the
    /// chain has to be asked, because a `catch` in it is entitled to claim
    /// the throw and an `unwind-protect` in it has cleanups to run.
    ///
    /// The exit itself is already parked on the compiled side
    /// (`crossing::park_for_compiled`), where the chain's pad looks for it.
    CompiledRaise { drive: DriveCtx },
    /// A non-local exit is in flight. Discard frames until one claims it.
    ///
    /// This is where `break`/`return`/`return-from`/`throw` live. The value
    /// carried past the frames being discarded is rooted through the state
    /// slots, so the interpreter needs no dedicated single-slot root, and makes
    /// no "at most one throw in flight" assumption.
    ///
    /// `Heap::set_in_flight_throw` is still there for the *compiled* side: a
    /// compiled `throw` travels as a Rust panic, whose payload the collector
    /// cannot see, so `typelisp_rt::park_throw` parks the value in that slot.
    /// The one-slot assumption survives there — something Phase B has to look
    /// at, since two tasks can then be unwinding at once.
    Unwind(EvalError),
}

/// Everything a task needs to keep about a compiled body it is driving.
///
/// This is what used to be Rust locals in `Interp::call_compiled`. It has to
/// be data because the drive can stop in the middle: the marshaling happened
/// before the first entry and the decoding happens after the last return, and
/// any number of suspensions can sit between them.
///
/// It holds no `Value`, deliberately — the chain's frames are rooted by the
/// prologues that made them (`frame-begin`'s `rt_push_sexpr_root`), on *this
/// task's* root stack, and `Heap::gc` marks every root stack rather than only
/// the running one. So a suspended chain stays alive with nothing extra to
/// register.
struct DriveCtx {
    /// What the crossing was set up with, kept because the trace line and the
    /// return representation are still owed after the last resume.
    start: DriveStart,
    /// Where the root stack stood before marshaling, for the unwind path.
    roots_on_entry: usize,
    /// How many roots marshaling pushed, for the ordinary path.
    crossing_roots: usize,
    /// How deep `Task::compiled` was when this crossing began.
    ///
    /// **A task can have more than one chain segment standing** since C5: a
    /// compiled frame applies an interpreted closure, that closure calls a
    /// compiled function, and now there are two runs of compiled frames in
    /// one `FrameStack` with interpreted frames between them. Each segment
    /// drives down to its own base, and the frames below it belong to
    /// somebody else.
    ///
    /// Zero for the outermost segment, which is every crossing before C5 and
    /// still nearly all of them.
    base: usize,
}

/// What is known about a compiled crossing *before* its arguments are
/// marshaled.
///
/// The split is not cosmetic. Marshaling pushes GC roots, and it has to happen
/// **after** the continuation frame that named this call has released its own
/// roots — `step_task` truncates to the frame's base right after `resume`
/// returns, which would take the crossing's roots with it. So the arguments
/// travel to `CompiledEnter` as a heap list (rooted through the state slots,
/// the way `State::Enter`'s do) and are encoded there.
///
/// The synchronous `Interp::call_compiled` never had to think about this: it
/// pushed and popped inside one Rust call, entirely before the truncate.
struct DriveStart {
    /// Held for as long as the drive lasts, so the machine code cannot be
    /// dropped underneath a suspended chain by a redefinition.
    body: std::rc::Rc<dyn super::CompiledBody>,
    /// The callee's declared parameter representations — what the words the
    /// arguments encode to mean.
    params: Vec<Repr>,
    /// The callee's declared return representation — what the final word
    /// means. Nothing else can say: the driver hands raw bits through.
    ret: Repr,
    /// The trace line owed on the way back, if `trace` is on for this callee.
    watch: Option<(String, usize)>,
}

/// The `ArgsKind` of an already-checked call node — what `(go CALL)` wraps.
///
/// `check_go` has already refused anything that is not one of these four, so a
/// tag arriving here that is not a call means the checker and the evaluator
/// disagree about what `go` accepts.
fn call_kind(heap: &Heap, call: Value) -> Result<ArgsKind, EvalError> {
    let tag = match heap.car(call) {
        Ok(Value::Symbol(id)) => id,
        _ => return Err(EvalError::Internal("eval: (go ..) does not wrap a node".to_string())),
    };
    match Op::from_sym(tag) {
        Some(Op::Call) => Ok(ArgsKind::Call),
        Some(Op::Assoc) => Ok(ArgsKind::Assoc),
        Some(Op::DynCall) => Ok(ArgsKind::DynCall),
        Some(Op::Apply) => Ok(ArgsKind::Apply),
        other => Err(EvalError::Internal(format!(
            "eval: (go ..) wraps {:?}, which is not a call",
            other
        ))),
    }
}

/// Which node an `Args` frame is collecting arguments for.
///
/// Five core forms share one shape — a run of argument forms at a fixed offset,
/// evaluated left to right — and differ only in what happens once they are all
/// in hand. The leading fields they skip are the checker's and the bridge's
/// (names, paths, representations); evaluation is uniform over `Value`.
///
/// `Apply` is the odd one: its callee is a *form* at field 0 rather than a name
/// the checker resolved, so it needs its own two frames to evaluate that first.
/// Once the callee is a value it rejoins this path with the callee riding at
/// the head of `argv` — which is what lets `go` hand an `apply` over to a task
/// with no second `Enter` state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ArgsKind {
    /// `(call WRITTEN HOME PATH (R...) ARG...)`
    Call,
    /// `(construct PATH KEY VARIANT MUTABLE (R...) ARG...)`
    Construct,
    /// `(assoc PATH SYM INSTANCE (HOME...) RET-REPR (R...) ARG...)`
    Assoc,
    /// `(dyn-call PATH SYM SLOT VTABLE (R...) ARG...)`
    DynCall,
    /// `(apply CALLEE RET-REPR (R...) ARG...)`, with the *evaluated* callee at
    /// the head of the argument run rather than field 0's form.
    Apply,
}

impl ArgsKind {
    /// How many leading fields are not arguments.
    fn skip(self) -> usize {
        match self {
            ArgsKind::Call => 4,
            ArgsKind::Construct => 5,
            ArgsKind::Assoc => 7,
            ArgsKind::DynCall => 5,
            ArgsKind::Apply => 3,
        }
    }

    /// Which leading field holds the arguments' representations.
    ///
    /// The interpreter reads them for one thing only: decoding arguments that
    /// arrive from compiled code, where the word alone cannot say whether it
    /// is a raw `f64` bit pattern or a tagged pointer. `core_bridge`'s
    /// `call_node_shape` is the same table on the compiling side.
    fn repr_at(self) -> usize {
        match self {
            ArgsKind::Call => 3,
            ArgsKind::Construct => 4,
            ArgsKind::Assoc => 5,
            ArgsKind::DynCall => 4,
            ArgsKind::Apply => 2,
        }
    }

    fn what(self) -> &'static str {
        match self {
            ArgsKind::Call => "call",
            ArgsKind::Construct => "construct",
            ArgsKind::Assoc => "assoc",
            ArgsKind::DynCall => "dyn-call",
            ArgsKind::Apply => "apply",
        }
    }
}

/// A suspended computation: what to do once the subexpression being evaluated
/// produces a value.
///
/// Frames hold the *form* rather than its pieces, so one root covers every
/// part of it — `core::field` reads the piece back on resume.
enum Frame {
    /// `(if COND THEN ELSE)` — waiting on the condition.
    If { form: Value, env: Value },
    /// `(panic MSG)` — waiting on the message.
    Panic,
    /// `(field-get OBJ IDX REPR)` — waiting on the object.
    FieldGet { idx: usize },
    /// `(dyn-value INNER)` — waiting on the trait object.
    DynValue,
    /// `(set SYM VALUE)` — waiting on the value. The frame carries the *name*
    /// rather than the cell: the lookup happens on resume, once the value is
    /// in hand.
    Set { sym: SymRef, env: Value },
    /// A body's remaining forms. `rest` is a non-empty cons list of forms not
    /// yet evaluated; the value being resumed on is discarded, since only a
    /// sequence's last form contributes its value.
    Seq { rest: Value, env: Value },
    /// Waiting on one argument of a call, construction or method dispatch.
    ///
    /// `done` holds the arguments already evaluated, in order; the form
    /// carries the rest. An argument evaluated three ago is as collectible as
    /// the one that just arrived, so all of them stay rooted until the call
    /// itself is made.
    /// `spawn` marks the arguments of a `(go ...)`: when they are all in,
    /// the call is handed to a new task instead of being made here.
    Args { form: Value, done: Vec<Value>, env: Value, kind: ArgsKind, spawn: bool },
    /// `(step E)` — running `E` with stepping armed, holding what to restore.
    Step { was_stepping: bool, was_quiet: usize },
    /// A traced or stepped call, open around its callee's body.
    ///
    /// This is the one frame a call gets: `enter_fn` normally makes a body a
    /// tail jump, which leaves nothing to report a return from. Tracing needs
    /// the frame, so it costs the tail position — as it does in the recursive
    /// evaluator, where a traced call is a real Rust frame too.
    TracedCall { name: String, depth: usize },
    /// A compiled chain is standing and this task was put down waiting for
    /// something. When the value arrives, put it in the chain's value slot and
    /// keep driving.
    ///
    /// A frame rather than a wider `Blocked` state, so the scheduler needs no
    /// changes at all: it wakes a task by replacing its state with
    /// `State::Apply(v)`, and this is the thing that catches that value on
    /// behalf of the compiled world. `wake` is the representation to encode it
    /// back through, which the suspension site knows and nothing else does.
    DriveCompiled { drive: DriveCtx, wake: Repr },
    /// Waiting for a `select` to be decided, holding the arms and the
    /// environment to run the winner in.
    ///
    /// Also what keeps a parked `select`'s operands alive: they are bindings
    /// of the `let` wrapped around the node, so rooting `env` roots every
    /// channel it is waiting on and every value it is offering to send.
    SelectArm { form: Value, env: Value },
    /// `(dyn-new STR PATH ((PATH SYM)...) (...) R E)` — waiting on the value
    /// to box.
    DynNew { form: Value },
    /// `(dyn-upcast PATH E)` — waiting on the trait object to re-view.
    DynUpcast { form: Value },
    /// `(apply E RET-R (R...) E...)` — waiting on the callee.
    ApplyCallee { form: Value, env: Value, spawn: bool },
    /// `(apply E RET-R (R...) E...)` — waiting on one argument, callee in hand.
    ApplyArgs { form: Value, callee: Value, done: Vec<Value>, env: Value, spawn: bool },
    /// `(match E R (P E...) ...)` — waiting on the scrutinee. The first arm
    /// whose pattern matches wins.
    MatchArms { form: Value, env: Value },
    /// `(field-set OBJ IDX R VALUE)` — waiting on the object.
    FieldSetObj { form: Value, env: Value },
    /// `(field-set OBJ IDX R VALUE)` — waiting on the value, object in hand.
    FieldSetVal { obj: Value, idx: usize },
    /// `(set-global (WRITTEN...) (HOME...) PATH R VALUE)` — waiting on the value.
    SetGlobal { form: Value },
    /// `(loop BODY...)` — running the body round and round. `rest` is what
    /// is left of this pass; when it runs out the frame starts over from
    /// `body`. The only way out is an exit this frame claims.
    Loop { body: Value, rest: Value, env: Value },
    /// `(block NAME BODY)` — the lexical named escape's landing site.
    ///
    /// The name is matched only to tell nested blocks apart while the signal
    /// travels: the checker already decided which block a `return-from`
    /// belongs to, so an unmatched one is not a runtime possibility the way a
    /// `throw` with no `catch` is.
    Block { name: String },
    /// `(catch 'TAG BODY)` — the dynamic escape's landing site. A throw on
    /// some *other* tag keeps travelling; swallowing it here is the bug CL's
    /// `eq` tag comparison exists to prevent.
    Catch { tag: String },
    /// `(return VALUE)` — waiting on the value to carry out.
    Return,
    /// `(return-from NAME VALUE)` — waiting on the value.
    ReturnFrom { name: String },
    /// `(throw 'TAG VALUE)` — waiting on the value.
    Throw { tag: String },
    /// `(unwind-protect PROTECTED CLEANUP)` — running `PROTECTED`. However it
    /// is left, the cleanup runs.
    Protect { cleanup: Value, env: Value },
    /// Running a cleanup after its protected form produced `value`; the
    /// cleanup's own value is discarded.
    CleanupValue { value: Value },
    /// Running a cleanup while `pending` waits to resume its flight.
    ///
    /// A non-local exit *from the cleanup itself* wins over the pending one,
    /// matching CLHS: "the cleanup-forms of unwind-protect are not protected
    /// by that unwind-protect".
    CleanupUnwind { pending: EvalError },
    /// `(let ((SYM R INIT)...) BODY...)` — waiting on one initialiser.
    ///
    /// `binds` is the whole binding list (the names are read back from it once
    /// every value is in), `rest` the bindings still to evaluate, and `done`
    /// the values collected so far, in binding order.
    LetInit {
        binds: Value,
        rest: Value,
        done: Vec<Value>,
        body: Value,
        env: Value,
    },
}

/// The continuation stack: frames, each with the root count from just before
/// it was pushed (so unwinding one drops exactly the roots it added) and the
/// source location of the form that pushed it.
///
/// The location is what places a runtime error raised while *resuming* — the
/// value is in hand by then, and the form it belongs to is no longer the one
/// being evaluated. `EvalError::at` keeps the innermost, so a subexpression
/// that already placed the error still wins.
struct CpsStack {
    frames: Vec<(Frame, usize, Option<crate::Loc>)>,
}

impl CpsStack {
    fn new() -> CpsStack {
        CpsStack { frames: Vec::new() }
    }

    /// Pushes `frame`, rooting the values it holds.
    fn push(&mut self, heap: &mut Heap, frame: Frame, loc: Option<crate::Loc>) {
        let base = heap.root_count();
        match &frame {
            Frame::If { form, env } => {
                heap.push_root(*form);
                heap.push_root(*env);
            }
            Frame::Set { env, .. } => heap.push_root(*env),
            Frame::ApplyArgs { form, callee, done, env, .. } => {
                heap.push_root(*form);
                heap.push_root(*callee);
                heap.push_root(*env);
                for v in done {
                    heap.push_root(*v);
                }
            }
            Frame::ApplyCallee { form, env, .. }
            | Frame::MatchArms { form, env }
            | Frame::SelectArm { form, env }
            | Frame::FieldSetObj { form, env } => {
                heap.push_root(*form);
                heap.push_root(*env);
            }
            // The box has to stay reachable while the value expression runs:
            // that expression can allocate, and nothing else refers to it.
            Frame::FieldSetVal { obj, .. } => heap.push_root(*obj),
            Frame::SetGlobal { form } | Frame::DynNew { form } | Frame::DynUpcast { form } => {
                heap.push_root(*form)
            }
            Frame::Loop { body, rest, env } => {
                heap.push_root(*body);
                heap.push_root(*rest);
                heap.push_root(*env);
            }
            Frame::Protect { cleanup, env } => {
                heap.push_root(*cleanup);
                heap.push_root(*env);
            }
            // The protected form's value has to outlive the cleanup, which
            // allocates — the one unwind that runs code before continuing.
            Frame::CleanupValue { value } => heap.push_root(*value),
            Frame::CleanupUnwind { pending } => {
                if let EvalError::Return(v) | EvalError::ReturnFrom(_, v) | EvalError::Throw(_, v) = pending {
                    heap.push_root(**v);
                }
            }
            // Names and tags are `String`s, and an exit carries its value in
            // the state slots rather than here. So do the stepping flags.
            Frame::Step { .. } | Frame::TracedCall { .. } | Frame::DriveCompiled { .. } => {}
            Frame::Block { .. } | Frame::Catch { .. } | Frame::Return | Frame::ReturnFrom { .. } | Frame::Throw { .. } => {}
            Frame::Args { form, done, env, .. } => {
                heap.push_root(*form);
                heap.push_root(*env);
                for v in done {
                    heap.push_root(*v);
                }
            }
            Frame::Seq { rest, env } => {
                heap.push_root(*rest);
                heap.push_root(*env);
            }
            Frame::LetInit { binds, rest, done, body, env } => {
                heap.push_root(*binds);
                heap.push_root(*rest);
                heap.push_root(*body);
                heap.push_root(*env);
                // Each initialiser's value stays rooted for the rest of them:
                // one bound three initialisers ago is just as collectible as
                // the one that just arrived.
                for v in done {
                    heap.push_root(*v);
                }
            }
            // Nothing to root: these carry only a plain integer, or nothing
            // at all. The value they are waiting on is rooted by the state
            // slots, and the object they act on is reachable from it.
            Frame::Panic | Frame::FieldGet { .. } | Frame::DynValue => {}
        }
        self.frames.push((frame, base, loc));
    }

    /// Removes the top frame. **The caller must `truncate_roots` to the
    /// returned base** — after it has finished reading the frame, and with no
    /// allocation between that truncation and re-rooting the next state.
    fn pop(&mut self) -> Option<(Frame, usize, Option<crate::Loc>)> {
        self.frames.pop()
    }
}

/// One unit of execution: a continuation stack, the state it is in, and the
/// bottom of the roots that belong to it.
///
/// [`Interp::eval_cps`] starts one and runs it straight through. A task
/// (goroutine) is this same structure *kept across suspensions* instead — which
/// is the whole reason the evaluator moved off the Rust stack, since a Rust
/// recursion cannot be stopped between two steps and picked up later.
struct Task {
    /// Where this task's roots begin. The two state slots are `sbase` and
    /// `sbase + 1`; frames root above them. **The starter owns these**: whoever
    /// called [`Task::start`] must `truncate_roots(sbase)` once it is done.
    sbase: usize,
    stack: CpsStack,
    state: State,
    /// The compiled call chain this task is in the middle of, if any.
    ///
    /// **At most one, and empty whenever the task is doing interpreted work.**
    /// A chain is left only by returning or suspending, and while it stands the
    /// task is either driving it or blocked on it — so `step_cps` cannot run
    /// and cannot start a second one — but it can hold **more than one
    /// segment**. A compiled frame that applies an interpreted closure (C5)
    /// runs it on this task's continuation stack, and if that closure calls a
    /// compiled function the chain grows a second run of frames above the
    /// first. `DriveCtx::base` is what keeps the two apart.
    compiled: typelisp_rt::coroutine::FrameStack,
}

/// What one step of a task did.
enum Progress {
    /// Still running. Step it again.
    Running,
    /// Suspended. The task keeps its frames and its roots; the scheduler puts
    /// it back on the queue when what it waits for has happened.
    Blocked(Waiting),
    /// Finished: the continuation stack ran out. The result is rooted in the
    /// state slots until the caller truncates them.
    Done(Result<Value, EvalError>),
}

/// What a task needs from the scheduler before it can go on.
///
/// Not always a *wait*: [`Scheduler::try_now`] answers several of these
/// immediately and the task never leaves the queue. The variants are here
/// rather than being four separate mechanisms because the scheduler is the
/// only thing that can answer any of them, and one shape means the
/// interpreted and the compiled path reach the same implementation — the
/// channel semantics are written once.
///
/// **Not `Copy`**: `Recv` carries the type key its answer is built with.
/// **Not `Eq`**: `Send` carries a `Value`, which is a heap index and has no
/// equality worth deriving.
#[derive(Clone, PartialEq, Debug)]
pub(super) enum Waiting {
    /// Another task's result — `(wait t)`.
    Task(TaskId),
    /// Nothing: the task gave up the rest of its turn — `(yield)`. It goes
    /// straight back onto the queue, behind whatever is already waiting.
    Yield,
    /// The clock — `(sleep secs)`. **This stops the task, not the thread**:
    /// everything else keeps running, and the program only reaches the OS's
    /// `sleep` when nothing at all is ready, for as long as the nearest
    /// deadline. `after` rides on the same mechanism.
    Until(std::time::Instant),
    /// A `Chan<T>` operation.
    Chan(ChanOp),
    /// `select` — any one of several channel operations, whichever can go
    /// first. `has_else` means the task does not wait: with nothing ready it
    /// is answered with the `else` arm instead.
    Select { ops: Vec<SelectOp>, has_else: bool },
}

/// One arm of a `select`, in the order the arms are written — the index *is*
/// the arm, which is what the answer names.
///
/// The values a `Send` arm offers need no root of their own, unlike a plain
/// [`ChanOp::Send`]'s: the checker hoisted every operand into a `let` around
/// the `select`, so each is a live binding in the environment the task's
/// `Frame::SelectArm` is holding.
#[derive(Clone, PartialEq, Debug)]
pub(super) enum SelectOp {
    Recv(ChanId, String),
    Send(ChanId, Value),
}

/// One channel operation, as the task asked for it.
///
/// Four of the six never park — they only need the scheduler's table, which
/// nothing below the front end can hold (`typelisp_abi::call_state::
/// SUSPEND_CHAN_NEW` records why). `Send` and `Recv` are the two that can.
#[derive(Clone, PartialEq, Debug)]
pub(super) enum ChanOp {
    /// `(Chan::new cap)` — answers with the handle.
    New(i64),
    /// `(len ch)` — how many values are buffered.
    Len(ChanId),
    /// `(cap ch)` — how many fit.
    Cap(ChanId),
    /// `(close ch)` — closing a closed channel is a panic, as in Go.
    Close(ChanId),
    /// `(send ch v)` — parks while the buffer is full and no receiver waits.
    ///
    /// The value is rooted through the task's own state slot (`set_state`),
    /// which is what lets a parked sender hold it with nothing else pointing
    /// at it: a `Value` inside a Rust enum is invisible to the collector.
    Send(ChanId, Value),
    /// `(recv ch)` — parks while the buffer is empty, no sender waits and the
    /// channel is open.
    ///
    /// The `String` is the `Option<T>` type key the answer is built with. It
    /// has to travel with the operation because a channel's buffer holds
    /// tagged words and a word does not name its type — the same reason
    /// `vector-op`'s `pop` carries one.
    Recv(ChanId, String),
}

/// Names a channel the scheduler is holding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct ChanId(usize);

/// One channel: a ring of buffered values and whether it is closed.
///
/// The values live in the scheduler's own root stack, `[base, base + cap)`,
/// claimed once at creation and never given back. A channel is therefore as
/// permanent as a finished task's result (`Slot::Done`), and for the same
/// reason: telling the scheduler that the last handle died needs the
/// collector to say so.
///
/// Nothing here records who is waiting. Blocked senders and receivers are
/// found by walking `Scheduler::slots`, which is where `finish` and
/// `wake_due` already look — one place that knows what a parked task is
/// parked on, rather than two that have to agree.
struct Chan {
    cap: usize,
    base: usize,
    head: usize,
    len: usize,
    closed: bool,
}

impl Task {
    /// Starts a task that evaluates `form` in `env`.
    fn start(heap: &mut Heap, form: Value, env: Value) -> Task {
        let sbase = heap.root_count();
        heap.push_root(env);
        heap.push_root(form);
        Task {
            sbase,
            stack: CpsStack::new(),
            state: State::Eval(form, env),
            compiled: typelisp_rt::coroutine::FrameStack::new(),
        }
    }

    /// Starts a task that completes a call whose callee and arguments have
    /// already been evaluated — what `(go (f a b))` hands over.
    fn start_call(heap: &mut Heap, form: Value, argv: Value, kind: ArgsKind) -> Task {
        let sbase = heap.root_count();
        heap.push_root(argv);
        heap.push_root(form);
        Task {
            sbase,
            stack: CpsStack::new(),
            state: State::Enter { form, argv, kind },
            compiled: typelisp_rt::coroutine::FrameStack::new(),
        }
    }
}

/// The runtime value of a `Task<T>`: a boxed struct holding the scheduler's id.
///
/// The key carries no type argument. `Task<i32>` and `Task<string>` are the
/// same thing at run time — no fields to read, and `wait`'s return type is
/// spelled at the call site — so a type argument here would distinguish nothing.
/// That is unlike `Vector<T>`, where the site has to carry its elements'
/// representation because the definition cannot tell you.
fn task_handle(heap: &mut Heap, id: TaskId) -> Value {
    // type-identity-ok: the built-in `Task`, a root name spelled in full
    crate::type_key::alloc_typed_struct(heap, &crate::Path::root("task"), vec![Value::Int(id.0 as i64)])
}

/// The runtime value of a `Chan<T>`: a boxed struct holding the scheduler's id.
///
/// Carries no type argument, for [`task_handle`]'s reason: `Chan<i32>` and
/// `Chan<string>` are the same thing at run time, and the element type is
/// spelled at every site that puts one in or takes one out.
fn chan_handle(heap: &mut Heap, id: ChanId) -> Value {
    // type-identity-ok: the built-in `Chan`, a root name spelled in full
    crate::type_key::alloc_typed_struct(heap, &crate::Path::root("chan"), vec![Value::Int(id.0 as i64)])
}

/// The scheduler id inside a `Chan<T>` handle — [`task_id_of`] for channels.
fn chan_id_of(heap: &Heap, v: Option<Value>) -> Result<ChanId, EvalError> {
    match v {
        Some(Value::Boxed(id)) if heap.struct_field_count(id) == 1 => match heap.struct_field(id, 0) {
            Value::Int(n) if n >= 0 => Ok(ChanId(n as usize)),
            other => Err(EvalError::Internal(format!("a channel handle holds {:?}", other))),
        },
        other => Err(EvalError::Internal(format!("{:?} is not a channel handle", other))),
    }
}

/// What to call a `Waiting` in an error addressed to a programmer.
fn waiting_name(w: &Waiting) -> &'static str {
    match w {
        Waiting::Task(_) => "`wait`",
        Waiting::Yield => "`yield`",
        Waiting::Until(_) => "`sleep`",
        Waiting::Select { .. } => "`select`",
        Waiting::Chan(ChanOp::Send(..)) => "`send`",
        Waiting::Chan(ChanOp::Recv(..)) => "`recv`",
        // The other three answer at once, so they never reach a message that
        // says something could not block.
        Waiting::Chan(_) => "a channel operation",
    }
}

/// The state a task resumes into, given what the scheduler answered it with.
fn answer_state(answer: Result<Value, EvalError>) -> State {
    match answer {
        Ok(v) => State::Apply(v),
        Err(e) => State::Unwind(e),
    }
}

/// Gives a parked task the state it will resume into, **rooting what that
/// state carries in the task's own stack**.
///
/// The root is the point. A value handed to a woken task — a `some` box built
/// by `recv`, say — exists only inside a Rust enum until this runs, and the
/// task is not stepped again until some other task has had its turn and
/// allocated. `Scheduler::finish` does not need this only because a finished
/// task's result is rooted in the scheduler's own stack instead.
fn resume_with(heap: &mut Heap, slot: &mut TaskSlot, state: State) {
    let home = heap.current_root_stack();
    heap.switch_to_root_stack(slot.roots);
    set_state(heap, slot.task.sbase, &state);
    heap.switch_to_root_stack(home);
    slot.task.state = state;
}

/// A task the scheduler owns, and the root stack that belongs to it.
struct TaskSlot {
    task: Task,
    roots: RootStackId,
}

/// Names a task the scheduler is holding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct TaskId(usize);

/// The tasks that exist and the order they run in.
///
/// Cooperative and single-threaded: nothing preempts a task, and one OS thread
/// runs all of them. `ACTIVE_HEAP` has to be a single thread-local and `Heap`
/// is `!Send`, so tasks cannot be spread across threads without making the heap
/// shareable first — a bigger job than the concurrency itself.
///
/// Lives in `Interp` behind a `RefCell`, and **every borrow of it is short**: a
/// task is taken *out* to be stepped, so stepping it — which re-enters the
/// evaluator, and can run compiled code that calls back in — never holds the
/// borrow.
#[derive(Default)]
pub(crate) struct Scheduler {
    /// One entry per task position — see [`Slot`].
    slots: Vec<Slot>,
    /// Ids ready to run, oldest first.
    ready: std::collections::VecDeque<TaskId>,
    /// Where finished tasks' results are rooted: position `i` holds task `i`'s
    /// value. Created on the first `finish`, and never truncated — see
    /// [`Slot::Done`] for why a result outlives its task.
    roots: Option<RootStackId>,
    /// Whether a `drive` loop is on the Rust stack. A *nested* `eval_cps` — a
    /// compiled callee re-entering, or `Interp::apply` — must not switch tasks:
    /// there is a Rust frame waiting on its result, and no continuation stack
    /// underneath it to come back to.
    driving: bool,
    /// How many tasks are `Waiting::Until`. Only a count, so the common case —
    /// nobody sleeping — costs one comparison per task switch instead of a
    /// walk over every slot.
    sleeping: usize,
    /// Every channel that has been made, by [`ChanId`]. See [`Chan`] for why
    /// none is ever removed.
    chans: Vec<Chan>,
    /// Where buffered values are rooted: each channel owns the contiguous
    /// run `[Chan::base, Chan::base + Chan::cap)`. One stack for all of them
    /// rather than one each, because a root stack is a `Heap`'s and a channel
    /// is not worth one.
    chan_roots: Option<RootStackId>,
    /// `select`'s choice among the arms that are ready. See
    /// [`Scheduler::next_random`].
    rng: u64,
    /// Whether to look at another task after **every single step**, rather
    /// than letting the running one keep going until it yields, waits or
    /// finishes.
    ///
    /// Cooperative scheduling means `false`, and nothing in the language can
    /// set it. The tests do: switching every step is the harshest check there
    /// is that a suspended task's frames and roots survive whatever another
    /// task does in between — the same role `gc_stress` plays for a single
    /// task's allocations, and the reason this knob is on the real scheduler
    /// rather than in a test harness of its own.
    switch_every_step: bool,
}

/// One position in the scheduler's table.
///
/// `Running` exists so a position stays **reserved** while its task is out
/// being stepped. Without it, `admit` reuses the position of whichever task is
/// currently running — which is exactly what a `go` inside the main task does,
/// and the new task then inherits main's id and is retired along with it.
enum Slot {
    /// No task here. `admit` may reuse this position.
    Empty,
    /// A task that exists and is not running.
    Parked(TaskSlot),
    /// A task that is taken out to be stepped.
    Running,
    /// A task that cannot run until `Waiting` is satisfied. It goes back to
    /// `Parked` — and onto the queue — when that happens.
    Blocked(TaskSlot, Waiting),
    /// A task that finished, and the value it answered with.
    ///
    /// **Kept for the life of the program**, because `wait` may be asked again
    /// at any time and has to give the same answer. A program that spawns
    /// without bound therefore accumulates one of these per task — the v1
    /// limit. Tying the result's lifetime to its handle's instead needs the
    /// collector to tell the scheduler when a handle dies.
    Done(Value),
}

impl Default for Slot {
    fn default() -> Slot {
        Slot::Empty
    }
}

impl Scheduler {
    /// Adds a task and makes it ready to run.
    fn admit(&mut self, slot: TaskSlot) -> TaskId {
        let id = match self.slots.iter().position(|s| matches!(s, Slot::Empty)) {
            Some(i) => TaskId(i),
            None => {
                self.slots.push(Slot::Empty);
                TaskId(self.slots.len() - 1)
            }
        };
        self.slots[id.0] = Slot::Parked(slot);
        self.ready.push_back(id);
        id
    }

    /// Takes the next ready task *out* to be stepped, reserving its position.
    /// The caller puts it back (`put_back`) or retires it.
    fn next_ready(&mut self) -> Option<(TaskId, TaskSlot)> {
        let id = self.ready.pop_front()?;
        match std::mem::replace(&mut self.slots[id.0], Slot::Running) {
            Slot::Parked(slot) => Some((id, slot)),
            _ => unreachable!("a queued task is parked in its slot"),
        }
    }

    /// Returns a task that is still running to the back of the queue.
    fn put_back(&mut self, id: TaskId, slot: TaskSlot) {
        self.slots[id.0] = Slot::Parked(slot);
        self.ready.push_back(id);
    }

    /// Frees a task's position entirely — for the main task, whose result goes
    /// back to the caller rather than being kept for a `wait`.
    fn retire(&mut self, id: TaskId) {
        self.slots[id.0] = Slot::Empty;
    }

    /// Answers `w` if it can be answered right now, without parking anybody.
    ///
    /// `None` means the asking task has to wait. This is the **whole** of what
    /// each operation means: `Scheduler::block` and the nested-evaluation path
    /// both go through it, so an interpreted `(recv ch)` and a compiled one
    /// are the same code, and so is the one inside a print method.
    ///
    /// Waking *other* tasks happens here too (a receiver taking a parked
    /// sender's value, `close` releasing everyone). That is not a side effect
    /// on the way to an answer: for a rendezvous it *is* the answer.
    fn try_now(&mut self, heap: &mut Heap, w: &Waiting) -> Option<Result<Value, EvalError>> {
        match w {
            // Nothing to wait for, but the task still goes to the back of the
            // queue — answering here would let it run on. That is the whole
            // of `yield`, and `block` is where it happens.
            Waiting::Yield => None,
            // A deadline that has already passed is not a wait at all — which
            // is what makes `(sleep 0.0)` CL's yield-ish zero, with no special
            // case for it anywhere.
            Waiting::Until(t) => (*t <= std::time::Instant::now()).then_some(Ok(Value::Empty)),
            Waiting::Task(on) => match self.slots.get(on.0) {
                Some(Slot::Done(v)) => Some(Ok(*v)),
                Some(Slot::Parked(_)) | Some(Slot::Running) | Some(Slot::Blocked(..)) => None,
                // The handle named a position nothing lives at. Only the main
                // task's position is ever freed, and no `Task<T>` names it.
                Some(Slot::Empty) | None => {
                    Some(Err(EvalError::Internal(format!("wait: {:?} is not a live task", on))))
                }
            },
            Waiting::Chan(op) => self.try_chan(heap, op),
            Waiting::Select { ops, has_else } => self.try_select(heap, ops, *has_else),
        }
    }

    /// [`Self::try_now`] for `select`: the arms that can go right now, one of
    /// them chosen, and that one **performed**.
    ///
    /// Choosing and performing are one step on purpose. Splitting them —
    /// asking which arm is ready and then doing it — would be a race even
    /// here, where nothing runs in between: the reason nothing runs in between
    /// is that this scheduler is cooperative, and a language feature must not
    /// be correct only because of that.
    fn try_select(
        &mut self,
        heap: &mut Heap,
        ops: &[SelectOp],
        has_else: bool,
    ) -> Option<Result<Value, EvalError>> {
        let ready: Vec<usize> =
            (0..ops.len()).filter(|i| self.select_arm_is_ready(&ops[*i])).collect();
        if ready.is_empty() {
            // `else` is the arm after the last channel arm — which is what
            // makes the checker's "`else` must be last" rule worth having.
            return has_else.then(|| self.select_answer(heap, ops.len(), Value::Empty));
        }
        let pick = ready[self.next_random(ready.len())];
        let answer = match &ops[pick] {
            SelectOp::Recv(c, key) => self.chan_recv(heap, *c, key),
            SelectOp::Send(c, v) => self.chan_send(heap, *c, *v),
        };
        match answer {
            Some(Ok(v)) => Some(self.select_answer(heap, pick, v)),
            // A send arm on a closed channel counts as ready and then panics,
            // which is what a plain `(send ch v)` on one does.
            Some(Err(e)) => Some(Err(e)),
            None => Some(Err(EvalError::Internal(
                "select: an arm reported ready and then could not go".to_string(),
            ))),
        }
    }

    /// Whether one arm could go without waiting.
    fn select_arm_is_ready(&self, op: &SelectOp) -> bool {
        match op {
            SelectOp::Recv(c, _) => {
                let ch = &self.chans[c.0];
                ch.len > 0 || ch.closed || self.waiting_sender(*c).is_some()
            }
            SelectOp::Send(c, _) => {
                let ch = &self.chans[c.0];
                ch.closed || ch.len < ch.cap || self.waiting_receiver(*c).is_some()
            }
        }
    }

    /// `(ARM . VALUE)` — what a `select` answers with. The arm index is the
    /// whole of the dispatch, and the value is the receive's `Option<T>` (or
    /// unit for a send or an `else`).
    fn select_answer(&self, heap: &mut Heap, arm: usize, payload: Value) -> Result<Value, EvalError> {
        heap.push_root(payload);
        let out = heap.cons(Value::Int(arm as i64), payload).map_err(heap_err);
        heap.pop_root();
        out
    }

    /// A number in `0..n`.
    ///
    /// Its own xorshift rather than the language's `*random-state*`: the
    /// scheduler sits below that, and what `select` needs from randomness is
    /// only that a program cannot depend on which of several ready arms wins
    /// (Go's reason — taking them in written order starves the later ones).
    /// The seed is fixed, so a run is reproducible, which is worth more here
    /// than being unpredictable.
    fn next_random(&mut self, n: usize) -> usize {
        if self.rng == 0 {
            // `Scheduler` is `Default`, and xorshift stays at zero forever.
            self.rng = 0x2545_F491_4F6C_DD1D;
        }
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        (x % n as u64) as usize
    }

    /// [`Self::try_now`] for the channel operations.
    fn try_chan(&mut self, heap: &mut Heap, op: &ChanOp) -> Option<Result<Value, EvalError>> {
        match op {
            ChanOp::New(cap) => {
                if *cap < 0 {
                    return Some(Err(EvalError::Panic(format!(
                        "Chan::new: {} is not a capacity a channel can have",
                        cap
                    ))));
                }
                let id = self.chan_new(heap, *cap as usize);
                Some(Ok(chan_handle(heap, id)))
            }
            ChanOp::Len(c) => Some(Ok(Value::Int(self.chans[c.0].len as i64))),
            ChanOp::Cap(c) => Some(Ok(Value::Int(self.chans[c.0].cap as i64))),
            ChanOp::Close(c) => Some(self.chan_close(heap, *c).map(|()| Value::Empty)),
            ChanOp::Send(c, v) => self.chan_send(heap, *c, *v),
            ChanOp::Recv(c, key) => self.chan_recv(heap, *c, key),
        }
    }

    /// Suspends `id` until `w` is satisfied — or puts it straight back on the
    /// queue if it already is.
    fn block(&mut self, heap: &mut Heap, id: TaskId, mut slot: TaskSlot, w: Waiting) -> Result<(), EvalError> {
        match self.try_now(heap, &w) {
            Some(answer) => {
                resume_with(heap, &mut slot, answer_state(answer));
                self.slots[id.0] = Slot::Parked(slot);
                self.ready.push_back(id);
                Ok(())
            }
            None => {
                match w {
                    // Straight back onto the queue, behind everything already
                    // on it.
                    Waiting::Yield => {
                        resume_with(heap, &mut slot, State::Apply(Value::Empty));
                        self.slots[id.0] = Slot::Parked(slot);
                        self.ready.push_back(id);
                    }
                    Waiting::Until(_) => {
                        self.sleeping += 1;
                        self.slots[id.0] = Slot::Blocked(slot, w);
                    }
                    Waiting::Task(_) | Waiting::Chan(_) | Waiting::Select { .. } => {
                        self.slots[id.0] = Slot::Blocked(slot, w);
                    }
                }
                Ok(())
            }
        }
    }

    /// Makes a channel with room for `cap` values, claiming its root slots.
    fn chan_new(&mut self, heap: &mut Heap, cap: usize) -> ChanId {
        let stack = *self.chan_roots.get_or_insert_with(|| heap.new_root_stack());
        let home = heap.current_root_stack();
        heap.switch_to_root_stack(stack);
        let base = heap.root_count();
        for _ in 0..cap {
            heap.push_root(Value::Empty);
        }
        heap.switch_to_root_stack(home);
        self.chans.push(Chan { cap, base, head: 0, len: 0, closed: false });
        ChanId(self.chans.len() - 1)
    }

    /// Appends to a channel's ring. The caller has checked there is room.
    fn chan_push(&mut self, heap: &mut Heap, c: ChanId, v: Value) {
        let ch = &mut self.chans[c.0];
        let idx = ch.base + (ch.head + ch.len) % ch.cap;
        ch.len += 1;
        let stack = self.chan_roots.expect("a channel exists, so its root stack does");
        let home = heap.current_root_stack();
        heap.switch_to_root_stack(stack);
        heap.set_root(idx, v);
        heap.switch_to_root_stack(home);
    }

    /// Takes the oldest value out of a channel's ring, or `None` if empty.
    fn chan_pop(&mut self, heap: &mut Heap, c: ChanId) -> Option<Value> {
        let ch = &mut self.chans[c.0];
        if ch.len == 0 {
            return None;
        }
        let idx = ch.base + ch.head;
        ch.head = (ch.head + 1) % ch.cap;
        ch.len -= 1;
        let stack = self.chan_roots.expect("a channel exists, so its root stack does");
        let home = heap.current_root_stack();
        heap.switch_to_root_stack(stack);
        let v = heap.root(idx);
        // The ring slot is a root: leaving the value in it would keep the
        // whole graph under it alive for as long as the channel exists.
        heap.set_root(idx, Value::Empty);
        heap.switch_to_root_stack(home);
        Some(v)
    }

    /// A task parked in a way that can take a value from `c`: which slot it
    /// is in, which `select` arm to report (`None` for a plain `recv`), and
    /// the key its answer is built with.
    ///
    /// Slot order, not arrival order. Go's channels hand out in arrival order;
    /// nothing in this scheduler records arrival, and the alternative is a
    /// second queue per channel that has to agree with `slots` about who is
    /// parked — the disagreement that `finish`/`wake_due` avoid by walking.
    fn waiting_receiver(&self, c: ChanId) -> Option<(usize, Option<usize>, String)> {
        self.slots.iter().enumerate().find_map(|(i, s)| match s {
            Slot::Blocked(_, Waiting::Chan(ChanOp::Recv(on, key))) if *on == c => {
                Some((i, None, key.clone()))
            }
            Slot::Blocked(_, Waiting::Select { ops, .. }) => {
                ops.iter().enumerate().find_map(|(a, op)| match op {
                    SelectOp::Recv(on, key) if *on == c => Some((i, Some(a), key.clone())),
                    _ => None,
                })
            }
            _ => None,
        })
    }

    /// The same for a task that can hand a value *to* `c`, and the value it is
    /// offering.
    fn waiting_sender(&self, c: ChanId) -> Option<(usize, Option<usize>, Value)> {
        self.slots.iter().enumerate().find_map(|(i, s)| match s {
            Slot::Blocked(_, Waiting::Chan(ChanOp::Send(on, v))) if *on == c => Some((i, None, *v)),
            Slot::Blocked(_, Waiting::Select { ops, .. }) => {
                ops.iter().enumerate().find_map(|(a, op)| match op {
                    SelectOp::Send(on, v) if *on == c => Some((i, Some(a), *v)),
                    _ => None,
                })
            }
            _ => None,
        })
    }

    /// Puts a parked task back on the queue with the state it resumes into.
    fn wake(&mut self, heap: &mut Heap, i: usize, state: State) {
        let Slot::Blocked(mut slot, _) = std::mem::replace(&mut self.slots[i], Slot::Running) else {
            unreachable!("only a blocked task is woken");
        };
        resume_with(heap, &mut slot, state);
        self.slots[i] = Slot::Parked(slot);
        self.ready.push_back(TaskId(i));
    }

    /// Wakes a receiver with `v` (or the end of a closed channel), answering
    /// in whichever shape it was waiting in.
    fn wake_receiver(&mut self, heap: &mut Heap, i: usize, arm: Option<usize>, key: &str, v: Option<Value>) {
        let payload = option_value(heap, key, v);
        let answer = match arm {
            Some(a) => match self.select_answer(heap, a, payload) {
                Ok(v) => v,
                Err(_) => payload,
            },
            None => payload,
        };
        self.wake(heap, i, State::Apply(answer));
    }

    /// Wakes a sender whose value has been taken.
    fn wake_sender(&mut self, heap: &mut Heap, i: usize, arm: Option<usize>) {
        let answer = match arm {
            Some(a) => self.select_answer(heap, a, Value::Empty).unwrap_or(Value::Empty),
            None => Value::Empty,
        };
        self.wake(heap, i, State::Apply(answer));
    }

    /// `(send ch v)`. `None` means the sender has to wait for room.
    fn chan_send(&mut self, heap: &mut Heap, c: ChanId, v: Value) -> Option<Result<Value, EvalError>> {
        if self.chans[c.0].closed {
            return Some(Err(EvalError::Panic("send: the channel is closed".to_string())));
        }
        // A waiting receiver takes it directly, buffer or no buffer. That is
        // the whole of a rendezvous, and for a buffered channel it cannot be
        // wrong: nobody waits to receive from a channel with anything in it.
        if let Some((i, arm, key)) = self.waiting_receiver(c) {
            self.wake_receiver(heap, i, arm, &key, Some(v));
            return Some(Ok(Value::Empty));
        }
        if self.chans[c.0].len < self.chans[c.0].cap {
            self.chan_push(heap, c, v);
            return Some(Ok(Value::Empty));
        }
        None
    }

    /// `(recv ch)`. `None` means the receiver has to wait for a value.
    fn chan_recv(&mut self, heap: &mut Heap, c: ChanId, key: &str) -> Option<Result<Value, EvalError>> {
        if let Some(v) = self.chan_pop(heap, c) {
            // Taking one out made room, so a sender that was waiting for room
            // can put its value in and go.
            if let Some((i, arm, offered)) = self.waiting_sender(c) {
                self.chan_push(heap, c, offered);
                self.wake_sender(heap, i, arm);
            }
            return Some(Ok(option_value(heap, key, Some(v))));
        }
        // Nothing buffered — which for an unbuffered channel is always, so
        // this is the rendezvous seen from the receiving side.
        if let Some((i, arm, offered)) = self.waiting_sender(c) {
            self.wake_sender(heap, i, arm);
            return Some(Ok(option_value(heap, key, Some(offered))));
        }
        if self.chans[c.0].closed {
            return Some(Ok(option_value(heap, key, None)));
        }
        None
    }

    /// `(close ch)`. Everyone parked on it is released.
    fn chan_close(&mut self, heap: &mut Heap, c: ChanId) -> Result<(), EvalError> {
        if self.chans[c.0].closed {
            return Err(EvalError::Panic("close: the channel is already closed".to_string()));
        }
        self.chans[c.0].closed = true;
        // Receivers get `none`: a receiver only parks with the buffer empty
        // and no sender waiting, so there is nothing left for them.
        while let Some((i, arm, key)) = self.waiting_receiver(c) {
            self.wake_receiver(heap, i, arm, &key, None);
        }
        // Senders panic, which is Go's rule. The value each was offering is
        // dropped with the task's state, and its root goes with it.
        while let Some((i, _, _)) = self.waiting_sender(c) {
            self.wake(
                heap,
                i,
                State::Unwind(EvalError::Panic("send: the channel was closed while waiting".to_string())),
            );
        }
        Ok(())
    }

    /// Records `id`'s result and wakes everything that was waiting for it.
    ///
    /// The value is rooted in the scheduler's own stack first: it has outlived
    /// the task's stack, and the tasks being woken will not touch it until they
    /// are stepped.
    fn finish(&mut self, heap: &mut Heap, id: TaskId, v: Value) {
        let sched = *self.roots.get_or_insert_with(|| heap.new_root_stack());
        let home = heap.current_root_stack();
        heap.switch_to_root_stack(sched);
        while heap.root_count() <= id.0 {
            heap.push_root(Value::Empty);
        }
        heap.set_root(id.0, v);
        heap.switch_to_root_stack(home);

        self.slots[id.0] = Slot::Done(v);
        for i in 0..self.slots.len() {
            if !matches!(&self.slots[i], Slot::Blocked(_, Waiting::Task(on)) if *on == id) {
                continue;
            }
            if let Slot::Blocked(mut slot, _) = std::mem::replace(&mut self.slots[i], Slot::Running) {
                slot.task.state = State::Apply(v);
                self.slots[i] = Slot::Parked(slot);
                self.ready.push_back(TaskId(i));
            }
        }
    }

    /// Puts every sleeping task whose deadline has passed back on the queue.
    ///
    /// Called before each task switch rather than only when nothing is ready:
    /// a task that asked for 10ms while another runs for a second should be
    /// runnable again after 10ms, not after the second.
    fn wake_due(&mut self) {
        if self.sleeping == 0 {
            return;
        }
        let now = std::time::Instant::now();
        for i in 0..self.slots.len() {
            if !matches!(&self.slots[i], Slot::Blocked(_, Waiting::Until(t)) if *t <= now) {
                continue;
            }
            if let Slot::Blocked(mut slot, _) = std::mem::replace(&mut self.slots[i], Slot::Running) {
                slot.task.state = State::Apply(Value::Empty);
                self.slots[i] = Slot::Parked(slot);
                self.ready.push_back(TaskId(i));
                self.sleeping -= 1;
            }
        }
    }

    /// The nearest deadline any task is sleeping until.
    ///
    /// `None` means nobody is: with nothing ready either, every remaining task
    /// is waiting on something that will never happen.
    fn earliest_deadline(&self) -> Option<std::time::Instant> {
        if self.sleeping == 0 {
            return None;
        }
        self.slots
            .iter()
            .filter_map(|s| match s {
                Slot::Blocked(_, Waiting::Until(t)) => Some(*t),
                _ => None,
            })
            .min()
    }

    /// The value a finished task answered with, for a caller that drove the
    /// tasks itself rather than through `wait`.
    #[cfg(test)]
    fn done_value(&self, id: TaskId) -> Option<Value> {
        match self.slots.get(id.0) {
            Some(Slot::Done(v)) => Some(*v),
            _ => None,
        }
    }
}

/// The scheduler id inside a `Task<T>` handle.
///
/// The handle is a boxed struct with the id in field 0 and nothing else — see
/// `task_handle`. Nothing in the language can build one, so a value of the
/// wrong shape here means the evaluator built it wrong.
fn task_id_of(heap: &Heap, v: Option<Value>) -> Result<TaskId, EvalError> {
    match v {
        Some(Value::Boxed(id)) if heap.struct_field_count(id) == 1 => {
            match heap.struct_field(id, 0) {
                Value::Int(n) if n >= 0 => Ok(TaskId(n as usize)),
                other => Err(EvalError::Internal(format!("wait: a task handle holds {:?}", other))),
            }
        }
        other => Err(EvalError::Internal(format!("wait: {:?} is not a task handle", other))),
    }
}

/// What a task's failure does to the program.
///
/// A `throw` that leaves a task has no catch to reach — a tag does not cross a
/// task boundary — and a panic is not recoverable by definition. Either way the
/// program stops, which is Go's rule for an unrecovered panic in a goroutine.
fn escaped_task_failure(e: EvalError) -> EvalError {
    match e {
        EvalError::Throw(tag, _) => EvalError::Panic(format!("`throw` of `{}` left its task", tag)),
        other => other,
    }
}

/// Places `e` at `loc`, if there is one. `EvalError::at` keeps the innermost
/// location, so calling this more than once on the way out is harmless.
fn place(e: EvalError, loc: Option<crate::Loc>) -> EvalError {
    match loc {
        Some(l) => e.at(l),
        None => e,
    }
}

/// Points the two state slots at whatever `state` carries.
///
/// Must not allocate: it runs in the window right after a `truncate_roots`.
/// What a compiled frame that just returned `STATUS_SUSPEND` is waiting for,
/// and the representation its answer will come back through.
///
/// The compiled side publishes two raw words (`typelisp_abi::call_state`),
/// because the crate it publishes from sits below the one that owns `Waiting`
/// — a `TaskId` and an `Instant` are the scheduler's types. This is the one
/// place that turns them back.
fn pending_wait(heap: &Heap) -> Result<(Waiting, Repr), EvalError> {
    use typelisp_abi::call_state as cs;
    let (kind, payload, second) = cs::take_pending_suspend().ok_or_else(|| {
        EvalError::Internal(
            "a compiled frame suspended without recording what it was waiting for".to_string(),
        )
    })?;
    match kind {
        // Nothing comes back: the wake value is unit, and the resume block
        // does not read the frame's value slot at all (`(suspend ...)` carries
        // kind `0` for these).
        cs::SUSPEND_YIELD => Ok((Waiting::Yield, Repr::Unit)),
        // A compiled loop's back edge offering a turn (C7). Indistinguishable
        // from `(yield)` once a scheduler is the one answering — the two are
        // separate kinds so that a driver *without* a scheduler can tell an
        // offer from a request.
        cs::SUSPEND_SAFEPOINT => Ok((Waiting::Yield, Repr::Unit)),
        cs::SUSPEND_SLEEP => {
            // The same refusals `Interp::sleep_until` makes, in the same
            // words: this is the same `sleep`, and a program must not be able
            // to tell which side it ran on.
            let secs = f64::from_bits(payload as u64);
            if !(secs >= 0.0) {
                return Err(EvalError::Panic(format!(
                    "sleep: {} is not a non-negative number of seconds",
                    secs
                )));
            }
            let d = std::time::Duration::try_from_secs_f64(secs)
                .map_err(|_| EvalError::Panic(format!("sleep: {} is longer than this can wait", secs)))?;
            Ok((Waiting::Until(std::time::Instant::now() + d), Repr::Unit))
        }
        // The payload is the `Task<T>` handle, still tagged — reading the id
        // out of the box is this side's job, because `TaskId` is the
        // scheduler's type. The answer comes back as a tagged `Sexpr` whatever
        // `T` is, and the resume block decodes it with the kind the bridge
        // baked in: the same division `catch`/`throw` make for a thrown value.
        cs::SUSPEND_WAIT => {
            let handle = typelisp_abi::decode(payload);
            Ok((Waiting::Task(task_id_of(heap, Some(handle))?), Repr::Sexpr))
        }
        // The channel operations. The handle is tagged, like `wait`'s; so is
        // `send`'s value, which the suspension site tagged per the element's
        // own `Repr::field_kind` because the driver has only a word.
        // The capacity is an ordinary `i32` argument, so it crosses raw — the
        // suspension site tags only what the driver could not otherwise read.
        cs::SUSPEND_CHAN_NEW => Ok((Waiting::Chan(ChanOp::New(payload)), Repr::Sexpr)),
        // `Repr::Sexpr` and not `Repr::Int`: a wake value always crosses back
        // **tagged**, whatever its type, and the resume block decodes it with
        // the kind the bridge baked in — the same division `wait` makes.
        cs::SUSPEND_CHAN_LEN => {
            Ok((Waiting::Chan(ChanOp::Len(chan_id_of(heap, Some(typelisp_abi::decode(payload)))?)), Repr::Sexpr))
        }
        cs::SUSPEND_CHAN_CAP => {
            Ok((Waiting::Chan(ChanOp::Cap(chan_id_of(heap, Some(typelisp_abi::decode(payload)))?)), Repr::Sexpr))
        }
        cs::SUSPEND_CHAN_CLOSE => {
            Ok((Waiting::Chan(ChanOp::Close(chan_id_of(heap, Some(typelisp_abi::decode(payload)))?)), Repr::Unit))
        }
        cs::SUSPEND_CHAN_SEND => {
            let c = chan_id_of(heap, Some(typelisp_abi::decode(payload)))?;
            Ok((Waiting::Chan(ChanOp::Send(c, typelisp_abi::decode(second))), Repr::Unit))
        }
        cs::SUSPEND_CHAN_RECV => {
            let c = chan_id_of(heap, Some(typelisp_abi::decode(payload)))?;
            let key = match typelisp_abi::decode(second) {
                Value::Str(id) => heap.string(id).to_string(),
                other => {
                    return Err(EvalError::Internal(format!(
                        "a compiled `recv` carried {:?} where its `Option<T>` key should be",
                        other
                    )))
                }
            };
            Ok((Waiting::Chan(ChanOp::Recv(c, key)), Repr::Sexpr))
        }
        // The arms travel in their own slot, and every word in them is
        // tagged — the descriptor was built in a compiled frame's slots, and
        // the collector walks those.
        cs::SUSPEND_CHAN_SELECT => {
            let words = cs::take_pending_select();
            let untag = |w: i64| w >> 3;
            if words.len() < 2 {
                return Err(EvalError::Internal("a compiled `select` carried no arms".to_string()));
            }
            let n = untag(words[0]).max(0) as usize;
            let has_else = untag(words[1]) != 0;
            if words.len() < 2 + 3 * n {
                return Err(EvalError::Internal(format!(
                    "a compiled `select` said it had {} arms and carried {} words",
                    n,
                    words.len()
                )));
            }
            let mut ops = Vec::with_capacity(n);
            for i in 0..n {
                let base = 2 + 3 * i;
                let c = chan_id_of(heap, Some(typelisp_abi::decode(words[base + 1])))?;
                let extra = typelisp_abi::decode(words[base + 2]);
                ops.push(if untag(words[base]) == 0 {
                    match extra {
                        Value::Str(id) => SelectOp::Recv(c, heap.string(id).to_string()),
                        other => {
                            return Err(EvalError::Internal(format!(
                                "a compiled `select` receive arm carried {:?} where its key should be",
                                other
                            )))
                        }
                    }
                } else {
                    SelectOp::Send(c, extra)
                });
            }
            Ok((Waiting::Select { ops, has_else }, Repr::Sexpr))
        }
        other => Err(EvalError::Internal(format!(
            "a compiled frame asked to wait on kind {} (payload {}), which this build does not lower",
            other, payload
        ))),
    }
}

fn set_state(heap: &mut Heap, sbase: usize, state: &State) {
    match state {
        State::Eval(form, env) => {
            heap.set_root(sbase, *env);
            heap.set_root(sbase + 1, *form);
        }
        State::Enter { form, argv, .. } => {
            heap.set_root(sbase, *argv);
            heap.set_root(sbase + 1, *form);
        }
        // A blocked task carries nothing in the state slots. What its frames
        // hold is rooted by the frames, in this task's own stack, which stays
        // exactly as it is while the task waits.
        //
        // A compiled drive is the same answer for the same reason, with one
        // addition: the wake value on its way *into* the chain does need a
        // root, because it is a Rust local until the frame slot is written.
        // A blocked task carries nothing in the state slots, with one
        // exception: a parked `send` is holding the value it is offering, and
        // nothing else points at it.
        //
        // A parked `select` is **not** a second exception, even though its
        // send arms hold values too: every operand of a `select` is a binding
        // of the `let` the checker wrapped around it, so the environment the
        // task's `Frame::SelectArm` roots already holds them all.
        State::Blocked(Waiting::Chan(ChanOp::Send(_, v))) => {
            heap.set_root(sbase, *v);
            heap.set_root(sbase + 1, Value::Empty);
        }
        State::Blocked(_) => {
            heap.set_root(sbase, Value::Empty);
            heap.set_root(sbase + 1, Value::Empty);
        }
        // The arguments are still values here, and they are the reason this
        // state exists at all: they have to outlive the truncate that releases
        // the frame which named the call, and a `Vec<Value>` in a Rust local
        // is invisible to the collector.
        State::CompiledEnter { argv, .. } => {
            heap.set_root(sbase, *argv);
            heap.set_root(sbase + 1, Value::Empty);
        }
        // Nothing in the state slots: the exit is parked on the compiled
        // side, and the value a throw carries is rooted there
        // (`Heap::set_in_flight_throw`) for exactly this flight.
        State::CompiledRaise { .. } => {
            heap.set_root(sbase, Value::Empty);
            heap.set_root(sbase + 1, Value::Empty);
        }
        State::CompiledResume { wake, .. } => {
            heap.set_root(sbase, wake.as_ref().map(|(v, _)| *v).unwrap_or(Value::Empty));
            heap.set_root(sbase + 1, Value::Empty);
        }
        State::Apply(v) => {
            heap.set_root(sbase, *v);
            heap.set_root(sbase + 1, Value::Empty);
        }
        // The exit's value keeps its root for the whole flight: a cleanup on
        // the way out allocates, and every scope that rooted it is being
        // discarded. `Break` and a plain `Return` with no value carry nothing.
        State::Unwind(e) => {
            let carried = match e {
                EvalError::Return(v) | EvalError::ReturnFrom(_, v) | EvalError::Throw(_, v) => **v,
                _ => Value::Empty,
            };
            heap.set_root(sbase, carried);
            heap.set_root(sbase + 1, Value::Empty);
        }
    }
}

/// The state that evaluates `forms` (a cons list) in order, with the value
/// of the last one as the result.
///
/// An empty sequence is unit, exactly as `(let () )` evaluates. A single form
/// is a *tail jump*: no frame, so a body's last form costs no stack.
fn sequence_state(heap: &Heap, forms: Value, env: Value) -> Result<(State, Option<Frame>), EvalError> {
    if matches!(forms, Value::Empty) {
        return Ok((State::Apply(Value::Empty), None));
    }
    let first = heap
        .car(forms)
        .map_err(|e| EvalError::Internal(format!("eval: body form: {}", e)))?;
    let rest = heap
        .cdr(forms)
        .map_err(|e| EvalError::Internal(format!("eval: body rest: {}", e)))?;
    let frame = if matches!(rest, Value::Empty) {
        None
    } else {
        Some(Frame::Seq { rest, env })
    };
    Ok((State::Eval(first, env), frame))
}

/// The argument forms of a node whose arguments start at a fixed offset.
fn arg_forms(heap: &Heap, form: Value, kind: ArgsKind) -> Result<Vec<Value>, EvalError> {
    let fields =
        core::fields(heap, form).map_err(|e| EvalError::Internal(format!("eval: ({} ..): {}", kind.what(), e)))?;
    let skip = kind.skip();
    if fields.len() < skip {
        return Err(EvalError::Internal(format!(
            "eval: malformed {}: {}",
            kind.what(),
            core::print(heap, form)
        )));
    }
    Ok(fields[skip..].to_vec())
}

/// A `let` binding's name and initialiser.
///
/// A binding is `(SYM R E)` — a plain three-element list, not a tagged node,
/// so its elements are read positionally (`core::field` would skip the first
/// as a tag).
fn bind_parts(heap: &Heap, b: Value) -> Result<(SymRef, Value), EvalError> {
    let bad = |what: &str| EvalError::Internal(format!("eval: (let ..) binding {}", what));
    let Ok(Value::Symbol(sym)) = heap.car(b) else {
        return Err(bad("name is not a symbol"));
    };
    let after_name = heap.cdr(b).map_err(|_| bad("has no representation"))?;
    let after_repr = heap.cdr(after_name).map_err(|_| bad("has no initialiser"))?;
    let init = heap.car(after_repr).map_err(|_| bad("has no initialiser"))?;
    Ok((sym, init))
}

impl Interp {
    /// Evaluate `form` in `env` with no Rust recursion.
    ///
    /// What [`Interp::eval_core`](super::Interp::eval_core) is: the depth of a
    /// Lisp computation costs continuation frames on the heap-side stack, not
    /// Rust frames. Every `Op` is handled here — `step_cps` matches without a
    /// catch-all, so a tag with no frame fails to build.
    pub(crate) fn eval_cps(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        if self.scheduler.borrow().driving {
            // Nested: a Rust frame is waiting on this call. `Interp::apply`
            // and the public API, the `eval` builtin, a `print-object`
            // method, a reader macro — and a compiled callee reached through
            // a driver that is itself standing on a machine frame
            // (`FrameStack::run_to_end`). This evaluation cannot be
            // suspended, because there is no continuation stack under that
            // frame to come back to, so it runs straight through on the
            // caller's own root stack.
            return self.run_to_completion(heap, form, env);
        }

        // The outermost evaluation is the *main task*. Tasks `go` starts
        // outlive it: this returns as soon as main is done, and whatever is
        // left keeps its state for the next time the scheduler runs — which is
        // what makes `(go ...)` at a REPL prompt behave.
        let home = heap.current_root_stack();
        let roots = heap.new_root_stack();
        heap.switch_to_root_stack(roots);
        let task = Task::start(heap, form, env);
        heap.switch_to_root_stack(home);
        let main = self.scheduler.borrow_mut().admit(TaskSlot { task, roots });

        self.scheduler.borrow_mut().driving = true;
        let out = self.drive(heap, main);
        self.scheduler.borrow_mut().driving = false;
        out
    }

    /// Runs one evaluation to its end on the caller's root stack, with no
    /// scheduling. What a nested `eval_cps` does.
    fn run_to_completion(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        let mut task = Task::start(heap, form, env);
        let out = loop {
            match self.step_task(heap, &mut task) {
                Progress::Running => {}
                Progress::Done(r) => break r,
                // There is a Rust frame waiting on this evaluation, so there is
                // nothing to switch *to*: suspending would strand it. Refused
                // rather than deadlocked — see the plan's B6.
                //
                // **Not the compiled boundary any more.** C5 moved a compiled
                // `apply` and `:dyn` call onto the task's own continuation
                // stack, so what is left here is the callers that genuinely
                // hold a Rust frame: `Interp::apply`, `eval`, a print method,
                // a reader macro, and a driver that entered from one of those.
                //
                // Asking first is not a softening of that: several of these
                // are not waits at all (`(recv ch)` with something buffered,
                // `(wait t)` on a task that has finished), and refusing them
                // here would mean a channel could not be read from a print
                // method even when reading it stops for nothing.
                Progress::Blocked(w) => {
                    let answered = self.scheduler.borrow_mut().try_now(heap, &w);
                    match answered {
                        Some(answer) => {
                            let next = answer_state(answer);
                            set_state(heap, task.sbase, &next);
                            task.state = next;
                        }
                        None => {
                            break Err(EvalError::Panic(format!(
                                "{} cannot block: it was reached from a Rust caller, which has no \
                                 continuation stack to suspend",
                                waiting_name(&w)
                            )))
                        }
                    }
                }
            }
        };
        heap.truncate_roots(task.sbase);
        out
    }

    /// Runs ready tasks until `main` finishes, and returns its result.
    ///
    /// The result is *not* rooted here, exactly as an ordinary evaluation's is
    /// not: the caller has it in hand and roots it if it keeps it.
    fn drive(&self, heap: &mut Heap, main: TaskId) -> Result<Value, EvalError> {
        let home = heap.current_root_stack();
        loop {
            let (id, mut slot) = loop {
                self.scheduler.borrow_mut().wake_due();
                let taken = self.scheduler.borrow_mut().next_ready();
                if let Some(t) = taken {
                    break t;
                }
                // Nothing can run. If a task is sleeping, what it waits for is
                // the clock — so **this** is the one place the program reaches
                // the OS's `sleep`, and only for as long as the nearest
                // deadline. That is the difference between stopping a task and
                // stopping the thread.
                let Some(deadline) = self.scheduler.borrow().earliest_deadline() else {
                    // `main` has not finished and nothing can run: every
                    // remaining task is waiting on something that will never
                    // happen.
                    return Err(EvalError::Internal(
                        "scheduler: every task is blocked and none can proceed".to_string(),
                    ));
                };
                let now = std::time::Instant::now();
                if deadline > now {
                    std::thread::sleep(deadline - now);
                }
            };
            heap.switch_to_root_stack(slot.roots);
            let every_step = self.scheduler.borrow().switch_every_step;
            let outcome = loop {
                match self.step_task(heap, &mut slot.task) {
                    Progress::Running if every_step => break Progress::Running,
                    Progress::Running => {}
                    done => break done,
                }
            };
            match outcome {
                Progress::Running => {
                    heap.switch_to_root_stack(home);
                    self.scheduler.borrow_mut().put_back(id, slot);
                }
                Progress::Blocked(w) => {
                    heap.switch_to_root_stack(home);
                    self.scheduler.borrow_mut().block(heap, id, slot, w)?;
                }
                Progress::Done(r) => {
                    heap.truncate_roots(slot.task.sbase);
                    heap.switch_to_root_stack(home);
                    heap.drop_root_stack(slot.roots);
                    if id == main {
                        // The caller takes this result, so nothing keeps it —
                        // main's position is freed rather than kept for a
                        // `wait`, and no `Task<T>` names it.
                        self.scheduler.borrow_mut().retire(id);
                        return r;
                    }
                    match r {
                        Ok(v) => self.scheduler.borrow_mut().finish(heap, id, v),
                        Err(e) => return Err(escaped_task_failure(e)),
                    }
                }
            }
        }
    }

    /// Runs `task` one step.
    ///
    /// A step is one `Eval` decomposition, one frame resumed, or one frame
    /// unwound through. Between two calls the task holds everything it needs to
    /// continue, so a scheduler may leave it alone and step a different one —
    /// that is what makes a task suspendable.
    fn step_task(&self, heap: &mut Heap, task: &mut Task) -> Progress {
        // Taken out to be consumed; every path below either puts the next state
        // back or returns `Done`, in which case what is left here is never read.
        let state = std::mem::replace(&mut task.state, State::Apply(Value::Empty));
        let next = match state {
            State::Eval(form, env) => {
                // Read the location before stepping: it names the form the
                // error came from, and `EvalError::at` keeps the innermost
                // one, so a subexpression that already placed the error
                // wins over the form containing it.
                let loc = heap.cons_loc(form);
                match self.step_cps(heap, &mut task.stack, form, env) {
                    Ok(next) => next,
                    Err(e) => State::Unwind(place(e, loc)),
                }
            }
            // A task `go` started begins here: the call's parts are in hand,
            // so there is nothing to evaluate before making it.
            State::Enter { form, argv, kind } => {
                let loc = heap.cons_loc(form);
                match heap.list_to_vec(argv).map_err(heap_err).and_then(|argv| {
                    self.finish_args(heap, form, argv, kind, false)
                }) {
                    Ok((next, pushed)) => {
                        if let Some(f) = pushed {
                            task.stack.push(heap, f, loc);
                        }
                        next
                    }
                    Err(e) => State::Unwind(place(e, loc)),
                }
            }
            // Only the scheduler moves a task out of this, by replacing the
            // state with the value it was waiting for.
            State::Blocked(w) => {
                task.state = State::Blocked(w.clone());
                return Progress::Blocked(w);
            }
            State::CompiledEnter { argv, start } => {
                // Marshaled *here* rather than where the call was named: the
                // roots this pushes must outlive nothing and survive nothing,
                // but they must be pushed after the truncate that released the
                // naming frame's roots. See `DriveStart`.
                match heap.list_to_vec(argv).map_err(heap_err).and_then(|argv| {
                    let (args, crossing_roots, roots_on_entry) =
                        self.begin_compiled(heap, &argv, &start.params)?;
                    Ok((args, roots_on_entry, crossing_roots, start))
                }) {
                    Ok((args, roots_on_entry, crossing_roots, start)) => {
                        let drive = DriveCtx { start, roots_on_entry, crossing_roots, base: task.compiled.depth() };
                        // SAFETY: the address is a symbol the backend resolved
                        // out of a module that defines it, declared under
                        // `coroutine_fn_type` — the same provenance every
                        // indirect compiled call relies on, and `body_abi` is
                        // what said it was this one.
                        let f: typelisp_rt::coroutine::CoroutineFn = unsafe {
                            std::mem::transmute::<usize, typelisp_rt::coroutine::CoroutineFn>(
                                drive.start.body.address(),
                            )
                        };
                        let outcome = {
                            let stack = &mut task.compiled;
                            crate::eval::crossing::catch_compiled_panic(|| stack.run(heap, f, &args))
                        };
                        self.after_drive(heap, task, outcome, drive)
                    }
                    Err(e) => State::Unwind(e),
                }
            }
            State::CompiledResume { drive, wake } => {
                let delivered = match wake {
                    Some((v, repr)) => self.deliver_wake(heap, task, v, &repr),
                    None => Ok(()),
                };
                match delivered {
                    Ok(()) => {
                        let base = drive.base;
                        let outcome = {
                            let stack = &mut task.compiled;
                            crate::eval::crossing::catch_compiled_panic(|| stack.resume(heap, base))
                        };
                        self.after_drive(heap, task, outcome, drive)
                    }
                    Err(e) => {
                        heap.truncate_roots(drive.roots_on_entry);
                        State::Unwind(e)
                    }
                }
            }
            State::CompiledRaise { drive } => {
                let base = drive.base;
                let outcome = {
                    let stack = &mut task.compiled;
                    crate::eval::crossing::catch_compiled_panic(|| stack.raise(heap, base))
                };
                self.after_drive(heap, task, outcome, drive)
            }
            State::Unwind(e) => match task.stack.pop() {
                None => return Progress::Done(Err(e)),
                Some((frame, fbase, loc)) => {
                    let (next, pushed) = match self.unwind_through(heap, frame, e) {
                        Ok(pair) => pair,
                        // An error *while* unwinding replaces the exit in
                        // flight — the same rule a cleanup's own exit follows.
                        Err(e) => (State::Unwind(place(e, loc.clone())), None),
                    };
                    heap.truncate_roots(fbase);
                    if let Some(f) = pushed {
                        task.stack.push(heap, f, loc);
                    }
                    next
                }
            },
            State::Apply(v) => match task.stack.pop() {
                None => return Progress::Done(Ok(v)),
                Some((frame, fbase, loc)) => {
                    let (next, pushed) = match self.resume(heap, frame, v) {
                        Ok(pair) => pair,
                        Err(e) => (State::Unwind(place(e, loc.clone())), None),
                    };
                    // Nothing from here to `set_state` may allocate: the
                    // frame's roots are gone and whatever the next state
                    // carries lives only in Rust locals until it is rooted
                    // again. `truncate_roots`, `push` and `set_root` all
                    // leave the heap alone.
                    heap.truncate_roots(fbase);
                    if let Some(f) = pushed {
                        task.stack.push(heap, f, loc);
                    }
                    next
                }
            },
        };
        // A channel operation the scheduler can answer on the spot does not
        // cost the task its turn.
        //
        // Only channel operations. `yield` is a turn given up by definition;
        // `sleep` gives one up even when the deadline has passed, which is
        // what makes `(sleep 0.0)` CL's yield-ish zero; `wait` on a finished
        // task keeps the shape those two have. But `(len ch)` is a read, and
        // a cooperative scheduler's promise is that a task switches where it
        // says so — a program that never blocks should not be interleaved by
        // asking a channel how full it is.
        if let State::Blocked(w @ (Waiting::Chan(_) | Waiting::Select { .. })) = &next {
            // Rooted *first*: a parked `send`'s value lives in the state slot
            // and nowhere else, and answering the operation allocates (the
            // `some` box a waiting receiver gets).
            set_state(heap, task.sbase, &next);
            let answered = self.scheduler.borrow_mut().try_now(heap, w);
            if let Some(answer) = answered {
                let next = answer_state(answer);
                set_state(heap, task.sbase, &next);
                task.state = next;
                return Progress::Running;
            }
        }
        set_state(heap, task.sbase, &next);
        if let State::Blocked(w) = &next {
            let w = w.clone();
            task.state = next;
            return Progress::Blocked(w);
        }
        task.state = next;
        Progress::Running
    }

    /// What the driver's answer means for the task.
    ///
    /// The four outcomes are the protocol's own, one level up: an answer ends
    /// the crossing, a suspension parks the task with its chain standing, an
    /// unwind becomes this task's own `State::Unwind`, and a Rust panic is one
    /// that got past the driver's catch — which now means only a payload that
    /// is nobody's on this tier.
    fn after_drive(
        &self,
        heap: &mut Heap,
        task: &mut Task,
        outcome: Result<Result<i64, typelisp_rt::coroutine::Paused>, EvalError>,
        drive: DriveCtx,
    ) -> State {
        match outcome {
            Ok(Ok(word)) => match self.finish_compiled(heap, word, &drive.start.ret, drive.crossing_roots) {
                Ok(v) => {
                    if let Some((name, depth)) = &drive.start.watch {
                        self.trace_depth.set(*depth);
                        if self.step_quiet_depth.get() == *depth {
                            self.step_quiet_depth.set(usize::MAX);
                        }
                        let text = self.trace_render(heap, v);
                        self.trace_line(heap, *depth, &format!("{} returned {}", name, text));
                    }
                    State::Apply(v)
                }
                Err(e) => State::Unwind(e),
            },
            // The chain stays exactly as it is, rooted by the prologues that
            // built it. The frame pushed here is what catches the value the
            // scheduler eventually wakes this task with.
            Ok(Err(typelisp_rt::coroutine::Paused::Suspended)) => match pending_wait(heap) {
                Ok((w, wake)) => {
                    task.stack.push(heap, Frame::DriveCompiled { drive, wake }, None);
                    State::Blocked(w)
                }
                Err(e) => State::Unwind(e),
            },
            // The driver already popped the chain and cut its roots back per
            // frame; the truncate here is for the crossing's own roots, which
            // the interpreter pushed before entering.
            // A function value the chain applied turned out to be
            // interpreted. It runs here, on this task's own continuation
            // stack — which is the whole of C5: the call can suspend, because
            // there is no machine frame under it waiting for an answer.
            Ok(Err(typelisp_rt::coroutine::Paused::Applying { closure, args })) => {
                let roots_on_entry = drive.roots_on_entry;
                match self.begin_applying(heap, task, drive, closure, &args) {
                    Ok(state) => state,
                    Err(e) => {
                        heap.truncate_roots(roots_on_entry);
                        State::Unwind(e)
                    }
                }
            }
            Ok(Err(typelisp_rt::coroutine::Paused::Unwinding)) => {
                let e = crate::eval::crossing::carried_unwind_error();
                heap.truncate_roots(drive.roots_on_entry);
                State::Unwind(e)
            }
            // A compiled `(panic ...)`/`throw` unwound out as a Rust panic and
            // `catch_compiled_panic` turned it back into an error. None of the
            // pops that balance the crossing ran, hence the truncate.
            Err(e) => {
                heap.truncate_roots(drive.roots_on_entry);
                State::Unwind(e)
            }
        }
    }

    /// Writes what the task was waiting for into its chain's innermost frame.
    ///
    /// Encoded through the representation the *suspension site* recorded, for
    /// the reason every crossing obeys: the word's meaning is in the type, and
    /// the value cannot say. `(yield)` and `(sleep ..)` wake with unit, which
    /// encodes to a word the collector never follows.
    fn deliver_wake(&self, heap: &mut Heap, task: &mut Task, v: Value, repr: &Repr) -> Result<(), EvalError> {
        let (words, roots) = self.encode_crossing_args(heap, &[v], std::slice::from_ref(repr), false)?;
        let word = *words.first().ok_or_else(|| {
            EvalError::Internal("delivering a wake value produced no word".to_string())
        })?;
        task.compiled.set_top_value(heap, word);
        for _ in 0..roots {
            heap.pop_root();
        }
        Ok(())
    }

    /// Turns a chain's `apply` of an *interpreted* function value into
    /// continuation frames on this task's stack.
    ///
    /// The representations to decode and encode by come from the closure box
    /// itself, because nothing else at this point has them: the apply site
    /// knows the callee's `Fn` type statically but carries none of it into
    /// the emitted call, and a machine word is exactly as ambiguous here as
    /// anywhere else on this boundary. That is the same reading
    /// `Interp::apply_interpreted` does — what differs is where the call then
    /// happens.
    ///
    /// `Frame::DriveCompiled` is pushed *first*, so it sits below everything
    /// the application builds and catches the answer when it comes back. It
    /// is the same frame a suspension uses, with the callee's return
    /// representation as the `wake`: from the chain's side, an answer from a
    /// scheduler and an answer from an interpreted callee are the same thing.
    fn begin_applying(
        &self,
        heap: &mut Heap,
        task: &mut Task,
        drive: DriveCtx,
        closure: i64,
        argv: &[i64],
    ) -> Result<State, EvalError> {
        let f = typelisp_rt::decode(closure);
        let id = match f {
            Value::Boxed(id) if heap.is_closure(id) => id,
            Value::Boxed(id) if heap.is_builtin_fn(id) => {
                return Err(EvalError::Internal(format!(
                    "the built-in \"{}\" cannot be applied from compiled code: its argument representations are not carried at the apply site",
                    heap.builtin_fn_name(id)
                )))
            }
            other => {
                return Err(EvalError::Internal(format!(
                    "the applied callee is not an interpreted closure: {:?}",
                    other
                )))
            }
        };
        let (params, _, _) = heap.closure_parts(id);
        let reprs = param_reprs(heap, params)?;
        let ret = Repr::read(heap, heap.closure_ret(id)).ok_or_else(|| {
            EvalError::Internal("the applied closure has no declared return representation".to_string())
        })?;
        if argv.len() != reprs.len() {
            return Err(EvalError::Internal(format!(
                "arity mismatch: the applied closure takes {} argument(s), given {}",
                reprs.len(),
                argv.len()
            )));
        }

        task.stack.push(heap, Frame::DriveCompiled { drive, wake: ret }, None);
        // The box is reachable only from the caller's compiled frame, which
        // the collector cannot see, and the *decoded* arguments are new
        // objects reachable from nothing until the call environment binds
        // them.
        let base = heap.root_count();
        heap.push_root(f);
        let mut args = Vec::with_capacity(argv.len());
        for (raw, r) in argv.iter().zip(&reprs) {
            let v = self.decode_compiled_return(heap, *raw, r)?;
            heap.push_root(v);
            args.push(v);
        }
        let (call_env, body) = self.closure_frame(heap, id, args)?;
        let body = core::list(heap, &body)
            .map_err(|e| EvalError::Internal(format!("apply from compiled code: body: {}", e)))?;
        heap.truncate_roots(base);
        // Nothing allocates from here: `sequence_state` only walks conses,
        // and a frame push only roots.
        let (state, extra) = sequence_state(heap, body, call_env)?;
        if let Some(frame) = extra {
            task.stack.push(heap, frame, None);
        }
        Ok(state)
    }

    /// One step of evaluation: reduce `form` to a value, or to a
    /// subexpression with a frame remembering what to do with its value.
    fn step_cps(&self, heap: &mut Heap, stack: &mut CpsStack, form: Value, env: Value) -> Result<State, EvalError> {
        // Every frame this pushes belongs to `form`, and carries its location
        // so an error raised on resuming is placed there.
        let loc = heap.cons_loc(form);
        let tag = match heap.car(form) {
            Ok(Value::Symbol(id)) => id,
            _ => return Err(EvalError::Internal(format!("eval: not a core form: {}", core::print(heap, form)))),
        };
        let op = Op::from_sym(tag)
            .ok_or_else(|| EvalError::Internal(format!("eval: unknown core tag `{}`", heap.symbol_name(tag))))?;

        match op {
            // Leaves: no subexpression, so no frame. These reach a value
            // without recursion even in the old evaluator, so they run
            // through its implementation unchanged.
            Op::Int
            | Op::Float
            | Op::Bignum
            | Op::Ratio
            | Op::Char
            | Op::Bool
            | Op::Str
            | Op::Sym
            | Op::Unit
            | Op::Var
            | Op::Quote
            | Op::Lambda
            | Op::FnRef
            | Op::MethodRef
            | Op::Global
            | Op::CompileFn
            | Op::Trace
            | Op::Untrace
            | Op::DisassembleFn => Ok(State::Apply(self.eval_leaf(heap, form, env)?)),

            // `(go CALL)` — the call's own arguments are collected here, in
            // this task, and only then handed over. `arg_forms` reads them out
            // of the inner node, so the collection is the ordinary one; `spawn`
            // is the single bit that changes what happens once they are in.
            // Every operand is already a value: the checker bound each
            // channel expression and each value to send in a `let` around
            // this node, so all that is left is to read them and hand the
            // whole set to the scheduler.
            Op::Select => {
                let arms = core::fields(heap, form)
                    .map_err(|e| EvalError::Internal(format!("eval: (select ..): {}", e)))?;
                let mut ops: Vec<SelectOp> = Vec::with_capacity(arms.len());
                let mut has_else = false;
                for arm in &arms {
                    let parts = heap.list_to_vec(*arm).map_err(heap_err)?;
                    let tag = match parts.first() {
                        Some(Value::Str(id)) => heap.string(*id).to_string(),
                        other => {
                            return Err(EvalError::Internal(format!(
                                "eval: (select ..) arm tag is {:?}",
                                other
                            )))
                        }
                    };
                    match tag.as_str() {
                        "recv" => {
                            let Some(Value::Str(kid)) = parts.get(2) else {
                                return Err(EvalError::Internal("eval: (select ..) recv arm has no key".into()));
                            };
                            let key = heap.string(*kid).to_string();
                            let chan = self.eval_leaf(heap, parts[3], env)?;
                            ops.push(SelectOp::Recv(chan_id_of(heap, Some(chan))?, key));
                        }
                        "send" => {
                            let chan = self.eval_leaf(heap, parts[2], env)?;
                            let value = self.eval_leaf(heap, parts[3], env)?;
                            ops.push(SelectOp::Send(chan_id_of(heap, Some(chan))?, value));
                        }
                        // Last, and carrying no operand — which is why the
                        // arm indices `ops` uses and the node's own agree up
                        // to this point, and why `ops.len()` names this one.
                        "else" => has_else = true,
                        other => {
                            return Err(EvalError::Internal(format!("eval: (select ..) arm tag `{}`", other)))
                        }
                    }
                }
                stack.push(heap, Frame::SelectArm { form, env }, loc.clone());
                Ok(State::Blocked(Waiting::Select { ops, has_else }))
            }

            Op::Go => {
                let call = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (go ..) has no call".to_string()))?;
                match call_kind(heap, call)? {
                    // `(go (f x))` where `f` is a *value*: the callee is a form
                    // like any argument and is evaluated here, in the starting
                    // task, exactly as `go`'s rule says every part of the call
                    // is. Only the application itself moves.
                    ArgsKind::Apply => {
                        let callee = core::field(heap, call, 0).ok_or_else(|| {
                            EvalError::Internal("eval: (apply ..) has no callee".to_string())
                        })?;
                        stack.push(heap, Frame::ApplyCallee { form: call, env, spawn: true }, loc.clone());
                        Ok(State::Eval(callee, env))
                    }
                    kind => self.start_args(heap, stack, call, env, kind, true, loc),
                }
            }

            Op::If => {
                let cond = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (if ..) has no condition".to_string()))?;
                stack.push(heap, Frame::If { form, env }, loc.clone());
                Ok(State::Eval(cond, env))
            }

            Op::Panic => {
                let msg = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (panic ..) has no message".to_string()))?;
                stack.push(heap, Frame::Panic, loc.clone());
                Ok(State::Eval(msg, env))
            }

            Op::FieldGet => {
                let obj = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (field-get ..) has no object".to_string()))?;
                // Field 2 is the field's representation, read by the bridge
                // and by nothing here.
                let idx = int_field(heap, form, 1, "field-get")? as usize;
                stack.push(heap, Frame::FieldGet { idx }, loc.clone());
                Ok(State::Eval(obj, env))
            }

            Op::DynValue => {
                let inner = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (dyn-value ..) has no operand".to_string()))?;
                stack.push(heap, Frame::DynValue, loc.clone());
                Ok(State::Eval(inner, env))
            }

            Op::Set => {
                let sym = self.sym_field(heap, form, 0, "set")?;
                let val = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (set ..) has no value".to_string()))?;
                stack.push(heap, Frame::Set { sym, env }, loc.clone());
                Ok(State::Eval(val, env))
            }

            Op::Let => {
                let bad = |what: &str| EvalError::Internal(format!("eval: (let ..) {}", what));
                let after_tag = heap.cdr(form).map_err(|_| bad("is not a list"))?;
                let binds = heap.car(after_tag).map_err(|_| bad("has no binding list"))?;
                let body = heap.cdr(after_tag).map_err(|_| bad("has no body"))?;
                if matches!(binds, Value::Empty) {
                    // `(let () E...)` is `progn`.
                    let (next, pushed) = sequence_state(heap, body, env)?;
                    if let Some(f) = pushed {
                        stack.push(heap, f, loc.clone());
                    }
                    return Ok(next);
                }
                let first = heap.car(binds).map_err(|_| bad("binding list is not a list"))?;
                let (_, init) = bind_parts(heap, first)?;
                stack.push(heap, Frame::LetInit { binds, rest: binds, done: Vec::new(), body, env }, loc.clone());
                Ok(State::Eval(init, env))
            }

            Op::Call => self.start_args(heap, stack, form, env, ArgsKind::Call, false, loc),
            Op::Construct => self.start_args(heap, stack, form, env, ArgsKind::Construct, false, loc),
            Op::Assoc => self.start_args(heap, stack, form, env, ArgsKind::Assoc, false, loc),
            Op::DynCall => self.start_args(heap, stack, form, env, ArgsKind::DynCall, false, loc),

            // `labels` evaluates nothing of its own: the closures are built
            // and bound, and only the body runs. Two passes, because the
            // siblings have to see each other — placeholders first, then each
            // closure written through its cell over the environment that
            // already names them all.
            // Arming the stepper is a side effect with an extent, so what to
            // restore rides on a frame — restored on the way out however the
            // body is left.
            Op::Step => {
                let inner = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (step ..) has no form".to_string()))?;
                if !typelisp_rt::sys_builtin::stdin_is_tty() {
                    return Ok(State::Eval(inner, env));
                }
                let frame = Frame::Step {
                    was_stepping: self.stepping.get(),
                    was_quiet: self.step_quiet_depth.get(),
                };
                self.stepping.set(true);
                self.step_quiet_depth.set(usize::MAX);
                stack.push(heap, frame, loc.clone());
                Ok(State::Eval(inner, env))
            }

            Op::DynNew => {
                // Field 4 is the boxed value's representation, read by the
                // bridge and by nothing here.
                let value = core::field(heap, form, 5)
                    .ok_or_else(|| EvalError::Internal("eval: (dyn-new ..) has no value".to_string()))?;
                stack.push(heap, Frame::DynNew { form }, loc.clone());
                Ok(State::Eval(value, env))
            }

            Op::DynUpcast => {
                let value = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (dyn-upcast ..) has no operand".to_string()))?;
                stack.push(heap, Frame::DynUpcast { form }, loc.clone());
                Ok(State::Eval(value, env))
            }

            Op::Apply => {
                let callee = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (apply ..) has no callee".to_string()))?;
                stack.push(heap, Frame::ApplyCallee { form, env, spawn: false }, loc.clone());
                Ok(State::Eval(callee, env))
            }

            Op::Labels => {
                let defs = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (labels ..) has no definitions".to_string()))?;
                let defs = heap
                    .list_to_vec(defs)
                    .map_err(|e| EvalError::Internal(format!("eval: (labels ..) definitions: {}", e)))?;

                let names = |heap: &Heap, d: Value| match heap.car(d) {
                    Ok(Value::Symbol(sym)) => Ok(sym),
                    other => Err(EvalError::Internal(format!("eval: (labels ..) definition names {:?}", other))),
                };
                let placeholders: Vec<(SymRef, Value)> = defs
                    .iter()
                    .map(|d| names(heap, *d).map(|sym| (sym, Value::Empty)))
                    .collect::<Result<_, _>>()?;

                // The environment is rooted while the closures are built:
                // `alloc_closure` allocates, and nothing else refers to it
                // yet. The root goes above whatever frames are open, and comes
                // off before the body starts.
                let base = heap.root_count();
                let inner = extend_env(heap, &placeholders, env)?;
                heap.push_root(inner);
                for d in &defs {
                    // A definition is `(SYM PARAMS RET-R E...)` — positional,
                    // like a `let` binding and unlike a tagged node.
                    let sym = names(heap, *d)?;
                    let rest = heap.cdr(*d).map_err(heap_err)?;
                    let params = heap.car(rest).map_err(heap_err)?;
                    let ret = heap.cdr(rest).and_then(|d| heap.car(d)).map_err(heap_err)?;
                    let body = tail_after_value(heap, rest, 2)?;
                    let f = heap.alloc_closure(params, ret, body, inner);
                    let cell = env_lookup(heap, inner, sym).ok_or_else(|| {
                        EvalError::Internal(format!("eval: (labels ..) lost the slot for `{}`", heap.symbol_name(sym)))
                    })?;
                    match cell {
                        Value::Boxed(id) if heap.is_cell(id) => heap.cell_set(id, f),
                        other => {
                            return Err(EvalError::Internal(format!("eval: (labels ..) slot is {:?}", other)))
                        }
                    }
                }
                let body = tail_after(heap, form, 1)?;
                heap.truncate_roots(base);
                // Nothing allocates from here: `sequence_state` only walks
                // conses, so `inner` survives in a Rust local until the state
                // slots take it.
                let (next, pushed) = sequence_state(heap, body, inner)?;
                if let Some(f) = pushed {
                    stack.push(heap, f, loc.clone());
                }
                Ok(next)
            }

            Op::Match => {
                let scrut = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (match ..) has no scrutinee".to_string()))?;
                stack.push(heap, Frame::MatchArms { form, env }, loc.clone());
                Ok(State::Eval(scrut, env))
            }

            Op::FieldSet => {
                let obj = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (field-set ..) has no object".to_string()))?;
                stack.push(heap, Frame::FieldSetObj { form, env }, loc.clone());
                Ok(State::Eval(obj, env))
            }

            Op::SetGlobal => {
                let value = core::field(heap, form, 4)
                    .ok_or_else(|| EvalError::Internal("eval: (set-global ..) has no value form".to_string()))?;
                stack.push(heap, Frame::SetGlobal { form }, loc.clone());
                Ok(State::Eval(value, env))
            }

            Op::Loop => {
                // `(loop REPR BODY...)`: past the tag *and* past the repr, which
                // is the compiled side's alone (`check::forms::loop_form`) — a
                // `break`/`return` value travels in a `State::Unwind` here, and
                // the collector walks that as a root source of its own.
                let fields = heap
                    .cdr(form)
                    .map_err(|e| EvalError::Internal(format!("eval: (loop ..): {}", e)))?;
                // The repr is *checked* here, alone among the repr fields this
                // evaluator ignores, because this is the only one that leads.
                // A trailing repr left out of a hand-written form (`catch`'s
                // tests do exactly that) still reads correctly; a leading one
                // left out silently promotes the first *statement* into the
                // repr's place and drops it. That is not a hypothetical: it
                // turned `(loop (if .. (break) ..) (set ..))` into a loop with
                // no exit, and the symptom was nine hours at 100% CPU with
                // nothing printed. Once per loop entry, not per iteration —
                // `Frame::Loop` restarts from the body without re-reading the
                // node.
                let repr = heap
                    .car(fields)
                    .map_err(|e| EvalError::Internal(format!("eval: (loop ..) has no representation: {}", e)))?;
                if crate::check::repr::Repr::read(heap, repr).is_none() {
                    return Err(EvalError::Internal(format!(
                        "eval: (loop ..)'s first field is not a representation: {}",
                        core::print(heap, form)
                    )));
                }
                let body = heap
                    .cdr(fields)
                    .map_err(|e| EvalError::Internal(format!("eval: (loop ..): {}", e)))?;
                if matches!(body, Value::Empty) {
                    return Err(EvalError::Internal("eval: (loop ..) has no body".to_string()));
                }
                let first = heap
                    .car(body)
                    .map_err(|e| EvalError::Internal(format!("eval: (loop ..): {}", e)))?;
                stack.push(heap, Frame::Loop { body, rest: body, env }, loc.clone());
                Ok(State::Eval(first, env))
            }

            Op::Break => Ok(State::Unwind(EvalError::Break)),

            Op::Return => match core::field(heap, form, 0) {
                Some(val) => {
                    stack.push(heap, Frame::Return, loc.clone());
                    Ok(State::Eval(val, env))
                }
                None => Ok(State::Unwind(EvalError::Return(Box::new(Value::Empty)))),
            },

            Op::Block => {
                let name = self.block_name_of(heap, form)?;
                let body = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (block ..) has no body".to_string()))?;
                stack.push(heap, Frame::Block { name }, loc.clone());
                Ok(State::Eval(body, env))
            }

            Op::ReturnFrom => {
                let name = self.block_name_of(heap, form)?;
                match core::field(heap, form, 1) {
                    Some(val) => {
                        stack.push(heap, Frame::ReturnFrom { name }, loc.clone());
                        Ok(State::Eval(val, env))
                    }
                    None => Ok(State::Unwind(EvalError::ReturnFrom(name, Box::new(Value::Empty)))),
                }
            }

            Op::Catch => {
                let tag = self.throw_tag_of(heap, form, "catch")?;
                let body = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (catch ..) has no body".to_string()))?;
                stack.push(heap, Frame::Catch { tag }, loc.clone());
                Ok(State::Eval(body, env))
            }

            Op::Throw => {
                let tag = self.throw_tag_of(heap, form, "throw")?;
                let value = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (throw ..) has no value".to_string()))?;
                stack.push(heap, Frame::Throw { tag }, loc.clone());
                Ok(State::Eval(value, env))
            }

            Op::UnwindProtect => {
                let protected = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (unwind-protect ..) has no protected form".to_string()))?;
                let cleanup = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (unwind-protect ..) has no cleanup form".to_string()))?;
                stack.push(heap, Frame::Protect { cleanup, env }, loc.clone());
                Ok(State::Eval(protected, env))
            }

        }
    }

    /// Begins evaluating an argument run: the first argument, with a frame to
    /// collect it, or the node's own work when there are no arguments at all.
    fn start_args(
        &self,
        heap: &mut Heap,
        stack: &mut CpsStack,
        form: Value,
        env: Value,
        kind: ArgsKind,
        spawn: bool,
        loc: Option<crate::Loc>,
    ) -> Result<State, EvalError> {
        let args = arg_forms(heap, form, kind)?;
        match args.first() {
            Some(&first) => {
                stack.push(heap, Frame::Args { form, done: Vec::new(), env, kind, spawn }, loc.clone());
                Ok(State::Eval(first, env))
            }
            None => {
                let (next, pushed) = self.finish_args(heap, form, Vec::new(), kind, spawn)?;
                if let Some(f) = pushed {
                    stack.push(heap, f, loc.clone());
                }
                Ok(next)
            }
        }
    }

    /// Every argument is in hand: do the node's own work.
    fn finish_args(
        &self,
        heap: &mut Heap,
        form: Value,
        argv: Vec<Value>,
        kind: ArgsKind,
        spawn: bool,
    ) -> Result<(State, Option<Frame>), EvalError> {
        // `go`: the call does not happen here. It becomes a task of its own,
        // and this form gets a handle on it instead of a result. Deciding it
        // *before* the call is what keeps `(go (f x))` concurrent even when `f`
        // is compiled — a compiled body runs to completion once entered, so
        // entering it here would make the `go` a plain call.
        if spawn {
            let handle = self.spawn_task(heap, form, argv, kind)?;
            return Ok((State::Apply(handle), None));
        }
        match kind {
            ArgsKind::Call => self.finish_call(heap, form, argv),
            ArgsKind::Construct => self.finish_construct(heap, form, argv),
            ArgsKind::Assoc => self.finish_assoc(heap, form, argv),
            ArgsKind::DynCall => self.finish_dyn_call(heap, form, argv),
            ArgsKind::Apply => {
                let mut it = argv.into_iter();
                let callee = it
                    .next()
                    .ok_or_else(|| EvalError::Internal("eval: (apply ..) lost its callee".to_string()))?;
                self.finish_apply(heap, form, callee, it.collect())
            }
        }
    }

    /// The callee and every argument of an `apply` are in hand — so either the
    /// application happens, or `go` hands it to a task.
    ///
    /// **The callee rides at the head of the argument list from here on.** That
    /// is what lets an `apply` reach `State::Enter`, whose two root slots hold
    /// one form and one list and have no third place to put a callee.
    fn finish_apply_args(
        &self,
        heap: &mut Heap,
        form: Value,
        callee: Value,
        args: Vec<Value>,
        spawn: bool,
    ) -> Result<(State, Option<Frame>), EvalError> {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(callee);
        argv.extend(args);
        self.finish_args(heap, form, argv, ArgsKind::Apply, spawn)
    }

    /// `(go ...)` reached from **compiled** code: the parts arrive as machine
    /// words, and a task is started from them.
    ///
    /// The node itself is `args[0]` — rebuilt by the compiled code that is
    /// starting the task, because an ahead-of-time compiled program shares no
    /// object table with the heap that compiled it (`core_bridge`'s
    /// `translate_go`). Everything after it is an argument, in the callee's
    /// **declared representation**, so the words are decoded by the same rule
    /// that decodes a compiled call's result — a word is a raw `f64` bit
    /// pattern or a tagged pointer, and only the declared type says which.
    ///
    /// The task is admitted and nothing is run: compiled code cannot suspend,
    /// so the caller keeps going and the scheduler picks the new task up at the
    /// next point some *interpreted* task yields, waits or finishes.
    pub(super) fn spawn_from_compiled(&self, heap: &mut Heap, args: &[i64]) -> Result<i64, EvalError> {
        let form = typelisp_rt::decode(args[0]);
        let kind = call_kind(heap, form)?;
        let reprs = {
            let field = core::field(heap, form, kind.repr_at()).ok_or_else(|| {
                EvalError::Internal(format!("go: ({} ..) has no representation list", kind.what()))
            })?;
            repr_list(heap, field, kind.what())?
        };

        let mut s = RootScope::new(heap);
        s.push_root(form);
        let mut argv = Vec::with_capacity(args.len() - 1);
        let mut raw = &args[1..];
        // An `apply`'s callee travels at the head of the argument run — see
        // `finish_apply_args`. It is a function value, so it is already tagged.
        if kind == ArgsKind::Apply {
            let (callee, rest) = raw.split_first().ok_or_else(|| {
                EvalError::Internal("go: (apply ..) arrived without its callee".to_string())
            })?;
            let callee = typelisp_rt::decode(*callee);
            s.push_root(callee);
            argv.push(callee);
            raw = rest;
        }
        if raw.len() != reprs.len() {
            return Err(EvalError::Internal(format!(
                "go: {} argument(s) for {} representation(s)",
                raw.len(),
                reprs.len()
            )));
        }
        for (w, r) in raw.iter().zip(&reprs) {
            // Decoding allocates (a float argument builds a box), so each one
            // is rooted before the next is decoded.
            let v = self.decode_compiled_return(&mut s, *w, r)?;
            s.push_root(v);
            argv.push(v);
        }

        let handle = self.spawn_task(&mut s, form, argv, kind)?;
        Ok(typelisp_rt::encode(handle))
    }

    /// `(sleep secs)` — suspend **this task** until `secs` from now.
    ///
    /// Zero is CL's yield-ish zero and falls out of the deadline already
    /// having passed: `Scheduler::block` puts such a task straight back on the
    /// queue, which is exactly `yield`.
    fn sleep_until(&self, heap: &Heap, argv: &[Value]) -> Result<(State, Option<Frame>), EvalError> {
        let v = argv
            .first()
            .copied()
            .ok_or_else(|| EvalError::Internal("eval: (sleep) has no argument".to_string()))?;
        let secs = super::rt_f64(heap, &v)?;
        // The same refusal `typelisp_rt::sys_builtin::sleep` makes, in the same
        // words: CL calls a negative or NaN wait an error, and there is no
        // duration to build from one.
        if !(secs >= 0.0) {
            return Err(EvalError::Panic(format!("sleep: {} is not a non-negative number of seconds", secs)));
        }
        let d = std::time::Duration::try_from_secs_f64(secs)
            .map_err(|_| EvalError::Panic(format!("sleep: {} is longer than this can wait", secs)))?;
        Ok((State::Blocked(Waiting::Until(std::time::Instant::now() + d)), None))
    }

    /// Hands a fully-evaluated call over to a new task, and returns its handle.
    fn spawn_task(
        &self,
        heap: &mut Heap,
        form: Value,
        argv: Vec<Value>,
        kind: ArgsKind,
    ) -> Result<Value, EvalError> {
        let home = heap.current_root_stack();
        let argv_list = core::list(heap, &argv).map_err(heap_err)?;
        heap.push_root(argv_list);
        let roots = heap.new_root_stack();
        // `form` and `argv_list` are rooted in `home`, which the collector
        // still walks, so they survive this switch.
        heap.switch_to_root_stack(roots);
        let task = Task::start_call(heap, form, argv_list, kind);
        heap.switch_to_root_stack(home);
        heap.pop_root();
        let id = self.scheduler.borrow_mut().admit(TaskSlot { task, roots });
        Ok(task_handle(heap, id))
    }

    /// Enters an interpreted or compiled function body with `argv` bound.
    ///
    /// A user function's body becomes a sequence in a fresh environment, so
    /// **its last form is a tail jump**. `Interp::apply` — the other way into a
    /// body, for a caller that is a Rust frame — cannot do that: it evaluates
    /// the last form with that frame still waiting on it.
    fn enter_fn(&self, heap: &mut Heap, f: &Rc<FnDef>, argv: Vec<Value>) -> Result<(State, Option<Frame>), EvalError> {
        // Tracing and stepping report a call *around* the callee, in CL's own
        // shape: `  0: (fact 3)` going in and `  0: fact returned 6` coming
        // out. That needs a frame to come back to, so a watched call gives up
        // its tail position: tracing a self-tail-recursive function makes it
        // grow the continuation stack again, for as long as it is traced.
        //
        // Two `Cell` loads on the path every call takes; the set membership is
        // tested only once one of them is armed.
        let stepping = self.stepping.get() && self.trace_depth.get() <= self.step_quiet_depth.get();
        let watched = stepping || (self.trace_armed.get() && self.traced.borrow().contains(&f.name));
        let watch = if watched {
            let depth = self.trace_depth.get();
            let call = {
                let rendered: Vec<String> = argv.iter().map(|a| self.trace_render(heap, *a)).collect();
                if rendered.is_empty() {
                    format!("({})", f.name)
                } else {
                    format!("({} {})", f.name, rendered.join(" "))
                }
            };
            self.trace_line(heap, depth, &call);
            if stepping {
                match self.step_prompt(heap) {
                    StepCmd::Into => {}
                    // Quiet *below* this frame: the command is "run this
                    // call", and the frame it was given at is the one to ask
                    // at again.
                    StepCmd::Over => self.step_quiet_depth.set(depth),
                    StepCmd::Continue => self.stepping.set(false),
                    StepCmd::Quit => return Err(EvalError::Panic("step: aborted".to_string())),
                }
            }
            self.trace_depth.set(depth + 1);
            Some(Frame::TracedCall { name: f.name.clone(), depth })
        } else {
            None
        };

        let compiled = f.compiled.borrow().clone();
        if let Some(compiled) = compiled {
            let sig = f.sig.as_ref().expect("a compiled function always has a type signature");
            // A coroutine-ABI body is **driven by this task**, not called: the
            // frames it builds live in `Task::compiled`, so it can stop in the
            // middle and the task can be put down with it. That is the whole
            // of Phase C3, and it is why the crossing is split in two around
            // however many times the body stops (`Interp::begin_compiled`).
            if compiled.body_abi() == typelisp_abi::BODY_ABI_COROUTINE {
                let watch = match &watch {
                    Some(Frame::TracedCall { name, depth }) => Some((name.clone(), *depth)),
                    _ => None,
                };
                let start = DriveStart {
                    body: compiled,
                    params: sig.0.clone(),
                    ret: sig.1.clone(),
                    watch,
                };
                // A heap list, not the `Vec`: it has to stay reachable across
                // the truncate that releases this call's own frame, and only
                // the state slots can root it (`set_state`).
                let argv = core::list(heap, &argv).map_err(heap_err)?;
                return Ok((State::CompiledEnter { argv, start }, None));
            }
            // A *classic* body runs on the Rust stack: the boundary is one
            // frame from the machine's point of view, and nothing can suspend
            // inside it. There is nowhere for it to keep its state, which is
            // the shape the coroutine ABI exists to replace.
            let v = self.call_compiled(heap, compiled.as_ref(), &argv, &sig.0, &sig.1)?;
            // The call is over already, so a watch frame would have nothing to
            // wait for: report the return here instead.
            if let Some(Frame::TracedCall { name, depth }) = watch {
                self.trace_depth.set(depth);
                if self.step_quiet_depth.get() == depth {
                    self.step_quiet_depth.set(usize::MAX);
                }
                let text = self.trace_render(heap, v);
                self.trace_line(heap, depth, &format!("{} returned {}", name, text));
            }
            return Ok((State::Apply(v), None));
        }
        if f.params.len() != argv.len() {
            return Err(EvalError::Internal(format!(
                "apply: the body takes {} argument(s), given {}",
                f.params.len(),
                argv.len()
            )));
        }
        let binds: Vec<(SymRef, Value)> = f
            .params
            .iter()
            .zip(argv)
            .map(|(name, v)| match heap.intern_symbol(name) {
                Value::Symbol(id) => (id, v),
                _ => unreachable!("Heap::intern_symbol always returns Value::Symbol"),
            })
            .collect();
        // A top-level function closes over nothing, so its frame extends the
        // empty environment rather than the caller's — that is what makes the
        // scope lexical.
        //
        // Rooted for the rest of this function: consing the body below
        // allocates, and nothing else refers to the environment yet. The root
        // goes above whatever frames are open, so the caller's truncation
        // drops it.
        let base = heap.root_count();
        let env = extend_env(heap, &binds, Value::Empty)?;
        heap.push_root(env);
        // A watched call has to hand back *its* frame, so the body cannot
        // also leave a sequence frame behind. Wrapping it as `(let () E...)`
        // — which is `progn` — makes it one form, and the `let` node opens
        // the sequence frame itself one step later. Only on the watched path,
        // which is already paying for a frame.
        if let Some(w) = watch {
            let mut items = Vec::with_capacity(f.body.len() + 1);
            items.push(Value::Empty);
            items.extend_from_slice(&f.body);
            let wrapped = core::tagged(heap, "let", &items)
                .map_err(|e| EvalError::Internal(format!("eval: watched call body: {}", e)))?;
            heap.truncate_roots(base);
            return Ok((State::Eval(wrapped, env), Some(w)));
        }
        // `FnDef::body` is a `Vec`, and a sequence frame walks a cons list, so
        // the body is consed up here. `core::list` roots every item and every
        // partial tail, so a collection mid-build is safe. It is the last
        // allocation on this path: `sequence_state` only walks conses, so
        // `body` and `env` stay in Rust locals — live because nothing
        // collects — until the caller roots them again.
        let body =
            core::list(heap, &f.body).map_err(|e| EvalError::Internal(format!("eval: call body: {}", e)))?;
        heap.truncate_roots(base);
        // Nothing allocates from here: `sequence_state` only walks conses.
        sequence_state(heap, body, env)
    }

    fn finish_call(&self, heap: &mut Heap, form: Value, argv: Vec<Value>) -> Result<(State, Option<Frame>), EvalError> {
        let written = self.name_list(heap, form, 0, "call")?;
        let home = self.name_list(heap, form, 1, "call")?;
        let path = path_field(heap, form, 2, "call")?;

        // `(yield)` gives up the rest of this turn — not a builtin, for the
        // same reason `wait` is not: a builtin answers with a value, and this
        // has to say "stop here" instead.
        if path == crate::Path::root("yield") {
            return Ok((State::Blocked(Waiting::Yield), None));
        }
        // `(sleep secs)` is the same shape: it has to say "not before then",
        // which no builtin's return value can. Intercepted here rather than
        // left to `eval_builtin`, whose implementation stops the whole OS
        // thread — with tasks, that would stop every one of them.
        if path == crate::Path::root("sleep") {
            return self.sleep_until(heap, &argv);
        }
        if let Some(f) = self.resolve_fn_named(&home, &written, &path) {
            return self.enter_fn(heap, &f, argv);
        }
        // Otherwise a built-in operator, which lives at the root and so is
        // always spelled as a bare name.
        if written.len() == 1 {
            if let Some(result) = self.eval_builtin(heap, &written[0], &argv) {
                return Ok((State::Apply(result?), None));
            }
        }
        Err(EvalError::NoSuchFunction(path.to_string()))
    }

    /// The callee and every argument of an `apply` are in hand.
    ///
    /// Four kinds of callee: an interpreted closure (entered as a tail jump,
    /// so a self-call in tail position costs no stack), one that came *out* of
    /// compiled code (crossed on the Rust stack — the boundary is one frame
    /// and nothing suspends inside it), a built-in used as a function value,
    /// or something the checker should have rejected.
    fn finish_apply(
        &self,
        heap: &mut Heap,
        form: Value,
        f: Value,
        argv: Vec<Value>,
    ) -> Result<(State, Option<Frame>), EvalError> {
        let fields = core::fields(heap, form).map_err(|e| EvalError::Internal(format!("eval: (apply ..): {}", e)))?;
        if fields.len() < 3 {
            return Err(EvalError::Internal(format!("eval: malformed apply: {}", core::print(heap, form))));
        }
        let ret_repr_field = fields[1];
        let arg_reprs_field = fields[2];

        let id = match f {
            Value::Boxed(id) if heap.is_closure(id) => id,

            // Its arguments cross the boundary and its result comes back, both
            // driven by the declared representations this node carries.
            Value::Boxed(id) if heap.is_compiled_closure(id) => {
                let arg_reprs = repr_list(heap, arg_reprs_field, "apply")?;
                let ret = Repr::read(heap, ret_repr_field)
                    .ok_or_else(|| EvalError::Internal("eval: (apply ..) has no return representation".to_string()))?;
                let (int_args, crossing_roots) = self.encode_crossing_args(heap, &argv, &arg_reprs, false)?;
                self.enter_compiled(heap);
                // An unwinding `(panic ...)` inside the closure runs none of
                // the pops below and leaves whatever roots the compiled body
                // pushed; the caller's `truncate_roots` past this frame is the
                // repair.
                let raw = crate::eval::crossing::catch_compiled_panic(|| Interp::call_closure_box(heap, id, &int_args))?;
                for _ in 0..crossing_roots {
                    heap.pop_root();
                }
                let v = self.decode_compiled_return(heap, raw, &ret)?;
                return Ok((State::Apply(v), None));
            }

            // A built-in used as a function value — dispatched by name through
            // the very same `eval_builtin`/`eval_builtin_method` a direct call
            // site goes through; the box carries only which name.
            Value::Boxed(id) if heap.is_builtin_fn(id) => {
                let name = heap.builtin_fn_name(id).to_string();
                let v = match heap.builtin_fn_recv(id) {
                    None => match self.eval_builtin(heap, &name, &argv) {
                        Some(r) => r?,
                        None => return Err(EvalError::NoSuchFunction(name)),
                    },
                    Some(pid) => {
                        let type_name = crate::types::path_from_id(heap, pid);
                        let ret_key = heap.builtin_fn_ret_key(id).to_string();
                        match super::eval_builtin_method(heap, &type_name, &name, &argv, &ret_key) {
                            Some(r) => r?,
                            None => {
                                return Err(EvalError::NoSuchFunction(format!("{}::{}", type_name, name)))
                            }
                        }
                    }
                };
                return Ok((State::Apply(v), None));
            }

            other => {
                return Err(EvalError::Internal(format!(
                    "eval: (apply ..) callee is not a function: {:?}",
                    other
                )))
            }
        };

        // The call environment extends the environment the closure *captured*,
        // not the caller's: that is what makes this lexical scope rather than
        // dynamic. `closure_frame` roots it on the heap.
        let base = heap.root_count();
        let (call_env, body) = self.closure_frame(heap, id, argv)?;
        let body = core::list(heap, &body)
            .map_err(|e| EvalError::Internal(format!("eval: (apply ..) body: {}", e)))?;
        heap.truncate_roots(base);
        // Nothing allocates from here: `sequence_state` only walks conses.
        sequence_state(heap, body, call_env)
    }

    /// `(construct PATH KEY VARIANT MUTABLE (R...) ARG...)` — a struct, enum
    /// variant or `Sexpr` constructor.
    fn finish_construct(
        &self,
        heap: &mut Heap,
        form: Value,
        argv: Vec<Value>,
    ) -> Result<(State, Option<Frame>), EvalError> {
        let path = path_field(heap, form, 0, "construct")?;
        // The value's runtime identity, spelled by the checker where the
        // instantiation was known: `gen<i32>` and `gen<string>` share `path`
        // and differ here.
        let key = str_field(heap, form, 1, "construct")?;
        let variant = int_field(heap, form, 2, "construct")? as usize;
        let mutable = bool_field(heap, form, 3, "construct")?;

        let v = if super::is_sexpr_type(&path) {
            construct_sexpr_core(heap, variant, &argv)?
        } else if mutable {
            crate::type_key::alloc_struct_keyed(heap, &key, argv)
        } else {
            crate::type_key::alloc_enum_keyed(heap, &key, variant, argv)
        };
        Ok((State::Apply(v), None))
    }

    /// `(assoc PATH SYM INSTANCE (HOME...) RET-REPR (R...) ARG...)` — a method
    /// call. The receiver, when there is one, is `argv[0]`.
    fn finish_assoc(
        &self,
        heap: &mut Heap,
        form: Value,
        argv: Vec<Value>,
    ) -> Result<(State, Option<Frame>), EvalError> {
        let type_name = path_field(heap, form, 0, "assoc")?;
        let method = sym_field(heap, form, 1, "assoc")?;
        let home = self.name_list(heap, form, 3, "assoc")?;
        // The result's runtime identity, for the built-in methods that build a
        // box: `Vector::new` has no field to read an instantiation off.
        let ret_key = str_field(heap, form, 6, "assoc")?;

        // `Task<T>::wait` cannot go through `eval_builtin_method`: it may have to
        // *suspend*, and a builtin answers with a value or an error, with no way
        // to say "not yet". Intercepted here, where a `State` can say it.
        if method == "wait" && type_name == crate::Path::root("task") {
            let on = task_id_of(heap, argv.first().copied())?;
            return Ok((State::Blocked(Waiting::Task(on)), None));
        }
        // Every `Chan<T>` operation, for the same reason and two more: the
        // table of channels is the scheduler's (nothing lower can hold values
        // across a `Heap`'s lifetime), and `send`/`recv` really can have to
        // wait. Naming the operation and letting `Scheduler::try_now` decide
        // is what makes this path and the compiled one the same code.
        // Only the six built-in operations: a *user* method on `Chan<T>` (the
        // prelude's own `Iter::next`, and any `impl` a program writes) has to
        // fall through to ordinary resolution, and a monomorphized one
        // arrives under a mangled name that matches none of these.
        if type_name == crate::Path::root("chan") {
            let op = match method.as_str() {
                "new" => match argv.first() {
                    Some(Value::Int(n)) => ChanOp::New(*n),
                    other => {
                        return Err(EvalError::Internal(format!(
                            "Chan::new: {:?} is not a capacity",
                            other
                        )))
                    }
                },
                "len" => ChanOp::Len(chan_id_of(heap, argv.first().copied())?),
                "cap" => ChanOp::Cap(chan_id_of(heap, argv.first().copied())?),
                "close" => ChanOp::Close(chan_id_of(heap, argv.first().copied())?),
                "send" => {
                    let c = chan_id_of(heap, argv.first().copied())?;
                    let v = argv.get(1).copied().ok_or_else(|| {
                        EvalError::Internal("send: no value to send".to_string())
                    })?;
                    ChanOp::Send(c, v)
                }
                // `ret_key` *is* the `Option<T>` this answer is built with —
                // the assoc node's result identity, the same slot
                // `Vector<T>::pop` reads.
                "recv" => ChanOp::Recv(chan_id_of(heap, argv.first().copied())?, ret_key.clone()),
                _ => return self.finish_assoc_resolved(heap, form, argv, type_name, method, home, ret_key),
            };
            return Ok((State::Blocked(Waiting::Chan(op)), None));
        }

        self.finish_assoc_resolved(heap, form, argv, type_name, method, home, ret_key)
    }

    /// [`Self::finish_assoc`] past the interceptions: an ordinary method, a
    /// user `impl`'s, or a built-in one.
    ///
    /// Its own function so the channel interception can *decline* — a method
    /// on `Chan<T>` that is not one of the six (`Iter::next`, or anything a
    /// program writes) has to reach exactly this.
    #[allow(clippy::too_many_arguments)]
    fn finish_assoc_resolved(
        &self,
        heap: &mut Heap,
        _form: Value,
        argv: Vec<Value>,
        type_name: crate::Path,
        method: String,
        home: Vec<String>,
        ret_key: String,
    ) -> Result<(State, Option<Frame>), EvalError> {
        let f = self.root.borrow().resolve_method(&home, &type_name, &method);
        match f {
            Some(f) => self.enter_fn(heap, &f, argv),
            None => match super::eval_builtin_method(heap, &type_name, &method, &argv, &ret_key) {
                Some(result) => Ok((State::Apply(result?), None)),
                None => Err(EvalError::NoSuchFunction(format!("{}::{}", type_name, method))),
            },
        }
    }

    /// `(dyn-call PATH SYM SLOT VTABLE (R...) ARG...)` — a call through a
    /// trait object's vtable.
    ///
    /// `argv[0]` is the fat box; the callee is an ordinary method body with an
    /// ordinary receiver, so the concrete value is substituted in its place and
    /// nothing about the callee knows it was reached dynamically.
    fn finish_dyn_call(
        &self,
        heap: &mut Heap,
        form: Value,
        mut argv: Vec<Value>,
    ) -> Result<(State, Option<Frame>), EvalError> {
        let trait_path = path_field(heap, form, 0, "dyn-call")?;
        let method = sym_field(heap, form, 1, "dyn-call")?;
        let slot = int_field(heap, form, 2, "dyn-call")? as usize;

        let (vtable_id, inner) = match argv.first() {
            Some(Value::Boxed(id)) if heap.is_dyn(*id) => (heap.dyn_vtable_id(*id), heap.dyn_value(*id)),
            other => {
                return Err(EvalError::Internal(format!(
                    "eval: (dyn-call {}::{} ..): receiver is not a trait object ({:?})",
                    trait_path, method, other
                )))
            }
        };
        let target = self.vtables.borrow().get(vtable_id as usize).and_then(|slots| slots.get(slot)).cloned();
        let Some((target_type, target_method)) = target else {
            return Err(EvalError::Internal(format!(
                "eval: (dyn-call {}::{} ..): vtable {} has no slot {}",
                trait_path, method, vtable_id, slot
            )));
        };
        argv[0] = inner;
        // Visibility was settled where the value was boxed, so this is the
        // direct lookup, not `resolve_method`'s `home`-relative one.
        let f = self.root.borrow().get_method(&target_type, &target_method);
        match f {
            Some(f) => self.enter_fn(heap, &f, argv),
            None => Err(EvalError::NoSuchFunction(format!("{}::{}", target_type, target_method))),
        }
    }

    /// Hands `v` to `frame` — the frame's "what to do with the value" half.
    ///
    /// Returns the next state and, when the resumption itself suspends on a
    /// subexpression, the frame to push for it. It does **not** push that
    /// frame: the caller has to drop this frame's roots first, and the root
    /// stack is LIFO, so anything pushed here would be truncated away with
    /// them.
    ///
    /// A value this method allocates (an extended environment, say) is rooted
    /// by nothing until the caller re-roots it, so **nothing may allocate
    /// after the last value the returned state carries is built**.
    fn resume(&self, heap: &mut Heap, frame: Frame, v: Value) -> Result<(State, Option<Frame>), EvalError> {
        match frame {
            Frame::If { form, env } => {
                let taken = match v {
                    Value::Bool(true) => 1,
                    Value::Bool(false) => 2,
                    other => return Err(EvalError::Internal(format!("eval: (if ..) condition is {:?}", other))),
                };
                let branch = core::field(heap, form, taken)
                    .ok_or_else(|| EvalError::Internal("eval: (if ..) is missing a branch".to_string()))?;
                // A tail jump: no frame, the branch's value is the `if`'s.
                Ok((State::Eval(branch, env), None))
            }

            // A non-string message is not reachable from a checked program
            // and carries nothing to report, so it panics with no text.
            Frame::Panic => Err(EvalError::Panic(match v {
                Value::Str(id) => heap.string(id).to_string(),
                _ => String::new(),
            })),

            // No decode: with one value world left, the field *is* the value.
            Frame::FieldGet { idx } => {
                let id = super::expect_struct_box(&v)?;
                Ok((State::Apply(heap.struct_field(id, idx)), None))
            }

            Frame::DynValue => match v {
                Value::Boxed(id) if heap.is_dyn(id) => Ok((State::Apply(heap.dyn_value(id)), None)),
                other => Err(EvalError::Internal(format!(
                    "eval: (dyn-value ..): not a trait object: {:?}",
                    other
                ))),
            },

            // Written through the cell rather than rebuilding the frame,
            // which is what makes the assignment visible through every other
            // reference to the same binding — a closure's capture, or
            // compiled code handed the same cell.
            Frame::Set { sym, env } => {
                let cell = env_lookup(heap, env, sym)
                    .ok_or_else(|| EvalError::Unbound(heap.symbol_name(sym).to_string()))?;
                match cell {
                    Value::Boxed(id) if heap.is_cell(id) => heap.cell_set(id, v),
                    other => {
                        return Err(EvalError::Internal(format!(
                            "eval: binding does not hold a cell: {:?}",
                            other
                        )))
                    }
                }
                // `setf` returns the value assigned (docs/syntax.md §7).
                Ok((State::Apply(v), None))
            }

            Frame::Args { form, mut done, env, kind, spawn } => {
                done.push(v);
                let args = arg_forms(heap, form, kind)?;
                match args.get(done.len()) {
                    Some(&next) => {
                        Ok((State::Eval(next, env), Some(Frame::Args { form, done, env, kind, spawn })))
                    }
                    None => self.finish_args(heap, form, done, kind, spawn),
                }
            }

            // Only the last form of a sequence contributes a value, so this
            // one is discarded.
            Frame::Seq { rest, env } => sequence_state(heap, rest, env),

            Frame::Step { was_stepping, was_quiet } => {
                self.stepping.set(was_stepping);
                self.step_quiet_depth.set(was_quiet);
                Ok((State::Apply(v), None))
            }

            // The value the task was put down for. It goes back into the
            // compiled chain rather than up the continuation stack, and
            // `CompiledResume` is where the encoding happens — this frame is
            // only what catches the scheduler's `Apply`.
            // `v` is `(ARM . VALUE)`: which arm won, and what it answered
            // with. The index is the whole of the dispatch — the checker put
            // the `else` arm last precisely so its index is the one past the
            // channel arms.
            Frame::SelectArm { form, env } => {
                let arm_index = match heap.car(v) {
                    Ok(Value::Int(n)) if n >= 0 => n as usize,
                    other => {
                        return Err(EvalError::Internal(format!(
                            "eval: (select ..) was answered with {:?}",
                            other
                        )))
                    }
                };
                let payload = heap.cdr(v).map_err(heap_err)?;
                let arm = core::field(heap, form, arm_index).ok_or_else(|| {
                    EvalError::Internal(format!("eval: (select ..) has no arm {}", arm_index))
                })?;
                let parts = heap.list_to_vec(arm).map_err(heap_err)?;
                let tag = match parts.first() {
                    Some(Value::Str(id)) => heap.string(*id).to_string(),
                    other => {
                        return Err(EvalError::Internal(format!(
                            "eval: (select ..) arm tag is {:?}",
                            other
                        )))
                    }
                };
                match tag.as_str() {
                    // The bound name sees the whole `Option<T>`: a closed
                    // channel is an answer, not a skipped arm.
                    "recv" => {
                        let Some(Value::Symbol(sym)) = parts.get(1).copied() else {
                            return Err(EvalError::Internal("eval: (select ..) recv arm has no name".into()));
                        };
                        let body = *parts.get(4).ok_or_else(|| {
                            EvalError::Internal("eval: (select ..) recv arm has no body".to_string())
                        })?;
                        let inner = extend_env(heap, &[(sym, payload)], env)?;
                        Ok((State::Eval(body, inner), None))
                    }
                    "send" => {
                        let body = *parts.get(4).ok_or_else(|| {
                            EvalError::Internal("eval: (select ..) send arm has no body".to_string())
                        })?;
                        Ok((State::Eval(body, env), None))
                    }
                    "else" => {
                        let body = *parts.get(1).ok_or_else(|| {
                            EvalError::Internal("eval: (select ..) else arm has no body".to_string())
                        })?;
                        Ok((State::Eval(body, env), None))
                    }
                    other => Err(EvalError::Internal(format!("eval: (select ..) arm tag `{}`", other))),
                }
            }

            Frame::DriveCompiled { drive, wake } => {
                Ok((State::CompiledResume { drive, wake: Some((v, wake)) }, None))
            }

            // The depth is restored *before* the return is reported, so the
            // two lines of one call line up.
            Frame::TracedCall { name, depth } => {
                self.trace_depth.set(depth);
                if self.step_quiet_depth.get() == depth {
                    self.step_quiet_depth.set(usize::MAX);
                }
                let text = self.trace_render(heap, v);
                self.trace_line(heap, depth, &format!("{} returned {}", name, text));
                Ok((State::Apply(v), None))
            }

            // Boxing is where the concrete type is still known, so it is
            // where every vtable this value could be viewed through gets
            // interned. That part is in `Interp::dyn_new_with_value`, next to
            // the table it interns into.
            Frame::DynNew { form } => Ok((State::Apply(self.dyn_new_with_value(heap, form, v)?), None)),
            Frame::DynUpcast { form } => Ok((State::Apply(self.dyn_upcast_with_value(heap, form, v)?), None)),

            // An unnamed callee — `((make-adder 1) 2)` — has no binding
            // keeping its box alive while the arguments allocate, so the
            // frame roots it.
            Frame::ApplyCallee { form, env, spawn } => {
                let args = arg_forms(heap, form, ArgsKind::Apply)?;
                match args.first() {
                    Some(&first) => Ok((
                        State::Eval(first, env),
                        Some(Frame::ApplyArgs { form, callee: v, done: Vec::new(), env, spawn }),
                    )),
                    None => self.finish_apply_args(heap, form, v, Vec::new(), spawn),
                }
            }
            Frame::ApplyArgs { form, callee, mut done, env, spawn } => {
                done.push(v);
                let args = arg_forms(heap, form, ArgsKind::Apply)?;
                match args.get(done.len()) {
                    Some(&next) => Ok((
                        State::Eval(next, env),
                        Some(Frame::ApplyArgs { form, callee, done, env, spawn }),
                    )),
                    None => self.finish_apply_args(heap, form, callee, done, spawn),
                }
            }

            // The scrutinee is rooted through the state slots for the whole
            // search, which matters because matching a `Sexpr` path pattern
            // allocates.
            Frame::MatchArms { form, env } => {
                let arms = core::fields(heap, form)
                    .map_err(|e| EvalError::Internal(format!("eval: (match ..): {}", e)))?;
                // The scrutinee, its representation (the bridge's), then the arms.
                for arm in arms.iter().skip(2) {
                    let parts = heap
                        .list_to_vec(*arm)
                        .map_err(|e| EvalError::Internal(format!("eval: (match ..) arm: {}", e)))?;
                    let Some((pat, body)) = parts.split_first() else {
                        return Err(EvalError::Internal("eval: (match ..) arm is empty".to_string()));
                    };
                    let Some(binds) = match_core_pattern(self, heap, env, *pat, v)? else {
                        continue;
                    };
                    // Both of these allocate, so the first result is rooted
                    // across the second. The root goes above this frame's, so
                    // the caller's truncation drops it.
                    let base = heap.root_count();
                    let new_env = extend_env(heap, &binds, env)?;
                    heap.push_root(new_env);
                    let body = core::list(heap, body)
                        .map_err(|e| EvalError::Internal(format!("eval: (match ..) arm body: {}", e)))?;
                    heap.truncate_roots(base);
                    // Nothing allocates from here: `sequence_state` only walks
                    // conses.
                    return sequence_state(heap, body, new_env);
                }
                Err(EvalError::Internal("eval: no matching match arm".to_string()))
            }

            // Field 2 is the field's representation, read by the bridge and
            // by nothing here.
            Frame::FieldSetObj { form, env } => {
                let idx = int_field(heap, form, 1, "field-set")? as usize;
                let val = core::field(heap, form, 3)
                    .ok_or_else(|| EvalError::Internal("eval: (field-set ..) has no value".to_string()))?;
                Ok((State::Eval(val, env), Some(Frame::FieldSetVal { obj: v, idx })))
            }
            Frame::FieldSetVal { obj, idx } => {
                let id = super::expect_struct_box(&obj)?;
                heap.struct_set_field(id, idx, v);
                Ok((State::Apply(Value::Empty), None))
            }

            // A global some compiled function reads or writes lives in a
            // permanent GC root instead of an ordinary cell, and must be
            // written through that same storage — otherwise an interpreted
            // read could see a stale value a compiled write already updated.
            Frame::SetGlobal { form } => {
                let written = self.name_list(heap, form, 0, "set-global")?;
                let home = self.name_list(heap, form, 1, "set-global")?;
                let path = path_field(heap, form, 2, "set-global")?;
                if let Some(&id) = self.compiled_globals.borrow().get(&path) {
                    let perm_idx = typelisp_rt::global_perm_idx(id).ok_or_else(|| {
                        EvalError::Internal(format!("global \"{}\": unknown compiled id {}", path, id))
                    })?;
                    heap.set_permanent_root(perm_idx, v);
                    return Ok((State::Apply(Value::Empty), None));
                }
                match self.resolve_global_named(&home, &written, &path) {
                    Some(slot) => {
                        slot.set(heap, v)?;
                        Ok((State::Apply(Value::Empty), None))
                    }
                    None => Err(EvalError::Unbound(path.to_string())),
                }
            }

            // A loop body's value is discarded and the body starts again. The
            // frame is rebuilt rather than kept, so the stack does not grow
            // with the iteration count.
            Frame::Loop { body, rest, env } => {
                let bad = |e| EvalError::Internal(format!("eval: (loop ..): {}", e));
                let next = heap.cdr(rest).map_err(bad)?;
                let rest = if matches!(next, Value::Empty) { body } else { next };
                let first = heap.car(rest).map_err(bad)?;
                Ok((State::Eval(first, env), Some(Frame::Loop { body, rest, env })))
            }

            // Reached without an exit: the body's value is the form's.
            Frame::Block { .. } | Frame::Catch { .. } => Ok((State::Apply(v), None)),

            Frame::Return => Ok((State::Unwind(EvalError::Return(Box::new(v))), None)),
            Frame::ReturnFrom { name } => Ok((State::Unwind(EvalError::ReturnFrom(name, Box::new(v))), None)),
            Frame::Throw { tag } => Ok((State::Unwind(EvalError::Throw(tag, Box::new(v))), None)),

            // The protected form finished normally. Its value waits in a
            // frame while the cleanup runs.
            Frame::Protect { cleanup, env } => {
                Ok((State::Eval(cleanup, env), Some(Frame::CleanupValue { value: v })))
            }
            // A cleanup's own value is discarded.
            Frame::CleanupValue { value } => Ok((State::Apply(value), None)),
            Frame::CleanupUnwind { pending } => Ok((State::Unwind(pending), None)),

            Frame::LetInit { binds, rest, mut done, body, env } => {
                done.push(v);
                let bad = |what: &str| EvalError::Internal(format!("eval: (let ..) {}", what));
                let next_rest = heap.cdr(rest).map_err(|_| bad("binding list is improper"))?;
                if !matches!(next_rest, Value::Empty) {
                    let b = heap.car(next_rest).map_err(|_| bad("binding list is improper"))?;
                    let (_, init) = bind_parts(heap, b)?;
                    return Ok((
                        State::Eval(init, env),
                        Some(Frame::LetInit { binds, rest: next_rest, done, body, env }),
                    ));
                }
                // Every initialiser is in. Pair the values back up with the
                // names, in binding order, and extend the environment.
                let mut pairs = Vec::with_capacity(done.len());
                let mut b = binds;
                for value in done {
                    let bind = heap.car(b).map_err(|_| bad("binding list is improper"))?;
                    let (sym, _) = bind_parts(heap, bind)?;
                    pairs.push((sym, value));
                    b = heap.cdr(b).map_err(|_| bad("binding list is improper"))?;
                }
                // The last allocation on this path: `sequence_state` only
                // walks conses, so the environment stays in a Rust local
                // until the caller roots it.
                let new_env = extend_env(heap, &pairs, env)?;
                sequence_state(heap, body, new_env)
            }
        }
    }

    /// Carries `exit` past `frame` — the unwinding half of a frame's job.
    ///
    /// Most frames have nothing to say and are simply discarded: that is what
    /// makes a non-local exit skip the intervening computation. Three claim an
    /// exit (`Loop`, `Block`, `Catch`) and one runs code on the way past
    /// (`Protect`).
    fn unwind_through(&self, heap: &mut Heap, frame: Frame, exit: EvalError) -> Result<(State, Option<Frame>), EvalError> {
        match frame {
            // `break` and `return` are caught by the nearest enclosing loop —
            // the checker guarantees there is one.
            Frame::Loop { .. } => match exit {
                EvalError::Break => Ok((State::Apply(Value::Empty), None)),
                EvalError::Return(v) => Ok((State::Apply(*v), None)),
                other => Ok((State::Unwind(other), None)),
            },

            Frame::Block { name } => match exit {
                EvalError::ReturnFrom(from, v) if from == name => Ok((State::Apply(*v), None)),
                other => Ok((State::Unwind(other), None)),
            },

            Frame::Catch { tag } => match exit {
                EvalError::Throw(thrown, v) if thrown == tag => Ok((State::Apply(*v), None)),
                other => Ok((State::Unwind(other), None)),
            },

            // The flags are restored however the body is left.
            Frame::Step { was_stepping, was_quiet } => {
                self.stepping.set(was_stepping);
                self.step_quiet_depth.set(was_quiet);
                Ok((State::Unwind(exit), None))
            }

            // An exit raised by an interpreted callee the chain applied
            // (C5). The chain has to be asked: a `catch` in it may claim the
            // throw, and an `unwind-protect` in it has cleanups to run. So
            // the exit is parked where the compiled side looks for it and
            // handed over as a status.
            //
            // A *suspension* cannot reach here — the only way out of
            // `Blocked` is the scheduler replacing the state with the value
            // waited for — so this frame being on the stack always means an
            // application, never a wait.
            Frame::DriveCompiled { drive, .. } => {
                crate::eval::crossing::park_for_compiled(exit);
                Ok((State::CompiledRaise { drive }, None))
            }

            // An unwind past a traced frame is exactly the moment a trace is
            // most worth having, so the exit is reported rather than silent.
            Frame::TracedCall { name, depth } => {
                self.trace_depth.set(depth);
                if self.step_quiet_depth.get() == depth {
                    self.step_quiet_depth.set(usize::MAX);
                }
                self.trace_line(heap, depth, &format!("{} exited non-locally: {}", name, exit));
                Ok((State::Unwind(exit), None))
            }

            // The cleanup runs on *every* way out, with the exit parked in a
            // frame until it is done.
            Frame::Protect { cleanup, env } => {
                Ok((State::Eval(cleanup, env), Some(Frame::CleanupUnwind { pending: exit })))
            }

            // An exit raised by a cleanup itself replaces the one that was in
            // flight (CLHS: the cleanup-forms are not protected by their own
            // unwind-protect). The pending exit is simply dropped.
            Frame::CleanupValue { .. } | Frame::CleanupUnwind { .. } => Ok((State::Unwind(exit), None)),

            // Everything else is computation the exit is escaping past.
            Frame::If { .. }
            | Frame::Panic
            | Frame::FieldGet { .. }
            | Frame::DynValue
            | Frame::Set { .. }
            | Frame::Seq { .. }
            | Frame::Return
            | Frame::ReturnFrom { .. }
            | Frame::Throw { .. }
            | Frame::Args { .. }
            | Frame::ApplyCallee { .. }
            | Frame::ApplyArgs { .. }
            | Frame::MatchArms { .. }
            | Frame::SelectArm { .. }
            | Frame::FieldSetObj { .. }
            | Frame::FieldSetVal { .. }
            | Frame::SetGlobal { .. }
            | Frame::DynNew { .. }
            | Frame::DynUpcast { .. }
            | Frame::LetInit { .. } => Ok((State::Unwind(exit), None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Reader;

    /// A heap in stress mode: every `cons` collects first, so a value a frame
    /// holds without rooting dies at the very next allocation rather than one
    /// run in a hundred. Every test here uses one — a continuation frame that
    /// forgets to root what it carries is the failure this rewrite is most
    /// likely to produce.
    fn stress_heap() -> Heap {
        let mut h = Heap::with_capacity(1 << 16);
        h.set_gc_stress(true);
        h
    }

    fn read1(heap: &mut Heap, src: &str) -> Value {
        let r = Reader::new();
        let mut vs = r.read_all(heap, src).expect("read failed");
        assert_eq!(vs.len(), 1, "expected exactly one form in {:?}", src);
        vs.pop().unwrap()
    }

    /// Evaluate a core form through the continuation machine.
    ///
    /// Asserts the root stack ends exactly where it started: a leaked root is
    /// not a crash but a value that can never be collected, and this is the
    /// only place it shows up.
    fn eval_src(heap: &mut Heap, src: &str) -> Result<Value, EvalError> {
        let form = read1(heap, src);
        heap.push_root(form);
        let before = heap.root_count();
        let interp = Interp::new();
        let out = interp.eval_cps(heap, form, Value::Empty);
        assert_eq!(heap.root_count(), before, "eval_cps leaked or over-popped roots on {:?}", src);
        out
    }

    /// Peels the location layers off an error. A runtime error is placed at
    /// the node it came from (`EvalError::At`), which is the right behaviour
    /// and not what these tests are checking.
    fn unlocated(e: &EvalError) -> &EvalError {
        match e {
            EvalError::At(_, inner) => unlocated(inner),
            other => other,
        }
    }

    fn eval_ok(heap: &mut Heap, src: &str) -> Value {
        eval_src(heap, src).unwrap_or_else(|e| panic!("{:?} failed to evaluate: {}", src, e))
    }

    /// The same form through both evaluators, asserting they agree. The whole
    /// premise of the rewrite is that it changes no program's meaning, so most
    /// tests here should be able to say exactly this.
    ///
    /// **Not for forms that produce a freshly allocated value.** `Value`
    /// compares strings and boxes by identity, and a string literal allocates
    /// a new string every time it is evaluated (deliberately — see
    /// `a_string_literal_is_a_fresh_string_every_time` in `core_eval`), so the
    /// two runs would differ on the id even when they agree on the content.
    /// Use `eval_ok` and compare what is inside.
    fn agrees(heap: &mut Heap, src: &str) -> Value {
        let form = read1(heap, src);
        heap.push_root(form);
        let interp = Interp::new();
        let old = interp.eval_core(heap, form, Value::Empty);
        let new = interp.eval_cps(heap, form, Value::Empty);
        match (old, new) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a, b, "the two evaluators disagree on {:?}", src);
                b
            }
            (Err(a), Err(b)) => panic!("both failed on {:?}: {} / {}", src, a, b),
            (a, b) => panic!("only one evaluator failed on {:?}: {:?} / {:?}", src, a, b),
        }
    }

    // ---- leaves ----------------------------------------------------------

    #[test]
    fn leaves_reach_a_value_without_a_frame() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, "(int-any-width 42)"), Value::Int(42));
        assert_eq!(agrees(&mut h, "(bool true)"), Value::Bool(true));
        assert_eq!(agrees(&mut h, r"(char #\a)"), Value::Char('a'));
        assert_eq!(agrees(&mut h, "(unit)"), Value::Empty);
        assert!(matches!(agrees(&mut h, "(sym foo)"), Value::Symbol(_)));
    }

    // ---- if: the first form that needs a frame ---------------------------

    #[test]
    fn if_takes_the_branch_the_condition_names() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, "(if (bool true) (int-any-width 1) (int-any-width 2))"), Value::Int(1));
        assert_eq!(agrees(&mut h, "(if (bool false) (int-any-width 1) (int-any-width 2))"), Value::Int(2));
    }

    /// The branch not taken is never evaluated — `panic` in the dead branch
    /// would be an error if it ran.
    #[test]
    fn the_branch_not_taken_does_not_run() {
        let mut h = stress_heap();
        assert_eq!(
            eval_ok(&mut h, r#"(if (bool true) (int-any-width 1) (panic (str "boom")))"#),
            Value::Int(1)
        );
    }

    /// A condition that is itself an `if` stacks a second frame while the
    /// first is still suspended. This is the property the whole design exists
    /// for, at depth 2.
    #[test]
    fn a_condition_that_is_itself_an_if_stacks_two_frames() {
        let mut h = stress_heap();
        let src = "(if (if (bool false) (bool false) (bool true)) (int-any-width 7) (int-any-width 9))";
        assert_eq!(agrees(&mut h, src), Value::Int(7));
    }

    /// Deeply nested conditions: 200 frames, each holding a form and an
    /// environment. Under `gc_stress` every step collects, so a frame that
    /// failed to root what it carries loses it here.
    #[test]
    fn nested_conditions_keep_their_frames_alive_under_collection() {
        let mut h = stress_heap();
        let mut src = String::from("(bool true)");
        for _ in 0..200 {
            src = format!("(if {} (bool true) (bool false))", src);
        }
        let src = format!("(if {} (int-any-width 1) (int-any-width 2))", src);
        assert_eq!(eval_ok(&mut h, &src), Value::Int(1));
    }

    /// A condition that is not a boolean is an internal error, not a silent
    /// branch — the checker should have made it impossible.
    #[test]
    fn a_non_boolean_condition_is_an_internal_error() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, "(if (int-any-width 1) (int-any-width 1) (int-any-width 2))")
            .expect_err("a non-boolean condition should not evaluate");
        assert!(matches!(unlocated(&e), EvalError::Internal(_)), "expected an internal error, got {:?}", e);
    }

    // ---- let, sequences, assignment --------------------------------------

    #[test]
    fn let_binds_and_the_body_sees_the_binding() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, "(let ((x r (int-any-width 5))) (var x))"), Value::Int(5));
    }

    /// Bindings are parallel: every initialiser runs in the *outer*
    /// environment, so the second cannot see the first.
    #[test]
    fn initialisers_run_in_the_outer_environment() {
        let mut h = stress_heap();
        let e = eval_src(
            &mut h,
            "(let ((x r (int-any-width 1)) (y r (var x))) (var y))",
        )
        .expect_err("the second initialiser must not see the first binding");
        assert!(matches!(unlocated(&e), EvalError::Unbound(_)), "expected Unbound, got {:?}", e);
    }

    /// `(let () E...)` is `progn`: every form runs, the last one's value wins.
    #[test]
    fn an_empty_binding_list_is_progn() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, "(let () (int-any-width 1) (int-any-width 2))"), Value::Int(2));
        assert_eq!(agrees(&mut h, "(let () (int-any-width 9))"), Value::Int(9));
        assert_eq!(agrees(&mut h, "(let ())"), Value::Empty);
    }

    /// `set` writes through the binding's cell and returns the value assigned.
    #[test]
    fn set_writes_through_the_cell_and_returns_the_value() {
        let mut h = stress_heap();
        let src = "(let ((x r (int-any-width 1))) (set x (int-any-width 42)) (var x))";
        assert_eq!(agrees(&mut h, src), Value::Int(42));
        let src = "(let ((x r (int-any-width 1))) (set x (int-any-width 42)))";
        assert_eq!(agrees(&mut h, src), Value::Int(42));
    }

    /// Many bindings, each initialiser allocating, with a collection between
    /// every one: a value bound early has to stay rooted while the later
    /// initialisers run.
    #[test]
    fn earlier_initialisers_survive_later_ones() {
        let mut h = stress_heap();
        let binds: String = (0..40)
            .map(|i| format!("(x{} r (str \"s{}\"))", i, i))
            .collect::<Vec<_>>()
            .join(" ");
        let src = format!("(let ({}) (var x0))", binds);
        match eval_ok(&mut h, &src) {
            Value::Str(id) => assert_eq!(h.string(id), "s0"),
            other => panic!("expected the first binding's string, got {:?}", other),
        }
    }

    /// Nested `let`s: 150 frames deep, each holding an environment, under
    /// collection at every allocation.
    #[test]
    fn deeply_nested_lets_keep_their_environments() {
        let mut h = stress_heap();
        let mut src = String::from("(var x0)");
        for i in (0..150).rev() {
            src = format!("(let ((x{} r (int-any-width {}))) {})", i, i, src);
        }
        assert_eq!(eval_ok(&mut h, &src), Value::Int(0));
    }

    /// A sequence's last form is a tail jump — no frame — so a body of any
    /// length costs the same stack as a body of one.
    #[test]
    fn a_long_body_does_not_grow_the_continuation_stack() {
        let mut h = stress_heap();
        let forms: String = (0..500)
            .map(|i| format!("(int-any-width {})", i))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(eval_ok(&mut h, &format!("(let () {})", forms)), Value::Int(499));
    }

    // ---- calls -----------------------------------------------------------

    /// A built-in operator: arguments left to right, then the operator.
    #[test]
    fn a_builtin_call_evaluates_its_arguments_then_applies() {
        let mut h = stress_heap();
        let v = agrees(
            &mut h,
            "(call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1) (int-any-width 2))",
        );
        assert_eq!(h.car(v).unwrap(), Value::Int(1));
        assert_eq!(h.cdr(v).unwrap(), Value::Int(2));
    }

    /// No arguments means no frame at all: the call is made straight from
    /// `step_cps`, which is the path a nullary builtin takes.
    #[test]
    fn a_call_with_no_arguments_needs_no_frame() {
        let mut h = stress_heap();
        let v = eval_ok(
            &mut h,
            "(call (make-random-state-fresh) () make-random-state-fresh ())",
        );
        assert!(matches!(v, Value::Boxed(_)), "expected a random-state box, got {:?}", v);
    }

    /// Nested calls: the inner one runs while the outer's argument frame is
    /// suspended, and the outer's earlier arguments have to survive it.
    #[test]
    fn an_argument_that_is_itself_a_call_suspends_the_outer_one() {
        let mut h = stress_heap();
        let src = "(call (sexpr-cons) () sexpr-cons (sexpr sexpr) \
                     (str \"first\") \
                     (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 1) (int-any-width 2)))";
        let v = agrees(&mut h, src);
        match h.car(v).unwrap() {
            Value::Str(id) => assert_eq!(h.string(id), "first"),
            other => panic!("the first argument did not survive the nested call: {:?}", other),
        }
    }

    /// Arguments are evaluated left to right, and each one stays rooted while
    /// the later ones allocate. 30 string arguments under `gc_stress`: if the
    /// frame dropped an early one, the list would come back wrong.
    #[test]
    fn earlier_arguments_survive_later_ones() {
        let mut h = stress_heap();
        let mut src = String::from("(unit)");
        for i in (0..30).rev() {
            src = format!(
                "(call (sexpr-cons) () sexpr-cons (sexpr sexpr) (str \"a{}\") {})",
                i, src
            );
        }
        let mut v = eval_ok(&mut h, &src);
        for i in 0..30 {
            match h.car(v).unwrap() {
                Value::Str(id) => assert_eq!(h.string(id), format!("a{}", i)),
                other => panic!("element {} is {:?}", i, other),
            }
            v = h.cdr(v).unwrap();
        }
    }

    // ---- construct / field-get -------------------------------------------

    #[test]
    fn construct_builds_a_struct_and_field_get_reads_it() {
        let mut h = stress_heap();
        let point = "(construct point \"point\" 0 true (int-any-width int-any-width) \
                       (int-any-width 1) (int-any-width 2))";
        assert_eq!(
            agrees(&mut h, &format!("(field-get {} 1 int-any-width)", point)),
            Value::Int(2)
        );
        assert_eq!(
            agrees(&mut h, &format!("(field-get {} 0 int-any-width)", point)),
            Value::Int(1)
        );
    }

    /// A field whose value is itself a construction: the outer node's earlier
    /// fields have to survive the inner one's allocation.
    #[test]
    fn a_nested_construction_does_not_lose_the_outer_fields() {
        let mut h = stress_heap();
        let inner = "(construct point \"point\" 0 true (int-any-width int-any-width) \
                       (int-any-width 3) (int-any-width 4))";
        let outer = format!(
            "(construct pair \"pair\" 0 true (str point) (str \"tag\") {})",
            inner
        );
        let src = format!("(field-get (field-get {} 1 point) 0 int-any-width)", outer);
        assert_eq!(agrees(&mut h, &src), Value::Int(3));
    }

    /// `set` on a struct field's binding, then reading it back: the value
    /// written has to be the value read, through the same cell.
    #[test]
    fn a_struct_survives_being_bound_and_read_back() {
        let mut h = stress_heap();
        let src = "(let ((p sexpr (construct point \"point\" 0 true (int-any-width int-any-width) \
                                    (int-any-width 5) (int-any-width 6))))
                     (field-get (var p) 1 int-any-width))";
        assert_eq!(agrees(&mut h, src), Value::Int(6));
    }

    // ---- labels / apply --------------------------------------------------

    #[test]
    fn a_lambda_is_applied_to_its_arguments() {
        let mut h = stress_heap();
        let src = "(apply (lambda ((x int-any-width)) int-any-width (var x)) int-any-width \
                     (int-any-width) (int-any-width 5))";
        assert_eq!(agrees(&mut h, src), Value::Int(5));
        assert_eq!(
            agrees(&mut h, "(apply (lambda () int-any-width (int-any-width 7)) int-any-width ())"),
            Value::Int(7)
        );
    }

    /// A closure sees what it captured, not what the caller has bound.
    #[test]
    fn a_closure_captures_its_defining_environment() {
        let mut h = stress_heap();
        let src = "(let ((n int-any-width (int-any-width 3)))
                     (apply (lambda () int-any-width (var n)) int-any-width ()))";
        assert_eq!(agrees(&mut h, src), Value::Int(3));
    }

    /// `labels` siblings see each other — the placeholder pass is what makes
    /// mutual recursion work.
    #[test]
    fn labels_siblings_can_call_each_other() {
        let mut h = stress_heap();
        let src = "(labels ((f ((x int-any-width)) int-any-width \
                              (apply (var g) int-any-width (int-any-width) (var x))) \
                            (g ((y int-any-width)) int-any-width (var y))) \
                     (apply (var f) int-any-width (int-any-width) (int-any-width 3)))";
        assert_eq!(agrees(&mut h, src), Value::Int(3));
    }

    /// A builtin used as a function value goes through the same dispatch a
    /// direct call site does — the `fnref` box carries only which name.
    #[test]
    fn a_builtin_can_be_applied_as_a_value() {
        let mut h = stress_heap();
        let src = "(apply (fnref (sexpr-cons) () sexpr-cons) sexpr (sexpr sexpr) \
                     (int-any-width 1) (int-any-width 2))";
        let v = agrees(&mut h, src);
        assert_eq!(h.car(v).unwrap(), Value::Int(1));
        assert_eq!(h.cdr(v).unwrap(), Value::Int(2));
    }

    // ---- the point of the whole rewrite ----------------------------------

    /// Builds `(cons 1 (cons 1 ... (cons 1 ())))` by recursion that is **not**
    /// in tail position: each call has to come back to cons its result.
    ///
    /// In the recursive evaluator this replaced, every one of those pending
    /// calls was a live Rust frame. **Measured before it was deleted**: the
    /// same form aborted with `fatal runtime error: stack overflow` at this
    /// depth — with the default test stack *and* with `RUST_MIN_STACK=32MB`, the value
    /// `scripts/test-serial.sh` sets. Here the pending calls are frames on a
    /// `Vec` and the Rust stack stays flat, which is the property a task needs
    /// in order to be suspended at all.
    ///
    /// No `gc_stress`: the point is depth, and collecting at every one of the
    /// allocations below would make this quadratic.
    #[test]
    fn deep_non_tail_recursion_does_not_touch_the_rust_stack() {
        const DEPTH: i64 = 20_000;
        let mut h = Heap::with_capacity(1 << 20);

        let src = format!(
            "(labels ((build ((n int-any-width)) sexpr
                        (if (call (sexpr-null) () sexpr-null (sexpr) (var n))
                            (unit)
                            (call (sexpr-cons) () sexpr-cons (sexpr sexpr)
                              (int-any-width 1)
                              (apply (var build) sexpr (sexpr)
                                (call (sexpr-cdr) () sexpr-cdr (sexpr) (var n)))))))
               (apply (var build) sexpr (sexpr) (var xs)))"
        );

        // `xs` is a list of DEPTH elements, built on the Rust side so the
        // source stays small.
        let mut xs = Value::Empty;
        h.push_root(xs);
        for _ in 0..DEPTH {
            xs = h.cons(Value::Int(0), xs).expect("heap exhausted building the input");
            h.set_root(0, xs);
        }
        let sym = match h.intern_symbol("xs") {
            Value::Symbol(id) => id,
            _ => unreachable!(),
        };
        let env = extend_env(&mut h, &[(sym, xs)], Value::Empty).expect("could not build the environment");
        h.push_root(env);

        let form = read1(&mut h, &src);
        h.push_root(form);

        let interp = Interp::new();
        let v = interp
            .eval_cps(&mut h, form, env)
            .expect("deep non-tail recursion failed");

        // The result is a list of DEPTH ones.
        let mut n = 0i64;
        let mut cur = v;
        while !matches!(cur, Value::Empty) {
            assert_eq!(h.car(cur).unwrap(), Value::Int(1));
            cur = h.cdr(cur).unwrap();
            n += 1;
        }
        assert_eq!(n, DEPTH, "the rebuilt list is the wrong length");
    }

    // ---- match -----------------------------------------------------------

    #[test]
    fn match_takes_the_first_arm_that_matches() {
        let mut h = stress_heap();
        let src = "(match (int-any-width 2) int-any-width \
                     ((pat-lit (int-any-width 1)) (int-any-width 10)) \
                     ((pat-lit (int-any-width 2)) (int-any-width 20)) \
                     ((pat-wild) (int-any-width 99)))";
        assert_eq!(agrees(&mut h, src), Value::Int(20));
    }

    #[test]
    fn a_wildcard_arm_catches_what_the_others_do_not() {
        let mut h = stress_heap();
        let src = "(match (int-any-width 7) int-any-width \
                     ((pat-lit (int-any-width 1)) (int-any-width 10)) \
                     ((pat-wild) (int-any-width 99)))";
        assert_eq!(agrees(&mut h, src), Value::Int(99));
    }

    /// A binding pattern extends the environment the arm's body runs in.
    #[test]
    fn a_binding_pattern_is_visible_in_the_arm() {
        let mut h = stress_heap();
        assert_eq!(
            agrees(&mut h, "(match (int-any-width 5) int-any-width ((pat-bind x) (var x)))"),
            Value::Int(5)
        );
    }

    /// An arm body of several forms runs in order, last value wins — and the
    /// bound value has to survive the earlier ones allocating.
    #[test]
    fn an_arm_body_is_a_sequence() {
        let mut h = stress_heap();
        let src = r#"(match (str "kept") string ((pat-bind x) (str "discarded") (var x)))"#;
        match eval_ok(&mut h, src) {
            Value::Str(id) => assert_eq!(h.string(id), "kept"),
            other => panic!("expected the bound string, got {:?}", other),
        }
    }

    // ---- field-set / set-global ------------------------------------------

    #[test]
    fn field_set_writes_through_the_struct() {
        let mut h = stress_heap();
        let src = "(let ((p sexpr (construct point \"point\" 0 true (int-any-width int-any-width) \
                                    (int-any-width 1) (int-any-width 2))))
                     (field-set (var p) 1 int-any-width (int-any-width 42))
                     (field-get (var p) 1 int-any-width))";
        assert_eq!(agrees(&mut h, src), Value::Int(42));
    }

    /// The object is evaluated before the value, and has to stay reachable
    /// while the value expression allocates.
    #[test]
    fn the_object_survives_an_allocating_value_expression() {
        let mut h = stress_heap();
        let src = r#"(let ((p sexpr (construct point "point" 0 true (int-any-width str)
                                      (int-any-width 1) (str "old"))))
                       (field-set (var p) 1 str (str "new"))
                       (field-get (var p) 1 str))"#;
        match eval_ok(&mut h, src) {
            Value::Str(id) => assert_eq!(h.string(id), "new"),
            other => panic!("expected the new string, got {:?}", other),
        }
    }

    // ---- panic -----------------------------------------------------------

    #[test]
    fn panic_carries_its_message() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, r#"(panic (str "boom"))"#).expect_err("panic should not produce a value");
        match unlocated(&e) {
            EvalError::Panic(m) => assert_eq!(m, "boom"),
            other => panic!("expected a panic, got {:?}", other),
        }
    }

    /// The message is evaluated, not taken literally.
    #[test]
    fn a_panic_message_is_a_computed_value() {
        let mut h = stress_heap();
        let e = eval_src(&mut h, r#"(let ((m r (str "late"))) (panic (var m)))"#)
            .expect_err("panic should not produce a value");
        match unlocated(&e) {
            EvalError::Panic(m) => assert_eq!(m, "late"),
            other => panic!("expected a panic, got {:?}", other),
        }
    }

    // ---- loop, break, return ---------------------------------------------

    /// Arithmetic lowers to method calls (`Op::Assoc`), which have not moved
    /// yet, so a loop here counts down a cons list with the `sexpr-*`
    /// builtins instead.
    fn countdown_list(n: usize) -> String {
        let mut src = String::from("(unit)");
        for i in (0..n).rev() {
            src = format!(
                "(call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width {}) {})",
                i, src
            );
        }
        src
    }

    #[test]
    fn a_loop_runs_until_it_is_broken_out_of() {
        let mut h = stress_heap();
        let src = format!(
            "(let ((lst r {}) (seen r (int-any-width 0)))
               (loop unit
                     (if (call (sexpr-null) () sexpr-null (sexpr) (var lst)) (break) (unit))
                     (set seen (call (sexpr-car) () sexpr-car (sexpr) (var lst)))
                     (set lst (call (sexpr-cdr) () sexpr-cdr (sexpr) (var lst))))
               (var seen))",
            countdown_list(5)
        );
        // The last element seen before the list ran out.
        assert_eq!(agrees(&mut h, &src), Value::Int(4));
    }

    /// `return` carries a value out of the nearest loop; `break` carries unit.
    #[test]
    fn return_carries_a_value_out_of_the_loop() {
        let mut h = stress_heap();
        // `loop`'s leading field is its own `Repr` -- positional, so unlike
        // `catch`'s trailing one it cannot be left out of a hand-written form.
        assert_eq!(agrees(&mut h, "(loop int-any-width (return (int-any-width 7)))"), Value::Int(7));
        assert_eq!(agrees(&mut h, "(loop unit (break))"), Value::Empty);
        assert_eq!(agrees(&mut h, "(loop unit (return))"), Value::Empty);
    }

    /// The *nearest* loop claims the exit, so an inner `break` leaves the
    /// outer one running.
    #[test]
    fn break_leaves_only_the_nearest_loop() {
        let mut h = stress_heap();
        // The inner `(loop (break))` must not end the outer one, which walks
        // the whole list.
        let src = format!(
            "(let ((lst r {}) (seen r (int-any-width 0)))
               (loop unit
                     (loop unit (break))
                     (if (call (sexpr-null) () sexpr-null (sexpr) (var lst)) (break) (unit))
                     (set seen (call (sexpr-car) () sexpr-car (sexpr) (var lst)))
                     (set lst (call (sexpr-cdr) () sexpr-cdr (sexpr) (var lst))))
               (var seen))",
            countdown_list(3)
        );
        assert_eq!(agrees(&mut h, &src), Value::Int(2));
    }

    /// A loop of any length costs one frame: it is rebuilt, not stacked. 300
    /// iterations, each allocating, with a collection at every allocation.
    #[test]
    fn a_long_loop_does_not_grow_the_stack() {
        let mut h = stress_heap();
        let src = format!(
            "(let ((lst r {}) (seen r (int-any-width 0)))
               (loop unit
                     (if (call (sexpr-null) () sexpr-null (sexpr) (var lst)) (break) (unit))
                     (set seen (call (sexpr-car) () sexpr-car (sexpr) (var lst)))
                     (set lst (call (sexpr-cdr) () sexpr-cdr (sexpr) (var lst))))
               (var seen))",
            countdown_list(300)
        );
        assert_eq!(eval_ok(&mut h, &src), Value::Int(299));
    }

    // ---- block / return-from ---------------------------------------------

    #[test]
    fn return_from_leaves_the_named_block() {
        let mut h = stress_heap();
        assert_eq!(
            agrees(&mut h, r#"(block (str "b") (let () (return-from (str "b") (int-any-width 3)) (int-any-width 9)))"#),
            Value::Int(3)
        );
    }

    /// A block reached without an exit is just its body.
    #[test]
    fn a_block_without_an_exit_is_its_body() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, r#"(block (str "b") (int-any-width 4))"#), Value::Int(4));
    }

    /// The exit passes through the inner block, which does not claim it.
    #[test]
    fn return_from_passes_through_blocks_of_other_names() {
        let mut h = stress_heap();
        let src = r#"(block (str "outer")
                       (block (str "inner")
                         (return-from (str "outer") (int-any-width 8))))"#;
        assert_eq!(agrees(&mut h, src), Value::Int(8));
    }

    // ---- catch / throw ---------------------------------------------------

    #[test]
    fn a_catch_without_a_throw_is_its_body() {
        let mut h = stress_heap();
        assert_eq!(agrees(&mut h, "(catch (quote done) (int-any-width 3))"), Value::Int(3));
    }

    #[test]
    fn a_throw_reaches_the_catch_on_its_tag() {
        let mut h = stress_heap();
        let src = "(catch (quote done) (let () (throw (quote done) (int-any-width 42)) (int-any-width 9)))";
        assert_eq!(agrees(&mut h, src), Value::Int(42));
    }

    /// A throw on another tag keeps travelling — the bug CL's `eq` tag
    /// comparison exists to prevent.
    #[test]
    fn a_throw_passes_through_a_catch_on_another_tag() {
        let mut h = stress_heap();
        let src = "(catch (quote outer)
                     (catch (quote inner)
                       (throw (quote outer) (int-any-width 5))))";
        assert_eq!(agrees(&mut h, src), Value::Int(5));
    }

    /// The thrown value stays rooted for the whole flight. Under `gc_stress`
    /// the cleanup on the way out collects, so a value the state slots failed
    /// to root would be gone by the time the catch claims it — which is what
    /// the interpreter does instead of parking it in a dedicated slot.
    #[test]
    fn a_thrown_value_survives_a_cleanup_that_allocates() {
        let mut h = stress_heap();
        let src = r#"(catch (quote done)
                       (unwind-protect
                         (throw (quote done) (str "carried"))
                         (str "the cleanup allocates")))"#;
        match eval_ok(&mut h, src) {
            Value::Str(id) => assert_eq!(h.string(id), "carried"),
            other => panic!("the thrown value did not survive the cleanup: {:?}", other),
        }
    }

    // ---- unwind-protect --------------------------------------------------

    /// The cleanup runs and its value is discarded.
    #[test]
    fn unwind_protect_returns_the_protected_value() {
        let mut h = stress_heap();
        let src = "(let ((n r (int-any-width 0)))
                     (unwind-protect (int-any-width 1) (set n (int-any-width 99)))
                     (var n))";
        assert_eq!(agrees(&mut h, src), Value::Int(99));
        assert_eq!(
            agrees(&mut h, "(unwind-protect (int-any-width 1) (int-any-width 2))"),
            Value::Int(1)
        );
    }

    /// The cleanup runs however the protected form is left — here by `break`.
    #[test]
    fn the_cleanup_runs_on_a_break_out_of_the_protected_form() {
        let mut h = stress_heap();
        let src = "(let ((n r (int-any-width 0)))
                     (loop unit (unwind-protect (break) (set n (int-any-width 7))))
                     (var n))";
        assert_eq!(agrees(&mut h, src), Value::Int(7));
    }

    /// The protected form's value survives a cleanup that allocates — the one
    /// unwind that runs code before continuing.
    #[test]
    fn the_protected_value_survives_an_allocating_cleanup() {
        let mut h = stress_heap();
        let src = r#"(unwind-protect (str "kept") (str "the cleanup allocates"))"#;
        match eval_ok(&mut h, src) {
            Value::Str(id) => assert_eq!(h.string(id), "kept"),
            other => panic!("the protected value did not survive: {:?}", other),
        }
    }

    /// Nested `unwind-protect`s run innermost first: the trail is consed in
    /// the order the cleanups ran, so the outer one ends up at the head.
    #[test]
    fn nested_cleanups_run_innermost_first() {
        let mut h = stress_heap();
        let src = r#"(let ((trail r (unit)))
                       (catch (quote done)
                         (unwind-protect
                           (unwind-protect
                             (throw (quote done) (int-any-width 0))
                             (set trail (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (str "i") (var trail))))
                           (set trail (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (str "o") (var trail)))))
                       (var trail))"#;
        let v = eval_ok(&mut h, src);
        let head = h.car(v).unwrap();
        let second = h.car(h.cdr(v).unwrap()).unwrap();
        match (head, second) {
            (Value::Str(a), Value::Str(b)) => {
                assert_eq!(h.string(a), "o", "the outer cleanup should have run last");
                assert_eq!(h.string(b), "i", "the inner cleanup should have run first");
            }
            other => panic!("expected two strings, got {:?}", other),
        }
    }

    /// An exit raised by the cleanup itself wins over the one in flight
    /// (CLHS: the cleanup-forms are not protected by their own
    /// unwind-protect).
    #[test]
    fn an_exit_from_the_cleanup_beats_the_one_in_flight() {
        let mut h = stress_heap();
        let src = "(catch (quote outer)
                     (catch (quote inner)
                       (unwind-protect
                         (throw (quote inner) (int-any-width 1))
                         (throw (quote outer) (int-any-width 2)))))";
        assert_eq!(agrees(&mut h, src), Value::Int(2));
    }

    // ---- tasks -----------------------------------------------------------

    /// Runs `srcs` as independent tasks on the real scheduler, switching after
    /// every single step.
    ///
    /// Every task's roots have to survive whatever the others do between two of
    /// its own steps, and under `stress_heap` that includes a collection per
    /// `cons`. This is the harshest arrangement the scheduler will ever see.
    ///
    /// `Interp::drive` is the driver here — the same one `eval_cps` uses — so
    /// what these tests exercise is the loop that ships. It returns when *its*
    /// task finishes, and the others keep running meanwhile, so a task named
    /// later may already be `Done` by the time its turn comes.
    fn run_interleaved(heap: &mut Heap, srcs: &[&str]) -> Vec<Result<Value, EvalError>> {
        let base = heap.root_count();
        let interp = Interp::new();
        interp.scheduler.borrow_mut().switch_every_step = true;

        let home = heap.current_root_stack();
        let mut ids = Vec::new();
        for src in srcs {
            let form = read1(heap, src);
            heap.push_root(form);
            let roots = heap.new_root_stack();
            // `form` is rooted in `home`, which the collector still walks.
            heap.switch_to_root_stack(roots);
            let task = Task::start(heap, form, Value::Empty);
            heap.switch_to_root_stack(home);
            ids.push(interp.scheduler.borrow_mut().admit(TaskSlot { task, roots }));
        }

        let mut out = Vec::new();
        for id in ids {
            let done = interp.scheduler.borrow().done_value(id);
            let r = match done {
                // It finished while another task was being driven, and its
                // value is rooted in the scheduler's own stack.
                Some(v) => Ok(v),
                None => interp.drive(heap, id),
            };
            // `drive` hands its result back unrooted — its task's stack is
            // gone and nothing keeps it for a `wait` — so the starter roots it,
            // which is the convention every caller of an evaluation follows.
            // An *exit* carries a value the same way a normal return does.
            match &r {
                Ok(v) => heap.push_root(*v),
                Err(EvalError::Return(v)) | Err(EvalError::ReturnFrom(_, v)) | Err(EvalError::Throw(_, v)) => {
                    heap.push_root(**v)
                }
                Err(_) => {}
            }
            out.push(r);
        }

        // The forms it was handed are still rooted, and each result that
        // carries a value added one of its own.
        assert!(
            heap.root_count() >= base + srcs.len(),
            "the scheduler dropped roots it was given ({} < {})",
            heap.root_count(),
            base + srcs.len()
        );
        out
    }

    /// Two tasks stepped alternately each reach their own answer.
    #[test]
    fn two_tasks_interleaved_keep_their_own_bindings() {
        let mut h = stress_heap();
        let out = run_interleaved(
            &mut h,
            &[
                "(let ((x r (int-any-width 1))) (var x))",
                "(let ((x r (int-any-width 2))) (var x))",
            ],
        );
        assert_eq!(out[0].as_ref().unwrap(), &Value::Int(1));
        assert_eq!(out[1].as_ref().unwrap(), &Value::Int(2));
    }

    /// A task's half-built structure survives the other task's allocations.
    ///
    /// Each `cons` collects under `gc_stress` and the two tasks take turns, so
    /// every cell one task is holding in an `Args` frame is exposed to a
    /// collection driven by the other before it is used.
    #[test]
    fn a_half_built_structure_survives_the_other_task() {
        let mut h = stress_heap();
        let build = |tag: i64| {
            format!(
                "(call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width {}) \
                   (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width {}) (unit)))",
                tag,
                tag + 1
            )
        };
        let (a, b) = (build(11), build(22));
        let out = run_interleaved(&mut h, &[&a, &b]);
        for (i, expect) in [(0usize, 11i64), (1, 22)] {
            let v = *out[i].as_ref().unwrap_or_else(|e| panic!("task {} failed: {}", i, e));
            assert_eq!(h.car(v).unwrap(), Value::Int(expect));
            assert_eq!(h.car(h.cdr(v).unwrap()).unwrap(), Value::Int(expect + 1));
        }
    }

    /// One task failing does not disturb the others: each result stands alone.
    #[test]
    fn a_failing_task_does_not_take_the_others_with_it() {
        let mut h = stress_heap();
        let out = run_interleaved(
            &mut h,
            &["(int-any-width 1)", "(var nope)", "(int-any-width 3)"],
        );
        assert_eq!(out[0].as_ref().unwrap(), &Value::Int(1));
        match &out[1] {
            Err(e) => assert!(matches!(unlocated(e), EvalError::Unbound(n) if n == "nope"), "got {}", e),
            Ok(v) => panic!("expected an unbound-variable error, got {:?}", v),
        }
        assert_eq!(out[2].as_ref().unwrap(), &Value::Int(3));
    }

    /// A thrown value is still alive when it reaches the starter, even though
    /// the stack that carried it is dropped the moment the task ends — so
    /// rooting it on arrival, as any caller of an evaluation does, is enough.
    #[test]
    fn a_task_that_throws_hands_its_value_over_rooted() {
        let mut h = stress_heap();
        let out = run_interleaved(
            &mut h,
            &[
                "(throw (quote escaped) \
                   (call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 5) (unit)))",
                "(call (sexpr-cons) () sexpr-cons (sexpr sexpr) (int-any-width 6) (unit))",
            ],
        );
        // Allocating now would collect an unrooted carried value.
        for _ in 0..64 {
            let _ = h.cons(Value::Int(0), Value::Empty).unwrap();
        }
        match &out[0] {
            Err(e) => match unlocated(e) {
                EvalError::Throw(tag, v) => {
                    assert_eq!(tag, "escaped");
                    assert_eq!(h.car(**v).unwrap(), Value::Int(5));
                }
                other => panic!("expected a throw, got {:?}", other),
            },
            Ok(v) => panic!("expected a throw, got {:?}", v),
        }
    }

    // ---- the vocabulary is closed ----------------------------------------

    /// Every `Op` is handled: `step_cps`'s match has no catch-all, so the
    /// compiler is the one holding this. Adding a tag to `Op` without giving
    /// it a frame is a build error, not a run-time surprise.
    ///
    /// This test exists to say so where a reader looks for it; there is
    /// nothing to assert at run time.
    #[test]
    fn every_op_has_a_frame() {
        let mut h = stress_heap();
        // A form of each shape, as a smoke test that the arms are wired at all.
        assert_eq!(eval_ok(&mut h, "(int-any-width 1)"), Value::Int(1));
        assert_eq!(eval_ok(&mut h, "(if (bool true) (int-any-width 1) (int-any-width 2))"), Value::Int(1));
        assert_eq!(eval_ok(&mut h, "(let ((x r (int-any-width 3))) (var x))"), Value::Int(3));
        assert_eq!(eval_ok(&mut h, "(loop int-any-width (return (int-any-width 4)))"), Value::Int(4));
        assert_eq!(eval_ok(&mut h, "(catch (quote t) (throw (quote t) (int-any-width 5)))"), Value::Int(5));
    }
}
