//! Tests for `deftype` — CL's type-specifier abbreviation, narrowed to what a
//! statically typed language can mean by it: a *spelling* for a type.
//!
//! `Checker::check_deftype` stores the body already canonical and already
//! alias-expanded (`check::registry::TypeAlias`), so a use site's expansion in
//! `Checker::canon` is one substitution and an alias cycle is unconstructible.
//! Because the rewrite happens inside the type parser, nothing downstream ever
//! sees an alias — including error messages, which is what several of these
//! tests actually assert.

extern crate typelisp;

mod common;
use common::{check_err, eval_ok};
use typelisp::Value;

// ---- the basic spelling ---------------------------------------------------

#[test]
fn an_alias_stands_for_its_body() {
    let src = "
        (deftype meters int)
        (defun double ((m meters)) meters (* m 2))
        (double 21)
    ";
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn an_alias_is_the_same_type_as_its_body() {
    // Not a new type — that is what `defstruct` is for. An `int` flows into
    // a `meters` parameter and back out with nothing to catch it.
    let src = "
        (deftype meters int)
        (defun as-meters ((n int)) meters n)
        (defun as-plain ((m meters)) int m)
        (as-plain (as-meters 7))
    ";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn an_alias_abbreviates_a_compound_type() {
    let src = "
        (deftype answer Result<int,string>)
        (defun ok-42 () answer (result::ok 42))
        (match (ok-42) ((ok n) n) ((err _) 0))
    ";
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn an_alias_abbreviates_a_function_type() {
    let src = "
        (deftype pred (fn (int) bool))
        (defun apply-to-3 ((p pred)) bool (p 3))
        (apply-to-3 (lambda ((n int)) bool (> n 2)))
    ";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

// ---- generic aliases -------------------------------------------------------

#[test]
fn a_generic_alias_substitutes_its_argument() {
    let src = "
        (deftype fallible<T> Result<T,string>)
        (defun ok-str () fallible<string> (result::ok \"hi\"))
        (match (ok-str) ((ok s) (length s)) ((err _) 0))
    ";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn a_generic_alias_takes_several_parameters() {
    let src = "
        (deftype swapped<A,B> Result<B,A>)
        (defun f () swapped<string,int> (result::ok 5))
        (match (f) ((ok n) n) ((err _) 0))
    ";
    assert_eq!(eval_ok(src), Value::Int(5));
}

#[test]
fn an_alias_may_stand_for_another_alias() {
    let src = "
        (deftype meters int)
        (deftype distance meters)
        (defun d () distance 9)
        (d)
    ";
    assert_eq!(eval_ok(src), Value::Int(9));
}

#[test]
fn a_generic_alias_may_be_used_inside_another_alias() {
    let src = "
        (deftype fallible<T> Result<T,string>)
        (deftype counter fallible<int>)
        (defun c () counter (result::ok 3))
        (match (c) ((ok n) n) ((err _) 0))
    ";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn an_alias_works_in_a_defstruct_field() {
    let src = "
        (deftype meters int)
        (defstruct segment (len meters))
        (len (segment::new 12))
    ";
    assert_eq!(eval_ok(src), Value::Int(12));
}

#[test]
fn an_alias_works_in_the_and_as_positions() {
    let src = "
        (deftype small int)
        (the small 4)
    ";
    assert_eq!(eval_ok(src), Value::Int(4));
}

// ---- what it is not --------------------------------------------------------

#[test]
fn a_mismatch_reports_the_expansion_not_the_alias() {
    // The rewrite happens inside the type parser, so by the time anything can
    // disagree the alias is gone. Saying so out loud here: this is the
    // deliberate trade, not an oversight.
    let src = "
        (deftype meters int)
        (defun f ((m meters)) meters m)
        (f \"x\")
    ";
    let msg = check_err(src);
    assert!(msg.contains("`int`"), "unexpected message: {}", msg);
    assert!(!msg.contains("meters"), "the alias should be gone by now: {}", msg);
}

#[test]
fn an_alias_may_not_mention_itself() {
    let msg = check_err("(deftype loopy Result<loopy,string>)");
    assert!(msg.contains("may not mention itself"), "unexpected message: {}", msg);
}

#[test]
fn a_generic_alias_rejects_the_wrong_argument_count() {
    let msg = check_err("(deftype fallible<T> Result<T,string>) (defun f () fallible 1)");
    assert!(msg.contains("takes 1 type argument"), "unexpected message: {}", msg);
}

#[test]
fn an_alias_and_a_type_may_not_share_a_name() {
    let msg = check_err("(deftype thing int) (defstruct thing (n int))");
    assert!(msg.contains("alias of that name already exists"), "unexpected message: {}", msg);
}

#[test]
fn a_type_and_an_alias_may_not_share_a_name() {
    let msg = check_err("(defstruct thing (n int)) (deftype thing int)");
    assert!(msg.contains("already"), "unexpected message: {}", msg);
}

#[test]
fn an_alias_and_a_trait_may_not_share_a_name() {
    let msg = check_err("(deftrait shown () (show ((self Self)) string)) (deftype shown int)");
    assert!(msg.contains("share one name space"), "unexpected message: {}", msg);
}

#[test]
fn deftype_needs_exactly_one_type_after_the_name() {
    let msg = check_err("(deftype pairish int string)");
    assert!(msg.contains("exactly one type"), "unexpected message: {}", msg);
}

// ---- modules and visibility -------------------------------------------------

#[test]
fn a_public_alias_is_reachable_from_another_module() {
    let src = "
        (module m (pub deftype meters int) (pub defun mk () meters 6))
        (defun use-it ((n m::meters)) int n)
        (use-it (m::mk))
    ";
    assert_eq!(eval_ok(src), Value::Int(6));
}

#[test]
fn a_private_alias_is_not_reachable_from_another_module() {
    let src = "
        (module m (deftype meters int) (pub defun mk () meters 6))
        (defun use-it ((n m::meters)) int n)
        (use-it (m::mk))
    ";
    // Not resolvable as an alias, and not a type either: the name is simply
    // unknown outside `m`.
    let msg = check_err(src);
    assert!(!msg.is_empty(), "expected the private alias to be out of reach");
}

#[test]
fn an_alias_is_reachable_through_use() {
    let src = "
        (module m (pub deftype meters int) (pub defun mk () meters 6))
        (use m::meters)
        (defun use-it ((n meters)) int n)
        (use-it (m::mk))
    ";
    assert_eq!(eval_ok(src), Value::Int(6));
}
