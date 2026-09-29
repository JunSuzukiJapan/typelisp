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
    ("parse-float", "result<f64,parsefloaterror>"),
    ("command-line-args", "vector<string>"),
    ("getenv", "option<string>"),
    ("home-directory", "option<string>"),
    ("machine-instance", "option<string>"),
    ("machine-version", "option<string>"),
    ("software-version", "option<string>"),
    ("timezone-offset-seconds", "option<int>"),
    ("timezone-daylight-p", "option<bool>"),
    ("dribble-start", "result<(),fileerror>"),
    ("dribble-stop", "result<(),fileerror>"),
    ("ed-open", "result<(),fileerror>"),
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

/// `(parse-float s)`: an `f64` via `str::parse` (which accepts `inf`/`nan`),
/// `Err` on anything else. The `Ok` payload is a boxed float, the one
/// representation an `f64` has once it is a field of something.
pub fn parse_float(heap: &mut Heap, s: &str) -> Value {
    match s.parse::<f64>() {
        Ok(f) => {
            let v = heap.alloc_f64(f);
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
    // A clock set before 1970 reads as negative Unix seconds, rounded down
    // like every other instant.
    let unix_secs = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(before) => {
            let d = before.duration();
            -(d.as_secs() as i64) - i64::from(d.subsec_nanos() > 0)
        }
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

/// `Some(v)`/`None` under the instantiation `name`'s key spells — the
/// niche when the payload allows one, else a box (`Heap::alloc_option`
/// decides). Spelled again here rather than shared with
/// [`crate::stream_builtin`] so neither module has to be linked for the
/// other's sake (see this module's note on archive-member granularity).
fn option_value(heap: &mut Heap, name: &str, v: Option<Value>) -> Value {
    heap.alloc_option(key_of(name), v)
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

// ---- Which OS thread ----------------------------------------------------

/// The next number [`thread_current_id`] hands out. Process-wide: two threads
/// must never share one, whichever heap they run on.
static NEXT_THREAD_ID: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);

thread_local! {
    /// This thread's number, taken the first time it is asked for.
    static THREAD_ID: i64 = NEXT_THREAD_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
    std::thread::available_parallelism()
        .map(|n| n.get() as i64)
        .map_err(|e| format!("Thread::available-parallelism: the system cannot say ({})", e))
}

// ---- What only the operating system knows -----------------------------
//
// The four below are the ones `std` has no equivalent of, so each is one
// call into [`crate::os`] — the single module that owns this workspace's
// direct C calls. Every one may honestly answer `none`; CL says so for
// `machine-instance`, `machine-version` and `software-version` in as many
// words ("or nil if no such ... can be determined").

/// `(get-internal-run-time)` (CLHS 25.1): CPU time this process has used,
/// user plus system, in the same microseconds `get-internal-real-time`
/// counts.
///
/// The pair is the point: real time says how long you waited, run time says
/// how long the CPU worked, and a program that is mostly blocked on I/O
/// shows a large difference. Substituting one for the other — which is what
/// this returned before there was a `getrusage` to call — is a lie precisely
/// when the number is interesting.
pub fn get_internal_run_time() -> i64 {
    match crate::os::run_time_micros() {
        Some(us) => us,
        // Unreachable: `getrusage(RUSAGE_SELF, &valid)` has no failure mode.
        // Reported rather than papered over, because a zero here would read
        // as "this program used no CPU".
        None => fatal("get-internal-run-time: getrusage(RUSAGE_SELF) failed"),
    }
}

/// `(machine-instance)` (CLHS 25.1): the host's name, or `none`.
pub fn machine_instance(heap: &mut Heap) -> Value {
    let v = crate::os::host_name().map(|s| heap.alloc_string(s));
    option_value(heap, "machine-instance", v)
}

/// `(machine-version)` (CLHS 25.1): the hardware this is *running* on
/// (`Apple M1`, `Intel(R) Xeon(R) …`), or `none` where nothing can say.
///
/// Distinct from `machine-type`, which is `std::env::consts::ARCH` — the
/// architecture this binary was *built* for.
pub fn machine_version(heap: &mut Heap) -> Value {
    let v = crate::os::machine_version().map(|s| heap.alloc_string(s));
    option_value(heap, "machine-version", v)
}

/// `(software-version)` (CLHS 25.1): the OS release string (`uname -r`,
/// e.g. `24.6.0`), or `none`. Its companion `software-type` is a
/// compile-time constant and needs no call.
pub fn software_version(heap: &mut Heap) -> Value {
    let v = crate::os::os_release().map(|s| heap.alloc_string(s));
    option_value(heap, "software-version", v)
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
    option_value(heap, "timezone-offset-seconds", v)
}

/// Whether daylight saving time is in force locally at that universal time —
/// `decode-universal-time`'s `daylight-p`, CL's eighth returned value.
pub fn timezone_daylight_p(heap: &mut Heap, day: i64, second: i64) -> Value {
    let v = local_zone(day, second).map(|(_, dst)| Value::Bool(dst));
    option_value(heap, "timezone-daylight-p", v)
}

/// `(seconds west, daylight in force)` for a universal time, or `None` when
/// the C library cannot represent that instant. The CL epoch is
/// 2_208_988_800 seconds before the Unix one; `i64` is wide enough that only
/// an absurd `day` overflows, and that arrives as the same `None`.
fn local_zone(day: i64, second: i64) -> Option<(i32, bool)> {
    const UNIX_TO_CL_EPOCH_SECS: i64 = 2_208_988_800;
    let unix = day.checked_mul(SECS_PER_DAY)?.checked_add(second)?.checked_sub(UNIX_TO_CL_EPOCH_SECS)?;
    crate::os::timezone_at(unix)
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
        TypeKeyId::HEAP_INFO,
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

/// `(dribble-start path)`: begin copying the session into `path`, truncating
/// it. The recording itself lives in [`typelisp_abi::dribble`] — see that
/// module's docs for why it is a crate below this one.
pub fn dribble_start(heap: &mut Heap, path: &str) -> Value {
    match typelisp_abi::dribble::start(path) {
        Ok(()) => result_ok(heap, "dribble-start", Value::Empty),
        Err(e) => result_err(heap, "dribble-start", TypeKeyId::FILE_ERROR, format!("dribble: {}: {}", path, e)),
    }
}

/// `(dribble-stop)`: close the dribble file. A no-op when none is open, as
/// CL's argument-less `dribble` is.
///
/// `Result`, not `()`, because the close *flushes*: a full disk is discovered
/// here or nowhere, and discovering it nowhere would mean a truncated
/// transcript reported as a complete one.
pub fn dribble_stop(heap: &mut Heap) -> Value {
    match typelisp_abi::dribble::stop() {
        Ok(()) => result_ok(heap, "dribble-stop", Value::Empty),
        Err(e) => result_err(heap, "dribble-stop", TypeKeyId::FILE_ERROR, format!("dribble: {}", e)),
    }
}

/// `(ed-open path line)`: run the user's editor on `path`, positioned at
/// `line` — the runtime half of the `ed` special form, which did the *name*
/// resolution at check time and reduced to this call.
///
/// `$VISUAL` first, then `$EDITOR`: that is the order every Unix tool uses,
/// and it is the one the two variables were invented to express (`VISUAL` for
/// a full-screen editor, `EDITOR` for whatever works on a teletype). Neither
/// set is an `Err`, not a guess at `vi` — inventing an editor is how `ed`
/// ends up launching something the user cannot exit.
///
/// The variable's value is split on whitespace so `EDITOR="code -w"` and
/// `EDITOR="emacsclient -nw"` work; the first word is the program and the
/// rest are leading arguments. This is the same treatment `git` gives it
/// short of running a shell, and running a shell would make the value a
/// command injection surface for no benefit.
///
/// `+N` is the line argument, understood by vi, emacs, nano, ed, joe and
/// `code --goto`'s fallback. It is omitted for line 0, which is what the
/// `ed`-with-a-path and `ed`-with-nothing forms produce.
///
/// Waits for the editor to exit (CL's `ed` returns when editing is done), and
/// reports a non-zero exit rather than swallowing it.
pub fn ed_open(heap: &mut Heap, path: &str, line: i64) -> Value {
    let err = |heap: &mut Heap, msg: String| result_err(heap, "ed-open", TypeKeyId::FILE_ERROR, msg);
    let spec = match std::env::var("VISUAL").ok().filter(|s| !s.trim().is_empty()) {
        Some(v) => v,
        None => match std::env::var("EDITOR").ok().filter(|s| !s.trim().is_empty()) {
            Some(v) => v,
            None => return err(heap, "ed: neither $VISUAL nor $EDITOR is set".to_string()),
        },
    };
    let mut words = spec.split_whitespace();
    let program = match words.next() {
        Some(p) => p,
        None => return err(heap, "ed: the editor variable is empty".to_string()),
    };
    let mut cmd = std::process::Command::new(program);
    cmd.args(words);
    if !path.is_empty() {
        if line > 0 {
            cmd.arg(format!("+{}", line));
        }
        cmd.arg(path);
    }
    // Native for as long as the editor runs.
    match heap.native(|| cmd.status()) {
        Ok(st) if st.success() => result_ok(heap, "ed-open", Value::Empty),
        Ok(st) => err(heap, format!("ed: {} exited with {}", program, st)),
        Err(e) => err(heap, format!("ed: cannot run {}: {}", program, e)),
    }
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
/// `isatty` and not a guess from an environment variable: this is the actual
/// question, and `libc` is already a dependency of this crate.
pub fn stdin_is_tty() -> bool {
    // SAFETY: `isatty` reads a file descriptor's type and touches no memory.
    // 0 is standard input, which every process has (or does not, in which case
    // `isatty` answers 0 — the same answer, for the same reason).
    unsafe { libc::isatty(0) == 1 }
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
/// `argc` must be `>= 1` and `args` must point to a valid `i64` encoding a
/// `Value::Str`; a `Heap` must be registered on this thread.
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
    heap.native(|| std::thread::sleep(std::time::Duration::from_secs_f64(secs)));
    Ok(())
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

/// `(get-internal-run-time)` for compiled code — a tagged `internal-time`
/// struct, like [`rt_get_internal_real_time`].
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_get_internal_run_time(_args: *const i64, _argc: u32) -> i64 {
    let v = internal_time_value(active_heap(), get_internal_run_time());
    encode(v)
}

/// `(machine-instance)` for compiled code.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_machine_instance(_args: *const i64, _argc: u32) -> i64 {
    encode(machine_instance(active_heap()))
}

/// `(machine-version)` for compiled code.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_machine_version(_args: *const i64, _argc: u32) -> i64 {
    encode(machine_version(active_heap()))
}

/// `(software-version)` for compiled code.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_software_version(_args: *const i64, _argc: u32) -> i64 {
    encode(software_version(active_heap()))
}

/// `(timezone-offset-seconds day second)` for compiled code. Both arguments
/// are `i32`s, so both arrive as bare machine words.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to two valid `i64`s; a `Heap`
/// must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_timezone_offset_seconds(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_timezone_offset_seconds: expected 2 arguments");
    }
    encode(timezone_offset_seconds(active_heap(), *args, *args.add(1)))
}

/// `(timezone-daylight-p day second)` for compiled code.
///
/// # Safety
///
/// Same as [`rt_timezone_offset_seconds`].
#[no_mangle]
pub unsafe extern "C" fn rt_timezone_daylight_p(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_timezone_daylight_p: expected 2 arguments");
    }
    encode(timezone_daylight_p(active_heap(), *args, *args.add(1)))
}

/// `(heap-info)` for compiled code — a freshly allocated `heap-info` struct,
/// as unrooted as every other allocating shim's result until its caller
/// protects it.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_heap_info(_args: *const i64, _argc: u32) -> i64 {
    encode(heap_info(active_heap()))
}

/// `(Thread::current-id)` for compiled code — a raw `int`.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary).
#[no_mangle]
pub unsafe extern "C" fn rt_thread_current_id(_args: *const i64, _argc: u32) -> i64 {
    thread_current_id()
}

/// `(Thread::available-parallelism)` for compiled code — a raw `int`, or a
/// panic the program can catch when the system cannot say.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary).
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_thread_available_parallelism(_args: *const i64, _argc: u32) -> i64 {
    match thread_available_parallelism() {
        Ok(n) => n,
        Err(msg) => typelisp_abi::raise(msg),
    }
}

/// `(dribble-start path)` for compiled code.
///
/// # Safety
///
/// Same as [`str_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_dribble_start(args: *const i64, argc: u32) -> i64 {
    let path = str_arg(args, argc, "rt_dribble_start");
    encode(dribble_start(active_heap(), &path))
}

/// `(dribble-stop)` for compiled code.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_dribble_stop(_args: *const i64, _argc: u32) -> i64 {
    encode(dribble_stop(active_heap()))
}

/// `(ed-open path line)` for compiled code: `args[0]` is a tagged `string`,
/// `args[1]` a raw `i32` line number.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to 2 valid `i64`s, the first
/// encoding a `Value::Str`; a `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_ed_open(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_ed_open: expected 2 arguments");
    }
    let path = match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        other => fatal(&format!("ed-open: argument is not a string, got {:?}", other)),
    };
    let line = *args.add(1);
    encode(ed_open(active_heap(), &path, line))
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
