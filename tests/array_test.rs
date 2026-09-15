//! Tests for `Array<T>`, the multi-dimensional array — cl-parity-plan.md
//! Phase 6b.
//!
//! `Array<T>` is a prelude `defstruct` over two `Vector`s (dimensions and
//! flat row-major storage) rather than a built-in type, so most of what is
//! exercised here is ordinary typelisp. The one piece that is not is
//! `(aref a i j)`: variadic bare subscripts, which a `defmethod` cannot
//! express, so the checker rewrites them into the `Vector<i32>`-subscript
//! `get`/`set` these tests also call directly.

extern crate typelisp;

mod common;
use common::{check_err, eval_err, eval_ok};
use typelisp::Value;

/// `(dims a b ...)` — a `Vector<i32>` written inline, since a subscript or a
/// dimension list is one and there is no vector literal.
const DIMS: &str = r#"
(defun dims2 ((a i32) (b i32)) Vector<i32>
  (let ((v (the Vector<i32> (Vector::new))))
    (progn (push v a) (push v b) v)))
(defun dims1 ((a i32)) Vector<i32>
  (let ((v (the Vector<i32> (Vector::new))))
    (progn (push v a) v)))
"#;

fn with_dims(src: &str) -> String {
    format!("{}\n{}", DIMS, src)
}

// ---- shape ------------------------------------------------------------------

#[test]
fn make_builds_an_array_of_the_requested_shape() {
    assert_eq!(eval_ok(&with_dims("(rank (Array::make (dims2 2 3) 0))")), Value::Int(2));
    assert_eq!(eval_ok(&with_dims("(total-size (Array::make (dims2 2 3) 0))")), Value::Int(6));
    assert_eq!(eval_ok(&with_dims("(dimension (Array::make (dims2 2 3) 0) 1)")), Value::Int(3));
}

#[test]
fn every_cell_starts_at_the_initial_element() {
    assert_eq!(
        eval_ok(&with_dims("(foldl (iter (Array::make (dims2 2 3) 7)) (lambda ((a i32) (b i32)) i32 (+ a b)) 0)")),
        Value::Int(42)
    );
}

#[test]
fn dimensions_hands_back_a_copy() {
    // Mutating what `dimensions` returned must not reshape the array.
    assert_eq!(
        eval_ok(&with_dims(
            "(let ((a (Array::make (dims2 2 3) 0)))
               (progn (set (dimensions a) 0 99) (dimension a 0)))"
        )),
        Value::Int(2)
    );
}

#[test]
fn make_copies_the_dimension_vector_it_was_given() {
    assert_eq!(
        eval_ok(&with_dims(
            "(let* ((d (dims2 2 3)) (a (Array::make d 0)))
               (progn (set d 0 99) (dimension a 0)))"
        )),
        Value::Int(2)
    );
}

#[test]
fn a_zero_dimensional_array_holds_exactly_one_cell() {
    // CL's rank-0 array: no subscripts, one element, total size 1.
    assert_eq!(
        eval_ok("(total-size (Array::make (the Vector<i32> (Vector::new)) 5))"),
        Value::Int(1)
    );
}

#[test]
fn a_negative_dimension_is_refused() {
    assert!(eval_err(&with_dims("(Array::make (dims2 2 -1) 0)")).contains("negative"));
}

// ---- row-major order --------------------------------------------------------

#[test]
fn row_major_index_runs_the_last_subscript_fastest() {
    let src = "(let ((a (Array::make (dims2 2 3) 0)))
                 (row-major-index a (dims2 1 2)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(5));
}

#[test]
fn get_and_set_address_the_same_cell_as_row_major_access() {
    let src = "(let ((a (Array::make (dims2 2 3) 0)))
                 (progn (set a (dims2 1 2) 9) (row-major-get a 5)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(9));
}

#[test]
fn row_major_set_is_visible_through_subscripts() {
    let src = "(let ((a (Array::make (dims2 2 3) 0)))
                 (progn (row-major-set a 4 8) (get a (dims2 1 1))))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(8));
}

#[test]
fn iteration_walks_row_major_order() {
    let src = "(let ((a (Array::make (dims2 2 2) 0)) (out (the Vector<i32> (Vector::new))))
                 (progn
                   (set a (dims2 0 0) 1) (set a (dims2 0 1) 2)
                   (set a (dims2 1 0) 3) (set a (dims2 1 1) 4)
                   (doiter (x (iter a)) (push out x))
                   (foldl (iter out) (lambda ((acc i32) (x i32)) i32 (+ (* acc 10) x)) 0)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(1234));
}

// ---- bounds -----------------------------------------------------------------

#[test]
fn in_bounds_answers_for_valid_and_invalid_subscripts() {
    let mk = |sub: &str| {
        format!("(let ((a (Array::make (dims2 2 3) 0))) (in-bounds a {}))", sub)
    };
    assert_eq!(eval_ok(&with_dims(&mk("(dims2 1 2)"))), Value::Bool(true));
    assert_eq!(eval_ok(&with_dims(&mk("(dims2 2 0)"))), Value::Bool(false));
    assert_eq!(eval_ok(&with_dims(&mk("(dims2 0 -1)"))), Value::Bool(false));
    // The wrong *number* of subscripts is a false answer, not an error — CL
    // says `array-in-bounds-p` may be asked anything.
    assert_eq!(eval_ok(&with_dims(&mk("(dims1 0)"))), Value::Bool(false));
}

#[test]
fn an_out_of_range_subscript_is_an_error_rather_than_a_wrong_cell() {
    // Without the check, `(0 5)` on a 2x3 would fold to offset 5 — a real
    // cell, in the wrong row.
    let src = "(let ((a (Array::make (dims2 2 3) 0))) (get a (dims2 0 5)))";
    assert!(eval_err(&with_dims(src)).contains("out of range"));
}

#[test]
fn the_wrong_number_of_subscripts_is_an_error() {
    let src = "(let ((a (Array::make (dims2 2 3) 0))) (get a (dims1 1)))";
    assert!(eval_err(&with_dims(src)).contains("out of range"));
}

// ---- fill pointer -----------------------------------------------------------

#[test]
fn a_fill_pointer_array_starts_at_its_fill_pointer_length() {
    // The fill pointer is the *length*; the reserved cells are the size.
    let a = "(Array::make (dims1 4) 0 :fill-pointer 0)";
    assert_eq!(eval_ok(&with_dims(&format!("(len {})", a))), Value::Int(0));
    assert_eq!(eval_ok(&with_dims(&format!("(total-size {})", a))), Value::Int(4));
}

#[test]
fn push_extend_fills_the_reserved_cells_before_growing() {
    let src = |what: &str| {
        format!(
            "(let ((a (Array::make (dims1 2) 0 :fill-pointer 0)))
               (progn (push-extend a 1) (push-extend a 2) ({} a)))",
            what
        )
    };
    assert_eq!(eval_ok(&with_dims(&src("len"))), Value::Int(2));
    assert_eq!(eval_ok(&with_dims(&src("total-size"))), Value::Int(2));
}

#[test]
fn push_extend_grows_the_single_dimension_past_the_reserved_cells() {
    let src = "(let ((a (Array::make (dims1 1) 0 :fill-pointer 0)))
                 (progn (push-extend a 1) (push-extend a 2)
                        (dimension a 0)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(2));
}

#[test]
fn pop_returns_the_last_pushed_element_and_none_when_empty() {
    let src = "(let ((a (Array::make (dims1 2) 0 :fill-pointer 0)))
                 (progn (push-extend a 7) (unwrap (pop a))))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(7));
    let empty = "(let ((a (Array::make (dims1 2) 0 :fill-pointer 0)))
                   (is-none (pop a)))";
    assert_eq!(eval_ok(&with_dims(empty)), Value::Bool(true));
}

#[test]
fn iteration_stops_at_the_fill_pointer() {
    let src = "(let ((a (Array::make (dims1 4) 9 :fill-pointer 0)) (n 0))
                 (progn (push-extend a 1) (doiter (x (iter a)) (setf n (+ n 1))) n))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(1));
}

#[test]
fn push_extend_needs_a_fill_pointer() {
    let src = "(let ((a (Array::make (dims1 2) 0))) (push-extend a 1))";
    assert!(eval_err(&with_dims(src)).contains("no fill pointer"));
}

#[test]
fn a_fill_pointer_needs_a_one_dimensional_array() {
    let src = "(Array::make (dims2 2 2) 0 :fill-pointer 0)";
    assert!(eval_err(&with_dims(src)).contains("one-dimensional"));
}

#[test]
fn a_fill_pointer_past_the_end_is_refused() {
    let src = "(Array::make (dims1 2) 0 :fill-pointer 3)";
    assert!(eval_err(&with_dims(src)).contains("fill-pointer"));
}

// ---- adjust -----------------------------------------------------------------

#[test]
fn adjust_keeps_every_element_still_in_range() {
    let src = "(let ((a (Array::make (dims2 2 2) 0)))
                 (progn
                   (set a (dims2 0 0) 1) (set a (dims2 0 1) 2)
                   (set a (dims2 1 0) 3) (set a (dims2 1 1) 4)
                   (adjust a (dims2 3 3) 0)
                   (+ (* 1000 (get a (dims2 0 0))) (* 100 (get a (dims2 0 1)))
                      (* 10 (get a (dims2 1 0))) (get a (dims2 1 1)))))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(1234));
}

#[test]
fn adjust_fills_the_new_cells_with_the_initial_element() {
    let src = "(let ((a (Array::make (dims2 1 1) 1)))
                 (progn (adjust a (dims2 2 2) 5) (get a (dims2 1 1))))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(5));
}

#[test]
fn adjust_drops_the_elements_that_no_longer_fit() {
    let src = "(let ((a (Array::make (dims2 2 2) 0)))
                 (progn (set a (dims2 1 1) 4) (adjust a (dims2 1 1) 0) (total-size a)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(1));
}

#[test]
fn adjust_keeps_the_rank() {
    let src = "(let ((a (Array::make (dims2 2 2) 0))) (adjust a (dims1 4) 0))";
    assert!(eval_err(&with_dims(src)).contains("dimension"));
}

#[test]
fn adjust_clamps_a_fill_pointer_that_no_longer_fits() {
    let src = "(let ((a (Array::make (dims1 4) 0 :fill-pointer 0)))
                 (progn (push-extend a 1) (push-extend a 2) (push-extend a 3)
                        (adjust a (dims1 2) 0)
                        (len a)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(2));
}

// ---- typing -----------------------------------------------------------------

#[test]
fn the_element_type_is_the_arrays_type_argument() {
    let src = "(let ((a (Array::make (dims1 2) \"x\"))) (equal (get a (dims1 0)) \"x\"))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Bool(true));
}

#[test]
fn an_element_of_the_wrong_type_is_a_type_error() {
    let src = "(let ((a (Array::make (dims1 2) 0))) (set a (dims1 0) \"x\"))";
    let msg = check_err(&with_dims(src));
    assert!(msg.contains("Str") || msg.contains("string"), "{}", msg);
}

// ---- `aref` sugar -----------------------------------------------------------

#[test]
fn aref_reads_the_cell_its_subscripts_name() {
    let src = "(let ((a (Array::make (dims2 2 3) 0)))
                 (progn (set a (dims2 1 2) 9) (aref a 1 2)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(9));
}

#[test]
fn setf_aref_writes_the_cell_its_subscripts_name() {
    let src = "(let ((a (Array::make (dims2 2 3) 0)))
                 (progn (setf (aref a 1 2) 9) (row-major-get a 5)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(9));
}

#[test]
fn aref_carries_the_range_check_of_the_method_it_expands_to() {
    // `(0 5)` on a 2x3 folds to offset 5, a real cell in the wrong row.
    let src = "(let ((a (Array::make (dims2 2 3) 0))) (aref a 0 5))";
    assert!(eval_err(&with_dims(src)).contains("out of range"));
}

#[test]
fn aref_rejects_the_wrong_number_of_subscripts() {
    let src = "(let ((a (Array::make (dims2 2 3) 0))) (aref a 1))";
    assert!(eval_err(&with_dims(src)).contains("out of range"));
}

#[test]
fn aref_works_at_every_rank_including_zero() {
    let one = "(let ((a (Array::make (dims1 3) 0))) (progn (setf (aref a 2) 7) (aref a 2)))";
    assert_eq!(eval_ok(&with_dims(one)), Value::Int(7));
    let none = "(let ((a (Array::make (the Vector<i32> (Vector::new)) 0)))
                  (progn (setf (aref a) 7) (aref a)))";
    assert_eq!(eval_ok(&with_dims(none)), Value::Int(7));
}

#[test]
fn aref_evaluates_the_array_before_its_subscripts() {
    // The rewrite reads the array last (at the `get`), so it must be bound
    // first for `(aref (f) (g))` to run `f` before `g` as written.
    let src = "(let ((log (the Vector<i32> (Vector::new))))
                 (labels ((arr () array<i32> (progn (push log 1) (Array::make (dims1 2) 0)))
                          (sub () i32 (progn (push log 2) 0)))
                   (progn (aref (arr) (sub))
                          (foldl (iter log) (lambda ((acc i32) (x i32)) i32 (+ (* acc 10) x)) 0))))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(12));
}

#[test]
fn aref_evaluates_each_subscript_exactly_once() {
    let src = "(let ((n 0) (a (Array::make (dims2 2 2) 0)))
                 (labels ((bump () i32 (progn (setf n (+ n 1)) 0)))
                   (progn (aref a (bump) (bump)) n)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(2));
}

#[test]
fn nested_aref_forms_do_not_share_an_index_vector() {
    // Each expansion mints its own temporaries, so the inner `aref`'s
    // subscript vector cannot leak into the outer one's.
    let src = "(let ((a (Array::make (dims2 2 2) 0)) (b (Array::make (dims1 2) 0)))
                 (progn (setf (aref b 1) 1)
                        (setf (aref a 1 1) 5)
                        (aref a (aref b 1) 1)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(5));
}

#[test]
fn aref_on_a_vector_is_a_type_error_rather_than_a_second_meaning() {
    // A `Vector<T>` is indexed with `get`/`set`; `aref` always builds the
    // `Vector<i32>` subscript an `Array<T>` takes.
    let msg = check_err("(let ((v (the Vector<i32> (Vector::new)))) (aref v 0))");
    assert!(msg.contains("is not an `Array<T>`"), "{}", msg);
    assert!(msg.contains("vector<i32>"), "{}", msg);
}

// ---- integration with the rest of the language ------------------------------

#[test]
fn the_fill_pointer_is_a_settable_field() {
    // `fill-pointer` is a `pub` slot, so CL's `(setf (fill-pointer v) n)` is
    // this language's ordinary field place.
    let src = "(let ((a (Array::make (dims1 4) 0 :fill-pointer 0)))
                 (progn (setf a::fill-pointer (Option::some 3)) (len a)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(3));
}

#[test]
fn an_array_without_a_fill_pointer_answers_none() {
    let src = "(is-none (fill-pointer (Array::make (dims1 4) 0)))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Bool(true));
}

#[test]
fn the_iter_combinators_work_on_an_array() {
    // `Iter` is the whole integration surface: implementing it once gives
    // `map`/`filter`/`foldl` and every other combinator in §6.
    let src = "(let ((a (Array::make (dims2 2 2) 3)))
                 (foldl (iter (map (iter a) (lambda ((x i32)) i32 (* x x))))
                        (lambda ((acc i32) (x i32)) i32 (+ acc x)) 0))";
    assert_eq!(eval_ok(&with_dims(src)), Value::Int(36));
}
