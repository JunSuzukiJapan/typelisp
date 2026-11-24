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

use typelisp_mem::prelude::*;
use typelisp_mem::{spec, wk, Heap, StdModule, TypeId, TypeSpec, Value};

use crate::result_value::{err_in, ok_in, option_in, TypeTable};

// The types these results are built with come from `typelisp_mem`'s
// pre-interned table — same standing exception (and same guard test) as
// [`crate::stream_builtin`]: this crate has no `Path` to derive a name from.

const PARSE_FLOAT_ERROR: TypeSpec = TypeSpec::Named(StdModule::NUM, "ParseFloatError", &[]);

/// The runtime type each of these builtins' results carries — the same
/// table [`crate::stream_builtin::RESULT_TYPES`] keeps, for the same reason
/// (a type's identity includes its instantiation, and this crate has no
/// checker to ask), checked against the registry by the same guard test.
pub const RESULT_TYPES: TypeTable = &[
    (wk::F_PARSE_FLOAT, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::F64, PARSE_FLOAT_ERROR])),
    (wk::F_MACHINE_VERSION, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
    (wk::F_TIMEZONE_OFFSET_SECONDS, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT])),
    (wk::F_TIMEZONE_DAYLIGHT_P, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::BOOL])),
];

/// `Ok(v)` under `name`'s row ([`crate::result_value`]).
fn result_ok(heap: &mut Heap, name: u32, v: Value) -> Value {
    ok_in(heap, RESULT_TYPES, name, v)
}

/// `Err(<ErrType>(msg))` under `name`'s row.
fn result_err(heap: &mut Heap, name: u32, err_type_key: TypeId, msg: String) -> Value {
    err_in(heap, RESULT_TYPES, name, err_type_key, msg)
}

/// `(parse-float s)`: an `f64` via `str::parse` (which accepts `inf`/`nan`),
/// `Err` on anything else. The `Ok` payload is a boxed float, the one
/// representation an `f64` has once it is a field of something.
pub fn parse_float(heap: &mut Heap, s: &str) -> Value {
    match s.parse::<f64>() {
        Ok(f) => {
            let v = heap.alloc_f64(f);
            result_ok(heap, wk::F_PARSE_FLOAT, v)
        }
        Err(_) => result_err(
            heap,
            wk::F_PARSE_FLOAT,
            TypeId::PARSE_FLOAT_ERROR,
            format!("parse-float: invalid float literal: {:?}", s),
        ),
    }
}

/// The seconds in a day — the divisor that splits a universal time into a
/// [`universal_time_value`]'s two fields.
const SECS_PER_DAY: i64 = 86_400;

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
    heap.alloc_struct(TypeId::UNIVERSAL_TIME, vec![Value::Int(day), Value::Int(sec)])
}

/// A microsecond count as the `internal-time` struct: whole seconds, and
/// microseconds within that second — [`universal_time_value`]'s counterpart
/// for `get-internal-real-time`, whose raw count overflows `i32` in about 35
/// minutes.
pub fn internal_time_value(heap: &mut Heap, micros: i64) -> Value {
    let sec = micros.div_euclid(1_000_000);
    let usec = micros.rem_euclid(1_000_000);
    heap.alloc_struct(TypeId::INTERNAL_TIME, vec![Value::Int(sec), Value::Int(usec)])
}

/// `(get-universal-time)` (CLHS 25.1): seconds since CL's epoch,
/// 1900-01-01 UTC — 2_208_988_800 seconds before the Unix one.
pub fn get_universal_time() -> i64 {
    const UNIX_TO_CL_EPOCH_SECS: i64 = 2_208_988_800;
    // A clock set before 1970 reads as negative Unix seconds, rounded down
    // like every other instant.
    let unix_secs = match typelisp_mem::host::unix_time() {
        Ok(d) => d.as_secs() as i64,
        Err(d) => -(d.as_secs() as i64) - i64::from(d.subsec_nanos() > 0),
    };
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
    static START: typelisp_mem::sync::OnceLock<typelisp_mem::host::Instant> = typelisp_mem::sync::OnceLock::new();
    let start = START.get_or_init(typelisp_mem::host::Instant::now);
    start.elapsed().as_micros() as i64
}

fn option_value(heap: &mut Heap, name: u32, v: Option<Value>) -> Value {
    option_in(heap, RESULT_TYPES, name, v)
}

/// `(lisp-implementation-version)` (CLHS 25.1): this build's version, taken
/// from Cargo at compile time so it cannot drift from the package's.
pub fn lisp_implementation_version(heap: &mut Heap) -> Value {
    heap.alloc_string(env!("CARGO_PKG_VERSION").to_string())
}

/// `(machine-type)` (CLHS 25.1): the CPU architecture (`aarch64`, `x86_64`,
/// …) — `std::env::consts::ARCH`, resolved at compile time.
pub fn machine_type(heap: &mut Heap) -> Value {
    heap.alloc_string(typelisp_mem::host::ARCH.to_string())
}

/// `(software-type)` (CLHS 25.1): the operating system (`macos`, `linux`,
/// …) — `std::env::consts::OS`, resolved at compile time.
pub fn software_type(heap: &mut Heap) -> Value {
    heap.alloc_string(typelisp_mem::host::OS.to_string())
}

// ---- Which OS thread ----------------------------------------------------

/// The next number [`thread_current_id`] hands out. Process-wide: two threads
/// must never share one, whichever heap they run on.
static NEXT_THREAD_ID: typelisp_mem::sync::AtomicCount = typelisp_mem::sync::AtomicCount::new(1);

typelisp_mem::thread_local! {
    /// This thread's number, taken the first time it is asked for.
    static THREAD_ID: i64 = NEXT_THREAD_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed) as i64;
}

/// `(Thread::current-id)`: a number naming the OS thread running the caller,
/// distinct from every other thread's for the life of the process.
///
/// Its own counter rather than `std::thread::ThreadId`, which has no stable
/// way to become a number. The numbers mean nothing beyond "same thread or
/// not" — which is all a program can ask of them, and how a test observes
/// that tasks ran on several threads.
pub fn thread_current_id() -> i64 {
    THREAD_ID.with(|id| *id)
}

/// `(Thread::available-parallelism)`: how many threads this machine can run
/// at once — `std::thread::available_parallelism`, which is also how many
/// threads run tasks when `TYPELISP_THREADS` is not set. The operating
/// system may refuse to say, and that is reported as it is.
pub fn thread_available_parallelism() -> Result<i64, String> {
    typelisp_mem::host::available_parallelism()
        .map(|n| n as i64)
        .map_err(|e| format!("Thread::available-parallelism: the system cannot say ({})", e))
}

// ---- What only the operating system knows -----------------------------
//
// What every target answers through `typelisp_mem::host`; the questions
// only a POSIX system can answer (`machine-instance`, `software-version`,
// `get-internal-run-time`) are `typelisp-rt-posix`'s. Every one may honestly
// answer `None`; CL says so for `machine-version` in as many words ("or nil
// if no such ... can be determined").

/// `(machine-version)` (CLHS 25.1): the hardware this is *running* on
/// (`Apple M1`, `Intel(R) Xeon(R) …`), or `None` where nothing can say.
///
/// Distinct from `machine-type`, which is `typelisp_mem::host::ARCH` — the
/// architecture this binary was *built* for.
pub fn machine_version(heap: &mut Heap) -> Value {
    let v = typelisp_mem::host::machine_version().map(|s| heap.alloc_string(s));
    option_value(heap, wk::F_MACHINE_VERSION, v)
}

/// The seconds a universal time is west of Greenwich in the *local* zone —
/// the primitive under `decode-universal-time`'s and
/// `encode-universal-time`'s no-zone default.
///
/// Takes the universal time already split into `day`/`second`, the two
/// fields of the `universal-time` struct, because the whole count does not
/// fit the `i32` a builtin argument can be. The offset depends on the
/// instant and not just the machine (a host is 5 hours west in January and
/// 4 in July), which is why there is an argument at all.
///
/// Seconds and not CL's hours because the offset is not always a whole
/// number of them — India is +5:30, Nepal +5:45 — and the prelude divides
/// once, into an `f64`, where the fraction survives.
pub fn timezone_offset_seconds(heap: &mut Heap, day: i64, second: i64) -> Value {
    let v = local_zone(day, second).map(|(west, _)| Value::Int(west as i64));
    option_value(heap, wk::F_TIMEZONE_OFFSET_SECONDS, v)
}

/// Whether daylight saving time is in force locally at that universal time —
/// `decode-universal-time`'s `daylight-p`, CL's eighth returned value.
pub fn timezone_daylight_p(heap: &mut Heap, day: i64, second: i64) -> Value {
    let v = local_zone(day, second).map(|(_, dst)| Value::Bool(dst));
    option_value(heap, wk::F_TIMEZONE_DAYLIGHT_P, v)
}

/// `(seconds west, daylight in force)` for a universal time, or `None` when
/// the C library cannot represent that instant. The CL epoch is
/// 2_208_988_800 seconds before the Unix one; `i64` is wide enough that only
/// an absurd `day` overflows, and that arrives as the same `None`.
fn local_zone(day: i64, second: i64) -> Option<(i32, bool)> {
    const UNIX_TO_CL_EPOCH_SECS: i64 = 2_208_988_800;
    let unix = day.checked_mul(SECS_PER_DAY)?.checked_add(second)?.checked_sub(UNIX_TO_CL_EPOCH_SECS)?;
    typelisp_mem::host::timezone_at(unix)
}

/// `(heap-info)`: what the collector knows about itself, as the prelude's
/// `heap-info` struct — the numbers CL's `room` prints, in a form a program
/// can also read.
///
/// Every figure is an `int`. The counts are bounded by the arena (an arena a
/// fixnum cannot count is one this machine cannot hold), so they are written
/// as fixnums directly; the *collection* counter has no such bound — it only
/// goes up — so it goes through `canonical_int`, which boxes it once it
/// outgrows a fixnum rather than wrapping it.
///
/// `growable` rather than the growth ceiling in cells: the ceiling is the one
/// figure here that is not bounded by the arena (it is a multiple of it), and
/// what a reader wants from it is the yes/no.
pub fn heap_info(heap: &mut Heap) -> Value {
    let capacity = heap.capacity();
    let free = heap.free_count();
    let live = heap.live_count();
    let symbols = heap.symbol_count();
    let strings = heap.string_count();
    let boxes = heap.box_count();
    let growable = heap.growth_limit() > capacity;
    let gc_count = heap.canonical_int(i128::from(heap.gc_count()));
    heap.alloc_struct(
        TypeId::HEAP_INFO,
        vec![
            Value::Int(capacity as i64),
            Value::Int(live as i64),
            Value::Int(free as i64),
            Value::Int(symbols as i64),
            Value::Int(strings as i64),
            Value::Int(boxes as i64),
            gc_count,
            Value::Bool(growable),
        ],
    )
}

/// Whether standard input is a terminal.
///
/// The one question `step` has to ask before it does anything: a stepper
/// prompts, and prompting something that is not a person — a piped script,
/// a test harness, `typl file.typl` in CI — would hang forever waiting for a
/// keystroke nobody is there to press. CLHS explicitly allows `step` to
/// simply evaluate its form, and that is what it does when the answer here is
/// `false`.
///
/// `isatty` (through `std`'s `IsTerminal`) and not a guess from an
/// environment variable: this is the actual question.
pub fn stdin_is_tty() -> bool {
    typelisp_mem::host::stdin_is_tty()
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

/// `(parse-float s)` for compiled code. The result is a freshly allocated
/// `Result` box, as unrooted as [`rt_str_new`]'s until its caller protects
/// it — the same contract every allocating shim here has.
///
/// # Safety
///
/// The word must encode a
/// `Value::Str`; a `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_parse_float(s: i64) -> i64 {
    let s = match decode(s) {
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
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_get_universal_time() -> i64 {
    let v = universal_time_value(active_heap(), get_universal_time());
    encode(v)
}

/// `(get-internal-real-time)` for compiled code — a tagged `internal-time`
/// box, as [`rt_get_universal_time`].
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_get_internal_real_time() -> i64 {
    let v = internal_time_value(active_heap(), get_internal_real_time());
    encode(v)
}

/// `(sleep secs)` — block this thread for `secs` seconds.
///
/// Seconds, as CL's `sleep` takes them, and `f64` because that is the type
/// a fractional wait has to be written in: an integer literal does not
/// become a float here (`(sleep 1)` is a type error, `(sleep 1.0)` is not),
/// the same rule Rust has.
///
/// A negative or NaN wait is an error in CL. Here it is nothing at all —
/// there is no duration to construct from it, and `Duration::from_secs_f64`
/// panics on one. Zero is a real answer (yield-ish) and goes through.
pub fn sleep(heap: &mut Heap, secs: f64) -> Result<(), String> {
    if !(secs >= 0.0) {
        return Err(format!("sleep: {} is not a non-negative number of seconds", secs));
    }
    heap.native(|| typelisp_mem::host::sleep(core::time::Duration::from_secs_f64(secs)));
    Ok(())
}

/// `(lisp-implementation-version)` for compiled code.
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_lisp_implementation_version() -> i64 {
    encode(lisp_implementation_version(active_heap()))
}

/// `(machine-type)` for compiled code.
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_machine_type() -> i64 {
    encode(machine_type(active_heap()))
}

/// `(software-type)` for compiled code.
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_software_type() -> i64 {
    encode(software_type(active_heap()))
}



/// `(machine-version)` for compiled code.
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_machine_version() -> i64 {
    encode(machine_version(active_heap()))
}


/// `(timezone-offset-seconds day second)` for compiled code. Both arguments
/// are `i32`s, so both arrive as bare machine words.
///
/// # Safety
///
/// The words must be valid; a `Heap`
/// must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_timezone_offset_seconds(day: i64, second: i64) -> i64 {
    encode(timezone_offset_seconds(active_heap(), day, second))
}

/// `(timezone-daylight-p day second)` for compiled code.
///
/// # Safety
///
/// Same as [`rt_timezone_offset_seconds`].
#[no_mangle]
pub unsafe extern "C" fn rt_timezone_daylight_p(day: i64, second: i64) -> i64 {
    encode(timezone_daylight_p(active_heap(), day, second))
}

/// `(heap-info)` for compiled code — a freshly allocated `heap-info` struct,
/// as unrooted as every other allocating shim's result until its caller
/// protects it.
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_heap_info() -> i64 {
    encode(heap_info(active_heap()))
}

/// `(Thread::current-id)` for compiled code — a raw `int`.
///
/// # Safety
///
/// None: the builtin is nullary.
#[no_mangle]
pub unsafe extern "C" fn rt_thread_current_id() -> i64 {
    thread_current_id()
}

/// `(Thread::available-parallelism)` for compiled code — a raw `int`, or a
/// panic the program can catch when the system cannot say.
///
/// # Safety
///
/// None: the builtin is nullary.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_thread_available_parallelism() -> i64 {
    match thread_available_parallelism() {
        Ok(n) => n,
        Err(msg) => typelisp_abi::raise(msg).into_word(),
    }
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
/// `x` is a raw `i64` exit code. Touches no
/// heap.
#[no_mangle]
pub unsafe extern "C" fn rt_exit(x: i64) -> i64 {
    typelisp_mem::host::exit(x as i32)
}
