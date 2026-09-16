//! A `string` binding survives collection.
//!
//! A `string` was a Rust-side `RtValue::Str(Rc<str>)` until the scalar
//! unification, so the collector could never touch one however long it lived.
//! It is a heap `Value::Str` now — swept like any other heap value — which
//! puts it under the same rule `random-state`/`bignum`/`ratio`/`f64` already
//! answer to: a binding holding a collectible value has to be reachable from
//! a GC root for as long as it is live, and which root that is depends on how
//! the binding's slot was classified —
//!
//! * a `Slot::Heap` is a `BoxedObj::Cell` the heap's own `cell_registry` walk
//!   visits on *every* collection;
//! * a `Slot::Native` only becomes a root when `Interp::sync_roots` next runs.
//!
//! Every loop below conses on each iteration: `Heap::alloc_string` takes a
//! string slot without ever triggering a collection — only `Heap::cons` does
//! — so string operations on their own collect exactly never, even under
//! `gc_stress` (see `tests/bignum_ratio_gc_test.rs`, where that fact made the
//! first version of these tests pass while the root was genuinely missing).

use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn env() -> (Heap, Checker, Interp) {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    (h, chk, interp)
}

fn eval_in(h: &mut Heap, chk: &mut Checker, interp: &mut Interp, src: &str) -> Result<Value, EvalError> {
    let r = Reader::new();
    let vs = r.read_all(h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(h, interp, v).expect("check failed");
        if let Some(val) = interp.exec(h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn assert_string(h: &Heap, v: &Value, expected: &str) {
    match v {
        typelisp::Value::Str(id) => assert_eq!(h.string(*id), expected),
        other => panic!("expected the string {:?}, got {:?}", expected, other),
    }
}

/// The global case: a `defvar`'s slot outlives every form that reads it.
#[test]
fn a_string_global_survives_constant_collection() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, r#"(defvar (*s* string) "hello")"#).expect("definition failed");
    h.set_gc_stress(true);
    for _ in 0..3 {
        let v = eval_in(&mut h, &mut chk, &mut interp, "*s*").expect("eval under gc stress failed");
        assert_string(&h, &v, "hello");
    }
}

/// The global read across a collection *within* one form: the `cons` in the
/// loop is what makes the collector run while `*s*` is reachable only through
/// its `defvar` slot.
#[test]
fn a_string_global_survives_collection_mid_form() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, r#"(defvar (*s* string) "hello")"#).expect("definition failed");
    eval_in(
        &mut h,
        &mut chk,
        &mut interp,
        "(defun churn () string \
           (let ((i 0)) \
             (while (< i 20) \
               (let ((junk (cons (quote a) (quote ())))) (setf i (+ i 1)))) \
             *s*))",
    )
    .expect("definition failed");
    h.set_gc_stress(true);
    let v = eval_in(&mut h, &mut chk, &mut interp, "(churn)").expect("eval under gc stress failed");
    assert_string(&h, &v, "hello");
}

/// A local binding held live across collections while the loop allocates.
#[test]
fn a_string_local_survives_collection_across_appends() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(
        &mut h,
        &mut chk,
        &mut interp,
        "(defun build ((x string)) string \
           (let ((acc x) (i 0)) \
             (while (< i 5) \
               (let ((junk (cons (quote a) (quote ())))) (setf acc (append acc x))) \
               (setf i (+ i 1))) \
             acc))",
    )
    .expect("definition failed");
    h.set_gc_stress(true);
    let v = eval_in(&mut h, &mut chk, &mut interp, r#"(build "ab")"#).expect("eval under gc stress failed");
    assert_string(&h, &v, "abababababab");
}

/// A string in a `defstruct` field: the struct's own rooting is what keeps it
/// alive.
#[test]
fn a_string_struct_field_survives_collection() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, "(defstruct named (n string) (k int))").expect("definition failed");
    eval_in(
        &mut h,
        &mut chk,
        &mut interp,
        "(defun churn ((p named)) string \
           (let ((i 0)) \
             (while (< i 20) \
               (let ((junk (cons (quote a) (quote ())))) (setf i (+ i 1)))) \
             p::n))",
    )
    .expect("definition failed");
    h.set_gc_stress(true);
    let v = eval_in(&mut h, &mut chk, &mut interp, r#"(churn (named::new "hello" 1))"#)
        .expect("eval under gc stress failed");
    assert_string(&h, &v, "hello");
}
