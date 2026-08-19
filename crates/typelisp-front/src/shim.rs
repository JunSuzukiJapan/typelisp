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
//! all. `compile_file` emits a startup sequence that hands this module the
//! program's own source text and the ids of its compiled globals, and
//! [`rt_eval_init`] builds an environment out of them: the prelude plus a
//! replay of the program's own definitions, checked and registered exactly as
//! the interpreter would have, sharing the running program's heap and global
//! storage.
//!
//! The AOT environment is built at startup rather than on the first `eval`
//! call, and that is forced rather than chosen: `Interp::new` calls
//! `typelisp_rt::reset_global_table`, so an `Interp` created after the
//! program's global-init sequence would wipe the very slots the machine code
//! addresses. The generated `main` therefore runs [`rt_eval_init`] *between*
//! `rt_heap_init` and the global inits.
//!
//! What an eval'd form gets is a tree-walking evaluation, including of the
//! program's own functions: the replay registers their bodies as ordinary
//! interpreted `FnDef`s. Their compiled bodies are in the executable and are
//! what the *program* runs; teaching the replay to reuse them would make an
//! eval'd call faster, not different.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use typelisp_abi::{active_heap, encode, fatal, tagged_arg};
use typelisp_mem::{Heap, Value};

use crate::check::Checker;
use crate::eval::interp::{with_active_interp, Interp};
use crate::read::Reader;
use crate::types::Path;

thread_local! {
    /// The program's own source, from [`rt_eval_source`]. A `&'static str`
    /// because it points into the executable's read-only data.
    static PROGRAM_SOURCE: Cell<Option<&'static str>> = const { Cell::new(None) };

    /// `(global path, compiled slot id)` from [`rt_eval_global`], applied to
    /// the environment before the replay so a `defvar` in the program's source
    /// is recognized as already initialized.
    static COMPILED_GLOBALS: RefCell<Vec<(String, usize)>> = const { RefCell::new(Vec::new()) };

    /// The environment [`rt_eval_init`] built, or null under JIT where the
    /// running `Interp` is used instead.
    ///
    /// A raw pointer to a leaked `Interp` rather than a `RefCell<Interp>`:
    /// an eval'd form may itself call `eval`, and a `RefCell` borrow held
    /// across the inner call would panic. The leak matches `rt_heap_init`'s —
    /// the environment must outlive every compiled frame in the process, so
    /// there is no point at which freeing it would be correct.
    static AOT_ENV: Cell<*const Interp> = const { Cell::new(std::ptr::null()) };
}

/// A `(pointer, length)` pair from the generated startup call, whose arguments
/// are LLVM global string constants.
///
/// # Safety
///
/// `args[i]`/`args[i + 1]` must be a valid pointer and length into live,
/// immortal memory.
unsafe fn static_str(args: *const i64, i: usize, who: &str) -> &'static str {
    let ptr = *args.add(i) as usize as *const u8;
    let len = *args.add(i + 1) as usize;
    match std::str::from_utf8(std::slice::from_raw_parts(ptr, len)) {
        Ok(s) => s,
        Err(_) => fatal(&format!("{}: argument {} is not valid UTF-8", who, i)),
    }
}

/// Registers the program's own source text: `args` is `[ptr, len]`.
///
/// # Safety
///
/// `args` must point to 2 valid `i64`s naming immortal UTF-8 bytes.
#[no_mangle]
pub unsafe extern "C" fn rt_eval_source(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_eval_source: expected 2 arguments");
    }
    let src = static_str(args, 0, "rt_eval_source");
    PROGRAM_SOURCE.with(|c| c.set(Some(src)));
    0
}

/// Registers one global's compiled slot id: `args` is `[name_ptr, name_len,
/// id]`, the name being the global's full path (`m::x`).
///
/// # Safety
///
/// `args` must point to 3 valid `i64`s in those representations.
#[no_mangle]
pub unsafe extern "C" fn rt_eval_global(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_eval_global: expected 3 arguments");
    }
    let name = static_str(args, 0, "rt_eval_global");
    let id = *args.add(2) as usize;
    COMPILED_GLOBALS.with(|t| t.borrow_mut().push((name.to_string(), id)));
    0
}

/// Builds the AOT `eval` environment. Called once from the generated `main`,
/// after `rt_heap_init` and before the program's global-init sequence — see
/// this module's doc comment for why that position is forced.
///
/// Everything here already type-checked once, in the `compile-file` run that
/// produced this executable, so a failure is a bug in the build rather than
/// anything a user could have caused: it aborts with a message instead of
/// returning an error nobody could act on.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_eval_init(_args: *const i64, _argc: u32) -> i64 {
    let source = match PROGRAM_SOURCE.with(|c| c.get()) {
        Some(s) => s,
        None => fatal("rt_eval_init: the program's source was never registered"),
    };
    let heap = active_heap();

    let mut chk = Checker::new();
    let mut interp = Interp::new();
    crate::prelude::load_interpreted(heap, &mut chk, &mut interp);

    // Before the replay: this is what makes the program's own `defvar`s
    // recognizable as somebody else's storage rather than this environment's
    // to create.
    COMPILED_GLOBALS.with(|t| {
        for (name, id) in t.borrow().iter() {
            interp.bind_compiled_global(parse_path(name), *id);
        }
    });

    let reader = Reader::new();
    let forms = match reader.read_all_in(heap, "<program>", source) {
        Ok(f) => f,
        Err(e) => fatal(&format!("rt_eval_init: re-reading the program's source failed: {}", e)),
    };
    chk.predeclare_program(heap, &forms);
    for v in forms {
        let tl = match chk.check_form(heap, &interp, v) {
            Ok(tl) => tl,
            Err(e) => fatal(&format!("rt_eval_init: re-checking the program's source failed: {}", e)),
        };
        // Warnings were already printed by the `compile-file` run; repeating
        // them at every startup would be noise on the program's own stderr.
        let _ = chk.take_warnings();
        if already_initialized_global(heap, &interp, tl) {
            continue;
        }
        if let Err(e) = interp.exec(heap, tl) {
            fatal(&format!("rt_eval_init: re-running the program's definitions failed: {}", e));
        }
    }

    interp.set_checker(Rc::new(RefCell::new(chk)));
    let interp: &'static Interp = Box::leak(Box::new(interp));
    // The load above ran `Interp::install_print_hooks` from inside, against an
    // `Interp` that was still a local of this function; the leak has just
    // moved it. Nothing in an AOT process would ever refresh that slot again —
    // there is no compiled-call crossing to do it at — so it has to be
    // re-registered here, at the address it will keep for the rest of the
    // process. Leaving it stale is not a missing optimisation: the printer and
    // this shim both dereference it, and a garbage `Interp` hangs or segfaults
    // rather than failing.
    interp.install_print_hooks();
    AOT_ENV.with(|c| c.set(interp as *const Interp));
    0
}

/// Whether `tl` is a `defvar` for a global the running program has already
/// created and initialized.
///
/// Replaying it would run the initializer a second time — with its side
/// effects, and overwriting whatever the program has since stored — because
/// the interpreter's `defvar` always evaluates and assigns (typelisp's
/// `defvar` is not CL's "only if unbound"). Skipping the `exec` costs nothing
/// else: the *checker* still saw the form, which is what `eval` needs to
/// type-check a reference to the variable, and reads and writes both consult
/// `compiled_globals` before the module tree.
fn already_initialized_global(heap: &Heap, interp: &Interp, tl: Value) -> bool {
    if crate::check::core::op(heap, tl) != Some("defvar") {
        return false;
    }
    // Through `core::path_field`, not by matching `Value::Path`: a root-level
    // name is stored as a bare `Value::Symbol`, so reading the field by hand
    // silently misses every unqualified global — which is most of them.
    match crate::check::core::path_field(heap, tl, 0) {
        Some(path) => interp.has_compiled_global(&path),
        None => false,
    }
}

/// `m::x` back into the `Path` the interpreter keys its tables by — the
/// inverse of the `Display` `compile-file` wrote the name with.
fn parse_path(name: &str) -> Path {
    Path::from_segments(name.split("::").map(str::to_string).collect())
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
pub unsafe extern "C" fn rt_eval(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_eval: expected 1 argument");
    }
    let form = tagged_arg(args, 0);
    let heap = active_heap();

    // AOT first, and by [`AOT_ENV`] rather than by asking whether there is an
    // active `Interp`. That slot is only meaningful *directly after* a
    // compiled-call crossing wrote it (see `with_active_interp`), and an AOT
    // program's calls into compiled code write nothing — so "is it set?" is
    // not a question about which deployment this is. This is: only a
    // generated `main` calls `rt_eval_init`.
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
