//! Trait objects and vtables.

use typelisp_mem::Word;
use typelisp_mem::prelude::*;
use core::cell::Cell;
use typelisp_abi::{active_heap, decode, encode, fatal};
use typelisp_mem::Value;
use crate::shared;

// ===========================================================================
// Trait objects and vtables
// ===========================================================================
//
// A trait object is a `BoxedObj::Dyn` fat box: a vtable id plus the concrete
// value. The vtable itself lives *outside* the heap, here — one table per
// (concrete type, trait) pair, holding raw native function pointers in the
// trait's declared method order (`TraitDef::method_order`). A call site knows
// its slot statically, so dispatch is `rt_dyn_vtable` -> naming the slot to
// the driver (`rt_frame_dyn_call`), with no lookup by name or type at run
// time — and the driver is also where a slot whose implementation is
// *interpreted* is handled. Ids are assigned by the interpreter
// (`Interp::vtable_id_for`), which keeps its own parallel table of
// `(type, method)` identities for tree-walking calls.

// vtable id -> slot -> native entry point (`compiled_fn_type`: `i64
// f(i64* args, i32 argc)`), or 0 for a slot never filled in — now
// `shared::RtShared::vtables` (`docs/dev/os-threads-design.md` §5/Phase
// 1d). Each slot is `(entry address, the ABI that address answers to)`.
// The pair travels together because an address alone does not say how to
// call it, and calling one convention as the other is not a type error
// anywhere -- it is a wrong answer or a crash.
//
// `(vtable id, trait id)` -> the vtable id of the *same concrete type* for
// that trait — the supertrait upcast table ([`rt_dyn_upcast`]), now
// `shared::RtShared::upcasts`. Filled in by the interpreter from
// `Expr::DynBox::supers` ([`upcast_define`]) and by AOT startup
// ([`rt_upcast_set`]), and reset alongside the vtables, whose id space it
// is keyed by.

/// Clears this thread's vtable and upcast tables — the trait-object
/// counterpart of [`reset_global_table`], called from `Interp::new` for the
/// same reason: vtable ids are per-`Interp`, so a stale table from an earlier
/// one on a reused `cargo test` worker thread would answer with function
/// pointers into a JIT module that has since been dropped.
pub fn reset_vtable_table() {
    let shared = shared::rt_shared();
    shared.vtables.write().clear();
    shared.upcasts.write().clear();
}

/// Records that a trait object dispatching through vtable `from` becomes one
/// dispatching through vtable `to` when upcast to the trait interned as
/// `trait_id` — the compiled tier's copy of `Interp::dyn_upcasts`, published
/// wherever the interpreter interns a boxing site's supertrait tables.
pub fn upcast_define(from: u32, trait_id: u32, to: u32) {
    shared::rt_shared().upcasts.write().insert((from, trait_id), to);
}

/// Installs (or replaces) vtable `id`'s slots. Called from the interpreter
/// once a compilation unit's function addresses are final
/// (`compile::driver::compile_scc`) and from AOT startup; a slot holding 0 means "not
/// compiled", which the driver answers by asking the interpreter
/// ([`reify_dyn_slot`]).
pub fn vtable_define(id: u32, slots: Vec<(usize, u8)>) {
    let shared = shared::rt_shared();
    let mut t = shared.vtables.write();
    if t.len() <= id as usize {
        t.resize(id as usize + 1, Vec::new());
    }
    t[id as usize] = slots;
}

/// Boxes `value` (a tagged value) as a trait object dispatching through
/// vtable `vtable_id`.
///
/// # Safety
///
/// The words must be valid; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_dyn_new(vtable_id: Word, value: Word) -> Word {
    let vtable_id = vtable_id as u32;
    let value = decode(value);
    encode(active_heap().alloc_dyn(vtable_id, value))
}

/// The vtable id of the trait object `x`, as a raw `i64`.
///
/// # Safety
///
/// , `x` must be a tagged `BoxedObj::Dyn`, and a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_dyn_vtable(x: Word) -> Word {
    match decode(x) {
        Value::Boxed(id) if active_heap().is_dyn(id) => active_heap().dyn_vtable_id(id) as Word,
        _ => fatal("rt_dyn_vtable: not a trait object"),
    }
}

/// The concrete value inside the trait object `x`, tagged.
///
/// # Safety
///
/// Same as [`rt_dyn_vtable`].
#[no_mangle]
pub unsafe extern "C" fn rt_dyn_value(x: Word) -> Word {
    match decode(x) {
        Value::Boxed(id) if active_heap().is_dyn(id) => encode(active_heap().dyn_value(id)),
        _ => fatal("rt_dyn_value: not a trait object"),
    }
}

pub(crate) fn vtable_slot_addr(id: usize, slot: usize) -> (usize, u8) {
    shared::rt_shared()
        .vtables
        .read()
        .get(id)
        .and_then(|s| s.get(slot).copied())
        .unwrap_or((0, typelisp_abi::BODY_ABI_CLASSIC))
}

/// Asks the interpreter for the closure an unfilled vtable slot stands for.
///
/// In an AOT executable every method the program can reach is compiled and no
/// interpreter exists to ask, so a missing hook is an invariant break rather
/// than a limitation — the same reading [`apply_on_this_frame`] makes of a
/// missing apply hook.
pub(crate) unsafe fn reify_dyn_slot(vtable: u32, slot: u32, what: &str) -> Result<Word, typelisp_abi::Raised> {
    match DYN_SLOT_CLOSURE.with(|cell| cell.get()) {
        Some(hook) => {
            let closure = hook(vtable, slot);
            typelisp_abi::raised().map(|()| closure)
        }
        None => fatal(&format!(
            "{}: vtable slot {}/{} is empty and no interpreter is registered on this thread",
            what, vtable, slot
        )),
    }
}

/// How [`reify_dyn_slot`] asks the interpreter for the closure standing
/// behind an unfilled vtable slot — `(vtable id, slot) -> a tagged
/// interpreted closure value`.
pub type DynSlotClosureFn = unsafe extern "C-unwind" fn(u32, u32) -> Word;

typelisp_mem::thread_local! {
    static DYN_SLOT_CLOSURE: Cell<Option<DynSlotClosureFn>> = const { Cell::new(None) };
}

/// Registers this thread's interpreter re-entry for `:dyn` dispatch —
/// `typelisp::eval::Interp` installs it alongside
/// [`set_apply_interpreted`], for the same reason and at the same moment.
pub fn set_dyn_slot_closure(f: Option<DynSlotClosureFn>) {
    DYN_SLOT_CLOSURE.with(|cell| cell.set(f));
}

/// What a function value turns out to be — the question `apply` asks and a
/// direct call never has to.
pub(crate) enum Callee {
    /// A compiled body under the coroutine ABI, with its captures. It can
    /// only be *driven*, so it joins a chain.
    Coroutine { f: crate::coroutine::CoroutineFn, env: Vec<Word> },
    /// A compiled body under the original ABI, with its captures. It runs to
    /// completion by construction, so calling it is enough.
    Classic { f: typelisp_abi::ClassicClosureFn, env: Vec<Word> },
    /// Not compiled. Only an interpreter can run it, and only whoever owns a
    /// continuation stack can let it suspend.
    Interpreted,
}

/// Reads a function value's captures and decides who can run its body.
///
/// The captures come out in the closure box's own mask: a masked slot is a
/// tagged value and an unmasked one a raw word, the same distinction every
/// compiled boundary makes and for the same reason — a machine word does not
/// say which it is.
///
/// # Safety
///
/// `closure` must be a tagged compiled- or interpreted-closure box in the
/// heap registered on this thread.
pub(crate) unsafe fn resolve_closure(closure: Word) -> Callee {
    let id = match decode(closure) {
        Value::Boxed(id) => id,
        other => fatal(&format!("apply: callee is not a function value: {:?}", other)),
    };
    let mut captured = Vec::new();
    let Some((fn_ptr, body_abi, mask)) = active_heap().compiled_closure_parts(id, &mut captured) else {
        return Callee::Interpreted;
    };
    let env: Vec<Word> = captured
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            if mask & (1 << i) != 0 {
                encode(v)
            } else {
                match v {
                    Value::Int(raw) => raw as Word,
                    other => fatal(&format!("apply: unmasked closure slot {} holds a non-raw value {:?}", i, other)),
                }
            }
        })
        .collect();
    if body_abi == typelisp_abi::BODY_ABI_COROUTINE {
        // SAFETY: `build-make-closure` only ever names a function it declared
        // under `coroutine_fn_type`, which is what `rt_coroutine_closure_new`
        // records by being the entry point that was called.
        Callee::Coroutine { f: core::mem::transmute::<usize, crate::coroutine::CoroutineFn>(fn_ptr), env }
    } else {
        // SAFETY: `rt_closure_new` is the only producer of a classic-ABI
        // `BoxedObj::CompiledClosure`, and `build-make-closure` only ever
        // hands it a function compiled under `compiled_fn_type_with_env`.
        Callee::Classic { f: core::mem::transmute::<usize, typelisp_abi::ClassicClosureFn>(fn_ptr), env }
    }
}

/// Upcasts the trait object `value` to the supertrait interned as trait id
/// `trait_id`, returning a fat box over the same concrete value dispatching
/// through that supertrait's own vtable (`Expr::DynUpcast`). Emitted only
/// where the layouts do *not* share a prefix — the leftmost-spine case is a
/// checker-level retype that reaches no code at all.
///
/// A missing entry aborts rather than mis-dispatching: the mapping is
/// registered wherever a box for this concrete type is created
/// (`Expr::DynBox::supers`), and the checker only admits the conversion when
/// the target is in the source's supertrait closure, so reaching one means
/// the box came from somewhere that skipped registration — a compiler bug.
///
/// The source box stays rooted by the caller (a `kind = 2` argument), which
/// is what keeps the concrete value alive across the allocation below.
///
/// # Safety
///
/// , `value` must be a tagged `BoxedObj::Dyn`, and a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_dyn_upcast(trait_id: Word, value: Word) -> Word {
    let trait_id = trait_id as u32;
    let value = decode(value);
    let Value::Boxed(id) = value else {
        fatal("rt_dyn_upcast: not a trait object");
    };
    if !active_heap().is_dyn(id) {
        fatal("rt_dyn_upcast: not a trait object");
    }
    let from = active_heap().dyn_vtable_id(id);
    let to = shared::rt_shared().upcasts.read().get(&(from, trait_id)).copied();
    let Some(to) = to else {
        fatal("rt_dyn_upcast: no supertrait vtable registered for this trait object");
    };
    if to == from {
        return encode(value);
    }
    let inner = active_heap().dyn_value(id);
    encode(active_heap().alloc_dyn(to, inner))
}

/// Interns the types a module that left its compiling process carries
/// (`typelisp_mem::type_words`) and writes each one's `TypeId::word` into the
/// table the module's code reads them from: `args` is `[words, word_count,
/// table]`, `table` having a word for every type the list writes. The
/// module's init function calls it (`compile::type_table`), and whoever loads
/// the module runs that before any of its code.
///
/// # Safety
///
/// the words must be valid, `words` to
/// `word_count` words of a list `TypeWords` wrote (its strings placed as
/// constants), and `table` to as many writable words as the list has types.
#[no_mangle]
pub unsafe extern "C" fn rt_types_define(words: Word, words_len: Word, table: Word) -> Word {
    let words = core::slice::from_raw_parts(words as usize as *const Word, words_len as usize);
    let table = table as usize as *mut Word;
    for (i, id) in typelisp_mem::type_words::define(words).into_iter().enumerate() {
        *table.add(i) = id.word();
    }
    0
}

/// Registers the upcast `(vtable `from`, trait id `trait_id`) -> vtable
/// `to`. Used by AOT startup for the same reason [`rt_vtable_set`] is —
/// the tables have to exist before `main` runs any user code, and there is
/// no interpreter around to fill them; the JIT calls [`upcast_define`]
/// Rust-side instead.
///
/// # Safety
///
/// Each word must be a value of the type the signature gives it.
#[no_mangle]
pub unsafe extern "C" fn rt_upcast_set(from: Word, trait_id: Word, to: Word) -> Word {
    upcast_define(from as u32, trait_id as u32, to as u32);
    0
}

/// Sets slot `slot` of vtable `id` to the native entry point
/// `ptr`, which answers to the ABI in `body_abi`, extending the table as
/// needed. Used by AOT startup, where the pointers are `ptrtoint` constants
/// the linker resolves; the JIT fills tables through [`vtable_define`]
/// instead.
///
/// # Safety
///
/// Each word must be a value of the type the signature gives it.
#[no_mangle]
pub unsafe extern "C" fn rt_vtable_set(id: Word, slot: Word, ptr: Word, body_abi: Word) -> Word {
    let id = id as usize;
    let slot = slot as usize;
    let ptr = ptr as usize;
    let body_abi = body_abi as u8;
    let shared = shared::rt_shared();
    let mut t = shared.vtables.write();
    if t.len() <= id {
        t.resize(id + 1, Vec::new());
    }
    let slots = &mut t[id];
    if slots.len() <= slot {
        slots.resize(slot + 1, (0, typelisp_abi::BODY_ABI_CLASSIC));
    }
    slots[slot] = (ptr, body_abi);
    0
}
