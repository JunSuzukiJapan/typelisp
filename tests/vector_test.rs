//! Tests for `Vector<T>` — a builtin growable sequence reusing
//! [`RtValue::Struct`] (no dedicated `RtValue` variant — see
//! [[typelisp-vector-defstruct-revert]] for why a dedicated variant was
//! rejected the first time `Vector<T>` was attempted).

extern crate typelisp;
use typelisp::{Checker, Error, EvalError, Heap, Interp, Reader, RtValue, TopLevel};

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

fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(v) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = v;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

/// The text of a `string` result.
///
/// A `string` is a heap `Value::Str` since the scalar unification, so reading
/// one needs the heap it lives in — and `run` above drops its heap on return.
/// Hence this parallel runner, which reads the text out first.
fn eval_string(src: &str) -> String {
    let mut h = Heap::with_capacity(8192);
    let mut chk = Checker::new();
    let interp = Interp::new();
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        RtValue::Sexpr(typelisp::Value::Str(id)) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

#[test]
fn new_makes_an_empty_vector_with_len_zero() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (len (make-v))";
    assert_eq!(eval_ok(src), RtValue::Int(0));
}

#[test]
fn push_then_get_round_trips() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 1)
                 (push v 2)
                 (push v 3)
                 (get v 1))";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn push_increments_len() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 10)
                 (push v 20)
                 (len v))";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn set_overwrites_an_existing_element() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 1)
                 (push v 2)
                 (set v 0 99)
                 (get v 0))";
    assert_eq!(eval_ok(src), RtValue::Int(99));
}

#[test]
fn get_out_of_bounds_panics() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v))) (push v 1) (get v 5))";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn set_out_of_bounds_panics() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v))) (set v 0 1))";
    assert!(matches!(run(src), Err(EvalError::Panic(_))));
}

#[test]
fn pop_removes_and_returns_some_of_the_last_element() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (defun f () i32
                 (let ((v (make-v)))
                   (push v 1)
                   (push v 2)
                   (push v 3)
                   (match (pop v) ((Some x) x) ((None) -1))))
               (f)";
    assert_eq!(eval_ok(src), RtValue::Int(3));
}

#[test]
fn pop_shrinks_len_by_one() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 1)
                 (push v 2)
                 (pop v)
                 (len v))";
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn pop_on_an_empty_vector_returns_none() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (defun f () i32
                 (let ((v (make-v)))
                   (match (pop v) ((Some x) x) ((None) -1))))
               (f)";
    assert_eq!(eval_ok(src), RtValue::Int(-1));
}

#[test]
fn pushing_a_mismatched_element_type_is_a_type_error() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v))) (push v \"oops\"))";
    assert!(check(src).is_err());
}

#[test]
fn vector_holds_strings_too() {
    let src = "(defun make-v () Vector<string> (Vector::new))
               (let ((v (make-v)))
                 (push v \"a\")
                 (push v \"b\")
                 (get v 1))";
    assert_eq!(eval_string(src), "b");
}

#[test]
fn two_vectors_with_different_element_types_coexist() {
    let src = "(defun make-i () Vector<i32> (Vector::new))
               (defun make-s () Vector<string> (Vector::new))
               (let ((vi (make-i)) (vs (make-s)))
                 (push vi 1)
                 (push vs \"x\")
                 (+ (get vi 0) (len vs)))";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn vector_of_sexpr_gets_elements_back_as_sexprs() {
    // `get`'s checked return type at this call site is `Sexpr`
    // (`Vector<Sexpr>`'s instantiated element type), which must win over
    // `decode_struct_field`'s shape heuristic — a stored quoted `42` is a
    // `Value::Int` in the slot, shape-identical to a `Vector<i64>` element.
    // Read back with `sexpr-int` (`match` on a `Sexpr` is fenced off —
    // Symbol/Sexpr redesign Phase 5); it still panics if the element decoded
    // to a bare scalar rather than a genuine `Sexpr` node.
    let src = "(defun make-v () Vector<Sexpr> (Vector::new))
               (let ((v (make-v)))
                 (push v '41)
                 (push v '42)
                 (sexpr-int (get v 1)))";
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

// ---- CL-order `push`/setf-able `(get ...)` place -----------------------------

#[test]
fn push_accepts_cl_argument_order_too() {
    // `(push item place)`, CL's own argument order, resolves via
    // `Checker::try_instance_method_swapped` once the receiver-first
    // `(push vec item)` convention fails to match.
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push 1 v)
                 (push v 2)
                 (push 3 v)
                 (get v 2))";
    assert_eq!(eval_ok(src), RtValue::Int(3));
}

#[test]
fn setf_get_writes_through_a_vector_element() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 1)
                 (push v 2)
                 (setf (get v 0) 99)
                 (get v 0))";
    assert_eq!(eval_ok(src), RtValue::Int(99));
}

#[test]
fn incf_on_a_call_form_place_evaluates_the_index_exactly_once() {
    // If `(incf (get v (progn (setf idx (+ idx 1)) idx)) 10)` evaluated the
    // index subform twice, the read and the write would land on different
    // indices (0 then 1) and `v[0]` would stay `10` — the double-eval bug
    // `Checker::place_dedup` exists to prevent.
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)) (idx -1))
                 (push v 10)
                 (push v 20)
                 (incf (get v (progn (setf idx (+ idx 1)) idx)) 10)
                 (get v 0))";
    assert_eq!(eval_ok(src), RtValue::Int(20));
}
