//! Tests for `Iter`/`VectorIter<T>` (`src/prelude.rs`) — the trait `doiter`
//! is built on, exercised here directly (without `doiter` itself) to confirm
//! `next`'s state-mutating `Self -> Option<Item>` model works before
//! `doiter` is layered on top.

extern crate typelisp;
mod common;
use common::{Load, check_last, run};
use typelisp::{Error, Value, TopLevelForm};

fn check(src: &str) -> Result<TopLevelForm, Error> {
    check_last(Load::Prelude, src)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

#[test]
fn next_on_an_empty_vector_returns_none() {
    let src = "(defun make-v () Vector<int> (Vector::new))
               (is-none (next (iter (make-v))))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn next_yields_elements_in_order_then_none() {
    let src = "(defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v)))
                 (push v 10)
                 (push v 20)
                 (let ((it (iter v)))
                   (let ((a (unwrap (next it)))
                         (b (unwrap (next it)))
                         (c (is-none (next it))))
                     (and (= a 10) (and (= b 20) c)))))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn iterator_state_is_independent_per_iter_call() {
    let src = "(defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v)))
                 (push v 1)
                 (push v 2)
                 (let ((it1 (iter v)))
                   (unwrap (next it1))
                   (let ((it2 (iter v)))
                     (unwrap (next it2)))))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn pushing_after_creating_an_iterator_is_visible_through_it() {
    // `VectorIter<T>` shares the same underlying `RtValue::Struct` as the
    // `Vector<T>` it was made from (see `vector-iter`'s doc comment in
    // `prelude.rs`), so a `push` after `(iter v)` is visible to `next`.
    let src = "(defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v)))
                 (push v 1)
                 (let ((it (iter v)))
                   (push v 2)
                   (unwrap (next it))
                   (unwrap (next it))))";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn next_on_a_type_with_no_iter_impl_is_a_type_error() {
    let src = "(defstruct box (n int)) (next (make-instance 'box :n 1))";
    assert!(check(src).is_err());
}
