//! What is known of the direct functions once all of them are emitted, made
//! use of: their slots are values, a volatile access of a constant address
//! is an instruction, and a call that cannot raise is not followed by a
//! check for a raise.
//!
//! At every optimization level, because they decide what the executable
//! needs of the runtime (`build_main_wrapper`), and what a program needs
//! must not depend on how hard it is optimized.

use std::collections::HashSet;

use inkwell::module::Module;
use inkwell::values::{AsValueRef, BasicValueEnum, FunctionValue, InstructionOpcode, InstructionValue, IntValue, Operand};

use typelisp_rt::c_mem;

/// Turns a function's stack slots into values and folds what that makes
/// constant.
const PASSES: &std::ffi::CStr = c"sroa,early-cse,simplifycfg";

/// Settles the direct functions `direct` of `module`.
pub(super) fn settle(module: &Module<'static>, direct: &[String]) -> Result<(), String> {
    let functions: Vec<FunctionValue<'static>> =
        direct.iter().filter_map(|name| module.get_function(name)).filter(|f| f.count_basic_blocks() > 0).collect();
    for f in &functions {
        simplify(*f)?;
        volatile_accesses(*f)?;
    }
    let raising = raising(&functions);
    for f in functions.iter().filter(|f| !raising.contains(&name_of(**f))) {
        for call in calls_of(*f) {
            drop_check(call);
        }
    }
    for f in &functions {
        simplify(*f)?;
    }
    Ok(())
}

fn simplify(f: FunctionValue<'static>) -> Result<(), String> {
    use inkwell::llvm_sys::error::LLVMGetErrorMessage;
    use inkwell::llvm_sys::transforms::pass_builder::{LLVMCreatePassBuilderOptions, LLVMDisposePassBuilderOptions, LLVMRunPassesOnFunction};
    // SAFETY: `f` is a live function with a body; no machine is needed for
    // passes that ask nothing of one.
    let failed = unsafe {
        let options = LLVMCreatePassBuilderOptions();
        let failed = LLVMRunPassesOnFunction(f.as_value_ref(), PASSES.as_ptr(), std::ptr::null_mut(), options);
        LLVMDisposePassBuilderOptions(options);
        failed
    };
    if failed.is_null() {
        return Ok(());
    }
    // SAFETY: the message of the error just answered, which it consumes.
    let message = unsafe { std::ffi::CStr::from_ptr(LLVMGetErrorMessage(failed)) }.to_string_lossy().into_owned();
    Err(format!("internal error: simplifying `{}` failed: {}", name_of(f), message))
}

fn name_of(f: FunctionValue<'static>) -> String {
    f.get_name().to_string_lossy().into_owned()
}

/// The function a call instruction calls by name.
fn callee(i: InstructionValue<'static>) -> Option<FunctionValue<'static>> {
    use inkwell::llvm_sys::core::{LLVMGetCalledValue, LLVMIsAFunction};
    if i.get_opcode() != InstructionOpcode::Call {
        return None;
    }
    // SAFETY: a live call instruction.
    unsafe {
        let called = LLVMGetCalledValue(i.as_value_ref());
        (!LLVMIsAFunction(called).is_null()).then(|| FunctionValue::new(called)).flatten()
    }
}

fn instructions(f: FunctionValue<'static>) -> Vec<InstructionValue<'static>> {
    f.get_basic_blocks().into_iter().flat_map(|b| b.get_instructions().collect::<Vec<_>>()).collect()
}

/// Every call of `f`.
fn calls_of(f: FunctionValue<'static>) -> Vec<InstructionValue<'static>> {
    use inkwell::llvm_sys::core::{LLVMGetFirstUse, LLVMGetNextUse, LLVMGetUser, LLVMIsACallInst};
    let mut calls = Vec::new();
    // SAFETY: walking the use list of a live function.
    unsafe {
        let mut u = LLVMGetFirstUse(f.as_value_ref());
        while !u.is_null() {
            let user = LLVMGetUser(u);
            if !LLVMIsACallInst(user).is_null() {
                calls.push(InstructionValue::new(user));
            }
            u = LLVMGetNextUse(u);
        }
    }
    calls
}

/// The functions among `functions` that can leave with a raise: one calls a
/// runtime function that raises, or another that can leave so.
fn raising(functions: &[FunctionValue<'static>]) -> HashSet<String> {
    let calls: Vec<(String, Vec<Option<String>>)> = functions
        .iter()
        .map(|f| {
            let calls = instructions(*f).into_iter().filter(|i| i.get_opcode() == InstructionOpcode::Call).map(|i| callee(i).map(name_of)).collect();
            (name_of(*f), calls)
        })
        .collect();
    let ours: HashSet<&str> = calls.iter().map(|(f, _)| f.as_str()).collect();
    let mut raising: HashSet<String> = HashSet::new();
    loop {
        let before = raising.len();
        for (f, callees) in &calls {
            let raises = callees.iter().any(|c| match c.as_deref() {
                None => true,
                Some(c) if ours.contains(c) => raising.contains(c),
                Some(c) => !c.starts_with("llvm.") && !crate::compile::externs::never_raises(c),
            });
            if raises {
                raising.insert(f.clone());
            }
        }
        if raising.len() == before {
            return raising;
        }
    }
}

/// Takes away the check that follows `call` (`raise_check::after_call`),
/// which is the next call in its block.
fn drop_check(call: InstructionValue<'static>) {
    let mut at = call.get_next_instruction();
    while let Some(i) = at {
        at = i.get_next_instruction();
        if i.get_opcode() != InstructionOpcode::Call {
            continue;
        }
        if callee(i).is_some_and(|f| f.get_name().to_bytes() == crate::compile::raise_check::RT_UNWINDING.as_bytes()) {
            let asked: IntValue<'static> = i.try_into().expect("rt_unwinding answers a word");
            asked.replace_all_uses_with(asked.get_type().const_zero());
            i.erase_from_basic_block();
        }
        return;
    }
}

/// Makes each volatile access in `f` of a constant address that is one, and
/// is aligned, an instruction: what `typelisp_rt::c_mem` would do with it.
/// Any other is left to the runtime, which is where a bad one raises.
fn volatile_accesses(f: FunctionValue<'static>) -> Result<(), String> {
    let ctx = crate::compile::llvm_context();
    let word_ty = crate::compile::carriers::word();
    let b = ctx.create_builder();
    let failed = |e: inkwell::builder::BuilderError| format!("a volatile access in `{}`: {}", name_of(f), e);
    let pointer_bytes = typelisp_mem::target::DataModel::current().pointer_bytes;
    let accesses: Vec<(InstructionValue<'static>, bool)> = instructions(f)
        .into_iter()
        .filter_map(|i| match callee(i)?.get_name().to_bytes() {
            b"rt_c_volatile_load" => Some((i, false)),
            b"rt_c_volatile_store" => Some((i, true)),
            _ => None,
        })
        .collect();
    for (call, store) in accesses {
        let word = |k: u32| match call.get_operand(k) {
            Some(Operand::Value(BasicValueEnum::IntValue(v))) => Some(v),
            _ => None,
        };
        let constant = |k: u32| word(k).and_then(|v| v.get_sign_extended_constant());
        let (Some(addr), Some(kind)) = (constant(0), constant(1)) else { continue };
        let bits = match kind {
            c_mem::KIND_U8 => 8,
            c_mem::KIND_U16 => 16,
            c_mem::KIND_U32 => 32,
            _ => continue,
        };
        let on_the_machine = addr >= 0 && (pointer_bytes >= 8 || (addr as u64) >> (8 * pointer_bytes) == 0);
        if !on_the_machine || addr % i64::from(bits / 8) != 0 {
            continue;
        }
        b.position_before(&call);
        let ptr = word_ty.const_int(addr as u64, false).const_to_pointer(ctx.ptr_type(inkwell::AddressSpace::default()));
        let ty = crate::compile::carriers::int(bits);
        let answer = if store {
            let value = word(2).ok_or("internal error: a volatile write of no word")?;
            let value = if bits < 32 { b.build_int_truncate(value, ty, "register_value").map_err(failed)? } else { value };
            let i = b.build_store(ptr, value).map_err(failed)?;
            i.set_volatile(true).map_err(|e| format!("a volatile write: {:?}", e))?;
            i.set_alignment(bits / 8).map_err(|e| format!("a volatile write: {:?}", e))?;
            word_ty.const_zero()
        } else {
            let v = b.build_load(ty, ptr, "register").map_err(failed)?.into_int_value();
            let i = v.as_instruction().ok_or("internal error: a load is an instruction")?;
            i.set_volatile(true).map_err(|e| format!("a volatile read: {:?}", e))?;
            i.set_alignment(bits / 8).map_err(|e| format!("a volatile read: {:?}", e))?;
            if bits < 32 {
                b.build_int_z_extend(v, ctx.i32_type(), "wide").map_err(failed)?
            } else {
                v
            }
        };
        drop_check(call);
        let old: IntValue<'static> = call.try_into().map_err(|()| "internal error: a volatile access answers a word".to_string())?;
        old.replace_all_uses_with(answer);
        call.erase_from_basic_block();
    }
    Ok(())
}
