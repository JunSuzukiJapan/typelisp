//! Tests for `dolist` — `(dolist (var list-form [result-form]) body...)`, the
//! cons-cell (`Sexpr`) list loop. Unlike `doiter` (homogeneous `Iter`), each
//! element is a heterogeneous `Sexpr`, so the body dispatches on its shape with
//! an ordinary `match` — `dolist` keeps iteration and pattern dispatch
//! orthogonal (Pattern A), rather than fusing the `match` into the loop.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value, TopLevelForm};

// `dolist` is a pure-interpreter prelude macro (`let`/`while`/`match`/`setf`/
// `sexpr-*`), so these tests only need `load_prelude` — not `load_compiler`.
// Skipping the self-hosting compiler island keeps this binary off the shared
// `LLVMContext`, so it stays safe under `cargo test`'s default thread
// parallelism (unlike `doiter_test`/`compile_test`, which must run serially —
// see `scripts/test-serial.sh`).
fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(v) = interp.exec(&mut h, tl)? {
            last = v;
        }
    }
    Ok(last)
}

fn check(src: &str) -> Result<TopLevelForm, Error> {
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

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

#[test]
fn dolist_iterates_with_a_plain_body() {
    // Pattern A: the body need not be a `match` at all — plain iteration,
    // counting the elements of a quoted list.
    let src = "(let ((n 0))
                 (dolist (x (quote (10 20 30)))
                   (setf n (+ n 1)))
                 n)";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn dolist_dispatches_on_element_shape() {
    // The heterogeneous case: each `x` is a `Sexpr`, so the body `match`es on
    // its constructor. `100*ints + 10*syms + strs`.
    // Each arm ends in `()` so all arms agree at `Unit` (`setf` yields the
    // assigned value, not `Unit`).
    let src = r#"(let ((ints 0) (syms 0) (strs 0))
                   (dolist (x (quote (1 foo "bar" 2 baz)))
                     (match x
                       ((int _) (setf ints (+ ints 1)) ())
                       ((sym _) (setf syms (+ syms 1)) ())
                       ((str _) (setf strs (+ strs 1)) ())
                       (_ ())))
                   (+ (* ints 100) (+ (* syms 10) strs)))"#;
    // 2 ints, 2 syms, 1 str -> 221
    assert_eq!(eval_ok(src), Value::Int(221));
}

#[test]
fn dolist_sums_matched_int_bindings() {
    // `var` is `Sexpr`-typed (inferred from `sexpr-car`); the `(int n)` pattern
    // narrows it and binds `n : i64`, checked by the ordinary match machinery.
    let src = "(let ((acc (the int 0)))
                 (dolist (x (quote (1 2 3 4)))
                   (match x ((int n) (setf acc (+ acc n)) ()) (_ ())))
                 acc)";
    assert_eq!(eval_ok(src), Value::Int(10));
}

#[test]
fn dolist_over_empty_list_runs_zero_times() {
    let src = "(let ((n 0))
                 (dolist (x (quote ()))
                   (setf n (+ n 1)))
                 n)";
    assert_eq!(eval_ok(src), Value::Int(0));
}

#[test]
fn dolist_returns_its_result_form() {
    // The optional third spec element is the whole construct's value.
    let src = "(let ((acc (the int 0)))
                 (dolist (x (quote (5 7 9)) acc)
                   (match x ((int n) (setf acc (+ acc n)) ()) (_ ()))))";
    assert_eq!(eval_ok(src), Value::Int(21));
}

#[test]
fn dolist_without_result_form_is_unit() {
    let src = "(dolist (x (quote (1 2 3))) ())";
    assert_eq!(eval_ok(src), Value::Empty);
}

#[test]
fn break_exits_dolist_early() {
    // Stop accumulating once a `2` is seen (`break` exits the nearest loop).
    let src = "(let ((acc (the int 0)))
                 (dolist (x (quote (1 2 3 4)))
                   (match x
                     ((int n) (if (= n 2) (break) (progn (setf acc (+ acc n)) ())))
                     (_ ())))
                 acc)";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn dolist_stops_at_a_dotted_tail() {
    // An improper (dotted) list stops at the first non-`cons` cdr rather than
    // erroring — the `sexpr-consp` guard, not a hard `car`/`cdr` on an atom.
    let src = "(let ((n 0))
                 (dolist (x (sexpr-cons (int 1) (sexpr-cons (int 2) (int 3))))
                   (setf n (+ n 1)))
                 n)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn dolist_body_match_still_gets_exhaustiveness_checking() {
    // The design payoff of Pattern A: the body's `match` is an ordinary match,
    // so it reuses the checker's exhaustiveness analysis for free.
    let err = check("(dolist (x (quote (1 2))) (match x ((int n) n)))")
        .expect_err("non-exhaustive match in dolist body should be a type error");
    let msg = format!("{:?}", err);
    assert!(msg.contains("non-exhaustive"), "unexpected error: {}", msg);
}
