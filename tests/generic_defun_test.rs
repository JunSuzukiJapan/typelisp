//! Tests for generic type parameters on `defun`: `(defun name<T1,T2...> ...)`.
//!
//! The call-site inference reuses the same `unify`/`subst_apply` machinery
//! `check_construct` uses for an ADT's `params` (see `Checker::check_call`),
//! so these tests focus on (a) the new declaration syntax parsing, (b)
//! inference from argument types, (c) interaction with existing generic
//! ADTs (`Option<T>`) including `match` binding inside a generic body, and
//! (d) that non-generic `defun` keeps behaving exactly as before.

extern crate typelisp;
use typelisp::{Checker, Error, Heap, Interp, Reader, Value, TopLevelForm};

fn run(src: &str) -> Result<(Value, Heap), Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?;
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    Ok((last, h))
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed").0
}

/// Check (but don't evaluate) every form in `src`, returning the last form's
/// checked result. Used to assert on type errors without needing a runnable
/// program.
fn check_program(src: &str) -> Result<TopLevelForm, Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?);
    }
    Ok(last.expect("no forms"))
}

fn assert_type_error(src: &str) {
    match check_program(src) {
        Err(Error::TypeError(_)) => {}
        other => panic!("expected TypeError, got {:?}", other),
    }
}

// ---- declaration syntax + basic inference ----------------------------------

#[test]
fn identity_resolves_per_call_site_type() {
    assert_eq!(
        eval_ok("(defun identity<T> ((x T)) T x) (identity 42)"),
        Value::Int(42)
    );
    assert_eq!(
        eval_ok("(defun identity<T> ((x T)) T x) (identity true)"),
        Value::Bool(true)
    );
}

#[test]
fn generic_defun_with_two_type_params() {
    // `fst`/`snd`-style: only one of the two type params is actually used,
    // proving each call site can bind them independently.
    let src = r#"
        (defun pick-first<A,B> ((x A) (y B)) A x)
        (pick-first 7 true)
    "#;
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn recursive_generic_defun() {
    // Self-recursive generic call: each recursive call re-resolves T against
    // the same concrete type (i32 here), exercising that the signature
    // registered before body-checking carries `type_params` through.
    let src = r#"
        (defun last-of<T> ((n i32) (x T)) T
          (if (= n 0) x (last-of (- n 1) x)))
        (last-of 3 99)
    "#;
    assert_eq!(eval_ok(src), Value::Int(99));
}

// ---- interaction with existing generic ADTs ---------------------------------

#[test]
fn unwrap_option_propagates_bound_type_through_match() {
    // `opt`'s static type is `Option<T>`; the `some` arm's bound variable
    // must come back out as `T` (not the ADT's own internal param name),
    // confirming `check_ctor_pattern`'s subst chain composes correctly with
    // a defun-level type parameter.
    let src = r#"
        (defun unwrap<T> ((opt Option<T>)) T
          (match opt ((some x) x) ((none) (panic "unwrap: None"))))
        (unwrap (option::some 5))
    "#;
    assert_eq!(eval_ok(src), Value::Int(5));
}

#[test]
fn generic_defun_returning_option() {
    let src = r#"
        (defun wrap<T> ((x T)) Option<T> (option::some x))
        (match (wrap 9) ((some x) x) ((none) 0))
    "#;
    assert_eq!(eval_ok(src), Value::Int(9));
}

// ---- inference failure -------------------------------------------------------

#[test]
fn unused_type_param_is_uninferable() {
    // `T` appears only in the return type, with no argument and no expected
    // type to seed it from — `check_construct` has the same "cannot infer"
    // failure mode for an ADT's unconstrained params.
    assert_type_error("(defun make-none<T> () Option<T> (option::none)) (make-none)");
}

#[test]
fn mismatched_concrete_argument_types_still_rejected() {
    // Calling with concrete types that don't match the (non-generic) param
    // type is still a plain type error.
    assert_type_error("(defun add1 ((x i32)) i32 (+ x 1)) (add1 true)");
}

// ---- non-generic defun is unaffected -----------------------------------------

#[test]
fn ordinary_defun_without_type_params_still_works() {
    assert_eq!(
        eval_ok("(defun add1 ((x i32)) i32 (+ x 1)) (add1 41)"),
        Value::Int(42)
    );
}
