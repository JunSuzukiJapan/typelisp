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

use typelisp_mem::{Heap, SymRef, Value};
use typelisp_rt::sched::{self, chan_id_of, io_deadline, io_wait, task_id_of, ChanOp, Progress, SchedError, SelectOp, Waiting, Wake};

use crate::check::core;
use crate::check::repr::Repr;
use crate::eval::value::EvalError;

use super::core_eval::{
    bool_field, construct_sexpr_core, env_lookup, extend_env, heap_err, int_field, match_core_pattern, path_field,
    param_reprs, repr_list, str_field, sym_field, tail_after, tail_after_value, Op,
};
use super::{FnDef, Interp};

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
    /// One slot per task, not one for the whole heap — see `RootStack`'s
    /// `in_flight_throw` field — is what lets two tasks be unwinding on the
    /// compiled side at once without one's parked value overwriting the
    /// other's (`docs/dev/os-threads-design.md` §3).
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
    /// What the chain is entered with — see [`DriveCallee`].
    callee: DriveCallee,
    /// The callee's declared parameter representations — what the words the
    /// arguments encode to mean.
    params: Vec<Repr>,
    /// The callee's declared return representation — what the final word
    /// means. Nothing else can say: the driver hands raw bits through.
    ret: Repr,
    /// The trace line owed on the way back, if `trace` is on for this callee.
    watch: Option<(String, usize, Option<Repr>)>,
}

/// What a [`State::CompiledEnter`] drive is entered with.
///
/// Both a named call and an `apply` of a compiled closure — including the
/// closure a task `go` started, which used to be its own `State` — are one
/// task-driven crossing now: the difference is only how the chain's entry
/// point is found. Neither carries a `Value` here — `DriveCtx` holds none,
/// on purpose (its own doc comment) — so a `Closure` rides at the head of
/// `CompiledEnter`'s own `argv` list, tagged like any other argument, and is
/// rooted exactly as its sibling arguments are (through the state slots,
/// `set_state`'s `State::CompiledEnter` arm).
enum DriveCallee {
    /// A named top-level body — held for as long as the drive lasts, so the
    /// machine code cannot be dropped underneath a suspended chain by a
    /// redefinition.
    Body(std::rc::Rc<dyn super::CompiledBody>),
    /// A closure value: `params` is prefixed with [`Repr::Fn`] for it, and
    /// its tagged word is `CompiledEnter`'s first argument.
    Closure,
    /// A raw coroutine-ABI entry address, taking no arguments — what an
    /// `eval`-carrying AOT program's own initialisers and `main` are, once
    /// its `Interp` drives them (`Interp::run_compiled_program`). Held as a
    /// bare address rather than an `Rc<dyn CompiledBody>` because there is
    /// no `FnDef`/JIT-owned body wrapping it: `AOT_ENV`'s `Interp` outlives
    /// the process, the same immortality `rt_eval_init`'s leak gives the
    /// dump it restores from, so nothing needs to keep this alive.
    Address(usize),
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
    /// `(untag-int E)`: check that the value is a fixnum, keep it.
    UntagInt,
    /// `(box-option KEY E)` — waiting on the value; `key` is the `Option`
    /// instantiation's identity, as the form's own string.
    BoxOption { key: Value },
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
    /// A traced call's exit line: the name, the depth to restore, and the
    /// declared return representation the value is rendered by (a niched
    /// `Option` prints as one only if the renderer is told).
    TracedCall { name: String, depth: usize, ret: Option<Repr> },
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
            Frame::Panic | Frame::FieldGet { .. } | Frame::DynValue | Frame::UntagInt => {}
            Frame::BoxOption { key } => heap.push_root(*key),
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
///
/// This is the interpreter's [`sched::TaskBody`]: the scheduler in
/// `typelisp_rt` owns the queue and the waits, and this owns how one step
/// runs. The other body is `typelisp_rt::sched::CompiledTask`, a compiled
/// chain and nothing else, which is what an AOT executable's tasks are.
pub(crate) struct Task {
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
    /// This task's own share of `typelisp_rt`'s in-flight-unwind
    /// thread-locals, installed before this task's own step of
    /// [`Interp::step_task`] runs and taken back once it stops — the
    /// interpreted-side counterpart of `sched::CompiledTask`'s field of the
    /// same name (`docs/dev/os-threads-design.md` §3). Needed here too:
    /// `step_task` can enter a compiled chain (`State::CompiledEnter` and
    /// friends), which parks a throw's tag/caught-unwind through the same
    /// thread-locals a `CompiledTask` does.
    parked_unwind: typelisp_rt::ParkedUnwind,
}

impl sched::TaskBody for Task {
    type Cx = Interp;
    type Error = EvalError;

    fn start_closure(heap: &mut Heap, closure: Value) -> Task {
        let sbase = heap.root_count();
        // A one-element argument list holding only the closure itself — the
        // `go` site's thunk takes no arguments of its own and answers with
        // its result already tagged (`core_bridge::translate_go`), so this
        // crossing has no other representations to marshal by.
        heap.push_root(closure);
        let argv = core::list(heap, &[closure]).unwrap_or_else(|e| {
            typelisp_abi::fatal(&format!("go: building the closure's argument list failed: {}", e))
        });
        heap.pop_root();
        heap.push_root(argv);
        heap.push_root(Value::Empty);
        let start = DriveStart { callee: DriveCallee::Closure, params: vec![Repr::Fn], ret: Repr::Sexpr, watch: None };
        Task {
            sbase,
            stack: CpsStack::new(),
            state: State::CompiledEnter { argv, start },
            compiled: typelisp_rt::coroutine::FrameStack::new(),
            parked_unwind: typelisp_rt::ParkedUnwind::default(),
        }
    }

    fn sbase(&self) -> usize {
        self.sbase
    }

    fn step(&mut self, heap: &mut Heap, interp: &Interp) -> Progress<EvalError> {
        interp.step_task_isolated(heap, self)
    }

    /// Gives this task the state it will resume into, rooting what that
    /// state carries in the task's own stack (the caller made it current).
    fn deliver(&mut self, heap: &mut Heap, answer: Result<Value, SchedError>) {
        let state = answer_state(answer);
        set_state(heap, self.sbase, &state);
        self.state = state;
    }

    fn failure_left_task(e: EvalError) -> EvalError {
        escaped_task_failure(e)
    }
}

impl From<SchedError> for EvalError {
    fn from(e: SchedError) -> EvalError {
        match e {
            SchedError::Panic(m) => EvalError::Panic(m),
            SchedError::Internal(m) => EvalError::Internal(m),
        }
    }
}

/// The state a task resumes into, given what the scheduler answered it with.
fn answer_state(answer: Result<Value, SchedError>) -> State {
    match answer {
        Ok(v) => State::Apply(v),
        Err(e) => State::Unwind(e.into()),
    }
}

/// How a compiled frame reads its wake value, as the representation the
/// crossing code speaks: unit is a word the collector never follows, and a
/// typed answer is a tagged `Sexpr` whatever its type.
fn wake_repr(w: Wake) -> Repr {
    match w {
        Wake::Unit => Repr::Unit,
        Wake::Tagged => Repr::Sexpr,
    }
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
            parked_unwind: typelisp_rt::ParkedUnwind::default(),
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
            parked_unwind: typelisp_rt::ParkedUnwind::default(),
        }
    }

    /// Starts a task that enters a raw coroutine-ABI address with no
    /// arguments — an `eval`-carrying AOT program's own initialiser or
    /// entry point, driven by [`Interp::run_compiled_program`].
    fn start_compiled_entry(heap: &mut Heap, address: usize, ret: Repr) -> Task {
        let sbase = heap.root_count();
        heap.push_root(Value::Empty);
        heap.push_root(Value::Empty);
        let start = DriveStart { callee: DriveCallee::Address(address), params: Vec::new(), ret, watch: None };
        Task {
            sbase,
            stack: CpsStack::new(),
            state: State::CompiledEnter { argv: Value::Empty, start },
            compiled: typelisp_rt::coroutine::FrameStack::new(),
            parked_unwind: typelisp_rt::ParkedUnwind::default(),
        }
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
        State::Blocked(Waiting::Chan(ChanOp::Send(_, v))) | State::Blocked(Waiting::Spawn(v)) => {
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
        if self.scheduler.borrow().driving() {
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
        // outlive it: `sched::drive` returns as soon as main is done, and
        // whatever is left keeps its state for the next time the scheduler
        // runs — which is what makes `(go ...)` at a REPL prompt behave.
        // `form` and `env` are rooted by the caller, so they survive the
        // switch to the task's own root stack that `admit` makes.
        let main = self.scheduler.borrow_mut().admit(heap, |heap| Task::start(heap, form, env));

        self.scheduler.borrow_mut().set_driving(true);
        let out = sched::drive(&self.scheduler, heap, self, main);
        self.scheduler.borrow_mut().set_driving(false);
        out
    }

    /// Runs an `eval`-carrying AOT program's own initialisers and entry
    /// point, each as the main task of one drive of *this* `Interp`'s
    /// scheduler — the shape `eval_cps` gives one evaluation, so a program
    /// that calls `eval` and the tasks its own `(go ...)`s admit share one
    /// scheduler, one queue, one clock, rather than the AOT scheduler and
    /// the `Interp`'s running as two that never see each other.
    ///
    /// Unlike `eval_cps` this is only ever the outermost call — the
    /// generated `main` of an `eval`-carrying executable calls
    /// [`crate::shim::rt_run_program_interp`] once, from Rust, with nothing
    /// else on this thread's stack — so `driving()` starts `false` and this
    /// sets it for the whole run rather than checking it first.
    ///
    /// `inits` and `entry` are raw `coroutine_fn_type` addresses out of the
    /// executable's own generated code (`compile::aot::build_main_wrapper`),
    /// each entered with no arguments; `entry_ret` is `entry`'s declared
    /// return representation (`Repr::Int` for an `int`-returning `main`,
    /// `Repr::Unit` otherwise — `compile-file` already knows which). Each
    /// initialiser's own answer is `rt_global_new`'s raw storage id, which
    /// the body has already stored and nothing here reads back — decoded as
    /// `Repr::Unit`, which reads no bits at all.
    pub fn run_compiled_program(
        &self,
        heap: &mut Heap,
        inits: &[usize],
        entry: usize,
        entry_ret: Repr,
    ) -> Result<Value, EvalError> {
        self.scheduler.borrow_mut().set_driving(true);
        let mut run = |address: usize, ret: Repr| {
            let id = self.scheduler.borrow_mut().admit(heap, |heap| Task::start_compiled_entry(heap, address, ret));
            sched::drive(&self.scheduler, heap, self, id)
        };
        let result = (|| {
            for &address in inits {
                run(address, Repr::Unit)?;
            }
            run(entry, entry_ret)
        })();
        self.scheduler.borrow_mut().set_driving(false);
        result
    }

    /// Runs one evaluation to its end on the caller's root stack, with no
    /// scheduling. What a nested `eval_cps` does.
    fn run_to_completion(&self, heap: &mut Heap, form: Value, env: Value) -> Result<Value, EvalError> {
        let mut task = Task::start(heap, form, env);
        let out = loop {
            match self.step_task_isolated(heap, &mut task) {
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
                    // A `(yield)` has nobody to yield to here, and simply
                    // goes on — the same answer a compiled safepoint gets on
                    // a machine frame. Refusing it would make "give up the
                    // rest of the turn" an error exactly where there is no
                    // turn to give up.
                    let answered = if matches!(w, Waiting::Yield) {
                        Some(Ok(Value::Empty))
                    } else {
                        self.scheduler.borrow_mut().try_now(heap, &w)
                    };
                    match answered {
                        Some(answer) => {
                            let next = answer_state(answer);
                            set_state(heap, task.sbase, &next);
                            task.state = next;
                        }
                        None => break Err(EvalError::Panic(sched::cannot_block_message(&w))),
                    }
                }
            }
        };
        heap.truncate_roots(task.sbase);
        out
    }

    /// [`Self::step_task`], with `typelisp_rt`'s in-flight-unwind
    /// thread-locals saved and restored around it so `task`'s own state
    /// (`Task::parked_unwind`) is what a `catch`'s dispatch or an
    /// `unwind-protect`'s resume sees, not whatever a *different* task last
    /// left there — see `docs/dev/os-threads-design.md` §3.
    ///
    /// Both `TaskBody::step`'s call and `run_to_completion`'s nested one go
    /// through this rather than `step_task` directly: the save/restore is
    /// correct either way because it follows the Rust call stack — a nested
    /// call installs its own task's (typically empty) state, runs, and
    /// restores what the outer call had installed, exactly as if the two
    /// were unrelated threads taking turns on the same thread-locals.
    fn step_task_isolated(&self, heap: &mut Heap, task: &mut Task) -> Progress<EvalError> {
        typelisp_rt::restore_parked_unwind(std::mem::take(&mut task.parked_unwind));
        let progress = self.step_task(heap, task);
        task.parked_unwind = typelisp_rt::take_parked_unwind();
        progress
    }

    /// Runs `task` one step.
    ///
    /// A step is one `Eval` decomposition, one frame resumed, or one frame
    /// unwound through. Between two calls the task holds everything it needs to
    /// continue, so a scheduler may leave it alone and step a different one —
    /// that is what makes a task suspendable.
    fn step_task(&self, heap: &mut Heap, task: &mut Task) -> Progress<EvalError> {
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
                        let outcome = match &drive.start.callee {
                            // SAFETY: the address is a symbol the backend
                            // resolved out of a module that defines it,
                            // declared under `coroutine_fn_type` — the same
                            // provenance every indirect compiled call relies
                            // on, and `body_abi` is what said it was this one.
                            DriveCallee::Body(body) => {
                                let f: typelisp_rt::coroutine::CoroutineFn = unsafe {
                                    std::mem::transmute::<usize, typelisp_rt::coroutine::CoroutineFn>(body.address())
                                };
                                let stack = &mut task.compiled;
                                crate::eval::crossing::catch_compiled_panic(|| stack.run(heap, f, &args))
                            }
                            // SAFETY: `Address`'s only producer
                            // (`Interp::run_compiled_program`) reads it out of
                            // an AOT executable's own embedded initialiser
                            // array and entry point, declared under
                            // `coroutine_fn_type` by `compile-file`'s
                            // generator — the same provenance `rt_run_program`
                            // trusts on the AOT-only side of this boundary.
                            DriveCallee::Address(addr) => {
                                let f: typelisp_rt::coroutine::CoroutineFn =
                                    unsafe { std::mem::transmute::<usize, typelisp_rt::coroutine::CoroutineFn>(*addr) };
                                let stack = &mut task.compiled;
                                crate::eval::crossing::catch_compiled_panic(|| stack.run(heap, f, &args))
                            }
                            // The closure rides at the head of `args`, tagged
                            // like any other argument (`Repr::Fn` is first in
                            // `start.params`); everything after it is the
                            // call's own arguments.
                            DriveCallee::Closure => {
                                let Some((&closure, rest)) = args.split_first() else {
                                    heap.truncate_roots(roots_on_entry);
                                    return Progress::Done(Err(EvalError::Internal(
                                        "eval: a compiled closure entry carries no closure".to_string(),
                                    )));
                                };
                                let stack = &mut task.compiled;
                                crate::eval::crossing::catch_compiled_panic(|| stack.run_closure(heap, closure, rest))
                            }
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
        if let State::Blocked(w @ (Waiting::Chan(_) | Waiting::Select { .. } | Waiting::Spawn(_))) = &next {
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
                    if let Some((name, depth, ret)) = &drive.start.watch {
                        self.trace_call_exit(heap, name, *depth, ret.as_ref(), Ok(v));
                    }
                    State::Apply(v)
                }
                Err(e) => State::Unwind(e),
            },
            // The chain stays exactly as it is, rooted by the prologues that
            // built it. The frame pushed here is what catches the value the
            // scheduler eventually wakes this task with.
            Ok(Err(typelisp_rt::coroutine::Paused::Suspended)) => match sched::pending_wait(heap) {
                Ok((w, wake)) => {
                    task.stack.push(heap, Frame::DriveCompiled { drive, wake: wake_repr(wake) }, None);
                    State::Blocked(w)
                }
                Err(e) => State::Unwind(e.into()),
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
            | Op::Integer
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
                // Field 0 is the result representation, read by the bridge and
                // by nothing here.
                let call = core::field(heap, form, 1)
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
            Op::UntagInt => {
                let inner = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (untag-int ..) has no operand".to_string()))?;
                stack.push(heap, Frame::UntagInt, loc.clone());
                Ok(State::Eval(inner, env))
            }
            // The identity: an `int` value is what it is on this side; the
            // tag is compiled code's concern.
            Op::TagInt => {
                let inner = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (tag-int ..) has no operand".to_string()))?;
                Ok(State::Eval(inner, env))
            }
            // The identity too: a niched `Option`'s `some v` is the `Value`
            // of `v`. Field 0 is the payload representation, compiled
            // code's business.
            Op::SomeOf => {
                let inner = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (some-of ..) has no operand".to_string()))?;
                Ok(State::Eval(inner, env))
            }
            Op::BoxOption => {
                let key = core::field(heap, form, 0)
                    .ok_or_else(|| EvalError::Internal("eval: (box-option ..) has no key".to_string()))?;
                let inner = core::field(heap, form, 1)
                    .ok_or_else(|| EvalError::Internal("eval: (box-option ..) has no operand".to_string()))?;
                stack.push(heap, Frame::BoxOption { key }, loc.clone());
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
        // The refusals are the scheduler's (`sched::sleep_wait`), so the
        // compiled `sleep` makes the same ones in the same words.
        Ok((State::Blocked(sched::sleep_wait(secs)?), None))
    }

    /// Hands a fully-evaluated call over to a new task, and returns its handle.
    fn spawn_task(
        &self,
        heap: &mut Heap,
        form: Value,
        argv: Vec<Value>,
        kind: ArgsKind,
    ) -> Result<Value, EvalError> {
        let argv_list = core::list(heap, &argv).map_err(heap_err)?;
        heap.push_root(argv_list);
        // `form` and `argv_list` are rooted here, which the collector still
        // walks, so they survive the switch to the task's own root stack.
        let id = self.scheduler.borrow_mut().admit(heap, |heap| Task::start_call(heap, form, argv_list, kind));
        heap.pop_root();
        Ok(sched::task_handle(heap, id))
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
            let depth = self.trace_call_entry(heap, f, &argv, stepping)?;
            Some(Frame::TracedCall { name: f.name.clone(), depth, ret: f.sig.as_ref().map(|s| s.1.clone()) })
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
                    Some(Frame::TracedCall { name, depth, ret }) => Some((name.clone(), *depth, ret.clone())),
                    _ => None,
                };
                let start = DriveStart {
                    callee: DriveCallee::Body(compiled),
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
            if let Some(Frame::TracedCall { name, depth, ret }) = watch {
                self.trace_call_exit(heap, &name, depth, ret.as_ref(), Ok(v));
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
        // `(net-wait h interest)` — "not before this socket is ready" — is
        // the third shape of the same thing.
        if path == crate::Path::root("net-wait") {
            return Ok((State::Blocked(io_wait(heap, &argv, None)?), None));
        }
        if path == crate::Path::root("net-wait-for") {
            let secs = argv.get(2).copied().ok_or_else(|| EvalError::Internal("net-wait-for: no timeout".to_string()))?;
            let d = io_deadline(super::rt_f64(heap, &secs)?)?;
            return Ok((State::Blocked(io_wait(heap, &argv, Some(d))?), None));
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
    /// compiled code — driven by this task under the coroutine ABI (so a
    /// suspension inside it parks the task exactly as one inside a named
    /// call does) and called on the Rust stack under the classic one (no
    /// suspension, no state to keep) — a built-in used as a function value,
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
                if heap.compiled_closure_body_abi(id) == typelisp_abi::BODY_ABI_COROUTINE {
                    // Driven by this task rather than called: the frames it
                    // builds live in `Task::compiled`, so it can stop in the
                    // middle and the task can be put down with it — the same
                    // reason a *named* coroutine-ABI body is driven rather
                    // than called (`enter_fn`). The closure rides at the
                    // head of `CompiledEnter`'s own argument list, tagged as
                    // `Repr::Fn`, ahead of the call's own arguments — see
                    // `DriveCallee::Closure`.
                    let mut params = Vec::with_capacity(arg_reprs.len() + 1);
                    params.push(Repr::Fn);
                    params.extend(arg_reprs);
                    let mut all_args = Vec::with_capacity(argv.len() + 1);
                    all_args.push(f);
                    all_args.extend(argv);
                    let start = DriveStart { callee: DriveCallee::Closure, params, ret, watch: None };
                    // A heap list, not the `Vec`: it has to stay reachable
                    // across the truncate that releases this call's own
                    // frame, and only the state slots can root it
                    // (`set_state`).
                    let argv_list = core::list(heap, &all_args).map_err(heap_err)?;
                    return Ok((State::CompiledEnter { argv: argv_list, start }, None));
                }
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
            // The same refusal compiled code makes at `compile-untag-int`,
            // with the same wording, so a program is told the same thing in
            // both tiers.
            Frame::UntagInt => match v {
                Value::Int(_) => Ok((State::Apply(v), None)),
                Value::Boxed(id) if heap.is_bignum(id) => {
                    Err(EvalError::Panic("an integer argument does not fit a fixnum".to_string()))
                }
                other => Err(EvalError::Internal(format!("eval: (untag-int ..): not an int: {:?}", other))),
            },
            // The box the checker asked for is built whether or not the key
            // would niche: this node exists precisely because the slot the
            // value is going into needs the box.
            Frame::BoxOption { key } => {
                let key = match key {
                    Value::Str(id) => heap.string(id).to_string(),
                    other => return Err(EvalError::Internal(format!("eval: (box-option ..): key is {:?}", other))),
                };
                let (variant, fields) = if v.is_empty() { (1, vec![]) } else { (0, vec![v]) };
                Ok((State::Apply(crate::type_key::alloc_enum_keyed(heap, &key, variant, fields)), None))
            }

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

            Frame::TracedCall { name, depth, ret } => {
                self.trace_call_exit(heap, &name, depth, ret.as_ref(), Ok(v));
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
                        slot.set(heap, v);
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

            Frame::TracedCall { name, depth, ret } => {
                self.trace_call_exit(heap, &name, depth, ret.as_ref(), Err(&exit));
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
            | Frame::UntagInt
            | Frame::BoxOption { .. }
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
            Value::Str(id) => assert_eq!(&*h.string(id), "s0"),
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
            Value::Str(id) => assert_eq!(&*h.string(id), "first"),
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
                Value::Str(id) => assert_eq!(&*h.string(id), format!("a{}", i)),
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

        let src = "(labels ((build ((n int-any-width)) sexpr
                        (if (call (sexpr-null) () sexpr-null (sexpr) (var n))
                            (unit)
                            (call (sexpr-cons) () sexpr-cons (sexpr sexpr)
                              (int-any-width 1)
                              (apply (var build) sexpr (sexpr)
                                (call (sexpr-cdr) () sexpr-cdr (sexpr) (var n)))))))
               (apply (var build) sexpr (sexpr) (var xs)))".to_string();

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
            Value::Str(id) => assert_eq!(&*h.string(id), "kept"),
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
            Value::Str(id) => assert_eq!(&*h.string(id), "new"),
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
            Value::Str(id) => assert_eq!(&*h.string(id), "carried"),
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
            Value::Str(id) => assert_eq!(&*h.string(id), "kept"),
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
                assert_eq!(&*h.string(a), "o", "the outer cleanup should have run last");
                assert_eq!(&*h.string(b), "i", "the inner cleanup should have run first");
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
    /// `sched::drive` is the driver here — the same one `eval_cps` uses — so
    /// what these tests exercise is the loop that ships. It returns when *its*
    /// task finishes, and the others keep running meanwhile, so a task named
    /// later may already be `Done` by the time its turn comes.
    fn run_interleaved(heap: &mut Heap, srcs: &[&str]) -> Vec<Result<Value, EvalError>> {
        let base = heap.root_count();
        let interp = Interp::new();
        interp.scheduler.borrow_mut().set_switch_every_step(true);

        let mut ids = Vec::new();
        for src in srcs {
            let form = read1(heap, src);
            heap.push_root(form);
            // `form` is rooted here, which the collector still walks while
            // `admit` builds the task on a stack of its own.
            ids.push(interp.scheduler.borrow_mut().admit(heap, |heap| Task::start(heap, form, Value::Empty)));
        }

        let mut out = Vec::new();
        for id in ids {
            let done = interp.scheduler.borrow().done_value(id);
            let r = match done {
                // It finished while another task was being driven, and its
                // value is rooted in the scheduler's own stack.
                Some(v) => Ok(v),
                None => sched::drive(&interp.scheduler, heap, &interp, id),
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
