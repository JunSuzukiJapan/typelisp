//! `match` guards, `(pat :when test body...)`, and or-patterns,
//! `(:or p1 p2 ...)`.
//!
//! A guard is a pattern node of its own (`pat-when`), which each tier tests
//! where it tests a pattern: the interpreter in `match_core_pattern`, compiled
//! code in the island's `compile-pattern-test`. An or-pattern is one arm per
//! alternative, made by the checker. Every case runs interpreted and
//! compiled.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

const DEFS: &str = r##"
(defenum Shape (circle f64) (rect f64 f64) (dot))
(defstruct P (x int))
(impl Eq P (equals ((self P) (other P)) bool (= self::x other::x)))
(defun classify ((n int)) string
  (match n
    (0 "zero")
    (k :when (< k 0) "negative")
    (k :when (equals (mod k 2) 0) "even")
    (_ "odd")))
(defun kind ((s Shape)) string
  (match s
    ((:or (circle _) (dot)) "round")
    ((rect w h) :when (= w h) "square")
    ((rect _ _) "rect")))
"##;

/// `body` (a `string`-valued expression) after [`DEFS`], interpreted and
/// inside a compiled function, both expected to give `expected`.
fn both(body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", DEFS, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!(
        "{}\n(defun go () string {})\n(compile classify)\n(compile kind)\n(compile go)\n(go)",
        DEFS, body
    ));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

/// A false guard falls through to the next arm; the first true one wins.
#[test]
fn a_guard_selects_among_arms() {
    both(
        r##"(format false "~a ~a ~a ~a" (classify 0) (classify -3) (classify 4) (classify 7))"##,
        "zero negative even odd",
    );
}

/// The guard reads what the pattern bound, including fields.
#[test]
fn a_guard_reads_the_patterns_bindings() {
    both(
        r##"(format false "~a ~a" (kind (Shape::rect 2.0 2.0)) (kind (Shape::rect 2.0 3.0)))"##,
        "square rect",
    );
}

#[test]
fn an_or_pattern_shares_one_body() {
    both(r##"(format false "~a ~a" (kind (Shape::circle 1.0)) (kind (Shape::dot)))"##, "round round");
}

/// Alternatives that bind the same name at the same type, nested inside a
/// constructor.
#[test]
fn a_nested_or_pattern_binds_in_every_alternative() {
    both(
        r##"(let ((f (lambda ((o Option<Shape>)) f64
                     (match o
                       ((some (:or (circle r) (rect r _))) r)
                       (_ 0.0)))))
             (format false "~a ~a ~a" (f (Option::some (Shape::circle 1.5)))
                                      (f (Option::some (Shape::rect 2.5 9.0)))
                                      (f (Option::some (Shape::dot)))))"##,
        "1.5 2.5 0.0",
    );
}

/// Or-patterns of literals, with a guard on the whole.
#[test]
fn an_or_pattern_of_literals_with_a_guard() {
    both(
        r##"(let ((f (lambda ((n int) (on bool)) string
                     (match n
                       ((:or 1 2 3) :when on "small")
                       (_ "other")))))
             (format false "~a ~a ~a" (f 2 true) (f 2 false) (f 5 true)))"##,
        "small other other",
    );
}

/// A guard that reads a variable from outside the `match`, in a closure:
/// the closure has to capture it, so the free-variable walk must see it.
#[test]
fn a_guard_in_a_closure_reads_a_captured_variable() {
    both(
        r##"(let* ((limit 10)
                  (f (lambda ((n int)) bool (match n (k :when (> k limit) true) (_ false)))))
             (format false "~a ~a" (f 11) (f 3)))"##,
        "true false",
    );
}

/// A closure in the body that captures a pattern variable the guard also
/// reads: the body reads it through a cell, the guard through its own.
#[test]
fn a_pattern_variable_captured_by_the_body_is_readable_in_the_guard() {
    both(
        r##"(let ((g (match 5 (k :when (> k 1) (lambda () int (* k 2))) (_ (lambda () int 0)))))
             (format false "~a" (g)))"##,
        "10",
    );
}

/// A value pattern inside `(the T ...)`: its expression reads a variable
/// from outside the `match`, which the closure must capture.
#[test]
fn a_value_pattern_under_the_in_a_closure_reads_a_captured_variable() {
    both(
        r##"(let* ((want (P::new 3))
                  (f (lambda ((d Option<Sexpr>)) bool
                       (match d ((the P (= want)) true) (_ false)))))
             (format false "~a ~a" (f (P::new 3)) (f (P::new 4))))"##,
        "true false",
    );
}

/// A guarded arm covers nothing, so a match whose only `rect` arm is
/// guarded is not exhaustive.
#[test]
fn a_guarded_arm_does_not_count_for_exhaustiveness() {
    let msg = check_err(&format!(
        "{}\n(defun f ((s Shape)) int (match s ((circle _) 1) ((dot) 2) ((rect w _) :when (> w 0.0) 3)))",
        DEFS
    ));
    assert!(msg.contains("non-exhaustive match"), "{}", msg);
}

#[test]
fn the_alternatives_must_bind_the_same_names() {
    let msg = check_err(&format!(
        "{}\n(defun f ((s Shape)) f64 (match s ((:or (circle r) (dot)) 1.0) (_ 0.0)))",
        DEFS
    ));
    assert!(msg.contains("the alternatives of `(:or ...)` must bind the same variables"), "{}", msg);
}

#[test]
fn the_alternatives_must_bind_at_the_same_types() {
    let msg = check_err(
        "(defun f ((o Option<Sexpr>)) int (match o ((:or (int n) (str n)) 1) (_ 0)))",
    );
    assert!(msg.contains("the alternatives of `(:or ...)` must bind the same variables at the same types"), "{}", msg);
}

#[test]
fn a_guard_must_be_a_bool() {
    let msg = check_err("(defun f ((n int)) int (match n (k :when k 1) (_ 0)))");
    assert!(msg.contains("bool"), "{}", msg);
}

#[test]
fn a_guard_needs_a_condition() {
    let msg = check_err("(defun f ((n int)) int (match n (k :when) (_ 0)))");
    assert!(msg.contains("`:when` needs a condition"), "{}", msg);
}

/// `if-let` is a two-armed `match`, so its pattern takes `:or` too.
#[test]
fn if_let_takes_an_or_pattern() {
    both(
        r##"(let ((f (lambda ((s Shape)) string (if-let ((:or (circle _) (dot)) s) "round" "not"))))
             (format false "~a ~a" (f (Shape::dot)) (f (Shape::rect 1.0 1.0))))"##,
        "round not",
    );
}
