//! Boxed scalars inside a `Sexpr`: `f64`/`f32` and the narrow integer widths,
//! and the box-kind discriminator over them.

use typelisp_abi::{active_heap, decode, encode, fatal, raise};
use typelisp_mem::{TypeId, Value};
use crate::wsig;

// ---- Sexpr/RtValue unification, Stage 0: boxed objects (Float) ---------
//
// `Value::Boxed` (`TAG_BOXED`) is the tagged representation for the heap's
// general boxed-object store (`BoxedObj`, in `typelisp-mem`) — the
// mechanism the `Sexpr`/`RtValue` unification plan uses to bring
// struct/closure/`HashTable<K,V>`/`Scope<V>` values into `Sexpr` uniformly,
// instead of leaving them in a separate `Rc`-managed `RtValue` universe.
// `Float` is the first, deliberately trivial occupant (an `f64` doesn't fit
// losslessly alongside a 3-bit tag, so `Sexpr::f64` was never actually
// representable in compiled code before this). Unlike `rt_cons`'s `car`/
// `cdr` (already-tagged `Sexpr` values), `rt_f64_new`'s argument is a raw
// `f64` bit pattern (`f64::to_bits`), not a tagged `Sexpr` — there is no
// existing `Sexpr` value to decode, this constructs a brand new one, the
// same way `compile-construct-sexpr`'s other constructors take their raw
// field payloads (an `Int` literal's bits, a `Char`'s scalar value, ...)
// rather than already-tagged `Sexpr`s.

/// Allocates a boxed `Sexpr` float from a raw `f64` bit pattern.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// (the `f64`'s `to_bits()` reinterpreted as `i64`); a `Heap` must already
/// be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_f64_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_f64_new: expected 1 argument");
    }
    let f = f64::from_bits(*args as u64);
    encode(active_heap().alloc_f64(f))
}

/// [`rt_f64_new`] for an `f32`.
///
/// The argument is still an `f64` bit pattern: a compiled `f32` lives in an
/// `f64` register already rounded to binary32 (`build-fround32`), exactly the
/// way a compiled `u8` lives in an `i64` register already cut to 8 bits. The
/// carrier being wider than the type is not the type being lost — the value
/// *is* the number its type names either way. What changes here is the box,
/// which stores a real `f32` so that every later reader can tell.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_f32_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_f32_new: expected 1 argument");
    }
    let f = f64::from_bits(*args as u64);
    encode(active_heap().alloc_f32(f as f32))
}

/// Reads the `f64` bit pattern out of a boxed `Sexpr` float — `args[0]` is a
/// tagged `Sexpr` value (as [`rt_car`] etc. take), unlike `rt_f64_new`'s
/// raw payload. Fatal if it isn't actually a boxed float — the checker is
/// responsible for guaranteeing that never happens, same convention as
/// [`rt_car`] on a non-cons.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_f64_value(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_f64_value: expected 1 argument");
    }
    match decode(*args) {
        // Positively `is_f64`, not "any box": every other `BoxedObj` kind
        // would make `f64_value` *panic*, and a panic out of this `nounwind`
        // shim aborts the process instead of reporting anything — so the
        // doc'd fatal has to be an explicit test. An `f32` box is refused
        // here too: widening it would be this shim deciding a width its
        // caller already knew.
        Value::Boxed(id) if active_heap().is_f64(id) => active_heap().f64_value(id).to_bits() as i64,
        _ => fatal("rt_f64_value: argument is not a boxed f64"),
    }
}

/// [`rt_f64_value`] for an `f32`, widened into the `f64` bit pattern a
/// compiled `f32` register holds — see [`rt_f32_new`] for why the carrier is
/// wider than the type.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_f32_value(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_f32_value: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) if active_heap().is_f32(id) => {
            f64::from(active_heap().f32_value(id)).to_bits() as i64
        }
        _ => fatal("rt_f32_value: argument is not a boxed f32"),
    }
}

/// `(sexpr-f64 x)`: [`rt_f64_value`] for a program reading a `Sexpr` it
/// does not know the shape of. A node that is not an `f64` raises the
/// interpreter's panic; `rt_f64_value` itself serves typed reads, which the
/// checker guarantees.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_f64(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_f64: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) if active_heap().is_f64(id) => active_heap().f64_value(id).to_bits() as i64,
        _ => raise("sexpr-f64: expected an f64 Sexpr node".to_string()),
    }
}

/// [`rt_sexpr_f64`] for an `f32`, widened as [`rt_f32_value`] widens it.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_f32(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_f32: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) if active_heap().is_f32(id) => f64::from(active_heap().f32_value(id)).to_bits() as i64,
        _ => raise("sexpr-f32: expected an f32 Sexpr node".to_string()),
    }
}

// ---- narrow integers (`i8`/`i16`/`u8`/`u16`/`u32`) ----------------------
//
// The integer half of the float split above, and the same argument: a
// compiled `u8` lives in an `i64` register already cut to 8 bits, so the
// register cannot say which of the five narrow types it is. Inside a
// `Sexpr` — the one place a value has no declared type to be read off — it
// therefore gets a box that says (`BoxedObj::Narrow`). `i32` keeps its bare
// tagged word: that width is what an untagged `Sexpr` integer already means.

/// Allocates a boxed narrow-integer `Sexpr` from a raw register word plus
/// the `wsig` code naming its type — `args[0]` is the value (already
/// normalized by the arithmetic that produced it, and normalized again by
/// `Heap::alloc_narrow` regardless), `args[1]` is `width * 2 + signed`.
///
/// The `wsig` is a *constant* the island emits from the declared type, not
/// something read off the value: the word is the same bit pattern for a `u8`
/// `200` and an `i32` `200`, which is precisely why the box is needed.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_narrow_new(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_narrow_new: expected 2 arguments");
    }
    let (width, signed) = wsig(*args.add(1));
    encode(active_heap().alloc_narrow(width as u8, signed, *args))
}

/// Reads the raw register word out of a boxed narrow-integer `Sexpr` of
/// exactly the type `args[1]`'s `wsig` names — `args[0]` is a tagged `Sexpr`
/// value, as [`rt_f64_value`]'s is.
///
/// Fatal on any other box, the `wsig` included: a `u8` box handed to a
/// `u16` read would be this shim silently widening a type its caller already
/// knew, the same refusal [`rt_f64_value`] makes about an `f32` box.
///
/// # Safety
///
/// Same as [`rt_narrow_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_narrow_value(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_narrow_value: expected 2 arguments");
    }
    let (width, signed) = wsig(*args.add(1));
    narrow_value_of(*args, width as u8, signed, "rt_narrow_value")
}

/// [`rt_narrow_value`]'s body: decode, insist on this exact type, hand back
/// the word. The `sexpr-*` readers use [`sexpr_narrow_value`] instead.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread.
unsafe fn narrow_value_of(v: i64, width: u8, signed: bool, who: &str) -> i64 {
    match decode(v) {
        Value::Boxed(id) => match active_heap().narrow_box(id) {
            Some(n) if n.width == width && n.signed == signed => n.value,
            _ => fatal(&format!("{who}: argument is not a boxed {}{width}", if signed { "i" } else { "u" })),
        },
        _ => fatal(&format!("{who}: argument is not a boxed {}{width}", if signed { "i" } else { "u" })),
    }
}

/// The six fixed-width `sexpr-*` readers' body: [`narrow_value_of`], except
/// that a node of another type raises the interpreter's panic instead of
/// aborting. Which node a `Sexpr` holds is a run-time fact a program can get
/// wrong; a typed read of a narrow box ([`rt_narrow_value`]) has the checker
/// behind it and cannot.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread.
pub(crate) unsafe fn sexpr_narrow_value(v: i64, width: u8, signed: bool) -> i64 {
    if let Value::Boxed(id) = decode(v) {
        if let Some(n) = active_heap().narrow_box(id) {
            if n.width == width && n.signed == signed {
                return n.value;
            }
        }
    }
    let ty = format!("{}{width}", if signed { "i" } else { "u" });
    raise(format!("sexpr-{ty}: expected a {ty} Sexpr node"))
}

/// `(sexpr-i8 x)` and its four siblings: the typed payload extractors for
/// the narrow-integer `Sexpr` variants, alongside `sexpr-i32` (`cons`) and `sexpr-f64`/
/// `sexpr-f32`. One shim per type rather than one taking a `wsig`,
/// because a builtin accessor is called with exactly its own arguments and
/// the type is in the *name* — `compile-call`'s rename table
/// (`compile::externs`) maps each name to its own symbol.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_i8(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_i8: expected 1 argument");
    }
    sexpr_narrow_value(*args, 8, true)
}

/// [`rt_sexpr_i8`] for `i16`.
///
/// # Safety
///
/// Same as [`rt_sexpr_i8`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_i16(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_i16: expected 1 argument");
    }
    sexpr_narrow_value(*args, 16, true)
}

/// [`rt_sexpr_i8`] for `u8`.
///
/// # Safety
///
/// Same as [`rt_sexpr_i8`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_u8(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_u8: expected 1 argument");
    }
    sexpr_narrow_value(*args, 8, false)
}

/// [`rt_sexpr_i8`] for `u16`.
///
/// # Safety
///
/// Same as [`rt_sexpr_i8`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_u16(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_u16: expected 1 argument");
    }
    sexpr_narrow_value(*args, 16, false)
}

/// [`rt_sexpr_i8`] for `u32`.
///
/// # Safety
///
/// Same as [`rt_sexpr_i8`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_u32(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_u32: expected 1 argument");
    }
    sexpr_narrow_value(*args, 32, false)
}

/// Discriminates the numeric boxed `Sexpr` kinds that share `TAG_BOXED`'s
/// one tag: `1` for a boxed `f64`, `4` for a boxed `f32`, `2` for a bignum,
/// `3` for a ratio, `5`/`6`/`7`/`8`/`9`/`10` for a boxed `i8`/`i16`/`u8`/`u16`/
/// `u32`/`i32`, `11`/`12` for a `Sexpr` vector/array box, `0` for everything else — *including* non-boxed values, so it's total over
/// every tagged word and `compile-sexpr-tag-test` can call it without a
/// prior tag check (a `kind == 1` result already implies `TAG_BOXED`).
/// `args[0]` is a tagged `Sexpr` value, like [`rt_f64_value`]'s.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_box_kind(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_box_kind: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) => {
            let heap = active_heap();
            if heap.is_f64(id) {
                1
            } else if heap.is_f32(id) {
                4
            } else if heap.is_bignum(id) {
                2
            } else if heap.is_ratio(id) {
                3
            } else if let Some(n) = heap.narrow_box(id) {
                // The five narrow integer types get five numbers, not one:
                // this is the test `compile-sexpr-tag-test` uses to decide
                // which `Sexpr` variant a box is, and a `u8` and a `u16` are
                // different variants.
                match (n.width, n.signed) {
                    (8, true) => 5,
                    (16, true) => 6,
                    (8, false) => 7,
                    (16, false) => 8,
                    (32, false) => 9,
                    (32, true) => 10,
                    (w, s) => fatal(&format!("rt_box_kind: a narrow box of an unknown width {}{}", if s { "i" } else { "u" }, w)),
                }
            } else if heap.is_struct(id) && heap.struct_type_key(id) == TypeId::SEXPR_VECTOR {
                // `#(..)`'s box and `#nA(..)`'s: the `Sexpr` `vector`/`array`
                // variants. Told apart by identity, since a struct box of any
                // other type is not a `Sexpr` variant at all.
                11
            } else if heap.is_struct(id) && heap.struct_type_key(id) == TypeId::SEXPR_ARRAY {
                12
            } else if heap.sexpr_tuple_arity(id).is_some() {
                // `#{..}`'s box, of whatever arity: the `tuple` variant.
                13
            } else {
                0
            }
        }
        _ => 0,
    }
}
