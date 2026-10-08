//! Tests for `doiter` — `(doiter (var coll) body...)`, the `Iter`-trait
//! generalization of `dolist` (`docs/dev/implementation-log.md`'s
//! "trait機構（deftrait/impl/where） + Vector<T> + doiter").

extern crate typelisp;

mod common;
use common::{Load, Session};
use typelisp::{load_prelude, load_compiler, Checker, Error, EvalError, Heap, Interp, Reader, Value, TopLevelForm};

fn check(src: &str) -> Result<TopLevelForm, Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v)?);
    }
    Ok(last.expect("no forms"))
}

fn run(src: &str) -> Result<Value, EvalError> {
    Session::new(Load::Compiler).eval(src)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

#[test]
fn doiter_sums_a_vector() {
    let src = "(defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v)) (acc 0))
                 (push v 1)
                 (push v 2)
                 (push v 3)
                 (doiter (x (iter v)) (setf acc (+ acc x)))
                 acc)";
    assert_eq!(eval_ok(src), Value::Int(6));
}

#[test]
fn doiter_over_an_empty_vector_runs_zero_times() {
    let src = "(defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v)) (acc 0))
                 (doiter (x (iter v)) (setf acc (+ acc 1)))
                 acc)";
    assert_eq!(eval_ok(src), Value::Int(0));
}

#[test]
fn break_exits_a_doiter_early() {
    let src = "(defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v)) (acc 0))
                 (push v 1)
                 (push v 2)
                 (push v 3)
                 (doiter (x (iter v))
                   (if (= x 2) (break) ())
                   (setf acc (+ acc x)))
                 acc)";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn return_with_no_value_exits_the_doiter_loop_itself() {
    // typelisp's `return` exits the *nearest enclosing loop*, not the
    // function (no CL-style `block`/`return-from` exists) — `(return)`
    // inside `doiter` must agree with the loop's own type, fixed at `Unit`
    // the same way `while`/`dolist` fix it (`Checker::check_doiter` seeds
    // `check_loop_body` with `Type::Unit`), so only a value-less `(return)`
    // type-checks here, not `(return some-i32-value)`.
    let src = "(defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v)) (acc 0))
                 (push v 1)
                 (push v 2)
                 (push v 3)
                 (doiter (x (iter v))
                   (if (= (mod x 2) 0) (return) ())
                   (setf acc (+ acc x)))
                 acc)";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn doiter_on_a_type_with_no_iter_impl_is_a_type_error() {
    let src = "(defstruct box (n int)) (doiter (x (box::new 1)) ())";
    assert!(check(src).is_err());
}

#[test]
fn doiter_works_inside_a_where_bounded_generic_function() {
    // `it: T` inside `count-iter`'s own body — no concrete `AdtDef` for `t`,
    // so `next` resolves through `Expr::TraitCall` (`Checker::check_instance_method`'s
    // type-variable branch, reached transitively through `doiter`'s `defmacro`
    // expansion), not `Expr::Assoc`. `x`'s own type is `T`'s *associated*
    // type `Item` — left as `Iter`'s own unresolved `item` template here
    // because this `where` clause has no associated-type pin, so `x` is only
    // usable as an opaque value (bound, never operated on); this is
    // intentionally a `count`, not a `sum`, to keep this test exercising the
    // *un-pinned* `(where (Iter T))` form as a regression check — see
    // `doiter_sums_inside_a_where_bounded_generic_function_with_a_pinned_item`
    // for the pinned/`sum` sibling.
    let src = "(defun count-iter<T> ((it T)) int (where (Iter T))
                 (let ((n 0)) (doiter (x it) (setf n (+ n 1))) n))
               (defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v))) (push v 10) (push v 20) (count-iter (iter v)))";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn doiter_sums_inside_a_where_bounded_generic_function_with_a_pinned_item() {
    // Sibling of `doiter_works_inside_a_where_bounded_generic_function`, now
    // pinning `Item` to `int` via `(where (Iter T (Item int)))` — `x`'s type
    // resolves to a concrete `int` (not an opaque type variable), so
    // arithmetic (`+`) on it type-checks and runs.
    let src = "(defun sum-iter<T> ((it T)) int (where (Iter T (Item int)))
                 (let ((n 0)) (doiter (x it) (setf n (+ n x))) n))
               (defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v))) (push v 10) (push v 20) (sum-iter (iter v)))";
    assert_eq!(eval_ok(src), Value::Int(30));
}

#[test]
fn call_site_rejects_a_pinned_item_that_does_not_match_the_real_associated_type() {
    // `vector-iter<bool>`'s real `Item` is `bool`, but `sum-iter`'s `where`
    // clause pins `Item` to `int` — the call site must reject this, not
    // type-check the body's `(+ n x)` against a wrong assumption.
    let src = "(defun sum-iter<T> ((it T)) int (where (Iter T (Item int)))
                 (let ((n 0)) (doiter (x it) (setf n (+ n x))) n))
               (defun make-bv () Vector<bool> (Vector::new))
               (let ((v (make-bv))) (push v true) (sum-iter (iter v)))";
    assert!(check(src).is_err());
}

#[test]
fn a_where_pin_to_a_type_variable_is_inferred_from_the_iterator_alone() {
    // The Phase-4 combinators' key move: `A` is one of the function's own
    // type parameters, pinned via `(where (Iter I (Item A)))` and appearing in
    // *no* ordinary argument — so it can only be inferred by resolving the
    // concrete iterator's real `Item`. `Checker::check_call`'s pin-inference
    // pass binds `A = int` from `vector-iter<int>`, letting `(the Vector<A> …)`
    // and the returned element type resolve. `last-of` returns the final
    // element, exercising exactly this "element type known only through the
    // iterator" path.
    let src = "(defun last-of<I,A> ((it I)) Option<A> (where (Iter I (Item A)))
                 (let ((r (the Option<A> (Option::none))))
                   (doiter (x it) (setf r (Option::some x)))
                   r))
               (defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v))) (push v 10) (push v 20) (push v 30)
                 (unwrap-or (last-of (iter v)) -1))";
    assert_eq!(eval_ok(src), Value::Int(30));
}

#[test]
fn an_argument_after_one_waiting_on_a_pin_reads_its_type_from_that_argument() {
    // `foldl`'s `(f (fn (B A) B))` waits for `A`'s pin, and `(init B)` shares
    // `B` with it, so `init` waits too: checked after the lambda, the bare
    // `(Option::none)` is read as the `Option<int>` the lambda says `B` is.
    // Checked first, it had nothing to learn its type argument from.
    let src = "(defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v))) (push v 3) (push v 9) (push v 1)
                 (unwrap-or (foldl (iter v)
                                   (lambda ((best Option<int>) (x int)) Option<int>
                                     (match best
                                       ((some b) (if (< b x) (Option::some x) best))
                                       ((none) (Option::some x))))
                                   (Option::none))
                            -1))";
    assert_eq!(eval_ok(src), Value::Int(9));
}

#[test]
fn an_argument_waiting_with_a_pinned_one_also_reads_a_generic_callers_parameter() {
    // The same inside a generic function, where `B` is the caller's own
    // `Option<T>` — the tutorial's `largest`.
    let src = "(defun largest<T> ((v Vector<T>)) Option<T>
                 (where (Ord T))
                 (foldl (iter v)
                        (lambda ((best Option<T>) (x T)) Option<T>
                          (match best
                            ((some b) (if (less b x) (Option::some x) best))
                            ((none) (Option::some x))))
                        (Option::none)))
               (defun make-v () Vector<int> (Vector::new))
               (let ((v (make-v))) (push v 3) (push v 9) (push v 1)
                 (unwrap-or (largest v) -1))";
    assert_eq!(eval_ok(src), Value::Int(9));
}

#[test]
fn generic_iter_combinators_work_over_a_hashtable() {
    // The prelude's generic `count-if`/`map`/`foldl`/… take an *iterator*, so
    // a single definition serves any `Iter` type — here `HashTable<K,V>`'s
    // `hashtable-iter<K,V>` (Item = `cons-cell<K,V>`), not just `Vector<T>`.
    let src = "(defun make-h () HashTable<int,int> (HashTable::new))
               (let ((h (make-h)))
                 (set h 1 10) (set h 2 20) (set h 3 30)
                 (count-if (iter h) (lambda ((p cons-cell<int,int>)) bool (> p::cdr 15))))";
    assert_eq!(eval_ok(src), Value::Int(2));
}

// `Sexpr` has no `Iter` impl (see `src/prelude.rs`'s comment above
// `hashtable-iter<K,V>`): each `cons` cell's `car` is independently,
// dynamically typed, so there is no single, correct `Item` for a generic
// trait to declare — plain recursion or `dolist` are the right tools for a
// `Sexpr` list.

// ---- `HashTable<K,V>` (`hashtable-iter<K,V>`, `src/prelude.rs`) -----------

#[test]
fn doiter_sums_hashtable_values() {
    let src = "(defun make-h () HashTable<int,int> (HashTable::new))
               (let ((h (make-h)) (acc 0))
                 (set h 1 10)
                 (set h 2 20)
                 (set h 3 30)
                 (doiter (p (iter h)) (setf acc (+ acc (cdr p))))
                 acc)";
    assert_eq!(eval_ok(src), Value::Int(60));
}

#[test]
fn doiter_over_an_empty_hashtable_runs_zero_times() {
    let src = "(defun make-h () HashTable<int,int> (HashTable::new))
               (let ((h (make-h)) (acc 0))
                 (doiter (p (iter h)) (setf acc (+ acc 1)))
                 acc)";
    assert_eq!(eval_ok(src), Value::Int(0));
}

#[test]
fn nested_doiter_loops_do_not_interfere() {
    let src = "(defun make-v () Vector<int> (Vector::new))
               (let ((a (make-v)) (b (make-v)) (acc 0))
                 (push a 1) (push a 2)
                 (push b 10) (push b 20)
                 (doiter (x (iter a))
                   (doiter (y (iter b))
                     (setf acc (+ acc (* x y)))))
                 acc)";
    // (1*10 + 1*20) + (2*10 + 2*20) = 30 + 60 = 90
    assert_eq!(eval_ok(src), Value::Int(90));
}
