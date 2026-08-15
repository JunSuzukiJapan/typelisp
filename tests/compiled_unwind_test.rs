//! Phase A feasibility probe for making a compiled `panic` catchable instead
//! of aborting the process (`docs/dev/TODO.md`).
//!
//! Today `typelisp_rt::rt_panic` calls `std::process::abort()` because a Rust
//! `panic!` crossing back through JIT/AOT frames over an `extern "C"` boundary
//! is undefined behavior. Rust 1.71 stabilized `extern "C-unwind"` for exactly
//! this case: a panic may cross that boundary, provided every frame in between
//! is unwindable.
//!
//! The frames in between are LLVM-generated, so the open question is whether
//! *this* pipeline produces unwindable frames — MCJIT has to register the
//! module's `.eh_frame` with the system unwinder. Nothing in the codebase
//! exercises that today (no `personality`, `uwtable`, `invoke` or `landingpad`
//! anywhere), so this builds the smallest module that answers it: one function
//! under the real `CompiledSignature` ABI whose body calls an
//! `extern "C-unwind"` Rust function that panics.
//!
//! Three things this pins down, each of which changes the real fix:
//!
//! 1. **A panic does cross a JIT frame** — Phase A's level 1 ("unwind *through*
//!    a compiled frame"), the precondition for `rt_panic` to stop aborting.
//! 2. **No `uwtable` attribute is needed** on the generated function, so
//!    `compiler.rs`'s `compile-function` and the island artifact are untouched.
//! 3. **The execution engine must outlive the unwind.** Dropping it while the
//!    panic is still propagating through its own code *deadlocks the next
//!    unwind* — see [`a_second_panic_still_unwinds`]. The real
//!    [`typelisp::compile::CompiledFn`] already keeps its engine alive for the
//!    life of the process, so this is a constraint to preserve rather than one
//!    to fix, but it is invisible until a second panic happens.
//!
//! Deliberately built with raw inkwell rather than through the typelisp front
//! end: the point is to isolate the LLVM/JIT/unwinder interaction from every
//! other moving part, so a failure names the mechanism rather than the language.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::OnceLock;

use inkwell::module::Linkage;
use inkwell::values::ValueKind;
use inkwell::{AddressSpace, OptimizationLevel};

use typelisp::compile::{llvm_context, COMPILE_LOCK};

/// The message [`typelisp_test_maybe_panic`] panics with, asserted on the
/// caught payload so a test proves it caught *this* panic and not another.
const PANIC_MESSAGE: &str = "compiled-unwind probe";

/// The JIT-compiled probe's Rust-side type.
///
/// `extern "C-unwind"`, not the `extern "C"` of
/// [`typelisp::compile::CompiledSignature`]: calling through a plain
/// `extern "C"` pointer tells Rust the callee cannot unwind, which is exactly
/// the assumption being broken here. Note this is also why the real fix cannot
/// keep using `ExecutionEngine::get_function` — inkwell's
/// `UnsafeFunctionPointer` is implemented for `unsafe extern "C" fn` only, so
/// the address has to come from `get_function_address` and be transmuted.
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
/// process.
///
/// Built once on purpose. The execution engine owns the JIT'd code *and* the
/// `.eh_frame` registration the unwinder walks; dropping it while a panic is
/// still propagating through that code leaves the unwinder's registry in a
/// state where the *next* unwind hangs. `CompiledFn` holds its engine for the
/// life of the process too (`JitFunction` carries it internally), so building
/// once here matches the real lifetime rather than dodging the problem.
static PROBE: OnceLock<usize> = OnceLock::new();

/// Builds `probe(args, argc) -> i64`, whose body is
/// `typelisp_test_maybe_panic(args[0])`, and JITs it.
///
/// Mirrors `CompiledFn::new`'s setup — same `(i64*, u32) -> i64` ABI, same
/// `add_global_mapping` wiring for an external address — so a result here
/// carries over to the real compile path. No `uwtable` attribute is set: that
/// it works without one is part of what the probe establishes.
fn probe() -> UnwindingProbe {
    let addr = *PROBE.get_or_init(|| {
        // The whole-process LLVM context is shared and not thread-safe; every
        // other LLVM-driving test takes this lock for the same reason.
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = llvm_context();
        let module = ctx.create_module("unwind_probe");
        let i64_t = ctx.i64_type();
        let i32_t = ctx.i32_type();

        // The callee, declared with no body and wired to the real Rust
        // function's address below — how an already-compiled external is
        // linked in.
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

        let engine = module.create_jit_execution_engine(OptimizationLevel::None).expect("failed to create the JIT engine");
        engine.add_global_mapping(&callee, typelisp_test_maybe_panic as usize);
        let addr = engine.get_function_address("probe").expect("probe did not resolve");
        // Never dropped: see this static's doc comment.
        std::mem::forget(engine);
        addr as usize
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

/// The constraint that only shows up on the *second* panic: unwinding through
/// JIT'd code has to stay repeatable, because a REPL session will do it over
/// and over.
///
/// This deadlocked in the first version of this probe, which built a fresh
/// module and engine per call: the engine was dropped while the panic was
/// still propagating through its own code, and the next unwind then hung
/// forever. Keeping the engine alive — what `CompiledFn` already does — is
/// what makes it repeatable, so the real fix must not start dropping engines
/// on the panic path.
#[test]
fn a_second_panic_still_unwinds() {
    assert_eq!(caught_message(0), PANIC_MESSAGE);
    assert_eq!(caught_message(0), PANIC_MESSAGE);
    // ...and the frame is still usable for ordinary calls afterwards.
    assert_eq!(call_probe(50), 100);
}
