//! Tuples: the type `#{int string}`, the literal `#{1 "a"}`, `(tuple a b)`,
//! `t::0` and `(setf t::0 v)`, the pattern `#{a b}`, and `#{..}` read as
//! data — the `Sexpr` `tuple` variant.
//!
//! A tuple type is the prelude's `%internal::tupleN` struct, so most of what
//! it does is a generic struct's; what is checked here is the surface around
//! it. Every case runs interpreted and compiled.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

const DEFS: &str = r##"
(defstruct P (x int))
(defun swap ((t #{int string})) #{string int} (tuple t::1 t::0))
(defun rd ((s string)) Option<Sexpr> (car (unwrap (read-from-string s))))
"##;

/// `body` (a `string`-valued expression) after [`DEFS`], interpreted and
/// inside a compiled function, both expected to give `expected`.
fn both(body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", DEFS, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!(
        "{}\n(defun go () string {})\n(compile swap)\n(compile rd)\n(compile go)\n(go)",
        DEFS, body
    ));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

#[test]
fn tuple_builds_from_evaluated_arguments() {
    both(r##"(let ((n 2)) (format false "~s" (tuple (+ n 1) "x" 2.5)))"##, r##"#{3 "x" 2.5}"##);
}

#[test]
fn elements_are_read_by_index() {
    both(r##"(format false "~s" (swap (tuple 1 "a")))"##, r##"#{"a" 1}"##);
}

#[test]
fn an_element_can_be_set() {
    both(r##"(let ((t (tuple 1 "a"))) (setf t::0 7) (format false "~s" t))"##, r##"#{7 "a"}"##);
}

/// The literal's elements are literals, as in `#(..)`: a symbol is the
/// symbol. The context types each element by its position.
#[test]
fn a_literal_is_typed_by_its_context() {
    both(r##"(format false "~s" (the #{i32 f64} #{1 2.0}))"##, "#{1 2.0}");
    both(r##"(format false "~s" #{foo :bar 3})"##, "#{foo :bar 3}");
}

/// Each evaluation of a literal is a fresh tuple, since elements can be set.
#[test]
fn every_evaluation_is_a_fresh_tuple() {
    both(
        r##"(labels ((make () #{int int} #{1 2}))
             (let ((a (make)) (b (make))) (setf a::0 9) (format false "~a ~a" a::0 b::0)))"##,
        "9 1",
    );
}

#[test]
fn tuples_compare_and_hash_by_their_elements() {
    both(
        r##"(let ((h (the HashTable<#{int string},int> (HashTable::new))))
             (set h (tuple 1 "x") 10)
             (format false "~a ~a ~a ~a"
               (equals (tuple 1 "x") #{1 "x"})
               (less (tuple 1 "b") (tuple 1 "c"))
               (less (tuple 2 "a") (tuple 1 "z"))
               (unwrap (get h (tuple 1 "x")))))"##,
        "true true false 10",
    );
}

#[test]
fn a_tuple_pattern_binds_its_elements() {
    both(r##"(match (tuple 1 "a") (#{n s} (format false "~a/~a" n s)))"##, "1/a");
    both(r##"(match (tuple 0 "a") (#{0 _} "zero") (#{n _} (format false "~a" n)))"##, "zero");
}

/// A tuple type is written inside `<..>` like any other type.
#[test]
fn a_tuple_type_is_a_type_argument() {
    both(
        r##"(let ((ps (the Vector<#{int string}> (Vector::new))))
             (push ps (tuple 1 "a"))
             (push ps #{2 "b"})
             (format false "~s" ps))"##,
        r##"#(#{1 "a"} #{2 "b"})"##,
    );
}

/// An element type with no `print-object` of its own still prints as `#{..}`.
#[test]
fn a_tuple_of_a_type_without_print_object_prints_as_a_tuple() {
    both(r##"(format false "~a" (tuple 1 (P::new 2)))"##, "#{1 #<p x: 2>}");
}

/// Read as data, `#{..}` is the `Sexpr` `tuple` variant, whose payload is a
/// vector of the elements.
#[test]
fn a_read_tuple_is_the_tuple_variant() {
    both(
        r##"(match (rd "#{1 x \"s\"}") ((tuple v) (format false "~a ~s" (len v) (rd "#{1 x}"))) (_ "no"))"##,
        r##"3 #{1 x}"##,
    );
}

/// `#{a b}` against data matches a datum of that arity.
#[test]
fn a_tuple_pattern_against_data_matches_by_arity() {
    both(
        r##"(format false "~a ~a"
             (match (rd "#{1 2}") (#{a b} (format false "~a+~a" a b)) (_ "no"))
             (match (rd "#{1 2 3}") (#{a b} "two") (_ "not two")))"##,
        "1+2 not two",
    );
}

/// A quoted `#{..}` in compiled code is rebuilt from its elements.
#[test]
fn a_quoted_tuple() {
    both(r##"(format false "~s" '(#{1 q} x))"##, "(#{1 q} x)");
}

#[test]
fn an_index_past_the_end_is_a_type_error() {
    let msg = check_err("(defun f () int (let ((t (tuple 1 2))) t::2))");
    assert!(msg.contains("`#{int int}` has 2 element(s), numbered 0 to 1; there is no element `2`"), "{}", msg);
}

#[test]
fn the_types_must_agree() {
    let msg = check_err(r##"(defun f () #{int string} (tuple "a" 1))"##);
    assert!(msg.contains("#{int string}") || msg.contains("int"), "{}", msg);
}

#[test]
fn a_tuple_has_one_to_twelve_elements() {
    let msg = check_err("(defun f () () (let ((t (tuple 1 2 3 4 5 6 7 8 9 10 11 12 13))) ()))");
    assert!(msg.contains("tuple: takes 1 to 12 elements, got 13"), "{}", msg);
}
