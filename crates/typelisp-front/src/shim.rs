//! `eval` as compiled code calls it.
//!
//! The last of the builtins that used to make a function uncompilable
//! (`docs/ja/reference/syntax.md` §10 now says every builtin compiles). Unlike the other ten `rt_*` shims defined outside
//! `typelisp-rt`, this one cannot be a small self-contained routine: `eval`
//! type-checks a form against the program's environment and then runs it, so
//! its implementation *is* the checker and the interpreter. That is why they
//! are a crate — see [`crate`]'s doc comment.
//!
//! Two deployments reach [`rt_eval`], and they find their environment in
//! different places.
//!
//! **JIT.** A `(compile f)`'d body running inside the interpreter. There is
//! already an `Interp` on this thread, registered by `Interp::enter_compiled`,
//! and it is the environment — the same thread-local the printer's hooks read.
//!
//! **AOT.** A standalone executable, with no interpreter in the process at
//! all. `compile-file` builds one at *compile* time and writes it into the
//! executable ([`crate::dump`]); [`rt_eval_init`] rebuilds it from those
//! bytes at startup. Nothing is re-read and nothing is re-checked.
//!
//! [`rt_eval_init`] runs at startup rather than on the first `eval` call, and
//! that is forced rather than chosen: `Interp::new` calls
//! `typelisp_rt::reset_global_table`, so an `Interp` created after the
//! program's global-init sequence would wipe the very slots the machine code
//! addresses. The generated `main` therefore runs it *between* `rt_heap_init`
//! and the global inits.
//!
//! What an eval'd form gets is a tree-walking evaluation, including of the
//! program's own functions: the dump carries their checked bodies as
//! ordinary interpreted definitions. Their compiled bodies are in the
//! executable and are what the *program* runs; teaching the restore to reuse
//! them would make an eval'd call faster, not different.

use std::cell::Cell;

use typelisp_abi::{active_heap, encode, fatal, tagged_arg};

use crate::eval::interp::{with_active_interp, Interp};

thread_local! {
    /// The embedded environment dump and the namespace an eval'd form is
    /// checked in, from [`rt_eval_state`]. `'static` because both point into
    /// the executable's read-only data.
    static EVAL_DUMP: Cell<Option<(&'static [u8], &'static str)>> = const { Cell::new(None) };

    /// The environment [`rt_eval_init`] rebuilt, or null under JIT where the
    /// running `Interp` is used instead.
    ///
    /// A raw pointer to a leaked `Interp` rather than a `RefCell<Interp>`: an
    /// eval'd form may itself call `eval`, and a `RefCell` borrow held across
    /// the inner call would panic. The leak matches `rt_heap_init`'s — the
    /// environment must outlive every compiled frame in the process, so there
    /// is no point at which freeing it would be correct.
    static AOT_ENV: Cell<*const Interp> = const { Cell::new(std::ptr::null()) };
}

/// Registers the embedded environment dump: `args` is `[ptr, len, ns_ptr,
/// ns_len]`, the dump and the `::`-joined namespace an eval'd form is checked
/// in (the entry file's module).
///
/// # Safety
///
/// `args` must point to 4 valid `i64`s naming immortal bytes, the second pair
/// UTF-8.
#[no_mangle]
pub unsafe extern "C" fn rt_eval_state(args: *const i64, argc: u32) -> i64 {
    if argc < 4 {
        fatal("rt_eval_state: expected 4 arguments");
    }
    let bytes = std::slice::from_raw_parts(*args as usize as *const u8, *args.add(1) as usize);
    let ns_bytes = std::slice::from_raw_parts(*args.add(2) as usize as *const u8, *args.add(3) as usize);
    let ns = match std::str::from_utf8(ns_bytes) {
        Ok(s) => s,
        Err(e) => fatal(&format!("rt_eval_state: the namespace is not UTF-8: {}", e)),
    };
    EVAL_DUMP.with(|c| c.set(Some((bytes, ns))));
    0
}

/// Rebuilds the AOT `eval` environment from the registered dump. Called
/// once from the generated `main`, after `rt_heap_init` and before the
/// program's global-init sequence — see this module's doc comment for why that
/// position is forced.
///
/// The dump was produced by the same `compile-file` run that produced this
/// executable, so a failure is a bug in the build rather than anything a user
/// could have caused: it aborts with a message instead of returning an error
/// nobody could act on.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_eval_init(_args: *const i64, _argc: u32) -> i64 {
    let (bytes, ns) = match EVAL_DUMP.with(|c| c.get()) {
        Some(b) => b,
        None => fatal("rt_eval_init: the environment dump was never registered"),
    };
    let interp = match crate::dump::restore_dump(active_heap(), bytes, ns) {
        Ok(i) => i,
        Err(e) => fatal(&format!("rt_eval_init: {}", e)),
    };
    let interp: &'static Interp = Box::leak(Box::new(interp));
    // The restore above ran `Interp::install_print_hooks` from inside, against
    // an `Interp` that was still a local of `dump::restore_dump`; the leak has
    // just moved it. Nothing in an AOT process would ever refresh that slot
    // again — there is no compiled-call crossing to do it at — so it has to be
    // re-registered here, at the address it will keep for the rest of the
    // process. Leaving it stale is not a missing optimisation: the printer and
    // this shim both dereference it, and a garbage `Interp` hangs or segfaults
    // rather than failing.
    interp.install_print_hooks();
    interp.executable_printer.set(true);
    AOT_ENV.with(|c| c.set(interp as *const Interp));
    0
}

/// What a call that needs the interpreter says on a thread that has none — a
/// worker's, reached through a machine frame (a compiled `print-object` the
/// printer called), which cannot move to the interpreter's thread the way a
/// task does before such a call (`call_state::SUSPEND_MAIN`). A panic the
/// program can catch, not an abort: nothing about the runtime is broken.
const NO_INTERPRETER_HERE: &str = "{}: the interpreter runs on the main thread, and this call was made on another \
     inside a call that cannot move there (a print method, say)";

/// `eval` for compiled code: `args[0]` is the form, tagged; the result is the
/// tagged `Result<Sexpr, EvalError>`.
///
/// # Safety
///
/// `args` must point to at least 1 valid tagged `i64`; a `Heap` must be
/// registered on this thread, and either an `Interp` (JIT) or a completed
/// [`rt_eval_init`] (AOT).
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_eval(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_eval: expected 1 argument");
    }
    let form = tagged_arg(args, 0);
    let heap = active_heap();

    // AOT first, and by [`AOT_ENV`] rather than by asking whether there is an
    // active `Interp`. That slot is only meaningful *directly after* a
    // compiled-call crossing wrote it (see `with_active_interp`), and an AOT
    // program's calls into compiled code write nothing — so "is it set?" is
    // not a question about which deployment this is. This is: only a generated
    // `main` calls `rt_eval_init`.
    let env = AOT_ENV.with(|c| c.get());
    let result = if env.is_null() {
        // JIT: the running interpreter is the program, and an environment
        // built here would be a second one with its own tables. Its slot was
        // written by `Interp::enter_compiled` on the way into the compiled
        // frame this call sits in.
        match with_active_interp(|i| i.eval_form(heap, &form)) {
            Some(r) => r,
            None => typelisp_abi::raise(NO_INTERPRETER_HERE.replace("{}", "eval")),
        }
    } else {
        (*env).eval_form(heap, &form)
    };
    match result {
        Ok(v) => encode(v),
        Err(e) => fatal(&format!("eval: {}", e)),
    }
}

/// `macroexpand-1` for compiled code: `args[0]` is the form, tagged; the
/// result is the tagged `Result<Option<Sexpr>, EvalError>`.
///
/// # Safety
///
/// Same as [`rt_eval`]'s.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_macroexpand_1(args: *const i64, argc: u32) -> i64 {
    expand_shim(args, argc, "rt_macroexpand_1", Interp::macroexpand_1_form)
}

/// `macroexpand` for compiled code: `args[0]` is the form, tagged; the result
/// is the tagged `Result<Sexpr, EvalError>`.
///
/// # Safety
///
/// Same as [`rt_eval`]'s.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_macroexpand(args: *const i64, argc: u32) -> i64 {
    expand_shim(args, argc, "rt_macroexpand", Interp::macroexpand_form)
}

/// The half [`rt_macroexpand_1`] and [`rt_macroexpand`] share: find the
/// environment the way [`rt_eval`] does, run `f` in it, and tag the result.
///
/// # Safety
///
/// Same as [`rt_eval`]'s.
unsafe fn expand_shim(
    args: *const i64,
    argc: u32,
    who: &str,
    f: impl Fn(&Interp, &mut typelisp_mem::Heap, typelisp_mem::Value) -> Result<typelisp_mem::Value, crate::eval::interp::EvalError>,
) -> i64 {
    if argc < 1 {
        fatal(&format!("{}: expected 1 argument", who));
    }
    let form = tagged_arg(args, 0);
    let heap = active_heap();
    let env = AOT_ENV.with(|c| c.get());
    let result = if env.is_null() {
        match with_active_interp(|i| f(i, heap, form)) {
            Some(r) => r,
            None => typelisp_abi::raise(NO_INTERPRETER_HERE.replace("{}", who)),
        }
    } else {
        f(&*env, heap, form)
    };
    match result {
        Ok(v) => encode(v),
        Err(e) => fatal(&format!("{}: {}", who, e)),
    }
}


/// [`typelisp_rt::rt_run_program`]/`rt_run_program_int` for an
/// `eval`-carrying AOT executable: the same protocol — an initialiser
/// array, its count, and the entry point, all coroutine-ABI addresses — but
/// driven by *this thread's* `Interp` (`AOT_ENV`, built by [`rt_eval_init`])
/// through its own scheduler (`Interp::run_compiled_program`) rather than a
/// bare `Scheduler<CompiledTask>`.
///
/// The reason this exists at all rather than reusing `rt_run_program`: an
/// executable that calls `eval` has an `Interp`, and `(task ...)` inside an
/// eval'd form is admitted to *that* `Interp`'s scheduler
/// (`Interp::eval_cps`) — a second, bare `Scheduler<CompiledTask>` running
/// the program's own initialisers and `main` would be a scheduler the
/// eval'd tasks are invisible to, and vice versa. `compile-file` decides
/// which of the two shims the generated `main` calls by whether the program
/// calls `eval` at all (`aot::EVAL_SHIMS`), the same generation-time
/// decision every other ABI fork in the compiler makes.
///
/// # Safety
///
/// [`typelisp_rt::rt_run_program`]'s: `argc` must be 3, and `args` must
/// describe a valid initialiser array, its count, and an entry address, all
/// `coroutine_fn_type` — plus a completed [`rt_eval_init`] must already have
/// run on this thread (`AOT_ENV` must be set; the generated `main` always
/// calls it first when it emits a call to this shim at all).
#[no_mangle]
pub unsafe extern "C" fn rt_run_program_interp(args: *const i64, argc: u32) -> i64 {
    run_program_interp(args, argc, false)
}

/// [`rt_run_program_interp`] for a `main` declared to return `int`: the
/// answer is a tagged word, and the exit code is the fixnum's payload —
/// [`typelisp_rt::rt_run_program_int`]'s own reasoning, here for the
/// `eval`-carrying entry.
///
/// # Safety
///
/// [`rt_run_program_interp`]'s.
#[no_mangle]
pub unsafe extern "C" fn rt_run_program_interp_int(args: *const i64, argc: u32) -> i64 {
    run_program_interp(args, argc, true)
}

/// The half [`rt_run_program_interp`] and [`rt_run_program_interp_int`]
/// share: read the program, find the environment, drive it, and turn the
/// result into an exit code the same way the two entry points in
/// `typelisp_rt` do for the interpreter-free case.
///
/// # Safety
///
/// [`rt_run_program_interp`]'s.
unsafe fn run_program_interp(args: *const i64, argc: u32, main_returns_int: bool) -> i64 {
    let program = typelisp_rt::read_program(args, argc, "rt_run_program_interp");
    let interp = match AOT_ENV.with(|c| c.get()) {
        p if !p.is_null() => &*p,
        _ => fatal("rt_run_program_interp: no environment — rt_eval_init has not run on this thread"),
    };
    let heap = active_heap();
    let inits: Vec<usize> = program.inits.iter().map(|f| *f as usize).collect();
    let entry = program.entry as usize;
    let ret = if main_returns_int { crate::check::repr::Repr::Int } else { crate::check::repr::Repr::Unit };
    let call = std::panic::AssertUnwindSafe(|| match run_program_result(interp, heap, &inits, entry, ret, main_returns_int) {
        Ok(v) => v,
        Err(code) => code,
    });
    typelisp_rt::run_entry_payload(std::panic::catch_unwind(call))
}

/// [`run_program_interp`]'s inner call: `Interp::run_compiled_program`'s
/// result read off as the same two shapes `typelisp_rt::rt_run_program`/
/// `rt_run_program_int` read a `CompiledTask`'s off — an already-encoded
/// word, or a fixnum's raw payload — and a failure printed and turned into
/// [`typelisp_rt::EXIT_CODE_PANIC`], the same line a task's own failure
/// prints on the interpreter-free side.
unsafe fn run_program_result(
    interp: &Interp,
    heap: &mut typelisp_mem::Heap,
    inits: &[usize],
    entry: usize,
    ret: crate::check::repr::Repr,
    main_returns_int: bool,
) -> Result<i64, i64> {
    match interp.run_compiled_program(heap, inits, entry, ret) {
        Ok(v) if main_returns_int => match v {
            typelisp_mem::Value::Int(n) => Ok(n),
            typelisp_mem::Value::Boxed(id) if heap.is_bignum(id) => {
                fatal(&format!("main returned {}, which is not a process exit code", heap.bignum_value(id)))
            }
            other => fatal(&format!("main declared to return int answered {:?}", other)),
        },
        // A `()`-returning `main` that returns normally exits 0 — the same
        // answer `typelisp_rt::rt_run_program` gives; the word `()` is
        // encoded as is not an exit code.
        Ok(_) => Ok(typelisp_rt::EXIT_CODE_SUCCESS),
        Err(e) => {
            eprintln!("{}", e);
            Err(typelisp_rt::EXIT_CODE_PANIC)
        }
    }
}
