//! Boxed structs (`defstruct`, `Vector<T>`): construction and field access.

use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal, raise, Raised};
use typelisp_mem::{BoxId, TypeId, Value};

// ---- boxed objects (Struct) ------------------------------------------------
//
// `BoxedObj::Struct` (`typelisp-mem`) is the shared runtime shape behind a
// `defstruct` instance, `Vector<T>`, and `cons-cell<K,V>` alike — see its
// doc comment.

/// The type a compiled call site handed in: the word the module's type table
/// holds for it (`TypeId::word`), which the module's code reads from the
/// table and passes on (`compile::type_table`).
///
/// # Safety
///
/// A `Heap` need not be registered; `raw` is only read as a word.
pub(crate) unsafe fn type_arg(raw: i64, who: &str) -> TypeId {
    TypeId::from_word(raw).unwrap_or_else(|| fatal(&format!("{}: type argument {} is not a type", who, raw)))
}

/// `(rt-struct-new type-name field0 field1 ...)` for compiled code —
/// allocates a boxed `Sexpr` struct. `args[0]` is the struct's type (its
/// `TypeId::word`);
/// `args[1..argc]` are the
/// field values, already-tagged `Sexpr`s copied into the new struct's field
/// vector unchanged (this function doesn't interpret them, same as
/// [`rt_cons`] doesn't interpret its `car`/`cdr`).
///
/// # Safety
///
/// `argc` must be `>= 1`, `args` must point to at least `argc` valid `i64`s,
/// and `args[0]` must be a type's word; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_struct_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_struct_new: expected at least 1 argument (the type)");
    }
    let type_key = type_arg(*args, "rt_struct_new");
    let mut fields = Vec::with_capacity(argc as usize - 1);
    for i in 1..argc as isize {
        fields.push(decode(*args.offset(i)));
    }
    encode(active_heap().alloc_struct(type_key, fields))
}

/// The index bound both `rt_struct_field_*` functions check before touching
/// the heap, [`raise`]ing `vector_get`/`vector_set`'s own message when `idx`
/// is outside it.
///
/// The check is here rather than left to the mem layer on purpose: the mem
/// layer's own overrun is a Rust `panic!`, which would cross these
/// functions' FFI boundary as an abort. Catching that panic afterwards is
/// not an option either — the point is to *not* fail, so the bound is tested
/// before the access.
///
/// Both call sites that can actually reach the bound are `Vector<T>`'s
/// `get`/`set` (`compile-vector-op`), whose index is a run-time value; a
/// `defstruct` field access (`compile-field-get`/`compile-field-set`) has a
/// checker-fixed index and never fails here.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread and `id` must name a
/// live boxed struct in it.
unsafe fn checked_field_index(id: BoxId, idx: i64) -> Result<usize, Raised> {
    if idx < 0 || idx as usize >= active_heap().struct_field_count(id) {
        return Err(raise(format!("Vector: index {} out of bounds", idx)));
    }
    Ok(idx as usize)
}

/// `(rt-struct-field-get s idx)` for compiled code — the `idx`-th field of
/// boxed struct `id` (a tagged `Sexpr`), where `index` is a *raw*
/// (untagged) `i64` index, matching [`rt_str_ref`]'s convention for its own
/// raw index argument. Fatal if `id` isn't a boxed struct (a compiler
/// contract violation); an out-of-range `idx` is an ordinary recoverable
/// failure instead — see [`checked_field_index`].
///
/// # Safety
///
/// The words must be valid,
/// the first decoding to a `Value::Boxed` struct; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_struct_field_get(id: i64, index: i64) -> i64 {
    let id = match decode(id) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_field_get: first argument is not a boxed Sexpr"),
    };
    match checked_field_index(id, index) {
        Ok(idx) => encode(active_heap().struct_field(id, idx)),
        Err(raised) => raised.into_word(),
    }
}

/// `(rt-struct-field-set! s idx val)` for compiled code — overwrites the
/// `idx`-th field of boxed struct `id` in place with `val` (a
/// tagged `Sexpr`); `idx` is a raw index, like [`rt_struct_field_get`],
/// and out-of-range the same recoverable way ([`checked_field_index`]).
/// Returns the compiled representation of `Unit` (`0`), the same convention
/// [`rt_set_car`]/[`rt_set_cdr`] use for their own in-place mutation.
///
/// # Safety
///
/// The words must be valid,
/// the first decoding to a `Value::Boxed` struct; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_struct_field_set(id: i64, idx: i64, val: i64) -> i64 {
    let id = match decode(id) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_field_set: first argument is not a boxed Sexpr"),
    };
    let idx = match checked_field_index(id, idx) {
        Ok(idx) => idx,
        Err(raised) => return raised.into_word(),
    };
    let val = decode(val);
    active_heap().struct_set_field(id, idx, val);
    0
}

/// `(rt-struct-field-count s)` for compiled code — the number of fields boxed
/// struct `id` (a tagged `Sexpr`) holds, returned as a *raw* (untagged)
/// `i64` (like `rt_str_length`'s own raw count result, and the raw index
/// [`rt_struct_field_get`] consumes). `Vector<T>::len`'s primitive: a growable
/// boxed struct's field count is its element count. Fatal if `id` isn't a
/// boxed struct. Allocates nothing, so triggers no GC.
///
/// # Safety
///
/// The words must be valid
/// decoding to a `Value::Boxed` struct; a `Heap` must already be registered
/// on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_struct_field_count(id: i64) -> i64 {
    let id = match decode(id) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_field_count: first argument is not a boxed Sexpr"),
    };
    active_heap().struct_field_count(id) as i64
}

/// `(rt-struct-push-field! s val)` for compiled code — appends `val` (a
/// tagged `Sexpr`) as a new field of boxed struct `id`, growing its field
/// count by one. `Vector<T>::push`'s primitive. Returns the compiled
/// representation of `Unit` (`0`), the same convention [`rt_struct_field_set`]
/// uses for its own in-place mutation. Fatal if `id` isn't a boxed
/// struct. Only pushes an already-decoded `Value` onto the struct's own field
/// `Vec` (a Rust `Vec::push`, not a cons-heap allocation), so triggers no GC —
/// the caller need not keep `val` rooted across this call.
///
/// # Safety
///
/// The words must be valid, the
/// first decoding to a `Value::Boxed` struct; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_struct_push_field(id: i64, val: i64) -> i64 {
    let id = match decode(id) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_push_field: first argument is not a boxed Sexpr"),
    };
    let val = decode(val);
    active_heap().struct_push_field(id, val);
    0
}

/// `(rt-struct-pop-field! s)` for compiled code — removes and returns the
/// last field of boxed struct `id` (a tagged `Sexpr`), shrinking its
/// field count by one. `Vector<T>::pop`'s primitive, the inverse of
/// [`rt_struct_push_field`]. Only ever called after the compiled caller's
/// own `rt_struct_field_count` check confirmed at least one field present
/// (`compiler.rs`'s `compile-vector-op` "pop" branch) — mirrors
/// the "caller already checked the count" precondition, since `pop`,
/// returns `Option<T>` (`None` on empty is a legitimate outcome the checked
/// branch builds directly, never by calling this). Fatal if `id` isn't
/// a boxed struct, or if it turns out empty anyway (an internal-invariant
/// trap, the same convention every other `rt_*` bounds violation follows).
///
/// **Deliberately still `extern "C"` and `fatal`**, unlike its
/// [`rt_struct_field_get`]/[`rt_struct_field_set`] neighbours: `pop` on an
/// empty vector is not a user-reachable failure at all. The one caller
/// branches on `rt_struct_field_count` first and builds `None` itself, so
/// reaching the `fatal` below means the compiled caller stopped honouring
/// that contract — runtime corruption, which is exactly what [`fatal`] is
/// for.
///
/// # Safety
///
/// The words must be valid
/// decoding to a `Value::Boxed` struct; a `Heap` must already be registered
/// on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_struct_pop_field(id: i64) -> i64 {
    let id = match decode(id) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_pop_field: first argument is not a boxed Sexpr"),
    };
    match active_heap().struct_pop_field(id) {
        Some(v) => encode(v),
        None => fatal("rt_struct_pop_field: vector is empty (caller must check rt_struct_field_count first)"),
    }
}
