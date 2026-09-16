//! A `bignum`/`ratio` binding survives collection.
//!
//! These values were Rust-side `Rc<BigInt>`/`Rc<BigRational>` until the scalar
//! unification, which meant the collector could never touch them however long
//! they lived. They are `BoxedObj::Bignum`/`Ratio` now, so a binding holding
//! one has to be reachable from a GC root for as long as it is live — and
//! which root depends on how the binding's slot was classified:
//!
//! * a `Slot::Heap` is a `BoxedObj::Cell` the heap's own `cell_registry` walk
//!   visits on *every* collection;
//! * a `Slot::Native` only becomes a root when `Interp::sync_roots` next runs.
//!
//! So a `Native`-routed binding holding a collectible box is live only between
//! syncs, and `Interp::is_heap_repr_ty` has to classify these types `Heap` for
//! that reason rather than as an optimization. The same requirement caught
//! `random-state` when it moved to the heap (see
//! `tests/random_state_time_test.rs`).
//!
//! `gc_stress` collects on every single allocation, so a missing root fails
//! every run instead of once the free list happens to run dry.

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

fn assert_bignum(h: &Heap, v: &Value, expected: &str) {
    match v {
        typelisp::Value::Boxed(id) if h.is_bignum(*id) => {
            assert_eq!(h.bignum_value(*id).to_string(), expected)
        }
        other => panic!("expected the bignum {}, got {:?}", expected, other),
    }
}

const BIG: &str = "99999999999999999999999999999";

/// The global case: a `defvar`'s slot outlives every form that reads it, so a
/// missed root shows up as the value being gone on the *next* form rather than
/// within one.
#[test]
fn a_bignum_global_survives_constant_collection() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, &format!("(defvar (*b* int) {})", BIG))
        .expect("definition failed");
    h.set_gc_stress(true);
    for _ in 0..3 {
        let v = eval_in(&mut h, &mut chk, &mut interp, "*b*").expect("eval under gc stress failed");
        assert_bignum(&h, &v, BIG);
    }
}

#[test]
fn a_ratio_global_survives_constant_collection() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, "(defvar (*r* ratio) 3/4)").expect("definition failed");
    h.set_gc_stress(true);
    for _ in 0..3 {
        let v = eval_in(&mut h, &mut chk, &mut interp, "*r*").expect("eval under gc stress failed");
        match v {
            typelisp::Value::Boxed(id) if h.is_ratio(id) => {
                let r = h.ratio_value(id);
                assert_eq!((r.numer().to_string(), r.denom().to_string()), ("3".into(), "4".into()));
            }
            other => panic!("expected the ratio 3/4, got {:?}", other),
        }
    }
}

/// A local binding held across *collections*, which takes deliberate effort to
/// arrange: `Heap::int_from_bigint` allocates a box slot, and only `Heap::cons`
/// ever triggers a collection — so bignum arithmetic on its own, however much
/// of it, never collects even under `gc_stress`. The loop below conses on every
/// iteration so that it does.
#[test]
fn a_bignum_local_survives_collection_across_arithmetic() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(
        &mut h,
        &mut chk,
        &mut interp,
        "(defun sum-up ((n int)) int \
           (let ((acc n) (i 0)) \
             (while (< i 20) \
               (let ((junk (cons (quote a) (quote ())))) (setf acc (+ acc n))) \
               (setf i (+ i 1))) \
             acc))",
    )
    .expect("definition failed");
    h.set_gc_stress(true);
    let v = eval_in(&mut h, &mut chk, &mut interp, &format!("(sum-up {})", BIG))
        .expect("eval under gc stress failed");
    // 21 × the literal.
    assert_bignum(&h, &v, "2099999999999999999999999999979");
}

/// The global read across a collection, rather than merely across separate
/// top-level forms: the `cons` inside the loop is what actually makes the
/// collector run while `*b*` is only reachable through its `defvar` slot.
#[test]
fn a_bignum_global_survives_collection_mid_form() {
    let (mut h, mut chk, mut interp) = env();
    eval_in(&mut h, &mut chk, &mut interp, &format!("(defvar (*b* int) {})", BIG))
        .expect("definition failed");
    eval_in(
        &mut h,
        &mut chk,
        &mut interp,
        "(defun churn () int \
           (let ((i 0)) \
             (while (< i 20) \
               (let ((junk (cons (quote a) (quote ())))) (setf i (+ i 1)))) \
             *b*))",
    )
    .expect("definition failed");
    h.set_gc_stress(true);
    let v = eval_in(&mut h, &mut chk, &mut interp, "(churn)").expect("eval under gc stress failed");
    assert_bignum(&h, &v, BIG);
}
