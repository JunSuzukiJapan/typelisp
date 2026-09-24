//! The interpreter's `sched::TaskBody`: a task that is compiled code and
//! nothing else, which any thread may step, or one the interpreter runs,
//! which only the thread that owns the `Interp` may — and the crew of worker
//! threads that steps the first kind. `docs/dev/os-threads-design.md` §8
//! ("typl").
//!
//! The line between the two is *what the task needs*, not where it started:
//! a compiled `go`'s task is compiled, and stays so until its chain applies
//! an interpreted closure (or a `:dyn` method nobody compiled). From there
//! only the interpreter can go on, so the task stops, is turned into the
//! interpreter's kind with its chain standing ([`Task::adopt`]), and belongs
//! to the interpreter's thread **for good** — it is never handed back out.

use typelisp_mem::{Heap, Value};
use typelisp_rt::sched::{self, CompiledTask, Progress, SchedError, TaskBody, TaskFailure};

use super::core_cps::Task;
use super::Interp;
use crate::eval::value::EvalError;

/// One task of the interpreter's scheduler.
pub(crate) struct TyplBody(Kind);

enum Kind {
    /// A compiled chain and nothing else — see [`CompiledTask`]. Stops with
    /// `Progress::NeedsMain` where it needs the interpreter.
    Compiled(CompiledTask),
    /// A task only the interpreter can run.
    Interpreted(MainOnly),
    /// Only while [`TyplBody::step`] turns the first kind into the second.
    Moving,
}

/// A [`Task`] — which holds the interpreter's `Rc`s (a named body's
/// `Rc<dyn CompiledBody>`, frames that point into its tables) and so is not
/// `Send` — in a scheduler that is shared between threads.
///
/// **The one reason this is sound** is that no thread but the interpreter's
/// ever takes such a task out of the scheduler: its slot is main-only
/// (`TyplBody::runs_anywhere` is `false`), and the scheduler's only hand-out,
/// `Scheduler::next_ready`, gives a worker nothing main-only. A worker moves
/// the scheduler — this value with it — only while holding the scheduler's
/// lock, and never touches what is inside.
struct MainOnly(Task);

// SAFETY: see the type's doc comment.
unsafe impl Send for MainOnly {}

/// What a step is given: the interpreter, on the thread that owns it, and
/// nothing on any other.
pub(crate) struct TyplCx(*const Interp);

// SAFETY: a worker only ever holds [`TyplCx::NONE`], whose pointer is null;
// the one that points at an `Interp` is a local of the drive on the
// interpreter's own thread (`Interp::drive_one`) and never leaves it.
unsafe impl Sync for TyplCx {}

impl TyplCx {
    /// What a worker steps tasks with.
    const NONE: TyplCx = TyplCx(std::ptr::null());

    /// What the interpreter's own thread steps tasks with, for as long as
    /// `interp` is borrowed.
    pub(crate) fn of(interp: &Interp) -> TyplCx {
        TyplCx(interp as *const Interp)
    }
}

/// The worker's context, `'static` because a worker thread outlives any
/// borrow of the drive that started it.
static WORKER_CX: TyplCx = TyplCx::NONE;

impl TyplBody {
    /// A task the interpreter runs.
    pub(crate) fn interpreted(task: Task) -> TyplBody {
        TyplBody(Kind::Interpreted(MainOnly(task)))
    }

    /// A task that is compiled code — any thread may step it.
    pub(crate) fn compiled(task: CompiledTask) -> TyplBody {
        TyplBody(Kind::Compiled(task))
    }
}

impl TaskBody for TyplBody {
    type Cx = TyplCx;
    type Error = EvalError;

    /// A compiled `go`'s task — compiled, so a worker may take it.
    fn start_closure(heap: &mut Heap, closure: Value) -> TyplBody {
        TyplBody(Kind::Compiled(CompiledTask::start_closure_handing_off(heap, closure)))
    }

    fn sbase(&self) -> usize {
        match &self.0 {
            Kind::Compiled(t) => t.sbase(),
            Kind::Interpreted(t) => t.0.sbase(),
            Kind::Moving => unreachable!("a task is not asked anything while it is being moved"),
        }
    }

    fn step(&mut self, heap: &mut Heap, cx: &TyplCx) -> Progress<EvalError> {
        match &mut self.0 {
            Kind::Compiled(t) => {
                // A foreign unwind out of the chain — an interpreted failure
                // reached through a machine frame on the interpreter's thread
                // (the printer's door, say) — is the task's failure like any
                // other, not a crash of the thread stepping it.
                let progress = match crate::eval::crossing::catch_compiled_panic(|| t.step(heap, &())) {
                    Ok(p) => p,
                    Err(e) => return Progress::Done(Err(e)),
                };
                match progress {
                    Progress::Running => Progress::Running,
                    Progress::Blocked(w) => Progress::Blocked(w),
                    Progress::Done(r) => Progress::Done(r.map_err(failure_error)),
                    Progress::NeedsMain => {
                        let Kind::Compiled(t) = std::mem::replace(&mut self.0, Kind::Moving) else {
                            unreachable!("matched as compiled above");
                        };
                        self.0 = Kind::Interpreted(MainOnly(Task::adopt(t.hand_off())));
                        Progress::NeedsMain
                    }
                }
            }
            Kind::Interpreted(t) => {
                assert!(!cx.0.is_null(), "an interpreted task was handed to a thread with no interpreter");
                // SAFETY: non-null only on the interpreter's own thread, for
                // the extent of the drive that borrowed it (`TyplCx::of`).
                let interp = unsafe { &*cx.0 };
                t.0.step(heap, interp)
            }
            Kind::Moving => unreachable!("a task is not stepped while it is being moved"),
        }
    }

    fn runs_anywhere(&self) -> bool {
        matches!(self.0, Kind::Compiled(_))
    }

    fn deliver(&mut self, heap: &mut Heap, answer: Result<Value, SchedError>) {
        match &mut self.0 {
            Kind::Compiled(t) => t.deliver(heap, answer),
            Kind::Interpreted(t) => t.0.deliver(heap, answer),
            Kind::Moving => unreachable!("a task is not woken while it is being moved"),
        }
    }

    fn failure_values(e: &EvalError) -> Vec<Value> {
        match e {
            EvalError::Return(v) | EvalError::ReturnFrom(_, v) | EvalError::Throw(_, v) => vec![**v],
            _ => Vec::new(),
        }
    }

    fn failure_left_task(e: EvalError) -> EvalError {
        Task::failure_left_task(e)
    }
}

/// A compiled task's failure in the interpreter's words — the same ones
/// `EvalError` uses for the same failures on the interpreter's own path.
fn failure_error(f: TaskFailure) -> EvalError {
    match f {
        TaskFailure::Panic(m) => EvalError::Panic(m),
        // A tag does not cross a task boundary, and a compiled task is never
        // the main one, so the value is nobody's: `failure_left_task` turns
        // this into the panic it is.
        TaskFailure::Throw(tag) => EvalError::Throw(tag, Box::new(Value::Empty)),
        TaskFailure::Internal(m) => EvalError::Internal(m),
    }
}

impl Interp {
    /// Gives this interpreter's scheduler its crew of worker threads
    /// (`sched::install_crew`), the first time it is driven — there is no
    /// heap to attach them to before. `TYPELISP_THREADS` threads step tasks,
    /// this one included, as in an executable.
    ///
    /// A worker takes on this thread's runtime tables (globals, vtables,
    /// streams), prints from this interpreter's snapshot (`worker_print`) —
    /// or an `eval`-carrying executable's own tables — and has none of the
    /// interpreter's hooks: what needs the interpreter from a worker's
    /// machine frame is refused as a panic in the task
    /// ([`refuse_apply_off_main`]).
    pub(super) fn install_crew(&self, heap: &mut Heap) -> Result<(), EvalError> {
        if self.crew_installed.get() {
            return Ok(());
        }
        let threads = sched::thread_count().map_err(EvalError::Internal)?;
        let rt = typelisp_rt::shared::rt_shared();
        let print = typelisp_print::shared::print_shared();
        let snapshot = std::sync::Arc::clone(&self.worker_print);
        let executable_printer = self.executable_printer.get();
        sched::install_crew(&self.scheduler, heap, threads - 1, &WORKER_CX, move || {
            typelisp_rt::shared::set_rt_shared(std::sync::Arc::clone(&rt));
            if executable_printer {
                typelisp_print::shared::set_print_shared(std::sync::Arc::clone(&print));
                typelisp_print::aot::install();
            } else {
                super::worker_print::install(std::sync::Arc::clone(&snapshot));
            }
            typelisp_rt::set_apply_interpreted(Some(refuse_apply_off_main));
            typelisp_rt::set_dyn_slot_closure(Some(refuse_dyn_off_main));
        });
        self.crew_installed.set(true);
        Ok(())
    }
}

/// What a worker's machine frame does with an interpreted callee: a compiled
/// body driven to completion on a Rust frame (a `print-object` reached
/// through the printer's door) cannot stop and move to the interpreter's
/// thread, so the call is refused as a panic the program can catch. A task's
/// own chain never gets here — it stops and moves (`Progress::NeedsMain`).
///
/// # Safety
///
/// Only ever called as `typelisp_rt`'s apply hook.
unsafe extern "C-unwind" fn refuse_apply_off_main(_closure: i64, _args: *const i64, _argc: u32) -> i64 {
    typelisp_abi::raise(OFF_MAIN.to_string())
}

/// [`refuse_apply_off_main`] for a `:dyn` call whose method nobody compiled.
///
/// # Safety
///
/// Only ever called as `typelisp_rt`'s `:dyn` hook.
unsafe extern "C-unwind" fn refuse_dyn_off_main(_vtable: u32, _slot: u32) -> i64 {
    typelisp_abi::raise(OFF_MAIN.to_string())
}

const OFF_MAIN: &str = "an interpreted function was called from compiled code running on a worker thread, \
     inside a call that cannot move to the interpreter's thread (a print method, say); \
     `compile` the function, or make the call from the main task";
