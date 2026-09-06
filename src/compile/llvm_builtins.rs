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
//! backend -> front end, which is the direction it has to point for the front
//! end to become a crate of its own (`docs/dev/TODO.md`).
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
use inkwell::values::{BasicValueEnum, FunctionValue};
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
            _ => None,
        };
    }
    if *type_name == Path::root("llvm-function") {
        return match method {
            "append-block" => Some(llvm_function_append_block(heap, args)),
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
    let ptr = builder.borrow().build_alloca(array_ty, "call_args").map_err(|e| EvalError::Internal(format!("alloca-args: {}", e)))?;
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
    let rt_closure_new = module
        .get_function("rt_closure_new")
        .ok_or_else(|| EvalError::Internal("build-make-closure: rt_closure_new not declared in this module".into()))?;

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
                None => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: dangling llvm handle {}", op.type_key, op.method, raw)),
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
