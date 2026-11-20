//! Starting and finishing a compiled program: the AOT heap, the entry points an
//! executable's `main` calls, and driving a compiled body from a machine frame.

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
/// code for the same reason — see [`run_entry_payload`].
pub const EXIT_CODE_PANIC: i64 = 1;

/// The exit code of a program whose `()`-returning `main` returned normally.
/// Named, and not the value `main` answered, because that answer is the word
/// `()` is encoded as, which differs between the entry shims' decodings.
pub const EXIT_CODE_SUCCESS: i64 = 0;

/// Runs an AOT executable's compiled entry point (`tl_main`) with a catch
/// around it, returning its exit code — or, if a `(panic ...)` unwound out of
/// it, printing the message and returning [`EXIT_CODE_PANIC`].
///
/// This exists because an AOT executable has nowhere else to put the catch.
/// In the JIT path the interpreter is a Rust frame that can wrap the call
/// (`typelisp::compile::catch_compiled_panic`); AOT's `main` is *generated
/// LLVM code* that calls `rt_heap_init` and then `tl_main` directly
/// (`compile::aot`'s entry-point builder), with no Rust frame in between. So
/// the catch moves into the runtime, and the generated `main` calls this
/// instead of calling `tl_main` itself.
///
/// Letting the panic unwind out of `main` instead is not an option: `main` is
/// the C entry point, and unwinding past it is undefined.
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
    let f: unsafe extern "C-unwind" fn(*const i64, u32) -> i64 = std::mem::transmute(entry as usize);
    // Only a `()`-returning `main` reaches this shim (`compile::aot` refuses
    // an `int` one under this ABI), and a `main` that returns normally exits
    // 0 — whatever word its `()` happens to be.
    let call = std::panic::AssertUnwindSafe(|| {
        f(std::ptr::null(), 0);
        EXIT_CODE_SUCCESS
    });
    run_entry_payload(std::panic::catch_unwind(call))
}

/// [`rt_run_entry`] for a program built under the coroutine ABI: the same
/// panic handling with a **scheduler** in front of it, and the program's
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
    let program = read_program(args, argc, "rt_run_program");
    // A `()`-returning `main` that returns normally exits 0; the word its
    // `()` is encoded as is not an exit code.
    let call = std::panic::AssertUnwindSafe(|| match run_program(&program) {
        Ok(_) => EXIT_CODE_SUCCESS,
        Err(code) => code,
    });
    run_entry_payload(std::panic::catch_unwind(call))
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
    let program = read_program(args, argc, "rt_run_program_int");
    let call = std::panic::AssertUnwindSafe(|| match run_program(&program) {
        Ok(Value::Int(n)) => n,
        Ok(Value::Boxed(id)) if active_heap().is_bignum(id) => {
            fatal(&format!("main returned {}, which is not a process exit code", active_heap().bignum_value(id)))
        }
        Ok(other) => fatal(&format!("main declared to return int answered {:?}", other)),
        Err(code) => code,
    });
    run_entry_payload(std::panic::catch_unwind(call))
}

/// What a generated `main` hands the program entry: the initialisers and
/// the entry point, as coroutine-ABI addresses.
pub struct Program {
    pub inits: Vec<crate::coroutine::CoroutineFn>,
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
    let inits = words.into_iter().map(|w| std::mem::transmute::<usize, crate::coroutine::CoroutineFn>(w as usize)).collect();
    let entry: crate::coroutine::CoroutineFn = std::mem::transmute(*args.add(2) as usize);
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
/// `TYPELISP_THREADS` threads step tasks, this one included
/// ([`sched::thread_count`](crate::sched::thread_count)). The workers are
/// started before the first initialiser, share this thread's heap (a view
/// each), its global/vtable/stream tables and its printer's registrations,
/// and are never joined: when `main` returns the program ends, and every
/// task still running with it — Go's rule.
///
/// # Safety
///
/// [`rt_run_program`]'s.
unsafe fn run_program(program: &Program) -> Result<Value, i64> {
    let heap = active_heap();
    let threads = match crate::sched::thread_count() {
        Ok(n) => n,
        Err(m) => {
            eprintln!("error: {}", m);
            return Err(EXIT_CODE_PANIC);
        }
    };
    let sched = match crate::sched::SchedShared::<crate::sched::CompiledTask>::new() {
        Ok(s) => std::sync::Arc::new(s),
        Err(e) => fatal(&format!("the scheduler could not be set up: {}", e)),
    };
    // The printer reads the prelude's control variables from their compiled
    // slots, and the id -> slot table is this crate's.
    typelisp_print::aot::set_global_slots(global_perm_idx);
    let rt = crate::shared::rt_shared();
    let print = typelisp_print::shared::print_shared();
    let hooks = typelisp_print::runtime::print_hooks();
    let started = crate::sched::start_workers(&sched, heap, threads - 1, &(), move || {
        crate::shared::set_rt_shared(std::sync::Arc::clone(&rt));
        typelisp_print::shared::set_print_shared(std::sync::Arc::clone(&print));
        typelisp_print::runtime::set_print_hooks(Some(hooks));
    });
    if let Err(e) = started {
        fatal(&format!("a scheduler worker thread could not be started: {}", e));
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
            // The same line `run_entry_payload` prints for a panic that
            // reached the entry as a Rust unwind — a task's failure is the
            // program's.
            Err(failure) => {
                eprintln!("{}", failure);
                Err(EXIT_CODE_PANIC)
            }
        }
    };
    for init in &program.inits {
        // An initialiser's value is the global's storage id, already stored
        // by the body itself (`compile-global-init`); nothing here reads it.
        run(heap, *init, true)?;
    }
    run(heap, program.entry, false)
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
    let f: crate::coroutine::CoroutineFn = std::mem::transmute(*args as usize);
    let callee_args = *args.add(1) as usize as *const i64;
    let callee_argc = *args.add(2);
    let words = collect_words(callee_args, callee_argc);
    drive_to_completion(f, &words, &[], "rt_drive_body")
}

/// Turns whichever entry point ran into an exit code: a typelisp-level
/// `panic` or an uncaught `throw` that reached it as a Rust unwind is
/// reported and becomes [`EXIT_CODE_PANIC`]; anything else keeps unwinding.
///
/// `pub` for the interpreter's own program entry (`typelisp_front::shim`),
/// which puts the same catch around the same kind of run.
///
/// # Safety
///
/// A `Heap` must be registered on this thread (an uncaught throw's tag is
/// read out of it).
pub unsafe fn run_entry_payload(outcome: std::thread::Result<i64>) -> i64 {
    let call = std::panic::AssertUnwindSafe(|| match outcome {
        Ok(v) => v,
        Err(payload) => std::panic::resume_unwind(payload),
    });
    match std::panic::catch_unwind(call) {
        Ok(code) => code,
        // Only a typelisp-level `panic` is turned into an exit code; anything
        // else is a real bug in the runtime and keeps unwinding, exactly as
        // `catch_compiled_panic` does on the JIT side.
        Err(payload) => {
            let payload = match payload.downcast::<CompiledPanic>() {
                Ok(p) => {
                    eprintln!("panic: {}", p.message);
                    return EXIT_CODE_PANIC;
                }
                Err(other) => other,
            };
            // A `throw` that found no enclosing `catch` anywhere. Reported
            // with the same wording `EvalError::Throw`'s `Display` uses, so
            // the two front ends say the same thing about the same program.
            match payload.downcast::<CompiledThrow>() {
                Ok(_) => {
                    let (tag, _) =
                        take_throw().unwrap_or_else(|| fatal("a compiled throw unwound with no throw in flight"));
                    eprintln!("throw: no enclosing (catch '{}) for this throw", tag.spelling());
                    EXIT_CODE_PANIC
                }
                Err(other) => std::panic::resume_unwind(other),
            }
        }
    }
}

unsafe fn collect_words(p: *const i64, n: i64) -> Vec<i64> {
    if n < 0 {
        fatal(&format!("{} is not a word count", n));
    }
    (0..n as usize).map(|i| *p.add(i)).collect()
}

/// What a driver whose caller is a machine frame does with a pause.
///
/// An unwind is re-raised, so it keeps travelling towards a `catch` above
/// this boundary. Nothing else can come out: every driver standing on a
/// machine frame goes through `FrameStack::run_to_end`, which runs an
/// interpreted `apply` on that frame and resolves a suspension through the
/// driving scheduler (answering it, or refusing it into the chain as a
/// panic). What still stands on a machine frame is `rt_drive_body` (the
/// printer's door); the executable's initialisers and `main` run under a
/// scheduler (`run_program`).
unsafe fn pause_on_a_machine_frame(paused: crate::coroutine::Paused, what: &str) -> ! {
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
/// [`coroutine::Paused::Applying`].
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
pub unsafe fn apply_on_this_frame(closure: i64, args: &[i64], what: &str) -> i64 {
    match APPLY_INTERPRETED.with(|cell| cell.get()) {
        Some(hook) => hook(closure, args.as_ptr(), args.len() as u32),
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
) -> i64 {
    let mut stack = crate::coroutine::FrameStack::new();
    match stack.run_to_end(active_heap(), f, args, env, |closure, argv| {
        apply_on_this_frame(closure, argv, what)
    }) {
        Ok(v) => v,
        Err(paused) => pause_on_a_machine_frame(paused, what),
    }
}
