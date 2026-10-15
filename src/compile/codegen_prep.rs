//! What happens to a module between the island finishing it and LLVM turning
//! it into machine code — the one step every path to machine code takes (the
//! JIT, `compile-file`, and `disassemble`'s assembly).
//!
//! The island asks for a fresh stack array (`alloca-args`) for every scratch
//! word and every argument array it needs, and never hands one back. Emitted as
//! is, each of those is a slot of its own in the machine frame, so a function's
//! frame grew with the size of its body: `(+ 1 (+ 1 ... a))` a hundred
//! thousand deep needed 8.3MB, and overflowed the stack it was called on. What
//! is here, with the island's `scratch-slot` for its merge slots, makes the
//! frame grow with what is in use at once instead:
//!
//! - [`pool_argument_arrays`] gives argument arrays — filled, then handed to
//!   one call, all within one block — a few shared arrays, reused by every
//!   argument array whose span does not overlap theirs.
//! - [`forward_block_local_words`] removes the scratch words that are written
//!   and read back within one block, such as an overflow flag.
//! - [`rematerialize_slot_addresses`]: code is generated at `None`, whose
//!   register allocator gives every value used on another block than the one
//!   computing it a stack slot of its own. The coroutine prologue computes
//!   every frame slot's address up front, one such value per slot; computed
//!   where it is used instead, an address costs no slot. The island keeps
//!   its own values from crossing a block for the same reason (the operand
//!   slots of `integer-arith`).
//! - [`FRAME_WARNING_BYTES`]: a function whose frame still comes out larger
//!   than this is reported ([`take_frame_warnings`]) — a thread with less
//!   stack than that left overflows the moment it calls the function, and the
//!   frame's size is known only here, once the code is generated.
//!
//! Every step here is linear in the size of the function — which is why the
//! sharing is done here rather than left to LLVM. SROA would have turned the
//! scratch words into SSA values, and lifetime marks would have let code
//! generation share the arrays' slots, but both grow with the square of a
//! function's size, and a long body is exactly what this is for.

use inkwell::module::Module;
use inkwell::basic_block::BasicBlock;
use inkwell::values::{AsValueRef, BasicValue, InstructionOpcode, InstructionValue};
use std::collections::HashMap;

/// The frame size past which a compiled function is reported: the default of
/// Rust's `clippy::large_stack_frames`, a quarter of a 2MB thread stack.
pub(crate) const FRAME_WARNING_BYTES: u64 = 512 * 1024;

/// What code generation said about frames past [`FRAME_WARNING_BYTES`], as
/// LLVM words it, until [`take_frame_warnings`] collects them.
static FRAME_DIAGNOSTICS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Receives every diagnostic code generation issues on the shared context.
///
/// A frame past the limit is kept for [`take_frame_warnings`]. Everything else
/// is what LLVM's own handler — the one this replaces — does with it: printed
/// as a warning or note, and an error ends the process.
extern "C" fn on_diagnostic(info: inkwell::llvm_sys::prelude::LLVMDiagnosticInfoRef, _: *mut std::ffi::c_void) {
    use inkwell::llvm_sys::core::{LLVMDisposeMessage, LLVMGetDiagInfoDescription, LLVMGetDiagInfoSeverity};
    use inkwell::llvm_sys::LLVMDiagnosticSeverity;
    // SAFETY: `info` is the diagnostic being delivered; its description is a
    // fresh string this disposes of.
    let (severity, text) = unsafe {
        let raw = LLVMGetDiagInfoDescription(info);
        let text = std::ffi::CStr::from_ptr(raw).to_string_lossy().into_owned();
        LLVMDisposeMessage(raw);
        (LLVMGetDiagInfoSeverity(info), text)
    };
    match severity {
        // LLVM's words, after a location it has none to give for: "<unknown>:0:0:
        // stack frame size (N) exceeds limit (L) in function 'SYMBOL'".
        LLVMDiagnosticSeverity::LLVMDSWarning if text.contains("stack frame size (") => {
            FRAME_DIAGNOSTICS.lock().unwrap_or_else(|e| e.into_inner()).push(text)
        }
        LLVMDiagnosticSeverity::LLVMDSWarning => eprintln!("warning: {}", text),
        LLVMDiagnosticSeverity::LLVMDSNote => eprintln!("note: {}", text),
        LLVMDiagnosticSeverity::LLVMDSRemark => {}
        LLVMDiagnosticSeverity::LLVMDSError => {
            eprintln!("error: {}", text);
            std::process::exit(1);
        }
    }
}

/// Makes [`on_diagnostic`] the shared context's diagnostic handler.
///
/// Set again before every code generation rather than once: inkwell's
/// `Module::link_in_module` replaces the handler with one of its own and
/// leaves it there, pointing at a variable of a call that has returned — the
/// next diagnostic would write through it. Whoever links a module calls this
/// straight after.
pub(crate) fn install_diagnostic_handler() {
    // SAFETY: the shared context lives for the rest of the process, and the
    // handler is a plain function with no context of its own.
    unsafe {
        inkwell::llvm_sys::core::LLVMContextSetDiagnosticHandler(
            crate::compile::llvm_context().raw(),
            Some(on_diagnostic),
            std::ptr::null_mut(),
        )
    }
}

/// The warnings for every function code generation has found a frame past
/// [`FRAME_WARNING_BYTES`] in since the last call, named as the source names
/// them.
pub(crate) fn take_frame_warnings() -> Vec<String> {
    let found = std::mem::take(&mut *FRAME_DIAGNOSTICS.lock().unwrap_or_else(|e| e.into_inner()));
    found.iter().map(|text| frame_warning(text)).collect()
}

/// LLVM's "stack frame size (N) exceeds limit (L) in function 'SYMBOL'" as
/// this compiler's own warning; LLVM's words whole when they are not that.
fn frame_warning(text: &str) -> String {
    let number_after = |key: &str| -> Option<&str> {
        let rest = &text[text.find(key)? + key.len()..];
        rest.split(')').next()
    };
    let symbol = text.rsplit_once(" in function '").map(|(_, s)| s.trim_end_matches('\''));
    match (number_after("stack frame size ("), symbol) {
        (Some(size), Some(symbol)) => {
            let name = typelisp_mem::ConcreteNames::link_name_of_symbol(symbol)
                .map(|link| crate::CompileTarget::from_link_name(link).written_name())
                .unwrap_or_else(|| symbol.to_string());
            format!(
                "warning: compiled function `{}` needs a {}-byte stack frame (more than {}); \
                 a thread with less stack than that left overflows when it calls it",
                name, size, FRAME_WARNING_BYTES
            )
        }
        _ => format!("warning: {}", text),
    }
}

/// Prepares `module` to be turned into machine code. See the module
/// documentation for what and why.
pub(crate) fn prepare(module: &Module<'static>) -> Result<(), String> {
    install_diagnostic_handler();
    let limit = crate::compile::llvm_context().create_string_attribute("warn-stack-size", &FRAME_WARNING_BYTES.to_string());
    for function in module.get_functions().filter(|f| f.count_basic_blocks() > 0) {
        function.add_attribute(inkwell::attributes::AttributeLoc::Function, limit);
    }
    forward_block_local_words(module);
    pool_argument_arrays(module);
    rematerialize_slot_addresses(module)?;
    module.verify().map_err(|e| format!("internal error: the module failed verification after preparing it: {}", e))?;
    Ok(())
}

/// Recomputes every frame slot's address where it is used, instead of once in
/// the coroutine prologue for the whole function.
///
/// A slot's address is the frame's data pointer plus a constant, so computing
/// it again costs one instruction and changes nothing; what it buys is that no
/// value lives from the prologue to the far end of a long body. Applied to an
/// address that is an element of a pointer made by `inttoptr` in the same
/// block — the frame's data pointer, which that block's position in front of
/// every use already makes available wherever the address was. A `phi` use is
/// left as it is: it reads its value at the end of a predecessor, not where it
/// stands, and only an address with no use left is removed.
fn rematerialize_slot_addresses(module: &Module<'static>) -> Result<(), String> {
    let builder = crate::compile::llvm_context().create_builder();
    for function in module.get_functions() {
        let addresses: Vec<InstructionValue<'static>> = function
            .get_basic_block_iter()
            .flat_map(|block| block.get_instructions())
            .filter(|i| is_slot_address(*i))
            .collect();
        for address in addresses {
            let Some(uses) = users(address.get_first_use()) else { continue };
            let mut kept = false;
            for user in uses {
                if user.get_opcode() == InstructionOpcode::Phi {
                    kept = true;
                    continue;
                }
                let copy = address.explicit_clone();
                builder.position_before(&user);
                builder.insert_instruction(&copy, None);
                let copy_value = instruction_value_as_pointer(copy);
                for k in 0..user.get_num_operands() {
                    let is_this = user.get_operand(k).and_then(|o| o.value()).map(|v| v.as_value_ref())
                        == Some(address.as_value_ref());
                    if is_this && !user.set_operand(k, copy_value) {
                        return Err("internal error: could not replace a frame slot address".to_string());
                    }
                }
            }
            if !kept {
                address.erase_from_basic_block();
            }
        }
    }
    Ok(())
}

/// Whether `i` is a frame slot's address: a constant-index element of a
/// pointer made by `inttoptr` in the same block.
fn is_slot_address(i: InstructionValue<'static>) -> bool {
    if i.get_opcode() != InstructionOpcode::GetElementPtr {
        return false;
    }
    let Some(base) = i.get_operand(0).and_then(|o| o.value()) else { return false };
    let Some(base) = base.as_instruction_value() else { return false };
    if base.get_opcode() != InstructionOpcode::IntToPtr || base.get_parent() != i.get_parent() {
        return false;
    }
    (1..i.get_num_operands()).all(|k| {
        i.get_operand(k)
            .and_then(|o| o.value())
            .map(|v| v.is_int_value() && v.into_int_value().is_const())
            .unwrap_or(false)
    })
}

/// An instruction of pointer type as a pointer value.
fn instruction_value_as_pointer(i: InstructionValue<'static>) -> inkwell::values::PointerValue<'static> {
    // SAFETY: the callers pass an element address, which is of pointer type.
    unsafe { inkwell::values::PointerValue::new(i.as_value_ref()) }
}

/// Gives every argument array a shared one: an array whose span — from the
/// first store into it to the call it is handed to — overlaps no other
/// array's span on the same shared array.
///
/// An argument array is an entry-block `alloca` whose only uses are stores
/// into it — directly, or through an element address — followed, in the same
/// block, by one call that takes its address. Its contents matter from the
/// first store to the end of that call and never after: the coroutine ABI
/// already rules out a later read, since the machine frame does not survive a
/// call that suspends (see `llvm_builder_alloca_args`). Spans in different
/// blocks never overlap — each lies within one run of its block — so only
/// the spans of one block are compared, and the same few shared arrays serve
/// the whole function. An `alloca` used any other way keeps its own slot.
fn pool_argument_arrays(module: &Module<'static>) {
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    for function in module.get_functions() {
        let Some(entry) = function.get_first_basic_block() else { continue };
        let allocas: Vec<InstructionValue<'static>> =
            entry.get_instructions().filter(|i| i.get_opcode() == InstructionOpcode::Alloca).collect();
        let mut order = Order::default();
        // Each block's spans: (first store, call, words, alloca).
        let mut spans: HashMap<BasicBlock<'static>, Vec<(usize, usize, u32, InstructionValue<'static>)>> = HashMap::new();
        for alloca in allocas {
            let Some(words) = alloca.get_allocated_type().ok().and_then(|t| match t {
                inkwell::types::BasicTypeEnum::ArrayType(a) if a.get_element_type() == i64t.into() => Some(a.len()),
                _ => None,
            }) else {
                continue;
            };
            let Some((first, call)) = argument_array_span(alloca, &mut order) else { continue };
            let (from, to) = (order.of(first), order.of(call));
            if from > to {
                continue;
            }
            let block = call.get_parent().expect("an instruction found through a use is in a block");
            spans.entry(block).or_default().push((from, to, words, alloca));
        }
        // Shared arrays: (words, the alloca each span is moved onto).
        let mut shared: Vec<(u32, Vec<InstructionValue<'static>>)> = Vec::new();
        for (_, mut block_spans) in spans {
            block_spans.sort_by_key(|(from, ..)| *from);
            // Where each shared array is busy until, in this block.
            let mut busy: Vec<Option<usize>> = vec![None; shared.len()];
            for (from, to, words, alloca) in block_spans {
                let free = busy.iter().position(|until| until.is_none_or(|u| u < from));
                let n = match free {
                    Some(n) => n,
                    None => {
                        shared.push((0, Vec::new()));
                        busy.push(None);
                        shared.len() - 1
                    }
                };
                busy[n] = Some(to);
                shared[n].0 = shared[n].0.max(words);
                shared[n].1.push(alloca);
            }
        }
        let builder = ctx.create_builder();
        for (words, members) in shared {
            if members.len() < 2 {
                continue;
            }
            match entry.get_first_instruction() {
                Some(first) => builder.position_before(&first),
                None => builder.position_at_end(entry),
            }
            let array = builder.build_alloca(i64t.array_type(words.max(1)), "shared_args").expect("an alloca in the entry block");
            for member in members {
                // SAFETY: both are live values of this module, and of the same
                // (pointer) type.
                unsafe { inkwell::llvm_sys::core::LLVMReplaceAllUsesWith(member.as_value_ref(), array.as_value_ref()) };
                member.erase_from_basic_block();
            }
        }
    }
}

/// The first instruction that fills `alloca` and the one call it is handed
/// to, when `alloca` is an argument array (see [`pool_argument_arrays`]);
/// `None` when it is used any other way. The first instruction is the
/// earliest store or element address in the block, which the caller confirms
/// comes before the call.
fn argument_array_span(
    alloca: InstructionValue<'static>,
    order: &mut Order,
) -> Option<(InstructionValue<'static>, InstructionValue<'static>)> {
    let address = alloca.as_value_ref() as usize;
    let mut fills: Vec<InstructionValue<'static>> = Vec::new();
    let mut call: Option<InstructionValue<'static>> = None;
    let is_address = |i: InstructionValue<'static>, n: u32| {
        i.get_operand(n).and_then(|o| o.value()).map(|v| v.as_value_ref() as usize) == Some(address)
    };
    for user in users(alloca.get_first_use())? {
        match user.get_opcode() {
            // A store *into* the array, not of its address.
            InstructionOpcode::Store if is_address(user, 1) && !is_address(user, 0) => fills.push(user),
            // An element's address, used only to store into.
            InstructionOpcode::GetElementPtr if is_address(user, 0) => {
                for store in users(user.get_first_use())? {
                    let into_element = store.get_opcode() == InstructionOpcode::Store
                        && store.get_operand(1).and_then(|o| o.value()).map(|v| v.as_value_ref())
                            == Some(user.as_value_ref());
                    if !into_element || store.get_parent() != user.get_parent() {
                        return None;
                    }
                }
                fills.push(user);
            }
            InstructionOpcode::Call if call.is_none() => {
                // Its address as an argument — never as the callee.
                let n = user.get_num_operands();
                if (0..n.saturating_sub(1)).any(|k| is_address(user, k)) && !is_address(user, n - 1) {
                    call = Some(user);
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
    let call = call?;
    let block = call.get_parent();
    if fills.iter().any(|f| f.get_parent() != block) {
        return None;
    }
    let first = fills.into_iter().min_by_key(|f| order.of(*f))?;
    Some((first, call))
}

/// Where instructions sit in their blocks, counted from the top — each block
/// numbered once, the first time it is asked about.
///
/// Counting afresh at every question would be quadratic: the coroutine
/// prologue is one block with an instruction per frame slot, and every slot
/// asks about it. Kept for one pass at a time: adding or removing an
/// instruction moves the others' numbers, but never their order, and order
/// is all that is asked.
#[derive(Default)]
struct Order {
    blocks: HashMap<BasicBlock<'static>, HashMap<usize, usize>>,
}

impl Order {
    fn of(&mut self, i: InstructionValue<'static>) -> usize {
        let block = i.get_parent().expect("an instruction found through a use is in a block");
        let numbers = self.blocks.entry(block).or_insert_with(|| {
            block.get_instructions().enumerate().map(|(n, x)| (x.as_value_ref() as usize, n)).collect()
        });
        numbers[&(i.as_value_ref() as usize)]
    }
}

/// Every instruction that uses a value, from its first use on; `None` when one
/// of them is not an instruction.
fn users(first: Option<inkwell::values::BasicValueUse<'static>>) -> Option<Vec<InstructionValue<'static>>> {
    let mut out = Vec::new();
    let mut at = first;
    while let Some(u) = at {
        let user = u.get_user().as_value_ref();
        // SAFETY: `user` is a live value of this module; `LLVMIsAInstruction`
        // answers null for anything that is not an instruction.
        let instruction = unsafe { inkwell::llvm_sys::core::LLVMIsAInstruction(user) };
        if instruction.is_null() {
            return None;
        }
        // SAFETY: just checked to be an instruction.
        out.push(unsafe { InstructionValue::new(instruction) });
        at = u.get_next_use();
    }
    Some(out)
}

/// Removes every one-word entry-block array that is only written and read
/// within a single block, handing each read the value last written before
/// it.
///
/// What qualifies: every use is a load or a store
/// of the word — directly, or through its element `0` — and they all sit in
/// one block, with a store before the first load. Anything else (the address
/// passed on, a read before any write, uses in two blocks) is left alone.
fn forward_block_local_words(module: &Module<'static>) {
    for function in module.get_functions() {
        let Some(entry) = function.get_first_basic_block() else { continue };
        let allocas: Vec<InstructionValue<'static>> =
            entry.get_instructions().filter(|i| i.get_opcode() == InstructionOpcode::Alloca).collect();
        let mut order = Order::default();
        for alloca in allocas {
            let Some(plan) = block_local_word(alloca, &mut order) else { continue };
            for (load, value) in &plan.forwards {
                // SAFETY: both are live values of this module, of the same type
                // (checked by `block_local_word`).
                unsafe { inkwell::llvm_sys::core::LLVMReplaceAllUsesWith(load.as_value_ref(), *value) };
            }
            for i in plan.forwards.iter().map(|(l, _)| *l).chain(plan.rest) {
                i.erase_from_basic_block();
            }
            alloca.erase_from_basic_block();
        }
    }
}

/// What [`forward_block_local_words`] does to one array: each load with the
/// value that replaces it, then the instructions left to remove (stores, then
/// element addresses — users before what they use).
struct WordPlan {
    forwards: Vec<(InstructionValue<'static>, inkwell::llvm_sys::prelude::LLVMValueRef)>,
    rest: Vec<InstructionValue<'static>>,
}

fn block_local_word(alloca: InstructionValue<'static>, order: &mut Order) -> Option<WordPlan> {
    use inkwell::llvm_sys::core::LLVMTypeOf;
    let address = alloca.as_value_ref();
    let operand_is = |i: InstructionValue<'static>, n: u32, v| {
        i.get_operand(n).and_then(|o| o.value()).map(|x| x.as_value_ref()) == Some(v)
    };
    // (instruction, is-store) for every access, and what else must go.
    let mut accesses: Vec<(InstructionValue<'static>, bool)> = Vec::new();
    let mut elements: Vec<InstructionValue<'static>> = Vec::new();
    let access = |i: InstructionValue<'static>, through, accesses: &mut Vec<_>| match i.get_opcode() {
        InstructionOpcode::Load if operand_is(i, 0, through) => {
            accesses.push((i, false));
            true
        }
        InstructionOpcode::Store if operand_is(i, 1, through) && !operand_is(i, 0, through) => {
            accesses.push((i, true));
            true
        }
        _ => false,
    };
    for user in users(alloca.get_first_use())? {
        if user.get_opcode() == InstructionOpcode::GetElementPtr {
            // Element `0` only: one index, the constant zero.
            let zero = user.get_num_operands() == 2
                && user
                    .get_operand(1)
                    .and_then(|o| o.value())
                    .is_some_and(|v| v.is_int_value() && v.into_int_value().get_zero_extended_constant() == Some(0));
            if !zero || !operand_is(user, 0, address) {
                return None;
            }
            for u in users(user.get_first_use())? {
                if !access(u, user.as_value_ref(), &mut accesses) {
                    return None;
                }
            }
            elements.push(user);
        } else if !access(user, address, &mut accesses) {
            return None;
        }
    }
    let block = accesses.first()?.0.get_parent();
    if accesses.iter().any(|(i, _)| i.get_parent() != block) {
        return None;
    }
    accesses.sort_by_key(|(i, _)| order.of(*i));
    let mut last: Option<inkwell::llvm_sys::prelude::LLVMValueRef> = None;
    let mut forwards = Vec::new();
    let mut stores = Vec::new();
    for (i, is_store) in accesses {
        if is_store {
            last = Some(i.get_operand(0)?.value()?.as_value_ref());
            stores.push(i);
        } else {
            let value = last?;
            // SAFETY: both are live values of this module.
            if unsafe { LLVMTypeOf(value) != LLVMTypeOf(i.as_value_ref()) } {
                return None;
            }
            forwards.push((i, value));
        }
    }
    let mut rest = stores;
    rest.extend(elements);
    Some(WordPlan { forwards, rest })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{llvm_context, COMPILE_LOCK};
    use inkwell::AddressSpace;
    use inkwell::OptimizationLevel;
    use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine};

    fn host_machine() -> TargetMachine {
        Target::initialize_native(&InitializationConfig::default()).unwrap();
        let triple = TargetMachine::get_default_triple();
        Target::from_triple(&triple)
            .unwrap()
            .create_target_machine(&triple, "", "", OptimizationLevel::None, RelocMode::PIC, CodeModel::Default)
            .unwrap()
    }

    /// A function named `name` whose frame holds an array of `words` words,
    /// handed to an external function so that it cannot be dropped.
    fn with_frame(module: &Module<'static>, name: &str, words: u32) {
        let ctx = llvm_context();
        let ptr = ctx.ptr_type(AddressSpace::default());
        let take = module.get_function("take").unwrap_or_else(|| {
            module.add_function("take", ctx.void_type().fn_type(&[ptr.into()], false), None)
        });
        let f = module.add_function(name, ctx.void_type().fn_type(&[], false), None);
        let b = ctx.create_builder();
        b.position_at_end(ctx.append_basic_block(f, "entry"));
        let array = b.build_alloca(ctx.i64_type().array_type(words), "array").unwrap();
        b.build_call(take, &[array.into()], "").unwrap();
        b.build_return(None).unwrap();
    }

    #[test]
    fn a_frame_past_the_limit_is_reported_by_its_written_name() {
        let _guard = COMPILE_LOCK.lock().unwrap();
        let module = llvm_context().create_module("frame_warning_test");
        let big = typelisp_mem::ConcreteNames::symbol("demo::big");
        let small = typelisp_mem::ConcreteNames::symbol("demo::small");
        with_frame(&module, &big, (FRAME_WARNING_BYTES / 8 + 1000) as u32);
        with_frame(&module, &small, 16);
        take_frame_warnings();
        prepare(&module).unwrap();
        host_machine().write_to_memory_buffer(&module, FileType::Object).unwrap();
        let warnings = take_frame_warnings();
        assert_eq!(warnings.len(), 1, "{:?}", warnings);
        assert!(warnings[0].starts_with("warning: compiled function `demo::big` needs a "), "{}", warnings[0]);
        assert!(warnings[0].contains(&format!("(more than {})", FRAME_WARNING_BYTES)), "{}", warnings[0]);
    }

    /// `take(a)` then `take(b)`, each array filled just before its call: the
    /// two spans do not overlap, so they share one array. Filling `c` before
    /// `d`'s call and handing it over after keeps `c` and `d` apart.
    #[test]
    fn argument_arrays_whose_spans_do_not_overlap_share_one() {
        let _guard = COMPILE_LOCK.lock().unwrap();
        let ctx = llvm_context();
        let module = ctx.create_module("argument_pool_test");
        let i64t = ctx.i64_type();
        let ptr = ctx.ptr_type(AddressSpace::default());
        let take = module.add_function("take", ctx.void_type().fn_type(&[ptr.into()], false), None);
        let f = module.add_function("f", ctx.void_type().fn_type(&[i64t.into()], false), None);
        let b = ctx.create_builder();
        b.position_at_end(ctx.append_basic_block(f, "entry"));
        let x = f.get_nth_param(0).unwrap().into_int_value();
        let arrays: Vec<_> = (0..4).map(|n| b.build_alloca(i64t.array_type(2), &format!("args{}", n)).unwrap()).collect();
        let fill = |a| {
            let e = unsafe { b.build_gep(i64t, a, &[i64t.const_int(1, false)], "e").unwrap() };
            b.build_store(e, x).unwrap();
        };
        let call = |a: inkwell::values::PointerValue<'static>| {
            b.build_call(take, &[a.into()], "").unwrap();
        };
        fill(arrays[0]);
        call(arrays[0]);
        fill(arrays[1]);
        call(arrays[1]);
        fill(arrays[2]);
        fill(arrays[3]);
        call(arrays[3]);
        call(arrays[2]);
        b.build_return(None).unwrap();
        prepare(&module).unwrap();
        let allocas = f
            .get_first_basic_block()
            .unwrap()
            .get_instructions()
            .filter(|i| i.get_opcode() == InstructionOpcode::Alloca)
            .count();
        assert_eq!(allocas, 2, "{}", module.print_to_string().to_string());
    }

    #[test]
    fn a_word_written_and_read_in_one_block_leaves_no_slot() {
        let _guard = COMPILE_LOCK.lock().unwrap();
        let ctx = llvm_context();
        let module = ctx.create_module("block_local_word_test");
        let i64t = ctx.i64_type();
        let f = module.add_function("f", i64t.fn_type(&[i64t.into()], false), None);
        let b = ctx.create_builder();
        b.position_at_end(ctx.append_basic_block(f, "entry"));
        let slot = b.build_alloca(i64t.array_type(1), "flag").unwrap();
        let x = f.get_nth_param(0).unwrap().into_int_value();
        let element = unsafe { b.build_gep(i64t, slot, &[i64t.const_zero()], "flag0").unwrap() };
        b.build_store(element, x).unwrap();
        let v = b.build_load(i64t, element, "v").unwrap();
        b.build_return(Some(&v)).unwrap();
        prepare(&module).unwrap();
        let text = module.print_to_string().to_string();
        assert!(!text.contains("alloca"), "{}", text);
        assert!(text.contains("ret i64 %0"), "{}", text);
    }
}
