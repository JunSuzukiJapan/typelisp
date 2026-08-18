//! The printing builtins as compiled code calls them: `rt_format`,
//! `rt_print`, `rt_println`, `rt_pprint` and the `pprint-*` operators.
//!
//! # Why these are here and not in `typelisp-rt`
//!
//! Every other `rt_*` shim lives in `typelisp-rt`. These cannot, and the
//! reason is the linker rather than taste. `compile-file` links exactly one
//! archive — `typelisp-rt`'s `staticlib` — and the linker pulls archive
//! *members*: an object is taken whole or not at all. A wrapper in
//! `typelisp-rt` would share an object with `rt_cons`, which every program
//! calls, so its reference to the directive engine would drag the engine into
//! every program. Defining them in this crate keeps the whole subsystem in
//! members of its own, pulled in exactly when something calls one.
//!
//! (`typelisp-rt` names this crate once, in a `pub use` that generates no
//! code, purely so rustc puts its objects in the archive at all. See that
//! line's comment.)
//!
//! # Representations
//!
//! [`crate::runtime::print_builtin`] speaks [`Value`], the way the
//! interpreter does. The compiled ABI does not — an `i64`, a `bool` and `()`
//! are bare machine words there, only heap values are tagged — so each shim
//! converts at its own edge, per the signature the checker gave it
//! (`Checker::check_format`/`check_print_like`/`check_pprint`, and
//! `registry`'s `pprint-*` entries).
//!
//! # Unwinding
//!
//! `extern "C-unwind"`, not `extern "C"`: a bad `~` directive and a
//! `print-object` method that panics are both language-level failures, and
//! [`typelisp_abi::raise`] unwinds out through here to the `catch`/
//! `unwind-protect` region that should see them. A plain `extern "C"`
//! boundary would turn that into an abort.

use typelisp_abi::{active_heap, encode, fatal, raise, tagged_arg};
use typelisp_mem::Value;

use crate::runtime::{print_builtin, PrintError};

/// Runs `name` with `args` already decoded, and converts the outcome to the
/// compiled ABI: [`PrintError::Raise`] unwinds, [`PrintError::Shape`] aborts.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
unsafe fn run(name: &str, args: Vec<Value>) -> Value {
    match print_builtin(active_heap(), name, &args) {
        Some(Ok(v)) => v,
        Some(Err(PrintError::Raise(msg))) => raise(msg),
        Some(Err(PrintError::Shape(msg))) => fatal(&msg),
        None => fatal(&format!("print shim: \"{}\" is not a printing builtin", name)),
    }
}

/// Aborts unless the call site passed at least `n` arguments.
///
/// Every shim below calls this once, before reading anything: the reads
/// themselves are unchecked, so the check has to cover all of them and
/// therefore has to come first.
fn check_argc(argc: u32, n: usize, who: &str) {
    if (argc as usize) < n {
        fatal(&format!("{}: expected {} arguments, got {}", who, n, argc));
    }
}

/// `(format <bool> control args…)` — CLHS 22.3.
///
/// `args[0]` is the destination as a bare `0`/`1`, `args[1]` the control
/// string and `args[2]` the `Sexpr` list of already-wrapped directive
/// arguments. Returns the built text as a tagged string.
///
/// # Safety
///
/// `args` must point to at least 3 valid `i64`s in those representations; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_format(args: *const i64, argc: u32) -> i64 {
    check_argc(argc, 3, "rt_format");
    let dest = Value::Bool(*args != 0);
    let control = tagged_arg(args, 1);
    let list = tagged_arg(args, 2);
    encode(run("format-rt", vec![dest, control, list]))
}

/// Generates `rt_print`/`rt_println`, which differ only in the trailing
/// newline. `args[0]` is the control string, `args[1]` the argument list;
/// both return `()`, which is a bare `0`.
macro_rules! print_shim {
    ($shim:ident, $name:literal) => {
        /// `print`/`println` for compiled code — see [`crate::runtime::print_builtin`].
        ///
        /// # Safety
        ///
        /// `args` must point to at least 2 valid tagged `i64`s; a `Heap` must
        /// be registered on this thread.
        #[no_mangle]
        pub unsafe extern "C-unwind" fn $shim(args: *const i64, argc: u32) -> i64 {
            check_argc(argc, 2, stringify!($shim));
            run($name, vec![tagged_arg(args, 0), tagged_arg(args, 1)]);
            0
        }
    };
}

print_shim!(rt_print, "print-rt");
print_shim!(rt_println, "println-rt");

/// `pprint`/`pprint-fill`/`pprint-linear`/`pprint-tabular` — `args[0]` names
/// which (as a string, so one shim serves all four), `args[1]` is the object
/// and `args[2]` is `pprint-tabular`'s column width as a bare `i64`.
///
/// # Safety
///
/// `args` must point to at least 3 valid `i64`s in those representations; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint(args: *const i64, argc: u32) -> i64 {
    check_argc(argc, 3, "rt_pprint");
    let which = tagged_arg(args, 0);
    let value = tagged_arg(args, 1);
    let colinc = Value::Int(*args.add(2));
    run("pprint-rt", vec![which, value, colinc]);
    0
}

/// Opens a logical block: `args[0]` is the list `pprint-pop` walks,
/// `args[1]`/`args[3]` the prefix and suffix strings, `args[2]` the
/// per-line-prefix flag as a bare `0`/`1`.
///
/// # Safety
///
/// `args` must point to at least 4 valid `i64`s in those representations; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_block_start(args: *const i64, argc: u32) -> i64 {
    check_argc(argc, 4, "rt_pprint_block_start");
    let obj = tagged_arg(args, 0);
    let prefix = tagged_arg(args, 1);
    let per_line = Value::Bool(*args.add(2) != 0);
    let suffix = tagged_arg(args, 3);
    run("pprint-block-start-rt", vec![obj, prefix, per_line, suffix]);
    0
}

/// Closes a logical block; closing the outermost one writes the session out.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_block_end(_args: *const i64, _argc: u32) -> i64 {
    run("pprint-block-end-rt", Vec::new());
    0
}

/// `(pprint-newline :linear|:fill|:miser|:mandatory)`.
///
/// # Safety
///
/// `args` must point to at least 1 valid tagged `i64`; a `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_newline(args: *const i64, argc: u32) -> i64 {
    check_argc(argc, 1, "rt_pprint_newline");
    run("pprint-newline", vec![tagged_arg(args, 0)]);
    0
}

/// `(pprint-indent :block|:current n)` — `args[1]` is a bare `i64`.
///
/// # Safety
///
/// `args` must point to at least 2 valid `i64`s in those representations; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_indent(args: *const i64, argc: u32) -> i64 {
    check_argc(argc, 2, "rt_pprint_indent");
    let kind = tagged_arg(args, 0);
    let n = Value::Int(*args.add(1));
    run("pprint-indent", vec![kind, n]);
    0
}

/// `(pprint-tab kind colnum colinc)` — `args[1]`/`args[2]` are bare `i64`s.
///
/// # Safety
///
/// `args` must point to at least 3 valid `i64`s in those representations; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_tab(args: *const i64, argc: u32) -> i64 {
    check_argc(argc, 3, "rt_pprint_tab");
    let kind = tagged_arg(args, 0);
    let colnum = Value::Int(*args.add(1));
    let colinc = Value::Int(*args.add(2));
    run("pprint-tab", vec![kind, colnum, colinc]);
    0
}

/// `(pprint-pop)` — the next element of the innermost open block's list, as a
/// tagged `Sexpr`.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_pop(_args: *const i64, _argc: u32) -> i64 {
    encode(run("pprint-pop", Vec::new()))
}

/// `(pprint-list-exhausted)` — a bare `0`/`1`.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_list_exhausted(_args: *const i64, _argc: u32) -> i64 {
    match run("pprint-list-exhausted", Vec::new()) {
        Value::Bool(b) => i64::from(b),
        other => fatal(&format!("rt_pprint_list_exhausted: expected a bool, got {:?}", other)),
    }
}
