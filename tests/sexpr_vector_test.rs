//! `#(..)` and `#nA(..)` as data: the `Sexpr` `vector` and `array` variants.
//!
//! The reader builds a `Vector<Option<Sexpr>>` / `Array<Option<Sexpr>>` box,
//! and `match` takes it apart with `(vector v)` / `(array a)`. Every case runs
//! interpreted and compiled: compiled code tests the variant through
//! `rt_box_kind`, and rebuilds a quoted literal from its parts
//! (`core_bridge::quoted_form`), which are separate paths from the
//! interpreter's.

mod common;
use common::{eval_string, eval_string_compiled};

/// `describe` names what it got; `rd` reads one datum from a string.
const DESCRIBE: &str = r##"
(defun describe ((d Option<Sexpr>)) string
  (match d
    ((vector v) (format false "vector ~a" (len v)))
    ((array a) (format false "array ~a/~a" (rank a) (len a)))
    ((int n) (format false "int ~a" n))
    (_ "other")))
(defun rd ((s string)) Option<Sexpr> (car (unwrap (read-from-string s))))
"##;

/// `body` evaluated after [`DESCRIBE`], interpreted and with every function
/// compiled, both expected to give `expected`.
fn both(body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", DESCRIBE, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!(
        "{}\n(defun go () string {})\n(compile describe)\n(compile rd)\n(compile go)\n(go)",
        DESCRIBE, body
    ));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

#[test]
fn a_read_vector_is_the_vector_variant() {
    both(r##"(describe (rd "#(1 a \"s\" ())"))"##, "vector 4");
}

#[test]
fn an_empty_vector_reads() {
    both(r##"(describe (rd "#()"))"##, "vector 0");
}

#[test]
fn a_read_array_is_the_array_variant() {
    both(r##"(describe (rd "#2A((1 2 3) (4 5 6))"))"##, "array 2/6");
}

/// CL's `#0A x`: no dimensions, one element.
#[test]
fn a_zero_dimensional_array_holds_one_element() {
    both(r##"(describe (rd "#0A x"))"##, "array 0/1");
}

/// A list is not a vector, and a vector is not an int: the variants are
/// told apart, not merely recognized.
#[test]
fn other_data_are_not_vectors() {
    both(r##"(format false "~a ~a" (describe (rd "(1 2)")) (describe (rd "7")))"##, "other int 7");
}

/// The elements are data, reachable through the vector's own accessors.
#[test]
fn a_vectors_elements_are_data() {
    both(
        r##"(match (rd "#(10 20)")
             ((vector v) (match (get v 1) ((int n) (format false "~a" n)) (_ "?")))
             (_ "no"))"##,
        "20",
    );
}

/// The array is an ordinary `Array`: `aref` and the dimensions work on it.
#[test]
fn an_arrays_elements_are_in_row_major_order() {
    both(
        r##"(match (rd "#2A((1 2) (3 4))")
             ((array a) (format false "~a ~a" (aref a 1 0) (dimensions a)))
             (_ "no"))"##,
        "3 #(2 2)",
    );
}

/// A quoted literal in source: compiled code cannot refer to the compiling
/// heap's box, so it rebuilds one from the parts.
#[test]
fn a_quoted_vector_and_array_in_source() {
    both(
        r##"(let ((d (the Option<Sexpr> '(#(1 #(2)) #2A((x y))))))
             (format false "~a, ~a" (describe (sexpr-car d)) (describe (sexpr-car (sexpr-cdr d)))))"##,
        "vector 2, array 2/2",
    );
}

/// An array is rectangular; ragged contents are refused when read.
#[test]
fn a_ragged_array_is_a_read_error() {
    both(
        r##"(match (read-from-string "#2A((1 2) (3))")
             ((ok d) "read")
             ((err e) (message e)))"##,
        "read: <input>:1:15: read error: #2A: the contents are not rectangular — dimension 2 is 2 in one place and 1 in another",
    );
}
