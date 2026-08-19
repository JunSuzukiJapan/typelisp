//! `eval` as compiled code calls it.
//!
//! The last of the builtins that used to make a function uncompilable
//! (`docs/syntax.md` §10). Unlike the other ten `rt_*` shims defined outside
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
    /// The embedded environment dump, from [`rt_eval_state`]. A `&'static [u8]`
    /// because it points into the executable's read-only data.
    static EVAL_DUMP: Cell<Option<&'static [u8]>> = const { Cell::new(None) };

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

/// Registers the embedded environment dump: `args` is `[ptr, len]`.
///
/// # Safety
///
/// `args` must point to 2 valid `i64`s naming immortal bytes.
#[no_mangle]
pub unsafe extern "C" fn rt_eval_state(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_eval_state: expected 2 arguments");
    }
    let ptr = *args as usize as *const u8;
    let len = *args.add(1) as usize;
    EVAL_DUMP.with(|c| c.set(Some(std::slice::from_raw_parts(ptr, len))));
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
    let bytes = match EVAL_DUMP.with(|c| c.get()) {
        Some(b) => b,
        None => fatal("rt_eval_init: the environment dump was never registered"),
    };
    let interp = match crate::dump::restore_dump(active_heap(), bytes) {
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
    AOT_ENV.with(|c| c.set(interp as *const Interp));
    0
}

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
            None => fatal("rt_eval: no environment — neither an active interpreter nor a completed rt_eval_init"),
        }
    } else {
        (*env).eval_form(heap, &form)
    };
    match result {
        Ok(v) => encode(v),
        Err(e) => fatal(&format!("eval: {}", e)),
    }
}
