//! Compiled closures and binding cells.

use typelisp_mem::prelude::*;
use core::cell::Cell;
use typelisp_abi::{active_heap, decode, encode, fatal};
use typelisp_mem::Value;

// ---- compiled closures & binding cells — the heap-unified function value --
//
// `BoxedObj::CompiledClosure` (`typelisp-mem`) is the GC-heap flip of
// compiled code's raw `malloc`'d, reference-counted `ClosureBox` — see its
// doc comment for the env/`sexpr_mask` slot convention these four expose.
// `BoxedObj::Cell` (the interpreter's own shared binding cell) gets its
// first compiled-code face here too (`rt_cell_*`): a capture that must obey
// the language's shared-cell `setf` semantics is a cell *reference* in the
// closure env, and both worlds mutate the very same heap object.

/// `(rt-closure-new fn-ptr sexpr-mask slot0 slot1 ...)` for compiled code —
/// allocates a boxed compiled closure. `args[0]` is the *raw* native entry
/// point (a `compiled_fn_type_with_env`-ABI function pointer), `args[1]` is
/// the *raw* capture mask (bit `i` set ⇒ `args[2+i]` is a tagged `Sexpr`,
/// stored decoded; clear ⇒ raw native bits, stored verbatim behind an
/// immediate `Value::Int` — see `BoxedObj::CompiledClosure`), and
/// `args[2..argc]` are the captured slots. At most 64 slots — the same
/// limit the replaced `ClosureBox` fn-mask had. Allocation never triggers a
/// collection (the box store grows on demand), so the slot values may be
/// un-rooted at the moment of the call.
///
/// # Safety
///
/// `argc` must be `>= 2`, `args` must point to at least `argc` valid
/// `i64`s, masked slots must hold valid tagged values, and `args[0]` must
/// be a valid function pointer (opaque here — only stored); a `Heap` must
/// already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_closure_new(args: *const i64, argc: u32) -> i64 {
    closure_new(args, argc, typelisp_abi::BODY_ABI_CLASSIC, "rt_closure_new")
}

/// [`rt_closure_new`] for a body compiled under the coroutine ABI (Phase C2).
///
/// A separate entry point rather than an extra argument, because the island
/// that compiles the *next* island is the one already committed: it calls
/// `rt_closure_new` with the layout it was built against, and an argument
/// added in the middle would break the bootstrap at the one moment it cannot
/// be repaired from. The name carries the fact instead.
///
/// # Safety
///
/// As [`rt_closure_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_coroutine_closure_new(args: *const i64, argc: u32) -> i64 {
    closure_new(args, argc, typelisp_abi::BODY_ABI_COROUTINE, "rt_coroutine_closure_new")
}

unsafe fn closure_new(args: *const i64, argc: u32, body_abi: u8, what: &str) -> i64 {
    if argc < 2 {
        fatal(&format!("{}: expected at least 2 arguments (fn ptr, sexpr mask)", what));
    }
    let env_len = argc as usize - 2;
    if env_len > 64 {
        fatal(&format!("{}: more than 64 captured slots", what));
    }
    let fn_ptr = *args as usize;
    let sexpr_mask = *args.add(1) as u64;
    let mut env = Vec::with_capacity(env_len);
    for i in 0..env_len {
        let raw = *args.add(2 + i);
        if sexpr_mask & (1 << i) != 0 {
            env.push(decode(raw));
        } else {
            env.push(Value::Int(raw));
        }
    }
    encode(active_heap().alloc_compiled_closure(fn_ptr, env, sexpr_mask, body_abi))
}

/// `(rt-closure-fnptr clo)` for compiled code — the *raw* native entry
/// point of boxed compiled closure `args[0]` (a tagged `Sexpr`), ready for
/// an indirect call. Fatal if `args[0]` isn't a boxed compiled closure —
/// the checker's type discipline (`Type::Fn`) is what guarantees only
/// function values reach an apply site. Allocates nothing, so triggers no
/// GC.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// decoding to a `Value::Boxed` compiled closure; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_closure_fnptr(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_closure_fnptr: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_compiled_closure(id) => id,
        _ => fatal("rt_closure_fnptr: argument is not a boxed compiled closure"),
    };
    active_heap().compiled_closure_fnptr(id) as i64
}

/// `(rt-closure-env-len clo)` for compiled code — the number of captured
/// slots boxed compiled closure `args[0]` carries, as a *raw* `i64` (the
/// `env_len` argument an indirect `compiled_fn_type_with_env` call passes
/// on). Fatal like [`rt_closure_fnptr`]. Allocates nothing.
///
/// # Safety
///
/// Same contract as [`rt_closure_fnptr`].
#[no_mangle]
pub unsafe extern "C" fn rt_closure_env_len(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_closure_env_len: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_compiled_closure(id) => id,
        _ => fatal("rt_closure_env_len: argument is not a boxed compiled closure"),
    };
    active_heap().compiled_closure_env_len(id) as i64
}

/// `(rt-closure-env-get clo idx)` for compiled code — the `idx`-th captured
/// slot of boxed compiled closure `args[0]`, where `args[1]` is a *raw*
/// index. Hands back the exact `i64` word the slot was created with: a
/// masked slot re-encodes its stored tagged value, an unmasked slot unwraps
/// the raw native bits it smuggled through `Value::Int` (see
/// [`rt_closure_new`]). Fatal if `args[0]` isn't a boxed compiled closure
/// or `idx` is out of range. Allocates nothing.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// the first decoding to a `Value::Boxed` compiled closure; a `Heap` must
/// already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_closure_env_get(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_closure_env_get: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_compiled_closure(id) => id,
        _ => fatal("rt_closure_env_get: first argument is not a boxed compiled closure"),
    };
    let idx = *args.add(1);
    if idx < 0 {
        fatal("rt_closure_env_get: negative slot index");
    }
    let heap = active_heap();
    let v = heap.compiled_closure_env_get(id, idx as usize);
    if heap.compiled_closure_mask(id) & (1 << idx) != 0 {
        encode(v)
    } else {
        match v {
            Value::Int(raw) => raw,
            other => fatal(&format!("rt_closure_env_get: unmasked slot holds a non-raw value {:?}", other)),
        }
    }
}

/// How [`apply_on_this_frame`] re-enters the tree-walking interpreter: the closure
/// as a tagged word, then the very same `(args_ptr, argc)` the compiled
/// callee would have received.
///
/// A function pointer rather than a direct call because this crate depends
/// on `typelisp-mem` alone — the interpreter lives in the `typelisp` crate,
/// which depends on *this* one, so the edge can only run at runtime.
/// `typelisp::eval::Interp` installs it ([`set_apply_interpreted`]) at the
/// same moment it registers the active heap; an AOT-compiled executable has
/// no interpreter to install and leaves it unset.
pub type ApplyInterpretedFn = unsafe extern "C-unwind" fn(closure: i64, args: *const i64, argc: u32) -> i64;

typelisp_mem::thread_local! {
    pub(crate) static APPLY_INTERPRETED: Cell<Option<ApplyInterpretedFn>> = const { Cell::new(None) };
}

/// Installs (or, with `None`, clears) this thread's interpreter re-entry
/// hook — see [`ApplyInterpretedFn`]. Thread-local for exactly the reason
/// [`set_active_heap`] is: one `Interp` per thread, many per process under
/// `cargo test`.
pub fn set_apply_interpreted(f: Option<ApplyInterpretedFn>) {
    APPLY_INTERPRETED.with(|cell| cell.set(f));
}

/// `(rt-cell-new v)` for compiled code — allocates a shared binding cell
/// (`BoxedObj::Cell`) holding tagged `Sexpr` `args[0]`, returning the
/// cell's own tagged reference. Unlike the interpreter's `Heap::alloc_cell`
/// there is no owning-`Rc` liveness registration — the cell lives by
/// ordinary reachability (a root, a closure env slot, ...) like any other
/// box. Allocation never triggers a collection, so `args[0]` may be
/// un-rooted at the moment of the call — but the caller must make the
/// returned cell reachable before anything can trigger one.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// holding a valid tagged value; a `Heap` must already be registered on
/// this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_cell_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_cell_new: expected 1 argument");
    }
    let v = decode(*args);
    encode(active_heap().alloc_cell_unregistered(v))
}

/// `(rt-cell-get c)` for compiled code — the current contents of binding
/// cell `args[0]` (a tagged `Sexpr` reference to a `BoxedObj::Cell`), as a
/// tagged `Sexpr`; per-kind untagging is the caller's job
/// (`compile-sexpr-field` conventions), exactly as for a struct field.
/// Fatal if `args[0]` isn't a boxed cell. Allocates nothing.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// decoding to a `Value::Boxed` cell; a `Heap` must already be registered
/// on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_cell_get(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_cell_get: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) => match active_heap().try_cell_get(id) {
            Some(v) => encode(v),
            None => fatal("rt_cell_get: argument is not a boxed cell"),
        },
        _ => fatal("rt_cell_get: argument is not a boxed cell"),
    }
}

/// `(rt-cell-set! c v)` for compiled code — overwrites binding cell
/// `args[0]`'s contents in place with tagged `Sexpr` `args[1]`; `setf`'s
/// primitive for a shared (captured) binding, visible to every holder of
/// the cell — interpreter bindings included, since both worlds share the
/// very same `BoxedObj::Cell`. Returns the compiled representation of
/// `Unit` (`0`), the same convention [`rt_struct_field_set`] uses. Fatal if
/// `args[0]` isn't a boxed cell. Allocates nothing.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// the first decoding to a `Value::Boxed` cell, the second holding a valid
/// tagged value; a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_cell_set(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_cell_set: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_cell(id) => id,
        _ => fatal("rt_cell_set: first argument is not a boxed cell"),
    };
    let v = decode(*args.add(1));
    active_heap().cell_set(id, v);
    0
}
