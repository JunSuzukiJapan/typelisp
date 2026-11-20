//! Leaving an activation when a call raised.
//!
//! A shim that raises (`typelisp_abi::raise`) parks the unwind and returns.
//! So does compiled code under the classic ABI that saw a raise in what it
//! called. Whoever called it has to look before using the answer: right after
//! the call, generated code asks `rt_unwinding`, and if an unwind is waiting
//! it leaves its own activation at once — a coroutine-ABI body with
//! `STATUS_UNWIND`, which sends the driver to the frame's handler; any other
//! function with a zero of its return type, leaving the unwind waiting for
//! its own caller to see.
//!
//! [`after_call`] is the one place that emits this, for every call code
//! generation makes: the island's `build-call`, the coroutine prologue's
//! runtime calls, and the FFI thunks and entries.

use inkwell::builder::Builder;
use inkwell::llvm_sys::core::{LLVMGetGlobalParent, LLVMGetNamedFunction};
use inkwell::module::Module;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{AsValueRef, FunctionValue};
use inkwell::AddressSpace;

/// The shim generated code asks whether an unwind is waiting.
pub(crate) const RT_UNWINDING: &str = "rt_unwinding";

/// Emits, after the call to `callee` the builder has just made, the check
/// that leaves the current function when the call raised. Nothing is
/// emitted for a callee that never raises
/// ([`crate::compile::externs::never_raises`]).
pub(crate) fn after_call(builder: &Builder<'static>, callee: FunctionValue<'static>) -> Result<(), String> {
    let name = callee.get_name().to_string_lossy();
    if crate::compile::externs::never_raises(&name) {
        return Ok(());
    }
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let block = builder.get_insert_block().ok_or("a call was made with the builder outside any block")?;
    let current = block.get_parent().ok_or("a call was made in a block outside any function")?;
    let unwinding = rt_unwinding_in(current)?;
    let null = ctx.ptr_type(AddressSpace::default()).const_null();
    let asked = builder
        .build_call(unwinding, &[null.into(), ctx.i32_type().const_zero().into()], "unwinding")
        .map_err(|e| format!("raise check: call: {}", e))?;
    let asked = match asked.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
        inkwell::values::ValueKind::Instruction(_) => return Err("raise check: rt_unwinding produced no value".to_string()),
    };
    let raised = builder
        .build_int_compare(inkwell::IntPredicate::NE, asked, i64t.const_zero(), "raised")
        .map_err(|e| format!("raise check: compare: {}", e))?;
    let leave = ctx.append_basic_block(current, "raised");
    let go_on = ctx.append_basic_block(current, "not_raised");
    builder
        .build_conditional_branch(raised, leave, go_on)
        .map_err(|e| format!("raise check: branch: {}", e))?;
    builder.position_at_end(leave);
    leave_with_the_raise(builder, current)?;
    builder.position_at_end(go_on);
    Ok(())
}

/// The return that leaves `f` because of a raise: `STATUS_UNWIND` for a
/// coroutine-ABI body (`i64 f(i64 frame)`), a zero of the return type for
/// anything else.
fn leave_with_the_raise(builder: &Builder<'static>, f: FunctionValue<'static>) -> Result<(), String> {
    let ctx = crate::compile::llvm_context();
    let ty = f.get_type();
    let coroutine = ty.get_param_types().len() == 1
        && ty.get_param_types()[0] == BasicTypeEnum::IntType(ctx.i64_type()).into()
        && ty.get_return_type() == Some(BasicTypeEnum::IntType(ctx.i64_type()));
    let r = if coroutine {
        builder.build_return(Some(&ctx.i64_type().const_int(typelisp_abi::STATUS_UNWIND as u64, false)))
    } else {
        match ty.get_return_type() {
            Some(t) => builder.build_return(Some(&zero_of(t))),
            None => builder.build_return(None),
        }
    };
    r.map(|_| ()).map_err(|e| format!("raise check: return: {}", e))
}

fn zero_of(t: BasicTypeEnum<'static>) -> inkwell::values::BasicValueEnum<'static> {
    match t {
        BasicTypeEnum::IntType(t) => t.const_zero().into(),
        BasicTypeEnum::FloatType(t) => t.const_zero().into(),
        BasicTypeEnum::PointerType(t) => t.const_zero().into(),
        BasicTypeEnum::ArrayType(t) => t.const_zero().into(),
        BasicTypeEnum::StructType(t) => t.const_zero().into(),
        BasicTypeEnum::VectorType(t) => t.const_zero().into(),
        BasicTypeEnum::ScalableVectorType(t) => t.const_zero().into(),
    }
}

/// `rt_unwinding` as declared in the module `f` is in, declared there under
/// the shared shim signature if it is not yet.
fn rt_unwinding_in(f: FunctionValue<'static>) -> Result<FunctionValue<'static>, String> {
    let ctx = crate::compile::llvm_context();
    // SAFETY: `f` is a function in a live module; the module reference is
    // only used to look a name up and, failing that, to add a declaration.
    let module_ref = unsafe { LLVMGetGlobalParent(f.as_value_ref()) };
    let name = std::ffi::CString::new(RT_UNWINDING).expect("no NUL in a shim name");
    let found = unsafe { LLVMGetNamedFunction(module_ref, name.as_ptr()) };
    if !found.is_null() {
        // SAFETY: a function value of this module.
        return Ok(unsafe { FunctionValue::new(found) }.ok_or("raise check: rt_unwinding is not a function")?);
    }
    // SAFETY: borrowed for the one declaration and not dropped — the module is
    // owned elsewhere.
    let module: std::mem::ManuallyDrop<Module<'static>> = std::mem::ManuallyDrop::new(unsafe { Module::new(module_ref) });
    let fn_ty = ctx.i64_type().fn_type(&[ctx.ptr_type(AddressSpace::default()).into(), ctx.i32_type().into()], false);
    Ok(module.add_function(RT_UNWINDING, fn_ty, None))
}
