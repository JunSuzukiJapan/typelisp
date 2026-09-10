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
//! [`crate::eval::interp::set_llvm_builtin_hook`] instead, installed by
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
//! access to the one process-wide LLVM `Context` must never happen.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue, InstructionValue, IntValue, PointerValue};
use inkwell::AddressSpace;

use typelisp_mem::{Heap, Value};

use crate::eval::interp::{
    expect_str, expect_struct_box, scope_clone_frames_heap, scope_pop_frame_heap, scope_push_frame_heap,
    scope_set_raw, str_rt, EvalError,
};
use crate::types::Type;
use crate::types::Path;

/// The (typelisp-hosted) compiler's view of LLVM — `llvm-module`/
/// `llvm-function`/`llvm-builder`/`llvm-value` instance and static methods.
/// Same metadata-only pattern as the rest of `eval_builtin_method` (the
/// `AdtDef`s in `registry::llvm_module_def` etc. carry no `defmethod` body).
/// Every arm holds [`crate::compile::COMPILE_LOCK`] for its duration — see
/// that constant's doc comment for why concurrent access to the one
/// process-wide LLVM `Context` must never happen.
pub(crate) fn eval_llvm_builtin_method(heap: &mut Heap, type_name: &Path, method: &str, args: &[Value]) -> Option<Result<Value, EvalError>> {
    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap();
    if *type_name == Path::root("llvm-module") {
        return match method {
            "create" => Some(llvm_module_create(heap, args)),
            "add-function" => Some(llvm_module_add_function(heap, args)),
            "verify" => Some(llvm_module_verify(args)),
            "to-string" => Some(llvm_module_to_string(heap, args)),
            "get-function" => Some(llvm_module_get_function(heap, args)),
            "add-function-with-env" => Some(llvm_module_add_function_with_env(heap, args)),
            "add-coroutine-function" => Some(llvm_module_add_coroutine_function(heap, args)),
            _ => None,
        };
    }
    if *type_name == Path::root("llvm-function") {
        return match method {
            "append-block" => Some(llvm_function_append_block(heap, args)),
            "function-param" => Some(llvm_function_param(args)),
            _ => None,
        };
    }
    if *type_name == Path::root("llvm-builder") {
        return match method {
            "create" => Some(llvm_builder_create()),
            "position-at-end" => Some(llvm_builder_position_at_end(args)),
            "const-word" => Some(llvm_builder_const_word(args)),
            "build-ret" => Some(llvm_builder_build_ret(args)),
            "load-arg" => Some(llvm_builder_load_arg(args)),
            "build-add" => Some(llvm_builder_build_int_op(args, "add", Builder::build_int_add)),
            "build-sub" => Some(llvm_builder_build_int_op(args, "sub", Builder::build_int_sub)),
            "build-mul" => Some(llvm_builder_build_int_op(args, "mul", Builder::build_int_mul)),
            "build-and" => Some(llvm_builder_build_int_op(args, "and", Builder::build_and)),
            "build-or" => Some(llvm_builder_build_int_op(args, "or", Builder::build_or)),
            "build-xor" => Some(llvm_builder_build_int_op(args, "xor", Builder::build_xor)),
            "build-select" => Some(llvm_builder_build_select(args)),
            "build-shl" => Some(llvm_builder_build_int_op(args, "shl", Builder::build_left_shift)),
            "build-lshr" => Some(llvm_builder_build_int_op(args, "lshr", |b, lhs, rhs, name| b.build_right_shift(lhs, rhs, false, name))),
            "build-ashr" => Some(llvm_builder_build_int_op(args, "ashr", |b, lhs, rhs, name| b.build_right_shift(lhs, rhs, true, name))),
            "build-fadd" => Some(llvm_builder_build_float_op(args, "fadd", Builder::build_float_add)),
            "build-fsub" => Some(llvm_builder_build_float_op(args, "fsub", Builder::build_float_sub)),
            "build-fmul" => Some(llvm_builder_build_float_op(args, "fmul", Builder::build_float_mul)),
            "build-fdiv" => Some(llvm_builder_build_float_op(args, "fdiv", Builder::build_float_div)),
            "build-frem" => Some(llvm_builder_build_float_op(args, "frem", Builder::build_float_rem)),
            "build-fcmp-lt" => Some(llvm_builder_build_fcmp(args, "fcmp_lt", inkwell::FloatPredicate::OLT)),
            "build-fcmp-le" => Some(llvm_builder_build_fcmp(args, "fcmp_le", inkwell::FloatPredicate::OLE)),
            "build-fcmp-gt" => Some(llvm_builder_build_fcmp(args, "fcmp_gt", inkwell::FloatPredicate::OGT)),
            "build-fcmp-ge" => Some(llvm_builder_build_fcmp(args, "fcmp_ge", inkwell::FloatPredicate::OGE)),
            "build-fcmp-eq" => Some(llvm_builder_build_fcmp(args, "fcmp_eq", inkwell::FloatPredicate::OEQ)),
            "build-fcmp-ne" => Some(llvm_builder_build_fcmp(args, "fcmp_ne", inkwell::FloatPredicate::UNE)),
            "build-fsqrt" => Some(llvm_builder_build_float_unary_intrinsic(args, "fsqrt", "llvm.sqrt.f64")),
            "build-ffloor" => Some(llvm_builder_build_float_unary_intrinsic(args, "ffloor", "llvm.floor.f64")),
            "build-fceil" => Some(llvm_builder_build_float_unary_intrinsic(args, "fceil", "llvm.ceil.f64")),
            "build-fround" => Some(llvm_builder_build_float_unary_intrinsic(args, "fround", "llvm.round.f64")),
            "build-ftrunc" => Some(llvm_builder_build_float_unary_intrinsic(args, "ftrunc", "llvm.trunc.f64")),
            "build-fpow" => Some(llvm_builder_build_fpow(args)),
            "build-fmaxnum" => Some(llvm_builder_build_float_binary_intrinsic(args, "fmaxnum", "llvm.maxnum.f64")),
            "build-fminnum" => Some(llvm_builder_build_float_binary_intrinsic(args, "fminnum", "llvm.minnum.f64")),
            "build-fsin" => Some(llvm_builder_build_float_unary_intrinsic(args, "fsin", "llvm.sin.f64")),
            "build-fcos" => Some(llvm_builder_build_float_unary_intrinsic(args, "fcos", "llvm.cos.f64")),
            "build-fexp" => Some(llvm_builder_build_float_unary_intrinsic(args, "fexp", "llvm.exp.f64")),
            "build-flog" => Some(llvm_builder_build_float_unary_intrinsic(args, "flog", "llvm.log.f64")),
            "build-fptosi" => Some(llvm_builder_build_fptosi(args)),
            "build-sitofp" => Some(llvm_builder_build_sitofp(args)),
            "build-fround32" => Some(llvm_builder_build_fround32(args)),
            "alloca-args" => Some(llvm_builder_alloca_args(args)),
            "store-arg" => Some(llvm_builder_store_arg(args)),
            "build-call" => Some(llvm_builder_build_call(args)),
            "load-env" => Some(llvm_builder_load_env(args)),
            "build-call-with-env" => Some(llvm_builder_build_call_with_env(args)),
            "build-make-closure" => Some(llvm_builder_build_make_closure(args)),
            "build-closure-apply" => Some(llvm_builder_build_closure_apply(args)),
            "load-raw" => Some(llvm_builder_load_raw(args)),
            "build-slot-ptr" => Some(llvm_builder_build_slot_ptr(args)),
            "frame-begin" => Some(llvm_builder_frame_begin(args)),
            "frame-value" => Some(llvm_builder_frame_value(args)),
            "frame-slot" => Some(llvm_builder_frame_slot(args)),
            "frame-slot-rooted" => Some(llvm_builder_frame_slot_rooted(args)),
            "frame-end" => Some(llvm_builder_frame_end(args)),
            "coroutine-begin" => Some(llvm_builder_coroutine_begin(args)),
            "coroutine-call" => Some(llvm_builder_coroutine_call(args)),
            "coroutine-call-env" => Some(llvm_builder_coroutine_call_env(args)),
            "coroutine-end" => Some(llvm_builder_coroutine_end(args)),
            "build-icmp-lt" => Some(llvm_builder_build_icmp(args, "icmp_lt", inkwell::IntPredicate::SLT)),
            "build-icmp-le" => Some(llvm_builder_build_icmp(args, "icmp_le", inkwell::IntPredicate::SLE)),
            "build-icmp-gt" => Some(llvm_builder_build_icmp(args, "icmp_gt", inkwell::IntPredicate::SGT)),
            "build-icmp-ge" => Some(llvm_builder_build_icmp(args, "icmp_ge", inkwell::IntPredicate::SGE)),
            "build-icmp-eq" => Some(llvm_builder_build_icmp(args, "icmp_eq", inkwell::IntPredicate::EQ)),
            "build-icmp-ne" => Some(llvm_builder_build_icmp(args, "icmp_ne", inkwell::IntPredicate::NE)),
            "build-cond-br" => Some(llvm_builder_build_cond_br(args)),
            "build-br" => Some(llvm_builder_build_br(args)),
            "block-terminated?" => Some(llvm_builder_block_terminated(args)),
            "build-malloc" => Some(llvm_builder_build_malloc(args)),
            "build-free" => Some(llvm_builder_build_free(args)),
            "build-int-to-ptr" => Some(llvm_builder_build_int_to_ptr(args)),
            "build-ptr-to-int" => Some(llvm_builder_build_ptr_to_int(args)),
            "build-fn-address" => Some(llvm_builder_build_fn_address(args)),
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

fn llvm_module_create(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let name = expect_str(heap, &args[0])?;
    let module = crate::compile::llvm_context().create_module(name);
    Ok(llvm_module_value(module))
}

/// Every compiled function gets the same fixed C ABI — `i64 name(i64* args,
/// i32 argc)` — regardless of its typelisp-level arity (see
/// `registry::llvm_module_def`'s doc comment for why); `llvm-builder::load-arg`
/// reads a logical parameter back out of `args`. LLVM 17 defaults to opaque
/// pointers (inkwell's `llvm17-0` feature doesn't pull in its
/// `typed-pointers` feature — confirmed against inkwell's own `Cargo.toml`),
/// so the parameter type is `Context::ptr_type`, not `IntType::ptr_type`.
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
/// [`Interp::compile_scc`] forward-declares every member of a mutually
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
    let existing = module.borrow().get_function(name);
    let function = existing.unwrap_or_else(|| module.borrow_mut().add_function(name, compiled_fn_type(), None));
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
    let existing = module.borrow().get_function(name);
    let function = existing.unwrap_or_else(|| module.borrow_mut().add_function(name, coroutine_fn_type(), None));
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
/// ABI but **no body** — [`Interp::compile_function`]'s JIT-only step
/// (labels/closures Stage 3, `call` to a different top-level function)
/// that lets `compiler.rs`'s `compile-call` find an already-`compile`d
/// function via `get-function` before the real call target is wired in via
/// `add_global_mapping` once the engine running this declaration's own
/// module exists (see that method's doc comment). Not exposed as an
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
/// (`compile::core_freevars::free_vars`) is non-empty, and (labels/closures
/// Stage 4) for *every* function ever wrapped into a `ClosureBox` via
/// `build-make-closure`, capturing or not — see that builtin's doc comment
/// (`registry::llvm_builder_def`) for why unifying on one ABI regardless of
/// whether a given closure actually captures anything is what lets
/// `build-closure-apply` call through it without first checking which case
/// it's in.
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
    let function = module.borrow_mut().add_function(name, compiled_fn_type_with_env(), None);
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
/// see `registry::llvm_module_def`'s doc comment on `get-function` for why
/// this is the core lookup every direct call (self-recursion, `labels`
/// siblings, top-level `defun`-to-`defun` calls) is built on.
fn llvm_module_get_function(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let module = expect_llvm_module(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let found = module.borrow().get_function(name);
    found
        .map(llvm_function_value)
        .ok_or_else(|| EvalError::Panic(format!("get-function: no function named \"{}\" in this module", name)))
}

fn llvm_function_append_block(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    let function = expect_llvm_function(&args[0])?;
    let name = expect_str(heap, &args[1])?;
    let block = crate::compile::llvm_context().append_basic_block(function, name);
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
    let n = match &args[1] {
        Value::Int(n) => *n,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
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
    let index = match &args[2] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let args_ptr = function
        .get_nth_param(0)
        .ok_or_else(|| EvalError::Internal("load-arg: function has no args parameter".into()))?
        .into_pointer_value();
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(index, false);
    let b = builder.borrow();
    let elem_ptr = unsafe {
        b.build_gep(ctx.i64_type(), args_ptr, &[idx_val], "arg_ptr")
            .map_err(|e| EvalError::Internal(format!("load-arg: {}", e)))?
    };
    let loaded =
        b.build_load(ctx.i64_type(), elem_ptr, "arg_val").map_err(|e| EvalError::Internal(format!("load-arg: {}", e)))?;
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
    let index = match &args[2] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let env_ptr = function
        .get_nth_param(2)
        .ok_or_else(|| EvalError::Internal("load-env: function has no env parameter".into()))?
        .into_pointer_value();
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(index, false);
    let b = builder.borrow();
    let elem_ptr = unsafe {
        b.build_gep(ctx.i64_type(), env_ptr, &[idx_val], "env_ptr")
            .map_err(|e| EvalError::Internal(format!("load-env: {}", e)))?
    };
    let loaded =
        b.build_load(ctx.i64_type(), elem_ptr, "env_val").map_err(|e| EvalError::Internal(format!("load-env: {}", e)))?;
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

/// Shared by `build-fadd`/`build-fsub`/`build-fmul`/`build-fdiv`/`build-frem`
/// (compiled `f64` arithmetic). A compiled `f64` value is its raw `f64::to_bits`
/// pattern carried in an `i64` register (`compile-float`'s convention — "every
/// compiled value is a plain `i64`"), so each operand is `bitcast`ed `i64` ->
/// `double` here, the `op` applied, and the `double` result `bitcast`ed back to
/// `i64` — the whole float-ness stays contained in this one instruction from
/// the surrounding IR's point of view, exactly the way a `char`'s code point
/// stays a plain `i64` everywhere but the `char->int`/`int->char` edges.
fn llvm_builder_build_float_op(
    args: &[Value],
    name: &str,
    op: impl FnOnce(&Builder<'static>, inkwell::values::FloatValue<'static>, inkwell::values::FloatValue<'static>, &str) -> Result<inkwell::values::FloatValue<'static>, inkwell::builder::BuilderError>,
) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let a_bits = expect_llvm_value(&args[1])?.into_int_value();
    let b_bits = expect_llvm_value(&args[2])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let f64_ty = ctx.f64_type();
    let err = |e: inkwell::builder::BuilderError| EvalError::Internal(format!("build-{}: {}", name, e));
    let a = bld.build_bit_cast(a_bits, f64_ty, "a_f").map_err(err)?.into_float_value();
    let b = bld.build_bit_cast(b_bits, f64_ty, "b_f").map_err(err)?.into_float_value();
    let result = op(&bld, a, b, name).map_err(err)?;
    let bits = bld.build_bit_cast(result, ctx.i64_type(), name).map_err(err)?;
    Ok(llvm_value_value(bits))
}

/// The `f64` comparison counterpart of [`llvm_builder_build_int_op`]/
/// [`llvm_builder_build_icmp`] combined: `bitcast` both `i64`-carried operands
/// to `double`, `fcmp` with `predicate`, then zero-extend the `i1` result to
/// the `i64` every compiled value is (`build-icmp`'s own widening step).
/// `<`/`<=`/`>`/`>=` use the *ordered* predicates (`OLT`/... — false if either
/// operand is NaN, matching Rust's `<`/... the interpreter's `eval_float_builtin`
/// uses); `=`/`eq`/`eql`/`equal`/`equalp` use `OEQ` (NaN never equals NaN) and
/// `/=` uses `UNE` (Rust's `!=` is `!(a == b)`, true when either is NaN).
fn llvm_builder_build_fcmp(args: &[Value], name: &str, predicate: inkwell::FloatPredicate) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let a_bits = expect_llvm_value(&args[1])?.into_int_value();
    let b_bits = expect_llvm_value(&args[2])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let f64_ty = ctx.f64_type();
    let err = |e: inkwell::builder::BuilderError| EvalError::Internal(format!("{}: {}", name, e));
    let a = bld.build_bit_cast(a_bits, f64_ty, "a_f").map_err(err)?.into_float_value();
    let b = bld.build_bit_cast(b_bits, f64_ty, "b_f").map_err(err)?.into_float_value();
    let cmp = bld.build_float_compare(predicate, a, b, name).map_err(err)?;
    let widened = bld.build_int_z_extend(cmp, ctx.i64_type(), name).map_err(err)?;
    Ok(llvm_value_value(widened.into()))
}

/// The unary transcendental/rounding counterpart of
/// [`llvm_builder_build_float_op`]: `bitcast` the single `i64`-carried
/// operand to `double`, call the named LLVM intrinsic (`llvm.sqrt.f64`/
/// `llvm.floor.f64`/`llvm.ceil.f64`/`llvm.round.f64`/`llvm.trunc.f64`), then
/// `bitcast` the `double` result back. Unlike `build-fadd`/... (plain LLVM
/// instructions), these have no dedicated IR opcode, so they go through
/// `module`'s intrinsic declaration (`Intrinsic::get_declaration`, itself
/// idempotent — safe to call again for a later use of the same op in the
/// same module, same as `get-function` finding an already-declared `rt_*`
/// shim) rather than `get-function`'s fixed-ABI `rt_*` lookup. `round`
/// matches Rust's `f64::round` (`float-native-method?`'s uncompiled
/// fallback, kept in sync by `tests/compile_test.rs`): both round halfway
/// cases away from zero, not to even. `sqrt`/`floor`/`ceil`/`trunc` are
/// exact IEEE-754 operations with no rounding-mode ambiguity to begin with.
fn llvm_builder_build_float_unary_intrinsic(args: &[Value], name: &str, intrinsic_name: &str) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let x_bits = expect_llvm_value(&args[2])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let f64_ty = ctx.f64_type();
    let err = |e: String| EvalError::Internal(format!("build-{}: {}", name, e));
    let x = bld.build_bit_cast(x_bits, f64_ty, "x_f").map_err(|e| err(e.to_string()))?.into_float_value();
    let intrinsic = inkwell::intrinsics::Intrinsic::find(intrinsic_name)
        .ok_or_else(|| err(format!("no such LLVM intrinsic {}", intrinsic_name)))?;
    let decl = intrinsic
        .get_declaration(&module.borrow(), &[f64_ty.into()])
        .ok_or_else(|| err(format!("failed to declare {}", intrinsic_name)))?;
    let call = bld.build_call(decl, &[x.into()], name).map_err(|e| err(e.to_string()))?;
    let result = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_float_value(),
        inkwell::values::ValueKind::Instruction(_) => return Err(err(format!("{} produced no value", intrinsic_name))),
    };
    let bits = bld.build_bit_cast(result, ctx.i64_type(), name).map_err(|e| err(e.to_string()))?;
    Ok(llvm_value_value(bits))
}

/// `expt` (`f64,f64->f64`): the binary counterpart of
/// [`llvm_builder_build_float_unary_intrinsic`], `llvm.pow.f64` — matches
/// the interpreter's `f64::powf` (`float_expt`), both ultimately the
/// platform libm `pow` either way.
/// Shared by `build-fpow`/`build-fmaxnum`/`build-fminnum` — every binary
/// `f64` LLVM intrinsic this project uses. Each `i64`-carried operand is
/// `bitcast`ed to `double`, the intrinsic (overloaded on its `f64` operand
/// type, hence the `module` parameter — same reason
/// [`llvm_builder_build_float_unary_intrinsic`] takes one) applied, and the
/// `double` result `bitcast`ed back to `i64`.
fn llvm_builder_build_float_binary_intrinsic(args: &[Value], name: &str, intrinsic_name: &str) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let a_bits = expect_llvm_value(&args[2])?.into_int_value();
    let b_bits = expect_llvm_value(&args[3])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let f64_ty = ctx.f64_type();
    let err = |e: String| EvalError::Internal(format!("build-{}: {}", name, e));
    let a = bld.build_bit_cast(a_bits, f64_ty, "a_f").map_err(|e| err(e.to_string()))?.into_float_value();
    let b = bld.build_bit_cast(b_bits, f64_ty, "b_f").map_err(|e| err(e.to_string()))?.into_float_value();
    let intrinsic = inkwell::intrinsics::Intrinsic::find(intrinsic_name).ok_or_else(|| err(format!("no such LLVM intrinsic {}", intrinsic_name)))?;
    let decl = intrinsic
        .get_declaration(&module.borrow(), &[f64_ty.into()])
        .ok_or_else(|| err(format!("failed to declare {}", intrinsic_name)))?;
    let call = bld.build_call(decl, &[a.into(), b.into()], name).map_err(|e| err(e.to_string()))?;
    let result = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_float_value(),
        inkwell::values::ValueKind::Instruction(_) => return Err(err(format!("{} produced no value", intrinsic_name))),
    };
    let bits = bld.build_bit_cast(result, ctx.i64_type(), name).map_err(|e| err(e.to_string()))?;
    Ok(llvm_value_value(bits))
}

fn llvm_builder_build_fpow(args: &[Value]) -> Result<Value, EvalError> {
    llvm_builder_build_float_binary_intrinsic(args, "fpow", "llvm.pow.f64")
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

/// `float->int` (`f64->i32`, narrowing, truncating toward zero): `bitcast`
/// the `i64`-carried operand to `double`, then the `llvm.fptosi.sat`
/// intrinsic (overloaded on both its result and its `f64` operand type,
/// hence the `module` parameter every other overloaded-intrinsic builtin
/// here already takes — see [`llvm_builder_build_float_unary_intrinsic`]/
/// [`llvm_builder_build_fpow`]) to `i32`, then `sext` back to the full `i64`
/// register every compiled integer is carried in (see
/// `int-native-method?`'s doc comment).
///
/// Saturating to `i32` and not to the register: `float->int`'s declared
/// return type *is* `i32`, and a value in a register always holds the number
/// its type claims, sign-extended (`types::normalize_int`). Unlike a plain
/// `fptosi` instruction (poison on NaN/out-of-range input), `.sat` clamps:
/// NaN -> 0, `+inf`/an overflowing magnitude -> `i32::MAX`, `-inf`/an
/// underflowing magnitude -> `i32::MIN` — exactly Rust's `as` cast
/// semantics, matching the interpreter's `float_to_int` (`f as i32`) bit for
/// bit.
fn llvm_builder_build_fptosi(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let x_bits = expect_llvm_value(&args[2])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let f64_ty = ctx.f64_type();
    let i32_ty = ctx.i32_type();
    let i64_ty = ctx.i64_type();
    let err = |e: String| EvalError::Internal(format!("build-fptosi: {}", e));
    let x = bld.build_bit_cast(x_bits, f64_ty, "x_f").map_err(|e| err(e.to_string()))?.into_float_value();
    let intrinsic =
        inkwell::intrinsics::Intrinsic::find("llvm.fptosi.sat").ok_or_else(|| err("no such LLVM intrinsic llvm.fptosi.sat".into()))?;
    let decl = intrinsic
        .get_declaration(&module.borrow(), &[i32_ty.into(), f64_ty.into()])
        .ok_or_else(|| err("failed to declare llvm.fptosi.sat".into()))?;
    let call = bld.build_call(decl, &[x.into()], "fptosi_sat").map_err(|e| err(e.to_string()))?;
    let narrow = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
        inkwell::values::ValueKind::Instruction(_) => return Err(err("llvm.fptosi.sat produced no value".into())),
    };
    let result = bld.build_int_s_extend(narrow, i64_ty, "fptosi_sext").map_err(|e| err(e.to_string()))?;
    Ok(llvm_value_value(result.into()))
}

/// `int->float`: `sitofp` the operand (every compiled integer is carried in a
/// full `i64` register, whatever its static width) to `double`, then
/// `bitcast` it back to the raw word a compiled `f64` is carried in — the
/// exact inverse of [`llvm_builder_build_fptosi`]'s opening `bitcast`.
///
/// No saturating intrinsic and no `module` parameter: `sitofp` is total on
/// every input, so there is nothing to clamp, which matches the interpreter's
/// `int_to_float` (`n as f64`).
fn llvm_builder_build_sitofp(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let x = expect_llvm_value(&args[1])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let err = |e: String| EvalError::Internal(format!("build-sitofp: {}", e));
    let f = bld.build_signed_int_to_float(x, ctx.f64_type(), "x_f").map_err(|e| err(e.to_string()))?;
    let bits = bld.build_bit_cast(f, ctx.i64_type(), "x_bits").map_err(|e| err(e.to_string()))?;
    Ok(llvm_value_value(bits.into_int_value().into()))
}

/// The nearest binary32 value, still carried as an `f64`: `fptrunc` to
/// `float`, then `fpext` straight back to `double`.
///
/// This is what makes `f32` a width rather than a label. A float value lives
/// in a 64-bit word whichever type names it — every binary32 value is exactly
/// a binary64 value — but an `f32` *operation* rounds its result to binary32,
/// and this is that rounding. The interpreter's `float_at` does the same
/// thing as `v as f32 as f64`.
fn llvm_builder_build_fround32(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let x_bits = expect_llvm_value(&args[1])?.into_int_value();
    let bld = builder.borrow();
    let ctx = crate::compile::llvm_context();
    let err = |e: String| EvalError::Internal(format!("build-fround32: {}", e));
    let x = bld.build_bit_cast(x_bits, ctx.f64_type(), "x_f").map_err(|e| err(e.to_string()))?.into_float_value();
    let narrow = bld.build_float_trunc(x, ctx.f32_type(), "x_f32").map_err(|e| err(e.to_string()))?;
    let wide = bld.build_float_ext(narrow, ctx.f64_type(), "x_f64").map_err(|e| err(e.to_string()))?;
    let bits = bld.build_bit_cast(wide, ctx.i64_type(), "x_bits").map_err(|e| err(e.to_string()))?;
    Ok(llvm_value_value(bits.into_int_value().into()))
}

/// Stack-allocates a `[count x i64]` array and returns its base pointer, to
/// be filled in by `store-arg` and passed to `build-call` — the compiled-IR
/// equivalent of building the `i64* args` array every compiled function's
/// fixed ABI expects (see `llvm_module_add_function`'s doc comment). Opaque
/// pointers (LLVM 17's default) carry no element-type info of their own, so
/// this pointer is usable as a flat `i64*` exactly the way `load_arg`'s own
/// `args_ptr` parameter already is — every GEP against it supplies
/// `ctx.i64_type()` itself, regardless of the alloca's nominal array type.
fn llvm_builder_alloca_args(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let count = match &args[1] {
        Value::Int(n) => *n as u32,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let array_ty = ctx.i64_type().array_type(count);
    let b = builder.borrow();
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
    // stack-slot handling expects.
    //
    // What it does *not* buy is survival across a suspension. The machine
    // frame is torn down when the function hands control back, so an `alloca`
    // holds nothing on the way in again. Only a value that is stored and read
    // without a call in between may live here; anything that has to outlive a
    // call belongs in a frame slot.
    let here = b.get_insert_block();
    let entry = here
        .and_then(|blk| blk.get_parent())
        .and_then(|f| f.get_first_basic_block())
        .ok_or_else(|| EvalError::Internal("alloca-args: the builder is not inside a function".to_string()))?;
    match entry.get_terminator() {
        Some(term) => b.position_before(&term),
        None => b.position_at_end(entry),
    }
    let ptr =
        b.build_alloca(array_ty, "call_args").map_err(|e| EvalError::Internal(format!("alloca-args: {}", e)))?;
    if let Some(blk) = here {
        b.position_at_end(blk);
    }
    Ok(llvm_value_value(ptr.into()))
}

/// Writes `value` into slot `index` of an `alloca-args` array — the same
/// GEP pattern `load_arg` uses to *read* a logical argument, just paired
/// with a store instead of a load.
fn llvm_builder_store_arg(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let array_ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let index = match &args[2] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let value = expect_llvm_value(&args[3])?.into_int_value();
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(index, false);
    let b = builder.borrow();
    let elem_ptr = unsafe {
        b.build_gep(ctx.i64_type(), array_ptr, &[idx_val], "store_arg_ptr").map_err(|e| EvalError::Internal(format!("store-arg: {}", e)))?
    };
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
    let index = match &args[2] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(index, false);
    let b = builder.borrow();
    let elem_ptr = unsafe {
        b.build_gep(ctx.i64_type(), array_ptr, &[idx_val], "load_raw_ptr").map_err(|e| EvalError::Internal(format!("load-raw: {}", e)))?
    };
    let loaded = b.build_load(ctx.i64_type(), elem_ptr, "load_raw_val").map_err(|e| EvalError::Internal(format!("load-raw: {}", e)))?;
    Ok(llvm_value_value(loaded))
}

/// [`llvm_builder_load_raw`]'s GEP without the load: the *address* of one
/// word in an `i64` array.
///
/// A compiled frame is an `i64` array (`BoxedObj::Frame`), and a local is one
/// word in it. Everything that already handles a local —
/// `resolve-value`/`compile-set`/`retain-bindings` — speaks in pointers,
/// because a local has been a 1-element `alloca-args` slot since `setf` and
/// loop re-entry needed somewhere to write. So carving the slot out of a frame
/// instead of allocating it on the machine stack is the whole change, and this
/// is the one primitive it needs.
fn llvm_builder_build_slot_ptr(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let array_ptr = expect_llvm_value(&args[1])?.into_pointer_value();
    let index = match &args[2] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let idx_val = ctx.i64_type().const_int(index, false);
    let b = builder.borrow();
    let elem_ptr = unsafe {
        b.build_gep(ctx.i64_type(), array_ptr, &[idx_val], "slot_ptr")
            .map_err(|e| EvalError::Internal(format!("build-slot-ptr: {}", e)))?
    };
    Ok(llvm_value_value(elem_ptr.into()))
}

// ---- the frame under construction ---------------------------------------
//
// Phase C1. Locals move off the machine stack into a `BoxedObj::Frame`, and
// that needs two things the island cannot hold: a slot counter, and the
// frame's size — which is not known until the body has been emitted, because
// the body is what allocates the slots.
//
// **The builder is the key.** Each of the three places that builds a function
// (`compile-function`, `compile-lambda`, `compile-labels-bodies`) creates its
// own `llvm-builder::create`, so a builder is one-to-one with the function
// under construction. Keying this state on it is why none of the island's 56
// signatures grows an argument — `compile-value` already carries 16, and this
// phase is supposed to shorten that list, not lengthen it.
//
// This is the same division `alloca-args` already draws: the island decides
// *which* binding needs storage and when, Rust knows *where* the storage is.

/// The frame being built for one function, keyed by its builder.
struct FrameCtx {
    /// The tagged frame, for `rt_frame_mask_bit`.
    frame: BasicValueEnum<'static>,
    /// The address of its word array, to GEP slots out of.
    data: PointerValue<'static>,
    /// The `store` holding the slot count `rt_frame_new` reads. Its value
    /// operand starts at zero and is replaced by [`llvm_builder_frame_end`]
    /// once the true count is known.
    size_store: InstructionValue<'static>,
    /// How many slots have been handed out.
    next: u64,
    /// The coroutine prologue's parts, absent under the original ABI.
    coro: Option<CoroCtx>,
}

/// What the coroutine prologue leaves behind for the rest of the function.
///
/// A resume point is a basic block that only the dispatch chain branches to,
/// so the chain cannot be written until every call site has been emitted and
/// claimed its id — the same shape as the slot count, and patched the same
/// way: [`llvm_builder_coroutine_begin`] leaves `dispatch` empty and
/// [`llvm_builder_coroutine_end`] fills it.
struct CoroCtx {
    /// The function being built, to append resume blocks to.
    function: FunctionValue<'static>,
    /// Where the body starts — the dispatch chain's `pc == 0` arm.
    body: BasicBlock<'static>,
    /// Left empty by `coroutine-begin`, filled by `coroutine-end`.
    dispatch: BasicBlock<'static>,
    /// `rt_frame_pc(frame)`, read once in the prologue so it dominates the
    /// whole chain.
    pc: IntValue<'static>,
    /// Every call site's id and the block it resumes into, in the order they
    /// were claimed.
    resumes: Vec<(u64, BasicBlock<'static>)>,
    /// The `br` ending `coro.fresh`, to insert once-per-activation setup
    /// before — a slot's mask bit is a fact about the frame, so it belongs on
    /// the path that makes the frame, not on every resume.
    fresh_end: InstructionValue<'static>,
    /// The frame as `coro.fresh` knows it, for those mask-bit calls.
    fresh_frame: BasicValueEnum<'static>,
    /// The `br` ending `coro.prologue`, to insert slot *addresses* before.
    ///
    /// A slot address is a GEP off the frame's data pointer, and the resume
    /// blocks use it — so it has to be defined where both the fresh and the
    /// resuming entry can see it. Emitting it where the island asked would
    /// put it in whatever block the binding happened to be in, which no
    /// resume block is dominated by.
    prologue_end: InstructionValue<'static>,
}

thread_local! {
    /// Keyed by `Rc::as_ptr` of the builder. An entry lives from `frame-begin`
    /// to `frame-end`, which is exactly one function's construction; the three
    /// call sites each drop their builder afterwards, so nothing accumulates.
    static FRAME_CTXS: RefCell<HashMap<usize, FrameCtx>> = RefCell::new(HashMap::new());
}

fn builder_key(b: &Rc<RefCell<Builder<'static>>>) -> usize {
    Rc::as_ptr(b) as usize
}

/// Get-or-create a bodyless `rt_*` declaration, for the same reason
/// [`llvm_module_add_function`] is get-or-create: LLVM's `LLVMAddFunction`
/// does not merge a colliding name, it uniquifies it to `name.1` — and a
/// second, differently-named declaration is a symbol the JIT will not resolve.
fn ensure_declared(module: &Rc<RefCell<Module<'static>>>, name: &str) -> FunctionValue<'static> {
    let existing = module.borrow().get_function(name);
    match existing {
        Some(f) => f,
        None => module.borrow_mut().add_function(name, compiled_fn_type(), None),
    }
}

/// `(frame-begin builder m)` — emit a function's frame prologue and return the
/// tagged frame, which the caller must root.
///
/// The size passed to `rt_frame_new` is a placeholder zero. It cannot be
/// anything else yet: the count is what the body turns out to need, and the
/// body has not been emitted. [`llvm_builder_frame_end`] replaces it in place
/// once it is known, so the count in the emitted IR is by construction the
/// number of slots that were actually handed out — there is no second pass
/// over the body to keep in agreement with the first.
fn llvm_builder_frame_begin(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let err = |what: &str, e: String| EvalError::Internal(format!("frame-begin: {}: {}", what, e));

    // The prologue declares what it emits. A module reaching `compile-function`
    // need not have been through the driver — `tests/compile_test.rs` builds
    // bare ones with `llvm-module::create` — and until now nothing in the
    // prologue was unconditional, so those modules were never asked for an
    // `rt_*` declaration they had not made. `frame-begin` is emitted for every
    // function, so it owns its own dependencies rather than making every
    // caller know them.
    let frame_new = ensure_declared(&module, "rt_frame_new");
    let frame_data = ensure_declared(&module, "rt_frame_data");
    let push_root = ensure_declared(&module, "rt_push_sexpr_root");
    ensure_declared(&module, "rt_pop_sexpr_root");
    ensure_declared(&module, "rt_frame_mask_bit");

    let b = builder.borrow();
    // rt_frame_new(&[0])
    let size_args = b
        .build_alloca(i64t.array_type(1), "frame_size_args")
        .map_err(|e| err("alloca", e.to_string()))?;
    let size_slot = unsafe {
        b.build_gep(i64t, size_args, &[i64t.const_int(0, false)], "frame_size_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    let size_store = b
        .build_store(size_slot, i64t.const_int(0, false))
        .map_err(|e| err("store", e.to_string()))?;
    let frame = match b
        .build_call(frame_new, &[size_args.into(), ctx.i32_type().const_int(1, false).into()], "frame")
        .map_err(|e| err("call rt_frame_new", e.to_string()))?
        .try_as_basic_value()
    {
        inkwell::values::ValueKind::Basic(v) => v,
        inkwell::values::ValueKind::Instruction(_) => {
            return Err(EvalError::Internal("frame-begin: rt_frame_new returned no value".to_string()))
        }
    };

    // rt_frame_data(&[frame])
    let data_args = b
        .build_alloca(i64t.array_type(1), "frame_data_args")
        .map_err(|e| err("alloca", e.to_string()))?;
    let data_slot = unsafe {
        b.build_gep(i64t, data_args, &[i64t.const_int(0, false)], "frame_data_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    b.build_store(data_slot, frame).map_err(|e| err("store", e.to_string()))?;
    let data_word = match b
        .build_call(frame_data, &[data_args.into(), ctx.i32_type().const_int(1, false).into()], "frame_data")
        .map_err(|e| err("call rt_frame_data", e.to_string()))?
        .try_as_basic_value()
    {
        inkwell::values::ValueKind::Basic(v) => v,
        inkwell::values::ValueKind::Instruction(_) => {
            return Err(EvalError::Internal("frame-begin: rt_frame_data returned no value".to_string()))
        }
    };
    let data = b
        .build_int_to_ptr(data_word.into_int_value(), ctx.ptr_type(inkwell::AddressSpace::default()), "frame_ptr")
        .map_err(|e| err("inttoptr", e.to_string()))?;

    // rt_push_sexpr_root(frame) — the frame holds every collectable local, so
    // nothing in the function is reachable until the frame itself is. One push
    // per activation, where there used to be one per binding.
    let root_args = b.build_alloca(i64t.array_type(1), "frame_root_args").map_err(|e| err("alloca", e.to_string()))?;
    let root_slot = unsafe {
        b.build_gep(i64t, root_args, &[i64t.const_int(0, false)], "frame_root_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    b.build_store(root_slot, frame).map_err(|e| err("store", e.to_string()))?;
    b.build_call(push_root, &[root_args.into(), ctx.i32_type().const_int(1, false).into()], "")
        .map_err(|e| err("call rt_push_sexpr_root", e.to_string()))?;

    FRAME_CTXS.with(|c| {
        // `next: 1` — slot 0 is the driver protocol's value slot
        // (`typelisp_abi::FRAME_VALUE_SLOT`) and is never handed out as a
        // local. A function's result leaves through it, and the result of a
        // call it is waiting on arrives through it: from the resuming
        // function's side those are the same thing, the answer it was waiting
        // for.
        c.borrow_mut().insert(builder_key(&builder), FrameCtx { data, frame, size_store, next: 1, coro: None });
    });
    Ok(Value::Empty)
}

/// `(frame-value builder)` — the tagged frame this function is building into.
///
/// [`llvm_builder_frame_begin`] returns `()` because most of what a prologue
/// needs is already keyed on the builder, but the driver protocol has to name
/// the frame itself: `rt_frame_set_pc` and `rt_frame_entered` both take it.
fn llvm_builder_frame_value(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let key = builder_key(&builder);
    FRAME_CTXS.with(|c| {
        let map = c.borrow();
        let f = map
            .get(&key)
            .ok_or_else(|| EvalError::Internal("frame-value: no frame is open on this builder".to_string()))?;
        Ok(llvm_value_value(f.frame))
    })
}

/// `(frame-slot builder)` — the address of a fresh, **unmasked** frame slot.
///
/// Unmasked means the collector never reads it as a reference, which is what a
/// raw `i32`/`f64` local needs (`Repr::class`'s non-collectable case).
fn llvm_builder_frame_slot(args: &[Value]) -> Result<Value, EvalError> {
    frame_slot_impl(args, false)
}

/// `(frame-slot-rooted builder m)` — the address of a fresh frame slot, marked
/// so the collector traces it.
///
/// The `rt_frame_mask_bit` call this emits stands exactly where that binding's
/// `rt_push_sexpr_root` used to, and there is no pop to match it: the slot is
/// a root for as long as the frame lives, so no early exit can leave the root
/// stack out of step.
fn llvm_builder_frame_slot_rooted(args: &[Value]) -> Result<Value, EvalError> {
    frame_slot_impl(args, true)
}

fn frame_slot_impl(args: &[Value], rooted: bool) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let key = builder_key(&builder);

    let (frame, data, idx, coro) = FRAME_CTXS.with(|c| {
        let mut map = c.borrow_mut();
        let f = map
            .get_mut(&key)
            .ok_or_else(|| EvalError::Internal("frame-slot: no frame is open on this builder".to_string()))?;
        let idx = f.next;
        f.next += 1;
        let coro = f.coro.as_ref().map(|c| (c.fresh_end, c.fresh_frame, c.prologue_end));
        Ok::<_, EvalError>((f.frame, f.data, idx, coro))
    })?;

    // Under the coroutine ABI a slot's address and its mask bit both move out
    // of the block that asked for them. The address goes to `coro.prologue`,
    // the one block every entry falls into, because the resume blocks use it
    // and nothing in the body dominates them. The mask bit goes to
    // `coro.fresh`, because marking a slot is a fact about the frame and the
    // frame is made exactly once.
    //
    // Setting the bit before the slot is written is safe by construction: an
    // untouched slot reads as a raw `0`, whose low bits are `TAG_FIXNUM`, so
    // the collector follows nothing.
    if let Some((fresh_end, fresh_frame, prologue_end)) = coro {
        {
            let b = builder.borrow();
            let here = b.get_insert_block();
            if rooted {
                let module = expect_llvm_module(&args[1])?;
                let mask_fn = ensure_declared(&module, "rt_frame_mask_bit");
                b.position_before(&fresh_end);
                emit_rt_call(&b, mask_fn, &[fresh_frame, i64t.const_int(idx, false).into()], "mask", "frame-slot")?;
            }
            b.position_before(&prologue_end);
            let elem_ptr = unsafe {
                b.build_gep(i64t, data, &[i64t.const_int(idx, false)], "frame_slot")
                    .map_err(|e| EvalError::Internal(format!("frame-slot: gep: {}", e)))?
            };
            if let Some(block) = here {
                b.position_at_end(block);
            }
            return Ok(llvm_value_value(elem_ptr.into()));
        }
    }

    let b = builder.borrow();
    if rooted {
        let module = expect_llvm_module(&args[1])?;
        let mask_fn = ensure_declared(&module, "rt_frame_mask_bit");
        let err = |what: &str, e: String| EvalError::Internal(format!("frame-slot-rooted: {}: {}", what, e));
        let mask_args = b.build_alloca(i64t.array_type(2), "mask_args").map_err(|e| err("alloca", e.to_string()))?;
        let a0 = unsafe {
            b.build_gep(i64t, mask_args, &[i64t.const_int(0, false)], "mask_a0").map_err(|e| err("gep", e.to_string()))?
        };
        b.build_store(a0, frame).map_err(|e| err("store", e.to_string()))?;
        let a1 = unsafe {
            b.build_gep(i64t, mask_args, &[i64t.const_int(1, false)], "mask_a1").map_err(|e| err("gep", e.to_string()))?
        };
        b.build_store(a1, i64t.const_int(idx, false)).map_err(|e| err("store", e.to_string()))?;
        b.build_call(mask_fn, &[mask_args.into(), ctx.i32_type().const_int(2, false).into()], "")
            .map_err(|e| err("call rt_frame_mask_bit", e.to_string()))?;
    }
    let elem_ptr = unsafe {
        b.build_gep(i64t, data, &[i64t.const_int(idx, false)], "frame_slot")
            .map_err(|e| EvalError::Internal(format!("frame-slot: gep: {}", e)))?
    };
    Ok(llvm_value_value(elem_ptr.into()))
}

/// `(frame-end builder m)` — write the true slot count into the prologue,
/// drop the frame's own GC root, and close the frame.
///
/// The pair with [`llvm_builder_frame_begin`] owns the frame's whole lifetime,
/// which is why the island's three function-building sites are one line each
/// at either end. It is also why the root push and pop are emitted here rather
/// than by the island: a caller that had to remember them could forget one,
/// and an unbalanced root stack is exactly the class of bug this phase exists
/// to remove.
fn llvm_builder_frame_end(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let ctx = crate::compile::llvm_context();
    let key = builder_key(&builder);
    let f = FRAME_CTXS.with(|c| {
        c.borrow_mut()
            .remove(&key)
            .ok_or_else(|| EvalError::Internal("frame-end: no frame is open on this builder".to_string()))
    })?;
    if !f.size_store.set_operand(0, ctx.i64_type().const_int(f.next, false)) {
        return Err(EvalError::Internal("frame-end: could not write the slot count into the prologue".to_string()));
    }
    let pop_root = ensure_declared(&module, "rt_pop_sexpr_root");
    let b = builder.borrow();
    let empty = b
        .build_alloca(ctx.i64_type().array_type(0), "frame_unroot_args")
        .map_err(|e| EvalError::Internal(format!("frame-end: alloca: {}", e)))?;
    b.build_call(pop_root, &[empty.into(), ctx.i32_type().const_int(0, false).into()], "")
        .map_err(|e| EvalError::Internal(format!("frame-end: call rt_pop_sexpr_root: {}", e)))?;
    Ok(Value::Empty)
}

// ---------------------------------------------------------------------------
// The coroutine prologue (Phase C2)
// ---------------------------------------------------------------------------
//
// `frame-begin`/`frame-end` above build a frame inside a function that still
// runs to completion. The three below build one that can stop in the middle,
// and the difference is entirely in the *shape of the function*, not in the
// frame: a coroutine body is entered again, from the top, every time it
// resumes, and has to find its way back to where it left off.
//
// That is why the prologue is not a straight line. Two things it computes —
// the frame and the pointer into it — are used by resume blocks that the
// entry path never reaches, so they cannot be computed on the entry path:
// SSA requires a definition to dominate its uses. They are computed in a
// `coro.prologue` block that both the fresh and the resuming entry fall into,
// which is what makes it dominate everything after.
//
// The frame arrives through a cell rather than a phi node: an entry-block
// `alloca` is one address for the whole activation, so "the frame I made" and
// "the frame I was handed" merge by storing to the same place. The island has
// no way to build a phi, and would not want one — this is the same trick
// `alloca-args` already plays for every other value that has to outlive a
// branch.

/// Emits a call to an `rt_*` entry point with `vals` as its argument array.
///
/// Every one of them takes `(const i64 *args, u32 argc)`, so the array is an
/// `alloca` filled by hand — the same three lines repeated at a dozen sites
/// above, factored out here because the coroutine prologue makes eight such
/// calls.
fn emit_rt_call<'a>(
    b: &Builder<'static>,
    f: FunctionValue<'static>,
    vals: &[BasicValueEnum<'static>],
    name: &str,
    what: &'a str,
) -> Result<Option<BasicValueEnum<'static>>, EvalError> {
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let arr = b
        .build_alloca(i64t.array_type(vals.len().max(1) as u32), &format!("{}_args", name))
        .map_err(|e| err("alloca", e.to_string()))?;
    for (i, v) in vals.iter().enumerate() {
        let slot = unsafe {
            b.build_gep(i64t, arr, &[i64t.const_int(i as u64, false)], &format!("{}_a{}", name, i))
                .map_err(|e| err("gep", e.to_string()))?
        };
        b.build_store(slot, *v).map_err(|e| err("store", e.to_string()))?;
    }
    let call = b
        .build_call(f, &[arr.into(), ctx.i32_type().const_int(vals.len() as u64, false).into()], name)
        .map_err(|e| err("call", e.to_string()))?;
    Ok(match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Some(v),
        inkwell::values::ValueKind::Instruction(_) => None,
    })
}

/// `(coroutine-begin builder m f)` — emit the prologue of a coroutine-ABI
/// function and leave the builder at the start of its body.
///
/// ```text
/// entry:      %cell = alloca i64;  br (%param == 0), fresh, resumed
/// fresh:      %f = rt_frame_new(0);  store %f -> %cell
///             rt_push_sexpr_root(%f);  rt_frame_entered(%f);  br prologue
/// resumed:    store %param -> %cell;  br prologue
/// prologue:   %frame = load %cell;  %data = rt_frame_data(%frame)
///             %pc = rt_frame_pc(%frame);  br dispatch
/// dispatch:   (empty — `coroutine-end` writes the chain)
/// body:       ...
/// ```
///
/// `rt_frame_entered` is how the driver learns the frame at all: it is made
/// *inside* the callee, after the call has begun, so there is no other moment
/// the caller could have been told.
///
/// The root push has no pop here and its pop is not on this path — one push
/// on first entry, one pop at the single return. Between them the function may
/// hand control back to the driver any number of times, and the frames of a
/// call chain nest, so the root stack stays in order. That stops being true
/// once a *task* can be put down mid-chain (C3), and the frame's own root
/// moves to `FrameStack::roots()` then.
fn llvm_builder_coroutine_begin(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let function = expect_llvm_function(&args[2])?;
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let what = "coroutine-begin";
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));

    // The prologue owns its declarations for the same reason `frame-begin`
    // does: a module can reach here without having been through the driver.
    let frame_new = ensure_declared(&module, "rt_frame_new");
    let frame_data = ensure_declared(&module, "rt_frame_data");
    let frame_pc = ensure_declared(&module, "rt_frame_pc");
    let entered = ensure_declared(&module, "rt_frame_entered");
    let push_root = ensure_declared(&module, "rt_push_sexpr_root");
    ensure_declared(&module, "rt_pop_sexpr_root");
    ensure_declared(&module, "rt_frame_mask_bit");
    ensure_declared(&module, "rt_frame_set_pc");
    ensure_declared(&module, "rt_frame_call");
    ensure_declared(&module, "rt_pending_arg");
    ensure_declared(&module, "rt_pending_argc");

    let param = function
        .get_nth_param(0)
        .ok_or_else(|| EvalError::Internal(format!("{}: the function takes no frame parameter", what)))?
        .into_int_value();

    let fresh = ctx.append_basic_block(function, "coro.fresh");
    let resumed = ctx.append_basic_block(function, "coro.resumed");
    let prologue = ctx.append_basic_block(function, "coro.prologue");
    let dispatch = ctx.append_basic_block(function, "coro.dispatch");
    let body = ctx.append_basic_block(function, "coro.body");

    let b = builder.borrow();

    // entry — the block the caller positioned us at.
    let cell = b.build_alloca(i64t, "coro_frame_cell").map_err(|e| err("alloca", e.to_string()))?;
    let is_first = b
        .build_int_compare(inkwell::IntPredicate::EQ, param, i64t.const_int(0, false), "coro_first")
        .map_err(|e| err("icmp", e.to_string()))?;
    b.build_conditional_branch(is_first, fresh, resumed).map_err(|e| err("br", e.to_string()))?;

    // fresh — make the frame this activation runs in. The size is a
    // placeholder; `coroutine-end` writes the true count in.
    b.position_at_end(fresh);
    let size_args = b.build_alloca(i64t.array_type(1), "frame_size_args").map_err(|e| err("alloca", e.to_string()))?;
    let size_slot = unsafe {
        b.build_gep(i64t, size_args, &[i64t.const_int(0, false)], "frame_size_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    let size_store =
        b.build_store(size_slot, i64t.const_int(0, false)).map_err(|e| err("store", e.to_string()))?;
    let made = match b
        .build_call(frame_new, &[size_args.into(), ctx.i32_type().const_int(1, false).into()], "frame")
        .map_err(|e| err("call rt_frame_new", e.to_string()))?
        .try_as_basic_value()
    {
        inkwell::values::ValueKind::Basic(v) => v,
        inkwell::values::ValueKind::Instruction(_) => {
            return Err(EvalError::Internal(format!("{}: rt_frame_new returned no value", what)))
        }
    };
    b.build_store(cell, made).map_err(|e| err("store", e.to_string()))?;
    emit_rt_call(&b, push_root, &[made], "frame_root", what)?;
    emit_rt_call(&b, entered, &[made], "frame_entered", what)?;
    let fresh_end = b.build_unconditional_branch(prologue).map_err(|e| err("br", e.to_string()))?;

    // resumed — the driver handed back the frame we made last time.
    b.position_at_end(resumed);
    b.build_store(cell, param).map_err(|e| err("store", e.to_string()))?;
    b.build_unconditional_branch(prologue).map_err(|e| err("br", e.to_string()))?;

    // prologue — the one block both entries reach, so the only place the
    // frame and its data pointer can be defined.
    b.position_at_end(prologue);
    let frame = b.build_load(i64t, cell, "coro_frame").map_err(|e| err("load", e.to_string()))?;
    let data_word = emit_rt_call(&b, frame_data, &[frame], "frame_data", what)?
        .ok_or_else(|| EvalError::Internal(format!("{}: rt_frame_data returned no value", what)))?;
    let data = b
        .build_int_to_ptr(data_word.into_int_value(), ctx.ptr_type(inkwell::AddressSpace::default()), "frame_ptr")
        .map_err(|e| err("inttoptr", e.to_string()))?;
    let pc = emit_rt_call(&b, frame_pc, &[frame], "frame_pc", what)?
        .ok_or_else(|| EvalError::Internal(format!("{}: rt_frame_pc returned no value", what)))?
        .into_int_value();
    let prologue_end = b.build_unconditional_branch(dispatch).map_err(|e| err("br", e.to_string()))?;

    // dispatch stays empty until every call site has claimed its id.

    b.position_at_end(body);
    FRAME_CTXS.with(|c| {
        c.borrow_mut().insert(
            builder_key(&builder),
            FrameCtx {
                data,
                frame,
                size_store,
                // Slot 0 is the driver protocol's value slot, never a local.
                next: 1,
                coro: Some(CoroCtx {
                    function,
                    body,
                    dispatch,
                    pc,
                    resumes: Vec::new(),
                    fresh_end,
                    fresh_frame: made,
                    prologue_end,
                }),
            },
        );
    });
    Ok(Value::Empty)
}

/// `(coroutine-call builder m target args-ptr argc)` — hand a call to the
/// driver and come back with its answer.
///
/// This is where a compiled call stops being an LLVM `call`. The function
/// names its callee, records where to resume, and **returns**; the driver
/// enters the callee, and enters this function again when the answer is
/// ready. Two frames exist at once, on the heap, and the machine stack is the
/// same depth as it was — which is the whole point: the recursion limit
/// becomes the heap.
///
/// The block is split here. Everything after the call in the source is
/// emitted into a fresh resume block, so the island's `compile-value` goes on
/// building at what is now a different basic block without knowing it.
///
/// The arguments are copied into `call_state` by `rt_frame_call` and live in
/// a Rust `Vec` — invisible to the collector — until the callee's prologue
/// reads them back, and `rt_frame_new` allocates in that window. They survive
/// it because the caller already keeps every collectable argument in a marked
/// frame slot of its own (`root-temporary`), which is the same reason they
/// survived the callee's allocation under the original ABI.
fn llvm_builder_coroutine_call(args: &[Value]) -> Result<Value, EvalError> {
    coroutine_call_impl(args, None)
}

/// `(coroutine-call-env builder m target args-ptr argc env-ptr env-len)` —
/// [`llvm_builder_coroutine_call`] for a callee that also has captures.
///
/// The captures are read out of the caller's array by `rt_frame_call_env`
/// before the caller returns, because that array is on a machine stack that is
/// about to go away. Under the original ABI they could be passed as a pointer
/// and read by the callee, which is the difference a call that *returns*
/// makes.
fn llvm_builder_coroutine_call_env(args: &[Value]) -> Result<Value, EvalError> {
    let env_ptr = expect_llvm_value(&args[5])?.into_pointer_value();
    let env_len = match &args[6] {
        Value::Int(n) if *n >= 0 => *n as u64,
        other => {
            return Err(EvalError::Internal(format!("coroutine-call-env: {:?} is not a capture count", other)))
        }
    };
    coroutine_call_impl(args, Some((env_ptr, env_len)))
}

fn coroutine_call_impl(
    args: &[Value],
    env: Option<(PointerValue<'static>, u64)>,
) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let target = expect_llvm_value(&args[2])?;
    let args_ptr = expect_llvm_value(&args[3])?.into_pointer_value();
    let argc = match &args[4] {
        Value::Int(n) if *n >= 0 => *n as u64,
        other => return Err(EvalError::Internal(format!("coroutine-call: {:?} is not an argument count", other))),
    };
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let what = "coroutine-call";
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let key = builder_key(&builder);

    let (frame, data, function, id) = FRAME_CTXS.with(|c| {
        let mut map = c.borrow_mut();
        let f = map
            .get_mut(&key)
            .ok_or_else(|| EvalError::Internal(format!("{}: no frame is open on this builder", what)))?;
        let coro = f
            .coro
            .as_mut()
            .ok_or_else(|| EvalError::Internal(format!("{}: this function is not a coroutine", what)))?;
        // Resume ids start at 1: 0 is "start at the top", which is what an
        // untouched `pc` already says.
        let id = coro.resumes.len() as u64 + 1;
        Ok::<_, EvalError>((f.frame, f.data, coro.function, id))
    })?;

    let resume = ctx.append_basic_block(function, &format!("coro.resume{}", id));
    let frame_call =
        ensure_declared(&module, if env.is_some() { "rt_frame_call_env" } else { "rt_frame_call" });
    let set_pc = ensure_declared(&module, "rt_frame_set_pc");

    let b = builder.borrow();
    // rt_frame_call(callee, arg...) — one array, the callee first; the
    // captures-carrying form puts the env between them.
    let mut call_args: Vec<BasicValueEnum<'static>> = Vec::with_capacity(argc as usize + 3);
    call_args.push(target);
    if let Some((env_ptr, env_len)) = env {
        call_args.push(
            b.build_ptr_to_int(env_ptr, i64t, "coro_env_word").map_err(|e| err("ptrtoint", e.to_string()))?.into(),
        );
        call_args.push(i64t.const_int(env_len, false).into());
    }
    for i in 0..argc {
        let slot = unsafe {
            b.build_gep(i64t, args_ptr, &[i64t.const_int(i, false)], "coro_arg")
                .map_err(|e| err("gep", e.to_string()))?
        };
        call_args.push(b.build_load(i64t, slot, "coro_arg_val").map_err(|e| err("load", e.to_string()))?);
    }
    emit_rt_call(&b, frame_call, &call_args, "frame_call", what)?;
    emit_rt_call(&b, set_pc, &[frame, i64t.const_int(id, false).into()], "frame_set_pc", what)?;
    b.build_return(Some(&i64t.const_int(typelisp_abi::STATUS_CALL as u64, false)))
        .map_err(|e| err("ret", e.to_string()))?;

    // Everything after the call belongs to the resume block.
    b.position_at_end(resume);
    let value_slot = unsafe {
        b.build_gep(i64t, data, &[i64t.const_int(typelisp_abi::FRAME_VALUE_SLOT as u64, false)], "coro_result_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    let result = b.build_load(i64t, value_slot, "coro_result").map_err(|e| err("load", e.to_string()))?;

    FRAME_CTXS.with(|c| {
        let mut map = c.borrow_mut();
        if let Some(coro) = map.get_mut(&key).and_then(|f| f.coro.as_mut()) {
            coro.resumes.push((id, resume));
        }
    });
    Ok(llvm_value_value(result))
}

/// `(coroutine-end builder m v)` — return `v` through the protocol, then
/// write the two things that could not be known until now.
///
/// The value leaves in the frame's slot 0 rather than in the `ret`, because
/// the `ret` is carrying the *status*. `STATUS_RETURN` is what says the frame
/// is finished; the driver pops it and hands the slot's word to whoever was
/// waiting, without looking inside it — what those bits mean is the
/// function's declared return representation, which lives in the type system
/// and nowhere else.
///
/// Then: the slot count goes into the prologue's placeholder, and the
/// dispatch chain goes into the block `coroutine-begin` left empty. Both are
/// facts about the whole body, so both are written by the only party that has
/// seen all of it.
fn llvm_builder_coroutine_end(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let value = expect_llvm_value(&args[2])?;
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let what = "coroutine-end";
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let key = builder_key(&builder);

    let f = FRAME_CTXS.with(|c| {
        c.borrow_mut()
            .remove(&key)
            .ok_or_else(|| EvalError::Internal(format!("{}: no frame is open on this builder", what)))
    })?;
    let coro = f
        .coro
        .ok_or_else(|| EvalError::Internal(format!("{}: this function is not a coroutine", what)))?;
    let pop_root = ensure_declared(&module, "rt_pop_sexpr_root");

    let b = builder.borrow();
    let value_slot = unsafe {
        b.build_gep(i64t, f.data, &[i64t.const_int(typelisp_abi::FRAME_VALUE_SLOT as u64, false)], "coro_ret_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    b.build_store(value_slot, value).map_err(|e| err("store", e.to_string()))?;
    emit_rt_call(&b, pop_root, &[], "frame_unroot", what)?;
    b.build_return(Some(&i64t.const_int(typelisp_abi::STATUS_RETURN as u64, false)))
        .map_err(|e| err("ret", e.to_string()))?;

    if !f.size_store.set_operand(0, i64t.const_int(f.next, false)) {
        return Err(EvalError::Internal(format!("{}: could not write the slot count into the prologue", what)));
    }

    // The dispatch chain: `pc == 0` is a fresh entry, anything else names a
    // call site. The last arm is unconditional — `pc` is only ever written by
    // `coroutine-call`'s own `rt_frame_set_pc`, so past the final comparison
    // there is exactly one place left it can mean.
    b.position_at_end(coro.dispatch);
    let mut arms = std::iter::once((0u64, coro.body)).chain(coro.resumes.iter().copied()).peekable();
    while let Some((id, target)) = arms.next() {
        if arms.peek().is_none() {
            b.build_unconditional_branch(target).map_err(|e| err("br", e.to_string()))?;
            break;
        }
        let next = ctx.append_basic_block(coro.function, &format!("coro.dispatch{}", id + 1));
        let hit = b
            .build_int_compare(inkwell::IntPredicate::EQ, coro.pc, i64t.const_int(id, false), "coro_is")
            .map_err(|e| err("icmp", e.to_string()))?;
        b.build_conditional_branch(hit, target, next).map_err(|e| err("br", e.to_string()))?;
        b.position_at_end(next);
    }
    Ok(Value::Empty)
}

/// Shared by every `build-icmp-*` builtin (if/let/comparisons, labels/closures
/// Stage 5): runs `icmp <predicate>` on two `i64` operands, then widens the
/// resulting `i1` back to `i64` (0/1) via `build_int_z_extend` — every other
/// builtin here treats a compiled value as a plain `i64` (see
/// `registry::llvm_module_def`'s doc comment), and a comparison result is no
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
/// otherwise — built from an `icmp ne cond, 0` plus a conditional branch, the
/// same shape [`llvm_builder_build_closure_apply`] already uses internally
/// for its own env-loop bounds check.
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

/// See `registry::llvm_builder_def`'s doc comment for `block-terminated?`.
fn llvm_builder_block_terminated(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let terminated = builder.borrow().get_insert_block().and_then(|bb| bb.get_terminator()).is_some();
    Ok(Value::Bool(terminated))
}

/// A direct call to an already-declared `target` (typically `get-function`'s
/// result), passing `args_ptr`/`argc` straight through to its fixed ABI —
/// see `registry::llvm_module_def`'s doc comment for why every compiled
/// function shares that one signature regardless of arity. This is the one
/// new primitive that unlocks every statically-resolvable direct call:
/// self-recursion, `labels`-sibling calls, and top-level `defun`-to-`defun`
/// calls alike, since all three reduce to "the callee's `llvm-function`
/// already exists in this module, look it up and call it."
fn llvm_builder_build_call(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let target = expect_llvm_function(&args[1])?;
    let args_ptr = expect_llvm_value(&args[2])?;
    let argc = match &args[3] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let argc_val = ctx.i32_type().const_int(argc, false);
    let call = builder
        .borrow()
        .build_call(target, &[args_ptr.into(), argc_val.into()], "call_result")
        .map_err(|e| EvalError::Internal(format!("build-call: {}", e)))?;
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
    let argc = match &args[3] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let env_ptr = expect_llvm_value(&args[4])?;
    let env_len = match &args[5] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let argc_val = ctx.i32_type().const_int(argc, false);
    let env_len_val = ctx.i32_type().const_int(env_len, false);
    let call = builder
        .borrow()
        .build_call(target, &[args_ptr.into(), argc_val.into(), env_ptr.into(), env_len_val.into()], "call_result")
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
    let env_len = match &args[4] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    // The mask arrives as two 32-bit halves: it has one bit per captured slot
    // and the island's widest fixed-width integer is `i32`, so 64 slots do not
    // fit one of its words. Each half is a *bit pattern* (`pow2` doubles into
    // `i32`'s sign bit for slot 31), so it is masked back to 32 bits here
    // rather than sign-extended.
    let mask_half = |i: usize| -> Result<u64, EvalError> {
        match &args[i] {
            Value::Int(n) => Ok(*n as u64 & 0xFFFF_FFFF),
            other => Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
        }
    };
    let sexpr_mask = mask_half(5)? | (mask_half(6)? << 32);
    let ctx = crate::compile::llvm_context();
    let b = builder.borrow();
    let module = module.borrow();
    // Which constructor names the ABI the body was emitted under, so the
    // closure carries it. A process can hold both at once across an ABI
    // change — the island runs from its committed dump while this binary JITs
    // fresh code — so the answer cannot be a process-wide setting.
    let ctor_name = if crate::compile::EMITTED_BODY_ABI == typelisp_abi::BODY_ABI_COROUTINE {
        "rt_coroutine_closure_new"
    } else {
        "rt_closure_new"
    };
    let rt_closure_new = module.get_function(ctor_name).ok_or_else(|| {
        EvalError::Internal(format!("build-make-closure: {} not declared in this module", ctor_name))
    })?;

    let i64_ty = ctx.i64_type();
    let argc = 2 + env_len;
    let ctor_args_ptr = b
        .build_alloca(i64_ty.array_type(argc as u32), "closure_ctor_args")
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

/// `build-call-with-env`'s indirect counterpart: the callee isn't a
/// statically-known `llvm-function` here, only a tagged `Sexpr` function
/// value, so the call goes out through
/// [`typelisp_rt::rt_apply_any`](crate::compile::runtime::rt_apply_any) —
/// one `rt_*` call taking the closure word, the address of the argument
/// array the caller already built, and its length.
///
/// This used to emit the dispatch inline: `rt_closure_fnptr` +
/// `rt_closure_env_len`, an `rt_closure_env_get` copy loop into a fixed
/// 64-slot scratch buffer, then a `build_indirect_call` through
/// `compiled_fn_type_with_env`. That works for a `BoxedObj::CompiledClosure`
/// and *only* for one — `rt_closure_fnptr` `fatal`s on anything else, and a
/// `Type::Fn` value can equally be an interpreted `BoxedObj::Closure` since
/// the JIT stopped being mandatory. Emitting a call to a shim that dispatches
/// on the box instead moves the whole decision to where the value is, at the
/// cost of one call for the compiled case (the loop it replaces was already
/// three `rt_*` calls plus a per-slot one).
///
/// `compiler.rs` is untouched by this — it asks for `build-closure-apply`
/// and gets whatever this emits — which is what let the change happen with
/// the committed island bitcode frozen.
fn llvm_builder_build_closure_apply(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let closure = expect_llvm_value(&args[2])?;
    let args_ptr = expect_llvm_value(&args[3])?.into_pointer_value();
    let argc = match &args[4] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
    let ctx = crate::compile::llvm_context();
    let b = builder.borrow();
    let module = module.borrow();
    let err = |e: inkwell::builder::BuilderError| EvalError::Internal(format!("build-closure-apply: {}", e));

    let rt_apply_any = module
        .get_function("rt_apply_any")
        .ok_or_else(|| EvalError::Internal("build-closure-apply: rt_apply_any not declared in this module".into()))?;

    // `rt_apply_any` shares the one uniform `(args_ptr, argc) -> i64` `rt_*`
    // ABI (`compiled_fn_type`), so its own three arguments go into an
    // `alloca`'d array first — exactly what `compiler.rs`'s
    // `alloca-args`/`store-arg`/`build-call` triple does for every other
    // `rt_*` call. The *callee's* argument array is passed by address, as an
    // `i64`; it is a live `alloca` in this frame, so no copy is needed (and
    // unlike a `Vec`'s buffer it cannot be moved by anything the shim does).
    let i64_ty = ctx.i64_type();
    let shim_args = b.build_alloca(i64_ty.array_type(3), "apply_any_args").map_err(err)?;
    let store = |i: u64, v: BasicValueEnum<'static>, name: &str| -> Result<(), EvalError> {
        let p = unsafe { b.build_gep(i64_ty, shim_args, &[i64_ty.const_int(i, false)], name).map_err(err)? };
        b.build_store(p, v).map_err(err)?;
        Ok(())
    };
    let args_ptr_int = b.build_ptr_to_int(args_ptr, i64_ty, "apply_any_callee_args_int").map_err(err)?;
    store(0, closure, "apply_any_closure_ptr")?;
    store(1, args_ptr_int.into(), "apply_any_args_ptr")?;
    store(2, i64_ty.const_int(argc, false).into(), "apply_any_argc_ptr")?;

    let call = b
        .build_call(rt_apply_any, &[shim_args.into(), ctx.i32_type().const_int(3, false).into()], "closure_apply_result")
        .map_err(err)?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(llvm_value_value(v)),
        inkwell::values::ValueKind::Instruction(_) => Err(EvalError::Internal("build-closure-apply: rt_apply_any produced no value".into())),
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
    let count = match &args[1] {
        Value::Int(n) => *n as u64,
        other => return Err(EvalError::Internal(format!("expected an Int, got {:?}", other))),
    };
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
/// value is a plain `i64` (`registry::llvm_module_def`'s doc comment), so a
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
/// `compile-call`'s protected form is the only caller: inside a `catch`/
/// `unwind-protect` region a direct call goes through `rt_protected_call`,
/// which needs the target as a value it can be *handed* rather than as the
/// callee of a `call` instruction. LLVM already treats a `FunctionValue` as a
/// pointer constant, so this is a `ptrtoint` on it and nothing else.
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

// ---- LLVM handle registry + `rt_llvm_call` (interp-closure removal ------
// ---- Stage 1) -----------------------------------------------------------
//
// Compiled code represents every Rust-native LLVM value (an `llvm-*
// LlvmModule`/`LlvmBuilder`/`LlvmFunction`/`LlvmBasicBlock`/`LlvmValue`)
// object, and a native-repr `Scope<V>`) as an *LLVM handle*: an index into this
// thread-local registry, carried as a plain untraced `i64`
// (`Repr::Handle` — kind `1`, the integer kind). That is
// what makes the self-hosted compiler island's own `defun`s compilable:
// their `llvm-*` method calls lower to a single generic shim,
// [`rt_llvm_call`], which decodes handles back to those objects, dispatches
// into the very same [`eval_llvm_builtin_method`]/native-scope builtins the
// interpreter uses, and encodes the result.
//
// Registry entries are only ever *appended* during a compile session; the
// mark/release pair below lets the outermost compiled-island entry point
// drop everything it accumulated once the session ends (the `Rc`s inside
// the registered objects keep shared structures like a module or a
// scope's frames alive exactly as long as some other owner still needs
// them). Thread-local for the same reason `set_active_heap` is: `cargo
// test` workers each drive their own independent `Heap`/LLVM session.

/// A Rust-native object that has no representation in the value heap, held
/// here so a typelisp value can refer to it by an opaque integer handle.
///
/// These are the compiler's own working objects — an LLVM module, builder,
/// function, block, value, and a `Scope` whose `V` is one of those. They
/// cannot live on the GC heap (an inkwell handle is not a `Value` and has no
/// traceable shape), and they used to be carried as `RtValue::Llvm*`/`Scope`
/// variants instead: a second, Rust-side value universe running alongside the
/// heap one, which is what forced every binding to be routed by static type
/// (`SlotKind`) rather than just being a `Value`.
///
/// Keeping the object here and handing out an `i64` collapses that: at the
/// language level an `llvm-value` is an integer like any other, and the same
/// representation already crosses to compiled code
/// (`Repr::Handle`'s integer kind), so interpreted and
/// compiled tiers now agree by construction rather than by conversion. Type
/// safety is unaffected — `LLVM_HANDLE_TYPES` keeps these distinct from `i32`
/// statically, which is where it was always enforced.
///
/// `typelisp_rt::stream` represents OS streams the same way.
#[derive(Clone)]
pub(crate) enum NativeHandle {
    Module(Rc<RefCell<Module<'static>>>),
    Builder(Rc<RefCell<Builder<'static>>>),
    Function(FunctionValue<'static>),
    BasicBlock(BasicBlock<'static>),
    Value(BasicValueEnum<'static>),
}

/// The registry itself, wrapped only so its teardown can be given the same
/// treatment its explicit releases get.
///
/// A `Module`/`Builder` share sitting here is often the *last* one — the
/// module a compile was driven with is registered before
/// `run_compile_function`'s own mark, so it outlives the release bracket and
/// stays until the thread ends. Destroying it runs `~Module`, which rewrites
/// the shared LLVM Context's value-name tables, so it has to happen under
/// `COMPILE_LOCK` like every other Context-touching operation (see that
/// static's doc comment) — including at thread exit, which is where this
/// `Drop` comes in. The exiting thread holds no lock of its own, so taking
/// one here cannot deadlock.
///
/// The shares are not handed to `compile::retire_llvm` the way an engine is:
/// these are `Rc`s, and every share of a given module lives on this thread
/// (the registry is thread-local, and so is the compile that made it), so
/// moving one to a list another thread drains would put a non-atomic refcount
/// under two threads. Locking in place keeps the counting single-threaded and
/// still puts the destructor inside the lock.
struct LlvmHandles(Vec<NativeHandle>);

impl Drop for LlvmHandles {
    fn drop(&mut self) {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        self.0.clear();
    }
}

// A handle is an index into this vector, which is what lets one cross to
// compiled code as a plain integer with no ownership attached. It grows for
// the life of the thread except where `llvm_handles_mark`/`_release` bracket a
// compile (`Interp::run_compile_function`), which is the one place a bounded
// batch of handles is known to be finished with.
thread_local! {
    static LLVM_HANDLES: RefCell<LlvmHandles> = const { RefCell::new(LlvmHandles(Vec::new())) };
}

/// Registers `h` and returns its handle — the `i64` both the interpreter and
/// compiled code carry in place of the Rust-native object.
pub(crate) fn llvm_handle_register(h: NativeHandle) -> i64 {
    LLVM_HANDLES.with(|t| {
        let mut t = t.borrow_mut();
        t.0.push(h);
        (t.0.len() - 1) as i64
    })
}

/// The object behind handle `h`, or `None` for a never-issued (or already
/// released) handle.
pub(crate) fn llvm_handle_get(h: i64) -> Option<NativeHandle> {
    if h < 0 {
        return None;
    }
    LLVM_HANDLES.with(|t| t.borrow().0.get(h as usize).cloned())
}

/// Register `h` as the interpreter-level value standing for it.
fn handle_value(h: NativeHandle) -> Value {
    Value::Int(llvm_handle_register(h))
}

fn llvm_module_value(m: Module<'static>) -> Value {
    handle_value(NativeHandle::Module(Rc::new(RefCell::new(m))))
}

pub(crate) fn llvm_module_value_rc(m: Rc<RefCell<Module<'static>>>) -> Value {
    handle_value(NativeHandle::Module(m))
}

fn llvm_builder_value(b: Builder<'static>) -> Value {
    handle_value(NativeHandle::Builder(Rc::new(RefCell::new(b))))
}

fn llvm_function_value(f: FunctionValue<'static>) -> Value {
    handle_value(NativeHandle::Function(f))
}

fn llvm_block_value(b: BasicBlock<'static>) -> Value {
    handle_value(NativeHandle::BasicBlock(b))
}

fn llvm_value_value(v: BasicValueEnum<'static>) -> Value {
    handle_value(NativeHandle::Value(v))
}

/// The LLVM module `v` is a handle for, or `None` if it is not one.
///
/// An `llvm-module` is an opaque integer at the value level, so a caller
/// outside this module (a test inspecting generated IR, say) needs this to get
/// at the object behind it.
pub fn llvm_module_of(v: &Value) -> Option<Rc<RefCell<Module<'static>>>> {
    match llvm_handle_get(match v {
        Value::Int(h) => *h,
        _ => return None,
    })? {
        NativeHandle::Module(m) => Some(m),
        _ => None,
    }
}

/// The LLVM value `v` is a handle for — [`llvm_module_of`]'s counterpart.
pub fn llvm_value_of(v: &Value) -> Option<BasicValueEnum<'static>> {
    match llvm_handle_get(match v {
        Value::Int(h) => *h,
        _ => return None,
    })? {
        NativeHandle::Value(x) => Some(x),
        _ => None,
    }
}

/// The handle integer standing for `v` at the compiled boundary.
///
/// An LLVM object already *is* its handle, so this is the identity on it; only
/// a native scope still has to be registered on the way out.
fn handle_of(v: &Value) -> Option<i64> {
    match v {
        Value::Int(h) => Some(*h),
        _ => None,
    }
}

/// The interpreter value standing for handle integer `raw`, the inverse of
/// [`handle_of`].
fn value_of_handle(raw: i64) -> Option<Value> {
    llvm_handle_get(raw).map(|_| Value::Int(raw))
}

/// The current registry length — pass to [`llvm_handles_release`] to drop
/// every handle issued after this point.
///
/// [`Interp::run_compile_function`] brackets each compile with a pair, so the
/// transient handles the native compiler registers while emitting one
/// function's IR do not accumulate across many compiles. Everything else
/// issued outside such a bracket lives as long as the thread.
pub(crate) fn llvm_handles_mark() -> usize {
    LLVM_HANDLES.with(|t| t.borrow().0.len())
}

/// Drops every handle issued since the matching [`llvm_handles_mark`].
///
/// The handles come out of the registry first and are destroyed afterwards,
/// under `COMPILE_LOCK`: a released `Builder`/`Module` share is often the last
/// one, and its destructor touches the shared LLVM Context — see
/// [`LlvmHandles`]. Taking the lock here is safe because the only caller
/// (`Interp::run_compile_function`) runs outside it, as `add_compiled_function`
/// requires; the registry's own `RefCell` borrow ends before the lock is taken,
/// so a destructor that reached back into the registry could not deadlock on
/// that either.
pub(crate) fn llvm_handles_release(mark: usize) {
    let released: Vec<NativeHandle> = LLVM_HANDLES.with(|t| t.borrow_mut().0.split_off(mark));
    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    drop(released);
}

/// The handle integer `v` carries, for the `expect_llvm_*` accessors.
fn expect_handle(v: &Value, want: &str) -> Result<NativeHandle, EvalError> {
    let h = match v {
        Value::Int(n) => *n,
        other => return Err(EvalError::Internal(format!("expected an {}, got {:?}", want, other))),
    };
    llvm_handle_get(h).ok_or_else(|| EvalError::Internal(format!("expected an {}, got dangling handle {}", want, h)))
}

/// How `rt_llvm_call` decodes one raw argument word, per the op table.
#[derive(Clone, Copy, Debug)]
enum LlvmArgK {
    /// A registry handle — decode via [`llvm_handle_get`].
    Handle,
    /// A tagged heap `Value::Boxed` at a `StructPayload::Frames` scope.
    ///
    /// Compiled code treats a scope word as opaque: it only ever receives one
    /// from `rt_llvm_call` and hands it straight back, so what the word *is*
    /// is settled entirely here, and the committed island bitcode is
    /// indifferent to the change from registry handle to tagged box.
    Scope,
    /// A tagged heap `Value::Str`.
    Str,
    /// A raw untagged integer.
    Int,
    /// A raw 0/1 word.
    Bool,
}

/// How `rt_llvm_call` encodes the builtin's result.
#[derive(Clone, Copy, Debug)]
enum LlvmRetK {
    /// Register the value, return its handle.
    Handle,
    /// A scope box, tagged — see [`LlvmArgK::Scope`]. Registered as a session
    /// root on the way out: the committed island binds scope words at the
    /// untraced integer kind, so nothing else keeps the box alive while the
    /// compile session runs.
    Scope,
    /// Return `0` (`compile-unit`'s convention).
    Unit,
    /// Raw 0/1.
    Bool,
    /// A freshly-allocated tagged heap `Value::Str`.
    Str,
    /// `Option<handle>` as a heap `BoxedObj::Enum` (`Some` payload =
    /// `Value::Int(handle)`, decoded through the ordinary integer field
    /// kind by a compiled `match`) — the native scope `get`'s shape.
    OptHandle,
}

/// One dispatchable builtin: `(type_key, method)` plus its marshaling
/// shape, keyed in [`llvm_op_table`] by [`compile::symbols::llvm_op_id`].
struct LlvmOp {
    type_key: &'static str,
    method: String,
    args: Vec<LlvmArgK>,
    ret: LlvmRetK,
}

fn llvm_arg_kind(ty: &Type) -> LlvmArgK {
    if crate::check::repr::is_llvm_handle_ty(ty) {
        LlvmArgK::Handle
    } else if ty.is_integer() {
        LlvmArgK::Int
    } else {
        match ty {
            Type::Str => LlvmArgK::Str,
            Type::Bool => LlvmArgK::Bool,
            other => panic!("llvm_op_table: parameter type {:?} has no rt_llvm_call marshaling", other),
        }
    }
}

fn llvm_ret_kind(ty: &Type) -> LlvmRetK {
    if crate::check::repr::is_llvm_handle_ty(ty) {
        LlvmRetK::Handle
    } else {
        match ty {
            Type::Unit => LlvmRetK::Unit,
            Type::Bool => LlvmRetK::Bool,
            Type::Str => LlvmRetK::Str,
            other => panic!("llvm_op_table: return type {:?} has no rt_llvm_call marshaling", other),
        }
    }
}

/// The `rt_llvm_call` dispatch table, keyed by [`compile::symbols::llvm_op_id`]'s
/// stable hash. The `llvm-*` entries are *derived* from the same
/// `check::registry` `AdtDef`s the checker types these methods with —
/// table and signatures cannot drift apart. The six `native-scope` entries
/// are written out by hand because `scope_def`'s signatures are generic
/// over `V` (here always an LLVM handle — see
/// `core_bridge::llvm_assoc_key`, which only routes a `Scope<V>` with an
/// LLVM-handle `V` to this table in the first place).
fn llvm_op_table() -> &'static HashMap<i64, LlvmOp> {
    static TABLE: std::sync::OnceLock<HashMap<i64, LlvmOp>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t: HashMap<i64, LlvmOp> = HashMap::new();
        let insert = |t: &mut HashMap<i64, LlvmOp>, type_key: &'static str, method: String, args: Vec<LlvmArgK>, ret: LlvmRetK| {
            let id = crate::compile::symbols::llvm_op_id(type_key, &method);
            if t.insert(id, LlvmOp { type_key, method, args, ret }).is_some() {
                panic!("llvm_op_table: op id collision on {}", id);
            }
        };
        for (type_key, def) in [
            ("llvm-module", crate::check::registry::llvm_module_def()),
            ("llvm-function", crate::check::registry::llvm_function_def()),
            ("llvm-builder", crate::check::registry::llvm_builder_def()),
        ] {
            for (method, af) in def.assoc {
                let args = af.sig.params.iter().map(llvm_arg_kind).collect();
                let ret = llvm_ret_kind(&af.sig.ret);
                insert(&mut t, type_key, method, args, ret);
            }
        }
        use LlvmArgK::{Handle as H, Str as S};
        // The scope receiver is a tagged box word now, not a registry handle
        // (`LlvmArgK::Scope`). Only the op *ids* are baked into the committed
        // island bitcode — this table, and so the marshaling shape, is Rust
        // side and free to change with it.
        const SC: LlvmArgK = LlvmArgK::Scope;
        insert(&mut t, "native-scope", "new".to_string(), vec![], LlvmRetK::Scope);
        insert(&mut t, "native-scope", "clone-frames".to_string(), vec![SC], LlvmRetK::Scope);
        insert(&mut t, "native-scope", "push-frame".to_string(), vec![SC], LlvmRetK::Unit);
        insert(&mut t, "native-scope", "pop-frame".to_string(), vec![SC], LlvmRetK::Unit);
        insert(&mut t, "native-scope", "get".to_string(), vec![SC, S], LlvmRetK::OptHandle);
        insert(&mut t, "native-scope", "set".to_string(), vec![SC, S, H], LlvmRetK::Unit);
        t
    })
}

/// The `rt_*` family's abort-on-invariant-break convention
/// (`typelisp-rt`'s `fatal`), for the main-crate shims.
fn rt_llvm_fatal(msg: &str) -> ! {
    eprintln!("typelisp runtime error: {}", msg);
    std::process::abort();
}

/// `(rt-llvm-call opid arg...)` for compiled code — the generic dispatch
/// shim behind every compiled `llvm-*`/native-`Scope<V>` builtin method
/// call (`compiler.rs`'s `compile-llvm-op`; interp-closure removal Stage
/// 1). `args[0]` is the [`compile::symbols::llvm_op_id`] hash embedded at
/// translate time; the rest are marshaled per the matching
/// [`llvm_op_table`] entry and dispatched into the *exact same*
/// [`eval_llvm_builtin_method`]/native-scope builtins the interpreter
/// itself uses — one implementation, two callers, no drift.
///
/// Argument values are fully materialized into Rust-side objects
/// *before* anything here can allocate on the GC heap, so callers only
/// need their usual kind-driven rooting (a tagged `Str` argument crossing
/// in stays valid until then because nothing between the caller's own
/// allocation and this decode allocates).
///
/// # Safety
///
/// `args` must point to `argc` valid `i64`s; a `Heap` must already be
/// registered on this thread (`set_active_heap`). Errors abort via
/// [`rt_llvm_fatal`], mirroring `typelisp-rt`'s `fatal`.
pub(crate) unsafe extern "C" fn rt_llvm_call(args: *const i64, argc: u32) -> i64 {
    let argv = std::slice::from_raw_parts(args, argc as usize);
    let Some((&opid, raw_args)) = argv.split_first() else {
        rt_llvm_fatal("rt_llvm_call: missing op id");
    };
    let Some(op) = llvm_op_table().get(&opid) else {
        rt_llvm_fatal(&format!("rt_llvm_call: unknown op id {}", opid));
    };
    if raw_args.len() != op.args.len() {
        rt_llvm_fatal(&format!(
            "rt_llvm_call: {}::{} expects {} arguments, got {}",
            op.type_key,
            op.method,
            op.args.len(),
            raw_args.len()
        ));
    }
    let heap = crate::compile::runtime::active_heap();
    let mut vals: Vec<Value> = Vec::with_capacity(raw_args.len());
    for (raw, k) in raw_args.iter().zip(&op.args) {
        vals.push(match k {
            LlvmArgK::Handle => match value_of_handle(*raw) {
                Some(v) => v,
                None => rt_llvm_fatal(&format!(
                    "rt_llvm_call: {}::{}: dangling llvm handle {} (registry holds {}, args so far {:?})",
                    op.type_key,
                    op.method,
                    raw,
                    llvm_handles_mark(),
                    raw_args
                )),
            },
            LlvmArgK::Str => match crate::compile::runtime::decode(*raw) {
                v @ Value::Str(_) => v,
                other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a Str argument, got {:?}", op.type_key, op.method, other)),
            },
            LlvmArgK::Scope => match crate::compile::runtime::decode(*raw) {
                v @ Value::Boxed(_) => v,
                other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a scope box, got {:?}", op.type_key, op.method, other)),
            },
            LlvmArgK::Int => Value::Int(*raw),
            LlvmArgK::Bool => Value::Bool(*raw != 0),
        });
    }
    // The scope `get` returns `Option<V>` — encoded specially, so it's
    // handled before the generic single-value result path.
    //
    // The stored element word goes into the `Some` payload verbatim: for the
    // island's `Scope<llvm-value>`/`Scope<llvm-function>` that is the same
    // `Value::Int(handle)` this produced before scopes moved to the heap, so
    // the option box the committed bitcode unwraps is bit-identical.
    if op.type_key == "native-scope" && op.method == "get" {
        let found = match (expect_struct_box(&vals[0]), expect_str(heap, &vals[1])) {
            (Ok(id), Ok(name)) => heap.scope_get(id, name),
            (Err(e), _) | (_, Err(e)) => rt_llvm_fatal(&format!("rt_llvm_call: native-scope::get: {:?}", e)),
        };
        // `Option<llvm-value>`, spelled through the one producer: a type's
        // runtime identity includes its instantiation, and `native-scope::get`
        // is the one builtin here that returns a generic box.
        let key = typelisp_front::type_key::type_key_of_type(&crate::types::Type::Named(
            crate::Path::root("option"),
            vec![crate::types::Type::Named(crate::Path::root("llvm-value"), vec![])],
        ));
        let boxed = match found {
            Some(v) => typelisp_front::type_key::alloc_enum_keyed(heap, &key, 0, vec![v]),
            None => typelisp_front::type_key::alloc_enum_keyed(heap, &key, 1, vec![]),
        };
        return crate::compile::runtime::encode(boxed);
    }
    let result: Result<Value, EvalError> = if op.type_key == "native-scope" {
        match op.method.as_str() {
            "new" => Ok(heap.alloc_scope()),
            "clone-frames" => scope_clone_frames_heap(heap, &vals),
            "push-frame" => scope_push_frame_heap(heap, &vals),
            "pop-frame" => scope_pop_frame_heap(heap, &vals),
            // The element type is irrelevant here: the value arrives already
            // in its stored form, so `set` stores the word as given.
            "set" => scope_set_raw(heap, &vals),
            other => rt_llvm_fatal(&format!("rt_llvm_call: unknown native-scope method {}", other)),
        }
    } else {
        match eval_llvm_builtin_method(heap, &Path::root(op.type_key), &op.method, &vals) {
            Some(r) => r,
            None => rt_llvm_fatal(&format!("rt_llvm_call: {} has no builtin method {}", op.type_key, op.method)),
        }
    };
    let v = match result {
        Ok(v) => v,
        Err(e) => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: {:?}", op.type_key, op.method, e)),
    };
    match op.ret {
        LlvmRetK::Handle => match handle_of(&v) {
            Some(h) => h,
            None => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a handle result, got {:?}", op.type_key, op.method, v)),
        },
        LlvmRetK::Scope => {
            // Root for the compile session: the committed island holds this
            // word in an untraced local, so nothing else would keep the box
            // alive across the next collection.
            heap.push_session_root(v);
            crate::compile::runtime::encode(v)
        }
        LlvmRetK::Unit => 0,
        LlvmRetK::Bool => match v {
            Value::Bool(b) => i64::from(b),
            other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a Bool result, got {:?}", op.type_key, op.method, other)),
        },
        LlvmRetK::Str => match v {
            // Already a heap string; encoding is the tagged word, no copy.
            sv @ Value::Str(_) => crate::compile::runtime::encode(sv),
            other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a Str result, got {:?}", op.type_key, op.method, other)),
        },
        LlvmRetK::OptHandle => rt_llvm_fatal("rt_llvm_call: OptHandle result outside native-scope::get"),
    }
}
