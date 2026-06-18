//! Tests for the typelisp-defined prelude
//! ([cl-equivalence-catalog.md](../docs/cl-equivalence-catalog.md) §2.1,
//! roadmap step 7a): `consp`/`null`/`atom`/`equal` (`src/prelude.rs`), plus
//! their Rust-side foundations added alongside them — `not`, and the `eq`
//! coverage gaps it closes (`sexpr`/`bool`/`i32`/`i64`/`f64`).

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue};

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
        if let Some(val) = interp.exec(&mut h, tl)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

fn type_error(src: &str) {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut result: Result<_, Error> = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e);
            break;
        }
    }
    assert!(result.is_err(), "expected a type error");
}

// ---- not --------------------------------------------------------------------

#[test]
fn not_negates() {
    assert_eq!(eval_ok("(not true)"), RtValue::Bool(false));
    assert_eq!(eval_ok("(not false)"), RtValue::Bool(true));
}

// ---- eq, across every type that has it -----------------------------------------

#[test]
fn eq_on_bool() {
    assert_eq!(eval_ok("(eq true true)"), RtValue::Bool(true));
    assert_eq!(eval_ok("(eq true false)"), RtValue::Bool(false));
}

#[test]
fn eq_on_i32() {
    assert_eq!(eval_ok("(eq 1 1)"), RtValue::Bool(true));
    assert_eq!(eval_ok("(eq 1 2)"), RtValue::Bool(false));
}

#[test]
fn eq_on_i64() {
    let src = "(defun f ((a i64) (b i64)) bool (eq a b)) (f 1 1)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn eq_on_f64() {
    let src = "(defun f ((a f64) (b f64)) bool (eq a b)) (f 1.5 1.5)";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn eq_on_char() {
    assert_eq!(eval_ok(r"(eq #\a #\a)"), RtValue::Bool(true));
}

#[test]
fn eq_on_string() {
    assert_eq!(eval_ok(r#"(eq "a" "a")"#), RtValue::Bool(true));
}

#[test]
fn eq_on_sexpr_atoms_compares_by_value() {
    let src = "(eq (quote a) (quote a))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn eq_on_sexpr_cons_is_identity_not_structural() {
    // Two separately-built (quote (a)) cons cells have equal content but are
    // not the *same* cell — `eq` says false, `equal` says true (see below).
    let src = "(eq (quote (a)) (quote (a)))";
    assert_eq!(eval_ok(src), RtValue::Bool(false));
}

// ---- consp / null / atom ----------------------------------------------------

#[test]
fn consp_is_true_for_a_cons() {
    assert_eq!(eval_ok("(consp (quote (a b)))"), RtValue::Bool(true));
}

#[test]
fn consp_is_false_for_nil() {
    assert_eq!(eval_ok("(consp (quote ()))"), RtValue::Bool(false));
}

#[test]
fn consp_is_false_for_an_atom() {
    assert_eq!(eval_ok("(consp (quote a))"), RtValue::Bool(false));
}

#[test]
fn null_is_true_for_nil() {
    assert_eq!(eval_ok("(null (quote ()))"), RtValue::Bool(true));
}

#[test]
fn null_is_false_for_a_cons() {
    assert_eq!(eval_ok("(null (quote (a)))"), RtValue::Bool(false));
}

#[test]
fn atom_is_true_for_nil_and_scalars() {
    assert_eq!(eval_ok("(atom (quote ()))"), RtValue::Bool(true));
    assert_eq!(eval_ok("(atom (quote a))"), RtValue::Bool(true));
}

#[test]
fn atom_is_false_for_a_cons() {
    assert_eq!(eval_ok("(atom (quote (a)))"), RtValue::Bool(false));
}

// ---- equal --------------------------------------------------------------------

#[test]
fn equal_is_true_for_separately_built_equal_lists() {
    let src = "(equal (quote (a b c)) (quote (a b c)))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn equal_is_false_for_different_lists() {
    let src = "(equal (quote (a b c)) (quote (a b d)))";
    assert_eq!(eval_ok(src), RtValue::Bool(false));
}

#[test]
fn equal_is_false_for_different_lengths() {
    let src = "(equal (quote (a b)) (quote (a b c)))";
    assert_eq!(eval_ok(src), RtValue::Bool(false));
}

#[test]
fn equal_compares_string_content_not_identity() {
    // Two separately-built `Sexpr::Str`s with the same text: `eq` would say
    // false (different heap allocations), `equal` must say true.
    let src = r#"(equal (quote ("hi")) (quote ("hi")))"#;
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn equal_on_atoms_matches_eq() {
    assert_eq!(eval_ok("(equal (quote a) (quote a))"), RtValue::Bool(true));
    assert_eq!(eval_ok("(equal (quote a) (quote b))"), RtValue::Bool(false));
}

#[test]
fn equal_requires_matching_sexpr_type() {
    type_error("(equal 1 2)");
}
