//! Tests for `defenum` — user-defined sum types (tagged unions).
//!
//! `defenum` registers a multi-variant `AdtKind::Sum` type, structurally
//! identical to the built-in `Option`/`Result`. Construction/`match`/`if-let`
//! and exhaustiveness checking are the shared sum-type machinery, so most of
//! these tests exercise that reuse rather than any enum-specific runtime code.
//! Constructors are reached qualified (`Name::Variant`) or bare after
//! `(use Name)`, exactly like `Option`/`Result` (see `namespace_test.rs`).
//!
//! The prelude is loaded so `if-let` (a prelude macro over `match`) is
//! available; `match` itself is a checker special form and needs no prelude.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue, TopLevel};

/// Check every form in `src` (with the prelude loaded); return the last node.
fn check(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v)?);
    }
    Ok(last.expect("no forms"))
}

/// Check, then execute, every form in `src`; return the last value produced.
fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(v) = interp.exec(&mut h, tl)? {
            last = v;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

fn assert_check_err(src: &str, needle: &str) {
    match check(src) {
        Ok(_) => panic!("expected a check error containing {:?}, but check succeeded", needle),
        Err(e) => {
            let msg = format!("{:?}", e);
            assert!(msg.contains(needle), "error {:?} did not contain {:?}", msg, needle);
        }
    }
}

// ---- registration + construction --------------------------------------------

#[test]
fn defenum_constructs_a_nullary_variant() {
    // `Color::Red` is variant index 0 (declaration order), no payload.
    match eval_ok("(defenum Color (Red) (Green) (Blue)) (Color::Red)") {
        RtValue::Data { variant, fields, .. } => {
            assert_eq!(variant, 0);
            assert!(fields.is_empty());
        }
        other => panic!("expected Data, got {:?}", other),
    }
}

#[test]
fn defenum_constructs_a_later_variant() {
    match eval_ok("(defenum Color (Red) (Green) (Blue)) (Color::Blue)") {
        RtValue::Data { variant, .. } => assert_eq!(variant, 2),
        other => panic!("expected Data, got {:?}", other),
    }
}

#[test]
fn defenum_constructs_a_variant_with_a_payload() {
    match eval_ok("(defenum Shape (Circle i32) (Nothing)) (Shape::Circle 7)") {
        RtValue::Data { variant, fields, .. } => {
            assert_eq!(variant, 0);
            assert_eq!(fields, vec![RtValue::Int(7)]);
        }
        other => panic!("expected Data, got {:?}", other),
    }
}

#[test]
fn a_bare_symbol_nullary_variant_is_accepted() {
    // `A` (no parens) is a nullary variant, equivalent to `(A)`.
    match eval_ok("(defenum E A (B i32)) (E::A)") {
        RtValue::Data { variant, fields, .. } => {
            assert_eq!(variant, 0);
            assert!(fields.is_empty());
        }
        other => panic!("expected Data, got {:?}", other),
    }
}

// ---- match ------------------------------------------------------------------

#[test]
fn match_binds_a_variant_payload() {
    let src = "(defenum Maybe<T> (Just T) (Nothing)) \
               (match (Maybe::Just 5) ((Just v) v) ((Nothing) 0))";
    assert_eq!(eval_ok(src), RtValue::Int(5));
}

#[test]
fn match_selects_the_nullary_arm() {
    // `Nothing` alone can't infer its `T` (like a bare `(option::none)`), so
    // pin it via a return-type-annotated helper — this is a property of the
    // shared inference, not of `defenum`.
    let src = "(defenum Maybe<T> (Just T) (Nothing)) \
               (defun mk () Maybe<i32> (Maybe::Nothing)) \
               (match (mk) ((Just v) v) ((Nothing) 99))";
    assert_eq!(eval_ok(src), RtValue::Int(99));
}

#[test]
fn a_generic_enum_infers_its_type_argument() {
    // The nullary `Nothing` learns T=i32 from the arm result type unification.
    let src = "(defenum Maybe<T> (Just T) (Nothing)) \
               (match (Maybe::Just 42) ((Just v) v) ((Nothing) 0))";
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

#[test]
fn a_non_exhaustive_match_is_rejected() {
    let src = "(defenum Color (Red) (Green) (Blue)) \
               (match (Color::Red) ((Red) 1) ((Green) 2))";
    assert_check_err(src, "non-exhaustive");
}

#[test]
fn a_wildcard_makes_a_match_exhaustive() {
    let src = "(defenum Color (Red) (Green) (Blue)) \
               (match (Color::Blue) ((Red) 1) (_ 0))";
    assert_eq!(eval_ok(src), RtValue::Int(0));
}

// ---- if-let (prelude macro over match) --------------------------------------

#[test]
fn if_let_binds_a_matching_variant() {
    let src = "(defenum Maybe<T> (Just T) (Nothing)) \
               (if-let ((Just v) (Maybe::Just 5)) v 0)";
    assert_eq!(eval_ok(src), RtValue::Int(5));
}

#[test]
fn if_let_takes_the_else_branch_on_a_mismatch() {
    let src = "(defenum Maybe<T> (Just T) (Nothing)) \
               (defun mk () Maybe<i32> (Maybe::Nothing)) \
               (if-let ((Just v) (mk)) v 0)";
    assert_eq!(eval_ok(src), RtValue::Int(0));
}

// ---- constructor visibility (qualified / use) -------------------------------

#[test]
fn a_bare_constructor_is_unresolved_without_use() {
    // Like Option/Result, variants are not bare-callable until `use`.
    assert_check_err("(defenum Color (Red) (Green) (Blue)) (Red)", "");
}

#[test]
fn use_makes_constructors_bare_callable() {
    let src = "(defenum Color (Red) (Green) (Blue)) (use Color) (Green)";
    match eval_ok(src) {
        RtValue::Data { variant, .. } => assert_eq!(variant, 1),
        other => panic!("expected Data, got {:?}", other),
    }
}

// ---- error cases ------------------------------------------------------------

#[test]
fn a_duplicate_variant_name_is_rejected() {
    assert_check_err("(defenum Bad (A) (B) (A))", "duplicate variant");
}

#[test]
fn an_enum_with_no_variants_is_rejected() {
    assert_check_err("(defenum Empty)", "at least one variant");
}

#[test]
fn a_constructor_arity_mismatch_is_rejected() {
    assert_check_err("(defenum Shape (Circle i32) (Nothing)) (Shape::Circle 1 2)", "");
}

// ---- header syntax: `Name<T>` only, list form rejected ----------------------

#[test]
fn the_old_list_form_defstruct_header_is_rejected() {
    assert_check_err("(defstruct (Pair A B) (a A) (b B))", "definition name");
}

#[test]
fn the_old_list_form_defun_header_is_rejected() {
    assert_check_err("(defun (identity T) ((x T)) T x)", "definition name");
}

#[test]
fn the_angle_bracket_defenum_header_works() {
    // `Maybe<T>` reads as one symbol and splits into name + type params.
    let src = "(defenum Maybe<T> (Just T) (Nothing)) \
               (match (Maybe::Just 3) ((Just v) v) ((Nothing) 0))";
    assert_eq!(eval_ok(src), RtValue::Int(3));
}
