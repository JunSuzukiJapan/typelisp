//! The compiler island's view of LLVM, and the handle registry behind it.
//!
//! Three things live here, and they are the same thing seen from three sides:
//!
//! - the `llvm-module`/`llvm-function`/`llvm-builder`/`llvm-value` builtins
//!   the island calls to emit IR, interpreted
//!   ([`eval_llvm_builtin_method`]);
//! - the registry those builtins hand out opaque `i64` handles from
//!   ([`NativeHandle`]);
//! - [`rt_llvm_call`], the single shim a *compiled* island reaches all of them
//!   through.
//!
//! # Why they are not in the interpreter
//!
//! They used to be, in `eval::interp`, and they were the only reason that file
//! mentioned `inkwell` at all. Everything else the interpreter does — type
//! checking, tree-walking, the prelude, running the island's own source — is
//! LLVM-free, so ~2,300 lines of IR builders in the middle of it was what tied
//! the front end to the backend.
//!
//! The interpreter reaches them through
//! the `llvm_builtin` of [`crate::eval::interp::Backend`] instead, installed by
//! whichever backend entry point is about to run the island. That is an
//! inversion, not indirection for its own sake: the dependency now points
//! backend -> front end, which is the direction it had to point for the front
//! end to become a crate of its own — which it now is, `typelisp-front`
//! (`docs/dev/implementation-log.md`, "typelisp-front クレートと AOT 内 eval").
//!
//! # Locking
//!
//! Every arm of [`eval_llvm_builtin_method`] holds [`crate::compile::COMPILE_LOCK`]
//! for its duration — see that constant's doc comment for why concurrent
//! access to the one process-wide LLVM `Context` must never happen. A whole
//! compile holds that same lock across the island, so these takes usually
//! nest inside its guard rather than acquiring it themselves.

use std::cell::RefCell;
use std::rc::Rc;

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};
use inkwell::AddressSpace;

use typelisp_mem::{wk, Heap, SymRef, Value};

use crate::eval::interp::{expect_str, str_rt, EvalError};
use crate::types::Path;

mod coroutine;
mod handles;
mod numeric;
mod slots;
mod values;

use coroutine::*;
pub(crate) use coroutine::{pending_words, store_words};
use slots::{all_of, expect_slots, operand};
use values::value_or_return;
pub use handles::*;

/// The (typelisp-hosted) compiler's view of LLVM — `llvm-module`/
/// `llvm-function`/`llvm-builder`/`llvm-value` instance and static methods.
/// Same metadata-only pattern as the rest of `eval_builtin_method` (the
/// types in `typeck::builtins::llvm` carry no `defmethod` body).
/// Every arm holds [`crate::compile::COMPILE_LOCK`] for its duration — see
/// that constant's doc comment for why concurrent access to the one
/// process-wide LLVM `Context` must never happen — nesting inside the
/// whole-compile guard when the island runs under one.
pub(crate) fn eval_llvm_builtin_method(heap: &mut Heap, type_name: &Path, method: SymRef, args: &[Value]) -> Option<Result<Value, EvalError>> {
    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::LLVM_MODULE) {
        return match method.well_known() {
            wk::M_CREATE => Some(llvm_module_create(heap, args)),
            wk::M_ADD_FUNCTION => Some(llvm_module_add_function(heap, args)),
            wk::M_VERIFY => Some(llvm_module_verify(args)),
            wk::M_TO_STRING => Some(llvm_module_to_string(heap, args)),
            wk::M_GET_FUNCTION => Some(llvm_module_get_function(heap, args)),
            wk::M_C_LONG_BITS => Some(Ok(Value::Int(i64::from(typelisp_mem::target::DataModel::current().c_long_bits())))),
            wk::M_ADD_FUNCTION_WITH_ENV => Some(llvm_module_add_function_with_env(heap, args)),
            wk::M_ADD_COROUTINE_FUNCTION => Some(llvm_module_add_coroutine_function(heap, args)),
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::LLVM_FUNCTION) {
        return match method.well_known() {
            wk::M_APPEND_BLOCK => Some(llvm_function_append_block(heap, args)),
            wk::M_FUNCTION_PARAM => Some(llvm_function_param(args)),
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::LLVM_BUILDER) {
        return match method.well_known() {
            wk::M_CREATE => Some(llvm_builder_create()),
            wk::M_POSITION_AT_END => Some(llvm_builder_position_at_end(args)),
            wk::M_CONST_WORD => Some(llvm_builder_const_word(args)),
            wk::M_BUILD_RET => Some(llvm_builder_build_ret(args)),
            wk::M_LOAD_ARG => Some(llvm_builder_load_arg(args)),
            wk::M_BUILD_ADD => Some(llvm_builder_build_int_op(args, "add", Builder::build_int_add)),
            wk::M_BUILD_SUB => Some(llvm_builder_build_int_op(args, "sub", Builder::build_int_sub)),
            wk::M_BUILD_MUL => Some(llvm_builder_build_int_op(args, "mul", Builder::build_int_mul)),
            wk::M_BUILD_AND => Some(llvm_builder_build_int_op(args, "and", Builder::build_and)),
            wk::M_BUILD_OR => Some(llvm_builder_build_int_op(args, "or", Builder::build_or)),
            wk::M_BUILD_XOR => Some(llvm_builder_build_int_op(args, "xor", Builder::build_xor)),
            wk::M_BUILD_SELECT => Some(llvm_builder_build_select(args)),
            wk::M_BUILD_SHL => Some(llvm_builder_build_int_op(args, "shl", Builder::build_left_shift)),
            wk::M_BUILD_LSHR => Some(llvm_builder_build_int_op(args, "lshr", |b, lhs, rhs, name| b.build_right_shift(lhs, rhs, false, name))),
            wk::M_BUILD_ASHR => Some(llvm_builder_build_int_op(args, "ashr", |b, lhs, rhs, name| b.build_right_shift(lhs, rhs, true, name))),
            wk::M_BUILD_FIXED_BINOP => Some(numeric::build_fixed_binop(heap, args)),
            wk::M_BUILD_FIXED_UNARY => Some(numeric::build_fixed_unary(heap, args)),
            wk::M_BUILD_FIXED_DIV_CHECK => Some(numeric::build_fixed_div_check(heap, args)),
            wk::M_BUILD_FLOAT_BINOP => Some(numeric::build_float_binop(heap, args)),
            wk::M_BUILD_FLOAT_UNARY => Some(numeric::build_float_unary(heap, args)),
            wk::M_BUILD_SADD_OVERFLOW => Some(llvm_builder_build_int_overflow(args, "sadd", "llvm.sadd.with.overflow")),
            wk::M_BUILD_SSUB_OVERFLOW => Some(llvm_builder_build_int_overflow(args, "ssub", "llvm.ssub.with.overflow")),
            wk::M_BUILD_SMUL_OVERFLOW => Some(llvm_builder_build_int_overflow(args, "smul", "llvm.smul.with.overflow")),
            wk::M_NO_VALUE => Some(slots::no_value()),
            wk::M_CONST_VALUE => Some(values::const_value(args)),
            wk::M_CONST_LIKE => Some(values::const_like(args)),
            wk::M_CONVERT => Some(values::convert(args)),
            wk::M_COERCE => Some(values::coerce(args)),
            wk::M_BUILD_NOT => Some(values::build_not(args)),
            wk::M_TAG_FIELD => Some(values::tag_field(args)),
            wk::M_UNTAG_FIELD => Some(values::untag_field(args)),
            wk::M_ARG_LIST => Some(slots::arg_list(args)),
            wk::M_TEMP_SLOT => Some(slots::temp_slot()),
            wk::M_SCRATCH_SLOT => Some(slots::scratch_slot(args)),
            wk::M_FRAME_SLOTS => Some(slots::frame_slots()),
            wk::M_FRAME_SLOT => Some(slots::frame_slot(false)),
            wk::M_FRAME_SLOT_ROOTED => Some(slots::frame_slot(true)),
            wk::M_ADD_FRAME_SLOT => Some(slots::add_frame_slot(args, false)),
            wk::M_ADD_FRAME_SLOT_ROOTED => Some(slots::add_frame_slot(args, true)),
            wk::M_SLOT_SET => Some(slots::slot_set(args)),
            wk::M_SLOT_GET => Some(slots::slot_get(args)),
            wk::M_BUILD_CALL => Some(llvm_builder_build_call(args)),
            wk::M_LOAD_ENV => Some(llvm_builder_load_env(args)),
            wk::M_BUILD_MAKE_CLOSURE => Some(llvm_builder_build_make_closure(args)),
            wk::M_COROUTINE_BEGIN => Some(llvm_builder_coroutine_begin(args)),
            wk::M_COROUTINE_CALL => Some(llvm_builder_coroutine_call(args)),
            wk::M_COROUTINE_SUSPEND => Some(llvm_builder_coroutine_suspend(args)),
            wk::M_COROUTINE_UNWIND => Some(llvm_builder_coroutine_unwind(args)),
            wk::M_FRAME_SET_HANDLER => Some(llvm_builder_frame_set_handler(args)),
            wk::M_FRAME_CLEAR_HANDLER => Some(llvm_builder_frame_clear_handler(args)),
            wk::M_COROUTINE_CALL_ENV => Some(llvm_builder_coroutine_call_env(args)),
            wk::M_COROUTINE_APPLY => Some(llvm_builder_coroutine_apply(args)),
            wk::M_COROUTINE_DYN_CALL => Some(llvm_builder_coroutine_dyn_call(args)),
            wk::M_COROUTINE_END => Some(llvm_builder_coroutine_end(args)),
            wk::M_ENTRY_VALUE => Some(coroutine::llvm_builder_entry_value(heap, args)),
            wk::M_LOOP_SAFEPOINT => Some(coroutine::llvm_builder_loop_safepoint(args)),
            wk::M_BUILD_ICMP_LT => Some(llvm_builder_build_icmp(args, "icmp_lt", inkwell::IntPredicate::SLT)),
            wk::M_BUILD_ICMP_LE => Some(llvm_builder_build_icmp(args, "icmp_le", inkwell::IntPredicate::SLE)),
            wk::M_BUILD_ICMP_GT => Some(llvm_builder_build_icmp(args, "icmp_gt", inkwell::IntPredicate::SGT)),
            wk::M_BUILD_ICMP_GE => Some(llvm_builder_build_icmp(args, "icmp_ge", inkwell::IntPredicate::SGE)),
            wk::M_BUILD_ICMP_EQ => Some(llvm_builder_build_icmp(args, "icmp_eq", inkwell::IntPredicate::EQ)),
            wk::M_BUILD_ICMP_NE => Some(llvm_builder_build_icmp(args, "icmp_ne", inkwell::IntPredicate::NE)),
            wk::M_BUILD_COND_BR => Some(llvm_builder_build_cond_br(args)),
            wk::M_BUILD_BR => Some(llvm_builder_build_br(args)),
            wk::M_BLOCK_TERMINATED_P => Some(llvm_builder_block_terminated(args)),
            wk::M_BUILD_FN_ADDRESS => Some(llvm_builder_build_fn_address(args)),
            wk::M_LOAD_TYPE => Some(llvm_builder_load_type(args)),
            _ => None,
        };
    }
    None
}

// The five accessors below resolve a handle integer back to its native
// object. They return owned values (an `Rc` clone for the two shared ones)
// rather than references, because the object lives in a thread-local registry
// that cannot lend out a borrow.

fn expect_llvm_module(v: &Value) -> Result<Rc<RefCell<Module<'static>>>, EvalError> {
    match expect_handle(v, "LlvmModule")? {
        NativeHandle::Module(m) => Ok(m),
        _ => Err(EvalError::Internal(format!("expected an LlvmModule, got {:?}", v))),
    }
}

fn expect_llvm_function(v: &Value) -> Result<FunctionValue<'static>, EvalError> {
    match expect_handle(v, "LlvmFunction")? {
        NativeHandle::Function(f) => Ok(f),
        _ => Err(EvalError::Internal(format!("expected an LlvmFunction, got {:?}", v))),
    }
}

fn expect_llvm_builder(v: &Value) -> Result<Rc<RefCell<Builder<'static>>>, EvalError> {
    match expect_handle(v, "LlvmBuilder")? {
        NativeHandle::Builder(b) => Ok(b),
        _ => Err(EvalError::Internal(format!("expected an LlvmBuilder, got {:?}", v))),
    }
}

fn expect_llvm_basic_block(v: &Value) -> Result<BasicBlock<'static>, EvalError> {
    match expect_handle(v, "LlvmBasicBlock")? {
        NativeHandle::BasicBlock(b) => Ok(b),
        _ => Err(EvalError::Internal(format!("expected an LlvmBasicBlock, got {:?}", v))),
    }
}

fn expect_llvm_value(v: &Value) -> Result<BasicValueEnum<'static>, EvalError> {
    match expect_handle(v, "LlvmValue")? {
        NativeHandle::Value(x) => Ok(x),
        _ => Err(EvalError::Internal(format!("expected an LlvmValue, got {:?}", v))),
    }
}

fn expect_int(v: &Value) -> Result<i64, EvalError> {
    match v {
        Value::Int(n) => Ok(*n),
        other => Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    }
}

/// The address of word `index` in the `i64` array at `array_ptr`: the GEP
/// every builtin that reads or writes such an array starts with. `who` names
/// the builtin in the error.
fn i64_word_ptr(
    b: &Builder<'static>,
    array_ptr: PointerValue<'static>,
    index: u64,
    name: &str,
    who: &str,
) -> Result<PointerValue<'static>, EvalError> {
    let idx_val = crate::compile::carriers::word().const_int(index, false);
    unsafe { b.build_gep(crate::compile::carriers::word(), array_ptr, &[idx_val], name) }
        .map_err(|e| EvalError::Internal(format!("{}: {}", who, e)))
}

/// Loads word `index` of the `i64` array at `array_ptr`.
fn load_i64_word(
    b: &Builder<'static>,
    array_ptr: PointerValue<'static>,
    index: u64,
    names: (&str, &str),
    who: &str,
) -> Result<BasicValueEnum<'static>, EvalError> {
    let elem_ptr = i64_word_ptr(b, array_ptr, index, names.0, who)?;
    b.build_load(crate::compile::carriers::word(), elem_ptr, names.1).map_err(|e| EvalError::Internal(format!("{}: {}", who, e)))
}

fn llvm_module_create(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let name = expect_str(heap, &args[0])?;
    let module = crate::compile::llvm_context().create_module(&name);
    Ok(llvm_module_value(module))
}

/// Every compiled function gets the same fixed C ABI — `i64 name(i64* args,
/// i32 argc)` — regardless of its typelisp-level arity (see
/// the doc comment of `llvm-module`'s definition (`typeck::builtins::llvm`) for why); `llvm-builder::load-arg`
/// reads a logical parameter back out of `args`. LLVM 22 has only opaque
/// pointers, so the parameter type is `Context::ptr_type`, not
/// `IntType::ptr_type`.
/// Shared by [`llvm_module_add_function`] and [`declare_external_compiled_function`]
/// (labels/closures Stage 3's JIT-only forward declarations) — both declare
/// a function under this exact same signature, just with or without a body.
pub(crate) fn compiled_fn_type() -> inkwell::types::FunctionType<'static> {
    let ctx = crate::compile::llvm_context();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    crate::compile::carriers::word().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false)
}

/// Get-or-create: reuses an existing declaration under `name` (a bodyless
/// forward declaration — [`declare_external_compiled_function`] or an earlier call to
/// this very builtin) instead of always minting a fresh one. LLVM's own
/// `LLVMAddFunction` does *not* do this — a second call with a colliding
/// name silently gets uniquified (`"name.1"`), never merged with the first —
/// so this reuse has to happen here. Needed since labels/closures Stage 5:
/// [`crate::compile::driver::compile_scc`] forward-declares every member of a mutually
/// recursive group under its real internal name *before* any member's body
/// is translated, and `compiler.rs`'s `compile-function` (the sole caller of
/// this builtin) must attach that member's own body to that exact same
/// declaration, not a second, disconnected one — otherwise a sibling's call
/// to it (resolved by name against the module) would find only the empty
/// declaration. A no-op generalization for every call site that predates
/// Stage 5: none of them ever collided with a pre-existing declaration under
/// the same name.
fn llvm_module_add_function(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    // A runtime function has the one signature its definition gives it.
    if let Some(sig) = crate::compile::externs::runtime_sig(&name) {
        return Ok(llvm_function_value(crate::compile::runtime_abi::declare(&module.borrow(), &name, sig)));
    }
    let existing = module.borrow().get_function(&name);
    let function = existing.unwrap_or_else(|| module.borrow_mut().add_function(&name, compiled_fn_type(), None));
    Ok(llvm_function_value(function))
}

/// The **coroutine ABI** (Phase C2): `i64 f(i64 frame)`.
///
/// One parameter, and it has to be one: entering a function and resuming it
/// must be the same call, or every indirect call — a closure, a `:dyn` method
/// — would have to know which of the two it was making. `0` in place of a
/// frame means "first entry, make your own"; the arguments are waiting in
/// `typelisp_abi::call_state`.
///
/// What comes back is a **status word**, not the value. The value is in the
/// frame, which is what makes stopping in the middle sayable at all.
pub(crate) fn coroutine_fn_type() -> inkwell::types::FunctionType<'static> {
    crate::compile::carriers::word().fn_type(&[crate::compile::carriers::word().into()], false)
}

/// `(add-coroutine-function m name)` — get-or-create a function under
/// [`coroutine_fn_type`], for the same reason [`llvm_module_add_function`] is
/// get-or-create: LLVM uniquifies a colliding name rather than merging it.
fn llvm_module_add_coroutine_function(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let existing = module.borrow().get_function(&name);
    // A direct function (`crate::compile::direct`) has a signature of its own.
    let ty = crate::compile::direct::sig_of(&name).map_or_else(coroutine_fn_type, |sig| sig.fn_type());
    let function = existing.unwrap_or_else(|| module.borrow_mut().add_function(&name, ty, None));
    Ok(llvm_function_value(function))
}

/// `(function-param f idx)` — a function's raw LLVM parameter.
///
/// Distinct from `load-arg`, which reads a *logical* argument out of the
/// `i64*` array the old ABI passes. Under the coroutine ABI the frame is a
/// real parameter, so there is no array to read from.
fn llvm_function_param(args: &[Value]) -> Result<Value, EvalError> {
    let function = expect_llvm_function(&args[0])?;
    let idx = match &args[1] {
        Value::Int(n) => *n as u32,
        other => return Err(EvalError::Internal(format!("function-param: expected an Int, got {:?}", other))),
    };
    function
        .get_nth_param(idx)
        .map(llvm_value_value)
        .ok_or_else(|| EvalError::Internal(format!("function-param: no parameter {}", idx)))
}

/// Forward-declares the **compiled Lisp** function `name` in `module`, with
/// no body, under whichever ABI this process emits.
///
/// The two cannot share one signature any more. A forward declaration is
/// matched to its definition by name, and `add-coroutine-function` is
/// get-or-create — so a Lisp symbol pre-declared under the original ABI is the
/// declaration the island then tries to give a coroutine body to, and the
/// mismatch shows up as a `ptr` where the frame parameter should be. The
/// runtime keeps the original ABI regardless: `rt_*` functions are Rust, and
/// Rust does not have a driver to hand control back to.
pub(crate) fn declare_external_compiled_function(module: &Rc<RefCell<Module<'static>>>, name: &str) {
    let ty = if crate::compile::EMITTED_BODY_ABI == typelisp_abi::BODY_ABI_COROUTINE {
        coroutine_fn_type()
    } else {
        compiled_fn_type()
    };
    module.borrow_mut().add_function(name, ty, None);
}

/// The captures counterpart of [`compiled_fn_type`]: `i64 name(i64* args,
/// i32 argc, i64* env, i32 env_len)` — used for a `labels` sibling whenever
/// its block's shared captured-name list
/// (`check::freevars::free_vars`) is non-empty, and (labels/closures
/// Stage 4) for *every* function ever wrapped into a `ClosureBox` via
/// `build-make-closure`, capturing or not — see that builtin's doc comment
/// (`llvm-builder`'s definition (`typeck::builtins::llvm`)) for why unifying on one ABI regardless of
/// whether a given closure actually captures anything is what lets the
/// driver call through it without first checking which case it's in.
fn compiled_fn_type_with_env() -> inkwell::types::FunctionType<'static> {
    let ctx = crate::compile::llvm_context();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    crate::compile::carriers::word().fn_type(&[ptr_ty.into(), ctx.i32_type().into(), ptr_ty.into(), ctx.i32_type().into()], false)
}

/// `llvm-builder::load-env` reads a logical captured slot back out of `env`,
/// the same way `load-arg` reads a logical parameter out of `args`.
fn llvm_module_add_function_with_env(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let function = module.borrow_mut().add_function(&name, compiled_fn_type_with_env(), None);
    Ok(llvm_function_value(function))
}

fn llvm_module_verify(args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let ok = module.borrow().verify().is_ok();
    Ok(Value::Bool(ok))
}

fn llvm_module_to_string(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let text = module.borrow().print_to_string().to_string();
    Ok(str_rt(heap, text))
}

/// Looks up an already-`add-function`-declared `llvm-function` by name —
/// see the doc comment of `llvm-module`'s definition (`typeck::builtins::llvm`) on `get-function` for why
/// this is the core lookup every direct call (self-recursion, `labels`
/// siblings, top-level `defun`-to-`defun` calls) is built on.
fn llvm_module_get_function(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let found = module.borrow().get_function(&name);
    found
        .map(llvm_function_value)
        .ok_or_else(|| EvalError::Panic(format!("get-function: no function named \"{}\" in this module", name)))
}

fn llvm_function_append_block(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let function = expect_llvm_function(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let block = crate::compile::llvm_context().append_basic_block(function, &name);
    Ok(llvm_block_value(block))
}

fn llvm_builder_create() -> Result<Value, EvalError> {
    let builder = crate::compile::llvm_context().create_builder();
    Ok(llvm_builder_value(builder))
}

fn llvm_builder_position_at_end(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let block = expect_llvm_basic_block(&args[1])?;
    builder.borrow().position_at_end(block);
    Ok(Value::Empty)
}

/// `(const-word builder n)`: an LLVM 64-bit integer constant.
///
/// The *generated* code's word is 64 bits whatever the source language's
/// integer types are, which is why this outlived `i64` (the source type it
/// used to be named for). `n` is an `i32` and is sign-extended, so a caller
/// passing `-1` gets the all-ones word it means; the two places that need a
/// constant wider than `i32` (`compile-int`/`compile-float`) shift two halves
/// together in IR rather than asking for one here.
fn llvm_builder_const_word(args: &[Value]) -> Result<Value, EvalError> {
    let _builder = expect_llvm_builder(&args[0])?;
    let n = expect_int(&args[1])?;
    let value = crate::compile::carriers::word().const_int(n as u64, false);
    Ok(llvm_value_value(value.into()))
}

fn llvm_builder_build_ret(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let value = expect_llvm_value(&args[1])?;
    builder
        .borrow()
        .build_return(Some(&value))
        .map_err(|e| EvalError::Internal(format!("build-ret: {}", e)))?;
    Ok(Value::Empty)
}

/// Reads logical parameter `index` out of `function`'s fixed-ABI argument
/// array (its sole real LLVM parameter — see `llvm_module_add_function`'s
/// doc comment) via a GEP + load. `i64` only for now, matching every other
/// `llvm-builder` arithmetic builtin.
fn llvm_builder_load_arg(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let function = expect_llvm_function(&args[1])?;
    let index = expect_int(&args[2])? as u64;
    let args_ptr = function
        .get_nth_param(0)
        .ok_or_else(|| EvalError::Internal("load-arg: function has no args parameter".into()))?
        .into_pointer_value();
    let loaded = load_i64_word(&builder.borrow(), args_ptr, index, ("arg_ptr", "arg_val"), "load-arg")?;
    Ok(llvm_value_value(loaded))
}

/// Reads logical captured slot `index` out of `function`'s env array
/// (its 3rd real LLVM parameter, `get_nth_param(2)` — see
/// `llvm_module_add_function_with_env`'s doc comment) — the same GEP+load
/// pattern `load_arg` uses against the args array (parameter 0), just
/// against the env one instead.
fn llvm_builder_load_env(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let function = expect_llvm_function(&args[1])?;
    let index = expect_int(&args[2])? as u64;
    let env_ptr = function
        .get_nth_param(2)
        .ok_or_else(|| EvalError::Internal("load-env: function has no env parameter".into()))?
        .into_pointer_value();
    let loaded = load_i64_word(&builder.borrow(), env_ptr, index, ("env_ptr", "env_val"), "load-env")?;
    Ok(llvm_value_value(loaded))
}

/// Two operands of one integer type, which a builder of one instruction
/// takes: `None` when either is no value.
fn int_pair(a: &Value, b: &Value, what: &str) -> Result<Option<(inkwell::values::IntValue<'static>, inkwell::values::IntValue<'static>)>, EvalError> {
    match (operand(a)?, operand(b)?) {
        (Some(BasicValueEnum::IntValue(a)), Some(BasicValueEnum::IntValue(b))) if a.get_type() == b.get_type() => Ok(Some((a, b))),
        (None, _) | (_, None) => Ok(None),
        (Some(a), Some(b)) => Err(EvalError::Internal(format!(
            "{}: a {} and a {}",
            what,
            a.get_type().print_to_string(),
            b.get_type().print_to_string()
        ))),
    }
}

/// An instruction on two integers of one type.
fn llvm_builder_build_int_op(
    args: &[Value],
    name: &str,
    op: impl FnOnce(&Builder<'static>, inkwell::values::IntValue<'static>, inkwell::values::IntValue<'static>, &str) -> Result<inkwell::values::IntValue<'static>, inkwell::builder::BuilderError>,
) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let Some((a, b)) = int_pair(&args[1], &args[2], name)? else { return slots::no_value() };
    let result = op(&builder.borrow(), a, b, name).map_err(|e| EvalError::Internal(format!("build-{}: {}", name, e)))?;
    Ok(llvm_value_value(result.into()))
}

/// `(build-select builder cond then else)` — `then` when `cond`, a `bool`,
/// is true, and `else` otherwise; both are computed.
fn llvm_builder_build_select(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let cond = value_or_return!(&args[1]).into_int_value();
    let Some((then_v, else_v)) = int_pair(&args[2], &args[3], "build-select")? else { return slots::no_value() };
    let r = builder
        .borrow()
        .build_select(cond, then_v, else_v, "select")
        .map_err(|e| EvalError::Internal(format!("build-select: {}", e)))?;
    Ok(llvm_value_value(r))
}

/// `(build-sadd-overflow builder module a b flag-slot)` and its `ssub`/`smul`
/// siblings: the `llvm.*.with.overflow` intrinsic on two words, answering the
/// (wrapped) result and putting whether it overflowed, a `bool`, in slot 0 of
/// `flag-slot`.
///
/// This is `int`'s fast path (`compile-assoc`'s `int` branch): two tagged
/// fixnums are `n << 1`, so the tagged words add and subtract directly and
/// overflow the `i64` exactly when the 63-bit payload sum leaves the fixnum
/// range; for a product one operand is untagged first. A checked instruction
/// rather than the sign-comparison identity because `mul` has no cheap one,
/// and one shape for all three reads better than two.
///
/// Two results and one return value: the flag travels through a place. A
/// builder answering a struct would need the island to extract from it,
/// which no other builtin does, and a place is what the island already uses
/// to merge values across blocks (`build-try-option`).
fn llvm_builder_build_int_overflow(args: &[Value], name: &str, intrinsic_name: &str) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let Some((a, b)) = int_pair(&args[2], &args[3], name)? else { return slots::no_value() };
    let flag_slot = expect_slots(&args[4])?;
    let err = |e: String| EvalError::Internal(format!("build-{}-overflow: {}", name, e));
    let intrinsic = inkwell::intrinsics::Intrinsic::find(intrinsic_name).ok_or_else(|| err(format!("no such LLVM intrinsic {}", intrinsic_name)))?;
    let decl = intrinsic
        .get_declaration(&module.borrow(), &[a.get_type().into()])
        .ok_or_else(|| err(format!("failed to declare {}", intrinsic_name)))?;
    let (result, flag) = {
        let bld = builder.borrow();
        let call = bld.build_call(decl, &[a.into(), b.into()], name).map_err(|e| err(e.to_string()))?;
        let pair = match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_struct_value(),
            inkwell::values::ValueKind::Instruction(_) => return Err(err(format!("{} produced no value", intrinsic_name))),
        };
        let result = bld.build_extract_value(pair, 0, "ov_result").map_err(|e| err(e.to_string()))?;
        (result, bld.build_extract_value(pair, 1, "ov_flag").map_err(|e| err(e.to_string()))?)
    };
    slots::put(&builder, &flag_slot, 0, Some(flag))?;
    Ok(llvm_value_value(result))
}

/// An `alloca` of `ty` in the entry block of the function `b` is building,
/// with `b` left where it was.
fn entry_alloca(
    b: &Builder<'static>,
    ty: inkwell::types::BasicTypeEnum<'static>,
    name: &str,
) -> Result<PointerValue<'static>, String> {
    // The array is emitted in the **entry block**, not where the island asked
    // for it.
    //
    // An `alloca` is a definition like any other, so it has to dominate every
    // use, and under the coroutine ABI the block that asks is often not the
    // block that uses: a call splits the block underneath the island, and what
    // follows the call is emitted into a resume block that only the pc
    // dispatch chain branches to. The entry block dominates the whole
    // function, and it is the only block that does.
    //
    // This is also where an `alloca` belongs anyway — one per activation
    // rather than one per time round a loop, which is what LLVM's own
    // stack-slot handling expects. Anywhere else it is a *dynamic* `alloca`:
    // the stack pointer moves every time the block runs, and nothing moves it
    // back until the function returns.
    //
    // What it does *not* buy is survival across a suspension. The machine
    // frame is torn down when the function hands control back, so an `alloca`
    // holds nothing on the way in again. Only a value that is stored and read
    // without a call in between may live here; anything that has to outlive a
    // call belongs in a frame slot.
    //
    // Built with a builder of its own: `b` may stand in the middle of a block
    // (before its terminator, say), and a position there cannot be read back
    // to return to.
    let entry = b
        .get_insert_block()
        .and_then(|blk| blk.get_parent())
        .and_then(|f| f.get_first_basic_block())
        .ok_or_else(|| "the builder is not inside a function".to_string())?;
    let at_entry = crate::compile::llvm_context().create_builder();
    match entry.get_terminator() {
        Some(term) => at_entry.position_before(&term),
        None => at_entry.position_at_end(entry),
    }
    at_entry.build_alloca(ty, name).map_err(|e| e.to_string())
}

/// Every `build-icmp-*`: a comparison of two integers of one type, read as
/// signed, answering a `bool`.
fn llvm_builder_build_icmp(args: &[Value], name: &str, predicate: inkwell::IntPredicate) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let Some((a, b)) = int_pair(&args[1], &args[2], name)? else { return slots::no_value() };
    let cmp = builder.borrow().build_int_compare(predicate, a, b, name).map_err(|e| EvalError::Internal(format!("{}: {}", name, e)))?;
    Ok(llvm_value_value(cmp.into()))
}

/// `compile-if`'s branch primitive: to `then_block` when `cond`, a `bool`,
/// is true, and to `else_block` otherwise.
fn llvm_builder_build_cond_br(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let bool_ty = crate::compile::llvm_context().bool_type();
    let cond = match operand(&args[1])? {
        Some(BasicValueEnum::IntValue(v)) if v.get_type() == bool_ty => v,
        None => bool_ty.const_zero(),
        Some(other) => return Err(EvalError::Internal(format!("build-cond-br: on a {}", other.get_type().print_to_string()))),
    };
    let then_block = expect_llvm_basic_block(&args[2])?;
    let else_block = expect_llvm_basic_block(&args[3])?;
    builder
        .borrow()
        .build_conditional_branch(cond, then_block, else_block)
        .map_err(|e| EvalError::Internal(format!("build-cond-br: {}", e)))?;
    Ok(Value::Empty)
}

/// An unconditional branch — `compile-if`'s then/else arms use this to join
/// back at the merge block after storing their value into the shared slot.
fn llvm_builder_build_br(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_basic_block(&args[1])?;
    builder.borrow().build_unconditional_branch(target).map_err(|e| EvalError::Internal(format!("build-br: {}", e)))?;
    Ok(Value::Empty)
}

/// See the doc comment of `llvm-builder`'s definition (`typeck::builtins::llvm`) for `block-terminated?`.
fn llvm_builder_block_terminated(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let terminated = builder.borrow().get_insert_block().and_then(|bb| bb.get_terminator()).is_some();
    Ok(Value::Bool(terminated))
}

/// `(build-call builder target args)` — a call of `target` with the values in
/// the slots `args`: a runtime function, under its own signature.
///
/// A function under the array ABI (`compiled_fn_type`) is called with them in
/// an array of words, and so is a runtime function that takes a list of any
/// length.
fn llvm_builder_build_call(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_function(&args[1])?;
    let given = expect_slots(&args[2])?;
    let err = |e: String| EvalError::Internal(format!("build-call: {}", e));
    let b = builder.borrow();
    let given = all_of(&b, &given)?;
    if let Some(sig) = crate::compile::externs::runtime_sig(&target.get_name().to_string_lossy()).filter(|sig| !sig.takes_array()) {
        let given: Vec<_> = given
            .iter()
            .zip(&sig.params)
            .map(|(v, p)| v.unwrap_or_else(|| crate::compile::raise_check::zero_of(p.llvm())))
            .collect();
        let (_, result) = crate::compile::runtime_abi::call_declared(&b, target, sig, &given).map_err(err)?;
        crate::compile::raise_check::after_call(&b, target).map_err(err)?;
        return Ok(llvm_value_value(result));
    }
    let word_zero = crate::compile::carriers::word().const_zero();
    let given: Vec<_> = given.into_iter().map(|v| v.unwrap_or_else(|| word_zero.into())).collect();
    match emit_rt_call(&b, target, &given, "call", "build-call")? {
        Some(v) => Ok(llvm_value_value(v)),
        None => Err(err("callee produced no value".into())),
    }
}

/// `(build-make-closure builder m target env mask-low mask-high)` — a
/// function value of `target` over the values in the slots `env`.
///
/// `mask` marks which captured values are tagged words, for the runtime to
/// `decode` ([`BoxedObj::CompiledClosure`]'s own doc comment). It arrives as
/// two 32-bit halves: one bit per captured slot, and the island's widest
/// fixed-width integer is `i32`, so 64 slots do not fit one of its words.
/// Each half is a *bit pattern* (`pow2` doubles into `i32`'s sign bit for
/// slot 31), so it is masked back to 32 bits here rather than sign-extended.
fn llvm_builder_build_make_closure(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let target = expect_llvm_function(&args[2])?;
    let env = expect_slots(&args[3])?;
    let mask_half = |i: usize| -> Result<u64, EvalError> {
        Ok(expect_int(&args[i])? as u64 & 0xFFFF_FFFF)
    };
    let sexpr_mask = mask_half(4)? | (mask_half(5)? << 32);
    let err = |e: String| EvalError::Internal(format!("build-make-closure: {}", e));
    let b = builder.borrow();
    // Which constructor names the ABI the body was emitted under, so the
    // closure carries it. A process can hold both at once across an ABI
    // change — the island runs from its committed dump while this binary JITs
    // fresh code — so the answer cannot be a process-wide setting, and
    // `EMITTED_BODY_ABI` (which is one) was the wrong place to read it. The
    // right one is the function being boxed: its LLVM type *is* the ABI, so
    // there is nothing to record separately and nothing to get out of step.
    // `coroutine_fn_type` takes one word; the classic type takes a pointer
    // and a `u32`.
    let ctor_name = if target.get_type() == coroutine_fn_type() {
        "rt_coroutine_closure_new"
    } else {
        "rt_closure_new"
    };
    let ctor = module
        .borrow()
        .get_function(ctor_name)
        .ok_or_else(|| err(format!("{} not declared in this module", ctor_name)))?;
    let word_ty = crate::compile::carriers::word();
    let address = b
        .build_ptr_to_int(target.as_global_value().as_pointer_value(), word_ty, "closure_fn_ptr")
        .map_err(|e| err(e.to_string()))?;
    let mut given: Vec<BasicValueEnum<'static>> = vec![address.into(), word_ty.const_int(sexpr_mask, false).into()];
    given.extend(all_of(&b, &env)?.into_iter().map(|v| v.unwrap_or_else(|| word_ty.const_zero().into())));
    match emit_rt_call(&b, ctor, &given, "closure_new", "build-make-closure")? {
        Some(v) => Ok(llvm_value_value(v)),
        None => Err(err(format!("{} produced no value", ctor_name))),
    }
}

/// `build-fn-address`: a declared function's own address, as a word.
///
/// Every Lisp call is the caller: `coroutine-call` *names* its callee to the
/// driver rather than being a `call` instruction with it, so the target has to
/// be a value that can be handed over. LLVM already treats a `FunctionValue`
/// as a pointer constant, so this is a `ptrtoint` on it and nothing else.
///
/// Until C4 the region-aware trampolines were the reason this existed. They
/// are gone; the driver protocol needs it for the same reason they did.
fn llvm_builder_build_fn_address(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let f = expect_llvm_function(&args[1])?;
    let v = builder
        .borrow()
        .build_ptr_to_int(f.as_global_value().as_pointer_value(), crate::compile::carriers::word(), "fn_address")
        .map_err(|e| EvalError::Internal(format!("build-fn-address: {}", e)))?;
    Ok(llvm_value_value(v.into()))
}

/// `(load-type builder module n)`: the type at place `n` of this process's
/// type table, as the word the runtime takes it in, read out of `module`'s
/// own table ([`crate::compile::type_table::load`]).
fn llvm_builder_load_type(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let n = expect_int(&args[2])?;
    let ty = u32::try_from(n)
        .ok()
        .and_then(crate::TypeId::from_index)
        .ok_or_else(|| EvalError::Internal(format!("load-type: {} is no type", n)))?;
    let v = crate::compile::type_table::load(&builder.borrow(), &module.borrow(), ty).map_err(EvalError::Internal)?;
    Ok(llvm_value_value(v.into()))
}

