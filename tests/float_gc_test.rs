//! An `f64` binding survives collection.
//!
//! A float was a Rust-side `RtValue::Float(f64)` until the scalar
//! unification, so the collector could never touch one however long it lived.
//! It is a `BoxedObj::Float` now, which puts it under the same rule
//! `random-state` and `bignum`/`ratio` already answer to: a binding holding a
//! collectible box has to be reachable from a GC root for as long as it is
//! live, and which root that is depends on how the binding's slot was
//! classified —
//!
//! * a `Slot::Heap` is a `BoxedObj::Cell` the heap's own `cell_registry` walk
//!   visits on *every* collection;
//! * a `Slot::Native` only becomes a root when `Interp::sync_roots` next runs.
//!
//! So `Interp::is_heap_repr_ty` (and its checker twin) has to classify
//! `Type::Float` `Heap` as a matter of correctness rather than of tuning.
//!
//! Every loop below conses on each iteration, because `Heap::alloc_float`
//! takes a box slot without ever triggering a collection — only `Heap::cons`
//! does. Float arithmetic on its own, however much of it, collects exactly
//! never, even under `gc_stress`, which is how the first `bignum` version of
//! these tests passed while the root was genuinely missing
//! (`tests/bignum_ratio_gc_test.rs`).

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

fn assert_float(h: &Heap, v: &Value, expected: f64) {
    match v {
        typelisp::Value::Boxed(id) if h.is_f64(*id) => {
            let got = h.f64_value(*id);
            assert!((got - expected).abs() < 1e-9, "expected {}, got {}", expected, got);
        }
        other => panic!("expected the float {}, got {:?}", expected, other),
    }
}

/// The global case: a `defvar`'s slot outlives every form that reads it, so a
/// missed root shows up as the value being gone on a *later* form.
#[test]
fn a_float_global_survives_constant_collection() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, "(defvar (*f* f64) 2.5)").expect("definition failed");
    h.set_gc_stress(true);
    for _ in 0..3 {
        let v = eval_in(&mut h, &mut chk, &mut interp, "*f*").expect("eval under gc stress failed");
        assert_float(&h, &v, 2.5);
    }
}

/// The global read across a collection *within* one form rather than merely
/// between forms: the `cons` in the loop is what makes the collector actually
/// run while `*f*` is reachable only through its `defvar` slot.
#[test]
fn a_float_global_survives_collection_mid_form() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, "(defvar (*f* f64) 2.5)").expect("definition failed");
    eval_in(
        &mut h,
        &mut chk,
        &mut interp,
        "(defun churn () f64 \
           (let ((i 0)) \
             (while (< i 20) \
               (let ((junk (cons (quote a) (quote ())))) (setf i (+ i 1)))) \
             *f*))",
    )
    .expect("definition failed");
    h.set_gc_stress(true);
    let v = eval_in(&mut h, &mut chk, &mut interp, "(churn)").expect("eval under gc stress failed");
    assert_float(&h, &v, 2.5);
}

/// A local binding held live across collections while the loop keeps
/// allocating — the accumulator has to survive every one of them.
#[test]
fn a_float_local_survives_collection_across_arithmetic() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(
        &mut h,
        &mut chk,
        &mut interp,
        "(defun sum-up ((x f64)) f64 \
           (let ((acc x) (i 0)) \
             (while (< i 20) \
               (let ((junk (cons (quote a) (quote ())))) (setf acc (+ acc x))) \
               (setf i (+ i 1))) \
             acc))",
    )
    .expect("definition failed");
    h.set_gc_stress(true);
    let v = eval_in(&mut h, &mut chk, &mut interp, "(sum-up 2.5)").expect("eval under gc stress failed");
    assert_float(&h, &v, 52.5);
}

/// A float inside a `defstruct` field: a struct field declared `f64` is
/// stored as a box (`struct_field_kind`'s float kind), so the *struct's* own
/// rooting is what has to keep it alive.
#[test]
fn a_float_struct_field_survives_collection() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, "(defstruct pt (x f64) (y f64))").expect("definition failed");
    eval_in(
        &mut h,
        &mut chk,
        &mut interp,
        "(defun churn ((p pt)) f64 \
           (let ((i 0)) \
             (while (< i 20) \
               (let ((junk (cons (quote a) (quote ())))) (setf i (+ i 1)))) \
             p::x))",
    )
    .expect("definition failed");
    h.set_gc_stress(true);
    let v = eval_in(&mut h, &mut chk, &mut interp, "(churn (pt::new 2.5 1.0))").expect("eval under gc stress failed");
    assert_float(&h, &v, 2.5);
}
