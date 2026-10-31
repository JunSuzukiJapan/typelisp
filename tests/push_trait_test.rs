//! The `Push` and `Pop` traits: one `(push c x)` for every library collection
//! that adds one element at a time, and one `(pop c)` for each of those that
//! has a last element to take back, so a generic function can fill or drain
//! any of them. Every case runs interpreted and compiled.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

/// `body` (a `string`-valued expression), interpreted and inside a compiled
/// function, both expected to give `expected`.
fn both(defs: &str, body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", defs, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("{}\n(defun go () string {})\n(compile go)\n(go)", defs, body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

const PUSH_ALL: &str = r##"(use std::collections::Deque std::collections::HashSet)
(defun push-all<C,T> ((c C) (xs Vector<T>)) () (:where (Push C (Item T)))
  (doiter (x (iter xs)) (push c x)))"##;

#[test]
fn a_generic_function_fills_each_library_collection() {
    both(
        PUSH_ALL,
        r##"(let ((v (the Vector<int> (Vector::new)))
                  (d (the Deque<int> (Deque::new)))
                  (s (the HashSet<int> (HashSet::new)))
                  (a (the Array<int> (Array::new (the Vector<int> #(0)) 0 :fill-pointer 0)))
                  (xs (the Vector<int> #(1 2 2 3))))
              (push-all v xs) (push-all d xs) (push-all s xs) (push-all a xs)
              (format false "~a ~a ~a ~a" v d (count s) a))"##,
        "#(1 2 2 3) #<deque 1 2 2 3> 3 #(1 2 2 3)",
    );
}

/// A direct call reaches each collection's own way of adding.
#[test]
fn push_is_a_method_of_each_library_collection() {
    both(
        PUSH_ALL,
        r##"(let ((d (the Deque<int> (Deque::new)))
                  (s (the HashSet<string> (HashSet::new)))
                  (a (the Array<int> (Array::new (the Vector<int> #(0)) 0 :fill-pointer 0))))
              (push d 1) (push-front d 0) (push d 2)
              (push s "a") (push s "a")
              (push a 5)
              (format false "~a ~a ~a" d (count s) a))"##,
        "#<deque 0 1 2> 1 #(5)",
    );
}

#[test]
fn a_user_type_implements_push() {
    both(
        &format!(
            "{}\n{}",
            PUSH_ALL,
            r##"(defstruct Counter (n int))
                (impl Push Counter
                  (:type Item string)
                  (push (self (x string)) () (setf self::n (+ self::n (length x)))))"##
        ),
        r##"(let ((c (make-instance 'Counter :n 0)))
              (push-all c (the Vector<string> #("ab" "cde")))
              (push c "f")
              (format false "~a" c::n))"##,
        "6",
    );
}

/// `Vector`'s `push` is a built-in, which a trait object's table reaches
/// through a method that calls it.
#[test]
fn a_trait_object_pushes_to_a_vector_and_a_deque() {
    both(
        PUSH_ALL,
        r##"(let ((sinks (the Vector<:dyn Push<int>> (Vector::new)))
                  (v (the Vector<int> (Vector::new)))
                  (d (the Deque<int> (Deque::new))))
              (push sinks v) (push sinks d)
              (doiter (s (iter sinks)) (push s 7))
              (format false "~a ~a" v d))"##,
        "#(7) #<deque 7>",
    );
}

/// The same path serves a built-in method taken as a function value.
#[test]
fn a_builtin_generic_method_is_a_function_value() {
    both(
        "(defun apply-to ((f (fn (Vector<int> int) ())) (v Vector<int>)) () (f v 3))",
        r##"(let ((v (the Vector<int> (Vector::new))))
              (apply-to push v)
              (format false "~a" v))"##,
        "#(3)",
    );
}

#[test]
fn a_type_without_push_is_refused_by_the_bound() {
    let msg = check_err(&format!("{}\n(defun f () () (push-all 1 (the Vector<int> #(1))))", PUSH_ALL));
    assert!(msg.contains("does not implement trait `std::iter::Push`"), "{}", msg);
}

/// `HashSet`'s `impl` asks `(:where (Hash T))`, so `HashSet<f64>` is not a
/// `Push`, and the call that asks for one is refused — not the generic
/// function's body once it is specialized.
#[test]
fn a_hashset_of_an_unhashable_type_is_not_push() {
    let msg = check_err(&format!(
        "{}\n(defun f () () (push-all (the HashSet<f64> (HashSet::new)) (the Vector<f64> #(1.0))))",
        PUSH_ALL
    ));
    assert!(msg.contains("`HashSet<f64>` does not implement trait `std::iter::Push`"), "{}", msg);
}

const POP_ALL: &str = r##"(use std::collections::Deque std::collections::HashSet)
(defun pop-all<C,T> ((c C)) Vector<T> (:where (Pop C (Item T)))
  (let ((out (the Vector<T> (Vector::new))) (more true))
    (while more
      (match (pop c)
        ((some x) (push out x))
        ((none) (progn (setf more false) ()))))
    out))"##;

/// Each takes back the element `push` added last: a `Deque` its back.
#[test]
fn a_generic_function_drains_each_library_collection() {
    both(
        POP_ALL,
        r##"(let ((v (the Vector<int> #(1 2 3)))
                  (d (the Deque<int> (Deque::new)))
                  (a (the Array<int> (Array::new (the Vector<int> #(0)) 0 :fill-pointer 0))))
              (push d 1) (push d 2) (push-front d 0)
              (push a 5) (push a 6)
              (format false "~a ~a ~a ~a" (pop-all v) (pop-all d) (pop-all a) (pop v)))"##,
        "#(3 2 1) #(2 1 0) #(6 5) none",
    );
}

#[test]
fn a_trait_object_pops_from_a_vector_and_a_deque() {
    both(
        POP_ALL,
        r##"(let ((sources (the Vector<:dyn Pop<int>> (Vector::new)))
                  (v (the Vector<int> #(7)))
                  (d (the Deque<int> (Deque::new)))
                  (out (the Vector<Option<int>> (Vector::new))))
              (push d 8)
              (push sources v) (push sources d)
              (doiter (s (iter sources)) (push out (pop s)))
              (doiter (s (iter sources)) (push out (pop s)))
              (format false "~a" out))"##,
        "#((some 7) (some 8) none none)",
    );
}

/// A `HashSet` has no last element, so it is a `Push` but not a `Pop`.
#[test]
fn a_hashset_is_not_pop() {
    let msg = check_err(&format!(
        "{}\n(defun f () () (pop-all (the HashSet<int> (HashSet::new))) ())",
        POP_ALL
    ));
    assert!(msg.contains("`HashSet<int>` does not implement trait `std::iter::Pop`"), "{}", msg);
}
