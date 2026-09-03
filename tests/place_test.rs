//! Tests for the generalized `setf` place mechanism: `incf`/`decf`/
//! `rotatef`/`shiftf`, and the call-form place `(setf (get recv key...) v)`
//! (this project's analogue of CL's `(setf (gethash k h) v)`) — see
//! `Checker::check_setf_call_place`/`Checker::place_dedup` in
//! `src/check/checker.rs`. Variable and `var::field` places were already
//! covered elsewhere (`check_test.rs`'s `setf_*` tests, `struct_test.rs`).

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Type, Value};

/// Check every form and return the LAST one's expression type — `None` if that
/// form was a definition. The type is no longer part of the checked form (see
/// `check::core::Checked`), so it comes from `Checker::expr_type`.
fn check(src: &str) -> Result<Option<Type>, Error> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    for v in vs {
        chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?;
    }
    Ok(chk.expr_type().cloned())
}

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(v) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = v;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

/// [`run`], but with the prelude loaded first — for the tests whose place
/// needs something the prelude defines. A `HashTable<K,V>`'s key methods
/// carry `(where (Hash K))` since Phase 6a, and every `Hash` implementation
/// (including `string`'s) is a prelude `impl`, so a bare `Checker` cannot
/// type-check `(get h "a")` at all.
fn eval_ok_with_prelude(src: &str) -> Value {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
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

/// [`check`], but with the prelude loaded first -- same reason as
/// [`eval_ok_with_prelude`]: without the prelude's `Hash` impls the
/// `(where (Hash K))` on `get`/`set` cannot be discharged, so the check
/// fails for lack of a *method*, which would let a test that means to
/// assert a genuine type error pass for the wrong reason.
fn check_with_prelude(src: &str) -> Result<Option<Type>, Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    for v in vs {
        chk.check_form(&mut h, &interp, v).map_err(Error::into_kind)?;
    }
    Ok(chk.expr_type().cloned())
}

// ---- incf / decf --------------------------------------------------------

#[test]
fn incf_defaults_delta_to_one() {
    let src = "(let ((x 10)) (incf x) x)";
    assert_eq!(eval_ok(src), Value::Int(11));
}

#[test]
fn incf_with_explicit_delta() {
    let src = "(let ((x 10)) (incf x 5) x)";
    assert_eq!(eval_ok(src), Value::Int(15));
}

#[test]
fn incf_evaluates_to_the_new_value() {
    let src = "(let ((x 10)) (incf x 5))";
    assert_eq!(eval_ok(src), Value::Int(15));
}

#[test]
fn decf_with_explicit_delta() {
    let src = "(let ((x 10)) (decf x 3) x)";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn incf_on_a_hashtable_value_type_mismatch_is_rejected() {
    // `(get h k)` reads `Option<i32>`, not `i32` — `(+ (get h k) delta)`
    // is a genuine type error, not a bug: this project's `get`/`set`
    // convention has an asymmetric read/write type (unlike CL's untyped
    // `gethash`), so `incf` can never paper over a missing entry.
    let src = "(defun make-h () HashTable<string,i32> (HashTable::new))
               (let ((h (make-h))) (set h \"a\" 1) (incf (get h \"a\")))";
    assert!(matches!(check_with_prelude(src), Err(Error::TypeError(_))));
}

// ---- rotatef / shiftf -----------------------------------------------------

#[test]
fn rotatef_two_variables() {
    let src = "(let ((a 1) (b 2)) (rotatef a b) (- (* a 10) b))";
    // a should now be 2, b should now be 1 -> 2*10 - 1 = 19
    assert_eq!(eval_ok(src), Value::Int(19));
}

#[test]
fn rotatef_three_variables_shifts_cyclically() {
    let src = "(let ((a 1) (b 2) (c 3)) (rotatef a b c) (+ (* a 100) (+ (* b 10) c)))";
    // new a = old b = 2, new b = old c = 3, new c = old a = 1 -> 231
    assert_eq!(eval_ok(src), Value::Int(231));
}

#[test]
fn rotatef_single_place_is_a_no_op() {
    let src = "(let ((a 5)) (rotatef a) a)";
    assert_eq!(eval_ok(src), Value::Int(5));
}

#[test]
fn rotatef_with_no_places_checks_as_unit() {
    let src = "(rotatef)";
    assert_eq!(check(src).expect("check failed"), Some(Type::Unit));
}

#[test]
fn shiftf_returns_the_first_places_old_value() {
    let src = "(let ((a 1) (b 2)) (shiftf a b 99))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn shiftf_shifts_values_left_and_appends_the_new_value() {
    let src = "(let ((a 1) (b 2)) (shiftf a b 99) (+ (* a 100) b))";
    // new a = old b = 2, new b = 99 -> 299
    assert_eq!(eval_ok(src), Value::Int(299));
}

#[test]
fn shiftf_single_place() {
    let src = "(let ((a 1)) (shiftf a 42) a)";
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn shiftf_requires_at_least_one_place() {
    assert!(matches!(check("(shiftf 1)"), Err(Error::TypeError(_))));
}

#[test]
fn rotatef_call_form_places_dedup_shared_subexpressions() {
    // Both places share the same receiver `t` and only differ by index —
    // exercises `Checker::place_dedup` binding `t` once per place rather
    // than re-evaluating the receiver expression for the read and the write.
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 10)
                 (push v 20)
                 (rotatef (get v 0) (get v 1))
                 (+ (* (get v 0) 100) (get v 1)))";
    assert_eq!(eval_ok(src), Value::Int(2010));
}

// ---- setf on a call-form place --------------------------------------------

#[test]
fn setf_get_writes_through_a_hashtable_entry() {
    let src = "(defun make-h () HashTable<string,i32> (HashTable::new))
               (let ((h (make-h)))
                 (set h \"a\" 1)
                 (setf (get h \"a\") 41)
                 (match (get h \"a\") ((Some x) x) ((None) -1)))";
    assert_eq!(eval_ok_with_prelude(src), Value::Int(41));
}

#[test]
fn setf_rejects_a_call_form_place_with_no_matching_setter() {
    // `Vector<T>` has no `set-len` method, so `len` is a getter with no
    // corresponding setter — the receiver's *type* genuinely lacks one,
    // unlike `setf_rejects_a_place_that_is_neither_variable_field_nor_call_form`
    // below, which is about the place's *shape*.
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v))) (push v 1) (setf (len v) 2))";
    assert!(matches!(check(src), Err(Error::TypeError(_))));
}

#[test]
fn setf_rejects_a_place_that_is_neither_variable_field_nor_call_form() {
    assert!(matches!(check("(setf 1 2)"), Err(Error::TypeError(_))));
}

// ---- generalized `set-{accessor}` convention on a user-defined type ------

#[test]
fn setf_on_a_user_defined_accessor_uses_the_set_prefix_convention() {
    // No `defsetf`-style registration exists — `Checker::check_setf_call_place`
    // just checks whether `cells`'s (statically known) type has an instance
    // method literally named `set-at`, exactly the same `X`/`set-X`
    // convention `defstruct` already uses for field accessors.
    let src = "(defstruct cells (data Vector<i32>))
               (defmethod at ((self cells) (i i32)) i32 (get self::data i))
               (defmethod set-at ((self cells) (i i32) (v i32)) () (setf (get self::data i) v))
               (defun make-c () cells
                 (let ((d (the Vector<i32> (Vector::new))))
                   (push d 1)
                   (push d 2)
                   (cells::new d)))
               (let ((c (make-c)))
                 (setf (at c 0) 99)
                 (at c 0))";
    assert_eq!(eval_ok(src), Value::Int(99));
}

#[test]
fn setf_on_a_user_defined_accessor_with_no_setter_is_rejected() {
    let src = "(defstruct cells (data Vector<i32>))
               (defmethod at ((self cells) (i i32)) i32 (get self::data i))
               (defun make-c () cells (cells::new (the Vector<i32> (Vector::new))))
               (let ((c (make-c))) (setf (at c 0) 99))";
    assert!(matches!(check(src), Err(Error::TypeError(_))));
}
