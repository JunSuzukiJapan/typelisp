//! The small system builtins as *values*: one implementation, called from
//! both sides of the compile boundary.
//!
//! Same arrangement as [`crate::stream_builtin`], for the same reason: each
//! of these has a failure mode and a wrapper shape (`Result<_, ParseIntError>`
//! …), and a second implementation on the compiled side would be another
//! chance to disagree about the error text a program can read back out of the
//! `Err`. `Interp::eval_builtin` (interpreted) and the `rt_*` shims in
//! [`crate::lib`](crate) (compiled) both call in here.
//!
//! Deliberately *not* here: the argument/result representation conversions.
//! A compiled `i64` is a bare machine word and a compiled `string` is a
//! tagged heap value; that split belongs to the calling convention, so each
//! shim converts at its own edge.
//!
//! Kept in its own file rather than folded into [`crate::lib`](crate) so a
//! linked executable that never parses or asks the time does not drag the
//! `std::time` machinery in with it — see this crate's own module docs on how
//! archive-member granularity decides what an AOT binary pays for.

use typelisp_mem::{Heap, Value};

/// The type keys these results are built with. Same standing exception (and
/// same guard test) as [`crate::stream_builtin`]'s three: this crate has no
/// `Path` to derive a key from, so the spellings are written out and
/// `tests/type_identity_guard_test.rs` asserts each equals what
/// `type_key::type_key_of` produces for that root name.
pub const RESULT_TYPE_KEY: &str = "result";
pub const PARSE_INT_ERROR_TYPE_KEY: &str = "parseinterror";
pub const PARSE_FLOAT_ERROR_TYPE_KEY: &str = "parsefloaterror";

/// `Ok(v)`, matching `result_def`'s variant order (`ok` = 0, `err` = 1).
fn result_ok(heap: &mut Heap, v: Value) -> Value {
    heap.alloc_enum(RESULT_TYPE_KEY.to_string(), 0, vec![v])
}

/// `Err(<ErrType>(msg))`: the concrete single-variant error type of the
/// failing builtin, wrapped in `Result`'s `err`.
fn result_err(heap: &mut Heap, err_type_key: &str, msg: String) -> Value {
    let msg_val = heap.alloc_string(msg);
    let err_val = heap.alloc_enum(err_type_key.to_string(), 0, vec![msg_val]);
    heap.alloc_enum(RESULT_TYPE_KEY.to_string(), 1, vec![err_val])
}

/// `(parse-int s)`: a decimal `i32` via `str::parse`, `Err` on anything else.
/// The `Ok` payload is an `i32`-typed `Value::Int` — the checker's return
/// type is `Result<i32, ParseIntError>`, so the value must fit `i32` even
/// though the runtime representation is a uniform `i64`.
pub fn parse_int(heap: &mut Heap, s: &str) -> Value {
    match s.parse::<i32>() {
        Ok(n) => result_ok(heap, Value::Int(n as i64)),
        Err(_) => result_err(
            heap,
            PARSE_INT_ERROR_TYPE_KEY,
            format!("parse-int: invalid integer literal: {:?}", s),
        ),
    }
}

/// `(parse-float s)`: an `f64` via `str::parse` (which accepts `inf`/`nan`),
/// `Err` on anything else. The `Ok` payload is a boxed float, the one
/// representation an `f64` has once it is a field of something.
pub fn parse_float(heap: &mut Heap, s: &str) -> Value {
    match s.parse::<f64>() {
        Ok(f) => {
            let v = heap.alloc_float(f);
            result_ok(heap, v)
        }
        Err(_) => result_err(
            heap,
            PARSE_FLOAT_ERROR_TYPE_KEY,
            format!("parse-float: invalid float literal: {:?}", s),
        ),
    }
}

/// `(get-universal-time)` (CLHS 25.1): seconds since CL's epoch,
/// 1900-01-01 UTC — 2_208_988_800 seconds before the Unix one.
pub fn get_universal_time() -> i64 {
    const UNIX_TO_CL_EPOCH_SECS: i64 = 2_208_988_800;
    let unix_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    unix_secs + UNIX_TO_CL_EPOCH_SECS
}

/// `(get-internal-real-time)` (CLHS 25.1): elapsed
/// `internal-time-units-per-second` (microseconds) since a reference point
/// fixed at the first call — a monotonic `Instant`, not wall-clock, so
/// `time`'s measurement cannot go backwards under a clock adjustment.
///
/// The reference point is per *process*, which is what makes an interpreted
/// and a compiled call comparable: `(time ...)` around a compiled body still
/// subtracts two readings off one clock.
pub fn get_internal_real_time() -> i64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(std::time::Instant::now);
    start.elapsed().as_micros() as i64
}

// ---- The compiled-code edge -------------------------------------------
//
// The `#[no_mangle]` shims live *beside* their implementation rather than in
// `lib.rs`. A linker takes archive members whole and rustc partitions codegen
// units by module, so keeping a shim out of the 5,000-line `lib.rs` is what
// stops it from sharing an object file with `rt_cons` — which every AOT-linked
// executable references.
//
// Worth knowing: this is a strong tendency, not a guarantee. rustc merges
// codegen units below a size threshold, so a small module can still land
// beside a big one. Measured on a `(defun main () i32 42)` executable, this
// module stays out and `equality` does not. The only *guarantee* is a separate
// crate — which is why the printer, the one subsystem where it was worth
// 615 KB, is `typelisp-print` rather than a module here.

use crate::{active_heap, decode, encode, fatal};
// ---- The small system builtins ----------------------------------------
//
// Thin edges over [`crate::sys_builtin`], which is the implementation the
// interpreter calls too. Each converts only at the calling convention: a
// compiled `string` arrives tagged, a compiled `i64`/`i32` arrives raw.

/// `(parse-int s)` for compiled code. The result is a freshly allocated
/// `Result` box, as unrooted as [`rt_str_new`]'s until its caller protects
/// it — the same contract every allocating shim here has.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid `i64` encoding a
/// `Value::Str`; a `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_parse_int(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_parse_int: expected 1 argument");
    }
    let s = match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        other => fatal(&format!("parse-int: argument is not a string, got {:?}", other)),
    };
    encode(parse_int(active_heap(), &s))
}

/// `(parse-float s)` for compiled code. Same allocation contract as
/// [`rt_parse_int`].
///
/// # Safety
///
/// Same as [`rt_parse_int`].
#[no_mangle]
pub unsafe extern "C" fn rt_parse_float(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_parse_float: expected 1 argument");
    }
    let s = match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        other => fatal(&format!("parse-float: argument is not a string, got {:?}", other)),
    };
    encode(parse_float(active_heap(), &s))
}

/// `(get-universal-time)` for compiled code — a raw `i64`, not a tagged
/// value: the checker's return type is `i64`, whose compiled representation
/// is the bare machine word.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). Touches no heap.
#[no_mangle]
pub unsafe extern "C" fn rt_get_universal_time(_args: *const i64, _argc: u32) -> i64 {
    get_universal_time()
}

/// `(get-internal-real-time)` for compiled code — a raw `i64`, as
/// [`rt_get_universal_time`].
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). Touches no heap.
#[no_mangle]
pub unsafe extern "C" fn rt_get_internal_real_time(_args: *const i64, _argc: u32) -> i64 {
    get_internal_real_time()
}

/// `(exit code)` for compiled code: terminates the process through the OS
/// and never returns, which is why its typelisp return type is `!`.
///
/// Deliberately *not* an unwind: `exit` is not an error, and running the
/// cleanups an unwind would run (`unwind-protect`, `Drop`) is exactly what
/// CL's own `exit`-alikes do not promise. The interpreted arm calls the same
/// `std::process::exit`.
///
/// # Safety
///
/// `argc` must be `>= 1`; `args[0]` is a raw `i64` exit code. Touches no
/// heap.
#[no_mangle]
pub unsafe extern "C" fn rt_exit(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_exit: expected 1 argument");
    }
    std::process::exit(*args as i32)
}
