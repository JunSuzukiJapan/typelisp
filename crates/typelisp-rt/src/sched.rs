//! The task scheduler: which task runs next, and what each parked one is
//! waiting for.
//!
//! This is the runtime's, not the interpreter's, because an ahead-of-time
//! compiled executable has no interpreter and still has tasks: `task` starts
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
//! `(recv ch)` with something buffered, a `(task ...)`, a `Chan::new`. The
//! borrow is registered and cleared by a guard on the drive's frame, exactly
//! as `ACTIVE_HEAP` is registered per scope, so nothing outlives its owner.
//!
//! Cooperative, and on several OS threads: nothing preempts a task, but
//! `TYPELISP_THREADS` threads step ready tasks at the same time, all on one
//! heap ([`SchedShared`], [`drive_main`], [`drive_worker`];
//! `docs/dev/os-threads-design.md` §8). An interpreter's tasks hold its
//! `Rc`s and run only on its own thread ([`TaskBody::runs_anywhere`]); its
//! compiled ones go to workers that live for one drive ([`install_crew`]).

use typelisp_mem::prelude::*;
use typelisp_mem::{Heap, RootScope, RootStackId, TypeId, Value};

mod threads;
mod compiled_wait;
mod compiled_task;
mod io;
pub use threads::*;
pub use io::*;
pub use compiled_wait::*;
pub use compiled_task::*;

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

impl core::fmt::Display for SchedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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

    /// A task that applies `closure` — a compiled closure the `task` site built
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
    /// value handed to a woken task (a `Some` box built by `recv`, say)
    /// exists only inside a Rust enum until this runs, and the task is not
    /// stepped again until some other task has had its turn and allocated.
    fn deliver(&mut self, heap: &mut Heap, answer: Result<Value, SchedError>);

    /// The heap values `e` carries — a thrown value, say — which the thread
    /// driving the main task roots while it waits for its threads to stop
    /// ([`drive_main`]).
    fn failure_values(e: &Self::Error) -> Vec<Value>;

    /// What a task's failure does to the program, for a task that is not the
    /// main one — the interpreter turns an escaped `throw` into a panic here,
    /// which is Go's rule for an unrecovered panic.
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
    Until(typelisp_mem::host::Instant),
    /// A `Chan<T>` operation.
    Chan(ChanOp),
    /// `(task ...)` — start a task that applies this closure, and answer with
    /// its handle. Never a wait: answered on the spot like `Chan::new`, and
    /// through the scheduler for the same reason — the table of tasks is
    /// the scheduler's, and only the driver can reach it.
    ///
    /// The closure is rooted through the asking task's state slot while the
    /// operation is in flight, as a parked `send`'s value is. The type is
    /// the handle's ([`task_handle`]).
    Spawn(Value, TypeId),
    /// `(thread ...)` — [`Waiting::Spawn`] for a task that runs on an OS
    /// thread of its own, started for it ([`Scheduler::try_now`]), and
    /// answered with a `Thread<T>` handle. Rooted the way `Spawn`'s closure
    /// is, and typed the way its handle is.
    SpawnThread(Value, TypeId),
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
    ///
    /// `poller` is what watches the descriptor: the one the socket's backend
    /// named, which is the only way such a wait is made.
    Io { fd: i32, interest: Interest, deadline: Option<typelisp_mem::host::Instant>, poller: PollerRef },
}

/// What an `Io` wait answers with once the socket is ready: unit for a
/// plain `net-wait`, `true` for a `net-wait-for` (whose other answer,
/// `false`, is the clock's).
fn io_ready_answer(deadline: Option<typelisp_mem::host::Instant>) -> Value {
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
    Recv(ChanId, TypeId),
    Send(ChanId, Value),
}

/// One channel operation, as the task asked for it.
///
/// Four of the six never park — they only need the scheduler's table, which
/// only the driver can reach (`typelisp_abi::call_state::SUSPEND_CHAN_NEW`
/// records why). `Send` and `Recv` are the two that can.
#[derive(Clone, PartialEq, Debug)]
pub enum ChanOp {
    /// `(Chan::new cap)` — answers with the handle, built under the
    /// channel's type (`Chan<int>`), which the site wrote.
    New(i64, TypeId),
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
    /// The type is the `Option<T>` the answer is built with. It
    /// has to travel with the operation because a channel's buffer holds
    /// tagged words and a word does not name its type — the same reason
    /// `vector-op`'s `pop` carries one.
    Recv(ChanId, TypeId),
}

/// Names a channel the scheduler is holding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ChanId(usize);

/// Names a task the scheduler is holding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TaskId(usize);

/// The task's number, as a thread started for it is named.
impl core::fmt::Display for TaskId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

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

/// The runtime value of a `Task<T>`: a boxed struct holding the scheduler's id,
/// under the handle's own type (`Task<int>`).
///
/// The type is the one the checker wrote for this `task` node and every
/// layer below passed along: nothing here has a type to read the
/// instantiation off, and a handle prints as its type (`#<Task<int> 1>`),
/// the way a `Vector<T>` does. The id is not a number a program may read.
pub fn task_handle(heap: &mut Heap, id: TaskId, ty: TypeId) -> Value {
    heap.alloc_struct(ty, vec![Value::Int(id.0 as i64)])
}

/// The runtime value of a `Chan<T>`: a boxed struct holding the scheduler's
/// id, under the channel's type (`Chan<int>`) — [`task_handle`]'s
/// arrangement, the type coming from the `Chan::new` site.
pub fn chan_handle(heap: &mut Heap, id: ChanId, ty: TypeId) -> Value {
    heap.alloc_struct(ty, vec![Value::Int(id.0 as i64)])
}

/// The runtime value of a `Thread<T>`: a [`task_handle`] under a `Thread<T>`
/// type. `join` reads it with [`task_id_of`], because a thread *is* a task to
/// the scheduler — one it runs on an OS thread of its own.
pub fn thread_handle(heap: &mut Heap, id: TaskId, ty: TypeId) -> Value {
    heap.alloc_struct(ty, vec![Value::Int(id.0 as i64)])
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
        Waiting::Spawn(..) => "`task`",
        Waiting::SpawnThread(..) => "`thread`",
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

typelisp_mem::thread_local! {
    /// The scheduler whose [`drive_main`] (or [`drive_worker`]/[`drive_pinned`])
    /// is on this thread's stack, if any — a borrow published for that drive's
    /// extent and cleared by its guard. See the module comment's "Owned, not
    /// global". Per thread: each worker publishes the shared scheduler for
    /// its own loop.
    static DRIVING: core::cell::Cell<Option<core::ptr::NonNull<dyn AnswerNow>>> = const { core::cell::Cell::new(None) };
}

/// Publishes `sched` as the driving scheduler for as long as the guard lives,
/// and restores whatever was published before it — a nested drive (the REPL
/// evaluating inside a `print-object`, an `eval`-carrying executable before
/// its interpreter joined the one scheduler) must not leave the outer one
/// unreachable when it returns.
struct DrivingGuard {
    previous: Option<core::ptr::NonNull<dyn AnswerNow>>,
}

impl DrivingGuard {
    fn publish(sched: &dyn AnswerNow) -> DrivingGuard {
        // The lifetime is erased here and re-established by the guard: the
        // pointer is cleared on drop, and the drive holds its `&SchedShared`
        // for the whole of the guard's life. Nothing reads it after that.
        let ptr = core::ptr::NonNull::from(sched);
        let ptr: core::ptr::NonNull<dyn AnswerNow + 'static> = unsafe { core::mem::transmute(ptr) };
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
pub type ThreadStarter = alloc::sync::Arc<dyn Fn(TaskId) -> Result<(), String> + Send + Sync>;

/// One position in the scheduler's table.
///
/// `Running` exists so a position stays **reserved** while its task is out
/// being stepped. Without it, `admit` reuses the position of whichever task is
/// currently running — which is exactly what a `task` inside the main task does,
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
    ready: alloc::collections::VecDeque<TaskId>,
    /// The pinned tasks that are ready. Each is taken only by its own OS
    /// thread, so the order among them means nothing.
    pinned_ready: Vec<TaskId>,
    /// How a `thread` gets its OS thread, or `None` for a scheduler nobody
    /// gave OS threads ([`SchedShared::install_threads`]), which therefore
    /// refuses `thread`.
    thread_starter: Option<ThreadStarter>,
    /// The pinned tasks whose OS thread is running. A crew that lives for
    /// one drive stops them with the rest when the drive
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
    /// What watches the socket waits: the poller of the first one, which is
    /// every one's (a program has one crate with sockets).
    poller: Option<PollerRef>,
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
            ready: alloc::collections::VecDeque::new(),
            pinned_ready: Vec::new(),
            thread_starter: None,
            pinned_threads: Vec::new(),
            roots: None,
            sleeping: 0,
            io_waiting: 0,
            poller: None,
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
    /// [`SchedShared::install_threads`], the one caller.
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
        // Where there are OS threads, every scheduler that runs a program is
        // given a way to start one before its first task
        // ([`SchedShared::install_threads`]).
        let Some(start) = self.thread_starter.clone() else {
            return Err(SchedError::Panic("thread: there are no OS threads here to start one on".to_string()));
        };
        let id = self.admit_as(heap, true, make);
        if let Err(e) = self.start_pinned(&start, id) {
            self.pinned_ready.retain(|t| *t != id);
            let Slot::Parked(slot) = core::mem::replace(&mut self.slots[id.0], Slot::Empty) else {
                unreachable!("a task just admitted is parked");
            };
            heap.drop_root_stack(slot.roots);
            return Err(SchedError::Panic(format!("thread: cannot start an OS thread ({})", e)));
        }
        Ok(id)
    }

    /// Starts the OS thread of pinned task `id`, and remembers that it runs.
    fn start_pinned(&mut self, start: &ThreadStarter, id: TaskId) -> Result<(), String> {
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
        match core::mem::replace(&mut self.slots[id.0], Slot::Running) {
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
            Waiting::Until(t) => (*t <= typelisp_mem::host::Instant::now()).then_some(Ok(Value::Empty)),
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
            // finishes — which is what `task` promises.
            Waiting::Spawn(closure, key) => {
                let closure = *closure;
                let id = self.admit(heap, |heap| B::start_closure(heap, closure));
                Some(Ok(task_handle(heap, id, *key)))
            }
            // The same, on a thread of its own — which starts now, and takes
            // the task as soon as it gets the lock this is holding.
            Waiting::SpawnThread(closure, key) => {
                Some(self.admit_thread(heap, *closure).map(|id| thread_handle(heap, id, *key)))
            }
            Waiting::Main => Some(Ok(Value::Empty)),
            // One descriptor, zero timeout: ready now or not. A `poll` that
            // fails (a descriptor closed under the task) is reported as
            // ready, for the reason `poll_ready` gives — the retry will fail
            // with a message where a park would hang.
            Waiting::Io { fd, interest, deadline, poller } => {
                match poller.0.poll_ready(&[(*fd, *interest)], Some(core::time::Duration::ZERO)) {
                    Ok(ready) if ready.iter().any(|(_, r)| *r) => Some(Ok(io_ready_answer(*deadline))),
                    Ok(_) => match deadline {
                        Some(t) if *t <= typelisp_mem::host::Instant::now() => Some(Ok(Value::Bool(false))),
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
            SelectOp::Recv(c, key) => self.chan_recv(heap, *c, *key),
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
            ChanOp::New(cap, key) => {
                if *cap < 0 {
                    return Some(Err(SchedError::Panic(format!("Chan::new: {} is not a capacity a channel can have", cap))));
                }
                let id = self.chan_new(heap, *cap as usize);
                Some(Ok(chan_handle(heap, id, *key)))
            }
            ChanOp::Len(c) => Some(Ok(Value::Int(self.chans[c.0].len as i64))),
            ChanOp::Cap(c) => Some(Ok(Value::Int(self.chans[c.0].cap as i64))),
            ChanOp::Close(c) => Some(self.chan_close(heap, *c).map(|()| Value::Empty)),
            ChanOp::Send(c, v) => self.chan_send(heap, *c, *v),
            ChanOp::Recv(c, key) => self.chan_recv(heap, *c, *key),
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
                    Waiting::Io { poller, .. } => {
                        self.poller = Some(poller);
                        self.io_waiting += 1;
                        self.slots[id.0] = Slot::Blocked(slot, w);
                    }
                    Waiting::Task(_)
                    | Waiting::Chan(_)
                    | Waiting::Select { .. }
                    | Waiting::Spawn(..)
                    | Waiting::SpawnThread(..)
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
    /// the type its answer is built with.
    ///
    /// Slot order, not arrival order. Go's channels hand out in arrival order;
    /// nothing in this scheduler records arrival, and the alternative is a
    /// second queue per channel that has to agree with `slots` about who is
    /// parked — the disagreement that `finish`/`wake_due` avoid by walking.
    fn waiting_receiver(&self, c: ChanId) -> Option<(usize, Option<usize>, TypeId)> {
        self.slots.iter().enumerate().find_map(|(i, s)| match s {
            Slot::Blocked(_, Waiting::Chan(ChanOp::Recv(on, key))) if *on == c => Some((i, None, *key)),
            Slot::Blocked(_, Waiting::Select { ops, .. }) => ops.iter().enumerate().find_map(|(a, op)| match op {
                SelectOp::Recv(on, key) if *on == c => Some((i, Some(a), *key)),
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
        let Slot::Blocked(mut slot, _) = core::mem::replace(&mut self.slots[i], Slot::Running) else {
            unreachable!("only a blocked task is woken");
        };
        deliver_to(heap, &mut slot, answer);
        self.slots[i] = Slot::Parked(slot);
        self.make_ready(TaskId(i));
    }

    /// Wakes a receiver with `v` (or the end of a closed channel), answering
    /// in whichever shape it was waiting in.
    fn wake_receiver(&mut self, heap: &mut Heap, i: usize, arm: Option<usize>, key: TypeId, v: Option<Value>) {
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
            self.wake_receiver(heap, i, arm, key, Some(v));
            return Some(Ok(Value::Empty));
        }
        if self.chans[c.0].len < self.chans[c.0].cap {
            self.chan_push(heap, c, v);
            return Some(Ok(Value::Empty));
        }
        None
    }

    /// `(recv ch)`. `None` means the receiver has to wait for a value.
    fn chan_recv(&mut self, heap: &mut Heap, c: ChanId, key: TypeId) -> Option<Result<Value, SchedError>> {
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
        // Receivers get `None`: a receiver only parks with the buffer empty
        // and no sender waiting, so there is nothing left for them.
        while let Some((i, arm, key)) = self.waiting_receiver(c) {
            self.wake_receiver(heap, i, arm, key, None);
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

    /// Puts every sleeping task whose deadline has passed back on the queue,
    /// the earliest deadline first.
    ///
    /// Called before each task switch rather than only when nothing is ready:
    /// a task that asked for 10ms while another runs for a second should be
    /// runnable again after 10ms, not after the second.
    ///
    /// Several can be due at once — the thread was busy, or woke late — and
    /// then the queue order is the run order. Taken in slot order, a task
    /// that slept 0.2s ran ahead of one spawned after it that slept 0.05s.
    fn wake_due(&mut self, heap: &mut Heap) {
        if !self.on_the_clock() {
            return;
        }
        let now = typelisp_mem::host::Instant::now();
        let mut due = Vec::new();
        for i in 0..self.slots.len() {
            // A socket wait with a deadline is also a sleep: when the clock
            // runs out first, the answer is `false`.
            match &self.slots[i] {
                Slot::Blocked(_, Waiting::Until(t)) if *t <= now => due.push((*t, i, false)),
                Slot::Blocked(_, Waiting::Io { deadline: Some(t), .. }) if *t <= now => due.push((*t, i, true)),
                _ => {}
            }
        }
        due.sort();
        for (_, i, was_io) in due {
            let answer = if was_io { Value::Bool(false) } else { Value::Empty };
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
    /// thread waits on the network**, by way of the waits' [`Poller`].
    ///
    /// Called with a zero timeout before each task switch (`wake_due`'s
    /// reason: a socket that became readable while another task ran for a
    /// second should not wait for that second to end), and with the nearest
    /// sleeper's deadline — or no limit — when nothing at all is ready.
    /// [`drive_main`]'s threads wait on the network the same way, in the
    /// same three steps, except that the lock is let go of around the
    /// middle one ([`Locked::poll`]).
    fn wake_io(&mut self, heap: &mut Heap, timeout: Option<core::time::Duration>) {
        let Some(poller) = self.socket_poller() else { return };
        let waits = self.io_snapshot();
        let fds: Vec<(i32, Interest)> = waits.iter().map(|w| (w.fd, w.interest)).collect();
        // Native unless it cannot block: other threads may collect while
        // this one waits on the network.
        let polled = match timeout {
            Some(t) if t.is_zero() => poller.0.poll_ready(&fds, timeout),
            _ => heap.native(|| poller.0.poll_ready(&fds, timeout)),
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
                Slot::Blocked(_, Waiting::Io { fd, interest, deadline, .. }) => {
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
    fn apply_io(&mut self, heap: &mut Heap, waits: &[IoWait], polled: Result<Vec<(i32, bool)>, String>) {
        let ready: Vec<bool> = match polled {
            Ok(r) => r.iter().take(waits.len()).map(|(_, r)| *r).collect(),
            Err(_) => vec![true; waits.len()],
        };
        for (w, ready) in waits.iter().zip(ready) {
            let still = matches!(
                &self.slots[w.slot],
                Slot::Blocked(_, Waiting::Io { fd, interest, deadline, .. })
                    if *fd == w.fd && *interest == w.interest && *deadline == w.deadline
            );
            if !ready || !still {
                continue;
            }
            self.wake(heap, w.slot, Ok(io_ready_answer(w.deadline)));
            self.io_waiting -= 1;
        }
    }

    /// Whether any task waits on the clock: a sleep, or a socket wait (which
    /// may have a deadline).
    fn on_the_clock(&self) -> bool {
        self.io_waiting > 0 || self.sleeping > 0
    }

    /// What watches the socket waits, when any task waits on a socket.
    pub(super) fn socket_poller(&self) -> Option<PollerRef> {
        if self.io_waiting == 0 {
            return None;
        }
        self.poller
    }

    /// The nearest deadline any task is sleeping until.
    ///
    /// `None` means nobody is: with nothing ready either, every remaining task
    /// is waiting on something that will never happen.
    fn earliest_deadline(&self) -> Option<typelisp_mem::host::Instant> {
        if !self.on_the_clock() {
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
    interest: Interest,
    deadline: Option<typelisp_mem::host::Instant>,
}

/// `#{ARM VALUE}` — what a `select` answers with: field 0 is the arm index,
/// the whole of the dispatch, and field 1 the receive's `Option<T>` (or unit
/// for a send or an `else`). A box rather than a cons, so that `select` takes
/// no cell from the cons arena.
///
/// Until the box holds it, the payload — a `Some` box built moments ago — is
/// reachable from nothing the collector walks, so it is rooted around the
/// allocation.
fn select_answer(heap: &mut Heap, arm: usize, payload: Value) -> Result<Value, SchedError> {
    let mut s = RootScope::new(heap);
    s.push_root(payload);
    Ok(s.alloc_struct(typelisp_mem::sexpr_tuple_type(2), vec![Value::Int(arm as i64), payload]))
}

/// Hands `answer` to a parked task on **its own** root stack — see
/// [`TaskBody::deliver`] for why the switch matters.
fn deliver_to<B: TaskBody>(heap: &mut Heap, slot: &mut TaskSlot<B>, answer: Result<Value, SchedError>) {
    let home = heap.current_root_stack();
    heap.switch_to_root_stack(slot.roots);
    slot.task.deliver(heap, answer);
    heap.switch_to_root_stack(home);
}

