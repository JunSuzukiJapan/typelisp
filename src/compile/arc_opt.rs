//! A minimal, Swift-ARC-Optimizer-style peephole pass: eliminates a
//! `build-closure-retain` instruction group immediately followed — in the
//! same basic block, with nothing else in between — by a
//! `build-closure-release` call on the exact same closure value. Provably safe:
//! `build-closure-retain` is *inlined* (`load rc; add 1; store rc` against
//! the box's own refcount slot — see
//! [`crate::eval::interp::llvm_builder_build_closure_retain`]'s doc
//! comment), not a real call, so there's no call boundary between the two
//! that could let anything else observe the temporarily-bumped refcount;
//! running the pair leaves the refcount at exactly its pre-retain value,
//! and — because a retain always leaves the count at least `2` before
//! release's own decrement — the pair can never trigger release's
//! free/cascade branch either way. Eliding both is therefore behaviorally
//! identical to running them, just without the wasted refcount churn.
//!
//! Deliberately conservative: only ever matches the *exact* six-instruction
//! shape `build-closure-retain` is known to emit today (`inttoptr`, two
//! `getelementptr`s, `load`, `add`, `store`), immediately followed by a
//! `call void @__typelisp_closure_release`, with every data dependency
//! (which value flows into which, checked by raw `LLVMValueRef` identity
//! via [`AsValueRef`]) verified — not a general alias-analysis ARC
//! optimizer. No currently-shipped `compiler.rs` codegen path actually
//! produces this adjacent shape (every real `build-closure-retain` call
//! site — `retain-bindings`, `compile-escaping-env-args`, `compile-if-branch`
//! — has unrelated IR between its own retain and whatever release
//! eventually matches it), so this pass is a no-op on everything
//! `compiler.rs` emits today; it exists as a forward-looking, verifiably
//! correct building block for whenever a future codegen path *does*
//! produce the adjacent shape (or a raw caller builds one directly, as
//! `tests/compile_test.rs`'s `closure_retain_then_release_leaves_it_still_callable`
//! already does — see this module's own test below, which runs the pass
//! against exactly that shape).

use std::convert::TryFrom;

use inkwell::basic_block::BasicBlock;
use inkwell::module::Module;
use inkwell::values::{AsValueRef, CallSiteValue, InstructionOpcode, InstructionValue, Operand};

/// Runs the pass over every basic block of every function in `module`,
/// erasing each matched retain/release pair in place. Returns the number of
/// pairs eliminated (purely informational — callers don't need to act on
/// it, but it's what makes the pass's effect directly assertable in a
/// test).
pub fn eliminate_redundant_retain_release_pairs(module: &Module<'static>) -> usize {
    let mut eliminated = 0;
    let mut function = module.get_first_function();
    while let Some(f) = function {
        let mut block = f.get_first_basic_block();
        while let Some(b) = block {
            eliminated += eliminate_in_block(b);
            block = b.get_next_basic_block();
        }
        function = f.get_next_function();
    }
    eliminated
}

fn eliminate_in_block(block: BasicBlock<'static>) -> usize {
    let mut eliminated = 0;
    // Walked forward with `next` captured *before* any erase below — every
    // erase only ever touches instructions strictly before `next`, so it
    // can never invalidate this ongoing walk.
    let mut current = block.get_first_instruction();
    while let Some(instr) = current {
        let next = instr.get_next_instruction();
        if let Some(released) = release_call_argument(instr) {
            if let Some(group) = match_retain_group_ending_before(instr) {
                if group.retained_value == released {
                    for g in &group.instructions {
                        g.erase_from_basic_block();
                    }
                    instr.erase_from_basic_block();
                    eliminated += 1;
                }
            }
        }
        current = next;
    }
    eliminated
}

/// If `instr` is a `call void @__typelisp_closure_release(i64 closure)`,
/// returns that call's sole `closure` argument's raw value identity — the
/// value this specific release call's target pass needs to match a
/// preceding retain group against. `None` for anything else (including a
/// call to some other function entirely, e.g. a compiled method/`rt_*`
/// shim).
fn release_call_argument(instr: InstructionValue<'static>) -> Option<usize> {
    if instr.get_opcode() != InstructionOpcode::Call {
        return None;
    }
    let call = CallSiteValue::try_from(instr).ok()?;
    let callee = call.get_called_fn_value()?;
    if callee.get_name().to_str() != Ok("__typelisp_closure_release") {
        return None;
    }
    operand_ref(instr, 0)
}

/// A matched `build-closure-retain` instruction group: the six
/// instructions it always emits (in program order), and the closure value
/// it retained (the `inttoptr`'s own operand — see
/// [`crate::eval::interp::llvm_builder_build_closure_retain`], which
/// returns this exact value unchanged, so a caller chaining its result
/// straight into `build-closure-release` passes this identical SSA value
/// through).
struct RetainGroup<'ctx> {
    instructions: Vec<InstructionValue<'ctx>>,
    retained_value: usize,
}

fn operand_ref(instr: InstructionValue<'static>, idx: u32) -> Option<usize> {
    match instr.get_operand(idx) {
        Some(Operand::Value(v)) => Some(v.as_value_ref() as usize),
        _ => None,
    }
}

/// Walks backward from `release_call` (not included in the result) matching
/// the exact instruction shape `llvm_builder_build_closure_retain` emits:
/// (in reverse) `store`, `getelementptr`, `add`, `load`, `getelementptr`,
/// `inttoptr` — verifying every data dependency between them (the `store`
/// stores the `add`'s result to the second `getelementptr`'s pointer, the
/// `add`'s first operand is the `load`'s result and its second is the
/// constant `1`, the `load` reads from the first `getelementptr`'s pointer,
/// both `getelementptr`s share the same base pointer and slot index, and
/// that base pointer is the `inttoptr`'s own result). `None` if
/// `release_call` isn't immediately preceded by this exact shape — the
/// common case, since no currently-shipped `compiler.rs` codegen path
/// produces it (see this module's doc comment); a real gap here (an
/// unrelated instruction sitting between the retain and the release) is the
/// expected, safe failure mode, not a bug to work around.
fn match_retain_group_ending_before(release_call: InstructionValue<'static>) -> Option<RetainGroup<'static>> {
    let store = release_call.get_previous_instruction()?;
    if store.get_opcode() != InstructionOpcode::Store {
        return None;
    }
    let stored_value = operand_ref(store, 0)?;
    let store_ptr = operand_ref(store, 1)?;

    let gep2 = store.get_previous_instruction()?;
    if gep2.get_opcode() != InstructionOpcode::GetElementPtr {
        return None;
    }
    if (gep2.as_value_ref() as usize) != store_ptr {
        return None;
    }
    let gep2_base = operand_ref(gep2, 0)?;
    let gep2_index = operand_ref(gep2, 1)?;

    let add = gep2.get_previous_instruction()?;
    if add.get_opcode() != InstructionOpcode::Add {
        return None;
    }
    if (add.as_value_ref() as usize) != stored_value {
        return None;
    }
    let add_lhs = operand_ref(add, 0)?;
    let one = match add.get_operand(1) {
        Some(Operand::Value(v)) => v.into_int_value(),
        _ => return None,
    };
    if !one.is_const() || one.get_zero_extended_constant() != Some(1) {
        return None;
    }

    let load = add.get_previous_instruction()?;
    if load.get_opcode() != InstructionOpcode::Load {
        return None;
    }
    if (load.as_value_ref() as usize) != add_lhs {
        return None;
    }
    let load_ptr = operand_ref(load, 0)?;

    let gep1 = load.get_previous_instruction()?;
    if gep1.get_opcode() != InstructionOpcode::GetElementPtr {
        return None;
    }
    if (gep1.as_value_ref() as usize) != load_ptr {
        return None;
    }
    let gep1_base = operand_ref(gep1, 0)?;
    let gep1_index = operand_ref(gep1, 1)?;
    if gep1_base != gep2_base || gep1_index != gep2_index {
        return None;
    }

    let inttoptr = gep1.get_previous_instruction()?;
    if inttoptr.get_opcode() != InstructionOpcode::IntToPtr {
        return None;
    }
    if (inttoptr.as_value_ref() as usize) != gep1_base {
        return None;
    }
    let retained_value = operand_ref(inttoptr, 0)?;

    Some(RetainGroup { instructions: vec![inttoptr, gep1, load, add, gep2, store], retained_value })
}

#[cfg(test)]
mod tests {
    use super::eliminate_redundant_retain_release_pairs;
    use crate::compile::{llvm_context, COMPILE_LOCK};
    use crate::{Checker, Heap, Interp, Reader, RtValue};
    use inkwell::OptimizationLevel;

    fn eval_ok(src: &str) -> RtValue {
        let mut h = Heap::with_capacity(1 << 16);
        let r = Reader::new();
        let vs = r.read_all(&mut h, src).expect("read failed");
        let mut chk = Checker::new();
        let mut interp = Interp::new();
        let mut last = RtValue::Unit;
        for v in vs {
            let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
            if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
                last = val;
            }
        }
        last
    }

    /// Builds the exact same module
    /// `tests/compile_test.rs`'s `closure_retain_then_release_leaves_it_still_callable`
    /// does — a `build-closure-retain` immediately followed by a
    /// `build-closure-release` on its own returned value, nothing in
    /// between — then runs the pass and checks it found and erased exactly
    /// one pair, and that the function is still callable afterward and
    /// still produces the same result the un-optimized module already
    /// proved correct (105 = 5 + 100, the captured offset).
    #[test]
    fn eliminates_a_literally_adjacent_retain_release_pair_and_preserves_behavior() {
        let src = r#"
            (defun build-and-run-closure-module () llvm-module
              (let ((m (llvm-module::create "mod")))
                (let ((add-offset-fn (add-function-with-env m "add_offset")))
                  (let ((caller-fn (add-function m "caller")))
                    (let ((b1 (append-block add-offset-fn "entry")))
                      (let ((builder1 (llvm-builder::create)))
                        (position-at-end builder1 b1)
                        (let ((x (load-arg builder1 add-offset-fn 0)))
                          (let ((offset (load-env builder1 add-offset-fn 0)))
                            (build-ret builder1 (build-add builder1 x offset))))))
                    (let ((b2 (append-block caller-fn "entry")))
                      (let ((builder2 (llvm-builder::create)))
                        (position-at-end builder2 b2)
                        (let ((env-arr (alloca-args builder2 1)))
                          (store-arg builder2 env-arr 0 (const-i64 builder2 100))
                          (let ((closure (build-make-closure builder2 add-offset-fn env-arr 1 0)))
                            (let ((retained (build-closure-retain builder2 closure)))
                              (build-closure-release builder2 m retained)
                              (let ((args-arr (alloca-args builder2 1)))
                                (store-arg builder2 args-arr 0 (const-i64 builder2 5))
                                (build-ret builder2 (build-closure-apply builder2 closure args-arr 1))))))))
                    m))))
            (build-and-run-closure-module)
        "#;
        let module = match eval_ok(src) {
            RtValue::LlvmModule(m) => m,
            other => panic!("expected an LlvmModule, got {:?}", other),
        };
        let _guard = COMPILE_LOCK.lock().unwrap();

        let eliminated = eliminate_redundant_retain_release_pairs(&module.borrow());
        assert_eq!(eliminated, 1, "should find and erase exactly the one adjacent retain/release pair");

        module.borrow().verify().expect("module failed verification after the pass ran");

        let engine = module
            .borrow()
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("failed to create JIT execution engine");
        let caller = unsafe {
            engine
                .get_function::<unsafe extern "C" fn(*const i64, u32) -> i64>("caller")
                .expect("failed to look up the compiled `caller` function")
        };
        assert_eq!(unsafe { caller.call(std::ptr::null(), 0) }, 105, "behavior must be unchanged after eliding the redundant pair");
    }

    /// The negative case: a retain immediately followed by a release of a
    /// *different* closure value must not be touched — this is the shape
    /// `retain-bindings`/`release-bindings` could in principle produce for
    /// two unrelated Fn-typed bindings sitting next to each other (not the
    /// same binding), and eliding it would double-free one closure while
    /// leaking the other.
    #[test]
    fn does_not_touch_a_retain_and_release_of_different_closures() {
        let ctx = llvm_context();
        let _guard = COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("distinct_closures_test");
        let release_fn_ty = ctx.void_type().fn_type(&[ctx.i64_type().into()], false);
        module.add_function("__typelisp_closure_release", release_fn_ty, None);

        let i64_ty = ctx.i64_type();
        let ptr_ty = ctx.ptr_type(inkwell::AddressSpace::default());
        let fn_ty = i64_ty.fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        let f = module.add_function("noop", fn_ty, None);
        let entry = ctx.append_basic_block(f, "entry");
        let builder = ctx.create_builder();
        builder.position_at_end(entry);

        let closure_a = f.get_nth_param(0).unwrap().into_pointer_value();
        let closure_a = builder.build_ptr_to_int(closure_a, i64_ty, "a").unwrap();
        let closure_b = i64_ty.const_int(999, false);

        // Retain `closure_a`'s refcount slot...
        let box_a = builder.build_int_to_ptr(closure_a, ptr_ty, "box_a").unwrap();
        let slot_a = unsafe { builder.build_gep(i64_ty, box_a, &[i64_ty.const_int(2, false)], "slot_a").unwrap() };
        let rc_a = builder.build_load(i64_ty, slot_a, "rc_a").unwrap().into_int_value();
        let rc_a2 = builder.build_int_add(rc_a, i64_ty.const_int(1, false), "rc_a2").unwrap();
        let slot_a2 = unsafe { builder.build_gep(i64_ty, box_a, &[i64_ty.const_int(2, false)], "slot_a2").unwrap() };
        builder.build_store(slot_a2, rc_a2).unwrap();

        // ...then release `closure_b` (a different value entirely).
        let release_fn = module.get_function("__typelisp_closure_release").unwrap();
        builder.build_call(release_fn, &[closure_b.into()], "").unwrap();
        builder.build_return(Some(&closure_a)).unwrap();
        module.verify().expect("module failed verification");

        let eliminated = eliminate_redundant_retain_release_pairs(&module);
        assert_eq!(eliminated, 0, "must not eliminate a retain/release pair over two different closure values");
    }
}
