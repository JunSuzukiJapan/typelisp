//! `#(..)` and `#nA(..)` written in source: literals of `Vector<T>` and
//! `Array<T>`.
//!
//! The element type comes from the context, or from the first element when
//! the context says nothing; the contents are literals, never evaluated, as in
//! CL. Every case runs interpreted and compiled — the checker lowers a
//! literal to ordinary `Vector::new`/`push` calls, so both tiers see the same
//! program, but that is what is being checked.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

/// `body` (a `string`-valued expression) interpreted and inside a compiled
/// function, both expected to give `expected`.
fn both(body: &str, expected: &str) {
    assert_eq!(eval_string(body), expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("(defun go () string {})\n(compile go)\n(go)", body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

#[test]
fn an_unannotated_vector_takes_its_first_elements_type() {
    both(r##"(let ((v #(1 2 3))) (push v 10) (format false "~a ~a" (len v) (get v 3)))"##, "4 10");
}

#[test]
fn the_context_fixes_the_element_type() {
    both(r##"(let ((v (the Vector<i32> #(4 5)))) (format false "~a" (get v 1)))"##, "5");
    both(r##"(let ((v (the Vector<f64> #(1.5 2.5)))) (format false "~a" (get v 0)))"##, "1.5");
}

#[test]
fn strings_and_characters_are_elements_as_written() {
    both(r##"(format false "~a~a" (get #("x" "y") 1) (get #(#\a #\b) 0))"##, "ya");
}

/// As in CL, a symbol in a literal is the symbol, not a variable.
#[test]
fn a_symbol_in_a_literal_is_quoted() {
    both(r##"(let ((foo 1)) (format false "~a ~a" (get #(foo :bar) 0) (get #(foo :bar) 1)))"##, "foo :bar");
}

#[test]
fn literals_nest() {
    both(r##"(let ((v #(#(1 2) #(3)))) (format false "~a ~a" (len (get v 0)) (len (get v 1))))"##, "2 1");
}

/// Each evaluation is a fresh vector: a literal shared between calls would
/// let one caller's `push` show up in the next caller's value.
#[test]
fn every_evaluation_is_a_fresh_vector() {
    both(
        r##"(labels ((make () Vector<int> #(1)))
             (let ((a (make)))
               (push a 2)
               (format false "~a ~a" (len a) (len (make)))))"##,
        "2 1",
    );
}

/// Where S-expression data is expected, the literal is the `Sexpr` `vector`
/// variant, and its contents are data.
#[test]
fn a_literal_where_data_is_expected_is_a_data_vector() {
    both(
        r##"(match (the Option<Sexpr> #(1 x (2 3)))
             ((vector v) (match (get v 2) ((cons _ _) (format false "~a" (len v))) (_ "?")))
             (_ "not a vector"))"##,
        "3",
    );
}

#[test]
fn an_array_literal_is_row_major() {
    both(
        r##"(let ((m #2A((1 2 3) (4 5 6)))) (format false "~a ~a ~a" (aref m 1 2) (rank m) (len m)))"##,
        "6 2 6",
    );
}

/// A zero dimension leaves no element to take the type from, so the context
/// gives it — and the array is built without one.
#[test]
fn an_empty_array_literal_takes_its_type_from_the_context() {
    both(r##"(let ((z (the Array<f64> #2A(())))) (format false "~a ~a" (len z) (dimension z 0)))"##, "0 1");
}

#[test]
fn an_empty_literal_with_no_context_is_a_type_error() {
    let msg = check_err("(defun f () () (let ((v #())) ()))");
    assert!(msg.contains("`#(..)` with no elements has no element type to infer"), "{}", msg);
}

#[test]
fn mixed_element_types_are_a_type_error() {
    let msg = check_err(r##"(defun f () () (let ((v #(1 "a"))) ()))"##);
    assert!(msg.contains("string") && msg.contains("int"), "{}", msg);
}
