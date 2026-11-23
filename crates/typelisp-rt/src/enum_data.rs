//! Enum values (`Option`/`Result`/`defenum`) and the `Sexpr` downcast test.

use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal};
use typelisp_mem::Value;
use crate::type_arg;

// ---- enum values (Option/Result/defenum) — the heap-unified shape ------
//
// `BoxedObj::Enum` (`typelisp-mem`) is the shared runtime shape behind
// every enum value: `Option<T>`/`Result<T,E>`/a user `defenum` instance —
// see its doc comment for what it replaces (the interpreter's Rust-side
// `RtValue::Data` and compiled code's raw leaked `malloc` box). These three
// mirror `rt_struct_new`/`rt_struct_field_get`'s shapes; a separate family
// rather than `rt_struct_*` reuse so no caller ever has to know about a
// variant-tag slot offset, and so `rt_struct_field_get`'s "is a Struct"
// invariant stays intact.

/// `(rt-data-new type-name variant field0 field1 ...)` for compiled code —
/// allocates a boxed enum value. `args[0]` is the enum's type (its
/// `TypeId::word`), `args[1]` is the
/// *raw* (untagged) variant index, `args[2..argc]` are the variant's field
/// values, already-tagged `Sexpr`s stored unchanged.
///
/// # Safety
///
/// `argc` must be `>= 2`, `args` must point to at least `argc` valid
/// `i64`s, and `args[0]` must be a type's word; a `Heap` must
/// already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_data_new(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_data_new: expected at least 2 arguments (type, variant)");
    }
    let type_key = type_arg(*args, "rt_data_new");
    let variant = *args.add(1);
    if variant < 0 {
        fatal("rt_data_new: negative variant index");
    }
    let mut fields = Vec::with_capacity(argc as usize - 2);
    for i in 2..argc as isize {
        fields.push(decode(*args.offset(i)));
    }
    encode(active_heap().alloc_enum(type_key, variant as usize, fields))
}

/// `(rt-data-variant v)` for compiled code — the variant index of boxed
/// enum value `args[0]` (a tagged `Sexpr`), returned as a *raw* (untagged)
/// `i64`, ready for a `match` arm's tag comparison. Fatal if `args[0]`
/// isn't a boxed enum — the checker's type discipline is what guarantees
/// only enum-typed scrutinees reach a tag test. Allocates nothing, so
/// triggers no GC.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// decoding to a `Value::Boxed` enum; a `Heap` must already be registered
/// on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_data_variant(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_data_variant: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_enum(id) => id,
        _ => fatal("rt_data_variant: argument is not a boxed enum value"),
    };
    active_heap().enum_variant(id) as i64
}

/// `(rt-data-field v idx)` for compiled code — the `idx`-th field of boxed
/// enum value `args[0]` (a tagged `Sexpr`), where `args[1]` is a *raw*
/// (untagged) index like [`rt_struct_field_get`]'s. The result is the
/// stored tagged `Sexpr`; per-kind untagging is the caller's job
/// (`compile-sexpr-field`), exactly as for a struct field. Fatal if
/// `args[0]` isn't a boxed enum or `idx` is out of range. No `set`
/// counterpart exists: enum values are immutable.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// the first decoding to a `Value::Boxed` enum; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_data_field(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_data_field: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_enum(id) => id,
        _ => fatal("rt_data_field: first argument is not a boxed enum value"),
    };
    let idx = *args.add(1);
    if idx < 0 {
        fatal("rt_data_field: negative field index");
    }
    encode(active_heap().enum_field(id, idx as usize))
}

/// `(rt-sexpr-instance-test v type-name variant)` for compiled code's
/// Sexpr-downcast pattern guard (`(point x y)`/`(color::red)`/`(the T p)`
/// against a `Sexpr` scrutinee — `compiler.rs`'s `compile-sexpr-instance-
/// test`, `core_bridge::translate_pattern`'s `downcast`/`pat-typetest`
/// encoding). Tests whether tagged `Sexpr` value `args[0]` is a boxed
/// struct or enum whose own type is `args[1]`, and —
/// for an enum — whose variant also matches the *raw* `args[2]` (`-1` skips
/// the variant check, for a struct downcast or a `(the T p)` whole-enum
/// bind, where any variant of `T` matches).
///
/// Unlike every other `rt_*` shim here, a mismatch is never fatal: a
/// heterogeneous `Sexpr`'s runtime shape is only discoverable here — that's
/// the entire reason this function exists, rather than the checker ruling
/// out the mismatch the way it does for every ordinary (non-downcast)
/// pattern. Returns a raw (untagged) `i64` boolean, matching
/// [`rt_str_lt`]/every other `rt_*` predicate's convention (never a tagged
/// `Sexpr::Bool`) — `compile-sexpr-instance-test` wraps it in its own
/// `build-icmp-eq` against `1`, exactly like [`rt_box_kind`]'s callers do.
///
/// # Safety
///
/// `argc` must be `>= 3`, `args` must point to at least 3 valid `i64`s, and
/// `args[1]` must be a type's word; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_sexpr_instance_test(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_sexpr_instance_test: expected 3 arguments");
    }
    let v = decode(*args);
    let type_key = type_arg(*args.add(1), "rt_sexpr_instance_test");
    let id = match v {
        Value::Boxed(id) => id,
        _ => return 0,
    };
    let variant = *args.add(2);
    let heap = active_heap();
    let matches = if heap.is_struct(id) {
        heap.struct_type_key(id) == type_key
    } else if heap.is_enum(id) {
        heap.enum_type_key(id) == type_key && (variant < 0 || heap.enum_variant(id) as i64 == variant)
    } else {
        false
    };
    matches as i64
}
