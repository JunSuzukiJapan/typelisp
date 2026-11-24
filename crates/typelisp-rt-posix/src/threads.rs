//! Starting the OS threads a scheduler steps tasks from: the operating
//! system's half of [`SchedShared::install_threads`], and the program entries
//! of an executable whose tasks run on them.

use typelisp_mem::Word;
use typelisp_mem::prelude::*;
use typelisp_mem::Heap;

use typelisp_rt::sched::{drive_pinned, drive_worker, CompiledTask, Joiner, SchedShared, Spawner, TaskBody, TaskId, ThreadStarter, Threads, Waker};

/// A `std` thread, waited for by [`Joiner::join`].
struct StdJoiner(std::thread::JoinHandle<()>);

impl Joiner for StdJoiner {
    fn join(self: Box<Self>) {
        if let Err(payload) = self.0.join() {
            std::panic::resume_unwind(payload);
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
    shared: alloc::sync::Weak<SchedShared<B>>,
    heap: alloc::sync::Arc<typelisp_mem::heap::HeapShared>,
    setup: alloc::sync::Arc<dyn Fn() + Send + Sync>,
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
    ) -> Result<StdJoiner, String> {
        let shared = self.shared.upgrade().expect("a scheduler starting a thread is alive");
        let heap_shared = alloc::sync::Arc::clone(&self.heap);
        let setup = alloc::sync::Arc::clone(&self.setup);
        let cx = self.cx;
        std::thread::Builder::new()
            .name(name)
            .spawn(move || {
                let mut view = Heap::attach(&heap_shared);
                typelisp_abi::set_active_heap(&mut view);
                setup();
                run(&shared, &mut view, cx);
                typelisp_abi::set_active_heap(core::ptr::null_mut());
            })
            .map(StdJoiner)
            .map_err(|e| e.to_string())
    }

    /// What lets `thread` start OS threads on this scheduler: each runs
    /// [`drive_pinned`]. With `keep`, the thread is recorded for a crew's
    /// end to join; without, it is detached, and its resources go when it
    /// ends.
    fn thread_starter(self: &alloc::sync::Arc<Self>, keep: bool) -> ThreadStarter {
        let kit = alloc::sync::Arc::clone(self);
        alloc::sync::Arc::new(move |own: TaskId| {
            let t = kit.spawn(format!("typelisp-thread-{}", own), move |shared, heap, cx| {
                drive_pinned(shared, heap, cx, own)
            })?;
            if keep {
                let shared = kit.shared.upgrade().expect("a scheduler starting a thread is alive");
                shared.keep_joinable(Box::new(t));
            }
            Ok(())
        })
    }
}

/// How a thread waiting on the scheduler's sockets is woken: a pipe of its
/// own.
fn waker() -> Result<alloc::sync::Arc<dyn Waker>, String> {
    let pipe = crate::os::SelfPipe::new().map_err(|e| format!("the scheduler's wake-up pipe could not be made: {}", e))?;
    Ok(alloc::sync::Arc::new(pipe))
}

/// Starts `n` worker threads on `shared`, each running [`drive_worker`]
/// until the workers are stopped (see [`ThreadKit`] for how each is set up),
/// and lets `thread` start OS threads from then on, set up the same way and
/// running [`drive_pinned`] — with no workers (`n == 0`) too, since a
/// `thread` is not one of them.
///
/// Workers that live as long as the program: an executable's, which drops
/// what this answers and so never joins them. The interpreter's live for
/// one drive at a time ([`install_crew`]).
pub fn start_workers<B>(
    shared: &alloc::sync::Arc<SchedShared<B>>,
    heap: &mut Heap,
    n: usize,
    cx: &'static B::Cx,
    setup: impl Fn() + Send + Sync + 'static,
) -> Result<Vec<Box<dyn Joiner>>, String>
where
    B: TaskBody + Send + 'static,
    B::Error: Send,
    B::Cx: Sync,
{
    let kit = alloc::sync::Arc::new(ThreadKit {
        shared: alloc::sync::Arc::downgrade(shared),
        heap: heap.shared_handle(),
        setup: alloc::sync::Arc::new(setup),
        cx,
    });
    shared.install_threads(heap, Threads { starter: kit.thread_starter(false), waker: waker()?, crew: None });
    (0..n)
        .map(|i| {
            kit.spawn(format!("typelisp-worker-{}", i), drive_worker)
                .map(|t| Box::new(t) as Box<dyn Joiner>)
                .map_err(|e| format!("a scheduler worker thread could not be started: {}", e))
        })
        .collect()
}

/// Gives `shared` a crew of `n` workers: threads that exist only while a
/// [`typelisp_rt::sched::drive_main`] runs, started once a ready task is one they may
/// take, and stopped and joined when the drive ends. `thread` starts OS
/// threads the same way, stopped with the rest and started again by the next
/// drive. See [`ThreadKit`] for how each is set up.
///
/// What an owner that is not always driving has — the interpreter, which
/// reads the next line and checks the next form between two drives, and
/// whose JIT holds the code the threads run. No thread steps a task while
/// the owner is away from the heap, so nothing waits on a thread that is
/// blocked reading a terminal, and nothing outlives the owner.
pub fn install_crew<B>(
    shared: &alloc::sync::Arc<SchedShared<B>>,
    heap: &mut Heap,
    n: usize,
    cx: &'static B::Cx,
    setup: impl Fn() + Send + Sync + 'static,
) -> Result<(), String>
where
    B: TaskBody + Send + 'static,
    B::Error: Send,
    B::Cx: Sync,
{
    let kit = alloc::sync::Arc::new(ThreadKit {
        shared: alloc::sync::Arc::downgrade(shared),
        heap: heap.shared_handle(),
        setup: alloc::sync::Arc::new(setup),
        cx,
    });
    let starter = kit.thread_starter(true);
    let spawn: Spawner = alloc::sync::Arc::new(move |i: usize| {
        kit.spawn(format!("typelisp-worker-{}", i), drive_worker).map(|t| Box::new(t) as Box<dyn Joiner>)
    });
    shared.install_threads(heap, Threads { starter, waker: waker()?, crew: Some((n, spawn)) });
    Ok(())
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

/// What an executable's program entry starts its threads with
/// ([`typelisp_rt::StartThreads`]): `TYPELISP_THREADS` threads step tasks,
/// this one included ([`thread_count`]). The workers share this thread's heap
/// (a view each), its global/vtable/stream tables and its printer's
/// registrations.
fn start_program_threads(
    sched: &alloc::sync::Arc<SchedShared<CompiledTask>>,
    heap: &mut Heap,
) -> Result<(), String> {
    let threads = thread_count()?;
    let rt = typelisp_rt::shared::rt_shared();
    let print = typelisp_print::shared::print_shared();
    let hooks = typelisp_print::runtime::print_hooks();
    let started = start_workers(sched, heap, threads - 1, &(), move || {
        typelisp_rt::shared::set_rt_shared(alloc::sync::Arc::clone(&rt));
        typelisp_print::shared::set_print_shared(alloc::sync::Arc::clone(&print));
        typelisp_print::runtime::set_print_hooks(Some(hooks));
    });
    match started {
        // Never joined: when `main` returns the program ends, and every task
        // still running with it.
        Ok(workers) => drop(workers),
        Err(e) => typelisp_rt::fatal(&e),
    }
    Ok(())
}

/// `rt_run_program` on a target with OS threads: what a native executable's
/// `main` calls.
///
/// # Safety
///
/// `rt_run_program`'s.
#[no_mangle]
pub unsafe extern "C" fn rt_run_program_threads(inits: Word, count: Word, entry: Word) -> Word {
    typelisp_rt::run_entry(inits, count, entry, false, |program| typelisp_rt::run_program(program, Some(start_program_threads)))
}

/// `rt_run_program_int` on a target with OS threads.
///
/// # Safety
///
/// `rt_run_program`'s.
#[no_mangle]
pub unsafe extern "C" fn rt_run_program_threads_int(inits: Word, count: Word, entry: Word) -> Word {
    typelisp_rt::run_entry(inits, count, entry, true, |program| typelisp_rt::run_program(program, Some(start_program_threads)))
}
