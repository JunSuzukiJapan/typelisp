//! Tests for the generic collection combinators (`map`/`filter`/`foldl`/
//! `foldr`/`reverse`/`find`/`position`/`count`/`remove-if`) exercised here
//! over `Vector<T>` via its iterator. In the type-system redesign's Phase 4
//! (`docs/dev/symbol-sexpr-redesign.md`) these replaced both the `Sexpr`-list
//! versions and the Phase-3 `vector-`-prefixed ones: each now takes an
//! *iterator* (`(map (iter v) f)`), so a single generic `defun` — bounded
//! `(where (Iter I (Item A)))` — serves `Vector<T>`, `HashTable<K,V>`, and any
//! `Iter` type, monomorphized at its call site. Results materialize into a
//! fresh `Vector`. In Phase 6.5 `vector-append` became the generic two-iterator
//! `append` (`(append (iter a) (iter b))`, two `Iter` bounds pinned to one
//! `Item`); the rest of the rebuilt sequence library is covered in
//! `seq_ops_test.rs`.

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

/// A `(1 2 3)`-valued `Vector<i32>` builder shared by most cases — Phase 3
/// intentionally ships no variadic `vector-of` builder (a typed `&rest`
/// collapses to `Sexpr` in a `defun` body, so it would need a Rust builtin),
/// so tests populate a fresh vector with explicit `push`es.
const V123: &str = "(defun make-v () Vector<i32> (Vector::new))
                    (defvar (v Vector<i32>) (make-v))
                    (push v 1) (push v 2) (push v 3)";

// ---- map --------------------------------------------------------------------

#[test]
fn map_applies_a_same_type_function_elementwise() {
    let src = format!(
        "{V123}
         (let ((out (map (iter v) (lambda ((x i32)) i32 (* x 10)))))
           (+ (+ (get out 0) (get out 1)) (get out 2)))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(60));
}

#[test]
fn map_can_change_the_element_type() {
    // `i32 -> bool`: exercises the second type parameter `U` being distinct
    // from the receiver's `T`, the reason these are `defun`s not `defmethod`s.
    let src = format!(
        "{V123}
         (let ((out (map (iter v) (lambda ((x i32)) bool (> x 1)))))
           (if (get out 0) 1 (if (get out 1) 2 (if (get out 2) 3 0))))"
    );
    // element 0 is (> 1 1) = false, element 1 is (> 2 1) = true -> 2
    assert_eq!(eval_ok(&src), RtValue::Int(2));
}

#[test]
fn map_over_an_empty_vector_yields_an_empty_vector() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (len (map (iter v) (lambda ((x i32)) i32 x))))";
    assert_eq!(eval_ok(src), RtValue::Int(0));
}

// ---- filter -----------------------------------------------------------------

#[test]
fn filter_keeps_only_matching_elements() {
    let src = format!(
        "{V123}
         (push v 4)
         (len (filter (iter v) (lambda ((x i32)) bool (= (mod x 2) 0))))"
    );
    // 2 and 4 are even -> 2 kept
    assert_eq!(eval_ok(&src), RtValue::Int(2));
}

#[test]
fn filter_that_matches_nothing_is_empty() {
    let src = format!(
        "{V123}
         (len (filter (iter v) (lambda ((x i32)) bool (> x 100))))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(0));
}

// ---- foldl / foldr ----------------------------------------------------------

#[test]
fn foldl_accumulates_left_to_right() {
    let src = format!(
        "{V123}
         (foldl (iter v) (lambda ((acc i32) (x i32)) i32 (+ acc x)) 0)"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(6));
}

#[test]
fn foldl_can_use_a_different_accumulator_type() {
    // Accumulator `A = bool`, elements `T = i32` — a genuinely two-type fold
    // ("does any element satisfy the predicate"), exercising `A` distinct
    // from `T`.
    let src = format!(
        "{V123}
         (foldl (iter v)
           (lambda ((acc bool) (x i32)) bool (or acc (= (mod x 2) 0)))
           false)"
    );
    // 2 is even -> true
    assert_eq!(eval_ok(&src), RtValue::Bool(true));
}

#[test]
fn foldr_associates_to_the_right() {
    // (1 - (2 - (3 - 0))) = 1 - (2 - 3) = 1 - (-1) = 2, distinguishing it from
    // foldl's (((0 - 1) - 2) - 3) = -6.
    let src = format!(
        "{V123}
         (foldr (iter v) (lambda ((x i32) (acc i32)) i32 (- x acc)) 0)"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(2));
}

#[test]
fn foldl_associates_to_the_left() {
    let src = format!(
        "{V123}
         (foldl (iter v) (lambda ((acc i32) (x i32)) i32 (- acc x)) 0)"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(-6));
}

// ---- reverse ----------------------------------------------------------------

#[test]
fn reverse_flips_element_order() {
    let src = format!(
        "{V123}
         (let ((out (reverse (iter v))))
           (+ (+ (* (get out 0) 100) (* (get out 1) 10)) (get out 2)))"
    );
    // reversed is (3 2 1) -> 321
    assert_eq!(eval_ok(&src), RtValue::Int(321));
}

// ---- find -------------------------------------------------------------------

#[test]
fn find_returns_the_first_match() {
    let src = format!(
        "{V123}
         (match (find (iter v) (lambda ((x i32)) bool (> x 1)))
           ((some n) n) ((none) -1))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(2));
}

#[test]
fn find_returns_none_when_no_element_matches() {
    let src = format!(
        "{V123}
         (match (find (iter v) (lambda ((x i32)) bool (> x 100)))
           ((some n) n) ((none) -1))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(-1));
}

// ---- position ---------------------------------------------------------------

#[test]
fn position_returns_the_index_of_the_first_match() {
    let src = format!(
        "{V123}
         (match (position (iter v) (lambda ((x i32)) bool (= x 3)))
           ((some i) i) ((none) -1))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(2));
}

#[test]
fn position_returns_none_when_absent() {
    let src = format!(
        "{V123}
         (match (position (iter v) (lambda ((x i32)) bool (= x 99)))
           ((some i) i) ((none) -1))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(-1));
}

// ---- count ------------------------------------------------------------------

#[test]
fn count_tallies_matching_elements() {
    let src = format!(
        "{V123}
         (push v 4) (push v 5) (push v 6)
         (count (iter v) (lambda ((x i32)) bool (= (mod x 2) 0)))"
    );
    // 2, 4, 6 are even -> 3
    assert_eq!(eval_ok(&src), RtValue::Int(3));
}

// ---- append -----------------------------------------------------------------

#[test]
fn append_concatenates_two_vectors() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((a (make-v)) (b (make-v)))
                 (push a 1) (push a 2)
                 (push b 3) (push b 4)
                 (let ((out (append (iter a) (iter b))))
                   (+ (* (len out) 1000)
                      (+ (+ (get out 0) (get out 1)) (+ (get out 2) (get out 3))))))";
    // len 4 -> 4000, sum 1+2+3+4 = 10 -> 4010
    assert_eq!(eval_ok(src), RtValue::Int(4010));
}

#[test]
fn append_does_not_mutate_its_inputs() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((a (make-v)) (b (make-v)))
                 (push a 1)
                 (push b 2)
                 (append (iter a) (iter b))
                 (+ (len a) (len b)))";
    // both inputs still length 1 -> 2
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

// ---- also works on non-i32 element types ------------------------------------

#[test]
fn ops_work_over_a_vector_of_strings() {
    let src = "(defun make-v () Vector<string> (Vector::new))
               (let ((v (make-v)))
                 (push v \"aa\")
                 (push v \"b\")
                 (push v \"ccc\")
                 (foldl (iter (map (iter v) (lambda ((s string)) i32 (length s))))
                        (lambda ((acc i32) (n i32)) i32 (+ acc n))
                        0))";
    // lengths 2 + 1 + 3 = 6
    assert_eq!(eval_ok(src), RtValue::Int(6));
}

// ---- type errors ------------------------------------------------------------

#[test]
fn map_with_a_function_of_the_wrong_argument_type_is_a_type_error() {
    let src = "(defun make-v () Vector<i32> (Vector::new))
               (let ((v (make-v)))
                 (push v 1)
                 (map (iter v) (lambda ((s string)) i32 (length s))))";
    assert!(check(src).is_err());
}
