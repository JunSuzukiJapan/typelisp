//! One scheduler, several OS threads stepping its tasks
//! (`docs/dev/os-threads-design.md` §8) — checked below the language, with a
//! task body written in Rust, so that what is observed is the scheduler and
//! nothing else.
//!
//! [`Probe`] is a task that can do the three things the checks need: start
//! other tasks and wait for them (the main task), hold its thread until
//! another task is running at the same time (the children), and fail.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use typelisp_mem::{Heap, Value};
use typelisp_rt::sched::{
    drive_main, start_workers, task_id_of, Progress, SchedError, SchedShared, TaskBody, Waiting,
};

#[derive(Debug, PartialEq)]
struct ProbeError(String);

impl From<SchedError> for ProbeError {
    fn from(e: SchedError) -> ProbeError {
        ProbeError(e.to_string())
    }
}

/// What the children of one run report back to the test.
#[derive(Default)]
struct Observed {
    /// How long a child holds its thread waiting for another to start.
    patience: Duration,
    /// Children that have started stepping.
    started: AtomicUsize,
    /// The threads the children were stepped on.
    threads: Mutex<HashSet<std::thread::ThreadId>>,
    /// Children that found another child running at the same time.
    overlapped: AtomicUsize,
}

enum Probe {
    /// Starts `children` tasks, then waits for each and answers with the sum
    /// of their answers. `handles` are rooted in the task's own stack.
    Parent { children: i64, spawned: i64, handles: Vec<Value>, waited: usize, sum: i64 },
    /// Records its thread, waits (holding the thread) until some other child
    /// has started too or its patience runs out, and answers with `k` — or
    /// fails, for a negative `k`.
    Child { k: i64 },
}

// Which run's `Observed` a child reports to. A child is built by
// `start_closure`, which has only the heap and the closure value to go on.
static OBSERVED: Mutex<Option<Arc<Observed>>> = Mutex::new(None);
// Held for the whole of a run, so that the tests of this file, which libtest
// runs in parallel, do not share `OBSERVED`.
static ONE_RUN_AT_A_TIME: Mutex<()> = Mutex::new(());

impl TaskBody for Probe {
    type Cx = ();
    type Error = ProbeError;

    fn start_closure(heap: &mut Heap, closure: Value) -> Probe {
        heap.push_root(Value::Empty);
        heap.push_root(Value::Empty);
        match closure {
            Value::Int(k) => Probe::Child { k },
            other => panic!("a probe is started with an int, not {:?}", other),
        }
    }

    fn sbase(&self) -> usize {
        0
    }

    fn failure_values(_e: &ProbeError) -> Vec<Value> {
        Vec::new()
    }

    fn step(&mut self, heap: &mut Heap, _cx: &()) -> Progress<ProbeError> {
        match self {
            Probe::Parent { children, spawned, handles, waited, sum } => {
                if *spawned < *children {
                    *spawned += 1;
                    return Progress::Blocked(Waiting::Spawn(Value::Int(*spawned), "task<int>".to_string()));
                }
                if *waited < handles.len() {
                    let id = task_id_of(heap, Some(handles[*waited])).expect("a task handle");
                    *waited += 1;
                    return Progress::Blocked(Waiting::Task(id));
                }
                Progress::Done(Ok(Value::Int(*sum)))
            }
            Probe::Child { k } => {
                // The failing child is about a failure *on a worker*: on the
                // thread driving the main task it gives its turn away until a
                // worker picks it up.
                let on_worker = std::thread::current().name().is_some_and(|n| n.starts_with("typelisp-worker-"));
                if *k < 0 && !on_worker {
                    return Progress::Blocked(Waiting::Yield);
                }
                let observed = OBSERVED.lock().unwrap().clone().expect("a run is being observed");
                observed.threads.lock().unwrap().insert(std::thread::current().id());
                observed.started.fetch_add(1, Ordering::SeqCst);
                let limit = Instant::now() + observed.patience;
                while observed.started.load(Ordering::SeqCst) < 2 && Instant::now() < limit {
                    std::hint::spin_loop();
                }
                if observed.started.load(Ordering::SeqCst) >= 2 {
                    observed.overlapped.fetch_add(1, Ordering::SeqCst);
                }
                if *k < 0 {
                    return Progress::Done(Err(ProbeError(format!("child {} failed", k))));
                }
                Progress::Done(Ok(Value::Int(*k)))
            }
        }
    }

    fn deliver(&mut self, heap: &mut Heap, answer: Result<Value, SchedError>) {
        let v = answer.expect("a probe is never refused");
        if let Probe::Parent { spawned, handles, sum, .. } = self {
            // A spawn answers with the new task's handle, a wait with its
            // value: a spawn is outstanding exactly when there are fewer
            // handles than spawns.
            if handles.len() < *spawned as usize {
                heap.push_root(v);
                handles.push(v);
            } else if let Value::Int(n) = v {
                *sum += n;
            }
        }
    }

    fn failure_left_task(e: ProbeError) -> ProbeError {
        ProbeError(format!("left its task: {}", e.0))
    }
}

/// Runs a parent over `children` children on `threads` threads (the one
/// calling `drive_main` included) — plus, if `failing`, one more task that
/// fails — and answers with what the drive returned and what the children
/// saw.
fn run(threads: usize, children: i64, failing: bool, patience: Duration) -> (Result<Value, ProbeError>, Arc<Observed>) {
    let _one = ONE_RUN_AT_A_TIME.lock().unwrap();
    let observed = Arc::new(Observed { patience, ..Observed::default() });
    *OBSERVED.lock().unwrap() = Some(Arc::clone(&observed));
    let mut heap = Heap::with_capacity(1024);
    let shared = Arc::new(SchedShared::<Probe>::new().unwrap());
    let workers = start_workers(&shared, &mut heap, threads - 1, &(), || ()).unwrap();
    let main = shared.lock(&mut heap).admit(&mut heap, |heap| {
        heap.push_root(Value::Empty);
        heap.push_root(Value::Empty);
        Probe::Parent { children, spawned: 0, handles: Vec::new(), waited: 0, sum: 0 }
    });
    if failing {
        shared.lock(&mut heap).admit(&mut heap, |heap| Probe::start_closure(heap, Value::Int(-1)));
    }
    let r = drive_main(&shared, &mut heap, &(), main);
    shared.shutdown(&mut heap);
    for w in workers {
        heap.native(|| w.join().unwrap());
    }
    (r, observed)
}

/// Tasks started from one task are stepped on more than one thread at the
/// same time: every child holds its thread until another child has started,
/// which it can only do on another thread.
#[test]
fn tasks_run_on_several_threads_at_once() {
    let (r, observed) = run(4, 8, false, Duration::from_secs(10));
    assert_eq!(r, Ok(Value::Int((1..=8).sum())));
    assert_eq!(observed.overlapped.load(Ordering::SeqCst), 8, "every child should have seen another one running");
    assert!(observed.threads.lock().unwrap().len() >= 2, "children ran on {:?}", observed.threads.lock().unwrap());
}

/// With one thread the same program gives the same answer — the children
/// just run one after another (none waits for a partner that cannot come).
#[test]
fn one_thread_gives_the_same_answer() {
    let (r, observed) = run(1, 3, false, Duration::ZERO);
    assert_eq!(r, Ok(Value::Int(6)));
    assert_eq!(observed.threads.lock().unwrap().len(), 1);
}

/// A task that fails on a worker ends the drive on the main thread with the
/// failure, turned into what a failure leaving its task is.
#[test]
fn a_failure_on_a_worker_reaches_the_main_thread() {
    let (r, _) = run(4, 4, true, Duration::from_secs(10));
    assert_eq!(r, Err(ProbeError("left its task: child -1 failed".to_string())));
}
