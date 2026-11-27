//! One scheduler stepped from several OS threads: the shared, locked
//! scheduler, the drive loops, and starting the worker threads.

use typelisp_mem::prelude::*;
use typelisp_mem::{Heap, RootStackId, Value};
use super::{deliver_to, AnswerNow, ChanOp, DrivingGuard, Progress, SchedError, Scheduler, SelectOp, Slot, TaskBody, TaskId, TaskSlot, ThreadStarter, Waiting, Waker};

// ---- one scheduler, several threads -------------------------------------------

/// A scheduler several OS threads step tasks from — `docs/dev/os-threads-design.md`
/// §8. One thread calls [`drive_main`] and waits for the main task; the
/// others run [`drive_worker`] until [`Self::shutdown`]. Starting those is
/// the runtime crate's that has OS threads (`typelisp-rt-posix`), through
/// [`Self::install_threads`]; without it the one thread is all there is.
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
    state: typelisp_mem::sync::Mutex<SharedState<B>>,
    /// Where idle threads wait for [`Scheduler::dirty`].
    changed: typelisp_mem::sync::Condvar,
    /// The threads a [`Crew`] started this drive, for its end to join. A lock
    /// of its own, never held together with `state`: a `thread` starts its
    /// OS thread with `state` held and only then records it here.
    joinable: typelisp_mem::sync::Mutex<Vec<alloc::boxed::Box<dyn Joiner>>>,
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
    every_step: typelisp_mem::sync::atomic::AtomicBool,
}

struct SharedState<B: TaskBody> {
    sched: Scheduler<B>,
    /// Whether some thread is in `poll` over the scheduler's socket waits
    /// with the lock let go of. One at a time: the others wait on the
    /// condition variable, and a change the poller should see kicks it
    /// ([`Waker`]).
    polling: bool,
    /// The first task failure the program has had, and which task it was.
    /// [`drive_main`] reports it; a failure ends the program whichever task
    /// it was in (Go's rule for an unrecovered panic).
    failure: Option<(TaskId, B::Error)>,
    /// Workers take no more tasks: the program failed, or [`SchedShared::shutdown`],
    /// or a [`Crew`]'s drive is over.
    stopped: bool,
    /// The other threads, once [`SchedShared::install_threads`] has given
    /// the scheduler a way to start them; `None` while this thread is the
    /// only one.
    threads: Option<OtherThreads>,
}

/// What a scheduler whose tasks run on several OS threads has.
struct OtherThreads {
    /// How a thread waiting on the scheduler's sockets is woken when
    /// something it should see changes.
    waker: typelisp_mem::sync::Arc<dyn Waker>,
    /// Threads that live for one drive at a time, or `None` for workers that
    /// live as long as the program.
    crew: Option<Crew>,
}

/// An OS thread started for the scheduler, to be waited for.
pub trait Joiner: Send {
    /// Waits for the thread to end. A thread that ended in a Rust panic
    /// carries it on into the thread that waits.
    fn join(self: alloc::boxed::Box<Self>);
}

/// Starts worker `i` of a [`Crew`], running [`drive_worker`].
pub type Spawner = typelisp_mem::sync::Arc<dyn Fn(usize) -> Result<alloc::boxed::Box<dyn Joiner>, String> + Send + Sync>;

/// How the runtime crate that has OS threads lets a scheduler use them
/// ([`SchedShared::install_threads`]).
pub struct Threads {
    /// What `thread` starts an OS thread with, running [`drive_pinned`].
    pub starter: ThreadStarter,
    /// How a thread waiting on the scheduler's sockets is woken.
    pub waker: typelisp_mem::sync::Arc<dyn Waker>,
    /// For a scheduler whose owner is not always driving it: how many
    /// workers a drive may start, and how. `None` when the workers were
    /// started already and live as long as the program.
    pub crew: Option<(usize, Spawner)>,
}

/// The threads of a scheduler whose owner is not always driving it — the
/// interpreter's, which drives it once per top-level evaluation and does
/// other things (reads the next line, checks the next form) in between.
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
    /// Starts one worker thread.
    spawn: Spawner,
    /// Whether this drive's workers have been started.
    up: bool,
    /// Whether the last drive's end stopped the OS thread of a pinned task
    /// that is still alive, so this one has to start it again.
    orphans: bool,
}

impl<B: TaskBody> SchedShared<B> {
    pub fn new() -> SchedShared<B> {
        SchedShared {
            state: typelisp_mem::sync::Mutex::new(SharedState {
                sched: Scheduler::new(),
                polling: false,
                failure: None,
                stopped: false,
                threads: None,
            }),
            changed: typelisp_mem::sync::Condvar::new(),
            joinable: typelisp_mem::sync::Mutex::new(Vec::new()),
            every_step: typelisp_mem::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Lets tasks run on other OS threads: `thread` starts one through
    /// `threads.starter`, and a crew's workers start with each drive.
    pub fn install_threads(&self, heap: &mut Heap, threads: Threads) {
        let mut s = self.lock(heap);
        s.set_thread_starter(threads.starter);
        let crew = threads.crew.map(|(workers, spawn)| Crew { workers, spawn, up: false, orphans: false });
        s.guard.threads = Some(OtherThreads { waker: threads.waker, crew });
    }

    /// Records an OS thread for a crew's end to wait for.
    pub fn keep_joinable(&self, thread: alloc::boxed::Box<dyn Joiner>) {
        self.joinable.lock().push(thread);
    }

    /// The crew, if the scheduler has one.
    fn crew_mut(state: &mut SharedState<B>) -> Option<&mut Crew> {
        state.threads.as_mut().and_then(|t| t.crew.as_mut())
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

    /// Whether no task is ready and waiting to be run, and the program is
    /// going on — so that a running task offering its turn may keep it.
    fn nobody_waits(&self, heap: &mut Heap) -> bool {
        let s = self.lock(heap);
        !s.guard.stopped && s.ready.is_empty() && s.pinned_ready.is_empty()
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
        self.every_step.store(on, typelisp_mem::sync::atomic::Ordering::Relaxed);
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
            if Self::crew_mut(&mut s.guard).is_none() {
                return;
            }
            s.guard.stopped = true;
            s.guard.sched.dirty = true;
        }
        let threads = core::mem::take(&mut *self.joinable.lock());
        heap.native(|| threads.into_iter().for_each(|t| t.join()));
        let mut s = self.lock(heap);
        s.guard.stopped = false;
        let had_pinned = !s.guard.sched.pinned_threads.is_empty();
        s.guard.sched.pinned_threads.clear();
        if let Some(crew) = Self::crew_mut(&mut s.guard) {
            crew.up = false;
            crew.orphans |= had_pinned;
        }
    }
}

impl<B: TaskBody> AnswerNow for SchedShared<B> {
    /// Whatever `w` carries is rooted while the lock is taken — taking it can
    /// let another thread collect, and a `send`'s value or a `task`'s closure
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

/// The heap values a wait carries: a `send`'s value, a `task`'s closure, a
/// `select`'s send arms.
fn waiting_values(w: &Waiting) -> Vec<Value> {
    match w {
        Waiting::Chan(ChanOp::Send(_, v)) | Waiting::Spawn(v, _) | Waiting::SpawnThread(v, _) => vec![*v],
        Waiting::Select { ops, .. } => ops
            .iter()
            .filter_map(|op| match op {
                SelectOp::Send(_, v) => Some(*v),
                SelectOp::Recv(..) => None,
            })
            .collect(),
        Waiting::Task(_) | Waiting::Yield | Waiting::Until(_) | Waiting::Chan(_) | Waiting::Main | Waiting::Io { .. } => Vec::new(),
    }
}

/// The lock on a [`SchedShared`], dereferencing to its [`Scheduler`].
///
/// Letting go of it wakes the idle threads if anything they would want to
/// know about happened meanwhile ([`Scheduler::dirty`]) — the one place that
/// happens, so no operation has to remember to.
pub struct Locked<'a, B: TaskBody> {
    guard: typelisp_mem::sync::MutexGuard<'a, SharedState<B>>,
    shared: &'a SchedShared<B>,
}

impl<B: TaskBody> core::ops::Deref for Locked<'_, B> {
    type Target = Scheduler<B>;
    fn deref(&self) -> &Scheduler<B> {
        &self.guard.sched
    }
}

impl<B: TaskBody> core::ops::DerefMut for Locked<'_, B> {
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
    /// condition variable, and the poller through its [`Waker`].
    fn flush(&mut self) {
        if core::mem::take(&mut self.guard.sched.dirty) {
            self.shared.changed.notify_all();
            if self.guard.polling {
                if let Some(t) = &self.guard.threads {
                    t.waker.kick();
                }
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
        let Some(crew) = SchedShared::crew_mut(&mut self.guard) else { return Ok(()) };
        if core::mem::take(&mut crew.orphans) {
            if let Err((id, e)) = self.guard.sched.restart_pinned() {
                self.record_failure(id, e.into());
                return Ok(());
            }
        }
        let Some(crew) = SchedShared::crew_mut(&mut self.guard) else { return Ok(()) };
        if crew.up || crew.workers == 0 {
            return Ok(());
        }
        let (n, spawn) = (crew.workers, typelisp_mem::sync::Arc::clone(&crew.spawn));
        let slots = &self.guard.sched.slots;
        let for_workers = self.guard.sched.ready.iter().any(|t| !matches!(&slots[t.0], Slot::Parked(s) if s.main_only));
        if !for_workers {
            return Ok(());
        }
        if let Some(crew) = SchedShared::crew_mut(&mut self.guard) {
            crew.up = true;
        }
        for i in 0..n {
            let t = spawn(i).map_err(|e| {
                SchedError::Internal(format!("a scheduler worker thread could not be started: {}", e))
            })?;
            self.shared.keep_joinable(t);
        }
        Ok(())
    }

    /// Waits — native — until something changes or `until` passes.
    fn wait(mut self, heap: &mut Heap, until: Option<core::time::Duration>) -> Locked<'a, B> {
        self.flush();
        if self.guard.threads.is_some() {
            let changed = &self.shared.changed;
            let guard = &mut self.guard;
            heap.native(|| match until {
                Some(d) => {
                    changed.wait_for(guard, d);
                }
                None => changed.wait(guard),
            });
            return self;
        }
        // The one thread there is: nothing else can change anything while it
        // waits, so a wait with a deadline is a sleep, and one without is
        // never woken.
        match until {
            Some(d) => heap.native(|| typelisp_mem::host::sleep(d)),
            None => typelisp_abi::fatal("every task is waiting, and there is no other thread to wake one"),
        }
        self
    }

    /// Waits on the scheduler's sockets — and the [`Waker`], so that a change
    /// wakes it — for at most `until`, with the lock let go of, and wakes the
    /// tasks whose sockets are ready. The same three steps as
    /// [`Scheduler::wake_io`].
    fn poll(mut self, heap: &mut Heap, poller: super::PollerRef, until: Option<core::time::Duration>) -> Locked<'a, B> {
        let shared = self.shared;
        self.guard.polling = true;
        let waits = self.guard.sched.io_snapshot();
        let mut fds: Vec<(i32, super::Interest)> = waits.iter().map(|w| (w.fd, w.interest)).collect();
        let waker = self.guard.threads.as_ref().map(|t| typelisp_mem::sync::Arc::clone(&t.waker));
        if let Some(w) = &waker {
            fds.push((w.read_fd(), super::Interest::Readable));
        }
        drop(self);
        let polled = heap.native(|| poller.0.poll_ready(&fds, until));
        if let Some(w) = &waker {
            w.drain();
        }
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
pub(super) enum Taker {
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
                if typelisp_abi::interrupt::pending() {
                    s.interrupt_waiting(heap, main);
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
        s.wake_io(heap, Some(core::time::Duration::ZERO));
        let taker = match who {
            Who::Main(_) => Taker::Main,
            Who::Worker => Taker::Worker,
            Who::Pinned(own) => Taker::Pinned(own),
        };
        if let Some((id, slot)) = s.next_ready(taker) {
            return Next::Run(id, slot);
        }
        let deadline = s.earliest_deadline();
        let sockets = s.socket_poller();
        if matches!(who, Who::Main(_))
            && s.running == 0
            && s.pinned_ready.is_empty()
            && deadline.is_none()
            && sockets.is_none()
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
        let mut until = deadline.map(|d| d.saturating_duration_since(typelisp_mem::host::Instant::now()));
        // The thread waiting on the main task looks for the REPL's Ctrl-C
        // every so often, which nothing else would wake it for.
        if matches!(who, Who::Main(_)) && typelisp_abi::interrupt::watched() {
            let look = typelisp_abi::interrupt::LOOK_EVERY;
            until = Some(until.map_or(look, |u| u.min(look)));
        }
        if let Some(poller) = sockets {
            if !s.guard.polling {
                s = s.poll(heap, poller, until);
                continue;
            }
        }
        s = s.wait(heap, until);
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
    let every_step = shared.every_step.load(typelisp_mem::sync::atomic::Ordering::Relaxed);
    let outcome = loop {
        match slot.task.step(heap, cx) {
            Progress::Running if every_step => break Progress::Running,
            // Between steps the task's state is all in its frames and root
            // stack, so another thread's collection may run here.
            Progress::Running => heap.safepoint(),
            // A turn offered — a loop's safepoint, a `(yield)` — that nobody
            // is waiting for is declined here, without the task going back
            // on the queue. Going back would mark the scheduler changed and
            // wake every idle thread to find nothing to do, every few hundred
            // iterations of a loop. A task that becomes ready meanwhile is an
            // idle thread's to take; one that is due on the clock wakes the
            // thread waiting for it.
            Progress::Blocked(Waiting::Yield) if !every_step && shared.nobody_waits(heap) => {
                deliver_to(heap, &mut slot, Ok(Value::Empty));
                heap.safepoint();
            }
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
/// next drive — which is what makes `(task ...)` at a REPL prompt behave, and
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
    end_crew_holding(shared, heap, &result);
    result
}

/// [`SchedShared::end_crew`], with what the drive came to rooted meanwhile.
fn end_crew_holding<B: TaskBody>(shared: &SchedShared<B>, heap: &mut Heap, result: &Result<Value, B::Error>) {
    let carried = match result {
        Ok(v) => vec![*v],
        Err(e) => B::failure_values(e),
    };
    let base = heap.root_count();
    for v in &carried {
        heap.push_root(*v);
    }
    shared.end_crew(heap);
    heap.truncate_roots(base);
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

