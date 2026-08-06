//! A built-in used as a *function value* is a heap box.
//!
//! The interpreter used to hold these in Rust-side `RtValue::Builtin(String)`/
//! `BuiltinMethod(Path, String)` variants, which had no `mem::Value` form at
//! all. That gap is what kept `Type::Fn` off the list of types an enum field
//! can store (`Checker::enum_field_storable` and its `Interp` twin): a
//! `Type::Fn` field is fine for a *closure*, whose value is already a
//! `BoxedObj::CompiledClosure`, but `(op::binary +)` had nothing to put in the
//! slot — so any enum mentioning `Fn` fell back to the heap-invisible
//! `RtValue::Data` for every one of its instantiations.
//!
//! `BoxedObj::Builtin` closes it, and these tests pin both halves: that a
//! built-in still behaves as a function value at all (it now travels as a box
//! rather than a Rust string), and that it can be *stored* — put into an enum
//! field, carried around, matched back out, and applied. The storage half is
//! what was impossible before.
//!
//! The enum here is a `defenum` rather than an `Option<...>`: a generic
//! argument is parsed out of a *symbol* (`Option<i32>` is one token), so a
//! function type — which is a list, `(fn (i32) i32)` — cannot be written as
//! one. A `defenum` variant's field is an ordinary type expression, so it can.

use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, RtValue};

fn env() -> (Heap, Checker, Interp) {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    (h, chk, interp)
}

fn eval_in(h: &mut Heap, chk: &mut Checker, interp: &mut Interp, src: &str) -> Result<RtValue, EvalError> {
    let r = Reader::new();
    let vs = r.read_all(h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(h, interp, v).expect("check failed");
        if let Some(val) = interp.exec(h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, src).expect("eval failed")
}

/// A `defenum` with one function-typed field per variant, plus a driver that
/// matches it back out and applies it. Both the built-in *method* case
/// (`recv_type: Some`) and the free-built-in case (`recv_type: None`) have a
/// variant here.
const OP_ENUM: &str = "(defenum op (binary (fn (i32 i32) i32)) (nullary (fn () Symbol)) (none)) \
                       (defun run-binary ((o op) (a i32) (b i32)) i32 \
                         (match o ((binary f) (f a b)) ((nullary _) 0) ((none) -1))) \
                       (defun run-nullary ((o op)) Symbol \
                         (match o ((nullary g) (g)) ((binary _) (gensym)) ((none) (gensym))))";

// ---- still a function value ------------------------------------------------

#[test]
fn a_builtin_method_passed_as_an_argument_is_applied() {
    let src = "(defun call2 ((f (fn (i32 i32) i32)) (a i32) (b i32)) i32 (f a b)) \
               (call2 + 3 4)";
    assert_eq!(eval_ok(src), RtValue::Int(7));
}

/// Two separate reifications of one built-in must both dispatch to the same
/// implementation — the box carries only the name, so there is nothing
/// per-value to get wrong, and this pins that.
#[test]
fn two_reifications_of_one_builtin_both_apply() {
    let src = "(defun call2 ((f (fn (i32 i32) i32)) (a i32) (b i32)) i32 (f a b)) \
               (+ (call2 + 1 2) (call2 + 10 20))";
    assert_eq!(eval_ok(src), RtValue::Int(33));
}

#[test]
fn a_free_builtin_passed_as_an_argument_is_applied() {
    let src = "(defun call0 ((f (fn () Symbol))) Symbol (f)) (call0 gensym)";
    assert_sym(eval_ok(src));
}

/// `gensym` mints a fresh symbol per call, so the only thing to assert is the
/// shape — that the built-in really ran rather than the box being handed back.
fn assert_sym(v: RtValue) {
    match v {
        RtValue::Sexpr(typelisp::Value::Symbol(_)) => {}
        other => panic!("expected a gensym'd symbol, got {:?}", other),
    }
}

// ---- storable in an enum field (the part that was impossible) --------------

/// The case `enum_field_storable` used to answer `false` for, forcing every
/// instantiation of the enum onto the native `RtValue::Data` path. The
/// built-in goes into a field, comes back out through `match`, and is applied.
#[test]
fn a_builtin_method_stored_in_an_enum_field_is_recovered_and_applied() {
    let src = format!("{} (run-binary (op::binary +) 20 22)", OP_ENUM);
    assert_eq!(eval_ok(&src), RtValue::Int(42));
}

/// The free-built-in half — `recv_type: None` rather than a receiver path.
#[test]
fn a_free_builtin_stored_in_an_enum_field_is_recovered_and_applied() {
    let src = format!("{} (run-nullary (op::nullary gensym))", OP_ENUM);
    assert_sym(eval_ok(&src));
}

/// A *user* closure in the same field, so the two kinds of `Type::Fn` value —
/// `BoxedObj::CompiledClosure` and `BoxedObj::Builtin` — are shown to share
/// one field slot rather than the field being built-in-only.
#[test]
fn a_closure_and_a_builtin_share_the_same_enum_field() {
    let src = format!(
        "{} (+ (run-binary (op::binary +) 1 2) (run-binary (op::binary (lambda ((x i32) (y i32)) i32 (* x y))) 3 4))",
        OP_ENUM
    );
    assert_eq!(eval_ok(&src), RtValue::Int(15));
}

/// The payload-free variant, so the enum's storability classification is
/// exercised on a path that never writes a function into a field at all.
#[test]
fn the_payload_free_variant_still_matches() {
    let src = format!("{} (run-binary (op::none) 20 22)", OP_ENUM);
    assert_eq!(eval_ok(&src), RtValue::Int(-1));
}

/// The enum really is a heap `BoxedObj::Enum` now — the whole point of making
/// `Fn` storable. Before, `(op::binary +)` was an `RtValue::Data`, invisible to
/// the collector and unable to sit in a struct field or cross a boundary.
#[test]
fn an_enum_holding_a_builtin_is_a_heap_box_not_native_data() {
    let (mut h, mut chk, mut interp) = env();
    let v = eval_in(&mut h, &mut chk, &mut interp, &format!("{} (op::binary +)", OP_ENUM))
        .expect("eval failed");
    match v {
        RtValue::Sexpr(typelisp::Value::Boxed(id)) => {
            assert!(h.is_enum(id), "expected a heap enum box");
            assert_eq!(h.enum_field_count(id), 1);
            match h.enum_field(id, 0) {
                typelisp::Value::Boxed(f) => {
                    assert!(h.is_builtin_fn(f), "the stored field is not a built-in box");
                    assert_eq!(h.builtin_fn_name(f), "+");
                    assert!(h.builtin_fn_recv(f).is_some(), "`+` is a method, so it has a receiver type");
                }
                other => panic!("expected the field to be a boxed built-in, got {:?}", other),
            }
        }
        other => panic!("expected a heap enum value, got {:?}", other),
    }
}

// ---- surviving collection --------------------------------------------------

/// A stored built-in is a *collectible* box now rather than a Rust `String`, so
/// it has to stay reachable from a root for as long as the enum holding it is.
/// `gc_stress` collects on every single allocation, so a missed root fails
/// every run instead of one in a hundred.
#[test]
fn a_stored_builtin_survives_constant_collection() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, OP_ENUM).expect("definitions failed");
    h.set_gc_stress(true);
    let got = eval_in(&mut h, &mut chk, &mut interp, "(run-binary (op::binary +) 20 22)")
        .expect("eval under gc stress failed");
    assert_eq!(got, RtValue::Int(42));
}

// Printing a built-in function value has no test here because it has no
// *source-level* trigger: `format`'s `&rest` only accepts arguments with a
// `Sexpr` encoding, and a `Type::Fn` has none, so a built-in box cannot be
// handed to the printer from typelisp. The printer arms exist anyway — the
// box is a real heap value now, and the dispatch they replace ended in "any
// other box is an `f64`", i.e. a `Heap::float_value` panic. See
// `format.rs`'s `render_builtin` and `main.rs`'s `format_sexpr`.
