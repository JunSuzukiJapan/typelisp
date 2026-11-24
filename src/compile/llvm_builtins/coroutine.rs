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
use super::{entry_alloca, expect_llvm_basic_block, expect_llvm_builder, expect_llvm_function, expect_llvm_module, expect_llvm_value, llvm_value_value};

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
    /// How many slots have been handed out.
    next: u64,
    /// The function being built.
    function: FunctionValue<'static>,
    /// The slots handed out marked (`frame-slot-rooted`), which
    /// `coroutine-end` turns into the mask table once every slot is known.
    rooted: Vec<u64>,
    /// The branch ending the prologue, to insert slot *addresses* before.
    ///
    /// A slot address is a GEP off the frame's data pointer, and in a
    /// coroutine the resume blocks use it — so it has to be defined where
    /// both the fresh and the resuming entry can see it. Emitting it where
    /// the island asked would put it in whatever block the binding happened
    /// to be in, which no resume block is dominated by.
    prologue_end: InstructionValue<'static>,
    /// The `scratch-slot` handed out for each role, made on first request.
    pub(super) scratch: HashMap<i64, PointerValue<'static>>,
    /// What the function is.
    shape: Shape,
}

/// The two shapes a compiled function can have (`crate::compile::direct`).
enum Shape {
    Coroutine(CoroCtx),
    Direct(DirectCtx),
}

/// What the coroutine prologue leaves behind for the rest of the function.
///
/// A resume point is a basic block that only the dispatch chain branches to,
/// so the chain cannot be written until every call site has been emitted and
/// claimed its id — the same shape as the slot count, and patched the same
/// way: [`llvm_builder_coroutine_begin`] leaves `dispatch` empty and
/// [`llvm_builder_coroutine_end`] fills it.
struct CoroCtx {
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
    /// The `rt_frame_open` call. Its first argument, the slot count, starts
    /// at zero and is replaced by `coroutine-end` once the true count is
    /// known; its second, the mask table, likewise. Which slots are rooted is
    /// a fact about the function, so it is given where the frame is made, not
    /// on every resume.
    frame_open: InstructionValue<'static>,
}

/// What a direct function's prologue leaves behind.
struct DirectCtx {
    /// How its parameters and result are carried.
    sig: crate::compile::direct::DirectSig,
    /// The stack array the slots are cut from. Its length is a placeholder
    /// until `coroutine-end` knows it; when any slot is rooted, the array is
    /// replaced by a frame the collector sees (`rt_local_frame_open`).
    slots: InstructionValue<'static>,
    /// Where an unwind leaves the function from: a raise after a call, or a
    /// throw with no region left in the function to catch it.
    unwind: BasicBlock<'static>,
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
        match &f.shape {
            Shape::Coroutine(coro) => Ok((f.data, f.function, coro.resumes.len() as u64 + 1)),
            Shape::Direct(_) => Err(EvalError::Internal(format!(
                "{}: `{}` is a direct function, and a direct function cannot stop",
                what,
                f.function.get_name().to_string_lossy()
            ))),
        }
    })
}

/// Records that the resume point `id` resumes at `block`, in the coroutine on
/// `builder`.
fn push_resume(builder: &Rc<RefCell<Builder<'static>>>, id: u64, block: BasicBlock<'static>) {
    FRAME_CTXS.with(|c| {
        if let Some(FrameCtx { shape: Shape::Coroutine(coro), .. }) = c.borrow_mut().get_mut(&builder_key(builder)) {
            coro.resumes.push((id, block));
        }
    });
}

/// The signature of the function on `builder`, when it is a direct one.
fn direct_sig(builder: &Rc<RefCell<Builder<'static>>>) -> Option<crate::compile::direct::DirectSig> {
    FRAME_CTXS.with(|c| match c.borrow().get(&builder_key(builder)) {
        Some(FrameCtx { shape: Shape::Direct(d), .. }) => Some(d.sig.clone()),
        _ => None,
    })
}

/// The block an unwind leaves the direct function on `builder` from.
fn direct_unwind(builder: &Rc<RefCell<Builder<'static>>>) -> Option<BasicBlock<'static>> {
    FRAME_CTXS.with(|c| match c.borrow().get(&builder_key(builder)) {
        Some(FrameCtx { shape: Shape::Direct(d), .. }) => Some(d.unwind),
        _ => None,
    })
}

/// Get-or-create a bodyless `rt_*` declaration, for the same reason
/// [`llvm_module_add_function`] is get-or-create: LLVM's `LLVMAddFunction`
/// does not merge a colliding name, it uniquifies it to `name.1` — and a
/// second, differently-named declaration is a symbol the JIT will not resolve.
fn ensure_declared(module: &Rc<RefCell<Module<'static>>>, name: &str) -> FunctionValue<'static> {
    let sig = crate::compile::externs::runtime_sig(name).unwrap_or_else(|| unreachable!("`{}` is not a runtime function", name));
    crate::compile::runtime_abi::declare(&module.borrow(), name, sig)
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
    let word_ty = crate::compile::carriers::word();
    let key = builder_key(&builder);

    let (data, idx, prologue_end) = FRAME_CTXS.with(|c| {
        let mut map = c.borrow_mut();
        let f = map
            .get_mut(&key)
            .ok_or_else(|| EvalError::Internal("frame-slot: no frame is open on this builder".to_string()))?;
        let idx = f.next;
        f.next += 1;
        if rooted {
            f.rooted.push(idx);
        }
        Ok::<_, EvalError>((f.data, idx, f.prologue_end))
    })?;

    let b = builder.borrow();
    let here = b.get_insert_block();
    b.position_before(&prologue_end);
    let elem_ptr = unsafe {
        b.build_gep(word_ty, data, &[word_ty.const_int(idx, false)], "frame_slot")
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
    let word_ty = crate::compile::carriers::word();
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let slot = unsafe {
        b.build_gep(word_ty, data, &[word_ty.const_int(typelisp_abi::FRAME_PC_SLOT as u64, false)], "frame_pc_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    b.build_store(slot, word_ty.const_int(id, false)).map_err(|e| err("store", e.to_string()))?;
    Ok(())
}

/// Emits a call to `f`, a runtime function that takes its arguments as an
/// array and its length (`rt_frame_call` and its siblings, whose list is a
/// callee and any number of arguments), with `vals` as the array.
fn emit_rt_call(
    b: &Builder<'static>,
    f: FunctionValue<'static>,
    vals: &[BasicValueEnum<'static>],
    name: &str,
    what: &str,
) -> Result<Option<BasicValueEnum<'static>>, EvalError> {
    let ctx = crate::compile::llvm_context();
    let word_ty = crate::compile::carriers::word();
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let arr = entry_alloca(b, word_ty.array_type(vals.len().max(1) as u32), &format!("{}_args", name))
        .map_err(|e| err("alloca", e))?;
    for (i, v) in vals.iter().enumerate() {
        let slot = unsafe {
            b.build_gep(word_ty, arr, &[word_ty.const_int(i as u64, false)], &format!("{}_a{}", name, i))
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
/// ``text
/// entry:      br (%param == 0), fresh, resumed
/// fresh:      %d0 = rt_frame_open(N, MASK);  br prologue
/// resumed:    %pc1 = load %param[PC];  br prologue
/// prologue:   %data = phi [%d0, fresh], [%param, resumed]
///             %pc = phi [0, fresh], [%pc1, resumed];  br dispatch
/// dispatch:   (empty — `coroutine-end` writes the chain)
/// body:       ...
/// ``
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
    let word_ty = crate::compile::carriers::word();
    let what = "coroutine-begin";
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));

    if let Some(sig) = crate::compile::direct::sig_of(&function.get_name().to_string_lossy()) {
        return direct_begin(&builder, function, sig);
    }

    // The prologue owns its declarations: a module can reach here without
    // having been through the driver.
    ensure_declared(&module, "rt_frame_open");
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
        .build_int_compare(inkwell::IntPredicate::EQ, param, word_ty.const_int(0, false), "coro_first")
        .map_err(|e| err("icmp", e.to_string()))?;
    b.build_conditional_branch(is_first, fresh, resumed).map_err(|e| err("br", e.to_string()))?;

    // fresh — make the frame this activation runs in, in one call
    // (`rt_frame_open`), which answers with the address of its words. The
    // size and the mask table are placeholders; `coroutine-end` writes them
    // in.
    b.position_at_end(fresh);
    let (_, fresh_data) = crate::compile::runtime_abi::call(
        &b,
        &module.borrow(),
        "rt_frame_open",
        &[word_ty.const_int(0, false).into(), word_ty.const_int(0, false).into()],
    )
    .map_err(|e| err("call rt_frame_open", e))?;
    let frame_open = fresh_data.as_instruction().ok_or_else(|| err("call rt_frame_open", "the call is not an instruction".to_string()))?;
    b.build_unconditional_branch(prologue).map_err(|e| err("br", e.to_string()))?;

    // resumed — the driver handed back the address of the words of the frame
    // we made last time; where to resume is in them.
    b.position_at_end(resumed);
    let resumed_ptr = b
        .build_int_to_ptr(param, ctx.ptr_type(inkwell::AddressSpace::default()), "resumed_ptr")
        .map_err(|e| err("inttoptr", e.to_string()))?;
    let pc_slot = unsafe {
        b.build_gep(word_ty, resumed_ptr, &[word_ty.const_int(typelisp_abi::FRAME_PC_SLOT as u64, false)], "resumed_pc_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    let resumed_pc = b.build_load(word_ty, pc_slot, "resumed_pc").map_err(|e| err("load", e.to_string()))?.into_int_value();
    b.build_unconditional_branch(prologue).map_err(|e| err("br", e.to_string()))?;

    // prologue — the one block both entries reach, so the only place the
    // frame's data pointer can be defined. A fresh entry starts at the top
    // (`pc` 0).
    b.position_at_end(prologue);
    let data_word = b.build_phi(word_ty, "frame_data").map_err(|e| err("phi", e.to_string()))?;
    data_word.add_incoming(&[(&fresh_data, fresh), (&param, resumed)]);
    let pc_phi = b.build_phi(word_ty, "frame_pc").map_err(|e| err("phi", e.to_string()))?;
    pc_phi.add_incoming(&[(&word_ty.const_int(0, false), fresh), (&resumed_pc, resumed)]);
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
                // The driver protocol's own slots are never handed out as
                // locals.
                next: typelisp_abi::FRAME_RESERVED_SLOTS as u64,
                function,
                rooted: Vec::new(),
                prologue_end,
                scratch: HashMap::new(),
                shape: Shape::Coroutine(CoroCtx { body, dispatch, pc, resumes: Vec::new(), handlers: Vec::new(), frame_open }),
            },
        );
    });
    Ok(Value::Empty)
}

/// [`llvm_builder_coroutine_begin`] for a direct function: the prologue is a
/// straight line, since the function is entered once.
///
/// ``text
/// entry:      %slots = alloca i64, N;  br body
/// body:       ...
/// unwind:     (written by `coroutine-end`, when anything leaves through it)
/// ``
///
/// `N` is written in by `coroutine-end`, which also replaces the array with a
/// frame the collector sees when any slot turned out to be rooted.
fn direct_begin(
    builder: &Rc<RefCell<Builder<'static>>>,
    function: FunctionValue<'static>,
    sig: crate::compile::direct::DirectSig,
) -> Result<Value, EvalError> {
    let ctx = crate::compile::llvm_context();
    let word_ty = crate::compile::carriers::word();
    let err = |step: &str, e: String| EvalError::Internal(format!("coroutine-begin (direct): {}: {}", step, e));
    let body = ctx.append_basic_block(function, "body");
    let unwind = ctx.append_basic_block(function, "unwind");
    let b = builder.borrow();
    let data = b.build_array_alloca(word_ty, word_ty.const_zero(), "slots").map_err(|e| err("alloca", e.to_string()))?;
    let slots = data.as_instruction().ok_or_else(|| err("alloca", "not an instruction".to_string()))?;
    let prologue_end = b.build_unconditional_branch(body).map_err(|e| err("br", e.to_string()))?;
    b.position_at_end(body);
    crate::compile::direct::set_unwind_exit(function, unwind);
    FRAME_CTXS.with(|c| {
        c.borrow_mut().insert(
            builder_key(builder),
            FrameCtx {
                data,
                next: 0,
                function,
                rooted: Vec::new(),
                prologue_end,
                scratch: HashMap::new(),
                shape: Shape::Direct(DirectCtx { sig, slots, unwind }),
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
    let word_ty = crate::compile::carriers::word();
    let what = "coroutine-suspend";
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let (data, function, id) = open_coroutine_frame(&builder, what)?;

    let resume = ctx.append_basic_block(function, &format!("coro.resume{}", id));
    let b = builder.borrow();
    store_resume_point(&b, data, id, what)?;
    b.build_return(Some(&word_ty.const_int(typelisp_abi::STATUS_SUSPEND as u64, false)))
        .map_err(|e| err("ret", e.to_string()))?;

    b.position_at_end(resume);
    let value_slot = unsafe {
        b.build_gep(word_ty, data, &[word_ty.const_int(typelisp_abi::FRAME_VALUE_SLOT as u64, false)], "coro_wake_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    let result = b.build_load(word_ty, value_slot, "coro_wake").map_err(|e| err("load", e.to_string()))?;
    drop(b);
    push_resume(&builder, id, resume);
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
///
/// A direct function has no driver to pop it: it leaves through its unwind
/// block, which closes what it opened and returns.
pub(super) fn llvm_builder_coroutine_unwind(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    expect_llvm_module(&args[1])?;
    if let Some(unwind) = direct_unwind(&builder) {
        builder.borrow().build_unconditional_branch(unwind).map_err(|e| EvalError::Internal(format!("coroutine-unwind: br: {}", e)))?;
        return Ok(Value::Empty);
    }
    let b = builder.borrow();
    b.build_return(Some(&crate::compile::carriers::word().const_int(typelisp_abi::STATUS_UNWIND as u64, false)))
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
    let word_ty = crate::compile::carriers::word();
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let key = builder_key(&builder);

    let found = FRAME_CTXS.with(|c| {
        let mut map = c.borrow_mut();
        let f = map
            .get_mut(&key)
            .ok_or_else(|| EvalError::Internal(format!("{}: no frame is open on this builder", what)))?;
        let coro = match (&mut f.shape, pad) {
            (Shape::Coroutine(coro), _) => coro,
            // A direct function catches nothing, so a region leaving it has
            // nothing to say.
            (Shape::Direct(_), None) => return Ok(None),
            (Shape::Direct(_), Some(_)) => {
                return Err(EvalError::Internal(format!(
                    "{}: `{}` is a direct function, and a direct function has no handler",
                    what,
                    f.function.get_name().to_string_lossy()
                )))
            }
        };
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
        Ok::<_, EvalError>(Some((f.data, id)))
    })?;
    let Some((data, id)) = found else { return Ok(Value::Empty) };

    let b = builder.borrow();
    let slot = unsafe {
        b.build_gep(word_ty, data, &[word_ty.const_int(typelisp_abi::FRAME_HANDLER_SLOT as u64, false)], "handler_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    b.build_store(slot, word_ty.const_int(id, false)).map_err(|e| err("store", e.to_string()))?;
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
    let word_ty = crate::compile::carriers::word();
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    if env.is_none() && shim == "rt_frame_call" {
        if let Some((callee, sig)) = direct_callee(&module, &head[0]) {
            let b = builder.borrow();
            return direct_call(&b, callee, &sig, args_ptr, argc).map(|v| llvm_value_value(v.into())).map_err(|e| err("direct call", e));
        }
    }
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
            b.build_ptr_to_int(env_ptr, word_ty, "coro_env_word").map_err(|e| err("ptrtoint", e.to_string()))?.into(),
        );
        call_args.push(word_ty.const_int(env_len, false).into());
    }
    for i in 0..argc {
        let slot = unsafe {
            b.build_gep(word_ty, args_ptr, &[word_ty.const_int(i, false)], "coro_arg")
                .map_err(|e| err("gep", e.to_string()))?
        };
        call_args.push(b.build_load(word_ty, slot, "coro_arg_val").map_err(|e| err("load", e.to_string()))?);
    }
    emit_rt_call(&b, frame_call, &call_args, "frame_call", what)?;
    store_resume_point(&b, data, id, what)?;
    b.build_return(Some(&word_ty.const_int(typelisp_abi::STATUS_CALL as u64, false)))
        .map_err(|e| err("ret", e.to_string()))?;

    // Everything after the call belongs to the resume block.
    b.position_at_end(resume);
    let value_slot = unsafe {
        b.build_gep(word_ty, data, &[word_ty.const_int(typelisp_abi::FRAME_VALUE_SLOT as u64, false)], "coro_result_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    let result = b.build_load(word_ty, value_slot, "coro_result").map_err(|e| err("load", e.to_string()))?;
    drop(b);
    push_resume(&builder, id, resume);
    Ok(llvm_value_value(result))
}

/// The direct function a call's callee word names, with its signature: the
/// word is `ptrtoint @f` (`build-fn-address`) and `f` is direct or has a
/// direct copy.
fn direct_callee(module: &Rc<RefCell<Module<'static>>>, callee: &BasicValueEnum<'static>) -> Option<(FunctionValue<'static>, crate::compile::direct::DirectSig)> {
    use inkwell::llvm_sys::core::{LLVMGetConstOpcode, LLVMGetOperand, LLVMIsAConstantExpr, LLVMIsAFunction};
    use inkwell::values::AsValueRef;
    let v = callee.as_value_ref();
    // SAFETY: `v` is a live value; each step checks what it is before
    // reading what only that kind of value has.
    unsafe {
        if LLVMIsAConstantExpr(v).is_null() || LLVMGetConstOpcode(v) != inkwell::llvm_sys::LLVMOpcode::LLVMPtrToInt {
            return None;
        }
        let target = LLVMGetOperand(v, 0);
        if target.is_null() || LLVMIsAFunction(target).is_null() {
            return None;
        }
        let f = FunctionValue::new(target)?;
        let (symbol, sig) = crate::compile::direct::callee(&f.get_name().to_string_lossy())?;
        Some((module.borrow().get_function(&symbol)?, sig))
    }
}

/// A call to the direct function `callee`, its arguments the words in the
/// `argc` slots at `args_ptr`, each converted to the parameter's carrier; and
/// its result converted back to the word.
///
/// The arguments sit in the caller's slots, not in registers, because the
/// island evaluates a call's arguments one at a time into a slot each; LLVM
/// forwards the stores to the loads once the slots are on the stack.
fn direct_call(
    b: &Builder<'static>,
    callee: FunctionValue<'static>,
    sig: &crate::compile::direct::DirectSig,
    args_ptr: PointerValue<'static>,
    argc: u64,
) -> Result<IntValue<'static>, String> {
    let word_ty = crate::compile::carriers::word();
    if argc as usize != sig.params.len() {
        return Err(format!("`{}` takes {} arguments, given {}", callee.get_name().to_string_lossy(), sig.params.len(), argc));
    }
    let mut passed: Vec<inkwell::values::BasicMetadataValueEnum<'static>> = Vec::with_capacity(sig.params.len());
    for (i, carrier) in sig.params.iter().enumerate() {
        if *carrier == crate::compile::carriers::Carrier::Unit {
            continue;
        }
        let slot = unsafe { b.build_gep(word_ty, args_ptr, &[word_ty.const_int(i as u64, false)], "arg_slot").map_err(|e| e.to_string())? };
        let w = b.build_load(word_ty, slot, "arg_word").map_err(|e| e.to_string())?.into_int_value();
        let v = carrier.from_word(b, w)?.expect("a carried parameter");
        passed.push(v.into());
    }
    let call = b.build_call(callee, &passed, "direct").map_err(|e| e.to_string())?;
    crate::compile::raise_check::after_call(b, callee)?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => sig.ret.to_word(b, v),
        inkwell::values::ValueKind::Instruction(_) => Ok(word_ty.const_zero()),
    }
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
    let word_ty = crate::compile::carriers::word();
    let what = "coroutine-end";
    let err = |step: &str, e: String| EvalError::Internal(format!("{}: {}: {}", what, step, e));
    let key = builder_key(&builder);

    let f = FRAME_CTXS.with(|c| {
        c.borrow_mut()
            .remove(&key)
            .ok_or_else(|| EvalError::Internal(format!("{}: no frame is open on this builder", what)))
    })?;
    let coro = match f.shape {
        Shape::Coroutine(coro) => coro,
        Shape::Direct(direct) => {
            let b = builder.borrow();
            return direct_end(&b, &module, f.function, f.data, f.next, &f.rooted, f.prologue_end, direct, value.into_int_value())
                .map(|()| Value::Empty)
                .map_err(|e| err("direct", e));
        }
    };

    let b = builder.borrow();
    let value_slot = unsafe {
        b.build_gep(word_ty, f.data, &[word_ty.const_int(typelisp_abi::FRAME_VALUE_SLOT as u64, false)], "coro_ret_slot")
            .map_err(|e| err("gep", e.to_string()))?
    };
    b.build_store(value_slot, value).map_err(|e| err("store", e.to_string()))?;
    b.build_return(Some(&word_ty.const_int(typelisp_abi::STATUS_RETURN as u64, false)))
        .map_err(|e| err("ret", e.to_string()))?;

    if !coro.frame_open.set_operand(0, word_ty.const_int(f.next, false)) {
        return Err(EvalError::Internal(format!("{}: could not write the slot count into the prologue", what)));
    }
    if let Some(table) = rooted_table(&module, f.function, &f.rooted, f.next) {
        if !coro.frame_open.set_operand(1, table) {
            return Err(err("rooted slots", "could not write the table's address into the prologue".to_string()));
        }
    }

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
        let next = ctx.append_basic_block(f.function, &format!("coro.dispatch{}", id + 1));
        let hit = b
            .build_int_compare(inkwell::IntPredicate::EQ, coro.pc, word_ty.const_int(id, false), "coro_is")
            .map_err(|e| err("icmp", e.to_string()))?;
        b.build_conditional_branch(hit, target, next).map_err(|e| err("br", e.to_string()))?;
        b.position_at_end(next);
    }
    Ok(Value::Empty)
}

/// [`llvm_builder_coroutine_end`] for a direct function: return `value` at
/// the result's carrier, and write the slots.
///
/// The slots are a stack array, `nslots` long. When any of them is rooted
/// they are instead a frame on the task's root stack (`rt_local_frame_open`,
/// all of them, so one mask table describes them as it does a coroutine's),
/// closed on both ways out: the return, and the unwind block, which a raise
/// after a call or an uncaught throw branches to. An unwind block nothing
/// branches to is removed.
#[allow(clippy::too_many_arguments)]
fn direct_end(
    b: &Builder<'static>,
    module: &Rc<RefCell<Module<'static>>>,
    function: FunctionValue<'static>,
    data: PointerValue<'static>,
    nslots: u64,
    rooted: &[u64],
    prologue_end: InstructionValue<'static>,
    direct: DirectCtx,
    value: IntValue<'static>,
) -> Result<(), String> {
    let word_ty = crate::compile::carriers::word();
    let e = |e: inkwell::builder::BuilderError| e.to_string();
    crate::compile::direct::take_unwind_exit(function);

    let frame = match rooted_table(module, function, rooted, nslots) {
        None => {
            if !direct.slots.set_operand(0, word_ty.const_int(nslots, false)) {
                return Err("could not write the slot count into the prologue".to_string());
            }
            None
        }
        Some(table) => {
            // Right after the array, ahead of every slot address cut from it.
            let here = b.get_insert_block();
            b.position_before(&direct.slots.get_next_instruction().unwrap_or(prologue_end));
            let (_, opened) = crate::compile::runtime_abi::call(
                b,
                &module.borrow(),
                "rt_local_frame_open",
                &[word_ty.const_int(nslots, false).into(), table.into()],
            )?;
            let ptr = b.build_int_to_ptr(opened, crate::compile::llvm_context().ptr_type(inkwell::AddressSpace::default()), "frame").map_err(e)?;
            if let Some(block) = here {
                b.position_at_end(block);
            }
            // The slots were cut from the array before anything said whether
            // the collector would have to see them.
            data.replace_all_uses_with(ptr);
            direct.slots.erase_from_basic_block();
            Some(opened)
        }
    };
    let close = |b: &Builder<'static>| -> Result<(), String> {
        if let Some(opened) = frame {
            crate::compile::runtime_abi::call(b, &module.borrow(), "rt_local_frame_close", &[opened.into()])?;
        }
        Ok(())
    };

    if b.get_insert_block().is_some_and(|block| block.get_terminator().is_none()) {
        let answer = direct.sig.ret.from_word(b, value)?;
        close(b)?;
        match answer {
            Some(v) => b.build_return(Some(&v)).map_err(e)?,
            None => b.build_return(None).map_err(e)?,
        };
    }

    if direct.unwind.get_first_use().is_none() {
        // SAFETY: a block nothing branches to, in a function being built.
        unsafe { direct.unwind.delete() }.map_err(|()| "could not remove the unused unwind block".to_string())?;
    } else {
        b.position_at_end(direct.unwind);
        close(b)?;
        match function.get_type().get_return_type() {
            Some(t) => b.build_return(Some(&crate::compile::raise_check::zero_of(t))).map_err(e)?,
            None => b.build_return(None).map_err(e)?,
        };
    }
    Ok(())
}

/// The mask of a frame whose rooted slots are `rooted`, out of `nslots`: a
/// constant table of bit words, bit `i` of word `i / 64` for slot `i`, one
/// word per 64 of the frame's `nslots`, as the word of its address — or
/// `None` when no slot is rooted. The frame points at it rather than copying
/// it, so making a frame costs the same however many slots are rooted.
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
fn rooted_table(
    module: &Rc<RefCell<Module<'static>>>,
    function: FunctionValue<'static>,
    rooted: &[u64],
    nslots: u64,
) -> Option<IntValue<'static>> {
    if rooted.is_empty() {
        return None;
    }
    let word_ty = crate::compile::carriers::word();
    let m = module.borrow();
    let mut bits = vec![0u64; nslots.div_ceil(64) as usize];
    for &slot in rooted {
        bits[(slot / 64) as usize] |= 1u64 << (slot % 64);
    }
    let table_ty = word_ty.array_type(bits.len() as u32);
    let name = format!("{}.rooted_slots", function.get_name().to_string_lossy());
    let table = m.add_global(table_ty, None, &name);
    let words: Vec<_> = bits.iter().map(|w| word_ty.const_int(*w, false)).collect();
    table.set_initializer(&word_ty.const_array(&words));
    table.set_constant(true);
    table.set_linkage(inkwell::module::Linkage::Private);
    table.set_unnamed_addr(true);
    Some(table.as_pointer_value().const_to_int(word_ty))
}

/// `(entry-word builder m entry idx)` — the word of the function's argument
/// `idx` (`entry` `"rt_pending_arg"`) or capture `idx` (`"rt_pending_env"`).
///
/// A coroutine's arguments and captures cannot arrive as parameters — it
/// has one, the frame — so they wait in `typelisp_abi::call_state` for the
/// prologue to copy in, and this asks the runtime function `entry` for one.
/// A direct function's arguments are its parameters, each at its carrier,
/// and this is the parameter as a word; a `()` one has no parameter and is
/// the word `0`. A direct function captures nothing.
pub(super) fn llvm_builder_entry_word(heap: &typelisp_mem::Heap, args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let entry = crate::eval::interp::expect_str(heap, &args[2])?.to_string();
    let idx = match &args[3] {
        Value::Int(n) if *n >= 0 => *n as usize,
        other => return Err(EvalError::Internal(format!("entry-word: {:?} is not an index", other))),
    };
    let err = |e: String| EvalError::Internal(format!("entry-word: {}", e));
    let b = builder.borrow();
    if let Some(sig) = direct_sig(&builder) {
        if entry != "rt_pending_arg" {
            return Err(err(format!("a direct function has no `{}`", entry)));
        }
        let carrier = *sig.params.get(idx).ok_or_else(|| err(format!("no parameter {}", idx)))?;
        let word = match sig.llvm_index(idx) {
            None => crate::compile::carriers::word().const_zero(),
            Some(i) => {
                let f = b.get_insert_block().and_then(|block| block.get_parent()).ok_or_else(|| err("outside any function".to_string()))?;
                let p = f.get_nth_param(i).ok_or_else(|| err(format!("no LLVM parameter {}", i)))?;
                carrier.to_word(&b, p).map_err(err)?
            }
        };
        return Ok(llvm_value_value(word.into()));
    }
    let word = crate::compile::carriers::word();
    let (f, v) = crate::compile::runtime_abi::call(&b, &module.borrow(), &entry, &[word.const_int(idx as u64, false).into()]).map_err(err)?;
    crate::compile::raise_check::after_call(&b, f).map_err(err)?;
    Ok(llvm_value_value(v.into()))
}

/// `(loop-safepoint builder m loop-block)` — the back edge of a loop, to
/// `loop-block`.
///
/// A coroutine offers the scheduler a turn on the way (`rt_loop_safepoint`)
/// and stops there when it is asked to: a loop that calls nothing would
/// otherwise keep every other task waiting. A direct function goes straight
/// back: it is direct only in a program with no other task to run.
pub(super) fn llvm_builder_loop_safepoint(args: &[Value]) -> Result<Value, EvalError> {
    let builder = expect_llvm_builder(&args[0])?;
    let module = expect_llvm_module(&args[1])?;
    let loop_block = expect_llvm_basic_block(&args[2])?;
    let err = |step: &str, e: String| EvalError::Internal(format!("loop-safepoint: {}: {}", step, e));
    if direct_sig(&builder).is_some() {
        builder.borrow().build_unconditional_branch(loop_block).map_err(|e| err("br", e.to_string()))?;
        crate::compile::direct::skipped_safepoint();
        return Ok(Value::Empty);
    }
    let ctx = crate::compile::llvm_context();
    let (data, function, id) = open_coroutine_frame(&builder, "loop-safepoint")?;
    let poll = ctx.append_basic_block(function, "loop-safepoint");
    let resume = ctx.append_basic_block(function, &format!("coro.resume{}", id));
    {
        let b = builder.borrow();
        let (f, offered) = crate::compile::runtime_abi::call(&b, &module.borrow(), "rt_loop_safepoint", &[]).map_err(|e| err("call", e))?;
        crate::compile::raise_check::after_call(&b, f).map_err(|e| err("raise check", e))?;
        let stay = b
            .build_int_compare(inkwell::IntPredicate::EQ, offered, crate::compile::carriers::word().const_zero(), "no_turn")
            .map_err(|e| err("icmp", e.to_string()))?;
        b.build_conditional_branch(stay, loop_block, poll).map_err(|e| err("br", e.to_string()))?;
        b.position_at_end(poll);
        store_resume_point(&b, data, id, "loop-safepoint")?;
        b.build_return(Some(&crate::compile::carriers::word().const_int(typelisp_abi::STATUS_SUSPEND as u64, false)))
            .map_err(|e| err("ret", e.to_string()))?;
        b.position_at_end(resume);
        b.build_unconditional_branch(loop_block).map_err(|e| err("br", e.to_string()))?;
    }
    push_resume(&builder, id, resume);
    Ok(Value::Empty)
}
