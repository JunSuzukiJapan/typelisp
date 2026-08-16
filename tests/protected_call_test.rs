//! The shape a compiled `catch` is built out of, in miniature.
//!
//! `tests/compiled_unwind_test.rs` established that a Rust panic can travel
//! *through* JIT frames. Stopping one is the other half, and it cannot be done
//! with an LLVM landing pad: consuming a Rust panic means freeing its
//! `_Unwind_Exception` and decrementing `panic_count`, and only
//! `std::panic::catch_unwind` does either (the object's own `exception_cleanup`
//! aborts with "Rust panics must be rethrown", and `panic_count::decrease` is
//! called from nowhere else). A landing pad that swallowed one would leak the
//! allocation and leave `std::thread::panicking()` true for the rest of the
//! process.
//!
//! So the catching Rust frame goes at the *call*: inside a protected region a
//! call is emitted as `rt_protected_call`, and what follows it is an ordinary
//! conditional branch on `rt_unwind_pending`. This module builds exactly that
//! by hand — one protected caller, one unprotected intermediate frame, and a
//! throw at the bottom — so a failure names the mechanism rather than the
//! island that will emit it.
//!
//! What it pins down:
//!
//! 1. A protected call that returns normally passes its value straight through
//!    and leaves nothing pending.
//! 2. A throw raised two compiled frames down is caught at the trampoline, and
//!    the region's dispatch block can ask whether the tag is its own
//!    ([`rt_throw_matches`]) and take the value ([`rt_throw_take_value`]).
//! 3. A tag that is *not* the region's keeps travelling: `rt_resume_unwind`
//!    re-raises, and the throw is still intact on the other side.
//! 4. All of it repeats — a session catches over and over.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::OnceLock;

use inkwell::module::Linkage;
use inkwell::values::ValueKind;
use inkwell::{AddressSpace, IntPredicate, OptimizationLevel};

use typelisp::compile::runtime::{encode, take_throw};
use typelisp::compile::{llvm_context, COMPILE_LOCK};
use typelisp_mem::{Heap, Value};

/// The probe's Rust-side type. `extern "C-unwind"`, like every real compiled
/// entry point, because a throw raised inside must be allowed to cross back.
type Probe = unsafe extern "C-unwind" fn(*const i64, u32) -> i64;

/// What `middle` returns when told not to throw.
const NO_THROW_RESULT: i64 = 999;

/// The argument array both compiled functions read, by index.
mod arg {
    /// The tag `middle` throws with (a tagged `Sexpr` symbol).
    pub const THROWN_TAG: u64 = 0;
    /// The value it throws (already tagged).
    pub const THROWN_VALUE: u64 = 1;
    /// The tag the protected region claims (a tagged `Sexpr` symbol).
    pub const CAUGHT_TAG: u64 = 2;
    /// Whether `middle` throws at all (raw `0`/`1`).
    pub const SHOULD_THROW: u64 = 3;
    /// How many slots the array has.
    pub const COUNT: u32 = 4;
}

static PROBE: OnceLock<usize> = OnceLock::new();

/// Builds and JITs the two-function probe:
///
/// ```text
/// middle(args, argc):                 ; NOT protected — the frame a throw
///   if args[SHOULD_THROW] == 0        ; must be able to unwind straight
///     ret NO_THROW_RESULT             ; through
///   rt_throw(args, 2)                 ; args[0]=tag, args[1]=value
///
/// catcher(args, argc):                ; the protected region
///   r = rt_protected_call([&middle, args, COUNT], 3)
///   if rt_unwind_pending() == 0: ret r
///   if rt_throw_matches([args[CAUGHT_TAG]], 1): ret rt_throw_take_value()
///   rt_resume_unwind()
/// ```
///
/// Built once and never dropped, for the reason `compiled_unwind_test`'s own
/// probe is: an `ExecutionEngine` dropped while a panic is still propagating
/// through its code hangs the *next* unwind.
fn probe() -> Probe {
    let addr = *PROBE.get_or_init(|| {
        let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = llvm_context();
        let module = ctx.create_module("protected_call_probe");
        let i64_t = ctx.i64_type();
        let i32_t = ctx.i32_type();
        let ptr_t = ctx.ptr_type(AddressSpace::default());
        let rt_ty = i64_t.fn_type(&[ptr_t.into(), i32_t.into()], false);

        let declare = |name: &str| module.add_function(name, rt_ty, Some(Linkage::External));
        let rt_throw = declare("rt_throw");
        let rt_protected_call = declare("rt_protected_call");
        let rt_unwind_pending = declare("rt_unwind_pending");
        let rt_throw_matches = declare("rt_throw_matches");
        let rt_throw_take_value = declare("rt_throw_take_value");
        let rt_resume_unwind = declare("rt_resume_unwind");

        let builder = ctx.create_builder();
        let load_arg = |f: inkwell::values::FunctionValue<'static>, i: u64, name: &str| {
            let args_ptr = f.get_nth_param(0).unwrap().into_pointer_value();
            let slot = unsafe {
                builder.build_gep(i64_t, args_ptr, &[i64_t.const_int(i, false)], "arg_ptr").unwrap()
            };
            builder.build_load(i64_t, slot, name).unwrap().into_int_value()
        };
        let call_rt = |f: inkwell::values::FunctionValue<'static>, args: inkwell::values::PointerValue<'static>, argc: u64, name: &str| {
            let call = builder.build_call(f, &[args.into(), i32_t.const_int(argc, false).into()], name).unwrap();
            match call.try_as_basic_value() {
                ValueKind::Basic(v) => v.into_int_value(),
                ValueKind::Instruction(_) => panic!("an rt declaration produced no value"),
            }
        };
        // A zero-length array is still a valid pointer to pass for an
        // argument-less rt call, matching what `alloca-args 0` gives the
        // island.
        let no_args = |name: &str| builder.build_array_alloca(i64_t, i64_t.const_int(0, false), name).unwrap();

        // ---- middle -------------------------------------------------------
        let middle = module.add_function("middle", rt_ty, None);
        let entry = ctx.append_basic_block(middle, "entry");
        let throwing = ctx.append_basic_block(middle, "throwing");
        let quiet = ctx.append_basic_block(middle, "quiet");
        builder.position_at_end(entry);
        let flag = load_arg(middle, arg::SHOULD_THROW, "should_throw");
        let is_set = builder.build_int_compare(IntPredicate::NE, flag, i64_t.const_int(0, false), "is_set").unwrap();
        builder.build_conditional_branch(is_set, throwing, quiet).unwrap();
        builder.position_at_end(quiet);
        builder.build_return(Some(&i64_t.const_int(NO_THROW_RESULT as u64, false))).unwrap();
        builder.position_at_end(throwing);
        // `rt_throw` reads only slots 0 and 1, which are already the tag and
        // the value — so the caller's own array is handed straight on.
        let middle_args = middle.get_nth_param(0).unwrap().into_pointer_value();
        call_rt(rt_throw, middle_args, 2, "throw");
        builder.build_return(Some(&i64_t.const_int(0, false))).unwrap();

        // ---- catcher ------------------------------------------------------
        let catcher = module.add_function("catcher", rt_ty, None);
        let entry = ctx.append_basic_block(catcher, "entry");
        let normal = ctx.append_basic_block(catcher, "normal");
        let pad = ctx.append_basic_block(catcher, "pad");
        let claim = ctx.append_basic_block(catcher, "claim");
        let pass_on = ctx.append_basic_block(catcher, "pass_on");

        builder.position_at_end(entry);
        let call_args = builder.build_array_alloca(i64_t, i64_t.const_int(3, false), "call_args").unwrap();
        let target = builder.build_ptr_to_int(middle.as_global_value().as_pointer_value(), i64_t, "target").unwrap();
        let catcher_args = catcher.get_nth_param(0).unwrap().into_pointer_value();
        let forwarded = builder.build_ptr_to_int(catcher_args, i64_t, "forwarded").unwrap();
        for (i, v) in [target, forwarded, i64_t.const_int(arg::COUNT as u64, false)].into_iter().enumerate() {
            let slot = unsafe {
                builder.build_gep(i64_t, call_args, &[i64_t.const_int(i as u64, false)], "call_arg_ptr").unwrap()
            };
            builder.build_store(slot, v).unwrap();
        }
        let result = call_rt(rt_protected_call, call_args, 3, "protected");
        let pending_args = no_args("pending_args");
        let pending = call_rt(rt_unwind_pending, pending_args, 0, "pending");
        let unwinding = builder.build_int_compare(IntPredicate::NE, pending, i64_t.const_int(0, false), "unwinding").unwrap();
        builder.build_conditional_branch(unwinding, pad, normal).unwrap();

        builder.position_at_end(normal);
        builder.build_return(Some(&result)).unwrap();

        builder.position_at_end(pad);
        let tag_args = builder.build_array_alloca(i64_t, i64_t.const_int(1, false), "tag_args").unwrap();
        let caught_tag = load_arg(catcher, arg::CAUGHT_TAG, "caught_tag");
        builder.build_store(tag_args, caught_tag).unwrap();
        let matches = call_rt(rt_throw_matches, tag_args, 1, "matches");
        let is_mine = builder.build_int_compare(IntPredicate::NE, matches, i64_t.const_int(0, false), "is_mine").unwrap();
        builder.build_conditional_branch(is_mine, claim, pass_on).unwrap();

        builder.position_at_end(claim);
        let take_args = no_args("take_args");
        let thrown = call_rt(rt_throw_take_value, take_args, 0, "thrown");
        builder.build_return(Some(&thrown)).unwrap();

        builder.position_at_end(pass_on);
        let resume_args = no_args("resume_args");
        call_rt(rt_resume_unwind, resume_args, 0, "resume");
        builder.build_return(Some(&i64_t.const_int(0, false))).unwrap();

        module.verify().expect("the probe module failed verification");

        let engine = module.create_jit_execution_engine(OptimizationLevel::None).expect("failed to create the JIT engine");
        for (decl, addr) in [
            (rt_throw, typelisp::compile::runtime::rt_throw as usize),
            (rt_protected_call, typelisp::compile::runtime::rt_protected_call as usize),
            (rt_unwind_pending, typelisp::compile::runtime::rt_unwind_pending as usize),
            (rt_throw_matches, typelisp::compile::runtime::rt_throw_matches as usize),
            (rt_throw_take_value, typelisp::compile::runtime::rt_throw_take_value as usize),
            (rt_resume_unwind, typelisp::compile::runtime::rt_resume_unwind as usize),
        ] {
            engine.add_global_mapping(&decl, addr);
        }
        let addr = engine.get_function_address("catcher").expect("catcher did not resolve");
        // Never dropped: see this function's doc comment.
        std::mem::forget(engine);
        addr as usize
    });
    unsafe { std::mem::transmute::<usize, Probe>(addr) }
}

/// A heap for one probe run, registered for the `rt_*` calls the compiled code
/// makes. Returned so the caller keeps it alive across the call.
fn heap() -> Box<Heap> {
    let mut heap = Box::new(Heap::with_capacity(1 << 12));
    typelisp::compile::runtime::set_active_heap(heap.as_mut() as *mut Heap);
    heap
}

/// Runs the probe: `middle` throws `(thrown_tag . 7)` when `should_throw`, and
/// the region claims `caught_tag`.
fn run(heap: &mut Heap, thrown_tag: &str, caught_tag: &str, should_throw: bool) -> i64 {
    let mut argv = [0i64; arg::COUNT as usize];
    argv[arg::THROWN_TAG as usize] = encode(heap.intern_symbol(thrown_tag));
    argv[arg::THROWN_VALUE as usize] = encode(Value::Int(7));
    argv[arg::CAUGHT_TAG as usize] = encode(heap.intern_symbol(caught_tag));
    argv[arg::SHOULD_THROW as usize] = i64::from(should_throw);
    unsafe { probe()(argv.as_ptr(), arg::COUNT) }
}

/// Runs `body` with the panic hook silenced, so an expected unwind does not
/// print a backtrace and make a passing run look like a failing one.
fn without_panic_output<T>(body: impl FnOnce() -> T) -> T {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = body();
    std::panic::set_hook(previous);
    out
}

/// The baseline: with nothing thrown, a protected call is just a call — the
/// callee's value comes back and no dispatch happens.
#[test]
fn a_protected_call_passes_a_normal_result_through() {
    let mut heap = heap();
    assert_eq!(run(&mut heap, "done", "done", false), NO_THROW_RESULT);
}

/// The mechanism: a throw two compiled frames down is caught at the
/// trampoline, recognised by tag, and its value produced in the catching
/// function — with no landing pad anywhere.
#[test]
fn a_matching_throw_is_claimed_by_the_protected_region() {
    let mut heap = heap();
    assert_eq!(run(&mut heap, "done", "done", true), encode(Value::Int(7)));
}

/// A tag that is not this region's belongs to an outer one, and swallowing it
/// here is exactly the bug CL's tag comparison exists to prevent. It leaves by
/// `rt_resume_unwind`, still carrying both halves.
#[test]
fn a_different_tag_keeps_travelling() {
    let mut heap = heap();
    let payload = without_panic_output(|| catch_unwind(AssertUnwindSafe(|| run(&mut heap, "outer", "inner", true))))
        .expect_err("the unmatched throw did not leave the protected region");
    assert!(payload.downcast_ref::<typelisp::compile::runtime::CompiledThrow>().is_some(), "payload was not a throw");
    let (tag, value) = unsafe { take_throw() }.expect("the throw was not still parked");
    assert_eq!(tag, "outer");
    assert_eq!(value, Value::Int(7));
}

/// Repeatable, for the same reason level 1 had to be: a session catches over
/// and over, and the engine stays usable for ordinary calls in between.
#[test]
fn catching_repeats() {
    let mut heap = heap();
    for _ in 0..3 {
        assert_eq!(run(&mut heap, "done", "done", true), encode(Value::Int(7)));
        assert_eq!(run(&mut heap, "done", "done", false), NO_THROW_RESULT);
    }
}
