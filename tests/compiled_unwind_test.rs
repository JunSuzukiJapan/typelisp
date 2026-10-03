//! What the unwinder will and will not do for LLVM-generated frames — the
//! ground the compiled-`panic` work and any future `unwind-protect` stand on.
//!
//! `typelisp_rt::rt_panic` used to call `std::process::abort()`, because a Rust
//! `panic!` crossing back through JIT/AOT frames over an `extern "C"` boundary
//! is undefined behavior. Rust 1.71 stabilized `extern "C-unwind"` for exactly
//! that case: a panic may cross, provided every frame in between is unwindable.
//! Whether *these* frames are was not something the codebase could answer — it
//! had no `personality`, `uwtable`, `invoke` or `landingpad` anywhere — so
//! these probes build the smallest modules that settle it, at two levels.
//!
//! **Level 1 — unwinding *through* a compiled frame**, which is all a catchable
//! `panic` needs:
//!
//! 1. A panic does cross a JIT frame and reach `catch_unwind`.
//! 2. **No `uwtable` attribute is needed**, so `compiler.rs`'s
//!    `compile-function` and the island artifact are untouched.
//! 3. **The code must outlive the unwind.** Freeing it while the panic is
//!    still propagating through it can leave the unwinder unable to finish the
//!    *next* unwind (it hangs) — which is why [`typelisp::compile::CompiledFn`]
//!    retires its code rather than dropping it. Freeing it *after* the unwind
//!    is fine, and [`freed_code_does_not_break_the_next_unwind`] checks that.
//!
//! **Level 2 — running cleanup *inside* a compiled frame**, which is what an
//! `unwind-protect` needs and level 1 does not provide:
//!
//! 4. An `invoke` with a cleanup-only `landingpad` runs its cleanup while the
//!    panic is in flight, `resume`s, and the panic still reaches the catcher —
//!    both halves, since cleanup that swallowed the unwind would be a `catch`
//!    instead.
//! 5. **The personality routine has to be `rust_eh_personality`.**
//!    `__gxx_personality_v0` works under JIT but does not link into an AOT
//!    executable; see [`protected_probe`].
//!
//! Deliberately built with raw inkwell rather than through the typelisp front
//! end: the point is to isolate the LLVM/JIT/unwinder interaction from every
//! other moving part, so a failure names the mechanism rather than the language.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use inkwell::module::Linkage;
use inkwell::values::ValueKind;
use inkwell::AddressSpace;

use typelisp::compile::{jit_engine, llvm_context, COMPILE_LOCK};

// std's personality routine, by its symbol: the address the `protected`
// probe's declaration of it is bound to. Never called from Rust.
extern "C" {
    fn rust_eh_personality();
}

/// The name a declaration was added under, as [`jit_engine`] takes it.
fn callee_name(function: inkwell::values::FunctionValue<'static>) -> String {
    function.get_name().to_string_lossy().into_owned()
}

/// The message [`typelisp_test_maybe_panic`] panics with, asserted on the
/// caught payload so a test proves it caught *this* panic and not another.
const PANIC_MESSAGE: &str = "compiled-unwind probe";

/// The JIT-compiled probe's Rust-side type.
///
/// `extern "C-unwind"`, not the `extern "C"` of
/// [`typelisp::compile::CompiledSignature`]: calling through a plain
/// `extern "C"` pointer tells Rust the callee cannot unwind, which is exactly
/// the assumption being broken here. Note this is also why the real fix cannot
/// call through an `extern "C"` type — the address is transmuted to this.
type UnwindingProbe = unsafe extern "C-unwind" fn(*const i64, u32) -> i64;

/// Stands in for `typelisp_rt::rt_panic`: an `extern "C-unwind"` function the
/// JIT-compiled code calls, which panics rather than returning.
///
/// `extern "C-unwind"` rather than `extern "C"` is the entire subject of the
/// probe — under plain `extern "C"` this panic is defined to abort the process
/// (the Rustonomicon's "FFI and unwinding"), which is what happens today.
///
/// # Safety
///
/// Called from JIT-compiled code under the `(i64) -> i64` signature declared
/// for it in the module below. It never returns normally when `n == 0`.
#[no_mangle]
pub extern "C-unwind" fn typelisp_test_maybe_panic(n: i64) -> i64 {
    if n == 0 {
        panic!("{}", PANIC_MESSAGE);
    }
    n * 2
}

/// The probe's JIT-resolved address, built once and kept for the life of the
/// process — as a `CompiledFn` keeps its code for as long as anything may be
/// running in it. [`freed_code_does_not_break_the_next_unwind`] builds its own.
static PROBE: OnceLock<usize> = OnceLock::new();

/// Builds the module of `probe(args, argc) -> i64`, whose body is
/// `typelisp_test_maybe_panic(args[0])`, and the externals it links against.
///
/// Mirrors `CompiledFn::new`'s setup — same `(i64*, u32) -> i64` ABI, the
/// external's address passed the same way — so a result here carries over to
/// the real compile path. No `uwtable` attribute is set: that it works without
/// one is part of what the probe establishes.
///
/// The caller must hold `COMPILE_LOCK`, and drop the module under it.
fn build_probe() -> (inkwell::module::Module<'static>, Vec<(String, usize)>) {
    let ctx = llvm_context();
    let module = ctx.create_module("unwind_probe");
    let i64_t = ctx.i64_type();
    let i32_t = ctx.i32_type();

    // The callee, declared with no body and given the real Rust function's
    // address below — how an already-compiled external is linked in.
    let callee_ty = i64_t.fn_type(&[i64_t.into()], false);
    let callee = module.add_function("typelisp_test_maybe_panic", callee_ty, Some(Linkage::External));

    let probe_ty = i64_t.fn_type(&[ctx.ptr_type(AddressSpace::default()).into(), i32_t.into()], false);
    let probe = module.add_function("probe", probe_ty, None);

    let builder = ctx.create_builder();
    builder.position_at_end(ctx.append_basic_block(probe, "entry"));
    let args_ptr = probe.get_nth_param(0).unwrap().into_pointer_value();
    let first = builder.build_load(i64_t, args_ptr, "arg0").unwrap().into_int_value();
    let called = builder.build_call(callee, &[first.into()], "call").unwrap();
    let result = match called.try_as_basic_value() {
        ValueKind::Basic(v) => v.into_int_value(),
        ValueKind::Instruction(_) => panic!("the callee declaration produced no value"),
    };
    builder.build_return(Some(&result)).unwrap();
    module.verify().expect("probe module failed verification");

    let externals = vec![(callee_name(callee), typelisp_test_maybe_panic as *const () as usize)];
    (module, externals)
}

fn probe() -> UnwindingProbe {
    let addr = *PROBE.get_or_init(|| {
        // The whole-process LLVM context is shared and not thread-safe; every
        // other LLVM-driving test takes this lock for the same reason.
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (module, externals) = build_probe();
        let code = jit_engine(&module, &externals).expect("failed to JIT-compile the probe");
        let addr = code.address("probe").expect("probe did not resolve");
        // Never dropped: see this static's doc comment.
        std::mem::forget(code);
        addr
    });
    unsafe { std::mem::transmute::<usize, UnwindingProbe>(addr) }
}

/// Calls the probe with one argument. A panic raised inside propagates out.
fn call_probe(arg: i64) -> i64 {
    let f = probe();
    let argv = [arg];
    unsafe { f(argv.as_ptr(), 1) }
}

/// Runs `body` with the panic hook silenced, so an *expected* panic does not
/// print a backtrace and make a passing run look like a failing one.
fn without_panic_output<T>(body: impl FnOnce() -> T) -> T {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = body();
    std::panic::set_hook(previous);
    out
}

/// Catches one panic out of the probe and returns its message.
fn caught_message(arg: i64) -> String {
    let payload = without_panic_output(|| catch_unwind(AssertUnwindSafe(|| call_probe(arg))))
        .expect_err("the panic did not propagate out of the JIT frame");
    payload
        .downcast_ref::<String>()
        .map(String::to_owned)
        .expect("panic payload was not the String this probe panics with")
}

/// The baseline: the call path works at all, and a non-panicking argument
/// returns through the JIT frame normally.
#[test]
fn the_probe_returns_normally_when_the_callee_does_not_panic() {
    assert_eq!(call_probe(21), 42);
}

/// Phase A, level 1: a Rust panic raised behind an `extern "C-unwind"`
/// boundary unwinds *through* the JIT-compiled frame and is caught on the
/// other side, instead of aborting the process.
///
/// A regression here aborts the test binary rather than reporting a failure —
/// that is the current `rt_panic` behavior, and telling "caught" apart from
/// "aborted" is the point.
#[test]
fn a_panic_crosses_a_jit_frame_when_the_callee_is_c_unwind() {
    assert_eq!(caught_message(0), PANIC_MESSAGE);
}

/// Unwinding through JIT'd code has to stay repeatable, because a REPL
/// session will do it over and over.
#[test]
fn a_second_panic_still_unwinds() {
    assert_eq!(caught_message(0), PANIC_MESSAGE);
    assert_eq!(caught_message(0), PANIC_MESSAGE);
    // ...and the frame is still usable for ordinary calls afterwards.
    assert_eq!(call_probe(50), 100);
}

// ---- Phase A level 2: running cleanup *inside* a compiled frame ------------
//
// Level 1 above only needs the unwinder to *walk past* a compiled frame. An
// `unwind-protect` needs more: the frame has to catch the in-flight unwind,
// run its cleanup forms, and resume — which in LLVM means the call becomes an
// `invoke` with a `landingpad` on its unwind edge, and the function needs a
// personality routine. None of that exists in `compiler.rs` today, so these
// tests establish whether it can before any of it is designed.

/// Set by [`typelisp_test_cleanup_ran`], so a test can tell whether the
/// landing pad actually executed rather than being merely emitted.
static CLEANUP_RUNS: AtomicUsize = AtomicUsize::new(0);

/// Stands in for an `unwind-protect`'s cleanup form: something observable
/// that the landing pad calls while the panic is in flight.
///
/// # Safety
///
/// Called from a landing pad with no arguments; touches nothing but its own
/// counter.
#[no_mangle]
pub extern "C-unwind" fn typelisp_test_cleanup_ran() {
    CLEANUP_RUNS.fetch_add(1, Ordering::SeqCst);
}

/// The level-2 probe's address, built once and kept, like [`PROBE`].
static PROTECTED_PROBE: OnceLock<usize> = OnceLock::new();

/// Builds `protected(args, argc) -> i64`: the same call as [`probe`], but
/// through an `invoke` whose unwind edge lands in a cleanup-only
/// `landingpad` that calls [`typelisp_test_cleanup_ran`] and then `resume`s.
///
/// That shape *is* `unwind-protect` in miniature: run a form, and whether it
/// completes or unwinds, run the cleanup — with the unwind continuing
/// afterwards rather than being swallowed.
///
/// The personality routine is **`rust_eh_personality`**, and which one it is
/// turned out to matter. `__gxx_personality_v0` (the Itanium C++ one) also
/// works under JIT — a Rust panic is a foreign exception to it, and a
/// cleanup-only clause runs for those — but it is *not linkable into an AOT
/// executable*: plain `cc` on macOS leaves it undefined, since it lives in
/// libc++abi and only the JIT process gets that for free by way of LLVM.
/// `rust_eh_personality` is defined in `libtypelisp_front.a`, which every AOT
/// executable already links, so it is the one personality available on both
/// paths.
fn protected_probe() -> UnwindingProbe {
    let addr = *PROTECTED_PROBE.get_or_init(|| {
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = llvm_context();
        let module = ctx.create_module("unwind_protect_probe");
        let i64_t = ctx.i64_type();
        let i32_t = ctx.i32_type();
        let ptr_t = ctx.ptr_type(AddressSpace::default());

        let callee = module.add_function("typelisp_test_maybe_panic", i64_t.fn_type(&[i64_t.into()], false), Some(Linkage::External));
        let cleanup_fn =
            module.add_function("typelisp_test_cleanup_ran", ctx.void_type().fn_type(&[], false), Some(Linkage::External));
        // `int (...)` — the personality's signature is never actually called
        // by generated code, only recorded on the function.
        let personality = module.add_function("rust_eh_personality", i32_t.fn_type(&[], true), Some(Linkage::External));

        let protected = module.add_function("protected", i64_t.fn_type(&[ptr_t.into(), i32_t.into()], false), None);
        protected.set_personality_function(personality);

        let entry = ctx.append_basic_block(protected, "entry");
        let normal = ctx.append_basic_block(protected, "normal");
        let cleanup = ctx.append_basic_block(protected, "cleanup");

        let builder = ctx.create_builder();
        builder.position_at_end(entry);
        let args_ptr = protected.get_nth_param(0).unwrap().into_pointer_value();
        let first = builder.build_load(i64_t, args_ptr, "arg0").unwrap().into_int_value();
        let invoked = builder.build_invoke(callee, &[first.into()], normal, cleanup, "invoke").unwrap();

        builder.position_at_end(normal);
        let result = match invoked.try_as_basic_value() {
            ValueKind::Basic(v) => v.into_int_value(),
            ValueKind::Instruction(_) => panic!("the callee declaration produced no value"),
        };
        builder.build_return(Some(&result)).unwrap();

        // Cleanup-only: no catch clauses, so the personality reports "cleanup"
        // and the `resume` hands the exception back to the unwinder.
        builder.position_at_end(cleanup);
        let exception_type = ctx.struct_type(&[ptr_t.into(), i32_t.into()], false);
        let pad = builder.build_landing_pad(exception_type, personality, &[], true, "pad").unwrap();
        builder.build_call(cleanup_fn, &[], "cleanup_call").unwrap();
        builder.build_resume(pad).unwrap();

        module.verify().expect("protected module failed verification");

        // The personality too: it is a declaration like the other two. The
        // `_Unwind_Resume` that `resume` becomes is not named here — code
        // generation adds that call, and the JIT supplies it.
        let externals = [
            (callee_name(callee), typelisp_test_maybe_panic as *const () as usize),
            (callee_name(cleanup_fn), typelisp_test_cleanup_ran as *const () as usize),
            (callee_name(personality), rust_eh_personality as *const () as usize),
        ];
        let code = jit_engine(&module, &externals).expect("failed to JIT-compile the protected probe");
        let addr = code.address("protected").expect("protected did not resolve");
        std::mem::forget(code);
        addr
    });
    unsafe { std::mem::transmute::<usize, UnwindingProbe>(addr) }
}

fn call_protected(arg: i64) -> i64 {
    let f = protected_probe();
    let argv = [arg];
    unsafe { f(argv.as_ptr(), 1) }
}

/// The normal path through an `invoke`: no unwind, so the landing pad does
/// not run and the value comes back as usual.
#[test]
fn a_protected_frame_returns_normally_and_skips_its_cleanup() {
    let before = CLEANUP_RUNS.load(Ordering::SeqCst);
    assert_eq!(call_protected(21), 42);
    assert_eq!(CLEANUP_RUNS.load(Ordering::SeqCst), before, "cleanup ran on the normal path");
}

/// Phase A, level 2: a compiled frame runs its cleanup while a panic unwinds
/// through it, and the panic still reaches the catcher afterwards.
///
/// Both halves matter. Cleanup that runs but swallows the unwind would turn
/// `(unwind-protect body cleanup)` into a `catch`; an unwind that reaches the
/// catcher without running cleanup would leak whatever the cleanup was there
/// to release.
#[test]
fn a_protected_frame_runs_its_cleanup_while_a_panic_unwinds_through() {
    let before = CLEANUP_RUNS.load(Ordering::SeqCst);
    let payload = without_panic_output(|| catch_unwind(AssertUnwindSafe(|| call_protected(0))))
        .expect_err("the panic did not propagate past the landing pad");
    assert_eq!(CLEANUP_RUNS.load(Ordering::SeqCst), before + 1, "the landing pad did not run the cleanup");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .expect("panic payload was not the String this probe panics with");
    assert_eq!(message, PANIC_MESSAGE);
}

/// Repeatable, for the same reason level 1 had to be: a session runs
/// `unwind-protect` over and over.
#[test]
fn a_protected_frame_is_reusable_after_an_unwind() {
    let before = CLEANUP_RUNS.load(Ordering::SeqCst);
    for _ in 0..3 {
        let _ = without_panic_output(|| catch_unwind(AssertUnwindSafe(|| call_protected(0)))).expect_err("expected a panic");
    }
    assert_eq!(CLEANUP_RUNS.load(Ordering::SeqCst), before + 3);
    assert_eq!(call_protected(50), 100);
}

// ---- what the JIT links against ---------------------------------------------

/// Freeing JIT'd code after a panic has unwound through it leaves the next
/// unwind working. A `CompiledFn` frees its code only through
/// `compile::retire_llvm`, when no compiled chain stands, which is this case.
///
/// A hang is the failure this guards against, so the second unwind runs on a
/// thread of its own and a hang fails the test instead of stopping it.
#[test]
fn freed_code_does_not_break_the_next_unwind() {
    let compile = || {
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (module, externals) = build_probe();
        let code = jit_engine(&module, &externals).expect("failed to JIT-compile the probe");
        let f = unsafe { std::mem::transmute::<usize, UnwindingProbe>(code.address("probe").expect("probe did not resolve")) };
        (code, f)
    };
    let unwind = |f: UnwindingProbe| {
        without_panic_output(|| catch_unwind(AssertUnwindSafe(|| unsafe { f([0i64].as_ptr(), 1) })))
            .expect_err("the panic did not propagate out of the JIT frame")
            .downcast_ref::<String>()
            .cloned()
            .expect("panic payload was not the String this probe panics with")
    };

    let (first, f) = compile();
    assert_eq!(unwind(f), PANIC_MESSAGE);
    {
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drop(first);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (second, g) = compile();
        let _ = tx.send(unwind(g));
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drop(second);
    });
    let message = rx.recv_timeout(std::time::Duration::from_secs(60)).expect("the second unwind hung or died");
    assert_eq!(message, PANIC_MESSAGE);
}

/// A name the code calls that nobody supplied is an error naming it, on every
/// OS — not a call to whatever the process happens to have under that name,
/// nor to address 0.
#[test]
fn a_callee_nobody_supplied_is_an_error_naming_it() {
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (module, _) = build_probe();
    let err = jit_engine(&module, &[]).err().expect("a callee with no address was accepted");
    assert!(err.contains("typelisp_test_maybe_panic"), "the error does not name the symbol: {}", err);
}

/// How many functions [`a_panic_crosses_any_function_of_a_large_module`]
/// puts in its module: enough that one table page of unwind records could not
/// hold them all, not only enough to have more than one.
const MANY: usize = 1200;

/// A panic unwinds through every function of a module, not only the first.
///
/// One-function modules — every other probe here — cannot tell. On Mach-O,
/// LLVM 22.1.8's JITLink ends a module's unwind table early when its last
/// functions' unwind encodings are alike, and the first panic through a
/// function past that end stops the process ("failed to initiate panic, error
/// 5") — see `compile::orc::end_unwind_table`. On x86_64 these functions all
/// share one encoding, so without the fix only the first is covered. A
/// regression here aborts the test binary rather than failing this test.
#[test]
fn a_panic_crosses_any_function_of_a_large_module() {
    let (code, names) = {
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = llvm_context();
        let module = ctx.create_module("many_functions");
        let i64_t = ctx.i64_type();
        let callee = module.add_function("typelisp_test_maybe_panic", i64_t.fn_type(&[i64_t.into()], false), Some(Linkage::External));
        let ty = i64_t.fn_type(&[ctx.ptr_type(AddressSpace::default()).into(), ctx.i32_type().into()], false);
        let builder = ctx.create_builder();
        let mut names = Vec::new();
        for i in 0..MANY {
            let name = format!("many_{}", i);
            let f = module.add_function(&name, ty, None);
            builder.position_at_end(ctx.append_basic_block(f, "entry"));
            let arg = builder
                .build_load(i64_t, f.get_nth_param(0).unwrap().into_pointer_value(), "arg0")
                .unwrap()
                .into_int_value();
            let called = builder.build_call(callee, &[arg.into()], "call").unwrap();
            let ValueKind::Basic(v) = called.try_as_basic_value() else { panic!("the callee declaration produced no value") };
            builder.build_return(Some(&v.into_int_value())).unwrap();
            names.push(name);
        }
        module.verify().expect("module failed verification");
        let externals = [(callee_name(callee), typelisp_test_maybe_panic as *const () as usize)];
        (jit_engine(&module, &externals).expect("failed to JIT-compile the module"), names)
    };
    for i in [0, 1, MANY / 2, MANY - 1] {
        let f = unsafe { std::mem::transmute::<usize, UnwindingProbe>(code.address(&names[i]).expect("function did not resolve")) };
        let payload = without_panic_output(|| catch_unwind(AssertUnwindSafe(|| unsafe { f([0i64].as_ptr(), 1) })))
            .expect_err("the panic did not propagate out of the JIT frame");
        assert_eq!(payload.downcast_ref::<String>().map(String::as_str), Some(PANIC_MESSAGE), "function {}", i);
    }
    // Freed only now, after every unwind through it has finished.
    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    drop(code);
}
