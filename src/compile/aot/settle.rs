//! What is known of the direct functions once all of them are emitted, made
//! use of: their slots are values, a volatile access of a constant address
//! is an instruction, a call that cannot raise is not followed by a check
//! for a raise, and slots no collection can happen under are not the
//! collector's.
//!
//! At every optimization level, because they decide what the executable
//! needs of the runtime (`build_main_wrapper`), and what a program needs
//! must not depend on how hard it is optimized.

use std::collections::HashSet;

use inkwell::module::Module;
use inkwell::values::{AsValueRef, BasicValue, BasicValueEnum, FunctionValue, InstructionOpcode, InstructionValue, IntValue, Operand};

use typelisp_rt::c_mem;

/// Turns a function's stack slots into values and folds what that makes
/// constant.
const PASSES: &std::ffi::CStr = c"sroa,early-cse<memssa>,simplifycfg";

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
    let quiet = quiet(&functions);
    for f in functions.iter().filter(|f| quiet.contains(&name_of(**f))) {
        frame_on_the_stack(*f)?;
    }
    for f in &functions {
        simplify(*f)?;
    }
    Ok(())
}

const FRAME_OPEN: &str = "rt_local_frame_open";
const FRAME_CLOSE: &str = "rt_local_frame_close";

/// The functions among `functions` nothing can be collected under: all they
/// call is one another, LLVM's own, and the opening and closing of a frame.
fn quiet(functions: &[FunctionValue<'static>]) -> HashSet<String> {
    let mut quiet: HashSet<String> = functions.iter().map(|f| name_of(*f)).collect();
    loop {
        let loud: Vec<String> = functions
            .iter()
            .filter(|f| quiet.contains(&name_of(**f)))
            .filter(|f| {
                instructions(**f).into_iter().filter(|i| i.get_opcode() == InstructionOpcode::Call).any(|i| match callee(i).map(name_of) {
                    None => true,
                    Some(c) => c != FRAME_OPEN && c != FRAME_CLOSE && !c.starts_with("llvm.") && !quiet.contains(&c),
                })
            })
            .map(|f| name_of(*f))
            .collect();
        if loud.is_empty() {
            return quiet;
        }
        for f in loud {
            quiet.remove(&f);
        }
    }
}

/// Makes the frame `f` opens for the collector, if it opens one, a stack
/// array.
fn frame_on_the_stack(f: FunctionValue<'static>) -> Result<(), String> {
    let called = |name: &str| -> Vec<InstructionValue<'static>> {
        instructions(f).into_iter().filter(|i| callee(*i).is_some_and(|c| c.get_name().to_bytes() == name.as_bytes())).collect()
    };
    let opens = called(FRAME_OPEN);
    let [open] = opens[..] else { return Ok(()) };
    let Some(Operand::Value(BasicValueEnum::IntValue(n))) = open.get_operand(0) else {
        return Err(format!("internal error: the frame of `{}` has no length", name_of(f)));
    };
    let n = n.get_zero_extended_constant().ok_or_else(|| format!("internal error: the frame of `{}` has no constant length", name_of(f)))?;
    let word_ty = crate::compile::carriers::word();
    let b = crate::compile::llvm_context().create_builder();
    b.position_before(&open);
    let slots = b.build_alloca(word_ty.array_type(n as u32), "slots").map_err(|e| format!("the frame of `{}`: {}", name_of(f), e))?;
    for close in called(FRAME_CLOSE) {
        close.erase_from_basic_block();
    }
    let data: IntValue<'static> = open.try_into().map_err(|()| "internal error: a frame is a word".to_string())?;
    let mut at = data.get_first_use();
    while let Some(used) = at {
        at = used.get_next_use();
        let inkwell::values::AnyValueEnum::PointerValue(address) = used.get_user() else {
            return Err(format!("internal error: the frame of `{}` is used other than by its address", name_of(f)));
        };
        address.replace_all_uses_with(slots);
        address.as_instruction().ok_or("internal error: a frame's address is an instruction")?.erase_from_basic_block();
    }
    open.erase_from_basic_block();
    Ok(())
}

/// Settles the static globals' initializers `inits`, each with its global's
/// LLVM variable and whether that can be assigned, in the order they run: one
/// that stores a constant becomes the variable's initial value. Answers the
/// names of those left to run.
pub(super) fn settle_inits(module: &Module<'static>, inits: &[(String, String, bool)]) -> Result<Vec<String>, String> {
    let mut left = Vec::new();
    for (name, variable, assigned) in inits {
        let f = module.get_function(name).ok_or_else(|| format!("internal error: `{}` was not compiled", name))?;
        let g = module.get_global(variable).ok_or_else(|| format!("internal error: `{}` sets no global", name))?;
        simplify(f)?;
        if !quiet(&[f]).is_empty() {
            frame_on_the_stack(f)?;
            simplify(f)?;
        }
        let constant = match instructions(f)[..] {
            [store, ret] if store.get_opcode() == InstructionOpcode::Store && ret.get_opcode() == InstructionOpcode::Return => match store.get_operand(0) {
                Some(Operand::Value(v)) if is_constant(v) => Some(v),
                _ => None,
            },
            _ => None,
        };
        let Some(value) = constant else {
            left.push(name.clone());
            continue;
        };
        g.set_initializer(&value);
        g.set_constant(!assigned);
        // SAFETY: nothing calls an initializer but the `main` yet to be built.
        unsafe { f.delete() };
    }
    Ok(left)
}

fn is_constant(v: BasicValueEnum<'static>) -> bool {
    match v {
        BasicValueEnum::IntValue(v) => v.is_constant_int(),
        BasicValueEnum::FloatValue(v) => v.is_const(),
        _ => false,
    }
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
    // The addresses computed from an `int` that is a constant by now: `n << 1`.
    // All found first: taking one away takes away the check after it too.
    let addresses: Vec<_> =
        instructions(f).into_iter().filter(|i| callee(*i).is_some_and(|f| f.get_name().to_bytes() == b"rt_c_address")).collect();
    for call in addresses {
        let Some(Operand::Value(BasicValueEnum::IntValue(n))) = call.get_operand(0) else { continue };
        let Some(n) = n.get_sign_extended_constant().filter(|n| *n >= 0 && n & 1 == 0) else { continue };
        drop_check(call);
        let old: IntValue<'static> = call.try_into().map_err(|()| "internal error: an address is a word".to_string())?;
        old.replace_all_uses_with(word_ty.const_int(n as u64 >> 1, false));
        call.erase_from_basic_block();
    }
    // An address may be on its way through a slot.
    simplify(f)?;
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
        let (Some(addr), Some(kind)) =
            (word(0).and_then(|v| v.get_zero_extended_constant()), word(1).and_then(|v| v.get_sign_extended_constant()))
        else {
            continue;
        };
        let bits = match kind {
            c_mem::KIND_U8 => 8,
            c_mem::KIND_U16 => 16,
            c_mem::KIND_U32 => 32,
            _ => continue,
        };
        if addr % u64::from(bits / 8) != 0 {
            continue;
        }
        b.position_before(&call);
        let ptr = word_ty.const_int(addr, false).const_to_pointer(ctx.ptr_type(inkwell::AddressSpace::default()));
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
