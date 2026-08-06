//! A `Symbol`-typed `defenum` field, across the interpreted and compiled tiers.
//!
//! `Type::Symbol` is the one scalar that no representability predicate names.
//! It is not a `Type::Named`, so it falls past every arm of `is_heap_repr_seen`
//! onto that function's `_ => false`, and it is absent from
//! `is_boxable_scalar`'s explicit list (integers, floats, `Bool`/`Char`/`Str`/
//! `Bignum`/`Ratio`) too — so `enum_field_storable` answers `false` for it. The
//! same "`Symbol` is not a `Type::Named`, so the predicate misses it" slip is
//! already recorded once at `Interp::decode_compiled_return`'s `Type::Symbol`
//! arm, which exists because `is_boxed_sexpr_type` misses a bare `Symbol`
//! return for that very reason.
//!
//! These tests exist because that `false` looks like it should be a bug and is
//! not — three separate mechanisms decide an enum's representation, and only
//! one of them consults that predicate:
//!
//! * **Construction** (`build_enum_value`) is value-driven: it boxes onto the
//!   heap whenever every field converts through `rtvalue_to_struct_field`. A
//!   `Symbol` is carried as `RtValue::Sexpr(Value::Symbol)`, so it converts.
//! * **The compiled-return decode** (`is_boxed_sexpr_type`) tests
//!   `is_enum_path` alone — any enum decodes as a box, whatever its fields are.
//! * **Binding-slot routing** (`is_heap_repr_ty`) is the one that does consult
//!   it, and answers "native". A `Slot::Native` holds an `RtValue::Sexpr` per-
//!   fectly well and `collect_sexpr_roots` roots it, so this costs a `sync_roots`
//!   pass rather than correctness.
//!
//! So the three disagree and nothing observable breaks. Pinning that here keeps
//! it from being "fixed" by adding `Symbol` to the predicate, which would treat
//! the symptom: the real defect is that construction decides a *static* question
//! from a runtime value shape at all, and it goes away structurally when
//! `RtValue::Data` — the thing it falls back to — is deleted.

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

/// `:hello` — a keyword, the one self-evaluating `Symbol` literal
/// (`Checker::check`'s `Value::Symbol` arm); `(quote hello)` is a `Sexpr`, not
/// a `Symbol`, so it cannot go in this field.
const TAG_ENUM: &str = "(defenum tag (named Symbol) (anon)) \
                        (defun make-tag ((s Symbol)) tag (tag::named s)) \
                        (defun tag-name ((t tag)) Symbol \
                          (match t ((named s) s) ((anon) :anon)))";

fn assert_named(h: &Heap, v: &RtValue, expected: &str) {
    match v {
        RtValue::Sexpr(typelisp::Value::Symbol(id)) => assert_eq!(h.symbol_name(*id), expected),
        other => panic!("expected the symbol `{}`, got {:?}", expected, other),
    }
}

// ---- interpreted tier: already correct, because construction is value-driven

#[test]
fn an_interpreted_enum_with_a_symbol_field_is_a_heap_box() {
    let (mut h, mut chk, mut interp) = env();
    let v = eval_in(&mut h, &mut chk, &mut interp, &format!("{} (tag::named :hello)", TAG_ENUM))
        .expect("eval failed");
    match v {
        RtValue::Sexpr(typelisp::Value::Boxed(id)) => {
            assert!(h.is_enum(id), "expected a heap enum box");
            assert_eq!(h.enum_field_count(id), 1);
            match h.enum_field(id, 0) {
                typelisp::Value::Symbol(s) => assert_eq!(h.symbol_name(s), ":hello"),
                other => panic!("expected a Value::Symbol field, got {:?}", other),
            }
        }
        other => panic!("expected a heap enum value, got {:?}", other),
    }
}

#[test]
fn a_symbol_stored_in_an_enum_field_is_recovered() {
    let (mut h, mut chk, mut interp) = env();
    let v = eval_in(&mut h, &mut chk, &mut interp, &format!("{} (tag-name (tag::named :hello))", TAG_ENUM))
        .expect("eval failed");
    assert_named(&h, &v, ":hello");
}

#[test]
fn the_payload_free_variant_still_matches() {
    let (mut h, mut chk, mut interp) = env();
    let v = eval_in(&mut h, &mut chk, &mut interp, &format!("{} (tag-name (tag::anon))", TAG_ENUM))
        .expect("eval failed");
    assert_named(&h, &v, ":anon");
}

/// The field is a real heap reference, so it has to stay reachable from a root.
/// `gc_stress` collects on every allocation, so a missed root fails every run.
#[test]
fn a_stored_symbol_survives_constant_collection() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, TAG_ENUM).expect("definitions failed");
    h.set_gc_stress(true);
    let v = eval_in(&mut h, &mut chk, &mut interp, "(tag-name (tag::named :hello))")
        .expect("eval under gc stress failed");
    assert_named(&h, &v, ":hello");
}

// ---- compiled tier ---------------------------------------------------------

/// A compiled function *returning* the enum. This is the crossing where a
/// mis-decode would be silent rather than loud: `decode_compiled_return` hands
/// a tagged word back as a bare `RtValue::Int` for any type
/// `is_boxed_sexpr_type` doesn't recognize, and every later read of it is then
/// wrong. It passes because that predicate tests `is_enum_path` alone — not
/// because the `Symbol` field is classified storable, which it is not.
#[test]
fn a_compiled_function_returning_the_enum_decodes_it_as_an_enum() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, TAG_ENUM).expect("definitions failed");
    if eval_in(&mut h, &mut chk, &mut interp, "(compile make-tag)").is_err() {
        panic!("`make-tag` declined compilation, so this test cannot observe the boundary");
    }
    let v = eval_in(&mut h, &mut chk, &mut interp, "(make-tag :hello)").expect("compiled call failed");
    match v {
        RtValue::Sexpr(typelisp::Value::Boxed(id)) => {
            assert!(h.is_enum(id), "expected a heap enum box");
            match h.enum_field(id, 0) {
                typelisp::Value::Symbol(s) => assert_eq!(h.symbol_name(s), ":hello"),
                other => panic!("expected a Value::Symbol field, got {:?}", other),
            }
        }
        other => panic!("expected a heap enum value from compiled code, got {:?}", other),
    }
}

/// The full round trip through the compiled tier: build the enum there, read
/// the field back out there.
#[test]
fn a_compiled_round_trip_through_the_enum_preserves_the_symbol() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, TAG_ENUM).expect("definitions failed");
    for f in ["make-tag", "tag-name"] {
        if eval_in(&mut h, &mut chk, &mut interp, &format!("(compile {})", f)).is_err() {
            panic!("`{}` declined compilation, so this test cannot observe the boundary", f);
        }
    }
    let v = eval_in(&mut h, &mut chk, &mut interp, "(tag-name (make-tag :hello))")
        .expect("compiled round trip failed");
    assert_named(&h, &v, ":hello");
}
