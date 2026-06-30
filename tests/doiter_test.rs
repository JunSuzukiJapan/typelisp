//! Tests for `doiter` — `(doiter (var coll) body...)`, the `Iter`-trait
//! generalization of `dolist` (`docs/TODO.md`'s `doiter` entry).

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, RtValue, TopLevel};

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
fn doiter_sums_a_vector() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)) (acc 0))
                 (push v 1)
                 (push v 2)
                 (push v 3)
                 (doiter (x (iter v)) (setf acc (+ acc x)))
                 acc)";
    assert_eq!(eval_ok(src), RtValue::Int(6));
}

#[test]
fn doiter_over_an_empty_vector_runs_zero_times() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)) (acc 0))
                 (doiter (x (iter v)) (setf acc (+ acc 1)))
                 acc)";
    assert_eq!(eval_ok(src), RtValue::Int(0));
}

#[test]
fn break_exits_a_doiter_early() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)) (acc 0))
                 (push v 1)
                 (push v 2)
                 (push v 3)
                 (doiter (x (iter v))
                   (if (= x 2) (break) ())
                   (setf acc (+ acc x)))
                 acc)";
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn return_with_no_value_exits_the_doiter_loop_itself() {
    // typelisp's `return` exits the *nearest enclosing loop*, not the
    // function (no CL-style `block`/`return-from` exists) — `(return)`
    // inside `doiter` must agree with the loop's own type, fixed at `Unit`
    // the same way `while`/`dolist` fix it (`Checker::check_doiter` seeds
    // `check_loop_body` with `Type::Unit`), so only a value-less `(return)`
    // type-checks here, not `(return some-i32-value)`.
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)) (acc 0))
                 (push v 1)
                 (push v 2)
                 (push v 3)
                 (doiter (x (iter v))
                   (if (= (mod x 2) 0) (return) ())
                   (setf acc (+ acc x)))
                 acc)";
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn doiter_on_a_type_with_no_iter_impl_is_a_type_error() {
    let src = "(defstruct box (n i32)) (doiter (x (box::new 1)) ())";
    assert!(check(src).is_err());
}

#[test]
fn doiter_works_inside_a_where_bounded_generic_function() {
    // `it: T` inside `count-iter`'s own body — no concrete `AdtDef` for `t`,
    // so `next` resolves through `Expr::TraitCall` (`Checker::check_doiter`'s
    // type-variable branch), not `Expr::Assoc`. `x`'s own type is `T`'s
    // *associated* type `Item` — itself an unresolved type variable inside a
    // generic body with no concrete `AdtDef` (`Checker::check_doiter` leaves
    // it as `Iter`'s own `item` template, see its doc comment) — so `x` is
    // only usable here as an opaque value (bound, never operated on); this
    // is intentionally a `count`, not a `sum`, since arithmetic on `Item`
    // would need a `where` clause expressive enough to pin `Item` to a
    // concrete type, which is out of scope for this `where`-bounds MVP.
    let src = "(defun (count-iter T) ((it T)) i32 (where (Iter T))
                 (let ((n 0)) (doiter (x it) (setf n (+ n 1))) n))
               (defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v))) (push v 10) (push v 20) (count-iter (iter v)))";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn nested_doiter_loops_do_not_interfere() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((a (make-v)) (b (make-v)) (acc 0))
                 (push a 1) (push a 2)
                 (push b 10) (push b 20)
                 (doiter (x (iter a))
                   (doiter (y (iter b))
                     (setf acc (+ acc (* x y)))))
                 acc)";
    // (1*10 + 1*20) + (2*10 + 2*20) = 30 + 60 = 90
    assert_eq!(eval_ok(src), RtValue::Int(90));
}
