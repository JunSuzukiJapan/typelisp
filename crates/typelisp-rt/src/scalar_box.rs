//! Boxed scalars inside a `Sexpr`: `f64`/`f32` and the narrow integer widths,
//! and the box-kind discriminator over them.

use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal, raise};
use typelisp_mem::fixed::FixedType;
use typelisp_mem::{NarrowInt, TypeId, Value};

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
/// The words must be valid
/// (the `f64`'s `to_bits()` reinterpreted as `i64`); a `Heap` must already
/// be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_f64_new(f: i64) -> i64 {
    let f = f64::from_bits(f as u64);
    encode(active_heap().alloc_f64(f))
}

/// [`rt_f64_new`] for an `f32`.
///
/// The argument is still an `f64` bit pattern: a compiled `f32` lives in an
/// `f64` register already rounded to binary32 (`build-float-binop`), exactly the
/// way a compiled `u8` lives in an `i64` register already cut to 8 bits. The
/// carrier being wider than the type is not the type being lost — the value
/// *is* the number its type names either way. What changes here is the box,
/// which stores a real `f32` so that every later reader can tell.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_f32_new(f: i64) -> i64 {
    let f = f64::from_bits(f as u64);
    encode(active_heap().alloc_f32(f as f32))
}

/// Reads the `f64` bit pattern out of a boxed `Sexpr` float — `x` is a
/// tagged `Sexpr` value (as [`rt_car`] etc. take), unlike `rt_f64_new`'s
/// raw payload. Fatal if it isn't actually a boxed float — the checker is
/// responsible for guaranteeing that never happens, same convention as
/// [`rt_car`] on a non-cons.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_f64_value(x: i64) -> i64 {
    match decode(x) {
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
pub unsafe extern "C" fn rt_f32_value(x: i64) -> i64 {
    match decode(x) {
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
pub unsafe extern "C-unwind" fn rt_sexpr_f64(x: i64) -> i64 {
    match decode(x) {
        Value::Boxed(id) if active_heap().is_f64(id) => active_heap().f64_value(id).to_bits() as i64,
        _ => raise("sexpr-f64: expected an f64 Sexpr node".to_string()).into_word(),
    }
}

/// [`rt_sexpr_f64`] for an `f32`, widened as [`rt_f32_value`] widens it.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_f32(x: i64) -> i64 {
    match decode(x) {
        Value::Boxed(id) if active_heap().is_f32(id) => f64::from(active_heap().f32_value(id)).to_bits() as i64,
        _ => raise("sexpr-f32: expected an f32 Sexpr node".to_string()).into_word(),
    }
}

// ---- fixed-width integers (`i8`/`i16`/`i32`/`u8`/`u16`/`u32`) ----------
//
// The integer half of the float split above, and the same argument: a
// compiled `u8` lives in an `i64` register already cut to 8 bits, so the
// register cannot say which of the six types it is. Inside a `Sexpr` — the
// one place a value has no declared type to be read off — it therefore gets
// a box that says (`BoxedObj::Narrow`).
//
// Three shims per type, named by the type: the box, its typed read, and the
// `sexpr-*` reader. The type is in the name rather than an operand, so a
// program links the shims of the widths it boxes and no others.

/// [`rt_i8_unbox`]'s body: decode, insist on this exact type, hand back the
/// word. Fatal on any other box: a `u8` box handed to a `u16` read would be
/// this shim silently widening a type its caller already knew, the same
/// refusal [`rt_f64_value`] makes about an `f32` box.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread.
unsafe fn narrow_value_of(v: i64, ty: FixedType, who: &str) -> i64 {
    if let Value::Boxed(id) = decode(v) {
        if let Some(n) = active_heap().narrow_box(id) {
            if n.ty() == ty {
                return n.word();
            }
        }
    }
    fatal(&format!("{who}: argument is not a boxed {}", type_name(ty)))
}

/// The `sexpr-*` readers' body: [`narrow_value_of`], except that a node of
/// another type raises the interpreter's panic instead of aborting. Which
/// node a `Sexpr` holds is a run-time fact a program can get wrong; a typed
/// read of a box (`rt_i8_unbox`) has the checker behind it and cannot.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread.
pub(crate) unsafe fn sexpr_narrow_value(v: i64, ty: FixedType) -> i64 {
    if let Value::Boxed(id) = decode(v) {
        if let Some(n) = active_heap().narrow_box(id) {
            if n.ty() == ty {
                return n.word();
            }
        }
    }
    let name = type_name(ty);
    raise(format!("sexpr-{name}: expected a {name} Sexpr node")).into_word()
}

fn type_name(ty: FixedType) -> &'static str {
    match ty {
        FixedType::I8 => "i8",
        FixedType::I16 => "i16",
        FixedType::I32 => "i32",
        FixedType::U8 => "u8",
        FixedType::U16 => "u16",
        FixedType::U32 => "u32",
        FixedType::I64 => "c-long",
        FixedType::U64 => "c-ulong",
    }
}

macro_rules! narrow_shims {
    ($($t:ident: $variant:ident, $boxer:ident, $unboxer:ident;)*) => {$(
        /// Box a raw register word as a `Sexpr` node of this type.
        ///
        /// # Safety
        ///
        /// The words must be valid; a
        /// `Heap` must already be registered on this thread.
        #[no_mangle]
        pub unsafe extern "C" fn $boxer(x: i64) -> i64 {
            encode(active_heap().alloc_narrow(NarrowInt::$variant(x as $t)))
        }

        /// The raw register word inside a box of exactly this type.
        ///
        /// # Safety
        ///
        /// Same as the box shim.
        #[no_mangle]
        pub unsafe extern "C" fn $unboxer(x: i64) -> i64 {
            narrow_value_of(x, FixedType::$variant, stringify!($unboxer))
        }
    )*};
}
narrow_shims! {
    i8: I8, rt_i8_box, rt_i8_unbox;
    i16: I16, rt_i16_box, rt_i16_unbox;
    i32: I32, rt_i32_box, rt_i32_unbox;
    u8: U8, rt_u8_box, rt_u8_unbox;
    u16: U16, rt_u16_box, rt_u16_unbox;
    u32: U32, rt_u32_box, rt_u32_unbox;
}

macro_rules! sexpr_readers {
    ($($reader:ident: $variant:ident;)*) => {$(
        /// `(sexpr-W x)`: the raw word inside a node of this type, raising
        /// the interpreter's panic for a node of any other.
        ///
        /// # Safety
        ///
        /// The words must be valid; a
        /// `Heap` must already be registered on this thread.
        #[no_mangle]
        pub unsafe extern "C-unwind" fn $reader(x: i64) -> i64 {
            sexpr_narrow_value(x, FixedType::$variant)
        }
    )*};
}
sexpr_readers! {
    rt_sexpr_i8: I8;
    rt_sexpr_i16: I16;
    rt_sexpr_i32: I32;
    rt_sexpr_u8: U8;
    rt_sexpr_u16: U16;
    rt_sexpr_u32: U32;
}

/// Discriminates the numeric boxed `Sexpr` kinds that share `TAG_BOXED`'s
/// one tag: `1` for a boxed `f64`, `4` for a boxed `f32`, `2` for a bignum,
/// `3` for a ratio, `5`/`6`/`7`/`8`/`9`/`10` for a boxed `i8`/`i16`/`u8`/`u16`/
/// `u32`/`i32`, `11`/`12` for a `Sexpr` vector/array box, `0` for everything else — *including* non-boxed values, so it's total over
/// every tagged word and `compile-sexpr-tag-test` can call it without a
/// prior tag check (a `kind == 1` result already implies `TAG_BOXED`).
/// `x` is a tagged `Sexpr` value, like [`rt_f64_value`]'s.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_box_kind(x: i64) -> i64 {
    match decode(x) {
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
                // The six integer types get six numbers, not one: this is the
                // test `compile-sexpr-tag-test` uses to decide which `Sexpr`
                // variant a box is, and a `u8` and a `u16` are different
                // variants.
                match n {
                    NarrowInt::I8(_) => 5,
                    NarrowInt::I16(_) => 6,
                    NarrowInt::U8(_) => 7,
                    NarrowInt::U16(_) => 8,
                    NarrowInt::U32(_) => 9,
                    NarrowInt::I32(_) => 10,
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
