//! The `llvm-builder` methods that build a coroutine-ABI body: its frame, its
//! prologue, and the calls, suspensions and unwinds that can stop it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue, InstructionValue, IntValue, PointerValue};
use typelisp_mem::Value;
use crate::eval::interp::EvalError;
use super::{compiled_fn_type, entry_alloca, expect_llvm_basic_block, expect_llvm_builder, expect_llvm_function, expect_llvm_module, expect_llvm_value, llvm_value_value};

// ---- the frame under construction ---------------------------------------
//
// Phase C1. Locals move off the machine stack into a compiled frame, and
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
pub(super) struct FrameCtx {
    /// The address of its word array, to GEP slots out of.
    data: PointerValue<'static>,
    /// The `store` holding the slot count `rt_frame_open` reads. Its value
    /// operand starts at zero and is replaced by `coroutine-end` once the true
    /// count is known.
    size_store: InstructionValue<'static>,
    /// How many slots have been handed out.
    next: u64,
    /// The coroutine prologue's parts.
    coro: CoroCtx,
    /// The `scratch-slot` handed out for each role, made on first request.
    pub(super) scratch: HashMap<i64, PointerValue<'static>>,
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
    /// Where to resume — 0 on a fresh entry, the frame's
    /// `FRAME_PC_SLOT` on a later one — merged in the prologue so it
    /// dominates the whole chain.
    pc: IntValue<'static>,
    /// Every call site's id and the block it resumes into, in the order they
    /// were claimed. A region's dispatch block is in here too: the driver
    /// resumes a handler exactly the way it resumes a call.
    resumes: Vec<(u64, BasicBlock<'static>)>,
    /// The id each region's dispatch block was given, so the many places that
    /// install the same handler (its entry, its exits, each cleanup copy) all
    /// name one id.
    handlers: Vec<(BasicBlock<'static>, u64)>,
    /// The `store` of `rt_frame_open`'s mask table address, in `coro.fresh` —
    /// null until `coroutine-end` knows the table. Which slots are rooted is a
    /// fact about the function, so it is given where the frame is made, not on
    /// every resume.
    rooted_table_store: InstructionValue<'static>,
    /// The slots handed out marked (`frame-slot-rooted`), which `coroutine-end`
    /// turns into the mask table once every slot is known.
    rooted: Vec<u64>,
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
    /// Keyed by `Rc::as_ptr` of the builder. An entry lives from
    /// `coroutine-begin` to `coroutine-end`, which is exactly one function's
    /// construction; the three
    /// call sites each drop their builder afterwards, so nothing accumulates.
    pub(super) static FRAME_CTXS: RefCell<HashMap<usize, FrameCtx>> = RefCell::new(HashMap::new());
}

pub(super) fn builder_key(b: &Rc<RefCell<Builder<'static>>>) -> usize {
    Rc::as_ptr(b) as usize
}

/// The open coroutine frame on `builder`, plus the id the resume point about
/// to be emitted claims: `(data, function, id)`.
///
/// Resume ids start at 1 — 0 is "start at the top", which is what an untouched
/// `pc` already says. Claiming one is a read, not a write: the caller appends
/// its block and registers it, which is what makes the *next* id one higher.
///
/// `what` names the builtin, for the failure reported here: no frame open on
/// this builder.
fn open_coroutine_frame(
    builder: &Rc<RefCell<Builder<'static>>>,
    what: &str,
) -> Result<(PointerValue<'static>, FunctionValue<'static>, u64), EvalError> {
    let key = builder_key(builder);
    FRAME_CTXS.with(|c| {
        let mut map = c.borrow_mut();
        let f = map
            .get_mut(&key)
            .ok_or_else(|| EvalError::Internal(format!("{}: no frame is open on this builder", what)))?;
        let id = f.coro.resumes.len() as u64 + 1;
        Ok((f.data, f.coro.function, id))
    })
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

/// `(frame-slot builder)` — the address of a fresh, **unmasked** frame slot.
///
/// Unmasked means the collector never reads it as a reference, which is what a
/// raw `i32`/`f64` local needs (`Repr::class`'s non-collectable case).
pub(super) fn llvm_builder_frame_slot(args: &[Value]) -> Result<Value, EvalError> {
    frame_slot_impl(args, false)
}

/// `(frame-slot-rooted builder m)` — the address of a fresh frame slot, marked
/// so the collector traces it.
///
/// The mark is a bit in the function's mask table (`set_rooted_table`), not
/// code: the slot is a root for as long as the frame lives, so no early exit
/// can leave anything out of step.
pub(super) fn llvm_builder_frame_slot_rooted(args: &[Value]) -> Result<Value, EvalError> {
    frame_slot_impl(args, true)
}

/// A slot's address moves out of the block that asked for it, to
/// `coro.prologue` — the one block every entry falls into — because the resume
/// blocks use it and nothing in the body dominates them. A rooted slot is only
/// recorded here (`CoroCtx::rooted`); `coroutine-end` puts them all in the
/// mask table.
fn frame_slot_impl(args: &[Value], rooted: bool) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let key = builder_key(&builder);

    let (data, idx, prologue_end) = FRAME_CTXS.with(|c| {
        let mut map = c.borrow_mut();
        let f = map
            .get_mut(&key)
            .ok_or_else(|| EvalError::Internal("frame-slot: no frame is open on this builder".to_string()))?;
        let idx = f.next;
        f.next += 1;
        if rooted {
            f.coro.rooted.push(idx);
        }
        Ok::<_, EvalError>((f.data, idx, f.coro.prologue_end))
    })?;

    let b = builder.borrow();
    let here = b.get_insert_block();
    b.position_before(&prologue_end);
    let elem_ptr = unsafe {
        b.build_gep(i64t, data, &[i64t.const_int(idx, false)], "frame_slot")
            .map_err(|e| EvalError::Internal(format!("frame-slot: gep: {}", e)))?
    };
    if let Some(block) = here {
        b.position_at_end(block);
    }
    Ok(llvm_value_value(elem_ptr.into()))
}

// ---------------------------------------------------------------------------
// The coroutine prologue (Phase C2)
// ---------------------------------------------------------------------------
//
// A coroutine body can stop in the middle: it is entered again, from the top,
// every time it resumes, and has to find its way back to where it left off.
//
// That is why the prologue is not a straight line. Two things it computes —
// the frame's data pointer and where to resume — are used by resume blocks
// that the entry path never reaches, so they cannot be computed on the entry
// path: SSA requires a definition to dominate its uses. They are merged in a
// `coro.prologue` block that both the fresh and the resuming entry fall into,
// which is what makes it dominate everything after.

/// Records resume point `id` in the frame whose words are at `data`, before
/// the activation hands control back: a plain store into its
/// [`FRAME_PC_SLOT`](typelisp_abi::FRAME_PC_SLOT).
fn store_resume_point(b: &Builder<'static>, data: PointerValue<'static>, id: u64, what: &str) -> Result<(), EvalError> {
    let i64t = crate::compile::llvm_context().i64_type();
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let slot = unsafe {
        b.build_gep(i64t, data, &[i64t.const_int(typelisp_abi::FRAME_PC_SLOT as u64, false)], "frame_pc_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    b.build_store(slot, i64t.const_int(id, false)).map_err(|e| err("store", e.to_string()))?;
    Ok(())
}

/// Emits a call to an `rt_*` entry point with `vals` as its argument array.
///
/// Every one of them takes `(const i64 *args, u32 argc)`, so the array is an
/// `alloca` filled by hand — the same three lines repeated at a dozen sites
/// above, factored out here because the coroutine prologue makes eight such
/// calls.
fn emit_rt_call(
    b: &Builder<'static>,
    f: FunctionValue<'static>,
    vals: &[BasicValueEnum<'static>],
    name: &str,
    what: &str,
) -> Result<Option<BasicValueEnum<'static>>, EvalError> {
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let arr = entry_alloca(b, i64t.array_type(vals.len().max(1) as u32), &format!("{}_args", name))
        .map_err(|e| err("alloca", e))?;
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
    crate::compile::raise_check::after_call(b, f).map_err(|e| err("raise check", e))?;
    Ok(match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Some(v),
        inkwell::values::ValueKind::Instruction(_) => None,
    })
}

/// `(coroutine-begin builder m f)` — emit the prologue of a coroutine-ABI
/// function and leave the builder at the start of its body.
///
/// ```text
/// entry:      br (%param == 0), fresh, resumed
/// fresh:      %d0 = rt_frame_open(N, MASK);  br prologue
/// resumed:    %pc1 = load %param[PC];  br prologue
/// prologue:   %data = phi [%d0, fresh], [%param, resumed]
///             %pc = phi [0, fresh], [%pc1, resumed];  br dispatch
/// dispatch:   (empty — `coroutine-end` writes the chain)
/// body:       ...
/// ```
///
/// `rt_frame_open` opens the frame on the task's root stack with the
/// function's mask table as its mask (`N` and `MASK` are written in by
/// `coroutine-end`, once every slot is known) and publishes it — which is how
/// the driver learns the frame at all: it is made *inside* the callee, after
/// the call has begun, so there is no other moment the caller could have been
/// told. It is the one runtime call on the way in, on the path every call
/// takes; a resumed entry makes none, and neither does the return: the driver
/// closes the frame when it takes the answer (`Heap::open_frame`).
pub(super) fn llvm_builder_coroutine_begin(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let function = expect_llvm_function(&args[2])?;
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let what = "coroutine-begin";
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));

    // The prologue owns its declarations: a module can reach here without
    // having been through the driver.
    let frame_open = ensure_declared(&module, "rt_frame_open");
    ensure_declared(&module, "rt_frame_call");
    ensure_declared(&module, "rt_pending_arg");
    ensure_declared(&module, "rt_pending_argc");
    // The capture-side counterparts. A parameter and a capture are read by
    // separate indices out of separate lists, so declaring only the argument
    // half left any body with captures reaching for an undeclared
    // `rt_pending_env` -- and a `labels` sibling that captures is the
    // ordinary case, not an exotic one.
    ensure_declared(&module, "rt_pending_env");
    ensure_declared(&module, "rt_pending_envc");
    ensure_declared(&module, "rt_frame_call_env");

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
    let is_first = b
        .build_int_compare(inkwell::IntPredicate::EQ, param, i64t.const_int(0, false), "coro_first")
        .map_err(|e| err("icmp", e.to_string()))?;
    b.build_conditional_branch(is_first, fresh, resumed).map_err(|e| err("br", e.to_string()))?;

    // fresh — make the frame this activation runs in, in one call
    // (`rt_frame_open`), which answers with the address of its words. The
    // size and the mask table are placeholders; `coroutine-end` writes them
    // in.
    b.position_at_end(fresh);
    let open_args = entry_alloca(&b, i64t.array_type(2), "frame_open_args").map_err(|e| err("alloca", e))?;
    let open_word = |n: u64, name: &str| unsafe {
        b.build_gep(i64t, open_args, &[i64t.const_int(n, false)], name).map_err(|e| err("gep", e.to_string()))
    };
    let size_store =
        b.build_store(open_word(0, "frame_size_slot")?, i64t.const_int(0, false)).map_err(|e| err("store", e.to_string()))?;
    let rooted_table_store =
        b.build_store(open_word(1, "frame_mask_slot")?, i64t.const_int(0, false)).map_err(|e| err("store", e.to_string()))?;
    let fresh_data = match b
        .build_call(frame_open, &[open_args.into(), ctx.i32_type().const_int(2, false).into()], "fresh_data")
        .map_err(|e| err("call rt_frame_open", e.to_string()))?
        .try_as_basic_value()
    {
        inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
        inkwell::values::ValueKind::Instruction(_) => {
            return Err(EvalError::Internal(format!("{}: rt_frame_open returned no value", what)))
        }
    };
    b.build_unconditional_branch(prologue).map_err(|e| err("br", e.to_string()))?;

    // resumed — the driver handed back the address of the words of the frame
    // we made last time; where to resume is in them.
    b.position_at_end(resumed);
    let resumed_ptr = b
        .build_int_to_ptr(param, ctx.ptr_type(inkwell::AddressSpace::default()), "resumed_ptr")
        .map_err(|e| err("inttoptr", e.to_string()))?;
    let pc_slot = unsafe {
        b.build_gep(i64t, resumed_ptr, &[i64t.const_int(typelisp_abi::FRAME_PC_SLOT as u64, false)], "resumed_pc_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    let resumed_pc = b.build_load(i64t, pc_slot, "resumed_pc").map_err(|e| err("load", e.to_string()))?.into_int_value();
    b.build_unconditional_branch(prologue).map_err(|e| err("br", e.to_string()))?;

    // prologue — the one block both entries reach, so the only place the
    // frame's data pointer can be defined. A fresh entry starts at the top
    // (`pc` 0).
    b.position_at_end(prologue);
    let data_word = b.build_phi(i64t, "frame_data").map_err(|e| err("phi", e.to_string()))?;
    data_word.add_incoming(&[(&fresh_data, fresh), (&param, resumed)]);
    let pc_phi = b.build_phi(i64t, "frame_pc").map_err(|e| err("phi", e.to_string()))?;
    pc_phi.add_incoming(&[(&i64t.const_int(0, false), fresh), (&resumed_pc, resumed)]);
    let pc = pc_phi.as_basic_value().into_int_value();
    let data = b
        .build_int_to_ptr(
            data_word.as_basic_value().into_int_value(),
            ctx.ptr_type(inkwell::AddressSpace::default()),
            "frame_ptr",
        )
        .map_err(|e| err("inttoptr", e.to_string()))?;
    let prologue_end = b.build_unconditional_branch(dispatch).map_err(|e| err("br", e.to_string()))?;

    // dispatch stays empty until every call site has claimed its id.

    b.position_at_end(body);
    FRAME_CTXS.with(|c| {
        c.borrow_mut().insert(
            builder_key(&builder),
            FrameCtx {
                data,
                size_store,
                // The driver protocol's own slots are never handed out as
                // locals.
                next: typelisp_abi::FRAME_RESERVED_SLOTS as u64,
                scratch: HashMap::new(),
                coro: CoroCtx {
                    function,
                    body,
                    dispatch,
                    pc,
                    resumes: Vec::new(),
                    handlers: Vec::new(),
                    rooted_table_store,
                    rooted: Vec::new(),
                    prologue_end,
                },
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
/// reads them back, and `rt_frame_open` makes its frame in that window. They survive
/// it because the caller already keeps every collectable argument in a marked
/// frame slot of its own (`root-temporary`), which is the same reason they
/// survived the callee's allocation under the original ABI.
pub(super) fn llvm_builder_coroutine_call(args: &[Value]) -> Result<Value, EvalError> {
    coroutine_call_impl(args, "rt_frame_call", "coroutine-call", 1, None)
}

/// `(coroutine-apply builder m closure args-ptr argc)` — hand an `apply` to
/// the driver and come back with its answer.
///
/// [`llvm_builder_coroutine_call`] with a function *value* in place of an
/// address. The difference is entirely on the driver's side: it has to look
/// at the value to know who can run it, and for an interpreted closure it
/// hands the call to the continuation stack instead of running it on a frame
/// of its own. From here the two are one instruction sequence — which is the
/// point. An `apply` used to be a plain call to `rt_apply_any`, a C function
/// that had to come back with an answer, so everything underneath it stood on
/// a machine frame that could not be put down.
pub(super) fn llvm_builder_coroutine_apply(args: &[Value]) -> Result<Value, EvalError> {
    coroutine_call_impl(args, "rt_frame_apply", "coroutine-apply", 1, None)
}

/// `(coroutine-suspend builder m)` — hand control back with `STATUS_SUSPEND`,
/// and resume here when the scheduler says the wait is over.
///
/// [`llvm_builder_coroutine_call`] with the callee left out, and that is the
/// whole of what suspension costs once calls already work this way: a call
/// and a suspension both end the activation and both come back through the
/// `pc` dispatch. What differs is the status word and who decides when to
/// re-enter.
///
/// The suspending builtin's own shim (`rt_suspend_*`) has already been called
/// by the time this runs and has recorded *what* the task waits for, so
/// nothing about that reaches here. The answer arrives in the frame's value
/// slot — the same place a returning callee leaves its answer, because from
/// the resuming function's side those are the same thing.
pub(super) fn llvm_builder_coroutine_suspend(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    expect_llvm_module(&args[1])?;
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let what = "coroutine-suspend";
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let key = builder_key(&builder);
    let (data, function, id) = open_coroutine_frame(&builder, what)?;

    let resume = ctx.append_basic_block(function, &format!("coro.resume{}", id));
    let b = builder.borrow();
    store_resume_point(&b, data, id, what)?;
    b.build_return(Some(&i64t.const_int(typelisp_abi::STATUS_SUSPEND as u64, false)))
        .map_err(|e| err("ret", e.to_string()))?;

    b.position_at_end(resume);
    let value_slot = unsafe {
        b.build_gep(i64t, data, &[i64t.const_int(typelisp_abi::FRAME_VALUE_SLOT as u64, false)], "coro_wake_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    let result = b.build_load(i64t, value_slot, "coro_wake").map_err(|e| err("load", e.to_string()))?;

    FRAME_CTXS.with(|c| {
        let mut map = c.borrow_mut();
        if let Some(coro) = map.get_mut(&key).map(|f| &mut f.coro) {
            coro.resumes.push((id, resume));
        }
    });
    Ok(llvm_value_value(result))
}

/// `(coroutine-unwind builder m)` — end the activation with
/// `STATUS_UNWIND`.
///
/// What a region that declines the unwind in flight does when there is no
/// enclosing region left in this function: the driver pops this frame,
/// cutting its roots back, and goes on looking below. The dynamic half of the
/// island's `emit-unwind-onward` — the static half is still a `br` to the
/// enclosing dispatch block, because that one is in this same function and
/// needs no driver at all.
///
/// The driver truncates the root stack to the depth this frame was entered at,
/// which closes the frame and drops everything it pushed after it in one step
/// — as it does on a return.
pub(super) fn llvm_builder_coroutine_unwind(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    expect_llvm_module(&args[1])?;
    let ctx = crate::compile::llvm_context();
    let b = builder.borrow();
    b.build_return(Some(&ctx.i64_type().const_int(typelisp_abi::STATUS_UNWIND as u64, false)))
        .map_err(|e| EvalError::Internal(format!("coroutine-unwind: ret: {}", e)))?;
    Ok(Value::Empty)
}

/// `(frame-set-handler builder m pad)` — say that an unwind reaching this
/// frame resumes at `pad`.
pub(super) fn llvm_builder_frame_set_handler(args: &[Value]) -> Result<Value, EvalError> {
    let pad = expect_llvm_basic_block(&args[2])?;
    frame_handler(args, Some(pad), "frame-set-handler")
}

/// `(frame-clear-handler builder m)` — say that this frame catches nothing,
/// which is what leaving a region means.
pub(super) fn llvm_builder_frame_clear_handler(args: &[Value]) -> Result<Value, EvalError> {
    frame_handler(args, None, "frame-clear-handler")
}

/// Writes the frame's handler slot, giving `pad` a resume id the first time it
/// is named.
///
/// The id goes in the same dispatch chain a call's resume point goes in, and
/// the same `pc` field names it — a handler and a resume point are the same
/// thing from the frame's side: a place to come back to that the frame's own
/// machine activation will not be around for.
fn frame_handler(args: &[Value], pad: Option<BasicBlock<'static>>, what: &str) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    expect_llvm_module(&args[1])?;
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let key = builder_key(&builder);

    let (data, id) = FRAME_CTXS.with(|c| {
        let mut map = c.borrow_mut();
        let f = map
            .get_mut(&key)
            .ok_or_else(|| EvalError::Internal(format!("{}: no frame is open on this builder", what)))?;
        let coro = &mut f.coro;
        let id = match pad {
            None => 0,
            Some(block) => match coro.handlers.iter().find(|(b, _)| *b == block) {
                Some((_, id)) => *id,
                None => {
                    let id = coro.resumes.len() as u64 + 1;
                    coro.resumes.push((id, block));
                    coro.handlers.push((block, id));
                    id
                }
            },
        };
        Ok::<_, EvalError>((f.data, id))
    })?;

    let b = builder.borrow();
    let slot = unsafe {
        b.build_gep(i64t, data, &[i64t.const_int(typelisp_abi::FRAME_HANDLER_SLOT as u64, false)], "handler_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    b.build_store(slot, i64t.const_int(id, false)).map_err(|e| err("store", e.to_string()))?;
    Ok(Value::Empty)
}

/// `(coroutine-dyn-call builder m vtable slot args-ptr argc)` — hand a `:dyn`
/// method call to the driver and come back with its answer.
///
/// [`llvm_builder_coroutine_apply`] for a callee named by a vtable slot
/// instead of by a value. The slot's *implementation* is the thing that can
/// be anything: a compiled method under either ABI, or — when the concrete
/// type behind the trait object is the user's own and its method is ordinary
/// interpreted code — no address at all, in which case the slot stands for a
/// closure the interpreter has to reify. That was `rt_dyn_call`'s whole
/// reason for existing at the runtime rather than at the call site, and it is
/// the same reason the decision now belongs to the driver.
pub(super) fn llvm_builder_coroutine_dyn_call(args: &[Value]) -> Result<Value, EvalError> {
    coroutine_call_impl(args, "rt_frame_dyn_call", "coroutine-dyn-call", 2, None)
}

/// `(coroutine-call-env builder m target args-ptr argc env-ptr env-len)` —
/// [`llvm_builder_coroutine_call`] for a callee that also has captures.
///
/// The captures are read out of the caller's array by `rt_frame_call_env`
/// before the caller returns, because that array is on a machine stack that is
/// about to go away. Under the original ABI they could be passed as a pointer
/// and read by the callee, which is the difference a call that *returns*
/// makes.
pub(super) fn llvm_builder_coroutine_call_env(args: &[Value]) -> Result<Value, EvalError> {
    let env_ptr = expect_llvm_value(&args[5])?.into_pointer_value();
    let env_len = match &args[6] {
        Value::Int(n) if *n >= 0 => *n as u64,
        other => {
            return Err(EvalError::Internal(format!("coroutine-call-env: {:?} is not a capture count", other)))
        }
    };
    coroutine_call_impl(args, "rt_frame_call_env", "coroutine-call-env", 1, Some((env_ptr, env_len)))
}

/// `head_len` is how many of `args[2..]` are the shim's own leading words —
/// one for a callee (an address or a function value), two for a `:dyn` call's
/// vtable and slot. The argument array and its count follow.
fn coroutine_call_impl(
    args: &[Value],
    shim: &str,
    what: &str,
    head_len: usize,
    env: Option<(PointerValue<'static>, u64)>,
) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let mut head: Vec<BasicValueEnum<'static>> = Vec::with_capacity(head_len + 2);
    for a in &args[2..2 + head_len] {
        head.push(expect_llvm_value(a)?);
    }
    let args_ptr = expect_llvm_value(&args[2 + head_len])?.into_pointer_value();
    let argc = match &args[3 + head_len] {
        Value::Int(n) if *n >= 0 => *n as u64,
        other => return Err(EvalError::Internal(format!("{}: {:?} is not an argument count", what, other))),
    };
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let key = builder_key(&builder);
    let (data, function, id) = open_coroutine_frame(&builder, what)?;

    let resume = ctx.append_basic_block(function, &format!("coro.resume{}", id));
    let frame_call = ensure_declared(&module, shim);

    let b = builder.borrow();
    // rt_frame_call(callee, arg...) — one array, the callee first; the
    // captures-carrying form puts the env between them.
    let mut call_args: Vec<BasicValueEnum<'static>> = Vec::with_capacity(argc as usize + head_len + 2);
    call_args.extend(head);
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
    store_resume_point(&b, data, id, what)?;
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
        if let Some(coro) = map.get_mut(&key).map(|f| &mut f.coro) {
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
pub(super) fn llvm_builder_coroutine_end(args: &[Value]) -> Result<Value, EvalError> {
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
    let coro = f.coro;

    let b = builder.borrow();
    let value_slot = unsafe {
        b.build_gep(i64t, f.data, &[i64t.const_int(typelisp_abi::FRAME_VALUE_SLOT as u64, false)], "coro_ret_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    b.build_store(value_slot, value).map_err(|e| err("store", e.to_string()))?;
    b.build_return(Some(&i64t.const_int(typelisp_abi::STATUS_RETURN as u64, false)))
        .map_err(|e| err("ret", e.to_string()))?;

    if !f.size_store.set_operand(0, i64t.const_int(f.next, false)) {
        return Err(EvalError::Internal(format!("{}: could not write the slot count into the prologue", what)));
    }
    set_rooted_table(&module, &coro, f.next).map_err(|e| err("rooted slots", e))?;

    // The dispatch chain: `pc == 0` is a fresh entry, anything else names a
    // call site. The last arm is unconditional — `pc` is only ever written by
    // a resume point's own store (`store_resume_point`) or, for a handler, by
    // the driver, so past the final comparison there is exactly one place
    // left it can mean.
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

/// Hands `rt_frame_open` the slots in `coro.rooted`, as the function's mask:
/// a constant table of bit words, bit `i` of word `i / 64` for slot `i`, one
/// word per 64 of the frame's `nslots`. The frame points at it rather than
/// copying it, so making a frame costs the same however many slots are
/// rooted. A function that roots none passes null.
///
/// A table rather than a call per slot, because a long body has a rooted slot
/// for nearly every value it keeps in the frame, and a straight line of that
/// many calls is one block that code generation's scheduler and register
/// allocator take time to the square of. The table grows with the body; the
/// code does not.
///
/// A bit set before the slot is written is safe by construction: an
/// untouched slot reads as a raw `0`, whose low bits are `TAG_FIXNUM`, so the
/// collector follows nothing.
fn set_rooted_table(module: &Rc<RefCell<Module<'static>>>, coro: &CoroCtx, nslots: u64) -> Result<(), String> {
    if coro.rooted.is_empty() {
        return Ok(());
    }
    let ctx = crate::compile::llvm_context();
    let i64t = ctx.i64_type();
    let m = module.borrow();
    let mut bits = vec![0u64; nslots.div_ceil(64) as usize];
    for &slot in &coro.rooted {
        bits[(slot / 64) as usize] |= 1u64 << (slot % 64);
    }
    let table_ty = i64t.array_type(bits.len() as u32);
    let name = format!("{}.rooted_slots", coro.function.get_name().to_string_lossy());
    let table = m.add_global(table_ty, None, &name);
    let words: Vec<_> = bits.iter().map(|w| i64t.const_int(*w, false)).collect();
    table.set_initializer(&i64t.const_array(&words));
    table.set_constant(true);
    table.set_linkage(inkwell::module::Linkage::Private);
    table.set_unnamed_addr(true);
    if !coro.rooted_table_store.set_operand(0, table.as_pointer_value().const_to_int(i64t)) {
        return Err("could not write the table's address into the prologue".to_string());
    }
    Ok(())
}
