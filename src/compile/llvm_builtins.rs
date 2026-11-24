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

use coroutine::*;
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
            wk::M_ALLOCA_ARGS => Some(llvm_builder_alloca_args(args)),
            wk::M_STORE_ARG => Some(llvm_builder_store_arg(args)),
            wk::M_SCRATCH_SLOT => Some(llvm_builder_scratch_slot(args)),
            wk::M_BUILD_CALL => Some(llvm_builder_build_call(args)),
            wk::M_LOAD_ENV => Some(llvm_builder_load_env(args)),
            wk::M_BUILD_CALL_WITH_ENV => Some(llvm_builder_build_call_with_env(args)),
            wk::M_BUILD_MAKE_CLOSURE => Some(llvm_builder_build_make_closure(args)),
            wk::M_LOAD_RAW => Some(llvm_builder_load_raw(args)),
            wk::M_BUILD_SLOT_PTR => Some(llvm_builder_build_slot_ptr(args)),
            wk::M_FRAME_SLOT => Some(llvm_builder_frame_slot(args)),
            wk::M_FRAME_SLOT_ROOTED => Some(llvm_builder_frame_slot_rooted(args)),
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
            wk::M_BUILD_ICMP_LT => Some(llvm_builder_build_icmp(args, "icmp_lt", inkwell::IntPredicate::SLT)),
            wk::M_BUILD_ICMP_LE => Some(llvm_builder_build_icmp(args, "icmp_le", inkwell::IntPredicate::SLE)),
            wk::M_BUILD_ICMP_GT => Some(llvm_builder_build_icmp(args, "icmp_gt", inkwell::IntPredicate::SGT)),
            wk::M_BUILD_ICMP_GE => Some(llvm_builder_build_icmp(args, "icmp_ge", inkwell::IntPredicate::SGE)),
            wk::M_BUILD_ICMP_EQ => Some(llvm_builder_build_icmp(args, "icmp_eq", inkwell::IntPredicate::EQ)),
            wk::M_BUILD_ICMP_NE => Some(llvm_builder_build_icmp(args, "icmp_ne", inkwell::IntPredicate::NE)),
            wk::M_BUILD_COND_BR => Some(llvm_builder_build_cond_br(args)),
            wk::M_BUILD_BR => Some(llvm_builder_build_br(args)),
            wk::M_BLOCK_TERMINATED_P => Some(llvm_builder_block_terminated(args)),
            wk::M_BUILD_MALLOC => Some(llvm_builder_build_malloc(args)),
            wk::M_BUILD_FREE => Some(llvm_builder_build_free(args)),
            wk::M_BUILD_INT_TO_PTR => Some(llvm_builder_build_int_to_ptr(args)),
            wk::M_BUILD_PTR_TO_INT => Some(llvm_builder_build_ptr_to_int(args)),
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
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(index, false);
    unsafe { b.build_gep(ctx.i64_type(), array_ptr, &[idx_val], name) }
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
    let ctx = crate::compile::llvm_context();
    b.build_load(ctx.i64_type(), elem_ptr, names.1).map_err(|e| EvalError::Internal(format!("{}: {}", who, e)))
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
/// Shared by [`llvm_module_add_function`] and [`declare_external_function`]
/// (labels/closures Stage 3's JIT-only forward declarations) — both declare
/// a function under this exact same signature, just with or without a body.
pub(crate) fn compiled_fn_type() -> inkwell::types::FunctionType<'static> {
    let ctx = crate::compile::llvm_context();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false)
}

/// Get-or-create: reuses an existing declaration under `name` (a bodyless
/// forward declaration — [`declare_external_function`] or an earlier call to
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
    let ctx = crate::compile::llvm_context();
    ctx.i64_type().fn_type(&[ctx.i64_type().into()], false)
}

/// `(add-coroutine-function m name)` — get-or-create a function under
/// [`coroutine_fn_type`], for the same reason [`llvm_module_add_function`] is
/// get-or-create: LLVM uniquifies a colliding name rather than merging it.
fn llvm_module_add_coroutine_function(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let existing = module.borrow().get_function(&name);
    let function = existing.unwrap_or_else(|| module.borrow_mut().add_function(&name, coroutine_fn_type(), None));
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

/// Forward-declares `name` in `module` with the standard compiled-function
/// ABI but **no body** — [`crate::compile::driver::compile_function`]'s JIT-only step
/// (labels/closures Stage 3, `call` to a different top-level function)
/// that lets `compiler.rs`'s `compile-call` find an already-`compile`d
/// function via `get-function` before the real call target's address is supplied
/// when this declaration's own module is JIT-compiled (see that method's doc
/// comment). Not exposed as an
/// `llvm-*` builtin — unlike [`llvm_module_add_function`], the typelisp
/// compiler body itself never needs to call this; only the Rust-side JIT
/// orchestration above does. Must be called with
/// [`crate::compile::COMPILE_LOCK`] held.
pub(crate) fn declare_external_function(module: &Rc<RefCell<Module<'static>>>, name: &str) {
    module.borrow_mut().add_function(name, compiled_fn_type(), None);
}

/// [`declare_external_function`] for a **compiled Lisp** function rather than
/// an `rt_*` entry point: declared under whichever ABI this process emits.
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
    ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into(), ptr_ty.into(), ctx.i32_type().into()], false)
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
    let value = crate::compile::llvm_context().i64_type().const_int(n as u64, false);
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

/// Shared by `build-add`/`build-sub`/`build-mul`: unwrap both `llvm-value`
/// operands to `IntValue`s, apply `op` (one of `Builder::build_int_add`/
/// `_sub`/`_mul`), and re-wrap the result.
fn llvm_builder_build_int_op(
    args: &[Value],
    name: &str,
    op: impl FnOnce(&Builder<'static>, inkwell::values::IntValue<'static>, inkwell::values::IntValue<'static>, &str) -> Result<inkwell::values::IntValue<'static>, inkwell::builder::BuilderError>,
) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let a = expect_llvm_value(&args[1])?.into_int_value();
    let b = expect_llvm_value(&args[2])?.into_int_value();
    let result = op(&builder.borrow(), a, b, name).map_err(|e| EvalError::Internal(format!("build-{}: {}", name, e)))?;
    Ok(llvm_value_value(result.into()))
}

/// `(select cond then else)`: CL's `max`/`min` (and `bignum`/`ratio`'s, via
/// their own three-way `rt_*_cmp` comparator) all lower to this — an
/// `icmp`+`select` pair, branch-free, rather than real control flow. `cond`
/// is an ordinary `i64`-valued `llvm-value` (nonzero = true, the same
/// convention [`llvm_builder_build_cond_br`]'s `cond` uses), narrowed to `i1`
/// with an `icmp ne cond, 0` before `select` — which, unlike `br`, requires a
/// genuine `i1` operand, not a widened `i64`.
fn llvm_builder_build_select(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let cond = expect_llvm_value(&args[1])?.into_int_value();
    let then_v = expect_llvm_value(&args[2])?.into_int_value();
    let else_v = expect_llvm_value(&args[3])?.into_int_value();
    let bld = builder.borrow();
    let err = |e: String| EvalError::Internal(format!("build-select: {}", e));
    let ctx = crate::compile::llvm_context();
    let zero = ctx.i64_type().const_zero();
    let is_nonzero = bld.build_int_compare(inkwell::IntPredicate::NE, cond, zero, "select_cond").map_err(|e| err(e.to_string()))?;
    let result = bld.build_select(is_nonzero, then_v, else_v, "select").map_err(|e| err(e.to_string()))?;
    Ok(llvm_value_value(result.into_int_value().into()))
}

/// `(build-sadd-overflow builder module a b flag-slot)` and its `ssub`/`smul`
/// siblings: the `llvm.*.with.overflow.i64` intrinsic on two `i64` words,
/// answering the (wrapped) result and storing the overflow flag, widened to
/// an `i64` `0`/`1`, into slot 0 of `flag-slot` (an `alloca-args` pointer).
///
/// This is `int`'s fast path (`compile-assoc`'s `int` branch): two tagged
/// fixnums are `n << 1`, so the tagged words add and subtract directly and
/// overflow the `i64` exactly when the 63-bit payload sum leaves the fixnum
/// range; for a product one operand is untagged first. A checked instruction
/// rather than the sign-comparison identity because `mul` has no cheap one,
/// and one shape for all three reads better than two.
///
/// Two results and one return value: the flag travels through memory. A
/// builder answering a struct would need the island to extract from it,
/// which no other builtin does, and a slot is what the island already uses
/// to merge values across blocks (`build-try-option`).
fn llvm_builder_build_int_overflow(args: &[Value], name: &str, intrinsic_name: &str) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let a = expect_llvm_value(&args[2])?.into_int_value();
    let b = expect_llvm_value(&args[3])?.into_int_value();
    let flag_slot = expect_llvm_value(&args[4])?.into_pointer_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let i64_ty = ctx.i64_type();
    let err = |e: String| EvalError::Internal(format!("build-{}-overflow: {}", name, e));
    let intrinsic = inkwell::intrinsics::Intrinsic::find(intrinsic_name).ok_or_else(|| err(format!("no such LLVM intrinsic {}", intrinsic_name)))?;
    let decl = intrinsic
        .get_declaration(&module.borrow(), &[i64_ty.into()])
        .ok_or_else(|| err(format!("failed to declare {}", intrinsic_name)))?;
    let call = bld.build_call(decl, &[a.into(), b.into()], name).map_err(|e| err(e.to_string()))?;
    let pair = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_struct_value(),
        inkwell::values::ValueKind::Instruction(_) => return Err(err(format!("{} produced no value", intrinsic_name))),
    };
    let result = bld.build_extract_value(pair, 0, "ov_result").map_err(|e| err(e.to_string()))?.into_int_value();
    let flag = bld.build_extract_value(pair, 1, "ov_flag").map_err(|e| err(e.to_string()))?.into_int_value();
    let flag_wide = bld.build_int_z_extend(flag, i64_ty, "ov_flag_i64").map_err(|e| err(e.to_string()))?;
    let zero = i64_ty.const_zero();
    let slot0 = unsafe { bld.build_gep(i64_ty, flag_slot, &[zero], "ov_flag_ptr").map_err(|e| err(e.to_string()))? };
    bld.build_store(slot0, flag_wide).map_err(|e| err(e.to_string()))?;
    Ok(llvm_value_value(result.into()))
}

/// Stack-allocates a `[count x i64]` array and returns its base pointer, to
/// be filled in by `store-arg` and passed to `build-call` — the compiled-IR
/// equivalent of building the `i64* args` array every compiled function's
/// fixed ABI expects (see `llvm_module_add_function`'s doc comment). Opaque
/// pointers (the only kind LLVM 22 has) carry no element-type info of their own, so
/// this pointer is usable as a flat `i64*` exactly the way `load_arg`'s own
/// `args_ptr` parameter already is — every GEP against it supplies
/// `ctx.i64_type()` itself, regardless of the alloca's nominal array type.
fn llvm_builder_alloca_args(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let count = expect_int(&args[1])? as u32;
    let ctx = crate::compile::llvm_context();
    let array_ty = ctx.i64_type().array_type(count);
    let b = builder.borrow();
    let ptr = entry_alloca(&b, array_ty, "call_args").map_err(|e| EvalError::Internal(format!("alloca-args: {}", e)))?;
    Ok(llvm_value_value(ptr.into()))
}

/// An `alloca` of `ty` in the entry block of the function `b` is building,
/// with `b` left where it was.
fn entry_alloca(
    b: &Builder<'static>,
    ty: inkwell::types::ArrayType<'static>,
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

/// `(scratch-slot builder role)` — the one-word stack slot this function uses
/// for `role`, the same slot every time it is asked for.
///
/// A slot of its own per request (`alloca-args`) is a slot of its own in the
/// machine frame, so a body with a merge slot per arithmetic operation had a
/// frame as long as the body: `(+ 1 (+ 1 ... a))` a hundred thousand deep
/// needed 8.3MB. What the island asks for here instead is one slot per role,
/// on its word that no two uses of one role are ever in progress at once —
/// true of a merge slot, which is filled on each branch and read where they
/// meet before anything else asks for that role.
///
/// Only inside a frame (`coroutine-begin` .. `coroutine-end`), which is what
/// keeps the answer to one function.
fn llvm_builder_scratch_slot(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let role = expect_int(&args[1])?;
    let key = builder_key(&builder);
    let known = FRAME_CTXS.with(|c| {
        c.borrow()
            .get(&key)
            .map(|f| f.scratch.get(&role).copied())
            .ok_or_else(|| EvalError::Internal("scratch-slot: no frame is open on this builder".to_string()))
    })?;
    if let Some(slot) = known {
        return Ok(llvm_value_value(slot.into()));
    }
    let slot = entry_alloca(&builder.borrow(), crate::compile::llvm_context().i64_type().array_type(1), &format!("scratch{}", role))
        .map_err(|e| EvalError::Internal(format!("scratch-slot: {}", e)))?;
    FRAME_CTXS.with(|c| {
        if let Some(f) = c.borrow_mut().get_mut(&key) {
            f.scratch.insert(role, slot);
        }
    });
    Ok(llvm_value_value(slot.into()))
}

/// Writes `value` into slot `index` of an `alloca-args` array — the same
/// GEP pattern `load_arg` uses to *read* a logical argument, just paired
/// with a store instead of a load.
fn llvm_builder_store_arg(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let array_ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let index = expect_int(&args[2])? as u64;
    let value = expect_llvm_value(&args[3])?.into_int_value();
    let b = builder.borrow();
    let elem_ptr = i64_word_ptr(&b, array_ptr, index, "store_arg_ptr", "store-arg")?;
    b.build_store(elem_ptr, value).map_err(|e| EvalError::Internal(format!("store-arg: {}", e)))?;
    Ok(Value::Empty)
}

/// The generic-pointer read counterpart of [`llvm_builder_store_arg`] — same
/// GEP pattern as [`llvm_builder_load_arg`], but against an arbitrary
/// `array_ptr` rather than a function's own args parameter (automatic
/// retain/release insertion's `release-pending-args` uses this to read back
/// the parallel "which call/env-array slots need releasing" array
/// `compile-call-args`/`compile-env-args` built via `store-arg`).
fn llvm_builder_load_raw(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let array_ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let index = expect_int(&args[2])? as u64;
    let loaded = load_i64_word(&builder.borrow(), array_ptr, index, ("load_raw_ptr", "load_raw_val"), "load-raw")?;
    Ok(llvm_value_value(loaded))
}

/// [`llvm_builder_load_raw`]'s GEP without the load: the *address* of one
/// word in an `i64` array.
///
/// A compiled frame is an `i64` array (`Heap::open_frame`), and a local is one
/// word in it. Everything that already handles a local —
/// `resolve-value`/`compile-set`/`retain-bindings` — speaks in pointers,
/// because a local has been a 1-element `alloca-args` slot since `setf` and
/// loop re-entry needed somewhere to write. So carving the slot out of a frame
/// instead of allocating it on the machine stack is the whole change, and this
/// is the one primitive it needs.
fn llvm_builder_build_slot_ptr(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let array_ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let index = expect_int(&args[2])? as u64;
    let elem_ptr = i64_word_ptr(&builder.borrow(), array_ptr, index, "slot_ptr", "build-slot-ptr")?;
    Ok(llvm_value_value(elem_ptr.into()))
}


/// Shared by every `build-icmp-*` builtin (if/let/comparisons, labels/closures
/// Stage 5): runs `icmp <predicate>` on two `i64` operands, then widens the
/// resulting `i1` back to `i64` (0/1) via `build_int_z_extend` — every other
/// builtin here treats a compiled value as a plain `i64` (see
/// the doc comment of `llvm-module`'s definition (`typeck::builtins::llvm`)), and a comparison result is no
/// exception, which is exactly what lets `compile-if`'s `build-cond-br` (and
/// ordinary arithmetic/storage) accept it without caring it came from a
/// comparison rather than `+`/a literal.
fn llvm_builder_build_icmp(args: &[Value], name: &str, predicate: inkwell::IntPredicate) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let a = expect_llvm_value(&args[1])?.into_int_value();
    let b = expect_llvm_value(&args[2])?.into_int_value();
    let bld = builder.borrow();
    let cmp = bld.build_int_compare(predicate, a, b, name).map_err(|e| EvalError::Internal(format!("{}: {}", name, e)))?;
    let ctx = crate::compile::llvm_context();
    let widened = bld.build_int_z_extend(cmp, ctx.i64_type(), name).map_err(|e| EvalError::Internal(format!("{}: {}", name, e)))?;
    Ok(llvm_value_value(widened.into()))
}

/// `compile-if`'s branch primitive: branches to `then_block` when `cond`
/// (an ordinary `i64`-valued `llvm-value`) is nonzero, `else_block`
/// otherwise — built from an `icmp ne cond, 0` plus a conditional branch.
fn llvm_builder_build_cond_br(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let cond = expect_llvm_value(&args[1])?.into_int_value();
    let then_block = expect_llvm_basic_block(&args[2])?;
    let else_block = expect_llvm_basic_block(&args[3])?;
    let b = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let zero = ctx.i64_type().const_zero();
    let is_nonzero =
        b.build_int_compare(inkwell::IntPredicate::NE, cond, zero, "if_cond_nz").map_err(|e| EvalError::Internal(format!("build-cond-br: {}", e)))?;
    b.build_conditional_branch(is_nonzero, then_block, else_block).map_err(|e| EvalError::Internal(format!("build-cond-br: {}", e)))?;
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

/// A direct call to an already-declared `target` (typically `get-function`'s
/// result), passing `args_ptr`/`argc` straight through to its fixed ABI —
/// see the doc comment of `llvm-module`'s definition (`typeck::builtins::llvm`) for why every compiled
/// function shares that one signature regardless of arity. This is the one
/// new primitive that unlocks every statically-resolvable direct call:
/// self-recursion, `labels`-sibling calls, and top-level `defun`-to-`defun`
/// calls alike, since all three reduce to "the callee's `llvm-function`
/// already exists in this module, look it up and call it."
fn llvm_builder_build_call(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_function(&args[1])?;
    let args_ptr = expect_llvm_value(&args[2])?;
    let argc = expect_int(&args[3])? as u64;
    let ctx = crate::compile::llvm_context();
    let argc_val = ctx.i32_type().const_int(argc, false);
    let call = builder
        .borrow()
        .build_call(target, &[args_ptr.into(), argc_val.into()], "call_result")
        .map_err(|e| EvalError::Internal(format!("build-call: {}", e)))?;
    crate::compile::raise_check::after_call(&builder.borrow(), target).map_err(|e| EvalError::Internal(format!("build-call: {}", e)))?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(llvm_value_value(v)),
        inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal("build-call: callee produced no value".into())),
    }
}

/// The captures counterpart of [`llvm_builder_build_call`]: calls `target`
/// (declared via `add-function-with-env`) passing both the args array
/// (`args_ptr`/`argc`, exactly as `build-call` does) and an env array
/// (`env_ptr`/`env_len`) under its extended ABI.
fn llvm_builder_build_call_with_env(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_function(&args[1])?;
    let args_ptr = expect_llvm_value(&args[2])?;
    let argc = expect_int(&args[3])? as u64;
    let env_ptr = expect_llvm_value(&args[4])?;
    let env_len = expect_int(&args[5])? as u64;
    let ctx = crate::compile::llvm_context();
    let argc_val = ctx.i32_type().const_int(argc, false);
    let env_len_val = ctx.i32_type().const_int(env_len, false);
    let call = builder
        .borrow()
        .build_call(target, &[args_ptr.into(), argc_val.into(), env_ptr.into(), env_len_val.into()], "call_result")
        .map_err(|e| EvalError::Internal(format!("build-call-with-env: {}", e)))?;
    crate::compile::raise_check::after_call(&builder.borrow(), target)
        .map_err(|e| EvalError::Internal(format!("build-call-with-env: {}", e)))?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(llvm_value_value(v)),
        inkwell::values::ValueKind::Instruction(_) => {
            Err(EvalError::Internal("build-call-with-env: callee produced no value".into()))
        }
    }
}

/// Heap-allocates a `BoxedObj::CompiledClosure` (`typelisp-mem`) wrapping
/// `target` (a function declared via `add-function-with-env` — see
/// `compiled_fn_type_with_env`'s doc comment for why *every* closure-boxed
/// function uses that ABI) and a copy of `env`'s `env_len` values (an
/// already-built `alloca-args`/`store-arg` array — the same shape a direct
/// capturing call already builds, see `compiler.rs`'s `compile-env-args`).
/// The closure-representation unification's flip of the retired
/// `ClosureBox` (a raw `malloc`'d, reference-counted block the GC never saw)
/// to a GC-heap value: builds a scratch `rt_closure_new` argument array
/// (`target`'s raw address, `sexpr_mask`, then each captured slot copied
/// verbatim from `env_ptr`) and calls it through the ordinary `rt_*` FFI
/// convention every other `BoxedObj` constructor uses (`rt_struct_new`,
/// `rt_data_new`, ...) — `rt_closure_new` is unconditionally forward-declared
/// into every module (`rt_extern_functions`), so `target`'s own parent module
/// already has it. `sexpr_mask` (`compiler.rs`'s `compute-sexpr-mask`) is
/// passed straight through unchanged: it marks which captured slots are
/// tagged `Sexpr` values for `rt_closure_new` to `decode`, exactly the mask
/// [`BoxedObj::CompiledClosure`]'s own doc comment describes.
fn llvm_builder_build_make_closure(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let target = expect_llvm_function(&args[2])?;
    let env_ptr = expect_llvm_value(&args[3])?.into_pointer_value();
    let env_len = expect_int(&args[4])? as u64;
    // The mask arrives as two 32-bit halves: it has one bit per captured slot
    // and the island's widest fixed-width integer is `i32`, so 64 slots do not
    // fit one of its words. Each half is a *bit pattern* (`pow2` doubles into
    // `i32`'s sign bit for slot 31), so it is masked back to 32 bits here
    // rather than sign-extended.
    let mask_half = |i: usize| -> Result<u64, EvalError> {
        Ok(expect_int(&args[i])? as u64 & 0xFFFF_FFFF)
    };
    let sexpr_mask = mask_half(5)? | (mask_half(6)? << 32);
    let ctx = crate::compile::llvm_context();
    let b = builder.borrow();
    let module = module.borrow();
    // Which constructor names the ABI the body was emitted under, so the
    // closure carries it. A process can hold both at once across an ABI
    // change — the island runs from its committed dump while this binary JITs
    // fresh code — so the answer cannot be a process-wide setting, and
    // `EMITTED_BODY_ABI` (which is one) was the wrong place to read it. The
    // right one is the function being boxed: its LLVM type *is* the ABI, so
    // there is nothing to record separately and nothing to get out of step.
    // `coroutine_fn_type` takes one `i64`; the classic type takes a pointer
    // and a `u32`.
    let ctor_name = if target.get_type() == coroutine_fn_type() {
        "rt_coroutine_closure_new"
    } else {
        "rt_closure_new"
    };
    let rt_closure_new = module.get_function(ctor_name).ok_or_else(|| {
        EvalError::Internal(format!("build-make-closure: {} not declared in this module", ctor_name))
    })?;

    let i64_ty = ctx.i64_type();
    let argc = 2 + env_len;
    let ctor_args_ptr = entry_alloca(&b, i64_ty.array_type(argc as u32), "closure_ctor_args")
        .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
    let store_slot = |idx: u64, v: BasicValueEnum<'static>| -> Result<(), EvalError> {
        let idx_val = i64_ty.const_int(idx, false);
        let p = unsafe {
            b.build_gep(i64_ty, ctor_args_ptr, &[idx_val], "closure_ctor_arg_ptr")
                .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?
        };
        b.build_store(p, v).map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
        Ok(())
    };

    let fn_ptr_int = b
        .build_ptr_to_int(target.as_global_value().as_pointer_value(), i64_ty, "closure_fn_ptr")
        .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
    store_slot(0, fn_ptr_int.into())?;
    store_slot(1, i64_ty.const_int(sexpr_mask, false).into())?;
    for i in 0..env_len {
        let idx_val = i64_ty.const_int(i, false);
        let src_ptr = unsafe {
            b.build_gep(i64_ty, env_ptr, &[idx_val], "closure_env_src")
                .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?
        };
        let v = b
            .build_load(i64_ty, src_ptr, "closure_env_val")
            .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
        store_slot(2 + i, v)?;
    }

    let argc_val = ctx.i32_type().const_int(argc, false);
    let call = b
        .build_call(rt_closure_new, &[ctor_args_ptr.into(), argc_val.into()], "closure_new_result")
        .map_err(|e| EvalError::Internal(format!("build-make-closure: {}", e)))?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(llvm_value_value(v)),
        inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal("build-make-closure: rt_closure_new produced no value".into())),
    }
}

// ---- Stage 6 of the Sexpr-representation plan: generic malloc/free ------
// (`docs/implementation-log.md`) — the `ClosureBox` generalization: a general ADT box
// (`Option`/`Result`/`defstruct`) needs heap storage and offset load/store
// exactly like a `ClosureBox` does, but with no fixed header shape to bake
// in (a variant tag slot, then one slot per field — `compiler.rs`'s
// `compile-construct` lays this out itself using the four primitives below
// plus the already-generic `store-arg`/`load-raw`). Unlike `ClosureBox`,
// no refcounting/cascading-release machinery exists yet for these boxes —
// `compile-construct` simply leaks them, the same accepted trade-off this
// codebase already takes for an unreferenced boxed `labels` sibling or a
// captured reference cycle (see `compiler.rs`'s module doc comment) —
// `build-free` is exposed regardless, ready for a later stage to wire up
// automatic freeing without needing a new Rust primitive then.

/// Heap-allocates `count` `i64` slots and returns the raw pointer — see this
/// section's own doc comment. The generalization of
/// [`llvm_builder_build_make_closure`]'s own `build_array_malloc` call,
/// without baking in `ClosureBox`'s fixed header layout.
fn llvm_builder_build_malloc(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let count = expect_int(&args[1])? as u64;
    let ctx = crate::compile::llvm_context();
    let count_val = ctx.i64_type().const_int(count, false);
    let ptr = builder
        .borrow()
        .build_array_malloc(ctx.i64_type(), count_val, "box")
        .map_err(|e| EvalError::Internal(format!("build-malloc: {}", e)))?;
    Ok(llvm_value_value(ptr.into()))
}

/// Frees a pointer `build-malloc` returned (or any other `llvm-value`
/// already holding a real pointer, e.g. after `build-int-to-ptr`) — the
/// inverse of `build-malloc`. See this section's own doc comment for why
/// nothing in `compiler.rs` calls this yet.
fn llvm_builder_build_free(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    builder.borrow().build_free(ptr).map_err(|e| EvalError::Internal(format!("build-free: {}", e)))?;
    Ok(Value::Empty)
}

/// Reinterprets an `i64`-valued `llvm-value` as a pointer — every compiled
/// value is a plain `i64` (the doc comment of `llvm-module`'s definition (`typeck::builtins::llvm`)), so a
/// general ADT box value read back out of a slot/argument/field needs this
/// before `load-raw`/`store-arg` (which both expect an already-pointer-typed
/// `llvm-value`) can dereference it — `compiler.rs`'s `compile-field-get`/
/// `compile-field-set` need it directly.
fn llvm_builder_build_int_to_ptr(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let v = expect_llvm_value(&args[1])?.into_int_value();
    let ctx = crate::compile::llvm_context();
    let ptr_ty = ctx.ptr_type(AddressSpace::default());
    let ptr = builder
        .borrow()
        .build_int_to_ptr(v, ptr_ty, "int_to_ptr")
        .map_err(|e| EvalError::Internal(format!("build-int-to-ptr: {}", e)))?;
    Ok(llvm_value_value(ptr.into()))
}

/// The inverse of `build-int-to-ptr` — `compile-construct`'s final step,
/// turning a freshly `build-malloc`'d pointer into the plain `i64` value
/// every other compiled value already is, matching how `build-make-closure`
/// does the same `ptrtoint` for a `ClosureBox`.
fn llvm_builder_build_ptr_to_int(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let ctx = crate::compile::llvm_context();
    let v = builder
        .borrow()
        .build_ptr_to_int(ptr, ctx.i64_type(), "ptr_to_int")
        .map_err(|e| EvalError::Internal(format!("build-ptr-to-int: {}", e)))?;
    Ok(llvm_value_value(v.into()))
}

/// `build-fn-address`: a declared function's own address as the plain `i64`
/// every other compiled value already is — `build-ptr-to-int` applied to the
/// function itself rather than to a `build-malloc`'d block.
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
    let ctx = crate::compile::llvm_context();
    let v = builder
        .borrow()
        .build_ptr_to_int(f.as_global_value().as_pointer_value(), ctx.i64_type(), "fn_address")
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

