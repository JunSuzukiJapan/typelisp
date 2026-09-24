//! The task scheduler: which task runs next, and what each parked one is
//! waiting for.
//!
//! This is the runtime's, not the interpreter's, because an ahead-of-time
//! compiled executable has no interpreter and still has tasks: `go` starts
//! one, `sleep`/`wait`/a channel/a socket that says "not yet" puts one down.
//! Until this moved here, the executable's `main` was one `FrameStack::run`
//! and the first suspension was an abort.
//!
//! # Two kinds of task body, one scheduler
//!
//! What the scheduler holds per task is a [`TaskBody`]: something it can
//! `step` and can `deliver` an answer to. The interpreter's task (a
//! continuation stack that may be in the middle of a compiled chain) is one;
//! [`CompiledTask`](crate::sched::CompiledTask) — a compiled chain and nothing
//! else, which is every task an AOT executable has — is the other. Everything
//! about *waiting* — the queue, the channel table, the clock, the one `poll`
//! on the network — is written once, here, and both bodies get it. That is
//! what keeps a compiled `(recv ch)` and an interpreted one the same
//! operation whichever program it is in.
//!
//! # Owned, not global
//!
//! A `Scheduler` is a value owned by whoever owns the `Heap`: a field of the
//! interpreter, a local of the AOT entry point. Never a `static` or a
//! `thread_local` — a channel's buffered values live in this heap's root
//! stacks, and a table that outlived the heap would be the dangling
//! `ACTIVE_HEAP` bug with values in it. Several OS threads share one through
//! an `Arc` ([`SchedShared`]) and the ownership is the same: the frame that
//! owns the program's run owns it, and the workers hold only the `Arc`.
//!
//! What *is* published thread-locally is a **borrow**, for the extent of one
//! drive and not a moment longer ([`answer_now`]): a driver standing on a
//! machine frame — the printer's door into a compiled `print-object`, a
//! `defvar` initialiser, a compiled body the interpreter called from Rust —
//! has no continuation stack to park a task on, but it can still ask the
//! scheduler whether the operation needs to wait at all. Most do not: a
//! `(recv ch)` with something buffered, a `(go ...)`, a `Chan::new`. The
//! borrow is registered and cleared by a guard on the drive's frame, exactly
//! as `ACTIVE_HEAP` is registered per scope, so nothing outlives its owner.
//!
//! Cooperative, and on several OS threads: nothing preempts a task, but
//! `TYPELISP_THREADS` threads step ready tasks at the same time, all on one
//! heap ([`SchedShared`], [`drive_main`], [`drive_worker`];
//! `docs/dev/os-threads-design.md` §8). An interpreter's tasks hold its
//! `Rc`s and run only on its own thread ([`TaskBody::runs_anywhere`]); its
//! compiled ones go to workers that live for one drive ([`install_crew`]).

use typelisp_mem::{Heap, RootScope, RootStackId, TypeKeyId, Value};

// ---- what a task is to the scheduler ---------------------------------------

/// How a scheduler operation fails.
///
/// Two kinds and no more, on purpose: `Panic` is a language-level error the
/// program can catch (`send` on a closed channel, a negative `sleep`), and
/// `Internal` is the runtime disagreeing with itself (a handle of the wrong
/// shape, every task blocked). The interpreter maps them onto its own error
/// type; the AOT entry point prints them.
#[derive(Debug, Clone, PartialEq)]
pub enum SchedError {
    Panic(String),
    Internal(String),
}

impl std::fmt::Display for SchedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchedError::Panic(m) | SchedError::Internal(m) => f.write_str(m),
        }
    }
}

/// What one step of a task did.
pub enum Progress<E> {
    /// Still running. Step it again.
    Running,
    /// Suspended. The task keeps its frames and its roots; the scheduler puts
    /// it back on the queue when what it waits for has happened.
    Blocked(Waiting),
    /// Finished: the task ran out. The result is rooted in the task's own
    /// state slots until the caller truncates them.
    Done(Result<Value, E>),
    /// Stopped where only the interpreter can go on — a compiled chain
    /// applying an interpreted closure, say — and already turned into the
    /// body that can ([`TaskBody::runs_anywhere`] is `false` from here). The
    /// scheduler keeps it for the thread that owns the interpreter, **for
    /// good**: a task is never handed back out. Only a body that
    /// `runs_anywhere` returns this.
    NeedsMain,
}

/// How a woken compiled frame reads what it was waiting for out of its value
/// slot: nothing (`yield`, `sleep`, `net-wait` answer with unit) or a tagged
/// word (`wait`, `recv`, every typed answer — whatever `T` is, the answer
/// crosses tagged and the resume block decodes it with the kind the bridge
/// baked in, the same division `catch`/`throw` make for a thrown value).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wake {
    Unit,
    Tagged,
}

/// One unit of execution, as the scheduler sees it.
///
/// The scheduler owns the queue and the waits; the body owns *how to run*.
/// Two methods are the whole contract. Between two `step`s the body holds
/// everything it needs to go on — that is what makes it a task rather than a
/// call — and `deliver` is the one way anything gets back in.
pub trait TaskBody: Sized {
    /// What `step` needs besides the heap: the interpreter for an interpreted
    /// task, nothing for a compiled one.
    type Cx: ?Sized;
    /// The body's own error type, which the scheduler's errors fold into.
    type Error: From<SchedError>;

    /// A task that applies `closure` — a compiled closure the `go` site built
    /// to make the call and answer with the result tagged — to no arguments.
    /// Built on the task's own root stack (`Scheduler::admit`), with
    /// `closure` rooted by the caller until then.
    fn start_closure(heap: &mut Heap, closure: Value) -> Self;

    /// Where this task's roots begin. The two state slots are `sbase` and
    /// `sbase + 1`; frames root above them. **Whoever finishes the task
    /// truncates to here.**
    fn sbase(&self) -> usize;

    /// Runs the task one step.
    ///
    /// `cx` is the stepping thread's: the interpreter on the thread that
    /// owns one, and whatever a worker has otherwise — which is why a body
    /// that needs the interpreter says so ([`Self::runs_anywhere`]) and is
    /// never stepped by a thread without it.
    fn step(&mut self, heap: &mut Heap, cx: &Self::Cx) -> Progress<Self::Error>;

    /// Whether any thread may step this task, or only the one that owns the
    /// interpreter ([`drive_main`]'s). Read when the task is admitted and
    /// after a [`Progress::NeedsMain`].
    fn runs_anywhere(&self) -> bool {
        true
    }

    /// Gives a parked task what it was waiting for — a value to go on with,
    /// or an error to unwind with — **rooting it in the task's own stack**.
    ///
    /// Called with the task's root stack current. The root is the point: a
    /// value handed to a woken task (a `some` box built by `recv`, say)
    /// exists only inside a Rust enum until this runs, and the task is not
    /// stepped again until some other task has had its turn and allocated.
    fn deliver(&mut self, heap: &mut Heap, answer: Result<Value, SchedError>);

    /// The heap values `e` carries — a thrown value, say — which the thread
    /// driving the main task roots while it waits for its threads to stop
    /// ([`drive_main`]).
    fn failure_values(e: &Self::Error) -> Vec<Value>;

    /// What a task's failure does to the program, for a task that is not the
    /// main one — the interpreter turns an escaped `throw` into a panic here,
    /// which is Go's rule for an unrecovered panic in a goroutine.
    fn failure_left_task(e: Self::Error) -> Self::Error;
}

// ---- what a task waits for ---------------------------------------------------

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
pub enum Waiting {
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
    /// `(go ...)` — start a task that applies this closure, and answer with
    /// its handle. Never a wait: answered on the spot like `Chan::new`, and
    /// through the scheduler for the same reason — the table of tasks is
    /// the scheduler's, and only the driver can reach it.
    ///
    /// The closure is rooted through the asking task's state slot while the
    /// operation is in flight, as a parked `send`'s value is.
    Spawn(Value),
    /// `(thread ...)` — [`Waiting::Spawn`] for a task that runs on an OS
    /// thread of its own, started for it ([`Scheduler::try_now`]), and
    /// answered with a `Thread<T>` handle. Rooted the way `Spawn`'s closure
    /// is.
    SpawnThread(Value),
    /// The interpreter's thread — asked before a call only the interpreter
    /// can answer (`call_state::SUSPEND_MAIN`). Never a wait: a body that can
    /// move there has done so before asking (`Progress::NeedsMain`), and to
    /// any other asker the answer is "go on where you are" — the interpreter
    /// is here, or there is none to move to and the call says so itself.
    Main,
    /// `select` — any one of several channel operations, whichever can go
    /// first. `has_else` means the task does not wait: with nothing ready it
    /// is answered with the `else` arm instead.
    Select { ops: Vec<SelectOp>, has_else: bool },
    /// A socket — `(net-wait h interest)`. The task is parked until `poll`
    /// reports the descriptor readable or writable, which is the prelude's
    /// cue to retry the non-blocking operation that said "not yet".
    ///
    /// The descriptor, not the handle: it is resolved when the wait is
    /// asked for (`io_wait`), so a handle that is not a socket or is
    /// already closed is refused there, and the scheduler hands `poll`
    /// plain integers. Like `Until`, this holds no `Value` and needs no
    /// root.
    ///
    /// With a `deadline` — `(net-wait-for h interest secs)` — the wait is
    /// also a sleep, and answers with a `bool`: `true` when the socket
    /// became ready, `false` when the clock ran out first. Without one the
    /// answer is unit, as `sleep`'s is.
    Io { fd: i32, interest: crate::os::Interest, deadline: Option<std::time::Instant> },
}

/// What an `Io` wait answers with once the socket is ready: unit for a
/// plain `net-wait`, `true` for a `net-wait-for` (whose other answer,
/// `false`, is the clock's).
fn io_ready_answer(deadline: Option<std::time::Instant>) -> Value {
    if deadline.is_some() {
        Value::Bool(true)
    } else {
        Value::Empty
    }
}

/// One arm of a `select`, in the order the arms are written — the index *is*
/// the arm, which is what the answer names.
///
/// The values a `Send` arm offers need no root of their own, unlike a plain
/// [`ChanOp::Send`]'s: the checker hoisted every operand into a `let` around
/// the `select`, so each is a live binding in the environment the task's
/// `Frame::SelectArm` is holding (and a compiled frame's slots, which the
/// collector walks).
#[derive(Clone, PartialEq, Debug)]
pub enum SelectOp {
    Recv(ChanId, String),
    Send(ChanId, Value),
}

/// One channel operation, as the task asked for it.
///
/// Four of the six never park — they only need the scheduler's table, which
/// only the driver can reach (`typelisp_abi::call_state::SUSPEND_CHAN_NEW`
/// records why). `Send` and `Recv` are the two that can.
#[derive(Clone, PartialEq, Debug)]
pub enum ChanOp {
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
    /// The value is rooted through the task's own state slot (the body's
    /// `deliver`/state bookkeeping), which is what lets a parked sender hold
    /// it with nothing else pointing at it: a `Value` inside a Rust enum is
    /// invisible to the collector.
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
pub struct ChanId(usize);

/// Names a task the scheduler is holding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TaskId(usize);

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

// ---- handles -----------------------------------------------------------------

/// The runtime value of a `Task<T>`: a boxed struct holding the scheduler's id.
///
/// The key carries no type argument. `Task<i32>` and `Task<string>` are the
/// same thing at run time — no fields to read, and `wait`'s return type is
/// spelled at the call site — so a type argument here would distinguish nothing.
/// That is unlike `Vector<T>`, where the site has to carry its elements'
/// representation because the definition cannot tell you.
///
/// The key is one of `typelisp_mem`'s pre-interned ones (`TypeKeyId::TASK`),
/// because this crate has no `Path` to derive it from — the same arrangement
/// `NetError` and `heap-info` have, checked by
/// `tests/type_identity_guard_test.rs`.
pub fn task_handle(heap: &mut Heap, id: TaskId) -> Value {
    heap.alloc_struct(TypeKeyId::TASK, vec![Value::Int(id.0 as i64)])
}

/// The runtime value of a `Chan<T>`: a boxed struct holding the scheduler's id.
///
/// Carries no type argument, for [`task_handle`]'s reason: `Chan<i32>` and
/// `Chan<string>` are the same thing at run time, and the element type is
/// spelled at every site that puts one in or takes one out.
pub fn chan_handle(heap: &mut Heap, id: ChanId) -> Value {
    heap.alloc_struct(TypeKeyId::CHAN, vec![Value::Int(id.0 as i64)])
}

/// The runtime value of a `Thread<T>`: a [`task_handle`] under the
/// `thread` key. `join` reads it with [`task_id_of`], because a thread *is*
/// a task to the scheduler — one it runs on an OS thread of its own.
pub fn thread_handle(heap: &mut Heap, id: TaskId) -> Value {
    heap.alloc_struct(TypeKeyId::THREAD, vec![Value::Int(id.0 as i64)])
}

/// The scheduler id inside a `Task<T>` handle.
///
/// The handle is a boxed struct with the id in field 0 and nothing else — see
/// [`task_handle`]. Nothing in the language can build one, so a value of the
/// wrong shape here means the evaluator built it wrong.
pub fn task_id_of(heap: &Heap, v: Option<Value>) -> Result<TaskId, SchedError> {
    match v {
        Some(Value::Boxed(id)) if heap.struct_field_count(id) == 1 => match heap.struct_field(id, 0) {
            Value::Int(n) if n >= 0 => Ok(TaskId(n as usize)),
            other => Err(SchedError::Internal(format!("wait: a task handle holds {:?}", other))),
        },
        other => Err(SchedError::Internal(format!("wait: {:?} is not a task handle", other))),
    }
}

/// The scheduler id inside a `Chan<T>` handle — [`task_id_of`] for channels.
pub fn chan_id_of(heap: &Heap, v: Option<Value>) -> Result<ChanId, SchedError> {
    match v {
        Some(Value::Boxed(id)) if heap.struct_field_count(id) == 1 => match heap.struct_field(id, 0) {
            Value::Int(n) if n >= 0 => Ok(ChanId(n as usize)),
            other => Err(SchedError::Internal(format!("a channel handle holds {:?}", other))),
        },
        other => Err(SchedError::Internal(format!("{:?} is not a channel handle", other))),
    }
}

/// What to call a `Waiting` in an error addressed to a programmer.
pub fn waiting_name(w: &Waiting) -> &'static str {
    match w {
        Waiting::Task(_) => "`wait`",
        Waiting::Yield => "`yield`",
        Waiting::Until(_) => "`sleep`",
        Waiting::Select { .. } => "`select`",
        Waiting::Chan(ChanOp::Send(..)) => "`send`",
        Waiting::Chan(ChanOp::Recv(..)) => "`recv`",
        // The other three answer at once, so they never reach a message that
        // says something could not block. Neither does a spawn.
        Waiting::Chan(_) => "a channel operation",
        Waiting::Spawn(_) => "`go`",
        Waiting::SpawnThread(_) => "`thread`",
        Waiting::Main => "a call that needs the interpreter",
        Waiting::Io { .. } => "a socket operation",
    }
}

/// The refusal a machine-frame driver makes when `w` would have to wait:
/// there is a Rust frame waiting on the answer, so there is nothing to
/// switch *to*. One wording for the interpreter's `run_to_completion` and the
/// compiled `FrameStack::run_to_end`, because the two front ends are refusing
/// the same thing for the same reason.
pub fn cannot_block_message(w: &Waiting) -> String {
    format!(
        "{} cannot block: it was reached from a Rust caller, which has no continuation stack to suspend",
        waiting_name(w)
    )
}

/// The word a woken compiled frame reads out of its value slot, encoded
/// through the shape the suspension recorded — for the reason every crossing
/// obeys: the word's meaning is in the type, and the value cannot say. A unit
/// wake is a word the collector never follows; a typed one crosses tagged.
pub fn wake_word(wake: Wake, v: Value) -> i64 {
    match wake {
        Wake::Unit => 0,
        Wake::Tagged => typelisp_abi::encode(v),
    }
}

// ---- the scheduler a machine-frame driver can reach -------------------------------

/// What a driver with no continuation stack may still ask of the scheduler
/// that is running the task it was reached from: [`Scheduler::try_now`].
///
/// Object-safe on purpose — the task bodies make several `SchedShared`
/// types, and the driver asking does not know which one is driving.
pub trait AnswerNow {
    fn answer_now(&self, heap: &mut Heap, w: &Waiting) -> Option<Result<Value, SchedError>>;
}

thread_local! {
    /// The scheduler whose [`drive_main`] (or [`drive_worker`]/[`drive_pinned`])
    /// is on this thread's stack, if any — a borrow published for that drive's
    /// extent and cleared by its guard. See the module comment's "Owned, not
    /// global". Per thread: each worker publishes the shared scheduler for
    /// its own loop.
    static DRIVING: std::cell::Cell<Option<std::ptr::NonNull<dyn AnswerNow>>> = const { std::cell::Cell::new(None) };
}

/// Publishes `sched` as the driving scheduler for as long as the guard lives,
/// and restores whatever was published before it — a nested drive (the REPL
/// evaluating inside a `print-object`, an `eval`-carrying executable before
/// its interpreter joined the one scheduler) must not leave the outer one
/// unreachable when it returns.
struct DrivingGuard {
    previous: Option<std::ptr::NonNull<dyn AnswerNow>>,
}

impl DrivingGuard {
    fn publish(sched: &dyn AnswerNow) -> DrivingGuard {
        // The lifetime is erased here and re-established by the guard: the
        // pointer is cleared on drop, and the drive holds its `&SchedShared`
        // for the whole of the guard's life. Nothing reads it after that.
        let ptr = std::ptr::NonNull::from(sched);
        let ptr: std::ptr::NonNull<dyn AnswerNow + 'static> = unsafe { std::mem::transmute(ptr) };
        let previous = DRIVING.with(|cell| cell.replace(Some(ptr)));
        DrivingGuard { previous }
    }
}

impl Drop for DrivingGuard {
    fn drop(&mut self) {
        DRIVING.with(|cell| cell.set(self.previous));
    }
}

/// Asks the driving scheduler whether `w` can be answered without parking
/// anybody — [`Scheduler::try_now`] reached from a machine frame.
///
/// `None` when the operation would have to wait, and also when no drive is
/// on this thread at all (a compiled body run outside any scheduler): either
/// way the caller has nothing to wait *with*, and refuses.
pub fn answer_now(heap: &mut Heap, w: &Waiting) -> Option<Result<Value, SchedError>> {
    let ptr = DRIVING.with(|cell| cell.get())?;
    // SAFETY: the pointer was published by a `DrivingGuard` still alive on
    // this thread's stack (its drop clears the slot), and the guard borrows
    // the scheduler for its whole life.
    unsafe { ptr.as_ref() }.answer_now(heap, w)
}

// ---- the table ---------------------------------------------------------------

/// A task the scheduler owns, and the root stack that belongs to it.
struct TaskSlot<B> {
    task: B,
    roots: RootStackId,
    /// Whether the task is a `thread`'s: run only by the OS thread started
    /// for it ([`drive_pinned`]), and made ready on [`Scheduler::pinned_ready`]
    /// rather than the queue every other thread takes from.
    pinned: bool,
    /// Whether only the thread that owns the interpreter may run the task
    /// (`!TaskBody::runs_anywhere`). Such a task waits on the one queue with
    /// every other, in order; a worker passes over it ([`Taker::Worker`]).
    main_only: bool,
}

/// Starts the OS thread that runs a pinned task — see
/// [`Scheduler::set_thread_starter`]. Called with the scheduler's lock held;
/// it only starts the thread, which takes the lock itself once it runs.
pub type ThreadStarter = std::sync::Arc<dyn Fn(TaskId) -> std::io::Result<()> + Send + Sync>;

/// One position in the scheduler's table.
///
/// `Running` exists so a position stays **reserved** while its task is out
/// being stepped. Without it, `admit` reuses the position of whichever task is
/// currently running — which is exactly what a `go` inside the main task does,
/// and the new task then inherits main's id and is retired along with it.
enum Slot<B> {
    /// No task here. `admit` may reuse this position.
    Empty,
    /// A task that exists and is not running.
    Parked(TaskSlot<B>),
    /// A task that is taken out to be stepped.
    Running,
    /// A task that cannot run until `Waiting` is satisfied. It goes back to
    /// `Parked` — and onto the queue — when that happens.
    Blocked(TaskSlot<B>, Waiting),
    /// A task that finished, and the value it answered with.
    ///
    /// **Kept for the life of the program**, because `wait` may be asked again
    /// at any time and has to give the same answer. A program that spawns
    /// without bound therefore accumulates one of these per task — the v1
    /// limit. Tying the result's lifetime to its handle's instead needs the
    /// collector to tell the scheduler when a handle dies.
    Done(Value),
}

/// The tasks that exist and the order they run in.
///
/// Lives behind [`SchedShared`]'s lock, and **every hold of it is short**: a
/// task is taken *out* to be stepped, so stepping it — which re-enters the
/// evaluator, and can run compiled code that calls back in — never holds the
/// lock. [`run_one`] is written that way, and so must any other driver be.
pub struct Scheduler<B> {
    /// One entry per task position — see [`Slot`].
    slots: Vec<Slot<B>>,
    /// Ids ready to run, oldest first — every task but the pinned ones.
    ready: std::collections::VecDeque<TaskId>,
    /// The pinned tasks that are ready. Each is taken only by its own OS
    /// thread, so the order among them means nothing.
    pinned_ready: Vec<TaskId>,
    /// How a `thread` gets its OS thread, or `None` for a scheduler nobody
    /// started threads for ([`start_workers`]/[`install_crew`]), which
    /// therefore refuses `thread`.
    thread_starter: Option<ThreadStarter>,
    /// The pinned tasks whose OS thread is running. A crew that lives for
    /// one drive ([`install_crew`]) stops them with the rest when the drive
    /// ends; a pinned task still alive then has no thread until the next
    /// drive starts it one again ([`Scheduler::restart_pinned`]).
    pinned_threads: Vec<TaskId>,
    /// Where finished tasks' results are rooted: position `i` holds task `i`'s
    /// value. Created on the first `finish`, and never truncated — see
    /// [`Slot::Done`] for why a result outlives its task.
    roots: Option<RootStackId>,
    /// How many tasks are `Waiting::Until`. Only a count, so the common case —
    /// nobody sleeping — costs one comparison per task switch instead of a
    /// walk over every slot.
    sleeping: usize,
    /// How many tasks are `Waiting::Io` — `sleeping`'s twin, so that a
    /// program with no sockets never builds a `poll` set.
    io_waiting: usize,
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
    /// How many tasks are out being stepped (`Slot::Running`) — what tells
    /// "every task is blocked" apart from "every task this thread can see is
    /// blocked" once other threads step tasks too.
    running: usize,
    /// Whether anything happened since the last time [`Locked`] looked that
    /// an idle thread would want to know about: a task made ready, a task
    /// finished, a new clock or socket wait. The lock's owner wakes the
    /// others when it lets go ([`Locked::flush`]).
    dirty: bool,
}

impl<B> Default for Scheduler<B> {
    fn default() -> Self {
        Scheduler {
            slots: Vec::new(),
            ready: std::collections::VecDeque::new(),
            pinned_ready: Vec::new(),
            thread_starter: None,
            pinned_threads: Vec::new(),
            roots: None,
            sleeping: 0,
            io_waiting: 0,
            chans: Vec::new(),
            chan_roots: None,
            rng: 0,
            running: 0,
            dirty: false,
        }
    }
}

impl<B: TaskBody> Scheduler<B> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lets `thread` start OS threads, through `start` — see
    /// [`start_workers`], the one caller.
    fn set_thread_starter(&mut self, start: ThreadStarter) {
        self.thread_starter = Some(start);
    }

    /// Puts a parked task where the thread that may run it will look.
    fn make_ready(&mut self, id: TaskId) {
        let Slot::Parked(slot) = &self.slots[id.0] else {
            unreachable!("only a parked task is made ready");
        };
        if slot.pinned {
            self.pinned_ready.push(id);
        } else {
            self.ready.push_back(id);
        }
        self.dirty = true;
    }

    /// Adds a task and makes it ready to run.
    ///
    /// The task is built by `make` **on a root stack of its own**, which is
    /// where everything it roots from then on lives; the caller's stack is
    /// current again by the time this returns. Whatever `make` reads had
    /// better be rooted in the caller's stack — the collector still walks it,
    /// so that is enough.
    pub fn admit(&mut self, heap: &mut Heap, make: impl FnOnce(&mut Heap) -> B) -> TaskId {
        self.admit_as(heap, false, make)
    }

    /// [`Self::admit`], for a task that is pinned or not.
    fn admit_as(&mut self, heap: &mut Heap, pinned: bool, make: impl FnOnce(&mut Heap) -> B) -> TaskId {
        let home = heap.current_root_stack();
        let roots = heap.new_root_stack();
        heap.switch_to_root_stack(roots);
        let task = make(heap);
        heap.switch_to_root_stack(home);
        let main_only = !task.runs_anywhere();
        let id = match self.slots.iter().position(|s| matches!(s, Slot::Empty)) {
            Some(i) => TaskId(i),
            None => {
                self.slots.push(Slot::Empty);
                TaskId(self.slots.len() - 1)
            }
        };
        self.slots[id.0] = Slot::Parked(TaskSlot { task, roots, pinned, main_only });
        self.make_ready(id);
        id
    }

    /// `(thread ...)`: admits the task pinned and starts the OS thread that
    /// will run it. A thread the system will not start is a panic in the
    /// asking task, and the task that would have run on it never existed.
    fn admit_thread(&mut self, heap: &mut Heap, closure: Value) -> Result<TaskId, SchedError> {
        self.admit_thread_with(heap, |heap| B::start_closure(heap, closure))
    }

    /// [`Self::admit_thread`] for a task `make` builds — an interpreted
    /// `thread`'s, which enters a compiled body with arguments rather than
    /// applying a closure the site built (`CompiledTask::enter_with`).
    pub fn admit_thread_with(&mut self, heap: &mut Heap, make: impl FnOnce(&mut Heap) -> B) -> Result<TaskId, SchedError> {
        // Every scheduler that runs a program is given one — by
        // `start_workers` or `install_crew` — before its first task.
        let Some(start) = self.thread_starter.clone() else {
            return Err(SchedError::Internal("thread: this scheduler was given no way to start an OS thread".to_string()));
        };
        let id = self.admit_as(heap, true, make);
        if let Err(e) = self.start_pinned(&start, id) {
            self.pinned_ready.retain(|t| *t != id);
            let Slot::Parked(slot) = std::mem::replace(&mut self.slots[id.0], Slot::Empty) else {
                unreachable!("a task just admitted is parked");
            };
            heap.drop_root_stack(slot.roots);
            return Err(SchedError::Panic(format!("thread: cannot start an OS thread ({})", e)));
        }
        Ok(id)
    }

    /// Starts the OS thread of pinned task `id`, and remembers that it runs.
    fn start_pinned(&mut self, start: &ThreadStarter, id: TaskId) -> std::io::Result<()> {
        start(id)?;
        self.pinned_threads.push(id);
        Ok(())
    }

    /// Starts an OS thread again for every pinned task that is alive and has
    /// none — the ones a crew's end stopped ([`install_crew`]). A thread the
    /// system will not start fails the program, as a panic in that task
    /// would: the task cannot run anywhere else.
    fn restart_pinned(&mut self) -> Result<(), (TaskId, SchedError)> {
        let Some(start) = self.thread_starter.clone() else { return Ok(()) };
        for i in 0..self.slots.len() {
            let alive_pinned = match &self.slots[i] {
                Slot::Parked(s) | Slot::Blocked(s, _) => s.pinned,
                Slot::Empty | Slot::Running | Slot::Done(_) => false,
            };
            if !alive_pinned || self.pinned_threads.contains(&TaskId(i)) {
                continue;
            }
            self.start_pinned(&start, TaskId(i)).map_err(|e| {
                (TaskId(i), SchedError::Panic(format!("thread: cannot start an OS thread again ({})", e)))
            })?;
        }
        Ok(())
    }

    /// Takes the next ready task *out* to be stepped, reserving its position.
    /// The caller puts it back (`put_back`) or retires it.
    ///
    /// Which task depends on who asks — see [`Taker`].
    fn next_ready(&mut self, taker: Taker) -> Option<(TaskId, TaskSlot<B>)> {
        let id = match taker {
            Taker::Main => self.ready.pop_front()?,
            Taker::Worker => {
                let slots = &self.slots;
                let at = self
                    .ready
                    .iter()
                    .position(|t| !matches!(&slots[t.0], Slot::Parked(s) if s.main_only))?;
                self.ready.remove(at)?
            }
            Taker::Pinned(own) => {
                let at = self.pinned_ready.iter().position(|t| *t == own)?;
                self.pinned_ready.swap_remove(at)
            }
        };
        match std::mem::replace(&mut self.slots[id.0], Slot::Running) {
            Slot::Parked(slot) => {
                self.running += 1;
                Some((id, slot))
            }
            _ => unreachable!("a queued task is parked in its slot"),
        }
    }

    /// Returns a task that is still running to the back of the queue.
    fn put_back(&mut self, id: TaskId, slot: TaskSlot<B>) {
        self.running -= 1;
        self.slots[id.0] = Slot::Parked(slot);
        self.make_ready(id);
    }

    /// Returns a task that answered [`Progress::NeedsMain`] to the back of
    /// the queue, as the interpreter's thread's alone from now on — a pinned
    /// one included: its OS thread has no interpreter either, and ends
    /// ([`next`]'s `Who::Pinned`) now that its task is nobody's to pin.
    fn put_back_for_main(&mut self, id: TaskId, mut slot: TaskSlot<B>) {
        debug_assert!(!slot.task.runs_anywhere(), "a task that needs main has already become main's");
        slot.main_only = true;
        slot.pinned = false;
        self.put_back(id, slot);
    }

    /// The main task's result, once it has one, taken back out of the
    /// scheduler: the position is freed and the root let go of, because the
    /// caller takes the value and no `Task<T>` names the main task.
    fn take_main_result(&mut self, heap: &mut Heap, id: TaskId) -> Option<Value> {
        let Some(Slot::Done(v)) = self.slots.get(id.0) else { return None };
        let v = *v;
        self.slots[id.0] = Slot::Empty;
        let stack = self.roots.expect("a finished task's result is rooted");
        let home = heap.current_root_stack();
        heap.switch_to_root_stack(stack);
        heap.set_root(id.0, Value::Empty);
        heap.switch_to_root_stack(home);
        Some(v)
    }

    /// Answers `w` if it can be answered right now, without parking anybody.
    ///
    /// `None` means the asking task has to wait. This is the **whole** of what
    /// each operation means: [`Scheduler::block`] and the nested-evaluation
    /// path both go through it, so an interpreted `(recv ch)` and a compiled
    /// one are the same code, and so is the one inside a print method.
    ///
    /// Waking *other* tasks happens here too (a receiver taking a parked
    /// sender's value, `close` releasing everyone). That is not a side effect
    /// on the way to an answer: for a rendezvous it *is* the answer.
    pub fn try_now(&mut self, heap: &mut Heap, w: &Waiting) -> Option<Result<Value, SchedError>> {
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
                    Some(Err(SchedError::Internal(format!("wait: {:?} is not a live task", on))))
                }
            },
            Waiting::Chan(op) => self.try_chan(heap, op),
            Waiting::Select { ops, has_else } => self.try_select(heap, ops, *has_else),
            // Admitted and nothing is run: the starting task keeps its turn,
            // and the new one is picked up when some task yields, waits or
            // finishes — which is what `go` promises.
            Waiting::Spawn(closure) => {
                let closure = *closure;
                let id = self.admit(heap, |heap| B::start_closure(heap, closure));
                Some(Ok(task_handle(heap, id)))
            }
            // The same, on a thread of its own — which starts now, and takes
            // the task as soon as it gets the lock this is holding.
            Waiting::SpawnThread(closure) => Some(self.admit_thread(heap, *closure).map(|id| thread_handle(heap, id))),
            Waiting::Main => Some(Ok(Value::Empty)),
            // One descriptor, zero timeout: ready now or not. A `poll` that
            // fails (a descriptor closed under the task) is reported as
            // ready, for the reason `poll_ready` gives — the retry will fail
            // with a message where a park would hang.
            Waiting::Io { fd, interest, deadline } => {
                match crate::os::poll_ready(&[(*fd, *interest)], Some(std::time::Duration::ZERO)) {
                    Ok(ready) if ready.iter().any(|(_, r)| *r) => Some(Ok(io_ready_answer(*deadline))),
                    Ok(_) => match deadline {
                        Some(t) if *t <= std::time::Instant::now() => Some(Ok(Value::Bool(false))),
                        _ => None,
                    },
                    Err(_) => Some(Ok(io_ready_answer(*deadline))),
                }
            }
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
    fn try_select(&mut self, heap: &mut Heap, ops: &[SelectOp], has_else: bool) -> Option<Result<Value, SchedError>> {
        let ready: Vec<usize> = (0..ops.len()).filter(|i| self.select_arm_is_ready(&ops[*i])).collect();
        if ready.is_empty() {
            // `else` is the arm after the last channel arm — which is what
            // makes the checker's "`else` must be last" rule worth having.
            return has_else.then(|| select_answer(heap, ops.len(), Value::Empty));
        }
        let pick = ready[self.next_random(ready.len())];
        let answer = match &ops[pick] {
            SelectOp::Recv(c, key) => self.chan_recv(heap, *c, key),
            SelectOp::Send(c, v) => self.chan_send(heap, *c, *v),
        };
        match answer {
            Some(Ok(v)) => Some(select_answer(heap, pick, v)),
            // A send arm on a closed channel counts as ready and then panics,
            // which is what a plain `(send ch v)` on one does.
            Some(Err(e)) => Some(Err(e)),
            None => Some(Err(SchedError::Internal("select: an arm reported ready and then could not go".to_string()))),
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
            // `Scheduler` starts at zero, and xorshift stays at zero forever.
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
    fn try_chan(&mut self, heap: &mut Heap, op: &ChanOp) -> Option<Result<Value, SchedError>> {
        match op {
            ChanOp::New(cap) => {
                if *cap < 0 {
                    return Some(Err(SchedError::Panic(format!("Chan::new: {} is not a capacity a channel can have", cap))));
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
    fn block(&mut self, heap: &mut Heap, id: TaskId, mut slot: TaskSlot<B>, w: Waiting) {
        self.running -= 1;
        // A new clock or socket wait changes what an idle thread should be
        // waiting for; a ready task is something for it to run.
        self.dirty = true;
        match self.try_now(heap, &w) {
            Some(answer) => {
                deliver_to(heap, &mut slot, answer);
                self.slots[id.0] = Slot::Parked(slot);
                self.make_ready(id);
            }
            None => {
                match w {
                    // Straight back onto the queue, behind everything already
                    // on it.
                    Waiting::Yield => {
                        deliver_to(heap, &mut slot, Ok(Value::Empty));
                        self.slots[id.0] = Slot::Parked(slot);
                        self.make_ready(id);
                    }
                    Waiting::Until(_) => {
                        self.sleeping += 1;
                        self.slots[id.0] = Slot::Blocked(slot, w);
                    }
                    Waiting::Io { .. } => {
                        self.io_waiting += 1;
                        self.slots[id.0] = Slot::Blocked(slot, w);
                    }
                    Waiting::Task(_)
                    | Waiting::Chan(_)
                    | Waiting::Select { .. }
                    | Waiting::Spawn(_)
                    | Waiting::SpawnThread(_)
                    | Waiting::Main => {
                        self.slots[id.0] = Slot::Blocked(slot, w);
                    }
                }
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
            Slot::Blocked(_, Waiting::Chan(ChanOp::Recv(on, key))) if *on == c => Some((i, None, key.clone())),
            Slot::Blocked(_, Waiting::Select { ops, .. }) => ops.iter().enumerate().find_map(|(a, op)| match op {
                SelectOp::Recv(on, key) if *on == c => Some((i, Some(a), key.clone())),
                _ => None,
            }),
            _ => None,
        })
    }

    /// The same for a task that can hand a value *to* `c`, and the value it is
    /// offering.
    fn waiting_sender(&self, c: ChanId) -> Option<(usize, Option<usize>, Value)> {
        self.slots.iter().enumerate().find_map(|(i, s)| match s {
            Slot::Blocked(_, Waiting::Chan(ChanOp::Send(on, v))) if *on == c => Some((i, None, *v)),
            Slot::Blocked(_, Waiting::Select { ops, .. }) => ops.iter().enumerate().find_map(|(a, op)| match op {
                SelectOp::Send(on, v) if *on == c => Some((i, Some(a), *v)),
                _ => None,
            }),
            _ => None,
        })
    }

    /// Puts a parked task back on the queue with what it resumes with.
    fn wake(&mut self, heap: &mut Heap, i: usize, answer: Result<Value, SchedError>) {
        let Slot::Blocked(mut slot, _) = std::mem::replace(&mut self.slots[i], Slot::Running) else {
            unreachable!("only a blocked task is woken");
        };
        deliver_to(heap, &mut slot, answer);
        self.slots[i] = Slot::Parked(slot);
        self.make_ready(TaskId(i));
    }

    /// Wakes a receiver with `v` (or the end of a closed channel), answering
    /// in whichever shape it was waiting in.
    fn wake_receiver(&mut self, heap: &mut Heap, i: usize, arm: Option<usize>, key: &str, v: Option<Value>) {
        let payload = heap.alloc_option(key, v);
        let answer = match arm {
            Some(a) => select_answer(heap, a, payload),
            None => Ok(payload),
        };
        self.wake(heap, i, answer);
    }

    /// Wakes a sender whose value has been taken.
    fn wake_sender(&mut self, heap: &mut Heap, i: usize, arm: Option<usize>) {
        let answer = match arm {
            Some(a) => select_answer(heap, a, Value::Empty),
            None => Ok(Value::Empty),
        };
        self.wake(heap, i, answer);
    }

    /// `(send ch v)`. `None` means the sender has to wait for room.
    fn chan_send(&mut self, heap: &mut Heap, c: ChanId, v: Value) -> Option<Result<Value, SchedError>> {
        if self.chans[c.0].closed {
            return Some(Err(SchedError::Panic("send: the channel is closed".to_string())));
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
    fn chan_recv(&mut self, heap: &mut Heap, c: ChanId, key: &str) -> Option<Result<Value, SchedError>> {
        if let Some(v) = self.chan_pop(heap, c) {
            // Taking one out made room, so a sender that was waiting for room
            // can put its value in and go. Waking a `select` builds its answer
            // with a `cons`, which can collect — and `v` left its ring slot
            // (its root) the moment it was popped.
            if let Some((i, arm, offered)) = self.waiting_sender(c) {
                self.chan_push(heap, c, offered);
                heap.push_root(v);
                self.wake_sender(heap, i, arm);
                heap.pop_root();
            }
            return Some(Ok(heap.alloc_option(key, Some(v))));
        }
        // Nothing buffered — which for an unbuffered channel is always, so
        // this is the rendezvous seen from the receiving side.
        if let Some((i, arm, offered)) = self.waiting_sender(c) {
            self.wake_sender(heap, i, arm);
            return Some(Ok(heap.alloc_option(key, Some(offered))));
        }
        if self.chans[c.0].closed {
            return Some(Ok(heap.alloc_option(key, None)));
        }
        None
    }

    /// `(close ch)`. Everyone parked on it is released.
    fn chan_close(&mut self, heap: &mut Heap, c: ChanId) -> Result<(), SchedError> {
        if self.chans[c.0].closed {
            return Err(SchedError::Panic("close: the channel is already closed".to_string()));
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
            self.wake(heap, i, Err(SchedError::Panic("send: the channel was closed while waiting".to_string())));
        }
        Ok(())
    }

    /// Records `id`'s result and wakes everything that was waiting for it.
    ///
    /// The value is rooted in the scheduler's own stack first: it has outlived
    /// the task's stack, and the tasks being woken will not touch it until they
    /// are stepped.
    fn finish(&mut self, heap: &mut Heap, id: TaskId, v: Value) {
        self.running -= 1;
        self.dirty = true;
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
            if matches!(&self.slots[i], Slot::Blocked(_, Waiting::Task(on)) if *on == id) {
                self.wake(heap, i, Ok(v));
            }
        }
    }

    /// Puts every sleeping task whose deadline has passed back on the queue.
    ///
    /// Called before each task switch rather than only when nothing is ready:
    /// a task that asked for 10ms while another runs for a second should be
    /// runnable again after 10ms, not after the second.
    fn wake_due(&mut self, heap: &mut Heap) {
        if self.sleeping == 0 && self.io_waiting == 0 {
            return;
        }
        let now = std::time::Instant::now();
        for i in 0..self.slots.len() {
            // A socket wait with a deadline is also a sleep: when the clock
            // runs out first, the answer is `false`.
            let (answer, was_io) = match &self.slots[i] {
                Slot::Blocked(_, Waiting::Until(t)) if *t <= now => (Value::Empty, false),
                Slot::Blocked(_, Waiting::Io { deadline: Some(t), .. }) if *t <= now => (Value::Bool(false), true),
                _ => continue,
            };
            self.wake(heap, i, Ok(answer));
            if was_io {
                self.io_waiting -= 1;
            } else {
                self.sleeping -= 1;
            }
        }
    }

    /// Puts every task whose socket is ready back on the queue, waiting at
    /// most `timeout` for one to become so — **the one place a single
    /// thread waits on the network**, by way of [`crate::os::poll_ready`].
    ///
    /// Called with a zero timeout before each task switch (`wake_due`'s
    /// reason: a socket that became readable while another task ran for a
    /// second should not wait for that second to end), and with the nearest
    /// sleeper's deadline — or no limit — when nothing at all is ready.
    /// [`drive_main`]'s threads wait on the network the same way, in the
    /// same three steps, except that the lock is let go of around the
    /// middle one ([`Locked::poll`]).
    fn wake_io(&mut self, heap: &mut Heap, timeout: Option<std::time::Duration>) {
        if self.io_waiting == 0 {
            return;
        }
        let waits = self.io_snapshot();
        let fds: Vec<(i32, crate::os::Interest)> = waits.iter().map(|w| (w.fd, w.interest)).collect();
        // Native unless it cannot block: other threads may collect while
        // this one waits on the network.
        let polled = match timeout {
            Some(t) if t.is_zero() => crate::os::poll_ready(&fds, timeout),
            _ => heap.native(|| crate::os::poll_ready(&fds, timeout)),
        };
        self.apply_io(heap, &waits, polled);
    }

    /// Every socket wait, and which slot it is in — what a `poll` is built
    /// from.
    fn io_snapshot(&self) -> Vec<IoWait> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(slot, s)| match s {
                Slot::Blocked(_, Waiting::Io { fd, interest, deadline }) => {
                    Some(IoWait { slot, fd: *fd, interest: *interest, deadline: *deadline })
                }
                _ => None,
            })
            .collect()
    }

    /// Wakes the tasks `polled` says are ready — `polled` being the answer
    /// for `waits` in the same order, perhaps with more after it (a
    /// poller's self-pipe).
    ///
    /// A `poll` failure wakes everyone waiting: each task's retry then fails
    /// with a message of its own, which beats parking forever on an error
    /// the scheduler cannot attribute.
    ///
    /// **Each slot is checked again** before it is woken: when the lock was
    /// let go of around the `poll`, the clock may have woken the task
    /// (`net-wait-for`'s deadline) and the task may since be waiting on
    /// something else — possibly another socket. Only a slot still waiting
    /// on exactly what was polled for is the one the answer is about.
    fn apply_io(&mut self, heap: &mut Heap, waits: &[IoWait], polled: std::io::Result<Vec<(i32, bool)>>) {
        let ready: Vec<bool> = match polled {
            Ok(r) => r.iter().take(waits.len()).map(|(_, r)| *r).collect(),
            Err(_) => vec![true; waits.len()],
        };
        for (w, ready) in waits.iter().zip(ready) {
            let still = matches!(
                &self.slots[w.slot],
                Slot::Blocked(_, Waiting::Io { fd, interest, deadline })
                    if *fd == w.fd && *interest == w.interest && *deadline == w.deadline
            );
            if !ready || !still {
                continue;
            }
            self.wake(heap, w.slot, Ok(io_ready_answer(w.deadline)));
            self.io_waiting -= 1;
        }
    }

    /// The nearest deadline any task is sleeping until.
    ///
    /// `None` means nobody is: with nothing ready either, every remaining task
    /// is waiting on something that will never happen.
    fn earliest_deadline(&self) -> Option<std::time::Instant> {
        if self.sleeping == 0 && self.io_waiting == 0 {
            return None;
        }
        self.slots
            .iter()
            .filter_map(|s| match s {
                Slot::Blocked(_, Waiting::Until(t)) => Some(*t),
                Slot::Blocked(_, Waiting::Io { deadline: Some(t), .. }) => Some(*t),
                _ => None,
            })
            .min()
    }
}

/// One task's socket wait, as it stood when a `poll` was built — see
/// [`Scheduler::apply_io`] for why the whole wait is kept and not just the
/// descriptor.
struct IoWait {
    slot: usize,
    fd: i32,
    interest: crate::os::Interest,
    deadline: Option<std::time::Instant>,
}

/// `(ARM . VALUE)` — what a `select` answers with. The arm index is the
/// whole of the dispatch, and the value is the receive's `Option<T>` (or
/// unit for a send or an `else`).
///
/// The `cons` can collect, and until it returns the payload — a `some` box
/// built moments ago — is reachable from nothing the collector walks, so it
/// is rooted around the allocation.
fn select_answer(heap: &mut Heap, arm: usize, payload: Value) -> Result<Value, SchedError> {
    let mut s = RootScope::new(heap);
    s.push_root(payload);
    s.cons(Value::Int(arm as i64), payload).map_err(|e| SchedError::Internal(e.to_string()))
}

/// Hands `answer` to a parked task on **its own** root stack — see
/// [`TaskBody::deliver`] for why the switch matters.
fn deliver_to<B: TaskBody>(heap: &mut Heap, slot: &mut TaskSlot<B>, answer: Result<Value, SchedError>) {
    let home = heap.current_root_stack();
    heap.switch_to_root_stack(slot.roots);
    slot.task.deliver(heap, answer);
    heap.switch_to_root_stack(home);
}

// ---- one scheduler, several threads -------------------------------------------

/// A scheduler several OS threads step tasks from — `docs/dev/os-threads-design.md`
/// §8. One thread calls [`drive_main`] and waits for the main task; the
/// others ([`start_workers`]) run [`drive_worker`] until [`Self::shutdown`].
///
/// **Every wait for the lock is native** ([`Self::lock`]), and that is what
/// lets the lock be held across a heap operation that can stop for a
/// collection — a `select`'s answer is a `cons`: a thread parked for a
/// collection while holding it is only in the way of threads that promised
/// not to touch the heap while they wait. The other direction never
/// happens: nothing holding one of the heap's own locks takes this one.
///
/// Stepping a task holds no lock: the step can ask the scheduler a question
/// of its own ([`answer_now`]).
pub struct SchedShared<B: TaskBody> {
    state: parking_lot::Mutex<SharedState<B>>,
    /// Where idle threads wait for [`Scheduler::dirty`].
    changed: parking_lot::Condvar,
    /// How an idle thread that is waiting on the network is woken instead —
    /// see [`crate::os::SelfPipe`].
    io_wake: crate::os::SelfPipe,
    /// The threads a [`Crew`] started this drive, for its end to join. A lock
    /// of its own, never held together with `state`: a `thread` starts its
    /// OS thread with `state` held and only then records it here.
    joinable: parking_lot::Mutex<Vec<std::thread::JoinHandle<()>>>,
    /// Whether to look at another task after **every single step**, rather
    /// than letting the running one keep going until it yields, waits or
    /// finishes.
    ///
    /// Cooperative scheduling means `false`, and nothing in the language can
    /// set it. The tests do: switching every step is the harshest check there
    /// is that a suspended task's frames and roots survive whatever another
    /// task does in between — the same role `gc_stress` plays for a single
    /// task's allocations, and the reason this knob is on the real scheduler
    /// rather than in a test harness of its own. The order it is harsh about
    /// is one thread's, which is all of it when nothing else steps tasks
    /// (`TYPELISP_THREADS=1`, or an interpreter's drive with no compiled
    /// task for a worker to take).
    every_step: std::sync::atomic::AtomicBool,
}

struct SharedState<B: TaskBody> {
    sched: Scheduler<B>,
    /// Whether some thread is in `poll` over the scheduler's socket waits
    /// with the lock let go of. One at a time: the others wait on the
    /// condition variable, and a change the poller should see kicks the
    /// pipe.
    polling: bool,
    /// The first task failure the program has had, and which task it was.
    /// [`drive_main`] reports it; a failure ends the program whichever task
    /// it was in (Go's rule for an unrecovered panic).
    failure: Option<(TaskId, B::Error)>,
    /// Workers take no more tasks: the program failed, or [`SchedShared::shutdown`],
    /// or a [`Crew`]'s drive is over.
    stopped: bool,
    /// Threads that live for one drive at a time, or `None` for workers that
    /// live as long as the program ([`start_workers`]).
    crew: Option<Crew>,
}

/// The threads of a scheduler whose owner is not always driving it — the
/// interpreter's, which drives it once per top-level evaluation and does
/// other things (reads the next line, checks the next form) in between.
/// See [`install_crew`].
///
/// Workers start with the drive, and only once there is a task one of them
/// may take; the drive's end stops them, and a `thread`'s OS thread with
/// them, and joins every one. Between two drives no thread steps a task, so
/// nothing has to reach a safepoint while the owner is away from the heap,
/// and nothing outlives the owner either: a crew's threads run compiled
/// code the owner's JIT holds.
struct Crew {
    /// How many workers a drive may start.
    workers: usize,
    /// Starts one worker thread — see [`install_crew`].
    spawn: std::sync::Arc<dyn Fn(usize) -> std::io::Result<std::thread::JoinHandle<()>> + Send + Sync>,
    /// Whether this drive's workers have been started.
    up: bool,
    /// Whether the last drive's end stopped the OS thread of a pinned task
    /// that is still alive, so this one has to start it again.
    orphans: bool,
}

impl<B: TaskBody> SchedShared<B> {
    /// Fails only if the operating system will not give this process a
    /// pipe.
    pub fn new() -> std::io::Result<SchedShared<B>> {
        Ok(SchedShared {
            state: parking_lot::Mutex::new(SharedState {
                sched: Scheduler::new(),
                polling: false,
                failure: None,
                stopped: false,
                crew: None,
            }),
            changed: parking_lot::Condvar::new(),
            io_wake: crate::os::SelfPipe::new()?,
            joinable: parking_lot::Mutex::new(Vec::new()),
            every_step: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Takes the lock — **as native**, because the thread holding it may be
    /// parked for a collection that is waiting for this one.
    ///
    /// Taking it is therefore a point where another thread may collect:
    /// what the caller still needs has to be rooted, as around a `cons`.
    pub fn lock(&self, heap: &mut Heap) -> Locked<'_, B> {
        let guard = heap.native(|| self.state.lock());
        Locked { guard, shared: self }
    }

    /// Stops the workers: each one returns from [`drive_worker`] the next
    /// time it looks for a task. A task one of them is stepping runs until
    /// it next stops.
    pub fn shutdown(&self, heap: &mut Heap) {
        let mut s = self.lock(heap);
        s.guard.stopped = true;
        s.guard.sched.dirty = true;
    }

    /// Switch tasks after every step — the tests' knob, see the field.
    pub fn set_switch_every_step(&self, on: bool) {
        self.every_step.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// Ends a [`Crew`]'s drive: stops its threads, waits for each to finish
    /// the step it is in, and lets the next drive start them again. Nothing
    /// for a scheduler without a crew.
    ///
    /// The wait is native — a thread finishing its step may need a
    /// collection, and this one must not be in the way of it.
    fn end_crew(&self, heap: &mut Heap) {
        {
            let mut s = self.lock(heap);
            if s.guard.crew.is_none() {
                return;
            }
            s.guard.stopped = true;
            s.guard.sched.dirty = true;
        }
        let threads = std::mem::take(&mut *self.joinable.lock());
        let joined: Vec<std::thread::Result<()>> = heap.native(|| threads.into_iter().map(|t| t.join()).collect());
        for r in joined {
            if let Err(payload) = r {
                std::panic::resume_unwind(payload);
            }
        }
        let mut s = self.lock(heap);
        s.guard.stopped = false;
        let had_pinned = !s.guard.sched.pinned_threads.is_empty();
        s.guard.sched.pinned_threads.clear();
        if let Some(crew) = &mut s.guard.crew {
            crew.up = false;
            crew.orphans |= had_pinned;
        }
    }
}

impl<B: TaskBody> AnswerNow for SchedShared<B> {
    /// Whatever `w` carries is rooted while the lock is taken — taking it can
    /// let another thread collect, and a `send`'s value or a `go`'s closure
    /// is in nothing but `w` on the way here from a machine frame.
    fn answer_now(&self, heap: &mut Heap, w: &Waiting) -> Option<Result<Value, SchedError>> {
        let carried = waiting_values(w);
        for v in &carried {
            heap.push_root(*v);
        }
        let answer = self.lock(heap).try_now(heap, w);
        for _ in &carried {
            heap.pop_root();
        }
        answer
    }
}

/// The heap values a wait carries: a `send`'s value, a `go`'s closure, a
/// `select`'s send arms.
fn waiting_values(w: &Waiting) -> Vec<Value> {
    match w {
        Waiting::Chan(ChanOp::Send(_, v)) | Waiting::Spawn(v) | Waiting::SpawnThread(v) => vec![*v],
        Waiting::Select { ops, .. } => ops
            .iter()
            .filter_map(|op| match op {
                SelectOp::Send(_, v) => Some(*v),
                SelectOp::Recv(..) => None,
            })
            .collect(),
        Waiting::Task(_) | Waiting::Yield | Waiting::Until(_) | Waiting::Chan(_) | Waiting::Io { .. } | Waiting::Main => {
            Vec::new()
        }
    }
}

/// The lock on a [`SchedShared`], dereferencing to its [`Scheduler`].
///
/// Letting go of it wakes the idle threads if anything they would want to
/// know about happened meanwhile ([`Scheduler::dirty`]) — the one place that
/// happens, so no operation has to remember to.
pub struct Locked<'a, B: TaskBody> {
    guard: parking_lot::MutexGuard<'a, SharedState<B>>,
    shared: &'a SchedShared<B>,
}

impl<B: TaskBody> std::ops::Deref for Locked<'_, B> {
    type Target = Scheduler<B>;
    fn deref(&self) -> &Scheduler<B> {
        &self.guard.sched
    }
}

impl<B: TaskBody> std::ops::DerefMut for Locked<'_, B> {
    fn deref_mut(&mut self) -> &mut Scheduler<B> {
        &mut self.guard.sched
    }
}

impl<B: TaskBody> Drop for Locked<'_, B> {
    fn drop(&mut self) {
        self.flush();
    }
}

impl<'a, B: TaskBody> Locked<'a, B> {
    /// Wakes every idle thread if something changed: the ones waiting on the
    /// condition variable, and the poller through its pipe.
    fn flush(&mut self) {
        if std::mem::take(&mut self.guard.sched.dirty) {
            self.shared.changed.notify_all();
            if self.guard.polling {
                self.shared.io_wake.kick();
            }
        }
    }

    /// Records a task's failure, and stops the workers — the program is
    /// over. The first failure is the one reported.
    fn fail(&mut self, id: TaskId, e: B::Error) {
        self.guard.sched.running -= 1;
        // Nothing is kept for a `wait`: the program is over, and the thread
        // driving the main task reports this instead.
        self.guard.sched.slots[id.0] = Slot::Empty;
        self.record_failure(id, e);
    }

    /// [`Self::fail`] for a task that is not out being stepped.
    fn record_failure(&mut self, id: TaskId, e: B::Error) {
        self.guard.sched.dirty = true;
        self.guard.stopped = true;
        if self.guard.failure.is_none() {
            self.guard.failure = Some((id, e));
        }
    }

    /// What the thread driving a [`Crew`] does before it looks for a task:
    /// starts the OS threads of pinned tasks the last drive's end stopped,
    /// and the workers, once there is a ready task one of them may take.
    /// Started with the lock held — each thread takes it before it does
    /// anything. A worker the system will not start is the drive's error,
    /// as it is the executable's (`run_program`): `TYPELISP_THREADS` asked
    /// for it.
    fn man_the_crew(&mut self) -> Result<(), SchedError> {
        let Some(crew) = &mut self.guard.crew else { return Ok(()) };
        if std::mem::take(&mut crew.orphans) {
            if let Err((id, e)) = self.guard.sched.restart_pinned() {
                self.record_failure(id, e.into());
                return Ok(());
            }
        }
        let Some(crew) = &self.guard.crew else { return Ok(()) };
        if crew.up || crew.workers == 0 {
            return Ok(());
        }
        let slots = &self.guard.sched.slots;
        let for_workers = self.guard.sched.ready.iter().any(|t| !matches!(&slots[t.0], Slot::Parked(s) if s.main_only));
        if !for_workers {
            return Ok(());
        }
        let (n, spawn) = (crew.workers, std::sync::Arc::clone(&crew.spawn));
        if let Some(crew) = &mut self.guard.crew {
            crew.up = true;
        }
        for i in 0..n {
            let t = spawn(i).map_err(|e| {
                SchedError::Internal(format!("a scheduler worker thread could not be started: {}", e))
            })?;
            self.shared.joinable.lock().push(t);
        }
        Ok(())
    }

    /// Waits — native — until something changes or `until` passes.
    fn wait(mut self, heap: &mut Heap, until: Option<std::time::Duration>) -> Locked<'a, B> {
        self.flush();
        let changed = &self.shared.changed;
        let guard = &mut self.guard;
        heap.native(|| match until {
            Some(d) => {
                changed.wait_for(guard, d);
            }
            None => changed.wait(guard),
        });
        self
    }

    /// Waits on the scheduler's sockets — and the self-pipe, so that a change
    /// wakes it — for at most `until`, with the lock let go of, and wakes the
    /// tasks whose sockets are ready. The same three steps as
    /// [`Scheduler::wake_io`].
    fn poll(mut self, heap: &mut Heap, until: Option<std::time::Duration>) -> Locked<'a, B> {
        let shared = self.shared;
        self.guard.polling = true;
        let waits = self.guard.sched.io_snapshot();
        let mut fds: Vec<(i32, crate::os::Interest)> = waits.iter().map(|w| (w.fd, w.interest)).collect();
        fds.push((shared.io_wake.read_fd(), crate::os::Interest::Readable));
        drop(self);
        let polled = heap.native(|| crate::os::poll_ready(&fds, until));
        shared.io_wake.drain();
        let mut s = shared.lock(heap);
        s.guard.polling = false;
        s.guard.sched.apply_io(heap, &waits, polled);
        s
    }
}

/// What a thread looking for work found.
enum Next<B: TaskBody> {
    /// A task to step.
    Run(TaskId, TaskSlot<B>),
    /// What the main task came to — only for the thread waiting on it.
    Main(Result<Value, B::Error>),
    /// Nothing more for this thread: the workers are stopped, or a pinned
    /// thread's task has finished.
    Stop,
}

/// Which thread is looking for work, and so which tasks it may take.
#[derive(Clone, Copy)]
enum Who {
    /// The thread waiting on the main task: any unpinned task.
    Main(TaskId),
    /// A worker: any unpinned task the interpreter is not needed for.
    Worker,
    /// The OS thread a `thread` started: its own task and no other.
    Pinned(TaskId),
}

/// Which ready tasks [`Scheduler::next_ready`] may hand out.
#[derive(Clone, Copy)]
enum Taker {
    /// The thread that owns the interpreter, if there is one: the oldest
    /// ready task that is not pinned, whatever it needs.
    Main,
    /// Any other thread stepping the shared queue: the oldest ready task
    /// that neither is pinned nor needs the interpreter. **The one thing
    /// that keeps a main-only task on the interpreter's thread** — see
    /// `MainOnly` in the interpreter's task body.
    Worker,
    /// The OS thread a `thread` started: its own task.
    Pinned(TaskId),
}

/// Waits for something this thread can do: a ready task, or — for the
/// thread waiting on `main` — the main task's end or the program's failure.
///
/// Idle, a thread waits on the clock and the condition variable, or — if
/// sockets are waited on and nobody is polling them yet — on the network.
/// "Every task is blocked" is decided only by the thread that has a main
/// task to wait for: a worker idle between two drives is not a deadlock.
fn next<B: TaskBody>(shared: &SchedShared<B>, heap: &mut Heap, who: Who) -> Next<B> {
    let mut s = shared.lock(heap);
    loop {
        match who {
            Who::Main(main) => {
                if let Some((id, e)) = s.guard.failure.take() {
                    return Next::Main(Err(if id == main { e } else { B::failure_left_task(e) }));
                }
                if let Some(v) = s.take_main_result(heap, main) {
                    return Next::Main(Ok(v));
                }
            }
            Who::Worker if s.guard.stopped => return Next::Stop,
            Who::Worker => {}
            // A pinned thread ends with its task: once the result is kept
            // (`finish`) there is nothing left that this thread may run —
            // and once the task has moved to the interpreter's thread
            // (`put_back_for_main`), nothing is left for this one either.
            Who::Pinned(own) => {
                let ended = match s.slots.get(own.0) {
                    Some(Slot::Parked(slot)) | Some(Slot::Blocked(slot, _)) => !slot.pinned,
                    Some(Slot::Running) => false,
                    Some(Slot::Done(_)) | Some(Slot::Empty) | None => true,
                };
                if s.guard.stopped || ended {
                    return Next::Stop;
                }
            }
        }
        if matches!(who, Who::Main(_)) {
            if let Err(e) = s.man_the_crew() {
                s.guard.stopped = true;
                s.dirty = true;
                return Next::Main(Err(e.into()));
            }
        }
        s.wake_due(heap);
        s.wake_io(heap, Some(std::time::Duration::ZERO));
        let taker = match who {
            Who::Main(_) => Taker::Main,
            Who::Worker => Taker::Worker,
            Who::Pinned(own) => Taker::Pinned(own),
        };
        if let Some((id, slot)) = s.next_ready(taker) {
            return Next::Run(id, slot);
        }
        let deadline = s.earliest_deadline();
        let io_waiting = s.io_waiting > 0;
        if matches!(who, Who::Main(_))
            && s.running == 0
            && s.pinned_ready.is_empty()
            && deadline.is_none()
            && !io_waiting
        {
            // Nothing is out being stepped on any thread, no pinned thread
            // has its task ready to take, and nothing can become ready by
            // itself: the main task is waiting on something that will never
            // happen.
            s.guard.stopped = true;
            s.dirty = true;
            return Next::Main(Err(
                SchedError::Internal("scheduler: every task is blocked and none can proceed".to_string()).into()
            ));
        }
        let until = deadline.map(|d| d.saturating_duration_since(std::time::Instant::now()));
        s = if io_waiting && !s.guard.polling { s.poll(heap, until) } else { s.wait(heap, until) };
    }
}

/// Steps one task until it stops, and puts it wherever it goes next.
///
/// The lock is taken **before** the task's roots are given up: taking it can
/// let another thread collect, and a finished task's result is rooted only
/// in the task's own stack until `finish` roots it in the scheduler's.
fn run_one<B: TaskBody>(
    shared: &SchedShared<B>,
    heap: &mut Heap,
    cx: &B::Cx,
    home: RootStackId,
    id: TaskId,
    mut slot: TaskSlot<B>,
) {
    heap.switch_to_root_stack(slot.roots);
    let every_step = shared.every_step.load(std::sync::atomic::Ordering::Relaxed);
    let outcome = loop {
        match slot.task.step(heap, cx) {
            Progress::Running if every_step => break Progress::Running,
            // Between steps the task's state is all in its frames and root
            // stack, so another thread's collection may run here.
            Progress::Running => heap.safepoint(),
            done => break done,
        }
    };
    let mut s = shared.lock(heap);
    match outcome {
        Progress::Running => {
            heap.switch_to_root_stack(home);
            s.put_back(id, slot);
        }
        Progress::NeedsMain => {
            heap.switch_to_root_stack(home);
            s.put_back_for_main(id, slot);
        }
        Progress::Blocked(w) => {
            heap.switch_to_root_stack(home);
            s.block(heap, id, slot, w);
        }
        Progress::Done(r) => {
            heap.truncate_roots(slot.task.sbase());
            heap.switch_to_root_stack(home);
            heap.drop_root_stack(slot.roots);
            match r {
                Ok(v) => s.finish(heap, id, v),
                Err(e) => s.fail(id, e),
            }
        }
    }
}

/// Runs tasks on this thread — alongside whatever workers run them on
/// theirs — until `main` finishes, and returns its result.
///
/// The main task may be stepped by any thread. Its result is kept rooted by
/// the scheduler until this thread takes it, and is *not* rooted once
/// returned: the caller has it in hand and roots it if it keeps it, as an
/// ordinary evaluation's.
///
/// Tasks outlive the main task: whatever is left keeps its state for the
/// next drive — which is what makes `(go ...)` at a REPL prompt behave, and
/// what cuts every other task off when a program's `main` returns (Go's
/// rule). An executable's workers go on running them; a [`Crew`]'s stop
/// here, and pick them up again with the next drive.
pub fn drive_main<B: TaskBody>(
    shared: &SchedShared<B>,
    heap: &mut Heap,
    cx: &B::Cx,
    main: TaskId,
) -> Result<Value, B::Error> {
    let _driving = DrivingGuard::publish(shared);
    let home = heap.current_root_stack();
    let result = loop {
        match next(shared, heap, Who::Main(main)) {
            Next::Run(id, slot) => run_one(shared, heap, cx, home, id, slot),
            Next::Main(r) => break r,
            Next::Stop => unreachable!("the thread waiting on the main task is not a worker"),
        }
    };
    // Ending a crew waits for its threads, and another thread's collection
    // may run meanwhile: the result is in nothing but this local.
    let carried = match &result {
        Ok(v) => vec![*v],
        Err(e) => B::failure_values(e),
    };
    let base = heap.root_count();
    for v in &carried {
        heap.push_root(*v);
    }
    shared.end_crew(heap);
    heap.truncate_roots(base);
    result
}

/// A worker thread's loop: steps whatever task is ready, until
/// [`SchedShared::shutdown`], a failure, or the end of a [`Crew`]'s drive
/// stops the workers.
pub fn drive_worker<B: TaskBody>(shared: &SchedShared<B>, heap: &mut Heap, cx: &B::Cx) {
    let _driving = DrivingGuard::publish(shared);
    let home = heap.current_root_stack();
    loop {
        match next(shared, heap, Who::Worker) {
            Next::Run(id, slot) => run_one(shared, heap, cx, home, id, slot),
            Next::Stop => return,
            Next::Main(_) => unreachable!("a worker waits on no main task"),
        }
    }
}

/// The loop of the OS thread a `thread` started: steps task `own` — and
/// nothing else — whenever it is ready, and returns once it has finished
/// (or the program has failed, or the task has moved to the interpreter's
/// thread, or a [`Crew`]'s drive is over). While the task waits, so does the
/// thread, natively, as an idle worker does; while it runs, it runs here and
/// nowhere else, so a call that blocks the thread holds up no other task.
pub fn drive_pinned<B: TaskBody>(shared: &SchedShared<B>, heap: &mut Heap, cx: &B::Cx, own: TaskId) {
    let _driving = DrivingGuard::publish(shared);
    let home = heap.current_root_stack();
    loop {
        match next(shared, heap, Who::Pinned(own)) {
            Next::Run(id, slot) => run_one(shared, heap, cx, home, id, slot),
            Next::Stop => return,
            Next::Main(_) => unreachable!("a pinned thread waits on no main task"),
        }
    }
}

/// How a thread that steps tasks for `shared` is set up: a view of its own
/// onto the heap the scheduler's tasks live in, registered as its active
/// heap, and `setup` — which is where it takes on whatever else the thread
/// that started it has per thread (the runtime's and the printer's shared
/// tables). `cx` is what [`TaskBody::step`] gets there: there is no
/// interpreter on any of these threads.
struct ThreadKit<B: TaskBody + 'static> {
    shared: std::sync::Weak<SchedShared<B>>,
    heap: std::sync::Arc<typelisp_mem::heap::HeapShared>,
    setup: std::sync::Arc<dyn Fn() + Send + Sync>,
    cx: &'static B::Cx,
}

impl<B> ThreadKit<B>
where
    B: TaskBody + Send + 'static,
    B::Error: Send,
    B::Cx: Sync,
{
    /// Starts an OS thread named `name` that runs `run` on `shared`, set up
    /// as the type's doc comment says.
    ///
    /// The kit holds a `Weak`: the scheduler holds the kit (in its thread
    /// starter), and a strong handle there would keep it alive forever.
    /// Upgrading cannot fail — a thread is started by one holding this
    /// scheduler, and the started thread keeps the strong handle for as
    /// long as it runs.
    fn spawn(
        &self,
        name: String,
        run: impl FnOnce(&SchedShared<B>, &mut Heap, &B::Cx) + Send + 'static,
    ) -> std::io::Result<std::thread::JoinHandle<()>> {
        let shared = self.shared.upgrade().expect("a scheduler starting a thread is alive");
        let heap_shared = std::sync::Arc::clone(&self.heap);
        let setup = std::sync::Arc::clone(&self.setup);
        let cx = self.cx;
        std::thread::Builder::new().name(name).spawn(move || {
            let mut view = Heap::attach(&heap_shared);
            typelisp_abi::set_active_heap(&mut view);
            setup();
            run(&shared, &mut view, cx);
            typelisp_abi::set_active_heap(std::ptr::null_mut());
        })
    }

    /// What lets `thread` start OS threads on this scheduler: each runs
    /// [`drive_pinned`]. With `keep`, the thread is recorded for a
    /// [`Crew`]'s end to join; without, it is detached, and its resources go
    /// when it ends.
    fn thread_starter(self: &std::sync::Arc<Self>, keep: bool) -> ThreadStarter {
        let kit = std::sync::Arc::clone(self);
        std::sync::Arc::new(move |own: TaskId| {
            let t = kit.spawn(format!("typelisp-thread-{}", own.0), move |shared, heap, cx| {
                drive_pinned(shared, heap, cx, own)
            })?;
            if keep {
                let shared = kit.shared.upgrade().expect("a scheduler starting a thread is alive");
                shared.joinable.lock().push(t);
            }
            Ok(())
        })
    }
}

/// Starts `n` worker threads on `shared`, each running [`drive_worker`]
/// until the workers are stopped (see [`ThreadKit`] for how each is set up),
/// and lets `thread` start OS threads from then on, set up the same way and
/// running [`drive_pinned`] — with no workers (`n == 0`) too, since a
/// `thread` is not one of them.
///
/// Workers that live as long as the program: an executable's. The
/// interpreter's live for one drive at a time ([`install_crew`]).
pub fn start_workers<B>(
    shared: &std::sync::Arc<SchedShared<B>>,
    heap: &mut Heap,
    n: usize,
    cx: &'static B::Cx,
    setup: impl Fn() + Send + Sync + 'static,
) -> std::io::Result<Vec<std::thread::JoinHandle<()>>>
where
    B: TaskBody + Send + 'static,
    B::Error: Send,
    B::Cx: Sync,
{
    let kit = std::sync::Arc::new(ThreadKit {
        shared: std::sync::Arc::downgrade(shared),
        heap: heap.shared_handle(),
        setup: std::sync::Arc::new(setup),
        cx,
    });
    shared.lock(heap).set_thread_starter(kit.thread_starter(false));
    (0..n).map(|i| kit.spawn(format!("typelisp-worker-{}", i), drive_worker)).collect()
}

/// Gives `shared` a [`Crew`] of `n` workers: threads that exist only while a
/// [`drive_main`] runs, started once a ready task is one they may take, and
/// stopped and joined when the drive ends. `thread` starts OS threads the
/// same way, stopped with the rest and started again by the next drive.
/// See [`ThreadKit`] for how each is set up.
///
/// What an owner that is not always driving has — the interpreter, which
/// reads the next line and checks the next form between two drives, and
/// whose JIT holds the code the threads run. No thread steps a task while
/// the owner is away from the heap, so nothing waits on a thread that is
/// blocked reading a terminal, and nothing outlives the owner.
pub fn install_crew<B>(
    shared: &std::sync::Arc<SchedShared<B>>,
    heap: &mut Heap,
    n: usize,
    cx: &'static B::Cx,
    setup: impl Fn() + Send + Sync + 'static,
) where
    B: TaskBody + Send + 'static,
    B::Error: Send,
    B::Cx: Sync,
{
    let kit = std::sync::Arc::new(ThreadKit {
        shared: std::sync::Arc::downgrade(shared),
        heap: heap.shared_handle(),
        setup: std::sync::Arc::new(setup),
        cx,
    });
    let starter = kit.thread_starter(true);
    let spawn = std::sync::Arc::new(move |i: usize| kit.spawn(format!("typelisp-worker-{}", i), drive_worker));
    let mut s = shared.lock(heap);
    s.set_thread_starter(starter);
    s.guard.crew = Some(Crew { workers: n, spawn, up: false, orphans: false });
}

/// How many OS threads run tasks, the one that waits on the main task
/// included: `TYPELISP_THREADS` when it is set, and the machine's
/// parallelism when it is not.
pub fn thread_count() -> Result<usize, String> {
    match std::env::var("TYPELISP_THREADS") {
        Ok(v) => match v.trim().parse::<usize>() {
            Ok(n) if n >= 1 => Ok(n),
            _ => Err(format!("TYPELISP_THREADS={:?} is not a number of threads (a whole number, 1 or more)", v)),
        },
        Err(std::env::VarError::NotPresent) => std::thread::available_parallelism()
            .map(|n| n.get())
            .map_err(|e| format!("cannot tell how many threads to run tasks on ({}); set TYPELISP_THREADS", e)),
        Err(std::env::VarError::NotUnicode(v)) => {
            Err(format!("TYPELISP_THREADS={:?} is not a number of threads (a whole number, 1 or more)", v))
        }
    }
}

// ---- turning a compiled suspension back into a wait ------------------------

/// `(net-wait h interest)` — what to park **this task** on: the socket
/// under handle `h`, for reading (`0`) or writing (`1`).
///
/// The handle is turned into a descriptor here, once, for both the
/// interpreted call and the compiled suspension (`pending_wait`), so the
/// two refuse the same handles in the same words. A closed or non-socket
/// handle is a program error — the stream was closed and then waited on —
/// and panics rather than parking a task nothing will ever wake.
pub fn io_wait(heap: &mut Heap, argv: &[Value], deadline: Option<std::time::Instant>) -> Result<Waiting, SchedError> {
    let handle = match argv.first() {
        Some(Value::Int(h)) => *h,
        other => return Err(SchedError::Internal(format!("net-wait: {:?} is not a stream handle", other))),
    };
    let interest = match argv.get(1) {
        Some(Value::Int(code)) => crate::os::Interest::from_code(*code).ok_or_else(|| {
            SchedError::Internal(format!("net-wait: {} is not an interest (0 readable, 1 writable)", code))
        })?,
        other => return Err(SchedError::Internal(format!("net-wait: {:?} is not an interest", other))),
    };
    let fd = crate::stream::with_streams(heap, |t| t.raw_fd(handle)).map_err(SchedError::Panic)?;
    Ok(Waiting::Io { fd, interest, deadline })
}

/// `secs` from now, with `sleep`'s refusals: negative or NaN is not a
/// duration, and neither is one the clock cannot hold.
pub fn io_deadline(secs: f64) -> Result<std::time::Instant, SchedError> {
    if !(secs >= 0.0) {
        return Err(SchedError::Panic(format!("net-wait-for: {} is not a non-negative number of seconds", secs)));
    }
    let d = std::time::Duration::try_from_secs_f64(secs)
        .map_err(|_| SchedError::Panic(format!("net-wait-for: {} is longer than this can wait", secs)))?;
    Ok(std::time::Instant::now() + d)
}

/// `(sleep secs)` as a wait, with the refusals CL makes: a negative or NaN
/// wait is an error, and there is no duration to build from one. The same
/// words on both sides, because a program must not be able to tell which
/// side its `sleep` ran on.
pub fn sleep_wait(secs: f64) -> Result<Waiting, SchedError> {
    if !(secs >= 0.0) {
        return Err(SchedError::Panic(format!("sleep: {} is not a non-negative number of seconds", secs)));
    }
    let d = std::time::Duration::try_from_secs_f64(secs)
        .map_err(|_| SchedError::Panic(format!("sleep: {} is longer than this can wait", secs)))?;
    Ok(Waiting::Until(std::time::Instant::now() + d))
}

/// What a compiled frame that just returned `STATUS_SUSPEND` is waiting for,
/// and how its answer will come back.
///
/// The compiled side publishes two raw words (`typelisp_abi::call_state`),
/// because the crate it publishes from sits below this one — a `TaskId` and
/// an `Instant` are the scheduler's types. This is the one place that turns
/// them back.
pub fn pending_wait(heap: &mut Heap) -> Result<(Waiting, Wake), SchedError> {
    use typelisp_abi::call_state as cs;
    let (kind, payload, second) = cs::take_pending_suspend().ok_or_else(|| {
        SchedError::Internal("a compiled frame suspended without recording what it was waiting for".to_string())
    })?;
    match kind {
        // Nothing comes back: the wake value is unit, and the resume block
        // does not read the frame's value slot at all (`(suspend ...)` carries
        // kind `0` for these).
        cs::SUSPEND_YIELD => Ok((Waiting::Yield, Wake::Unit)),
        // A compiled loop's back edge offering a turn (C7). Indistinguishable
        // from `(yield)` once a scheduler is the one answering — the two are
        // separate kinds so that a driver *without* a scheduler can tell an
        // offer from a request.
        cs::SUSPEND_SAFEPOINT => Ok((Waiting::Yield, Wake::Unit)),
        cs::SUSPEND_SLEEP => Ok((sleep_wait(f64::from_bits(payload as u64))?, Wake::Unit)),
        // The payload is the `Task<T>` handle, still tagged — reading the id
        // out of the box is this side's job, because `TaskId` is the
        // scheduler's type. The answer comes back as a tagged `Sexpr` whatever
        // `T` is, and the resume block decodes it with the kind the bridge
        // baked in: the same division `catch`/`throw` make for a thrown value.
        cs::SUSPEND_WAIT => {
            let handle = typelisp_abi::decode(payload);
            Ok((Waiting::Task(task_id_of(heap, Some(handle))?), Wake::Tagged))
        }
        // Both words raw: a stream handle and an interest code are plain
        // integers in compiled code (`sleep`'s bits cross the same way), and
        // this is the same resolution the interpreted `net-wait` makes.
        cs::SUSPEND_IO => {
            let w = io_wait(heap, &[Value::Int(payload), Value::Int(second)], None)?;
            Ok((w, Wake::Unit))
        }
        // The handle and the interest share the first word (`rt_suspend_io_for`
        // packs them: handle above, interest in the low bit) so the seconds'
        // bits can have the second. The answer is a `bool`, tagged like
        // every typed wake value.
        cs::SUSPEND_IO_FOR => {
            let d = io_deadline(f64::from_bits(second as u64))?;
            let w = io_wait(heap, &[Value::Int(payload >> 1), Value::Int(payload & 1)], Some(d))?;
            Ok((w, Wake::Tagged))
        }
        // The channel operations. The handle is tagged, like `wait`'s; so is
        // `send`'s value, which the suspension site tagged per the element's
        // own `Repr::field_kind` because the driver has only a word.
        // The capacity is an ordinary `i32` argument, so it crosses raw — the
        // suspension site tags only what the driver could not otherwise read.
        cs::SUSPEND_CHAN_NEW => Ok((Waiting::Chan(ChanOp::New(payload)), Wake::Tagged)),
        // The closure is tagged, and the answer — the handle — comes back
        // tagged like every other typed wake value.
        cs::SUSPEND_GO => Ok((Waiting::Spawn(typelisp_abi::decode(payload)), Wake::Tagged)),
        cs::SUSPEND_THREAD => Ok((Waiting::SpawnThread(typelisp_abi::decode(payload)), Wake::Tagged)),
        cs::SUSPEND_MAIN => Ok((Waiting::Main, Wake::Unit)),
        // `Wake::Tagged` for every typed answer: a wake value always crosses
        // back **tagged**, whatever its type, and the resume block decodes it
        // with the kind the bridge baked in — the same division `wait` makes.
        cs::SUSPEND_CHAN_LEN => {
            Ok((Waiting::Chan(ChanOp::Len(chan_id_of(heap, Some(typelisp_abi::decode(payload)))?)), Wake::Tagged))
        }
        cs::SUSPEND_CHAN_CAP => {
            Ok((Waiting::Chan(ChanOp::Cap(chan_id_of(heap, Some(typelisp_abi::decode(payload)))?)), Wake::Tagged))
        }
        cs::SUSPEND_CHAN_CLOSE => {
            Ok((Waiting::Chan(ChanOp::Close(chan_id_of(heap, Some(typelisp_abi::decode(payload)))?)), Wake::Unit))
        }
        cs::SUSPEND_CHAN_SEND => {
            let c = chan_id_of(heap, Some(typelisp_abi::decode(payload)))?;
            Ok((Waiting::Chan(ChanOp::Send(c, typelisp_abi::decode(second))), Wake::Unit))
        }
        cs::SUSPEND_CHAN_RECV => {
            let c = chan_id_of(heap, Some(typelisp_abi::decode(payload)))?;
            let key = match typelisp_abi::decode(second) {
                Value::Str(id) => heap.string(id).to_string(),
                other => {
                    return Err(SchedError::Internal(format!(
                        "a compiled `recv` carried {:?} where its `Option<T>` key should be",
                        other
                    )))
                }
            };
            Ok((Waiting::Chan(ChanOp::Recv(c, key)), Wake::Tagged))
        }
        // The arms travel in their own slot, and every word in them is
        // tagged — the descriptor was built in a compiled frame's slots, and
        // the collector walks those.
        cs::SUSPEND_CHAN_SELECT => {
            let words = cs::take_pending_select();
            let untag = |w: i64| w >> typelisp_mem::tagged::FIXNUM_SHIFT;
            if words.len() < 2 {
                return Err(SchedError::Internal("a compiled `select` carried no arms".to_string()));
            }
            let n = untag(words[0]).max(0) as usize;
            let has_else = untag(words[1]) != 0;
            if words.len() < 2 + 3 * n {
                return Err(SchedError::Internal(format!(
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
                            return Err(SchedError::Internal(format!(
                                "a compiled `select` receive arm carried {:?} where its key should be",
                                other
                            )))
                        }
                    }
                } else {
                    SelectOp::Send(c, extra)
                });
            }
            Ok((Waiting::Select { ops, has_else }, Wake::Tagged))
        }
        other => Err(SchedError::Internal(format!(
            "a compiled frame asked to wait on kind {} (payload {}), which this build does not lower",
            other, payload
        ))),
    }
}

// ---- a task that is a compiled chain and nothing else -------------------------

/// How a compiled-only task fails — the answer a program's own failure gets
/// in an executable, split the way the interpreter's `EvalError` splits it so
/// the two front ends say the same things about the same program.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskFailure {
    /// A `(panic ...)`, a runtime error, or a scheduler refusal: `panic: msg`.
    Panic(String),
    /// A `throw` no `catch` in the task claimed. Carries the tag.
    Throw(String),
    /// The runtime disagreeing with itself — every task blocked, a handle of
    /// the wrong shape.
    Internal(String),
}

impl From<SchedError> for TaskFailure {
    fn from(e: SchedError) -> TaskFailure {
        match e {
            SchedError::Panic(m) => TaskFailure::Panic(m),
            SchedError::Internal(m) => TaskFailure::Internal(m),
        }
    }
}

impl std::fmt::Display for TaskFailure {
    /// The line an executable prints on the way out — the same wording
    /// `EvalError`'s `Display` uses for the same failures.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskFailure::Panic(m) => write!(f, "panic: {}", m),
            TaskFailure::Throw(tag) => write!(f, "throw: no enclosing (catch '{}) for this throw", tag),
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
/// `go` started applies a closure, and from then on every step is a drive of
/// the standing chain. What it needs from the heap is two state slots at
/// `sbase`, for the same reason the interpreter's task has them — a value
/// on its way in (a wake value, the closure to apply, a parked `send`'s
/// offer) is a Rust local until the chain takes it, and nothing else roots
/// it.
///
/// **Every answer is a tagged word.** The closure a `go` site builds tags
/// its result before returning (`core_bridge::translate_go`), and an
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
    /// Apply this closure to no arguments — a task `go` started. Rooted in
    /// the first state slot.
    Start(Value),
    /// Enter this with these argument words — an interpreted `thread`'s
    /// task. What the words point at is rooted in the state slots until the
    /// entry's prologue has copied them into its frame.
    EnterWith(Entry, Vec<i64>),
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
            chain: crate::coroutine::FrameStack::new(),
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
    pub fn enter_with(heap: &mut Heap, entry: Entry, args: Vec<i64>, held: Value, answer: AnswerDecoder) -> CompiledTask {
        let sbase = heap.root_count();
        heap.push_root(match &entry {
            Entry::Closure(c) => *c,
            Entry::Body(_) => Value::Empty,
        });
        heap.push_root(held);
        CompiledTask {
            chain: crate::coroutine::FrameStack::new(),
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
            | CompiledState::Blocked(Waiting::Spawn(v))
            | CompiledState::Blocked(Waiting::SpawnThread(v)) => *v,
            _ => Value::Empty,
        };
        heap.set_root(self.sbase, held);
        heap.set_root(self.sbase + 1, Value::Empty);
    }

    /// What a drive that stopped means for the task.
    fn after_drive(&mut self, heap: &mut Heap, outcome: Result<i64, crate::coroutine::Paused>) -> Progress<TaskFailure> {
        match outcome {
            Ok(_) if self.discard_answer => Progress::Done(Ok(Value::Empty)),
            Ok(word) => match &self.answer {
                Some(answer) => Progress::Done(answer(heap, word).map_err(TaskFailure::Internal)),
                None => Progress::Done(Ok(typelisp_abi::decode(word))),
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
                let v = unsafe { crate::apply_on_this_frame(closure, &args, "a task") };
                self.state = CompiledState::Resume(v);
                self.set_slots(heap);
                Progress::Running
            }
            Err(crate::coroutine::Paused::ApplyingDyn { vtable, slot, args }) => {
                // SAFETY: as above, for a `:dyn` site's slot in a vtable this
                // heap's program registered.
                let v = unsafe {
                    let closure = crate::reify_dyn_slot(vtable, slot, "a :dyn call");
                    crate::apply_on_this_frame(closure, &args, "a task")
                };
                self.state = CompiledState::Resume(v);
                self.set_slots(heap);
                Progress::Running
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
unsafe fn unwind_failure(payload: Box<dyn std::any::Any + Send>) -> TaskFailure {
    let payload = match payload.downcast::<typelisp_abi::CompiledPanic>() {
        Ok(p) => return TaskFailure::Panic(p.message),
        Err(other) => other,
    };
    match payload.downcast::<typelisp_abi::CompiledThrow>() {
        Ok(_) => {
            let tag = crate::take_throw().map(|(t, _)| t).unwrap_or_default();
            TaskFailure::Throw(tag)
        }
        Err(other) => std::panic::resume_unwind(other),
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
            chain: crate::coroutine::FrameStack::new(),
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
        crate::restore_parked_unwind(std::mem::take(&mut self.parked_unwind));
        // Taken out to be consumed; every path below either puts the next
        // state back or reports `Done`, after which nothing reads it.
        let state = std::mem::replace(&mut self.state, CompiledState::Resume(0));
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
    /// panic in a goroutine.
    /// None: a thrown value is not kept past the task (`TaskFailure::Throw`
    /// carries the tag alone).
    fn failure_values(_e: &TaskFailure) -> Vec<Value> {
        Vec::new()
    }

    fn failure_left_task(e: TaskFailure) -> TaskFailure {
        match e {
            TaskFailure::Throw(tag) => TaskFailure::Panic(format!("`throw` of `{}` left its task", tag)),
            other => other,
        }
    }
}
