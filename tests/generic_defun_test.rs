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

/// [`eval_ok`] with the prelude loaded — for the tests below, which need
/// `Vector`/`Iter`/`doiter`/`find-if` rather than only the checker primitives.
fn eval_ok_with_prelude(src: &str) -> Value {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    last
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

/// A bounded generic delegating to *another* bounded generic, where the two
/// spell the associated-type pin differently.
///
/// `validate_where_bounds` compared the callee's declared pin against the
/// caller's raw, both written in their own type parameters — so the check only
/// passed when the two functions happened to use the same letter. The prelude's
/// `elt` could call `nth` (both say `A`); the identical shape spelled `B` could
/// not, and a pin that is a *structured* type (`(Item cons-cell<K,V>)`, which
/// `assoc-if` needs to reach `find-if`) never matched at all. That is why the
/// prelude's own comment claimed delegation was unimplemented while the code
/// right below it delegated.
#[test]
fn a_bounded_generic_can_delegate_when_the_item_variable_is_named_differently() {
    assert_eq!(
        eval_ok_with_prelude(
            "(defun mylen<I,B> ((it I)) i32 (where (Iter I (Item B)))
               (let ((n 0)) (doiter (x it) (setf n (+ n 1))) n))
             (defun mylen2<J,C> ((it J)) i32 (where (Iter J (Item C)))
               (mylen it))
             (let ((v (the Vector<i32> (Vector::new))))
               (push v 1) (push v 2)
               (mylen2 (iter v)))"
        ),
        Value::Int(2)
    );
}

/// The same delegation with a *structured* associated-type pin.
#[test]
fn a_bounded_generic_can_delegate_with_a_structured_associated_type_pin() {
    assert_eq!(
        eval_ok_with_prelude(
            "(defun firstkey<I,K,V> ((it I)) Option<K> (where (Iter I (Item cons-cell<K,V>)))
               (match (find-if it (lambda ((p cons-cell<K,V>)) bool true))
                 ((some p) (option::some (car p)))
                 ((none) (option::none))))
             (let ((al (the Vector<cons-cell<i32,i32>> (Vector::new))))
               (push al (cons 7 8))
               (unwrap (firstkey (iter al))))"
        ),
        Value::Int(7)
    );
}

/// A generic `defmethod` whose receiver spells the owner type's parameter with
/// a different letter than the owner's own declaration.
///
/// `check_assoc_call` specializes a call by zipping the *owner type's*
/// parameter names against the receiver's concrete arguments, and the
/// registered signature used to keep whatever names the `defmethod` was written
/// in — so `Vector<A>` left its `a` unsubstituted and rejected a concrete
/// argument, while the identical method spelled `Vector<T>` worked. Same
/// name-coincidence failure as the two above, in a third place.
#[test]
fn a_generic_defmethod_may_name_the_receivers_type_parameter_freely() {
    assert_eq!(
        eval_ok_with_prelude(
            "(defmethod keepif ((self Vector<A>) (pred (fn (A) bool))) Vector<A>
               (filter (iter self) pred))
             (let ((v (the Vector<i32> (Vector::new))))
               (push v 1) (push v 5)
               (len (keepif v (lambda ((x i32)) bool (> x 2)))))"
        ),
        Value::Int(1)
    );
}

/// And on a user `defstruct`, where the same rule applies.
#[test]
fn a_generic_defmethod_on_a_user_struct_may_rename_its_type_parameter() {
    assert_eq!(
        eval_ok_with_prelude(
            "(defstruct box<T> (v T))
             (defmethod apply-to ((self box<A>) (f (fn (A) A))) A (f self::v))
             (apply-to (box::new 4) (lambda ((x i32)) i32 (* x 2)))"
        ),
        Value::Int(8)
    );
}
