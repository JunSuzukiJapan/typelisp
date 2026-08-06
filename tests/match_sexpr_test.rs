//! Tests for `match` on a `Sexpr` scrutinee — re-allowed after Symbol/Sexpr
//! redesign Phase 5 had fenced it off (`Checker::check_match`), in
//! preparation for a user-facing `(read)`: read data's type is only known at
//! runtime, and `match` (with type refinement and exhaustiveness over the
//! ten `Sexpr` variants) is the language's natural eliminator for it. The
//! runtime machinery under test predates the fence: `match_sexpr_ctor` in
//! the interpreter, `compile-sexpr-tag-test`/`compile-sexpr-field` on the
//! compiled side (the latter exercised in `tests/compile_test.rs`).

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue, TopLevel};

fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

/// Like [`run`], but reading the `f64` result out before the run's `Heap`
/// drops: an `f64` is a `BoxedObj::Float` since the scalar unification, so
/// the value is an index into that heap rather than self-contained.
fn eval_f64(src: &str) -> f64 {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        RtValue::Sexpr(typelisp::Value::Boxed(id)) if h.is_float(id) => h.float_value(id),
        other => panic!("expected an f64, got {:?}", other),
    }
}

/// Like [`run`], but with the prelude loaded first — for `if-let`/
/// `while-let`, which are `defmacro`s expanding to a two-armed `match`
/// (`src/prelude.rs`), not checker-native special forms.
fn run_with_prelude(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok_with_prelude(src: &str) -> RtValue {
    run_with_prelude(src).expect("eval failed")
}

fn check(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v)?);
    }
    Ok(last.expect("no forms"))
}

// ---- per-variant dispatch and payload refinement ----------------------------

#[test]
fn match_refines_an_int_payload_to_i64() {
    assert_eq!(eval_ok("(match (Int 41) ((int n) (+ n 1)) (_ 0))"), RtValue::Int(42));
}

#[test]
fn match_refines_a_float_payload_to_f64() {
    assert_eq!(eval_f64("(match (Float 2.5) ((float f) f) (_ 0.0))"), 2.5);
}

#[test]
fn match_refines_a_str_payload_to_str() {
    assert_eq!(eval_ok("(match (Str \"hello\") ((str s) s) (_ \"no\"))"), RtValue::Str("hello".into()));
}

#[test]
fn match_refines_a_sym_payload_to_symbol() {
    assert_eq!(
        eval_ok("(match (quote foo) ((sym s) (symbol->string s)) (_ \"no\"))"),
        RtValue::Str("foo".into())
    );
}

#[test]
fn match_refines_char_and_bool_payloads() {
    assert_eq!(eval_ok("(match (Char #\\a) ((char c) c) (_ #\\z))"), RtValue::Char('a'));
    assert_eq!(eval_ok("(match (Bool true) ((bool b) b) (_ false))"), RtValue::Bool(true));
}

#[test]
fn match_selects_the_nil_arm_for_the_empty_list() {
    assert_eq!(eval_ok("(match (Nil) ((nil) 10) (_ 20))"), RtValue::Int(10));
}

#[test]
fn match_dispatches_bignum_and_ratio_arms() {
    assert_eq!(
        eval_ok("(match (Bignum 99999999999999999999999999) ((bignum _) 1) (_ 0))"),
        RtValue::Int(1)
    );
    assert_eq!(eval_ok("(match (Ratio 2/3) ((ratio _) 1) (_ 0))"), RtValue::Int(1));
}

#[test]
fn match_dispatches_a_runtime_chosen_variant() {
    let src = r#"
        (defun tag ((s Sexpr)) i32
          (match s
            ((nil) 0) ((int _) 1) ((float _) 2) ((char _) 3) ((bool _) 4)
            ((sym _) 5) ((str _) 6) ((cons _ _) 7) ((bignum _) 8) ((ratio _) 9)
            ((path _) 10)))
        (+ (+ (tag (Int 1)) (* 10 (tag (Str "s")))) (* 100 (tag (sexpr-cons (Nil) (Nil)))))
    "#;
    // 1 + 60 + 700: int=1, str=6, cons=7 — and the eleven-armed match above
    // is exhaustive without a wildcard, exercising full variant coverage.
    assert_eq!(eval_ok(src), RtValue::Int(761));
}

#[test]
fn match_dispatches_and_destructures_the_path_arm() {
    // A quoted `::`-path evaluates to `Value::Path` at runtime; `(path s)`
    // binds `s : Sexpr`, a fresh proper list of the segments as `sym`s (the
    // same shape a quoted `'(dep head)` list already has).
    assert_eq!(
        eval_ok(
            "(match (quote dep::head) ((path s) (equal s (list (quote dep) (quote head)))) (_ false))"
        ),
        RtValue::Bool(true)
    );
    // Segments stay real `sym`s, not re-stringified text: the first one is
    // reachable through ordinary list access.
    assert_eq!(
        eval_ok("(match (quote dep::head) ((path s) (sexpr-sym-name (sexpr-car s))) (_ \"no\"))"),
        RtValue::Str("dep".into())
    );
    // `Path` also constructs one back from a segment list — the inverse of
    // the match arm above.
    assert_eq!(
        eval_ok(
            "(match (Path (list (quote a) (quote b) (quote c)))
               ((path s) (equal s (list (quote a) (quote b) (quote c))))
               (_ false))"
        ),
        RtValue::Bool(true)
    );
    // A non-path scrutinee doesn't spuriously hit the `path` arm.
    assert_eq!(eval_ok("(match (Int 1) ((path _) 1) (_ 0))"), RtValue::Int(0));
}

// ---- structural (nested) patterns --------------------------------------------

#[test]
fn match_destructures_a_cons_with_nested_patterns() {
    assert_eq!(
        eval_ok("(match (sexpr-cons (Int 7) (Str \"tail\")) ((cons (int a) (str t)) a) (_ 0))"),
        RtValue::Int(7)
    );
}

#[test]
fn match_nested_pattern_mismatch_falls_through_to_the_next_arm() {
    // The scrutinee is a cons, but its car is a str, so the first arm's
    // nested (int a) fails and the second arm catches the same cons.
    assert_eq!(
        eval_ok(
            "(match (sexpr-cons (Str \"car\") (Nil))
               ((cons (int a) _) a)
               ((cons (str _) _) -1)
               (_ 0))"
        ),
        RtValue::Int(-1)
    );
}

#[test]
fn match_literal_sub_pattern_narrows_within_a_variant() {
    let src = r#"
        (defun pick ((s Sexpr)) i64
          (match s
            ((int 5) 50)
            ((int n) n)
            (_ 0)))
        (+ (pick (Int 5)) (pick (Int 3)))
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(53));
}

#[test]
fn match_walks_a_quoted_list() {
    // `quote` and macro arguments are the pre-`read` producers of compound
    // Sexpr data; summing a quoted list exercises match-driven recursion.
    let src = r#"
        (defun sum ((s Sexpr)) i64
          (match s
            ((cons (int n) rest) (+ n (sum rest)))
            (_ 0)))
        (sum (quote (1 2 3 4)))
    "#;
    assert_eq!(eval_ok(src), RtValue::Int(10));
}

// ---- if-let / while-let (prelude macros over the same match) -----------------

#[test]
fn if_let_binds_a_sexpr_pattern() {
    assert_eq!(eval_ok_with_prelude("(if-let ((int n) (Int 41)) (+ n 1) 0)"), RtValue::Int(42));
    assert_eq!(eval_ok_with_prelude("(if-let ((int n) (Str \"x\")) (+ n 1) 0)"), RtValue::Int(0));
}

#[test]
fn while_let_loops_over_a_sexpr_condition() {
    let src = r#"
        (let ((x (Int 3)) (acc (the i64 0)))
          (while-let ((int n) x)
            (setf acc (+ acc n))
            (setf x (if (> n 1) (Int (- n 1)) (Nil))))
          acc)
    "#;
    assert_eq!(eval_ok_with_prelude(src), RtValue::Int(6));
}

// ---- exhaustiveness ----------------------------------------------------------

#[test]
fn match_on_sexpr_without_full_coverage_is_a_type_error() {
    let err = check("(match (Int 1) ((int n) n))").expect_err("should be non-exhaustive");
    let msg = format!("{:?}", err);
    assert!(msg.contains("non-exhaustive"), "unexpected error: {}", msg);
}
