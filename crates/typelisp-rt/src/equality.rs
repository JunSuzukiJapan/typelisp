//! CL's four equality predicates as *values*: one implementation, called from
//! both sides of the compile boundary.
//!
//! Same arrangement (and same reason) as [`crate::stream_builtin`]. These are
//! not one instruction each: `equal`/`equalp` recurse through cons cells,
//! structs and enums, `equalp` folds ASCII case and compares numbers across
//! type, and `eql` has to see through the boxes `Float`/`bignum`/`ratio` live
//! in. A second implementation on the compiled side would be four chances for
//! `(equal x y)` to answer differently depending on whether the function
//! asking happened to be compiled.
//!
//! What is *not* here is the three predicates that genuinely are one
//! instruction on their operands — `eq` on a `Sexpr`/`symbol` handle,
//! `=`/`eq` on an integer, `eq` on a `string`'s `StrId`. Those the island
//! lowers in place (`compiler.rs`'s `compile-assoc`), because at the compiled
//! level they are a single `icmp eq` over the very same word the interpreter
//! compares.
//!
//! Its own file so an executable that never compares two `Sexpr`s structurally
//! doesn't link the recursion, `BigRational`, or the boxed-object accessors it
//! reaches through.

use num_bigint::BigInt;
use num_rational::BigRational;
use typelisp_mem::{FloatBox, Heap, Value};

/// The concrete value inside a trait object, or `v` unchanged. Comparison
/// (like printing) sees straight through a `BoxedObj::Dyn`: the box is a
/// dispatch mechanism, and — since it is usually created by an *implicit*
/// coercion at a `:dyn` parameter — letting it change the answer of `eq`/
/// `equal` would make an invisible conversion observable. Applied at the
/// entry of [`eql_val`]/[`equal_val`]/[`equalp_val`], so it covers nested
/// positions through their recursion too.
pub fn strip_dyn(heap: &Heap, v: Value) -> Value {
    match v {
        Value::Boxed(id) if heap.is_dyn(id) => heap.dyn_value(id),
        other => other,
    }
}

/// CL's `eq` on a `Sexpr`: the underlying `Value` compared directly — cons
/// identity, scalar/symbol value equality (`registry::sexpr_assoc`).
pub fn eq_val(heap: &Heap, a: Value, b: Value) -> bool {
    strip_dyn(heap, a) == strip_dyn(heap, b)
}

/// CL's `eql`: `eq` plus "two numbers of the same type and value are
/// equivalent even when they aren't the same object" — the one case that can
/// actually diverge from `eq`'s plain `Value` equality now that `Float`/
/// `bignum`/`ratio` are heap-boxed. `eq`'s `==` compares two boxes by `BoxId`
/// identity (correctly not `eq` for separately-allocated equal floats, the
/// same way two separately built `Str`s aren't `eq`), but they must still be
/// `eql`.
///
/// Every other variant is either immediate (`Int`/`Char`/`Bool`/`Sym`,
/// already value-equal under `eq`) or `eq`-as-identity by design
/// (`Cons`/`Str`) — CL's own `eql` agrees `eq` is already correct for those.
/// The other boxed kinds (structs, hash tables, binding cells, closures) are
/// aggregate/identity objects for which CL's `eql` is `eq` anyway, so they
/// fall through to the identity comparison at the end.
pub fn eql_val(heap: &Heap, a: Value, b: Value) -> bool {
    let (a, b) = (strip_dyn(heap, a), strip_dyn(heap, b));
    if let (Value::Boxed(ia), Value::Boxed(ib)) = (a, b) {
        // Two floats are `eql` when they are the *same width* and the same
        // number, which is CL: `(eql 1.0f0 1.0d0)` is false there because the
        // two are different types. Matching the pair rather than comparing
        // through a common width is what keeps that true here.
        if let (Some(fa), Some(fb)) = (heap.float_box(ia), heap.float_box(ib)) {
            return match (fa, fb) {
                (FloatBox::F32(x), FloatBox::F32(y)) => x == y,
                (FloatBox::F64(x), FloatBox::F64(y)) => x == y,
                _ => false,
            };
        }
        // The narrow integers are the floats' case exactly: `eql` is
        // type-sensitive, so a `u8` `5` and a `u16` `5` are two different
        // numbers of two different types and not `eql` — while two
        // separately boxed `u8` `5`s are. Comparing the whole `NarrowInt`
        // (width, signedness and value) says both at once.
        if let (Some(na), Some(nb)) = (heap.narrow_box(ia), heap.narrow_box(ib)) {
            return na == nb;
        }
        // Same rationale as the floats above: two separately-allocated but
        // equal-valued `bignum`/`ratio` boxes must still be `eql`.
        if heap.is_bignum(ia) && heap.is_bignum(ib) {
            return heap.bignum_value(ia) == heap.bignum_value(ib);
        }
        if heap.is_ratio(ia) && heap.is_ratio(ib) {
            return heap.ratio_value(ia) == heap.ratio_value(ib);
        }
    }
    a == b
}

/// CL's `equal`: [`eql_val`] on every atom but `Cons` (structural recursion)
/// and `Str` (case-sensitive content).
pub fn equal_val(heap: &Heap, a: Value, b: Value) -> bool {
    let (a, b) = (strip_dyn(heap, a), strip_dyn(heap, b));
    match (a, b) {
        (Value::Cons(_), Value::Cons(_)) => {
            let (Ok(ca), Ok(cb)) = (heap.car(a), heap.car(b)) else { return false };
            let (Ok(da), Ok(db)) = (heap.cdr(a), heap.cdr(b)) else { return false };
            equal_val(heap, ca, cb) && equal_val(heap, da, db)
        }
        (Value::Str(i), Value::Str(j)) => heap.string(i) == heap.string(j),
        (a, b) => eql_val(heap, a, b),
    }
}

/// Any of the numeric shapes (`Int`, a boxed `f32`/`f64`/`bignum`/`ratio`) as
/// one exact rational, so [`equalp_val`] can compare across type the way CL's
/// `equalp` requires. `None` for anything that isn't a number.
///
/// Unlike [`eql_val`], this deliberately *crosses* the float widths: `equalp`
/// on numbers is CL's `=`, which is about the numbers rather than the types.
/// An `f32` widens to `f64` exactly, so reading the width out and widening is
/// lossless — but the width is still read rather than assumed.
fn numeric_as_ratio(heap: &Heap, v: Value) -> Option<BigRational> {
    match v {
        Value::Int(n) => Some(BigRational::from_integer(BigInt::from(n))),
        Value::Boxed(id) => match heap.float_box(id) {
            Some(FloatBox::F32(f)) => BigRational::from_float(f64::from(f)),
            Some(FloatBox::F64(f)) => BigRational::from_float(f),
            // Crosses the integer widths for the same reason it crosses the
            // float ones: `equalp` on numbers is CL's `=`. The width is
            // still read out rather than assumed — the box is asked, and a
            // narrow integer is already the number its type names.
            None if heap.narrow_box(id).is_some() => {
                Some(BigRational::from_integer(BigInt::from(heap.narrow_box(id).expect("just tested").value)))
            }
            None if heap.is_bignum(id) => Some(BigRational::from_integer(heap.bignum_value(id).clone())),
            None if heap.is_ratio(id) => Some(heap.ratio_value(id).clone()),
            None => None,
        },
        _ => None,
    }
}

/// CL's `equalp`: like [`equal_val`] but `Str`/`Char` compare
/// case-insensitively, numbers compare across type via [`numeric_as_ratio`]
/// (CL defines two numbers as `equalp` by `=`, regardless of type, unlike
/// `eql`/`equal`'s same-type requirement), and structs/enums recurse
/// element-wise when their type keys and shapes match.
pub fn equalp_val(heap: &Heap, a: Value, b: Value) -> bool {
    let (a, b) = (strip_dyn(heap, a), strip_dyn(heap, b));
    if let (Some(x), Some(y)) = (numeric_as_ratio(heap, a), numeric_as_ratio(heap, b)) {
        return x == y;
    }
    match (a, b) {
        (Value::Cons(_), Value::Cons(_)) => {
            let (Ok(ca), Ok(cb)) = (heap.car(a), heap.car(b)) else { return false };
            let (Ok(da), Ok(db)) = (heap.cdr(a), heap.cdr(b)) else { return false };
            equalp_val(heap, ca, cb) && equalp_val(heap, da, db)
        }
        (Value::Str(i), Value::Str(j)) => heap.string(i).eq_ignore_ascii_case(heap.string(j)),
        (Value::Char(c), Value::Char(d)) => c.eq_ignore_ascii_case(&d),
        // CL's `equalp` on a structure: same type, and every slot `equalp`
        // (unlike `equal`, which is `eq` on structures — `eql_val`'s fallback
        // `a == b`, a pointer-identity `BoxId` compare, is exactly that, so
        // this recursive arm must come *before* it or it would never run).
        (Value::Boxed(ia), Value::Boxed(ib)) if heap.is_struct(ia) && heap.is_struct(ib) => {
            heap.struct_type_key(ia) == heap.struct_type_key(ib)
                && heap.struct_field_count(ia) == heap.struct_field_count(ib)
                && (0..heap.struct_field_count(ia))
                    .all(|i| equalp_val(heap, heap.struct_field(ia, i), heap.struct_field(ib, i)))
        }
        (Value::Boxed(ia), Value::Boxed(ib)) if heap.is_enum(ia) && heap.is_enum(ib) => {
            heap.enum_type_key(ia) == heap.enum_type_key(ib)
                && heap.enum_variant(ia) == heap.enum_variant(ib)
                && heap.enum_field_count(ia) == heap.enum_field_count(ib)
                && (0..heap.enum_field_count(ia))
                    .all(|i| equalp_val(heap, heap.enum_field(ia, i), heap.enum_field(ib, i)))
        }
        (a, b) => eql_val(heap, a, b),
    }
}

// ---- The compiled-code edge -------------------------------------------
//
// Beside the implementation, for the reason `sys_builtin`'s own edge spells
// out. As measured there, this module does *not* currently win that game —
// it is small enough that rustc merges it into a codegen unit every
// executable already pulls, costing ~28 KB. Left as is: the shims still
// belong next to what they call, and the fix if that ever matters is the same
// one the printer got, a crate of its own.

use crate::{active_heap, decode, fatal};
/// The three structural equality predicates for compiled code
/// ([`crate::equality`]). Each takes two tagged `Sexpr` words and returns a
/// raw `0`/`1` — a compiled `bool` is the bare machine word, not a tagged
/// value, so there is no `encode` on the way out.
///
/// `eq` has no shim on purpose: it is `icmp eq` on the two words, which the
/// island emits in place.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to 2 valid tagged `i64`s; a
/// `Heap` must be registered on this thread. Allocates nothing, so the result
/// needs no GC-root protection.
#[no_mangle]
pub unsafe extern "C" fn rt_sexpr_eql(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_sexpr_eql: expected 2 arguments");
    }
    i64::from(eql_val(active_heap(), decode(*args), decode(*args.offset(1))))
}

/// `(equal a b)` for compiled code — see [`rt_sexpr_eql`].
///
/// # Safety
///
/// Same as [`rt_sexpr_eql`].
#[no_mangle]
pub unsafe extern "C" fn rt_sexpr_equal(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_sexpr_equal: expected 2 arguments");
    }
    i64::from(equal_val(active_heap(), decode(*args), decode(*args.offset(1))))
}

/// `(equalp a b)` for compiled code — see [`rt_sexpr_eql`].
///
/// # Safety
///
/// Same as [`rt_sexpr_eql`].
#[no_mangle]
pub unsafe extern "C" fn rt_sexpr_equalp(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_sexpr_equalp: expected 2 arguments");
    }
    i64::from(equalp_val(active_heap(), decode(*args), decode(*args.offset(1))))
}
