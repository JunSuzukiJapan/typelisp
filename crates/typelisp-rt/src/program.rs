//! Starting and finishing a compiled program: the AOT heap, the entry points an
//! executable's `main` calls, and driving a compiled body from a machine frame.

use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, fatal, set_active_heap, CompiledPanic, CompiledThrow};
use typelisp_mem::{Heap, Value};
use crate::{global_perm_idx, take_throw};
use crate::closure::APPLY_INTERPRETED;

/// The cons-cell arena size an AOT-compiled executable allocates for itself
/// at startup (see [`rt_heap_init`]) when its `main` doesn't otherwise say
/// — matches the capacity `compile-file` itself already uses for the
/// *compiler's own* (unrelated) `Heap` in `typelisp::compile::aot::compile_file`.
pub const DEFAULT_AOT_HEAP_CAPACITY: usize = 1 << 16;

/// AOT's counterpart to the JIT path's [`set_active_heap`]: an AOT-compiled
/// executable is its own standalone process with no embedding Rust caller
/// to register a `Heap` for it, so it has to create and register one for
/// itself. `typelisp::compile::aot::build_main_wrapper` emits a call to this
/// as the very first thing the generated `main` does, before calling the
/// file's own `tl_main`.
///
/// Reads the desired capacity from `args[0]` if `argc >= 1`, otherwise uses
/// [`DEFAULT_AOT_HEAP_CAPACITY`]. `args[1]`, if `argc >= 2` and not 0, is the
/// bound on the bytes the whole process allocates (`typl -c --max-memory`,
/// [`crate::mem_limit`]); it is set before the arena is allocated, so the
/// arena counts toward it. `args[2]`, if `argc >= 3` and not 0, fixes the
/// arena at its capacity (`typl -c --heap-cells`); otherwise it grows as
/// [`Heap::with_capacity`] lets it. The allocated `Heap` is deliberately
/// never freed — it must outlive every other compiled function in the
/// process, i.e. live until the OS reclaims everything at process exit, the
/// same trade-off [`rt_push_permanent_sexpr_root`] already accepts for values
/// that outlive their last explicit owner.
///
/// # Safety
///
/// Same as [`rt_ping`]: `args` must point to at least `argc` valid,
/// readable `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_heap_init(args: *const i64, argc: u32) -> i64 {
    typelisp_mem::host::install_panic_report();
    let capacity = if argc >= 1 { *args as usize } else { DEFAULT_AOT_HEAP_CAPACITY };
    if argc >= 2 && *args.add(1) != 0 {
        crate::mem_limit::set_memory_limit(*args.add(1) as usize);
    }
    let mut heap = Box::new(Heap::with_capacity(capacity));
    if argc >= 3 && *args.add(2) != 0 {
        heap.set_growth_limit(0);
    }
    set_active_heap(Box::into_raw(heap));
    0
}

/// Diagnostic-only: the active `Heap`'s live cons-cell count
/// (`Heap::live_count`). Stage 1's proof that the registered `Heap` is
/// genuinely reachable and reflects real state, not a placeholder for any
/// actual language feature — Stage 3 replaces the need for this with real
/// `rt-cons`/`rt-car`/... functions.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread (see [`active_heap`]).
#[no_mangle]
pub unsafe extern "C" fn rt_heap_live_count(_args: *const i64, _argc: u32) -> i64 {
    active_heap().live_count() as i64
}

/// The exit code an AOT executable ends with when a `(panic ...)` reaches its
/// entry point uncaught — the same code `typl <file>` exits with when an
/// interpreted run ends in an error, so the two front ends agree.
///
/// `pub` for `typelisp_front::shim::rt_run_program_interp[_int]`, an
/// `eval`-carrying AOT executable's own entry, which answers with the same
/// code for the same reason — see [`exit_code`].
pub const EXIT_CODE_PANIC: i64 = 1;

/// The exit code of a program whose `()`-returning `main` returned normally.
/// Named, and not the value `main` answered, because that answer is the word
/// `()` is encoded as, which differs between the entry shims' decodings.
pub const EXIT_CODE_SUCCESS: i64 = 0;

/// Runs an AOT executable's compiled entry point (`tl_main`), returning its
/// exit code — or, if a `(panic ...)` or an uncaught `throw` was raised out
/// of it, printing the message and returning [`EXIT_CODE_PANIC`].
///
/// This exists because an AOT executable has nowhere else to look at what
/// was raised. In the JIT path the interpreter is a Rust frame around the
/// call (`typelisp::compile::catch_compiled_panic`); AOT's `main` is
/// *generated LLVM code* that calls `rt_heap_init` and then this, with no
/// Rust frame in between.
///
/// # Safety
///
/// `entry` must be the address of a function compiled under the standard
/// compiled-function ABI (`typelisp::compile::CompiledSignature`) that
/// tolerates being called with no arguments — which `tl_main` is, since the
/// generated `main` already called it that way. A `Heap` must already be
/// registered on this thread (`rt_heap_init` runs first).
#[no_mangle]
pub unsafe extern "C" fn rt_run_entry(entry: i64) -> i64 {
    let f: unsafe extern "C-unwind" fn(*const i64, u32) -> i64 = core::mem::transmute(entry as usize);
    // Only a `()`-returning `main` reaches this shim (`compile::aot` refuses
    // an `int` one under this ABI), and a `main` that returns normally exits
    // 0 — whatever word its `()` happens to be.
    f(core::ptr::null(), 0);
    exit_code(EXIT_CODE_SUCCESS)
}

/// [`rt_run_entry`] for a program built under the coroutine ABI: the same
/// handling of what was raised with a **scheduler** in front of it, and the program's
/// global initialisers run under that same scheduler ahead of `main`.
///
/// `args` is `[inits, n, entry]`: the address of an array of `n`
/// `coroutine_fn_type` addresses — one zero-argument initialiser per
/// `defvar`, in declaration order — and the entry point's own address. A
/// generated `main` cannot drive any of them itself: the protocol is a loop
/// over status words, and `main` is a handful of constant stores. Which of
/// the two entry shims this executable's `main` calls is decided when it is
/// generated, by the ABI that build emitted.
///
/// The scheduler lives on this frame for as long as the program does
/// (`sched::Scheduler` over `sched::CompiledTask`). Each initialiser is run
/// as the main task of one drive and `main` as the main task of the last —
/// the shape the REPL gives its top-level forms — so an initialiser that
/// makes a channel, starts a task or waits on one is an ordinary program and
/// not a startup special case, and a task an initialiser started is still
/// there when `main` runs. When `main` returns the program ends and every
/// other task is cut off — Go's rule, and `typl`'s. A task's own uncaught
/// failure stops the program with the same line the interpreter prints for
/// it, an initialiser's included.
///
/// # Safety
///
/// `argc` must be 3; `args[0]` must point to `args[1]` valid
/// `coroutine_fn_type` addresses of zero-argument bodies, and `args[2]` must
/// be one such address that tolerates being entered with no arguments; a
/// `Heap` must already be registered on this thread (`rt_heap_init` runs
/// first).
#[no_mangle]
pub unsafe extern "C" fn rt_run_program(args: *const i64, argc: u32) -> i64 {
    run_entry(args, argc, "rt_run_program", false, |program| run_program(program, None))
}

/// [`rt_run_program`] for a `main` declared to return `int`: the answer is
/// a tagged word, and the exit code is the fixnum's payload.
///
/// Decoded *here* rather than by the generated `main`, because a panic's
/// exit code comes out of the same call raw (`EXIT_CODE_PANIC`), and only
/// this side knows which of the two it is handing back. A bignum has no
/// exit code and is fatal.
///
/// # Safety
///
/// [`rt_run_program`]'s.
#[no_mangle]
pub unsafe extern "C" fn rt_run_program_int(args: *const i64, argc: u32) -> i64 {
    run_entry(args, argc, "rt_run_program_int", true, |program| run_program(program, None))
}

/// [`rt_run_program`] for a program that needs no scheduler: the compiler
/// found that nothing the executable can run starts a task or a thread,
/// uses a channel, sleeps or waits on a socket (`compile::aot`), so the
/// initialisers and `main` run one after another on this frame
/// ([`run_alone`]) and the scheduler is not linked in.
///
/// # Safety
///
/// [`rt_run_program`]'s.
#[no_mangle]
pub unsafe extern "C" fn rt_run_program_alone(args: *const i64, argc: u32) -> i64 {
    run_entry(args, argc, "rt_run_program_alone", false, |program| run_alone(program))
}

/// [`rt_run_program_alone`] for a `main` declared to return `int`, read as
/// [`rt_run_program_int`] reads it.
///
/// # Safety
///
/// [`rt_run_program`]'s.
#[no_mangle]
pub unsafe extern "C" fn rt_run_program_alone_int(args: *const i64, argc: u32) -> i64 {
    run_entry(args, argc, "rt_run_program_alone_int", true, |program| run_alone(program))
}

/// What starts the OS threads that step tasks alongside the one running the
/// program, on a target that has them: given the program's scheduler before
/// its first initialiser. An `Err` is a configuration the program cannot run
/// with, reported as `error: <message>`.
pub type StartThreads = fn(&alloc::sync::Arc<crate::sched::SchedShared<crate::sched::CompiledTask>>, &mut Heap) -> Result<(), String>;

/// The body of every program entry: reads the program, runs it with `run`
/// ([`run_program`] under a scheduler, [`run_alone`] without one), and
/// turns what it answered into the exit code — with `returns_int`, `main`'s
/// `int`.
///
/// # Safety
///
/// [`rt_run_program`]'s.
pub unsafe fn run_entry(
    args: *const i64,
    argc: u32,
    what: &str,
    returns_int: bool,
    run: impl FnOnce(&Program) -> Result<Value, i64>,
) -> i64 {
    let program = read_program(args, argc, what);
    let result = run(&program);
    if !returns_int {
        // A `()`-returning `main` that returns normally exits 0; the word its
        // `()` is encoded as is not an exit code.
        return exit_code(match result {
            Ok(_) => EXIT_CODE_SUCCESS,
            Err(code) => code,
        });
    }
    let code = match result {
        Ok(Value::Int(n)) => n,
        Ok(Value::Boxed(id)) if active_heap().is_bignum(id) => {
            fatal(&format!("main returned {}, which is not a process exit code", active_heap().bignum_text(id)))
        }
        Ok(other) => fatal(&format!("main declared to return int answered {:?}", other)),
        Err(code) => code,
    };
    exit_code(code)
}

/// What a generated `main` hands the program entry: the initialisers and
/// the entry point, as coroutine-ABI addresses.
pub struct Program {
    /// One per global, in id order: its initialiser, or `None` for one the
    /// executable left out ([`crate::global_skip`]).
    pub inits: Vec<Option<crate::coroutine::CoroutineFn>>,
    pub entry: crate::coroutine::CoroutineFn,
}

/// Reads `[inits, n, entry]` off the standard shim argument array.
///
/// # Safety
///
/// [`rt_run_program`]'s.
pub unsafe fn read_program(args: *const i64, argc: u32, what: &str) -> Program {
    if argc < 3 {
        fatal(&format!("{}: expected 3 arguments (the initialiser array, its count and the entry point)", what));
    }
    let words = collect_words(*args as usize as *const i64, *args.add(1));
    // A 0 is a global the executable left out.
    let inits = words
        .into_iter()
        .map(|w| (w != 0).then(|| core::mem::transmute::<usize, crate::coroutine::CoroutineFn>(w as usize)))
        .collect();
    let entry: crate::coroutine::CoroutineFn = core::mem::transmute(*args.add(2) as usize);
    Program { inits, entry }
}

/// Runs the program's initialisers and then its `main`, each as the main
/// task of one drive of a scheduler that lives on this frame, and answers
/// with `main`'s value — or with the exit code of a failure that has
/// already been reported.
///
/// The scheduler is a local of this frame and nothing else: it is owned by
/// the frame that owns the program's run, exactly as the interpreter's is a
/// field of the interpreter — never a global, because its tables hold roots
/// into the heap (`sched`'s module comment). The worker threads share it
/// through an `Arc`, and hold nothing else of this frame's.
///
/// With `start`, other OS threads step tasks too; they are started before
/// the first initialiser, and are never joined: when `main` returns the
/// program ends, and every task still running with it — Go's rule. Without
/// it, this thread is the only one.
///
/// # Safety
///
/// [`rt_run_program`]'s.
pub unsafe fn run_program(program: &Program, start: Option<StartThreads>) -> Result<Value, i64> {
    let heap = active_heap();
    let sched = alloc::sync::Arc::new(crate::sched::SchedShared::<crate::sched::CompiledTask>::new());
    // The printer reads the prelude's control variables from their compiled
    // slots, and the id -> slot table is this crate's.
    typelisp_print::aot::set_global_slots(global_perm_idx);
    if let Some(start) = start {
        if let Err(m) = start(&sched, heap) {
            typelisp_mem::host::eprintln(format_args!("error: {}", m));
            return Err(EXIT_CODE_PANIC);
        }
    }
    // Built inside `admit`, on the task's own root stack — `entry` pushes
    // the task's state slots, and they belong there and not on this frame's.
    let run = |heap: &mut Heap, f: crate::coroutine::CoroutineFn, initialiser: bool| {
        let main = sched.lock(heap).admit(heap, |heap| {
            if initialiser {
                crate::sched::CompiledTask::initialiser(heap, f)
            } else {
                crate::sched::CompiledTask::entry(heap, f)
            }
        });
        match crate::sched::drive_main(&sched, heap, &(), main) {
            Ok(v) => Ok(v),
            // The same line `exit_code` prints for a panic that
            // reached the entry as a Rust unwind — a task's failure is the
            // program's.
            Err(failure) => {
                typelisp_mem::host::eprintln(format_args!("{}", failure));
                Err(EXIT_CODE_PANIC)
            }
        }
    };
    for init in &program.inits {
        // An initialiser's value is the global's storage id, already stored
        // by the body itself (`compile-global-init`); nothing here reads it.
        match init {
            Some(init) => {
                run(heap, *init, true)?;
            }
            None => crate::global_skip(),
        }
    }
    run(heap, program.entry, false)
}

/// [`run_program`] without a scheduler: each initialiser and then `main`
/// run to their end on this frame, on the chain a task would have
/// ([`crate::coroutine::FrameStack::for_task`], which a board's interrupt
/// handler may break into). A failure is left raised for [`exit_code`] to
/// report, in the words a task's failure is reported in.
///
/// # Safety
///
/// [`rt_run_program`]'s.
unsafe fn run_alone(program: &Program) -> Result<Value, i64> {
    typelisp_print::aot::set_global_slots(global_perm_idx);
    let run = |f: crate::coroutine::CoroutineFn| {
        let mut chain = crate::coroutine::FrameStack::for_task();
        chain
            .run_to_end_alone(active_heap(), f, &[], &[])
            .map_err(|paused| {
                // The raise stays on the thread for `exit_code`.
                let _raised = pause_on_a_machine_frame(paused, "the program");
                EXIT_CODE_PANIC
            })
    };
    for init in &program.inits {
        // An initialiser's value is the global's storage id, already stored
        // by the body itself (`compile-global-init`); nothing here reads it.
        match init {
            Some(init) => {
                run(*init)?;
            }
            None => crate::global_skip(),
        }
    }
    run(program.entry).map(typelisp_abi::decode)
}

/// A classic-signature door onto a coroutine body: `args` is
/// `[address, argument array, count]`, and the answer is the body's value.
///
/// For a caller that has to look like `(ptr, i32) -> i64` and cannot drive
/// anything itself — `typelisp-print` holds the addresses `~/name/` and
/// `print-object` dispatch to, and that crate deliberately does not depend on
/// this one (see its `Cargo.toml`), so it has no `FrameStack` to reach for.
/// `compile::aot` wraps each registered method in four instructions that come
/// here instead, which puts the knowledge of the callee's ABI in the one
/// place that has it: the code generator that emitted the callee.
///
/// # Safety
///
/// `argc` must be 3; `args[0]` must be a `coroutine_fn_type` address and
/// `args[1]`/`args[2]` a valid argument array for it; a `Heap` must already
/// be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_drive_body(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_drive_body: expected 3 arguments (the address, the argument array and its count)");
    }
    let f: crate::coroutine::CoroutineFn = core::mem::transmute(*args as usize);
    let callee_args = *args.add(1) as usize as *const i64;
    let callee_argc = *args.add(2);
    let words = collect_words(callee_args, callee_argc);
    match drive_to_completion(f, &words, &[], "rt_drive_body") {
        Ok(v) => v,
        Err(raised) => raised.into_word(),
    }
}

/// Turns whichever entry point ran into an exit code: a typelisp-level
/// `panic` or an uncaught `throw` raised out of it is reported and becomes
/// [`EXIT_CODE_PANIC`]; otherwise the code is `code`.
///
/// `pub` for the interpreter's own program entry (`typelisp_front::shim`),
/// which looks at the same kind of run the same way.
///
/// # Safety
///
/// A `Heap` must be registered on this thread (an uncaught throw's tag is
/// read out of it).
pub unsafe fn exit_code(code: i64) -> i64 {
    let Some(payload) = typelisp_abi::take_raised() else { return code };
    if let Some(p) = payload.downcast_ref::<CompiledPanic>() {
        typelisp_mem::host::eprintln(format_args!("panic: {}", p.message));
        return EXIT_CODE_PANIC;
    }
    // A `throw` that found no enclosing `catch` anywhere. Reported with the
    // same wording `EvalError::Throw`'s `Display` uses, so the two front ends
    // say the same thing about the same program.
    if payload.is::<CompiledThrow>() {
        let (tag, _) = take_throw().unwrap_or_else(|| fatal("a compiled throw was raised with no throw in flight"));
        typelisp_mem::host::eprintln(format_args!("throw: no enclosing (catch '{}) for this throw", tag.spelling()));
        return EXIT_CODE_PANIC;
    }
    fatal("the program entry was left by an unwind that is neither a panic nor a throw")
}

unsafe fn collect_words(p: *const i64, n: i64) -> Vec<i64> {
    if n < 0 {
        fatal(&format!("{} is not a word count", n));
    }
    (0..n as usize).map(|i| *p.add(i)).collect()
}

/// What a driver whose caller is a machine frame does with a pause.
///
/// An unwind is raised again, so it keeps travelling towards a `catch` above
/// this boundary: the caller returns the [`typelisp_abi::Raised`]. Nothing else can come out: every driver standing on a
/// machine frame goes through `FrameStack::run_to_end`, which runs an
/// interpreted `apply` on that frame and resolves a suspension through the
/// driving scheduler (answering it, or refusing it into the chain as a
/// panic). What still stands on a machine frame is `rt_drive_body` (the
/// printer's door); the executable's initialisers and `main` run under a
/// scheduler (`run_program`).
unsafe fn pause_on_a_machine_frame(paused: crate::coroutine::Paused, what: &str) -> typelisp_abi::Raised {
    match paused {
        crate::coroutine::Paused::Unwinding => crate::coroutine::resume_unwinding(),
        // Reaching either means a driver was written to `run`/`resume`
        // directly — a protocol break, not a limitation.
        crate::coroutine::Paused::Applying { .. } | crate::coroutine::Paused::ApplyingDyn { .. } => fatal(&format!(
            "{}: a compiled frame applied an interpreted function value and the drive did not \
             resolve it — a driver on a machine frame must use `run_to_end`",
            what
        )),
        crate::coroutine::Paused::Suspended => fatal(&format!(
            "{}: a compiled frame suspended and the drive did not resolve it — a driver on a \
             machine frame must use `run_to_end`",
            what
        )),
    }
}

/// Applies an interpreted function value on *this* machine frame — what a
/// driver with no continuation stack behind it does with
/// [`coroutine::Paused::Applying`]. `Err` when the callee failed: what it
/// raised waits on the thread.
///
/// In an AOT executable there is no interpreter and nothing should produce an
/// interpreted closure either, so a missing hook is an invariant break rather
/// than a limitation to work around.
///
/// # Safety
///
/// `closure` must be a tagged interpreted-closure box in the heap registered
/// on this thread, and `args` its arguments in that closure's own declared
/// representations.
pub unsafe fn apply_on_this_frame(closure: i64, args: &[i64], what: &str) -> Result<i64, typelisp_abi::Raised> {
    match APPLY_INTERPRETED.with(|cell| cell.get()) {
        Some(hook) => {
            let v = hook(closure, args.as_ptr(), args.len() as u32);
            typelisp_abi::raised().map(|()| v)
        }
        None => fatal(&format!(
            "{}: a compiled frame applied an interpreted function value and no interpreter is \
             registered on this thread",
            what
        )),
    }
}

pub(crate) unsafe fn drive_to_completion(
    f: crate::coroutine::CoroutineFn,
    args: &[i64],
    env: &[i64],
    what: &str,
) -> Result<i64, typelisp_abi::Raised> {
    let mut stack = crate::coroutine::FrameStack::new();
    match stack.run_to_end(active_heap(), f, args, env, |closure, argv| {
        apply_on_this_frame(closure, argv, what)
    }) {
        Ok(v) => Ok(v),
        Err(paused) => Err(pause_on_a_machine_frame(paused, what)),
    }
}
