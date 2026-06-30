//! Tests for `Iter`/`VectorIter<T>` (`src/prelude.rs`) — the trait `doiter`
//! is built on, exercised here directly (without `doiter` itself) to confirm
//! `next`'s state-mutating `Self -> Option<Item>` model works before
//! `doiter` is layered on top.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue, TopLevel};

/// Checks `src` against a heap/checker/interp with the prelude already
/// loaded — needed since `Iter`/`VectorIter<T>` are themselves defined in
/// `src/prelude.rs` (`Checker::new()`/`Interp::new()` alone register only
/// the Rust-builtin types, not the prelude's `deftrait`/`impl`/`defstruct`
/// forms) — mirrors `hashtable_test.rs`'s `run_with_capacity_and_prelude`.
fn check(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v)?);
    }
    Ok(last.expect("no forms"))
}

fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
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

#[test]
fn next_on_an_empty_vector_returns_none() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (is-none (next (iter (make-v))))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn next_yields_elements_in_order_then_none() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 10)
                 (push v 20)
                 (let ((it (iter v)))
                   (let ((a (unwrap (next it)))
                         (b (unwrap (next it)))
                         (c (is-none (next it))))
                     (and (= a 10) (and (= b 20) c)))))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn iterator_state_is_independent_per_iter_call() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 1)
                 (push v 2)
                 (let ((it1 (iter v)))
                   (unwrap (next it1))
                   (let ((it2 (iter v)))
                     (unwrap (next it2)))))";
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn pushing_after_creating_an_iterator_is_visible_through_it() {
    // `VectorIter<T>` shares the same underlying `RtValue::Struct` as the
    // `Vector<T>` it was made from (see `vector-iter`'s doc comment in
    // `prelude.rs`), so a `push` after `(iter v)` is visible to `next`.
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 1)
                 (let ((it (iter v)))
                   (push v 2)
                   (unwrap (next it))
                   (unwrap (next it))))";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn next_on_a_type_with_no_iter_impl_is_a_type_error() {
    let src = "(defstruct box (n i32)) (next (box::new 1))";
    assert!(check(src).is_err());
}
