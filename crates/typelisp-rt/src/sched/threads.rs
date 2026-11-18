//! One scheduler stepped from several OS threads: the shared, locked
//! scheduler, the drive loops, and starting the worker threads.

use typelisp_mem::{Heap, RootStackId, Value};
use super::{deliver_to, AnswerNow, ChanOp, DrivingGuard, Progress, SchedError, Scheduler, SelectOp, Slot, TaskBody, TaskId, TaskSlot, ThreadStarter, Waiting};

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
