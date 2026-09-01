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

use typelisp_mem::{Heap, TypeKeyId, Value};

// The types these results are built with come from `typelisp_mem`'s
// pre-interned table — same standing exception (and same guard test) as
// [`crate::stream_builtin`]: this crate has no `Path` to derive a name from.

/// The runtime type key each of these builtins' results carries — the same
/// table [`crate::stream_builtin::RESULT_KEYS`] keeps, for the same reason
/// (a type's identity includes its instantiation, and this crate has no
/// `Path` to derive one from), checked against the registry by the same
/// guard test.
pub const RESULT_KEYS: &[(&str, &str)] = &[
    ("parse-int", "result<i32,parseinterror>"),
    ("parse-float", "result<f64,parsefloaterror>"),
    ("command-line-args", "vector<string>"),
    ("getenv", "option<string>"),
    ("home-directory", "option<string>"),
];

fn key_of(name: &str) -> &'static str {
    match RESULT_KEYS.iter().find(|(n, _)| *n == name) {
        Some((_, key)) => key,
        None => crate::fatal(&format!("sys_builtin: no result type key for `{}`", name)),
    }
}

/// `Ok(v)`, matching `result_def`'s variant order (`ok` = 0, `err` = 1).
fn result_ok(heap: &mut Heap, name: &str, v: Value) -> Value {
    let key = heap.intern_type_key(key_of(name));
    heap.alloc_enum(key, 0, vec![v])
}

/// `Err(<ErrType>(msg))`: the concrete single-variant error type of the
/// failing builtin, wrapped in `Result`'s `err`. The error types take no type
/// arguments, so their own keys are unchanged by the instantiation rule.
fn result_err(heap: &mut Heap, name: &str, err_type_key: TypeKeyId, msg: String) -> Value {
    let msg_val = heap.alloc_string(msg);
    let err_val = heap.alloc_enum(err_type_key, 0, vec![msg_val]);
    let key = heap.intern_type_key(key_of(name));
    heap.alloc_enum(key, 1, vec![err_val])
}

/// `(parse-int s)`: a decimal `i32` via `str::parse`, `Err` on anything else.
/// The `Ok` payload is an `i32`-typed `Value::Int` — the checker's return
/// type is `Result<i32, ParseIntError>`, so the value must fit `i32` even
/// though the runtime representation is a uniform `i64`.
pub fn parse_int(heap: &mut Heap, s: &str) -> Value {
    match s.parse::<i32>() {
        Ok(n) => result_ok(heap, "parse-int", Value::Int(n as i64)),
        Err(_) => result_err(
            heap,
            "parse-int",
            TypeKeyId::PARSE_INT_ERROR,
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
            result_ok(heap, "parse-float", v)
        }
        Err(_) => result_err(
            heap,
            "parse-float",
            TypeKeyId::PARSE_FLOAT_ERROR,
            format!("parse-float: invalid float literal: {:?}", s),
        ),
    }
}

/// The seconds in a day — the divisor that splits a universal time into a
/// [`universal_time_value`]'s two fields.
const SECS_PER_DAY: i64 = 86_400;

/// The type key of the `universal-time` struct (a prelude `defstruct`, and
/// `get-universal-time`/`file-modified-date`'s registry return type). Spelled
/// here rather than derived because this crate sits below the checker — the
/// same reason `stream_builtin::RESULT_KEYS` spells its keys, and
/// `tests/type_identity_guard_test.rs` checks both against the registry.
pub const UNIVERSAL_TIME_KEY: &str = "universal-time";

/// [`UNIVERSAL_TIME_KEY`]'s monotonic counterpart: the `internal-time` struct
/// `get-internal-real-time` returns.
pub const INTERNAL_TIME_KEY: &str = "internal-time";

/// A universal time (seconds since 1900-01-01 UTC) as the `universal-time`
/// struct: whole days, and seconds within that day.
///
/// The split is what lets a universal time have a type at all — the count
/// passed `i32` in 1968 and this language has no wider fixed-width integer
/// (`types::Type::is_integer`). It is also the division
/// `decode-universal-time` performs first thing, so nothing is spent on it.
/// Floored (`div_euclid`), so a pre-1900 timestamp still has a second-of-day
/// in `0..86400` rather than a negative one.
pub fn universal_time_value(heap: &mut Heap, secs: i64) -> Value {
    let day = secs.div_euclid(SECS_PER_DAY);
    let sec = secs.rem_euclid(SECS_PER_DAY);
    heap.alloc_struct(TypeKeyId::UNIVERSAL_TIME, vec![Value::Int(day), Value::Int(sec)])
}

/// A microsecond count as the `internal-time` struct: whole seconds, and
/// microseconds within that second — [`universal_time_value`]'s counterpart
/// for `get-internal-real-time`, whose raw count overflows `i32` in about 35
/// minutes.
pub fn internal_time_value(heap: &mut Heap, micros: i64) -> Value {
    let sec = micros.div_euclid(1_000_000);
    let usec = micros.rem_euclid(1_000_000);
    heap.alloc_struct(TypeKeyId::INTERNAL_TIME, vec![Value::Int(sec), Value::Int(usec)])
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

/// The process's own command line, or the override `typl` installs.
///
/// The two ways a typelisp program runs disagree about what `std::env::args`
/// means, which is the whole reason this slot exists:
///
/// | how it runs | `std::env::args()` | what the program should see |
/// |---|---|---|
/// | `./prog a b` (AOT) | `["./prog", "a", "b"]` | the same |
/// | `typl s.typl a b` | `["typl", "s.typl", "a", "b"]` | `["s.typl", "a", "b"]` |
///
/// So `typl` calls [`set_command_line_args`] with the script's own view
/// before running anything, and an AOT executable — whose `main` is the LLVM
/// wrapper in `compile::aot::build_main_wrapper`, which takes no `argc`/
/// `argv` — leaves the slot alone and reads its real argv here. Rust's `std`
/// captures argv at process start independently of `main`'s signature
/// (`_NSGetArgv` on macOS, an `.init_array` entry on Linux), so the wrapper's
/// empty parameter list costs nothing.
///
/// Element 0 names the program in both worlds: the script path interpreted,
/// the executable compiled. That is `sb-ext:*posix-argv*`'s convention and
/// Rust's, and it is the only choice that lets one source file be run either
/// way and index its arguments the same.
static COMMAND_LINE_ARGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// Install the command line a typelisp program should see. Called once, by
/// `typl`'s `main`, before any user code runs; ignored if called twice (the
/// first caller wins, and there is only ever one).
pub fn set_command_line_args(args: Vec<String>) {
    let _ = COMMAND_LINE_ARGS.set(args);
}

/// `(command-line-args)`: the command line as a `Vector<string>`.
///
/// Allocating every element before the vector box is deliberate and safe:
/// `alloc_string` and `alloc_struct` never collect — only [`Heap::cons`]
/// does — so the `Vec<Value>` of elements cannot be invalidated between the
/// first string and the box that finally roots them all. A version of this
/// that consed would need each element rooted, which is the shape of the
/// leaks recorded in `checker.rs`'s history.
pub fn command_line_args(heap: &mut Heap) -> Value {
    let args: Vec<String> = match COMMAND_LINE_ARGS.get() {
        Some(installed) => installed.clone(),
        None => std::env::args().collect(),
    };
    let elems: Vec<Value> = args.into_iter().map(|a| heap.alloc_string(a)).collect();
    {
        let key = heap.intern_type_key(key_of("command-line-args"));
        heap.alloc_struct(key, elems)
    }
}

/// `(getenv name)`: the environment variable's value, or `none` when it is
/// unset *or* not valid Unicode. CL has no equivalent at all, so the name is
/// the one every CL implementation's extension uses (`sb-posix:getenv`,
/// `ccl:getenv`).
///
/// The two failure modes collapse into one `none` on purpose: a `string` in
/// this language is Rust's, so a variable holding bytes that are not UTF-8
/// has no value that could be handed back, and "unset" is the only honest
/// answer a `Option<string>` can give.
pub fn getenv(heap: &mut Heap, name: &str) -> Value {
    let v = std::env::var(name).ok().map(|s| heap.alloc_string(s));
    option_value(heap, "getenv", v)
}

/// `(home-directory)`: `$HOME`, or `none` when it is unset — the primitive
/// the prelude's `user-homedir-pathname` (CLHS 25.1) is built on.
///
/// `none` rather than a guess: CL says `user-homedir-pathname` may return
/// `NIL` when the home directory cannot be determined, and inventing `"/"` or
/// the current directory would make an unset `$HOME` indistinguishable from a
/// real one.
pub fn home_directory(heap: &mut Heap) -> Value {
    let v = std::env::var("HOME").ok().map(|s| heap.alloc_string(s));
    option_value(heap, "home-directory", v)
}

/// `Some(v)`/`None` under `option_def`'s variant order (`some` = 0,
/// `none` = 1) — the same shape [`crate::stream_builtin`] builds, spelled
/// again here rather than shared so neither module has to be linked for the
/// other's sake (see this module's note on archive-member granularity).
fn option_value(heap: &mut Heap, name: &str, v: Option<Value>) -> Value {
    let (variant, fields) = match v {
        Some(x) => (0, vec![x]),
        None => (1, vec![]),
    };
    let key = heap.intern_type_key(key_of(name));
    heap.alloc_enum(key, variant, fields)
}

/// `(lisp-implementation-version)` (CLHS 25.1): this build's version, taken
/// from Cargo at compile time so it cannot drift from the package's.
pub fn lisp_implementation_version(heap: &mut Heap) -> Value {
    heap.alloc_string(env!("CARGO_PKG_VERSION").to_string())
}

/// `(machine-type)` (CLHS 25.1): the CPU architecture (`aarch64`, `x86_64`,
/// …) — `std::env::consts::ARCH`, resolved at compile time.
pub fn machine_type(heap: &mut Heap) -> Value {
    heap.alloc_string(std::env::consts::ARCH.to_string())
}

/// `(software-type)` (CLHS 25.1): the operating system (`macos`, `linux`,
/// …) — `std::env::consts::OS`, resolved at compile time.
pub fn software_type(heap: &mut Heap) -> Value {
    heap.alloc_string(std::env::consts::OS.to_string())
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

/// `(get-universal-time)` for compiled code — a tagged value, since the
/// checker's return type is the `universal-time` struct (a heap box), not a
/// number.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_get_universal_time(_args: *const i64, _argc: u32) -> i64 {
    let v = universal_time_value(active_heap(), get_universal_time());
    encode(v)
}

/// `(get-internal-real-time)` for compiled code — a tagged `internal-time`
/// box, as [`rt_get_universal_time`].
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_get_internal_real_time(_args: *const i64, _argc: u32) -> i64 {
    let v = internal_time_value(active_heap(), get_internal_real_time());
    encode(v)
}

/// The one-string argument every filesystem query below takes, decoded at the
/// calling convention's edge: a compiled `string` arrives as a tagged
/// `Value::Str`.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid `i64` encoding a
/// `Value::Str`; a `Heap` must be registered on this thread.
unsafe fn str_arg(args: *const i64, argc: u32, who: &str) -> String {
    if argc < 1 {
        fatal(&format!("{}: expected 1 argument", who));
    }
    match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        other => fatal(&format!("{}: argument is not a string, got {:?}", who, other)),
    }
}

/// `(command-line-args)` for compiled code — a freshly allocated
/// `Vector<string>`, as unrooted as every other allocating shim's result.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_command_line_args(_args: *const i64, _argc: u32) -> i64 {
    encode(command_line_args(active_heap()))
}

/// `(getenv name)` for compiled code.
///
/// # Safety
///
/// Same as [`str_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_getenv(args: *const i64, argc: u32) -> i64 {
    let name = str_arg(args, argc, "rt_getenv");
    encode(getenv(active_heap(), &name))
}

/// `(home-directory)` for compiled code.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_home_directory(_args: *const i64, _argc: u32) -> i64 {
    encode(home_directory(active_heap()))
}

/// `(lisp-implementation-version)` for compiled code.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_lisp_implementation_version(_args: *const i64, _argc: u32) -> i64 {
    encode(lisp_implementation_version(active_heap()))
}

/// `(machine-type)` for compiled code.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_machine_type(_args: *const i64, _argc: u32) -> i64 {
    encode(machine_type(active_heap()))
}

/// `(software-type)` for compiled code.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_software_type(_args: *const i64, _argc: u32) -> i64 {
    encode(software_type(active_heap()))
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
