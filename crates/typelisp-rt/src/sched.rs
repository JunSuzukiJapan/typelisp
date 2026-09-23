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
//! `ACTIVE_HEAP` bug with values in it. It also keeps the door open for OS
//! threads later: whether one scheduler is shared under a lock or each thread
//! has its own, the ownership is the same.
//!
//! What *is* published thread-locally is a **borrow**, for the extent of one
//! [`drive`] and not a moment longer ([`answer_now`]): a driver standing on a
//! machine frame — the printer's door into a compiled `print-object`, a
//! `defvar` initialiser, a compiled body the interpreter called from Rust —
//! has no continuation stack to park a task on, but it can still ask the
//! scheduler whether the operation needs to wait at all. Most do not: a
//! `(recv ch)` with something buffered, a `(go ...)`, a `Chan::new`. The
//! borrow is registered and cleared by a guard on `drive`'s frame, exactly as
//! `ACTIVE_HEAP` is registered per scope, so nothing outlives its owner.
//!
//! Cooperative and single-threaded today: nothing preempts a task, and one OS
//! thread runs all of them. `ACTIVE_HEAP` has to be a single thread-local and
//! `Heap` is `!Send`, so tasks cannot be spread across threads without making
//! the heap shareable first — a bigger job than the concurrency itself.

use std::cell::RefCell;

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
    fn step(&mut self, heap: &mut Heap, cx: &Self::Cx) -> Progress<Self::Error>;

    /// Gives a parked task what it was waiting for — a value to go on with,
    /// or an error to unwind with — **rooting it in the task's own stack**.
    ///
    /// Called with the task's root stack current. The root is the point: a
    /// value handed to a woken task (a `some` box built by `recv`, say)
    /// exists only inside a Rust enum until this runs, and the task is not
    /// stepped again until some other task has had its turn and allocated.
    fn deliver(&mut self, heap: &mut Heap, answer: Result<Value, SchedError>);

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
/// Object-safe on purpose — the two task bodies make two `Scheduler` types,
/// and the driver asking does not know which one is driving.
pub trait AnswerNow {
    fn answer_now(&self, heap: &mut Heap, w: &Waiting) -> Option<Result<Value, SchedError>>;
}

impl<B: TaskBody> AnswerNow for RefCell<Scheduler<B>> {
    /// A short borrow, like every borrow of a scheduler: [`drive`] holds none
    /// while a task is stepped, which is what lets a nested driver ask.
    fn answer_now(&self, heap: &mut Heap, w: &Waiting) -> Option<Result<Value, SchedError>> {
        self.borrow_mut().try_now(heap, w)
    }
}

thread_local! {
    /// The scheduler whose [`drive`] is on this thread's stack, if any — a
    /// borrow published for that drive's extent and cleared by its guard. See
    /// the module comment's "Owned, not global".
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
        // pointer is cleared on drop, and `drive` holds its `&RefCell` for
        // the whole of the guard's life. Nothing reads it after that.
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
}

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
/// Lives behind a `RefCell` in its owner, and **every borrow of it is short**:
/// a task is taken *out* to be stepped, so stepping it — which re-enters the
/// evaluator, and can run compiled code that calls back in — never holds the
/// borrow. [`drive`] is written that way, and so must any other driver be.
pub struct Scheduler<B> {
    /// One entry per task position — see [`Slot`].
    slots: Vec<Slot<B>>,
    /// Ids ready to run, oldest first.
    ready: std::collections::VecDeque<TaskId>,
    /// Where finished tasks' results are rooted: position `i` holds task `i`'s
    /// value. Created on the first `finish`, and never truncated — see
    /// [`Slot::Done`] for why a result outlives its task.
    roots: Option<RootStackId>,
    /// Whether a [`drive`] loop is on the Rust stack. A *nested* evaluation —
    /// a compiled callee re-entering the interpreter, or `Interp::apply` —
    /// must not switch tasks: there is a Rust frame waiting on its result, and
    /// no continuation stack underneath it to come back to.
    driving: bool,
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

impl<B> Default for Scheduler<B> {
    fn default() -> Self {
        Scheduler {
            slots: Vec::new(),
            ready: std::collections::VecDeque::new(),
            roots: None,
            driving: false,
            sleeping: 0,
            io_waiting: 0,
            chans: Vec::new(),
            chan_roots: None,
            rng: 0,
            switch_every_step: false,
        }
    }
}

impl<B: TaskBody> Scheduler<B> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a [`drive`] loop is on the Rust stack — see the field.
    pub fn driving(&self) -> bool {
        self.driving
    }

    pub fn set_driving(&mut self, on: bool) {
        self.driving = on;
    }

    /// Switch tasks after every step — the tests' knob, see the field.
    pub fn set_switch_every_step(&mut self, on: bool) {
        self.switch_every_step = on;
    }

    /// Adds a task and makes it ready to run.
    ///
    /// The task is built by `make` **on a root stack of its own**, which is
    /// where everything it roots from then on lives; the caller's stack is
    /// current again by the time this returns. Whatever `make` reads had
    /// better be rooted in the caller's stack — the collector still walks it,
    /// so that is enough.
    pub fn admit(&mut self, heap: &mut Heap, make: impl FnOnce(&mut Heap) -> B) -> TaskId {
        let home = heap.current_root_stack();
        let roots = heap.new_root_stack();
        heap.switch_to_root_stack(roots);
        let task = make(heap);
        heap.switch_to_root_stack(home);
        let id = match self.slots.iter().position(|s| matches!(s, Slot::Empty)) {
            Some(i) => TaskId(i),
            None => {
                self.slots.push(Slot::Empty);
                TaskId(self.slots.len() - 1)
            }
        };
        self.slots[id.0] = Slot::Parked(TaskSlot { task, roots });
        self.ready.push_back(id);
        id
    }

    /// Takes the next ready task *out* to be stepped, reserving its position.
    /// The caller puts it back (`put_back`) or retires it.
    fn next_ready(&mut self) -> Option<(TaskId, TaskSlot<B>)> {
        let id = self.ready.pop_front()?;
        match std::mem::replace(&mut self.slots[id.0], Slot::Running) {
            Slot::Parked(slot) => Some((id, slot)),
            _ => unreachable!("a queued task is parked in its slot"),
        }
    }

    /// Returns a task that is still running to the back of the queue.
    fn put_back(&mut self, id: TaskId, slot: TaskSlot<B>) {
        self.slots[id.0] = Slot::Parked(slot);
        self.ready.push_back(id);
    }

    /// Frees a task's position entirely — for the main task, whose result goes
    /// back to the caller rather than being kept for a `wait`.
    fn retire(&mut self, id: TaskId) {
        self.slots[id.0] = Slot::Empty;
    }

    /// The value a finished task answered with, for a caller that drove the
    /// tasks itself rather than through `wait`.
    pub fn done_value(&self, id: TaskId) -> Option<Value> {
        match self.slots.get(id.0) {
            Some(Slot::Done(v)) => Some(*v),
            _ => None,
        }
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
        match self.try_now(heap, &w) {
            Some(answer) => {
                deliver_to(heap, &mut slot, answer);
                self.slots[id.0] = Slot::Parked(slot);
                self.ready.push_back(id);
            }
            None => {
                match w {
                    // Straight back onto the queue, behind everything already
                    // on it.
                    Waiting::Yield => {
                        deliver_to(heap, &mut slot, Ok(Value::Empty));
                        self.slots[id.0] = Slot::Parked(slot);
                        self.ready.push_back(id);
                    }
                    Waiting::Until(_) => {
                        self.sleeping += 1;
                        self.slots[id.0] = Slot::Blocked(slot, w);
                    }
                    Waiting::Io { .. } => {
                        self.io_waiting += 1;
                        self.slots[id.0] = Slot::Blocked(slot, w);
                    }
                    Waiting::Task(_) | Waiting::Chan(_) | Waiting::Select { .. } | Waiting::Spawn(_) => {
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
        self.ready.push_back(TaskId(i));
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
            // can put its value in and go.
            if let Some((i, arm, offered)) = self.waiting_sender(c) {
                self.chan_push(heap, c, offered);
                self.wake_sender(heap, i, arm);
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
    /// most `timeout` for one to become so — **the one place the program
    /// waits on the network**, by way of [`crate::os::poll_ready`].
    ///
    /// Called with a zero timeout before each task switch (`wake_due`'s
    /// reason: a socket that became readable while another task ran for a
    /// second should not wait for that second to end), and with the nearest
    /// sleeper's deadline — or no limit — when nothing at all is ready.
    ///
    /// A `poll` failure wakes everyone waiting: each task's retry then fails
    /// with a message of its own, which beats parking forever on an error
    /// the scheduler cannot attribute.
    ///
    /// This is also the one piece that moves when tasks are spread over OS
    /// threads: the set and the call stay, only the caller changes.
    fn wake_io(&mut self, heap: &mut Heap, timeout: Option<std::time::Duration>) {
        if self.io_waiting == 0 {
            return;
        }
        let fds: Vec<(i32, crate::os::Interest)> = self
            .slots
            .iter()
            .filter_map(|s| match s {
                Slot::Blocked(_, Waiting::Io { fd, interest, .. }) => Some((*fd, *interest)),
                _ => None,
            })
            .collect();
        let ready: Vec<(i32, bool)> = match crate::os::poll_ready(&fds, timeout) {
            Ok(r) => r,
            Err(_) => fds.iter().map(|(fd, _)| (*fd, true)).collect(),
        };
        // Same order as `fds`, which is slot order — so this is one walk.
        let mut answers = ready.iter();
        for i in 0..self.slots.len() {
            let deadline = match &self.slots[i] {
                Slot::Blocked(_, Waiting::Io { deadline, .. }) => *deadline,
                _ => continue,
            };
            let woken = answers.next().map(|(_, r)| *r).unwrap_or(true);
            if !woken {
                continue;
            }
            self.wake(heap, i, Ok(io_ready_answer(deadline)));
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

// ---- the loop --------------------------------------------------------------------

/// Runs ready tasks until `main` finishes, and returns its result.
///
/// The result is *not* rooted here, exactly as an ordinary evaluation's is
/// not: the caller has it in hand and roots it if it keeps it.
///
/// Takes the `RefCell` rather than the scheduler because a task is stepped
/// with **no borrow held**: stepping re-enters whatever the body is — the
/// evaluator, compiled code — and that can ask the scheduler a question of
/// its own (`try_now` from a nested evaluation, `driving` from `eval_cps`).
///
/// The outermost evaluation is the *main task*. Tasks `go` starts outlive it:
/// this returns as soon as main is done, and whatever is left keeps its state
/// for the next time the scheduler runs — which is what makes `(go ...)` at a
/// REPL prompt behave, and what cuts every other task off when a program's
/// `main` returns (Go's rule).
pub fn drive<B: TaskBody>(
    sched: &RefCell<Scheduler<B>>,
    heap: &mut Heap,
    cx: &B::Cx,
    main: TaskId,
) -> Result<Value, B::Error> {
    let _driving = DrivingGuard::publish(sched);
    let home = heap.current_root_stack();
    loop {
        let (id, mut slot) = loop {
            sched.borrow_mut().wake_due(heap);
            sched.borrow_mut().wake_io(heap, Some(std::time::Duration::ZERO));
            let taken = sched.borrow_mut().next_ready();
            if let Some(t) = taken {
                break t;
            }
            // Nothing can run. What the remaining tasks wait for is the
            // clock or the network — so **this** is the one place the
            // program stops the thread: on `poll` if any socket is
            // waited on (for at most the nearest deadline), else on the
            // OS's `sleep` until that deadline. That is the difference
            // between stopping a task and stopping the thread.
            let deadline = sched.borrow().earliest_deadline();
            let io_waiting = sched.borrow().io_waiting > 0;
            if deadline.is_none() && !io_waiting {
                // `main` has not finished and nothing can run: every
                // remaining task is waiting on something that will never
                // happen.
                return Err(SchedError::Internal("scheduler: every task is blocked and none can proceed".to_string()).into());
            }
            let now = std::time::Instant::now();
            let until = deadline.map(|d| d.saturating_duration_since(now));
            if io_waiting {
                sched.borrow_mut().wake_io(heap, until);
            } else if let Some(d) = until {
                std::thread::sleep(d);
            }
        };
        heap.switch_to_root_stack(slot.roots);
        let every_step = sched.borrow().switch_every_step;
        let outcome = loop {
            match slot.task.step(heap, cx) {
                Progress::Running if every_step => break Progress::Running,
                Progress::Running => {}
                done => break done,
            }
        };
        match outcome {
            Progress::Running => {
                heap.switch_to_root_stack(home);
                sched.borrow_mut().put_back(id, slot);
            }
            Progress::Blocked(w) => {
                heap.switch_to_root_stack(home);
                sched.borrow_mut().block(heap, id, slot, w);
            }
            Progress::Done(r) => {
                heap.truncate_roots(slot.task.sbase());
                heap.switch_to_root_stack(home);
                heap.drop_root_stack(slot.roots);
                if id == main {
                    // The caller takes this result, so nothing keeps it —
                    // main's position is freed rather than kept for a
                    // `wait`, and no `Task<T>` names it.
                    sched.borrow_mut().retire(id);
                    return r;
                }
                match r {
                    Ok(v) => sched.borrow_mut().finish(heap, id, v),
                    Err(e) => return Err(B::failure_left_task(e)),
                }
            }
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
pub fn io_wait(argv: &[Value], deadline: Option<std::time::Instant>) -> Result<Waiting, SchedError> {
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
    let fd = crate::stream::with_streams(|t| t.raw_fd(handle)).map_err(SchedError::Panic)?;
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
pub fn pending_wait(heap: &Heap) -> Result<(Waiting, Wake), SchedError> {
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
            let w = io_wait(&[Value::Int(payload), Value::Int(second)], None)?;
            Ok((w, Wake::Unit))
        }
        // The handle and the interest share the first word (`rt_suspend_io_for`
        // packs them: handle above, interest in the low bit) so the seconds'
        // bits can have the second. The answer is a `bool`, tagged like
        // every typed wake value.
        cs::SUSPEND_IO_FOR => {
            let d = io_deadline(f64::from_bits(second as u64))?;
            let w = io_wait(&[Value::Int(payload >> 1), Value::Int(payload & 1)], Some(d))?;
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
}

enum CompiledState {
    /// Enter this body with no arguments — the executable's `main`.
    Entry(crate::coroutine::CoroutineFn),
    /// Apply this closure to no arguments — a task `go` started. Rooted in
    /// the first state slot.
    Start(Value),
    /// Re-enter the standing chain with this word in its value slot.
    Resume(i64),
    /// Hand the standing chain the unwind parked for it (`deliver` of an
    /// error) and keep driving.
    Raise,
    /// Waiting. The scheduler moves it out of this with `deliver`.
    Blocked(Waiting),
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
        }
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
            CompiledState::Blocked(Waiting::Chan(ChanOp::Send(_, v))) | CompiledState::Blocked(Waiting::Spawn(v)) => *v,
            _ => Value::Empty,
        };
        heap.set_root(self.sbase, held);
        heap.set_root(self.sbase + 1, Value::Empty);
    }

    /// What a drive that stopped means for the task.
    fn after_drive(&mut self, heap: &mut Heap, outcome: Result<i64, crate::coroutine::Paused>) -> Progress<TaskFailure> {
        match outcome {
            Ok(_) if self.discard_answer => Progress::Done(Ok(Value::Empty)),
            Ok(word) => Progress::Done(Ok(typelisp_abi::decode(word))),
            // The chain stays exactly as it is, rooted by the prologues that
            // built it; only the answer's shape has to be remembered.
            Err(crate::coroutine::Paused::Suspended) => match pending_wait(heap) {
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
            Err(crate::coroutine::Paused::Applying { closure, args }) => {
                // SAFETY: the closure and its arguments came from a compiled
                // `apply` site in the heap registered on this thread.
                let v = unsafe { crate::apply_on_this_frame(closure, &args, "a task") };
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
    fn failure_left_task(e: TaskFailure) -> TaskFailure {
        match e {
            TaskFailure::Throw(tag) => TaskFailure::Panic(format!("`throw` of `{}` left its task", tag)),
            other => other,
        }
    }
}
