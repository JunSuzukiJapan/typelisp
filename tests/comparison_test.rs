//! Tests for the `char`/`string` comparison operators (`< <= > >=`, overloaded
//! by receiver type like the numeric ones) and the expanded Rust-modelled
//! comparison traits — `Equal` (`equal`, ≈ `PartialEq`) and `Ord`
//! (`<`/`<=`/`>`/`>=`, ≈ `PartialOrd`).
//!
//! The operators are builtin instance methods, and on a scalar they are the
//! trait's methods too. The prelude is loaded so the traits are available.

extern crate typelisp;
mod common;
use common::{run, Load, Session};
use typelisp::Error;
use typelisp::Value;

fn b(src: &str) -> bool {
    match run(src).expect("eval failed") {
        Value::Bool(b) => b,
        other => panic!("expected Bool, got {:?}", other),
    }
}

// ---- char comparison operators ----------------------------------------------

#[test]
fn char_less_and_greater() {
    assert!(b(r"(< #\a #\b)"));
    assert!(!b(r"(< #\b #\a)"));
    assert!(!b(r"(< #\a #\a)"));
    assert!(b(r"(> #\b #\a)"));
    assert!(!b(r"(> #\a #\b)"));
}

#[test]
fn char_less_equal_and_greater_equal() {
    assert!(b(r"(<= #\a #\a)"));
    assert!(b(r"(<= #\a #\b)"));
    assert!(!b(r"(<= #\b #\a)"));
    assert!(b(r"(>= #\a #\a)"));
    assert!(b(r"(>= #\b #\a)"));
    assert!(!b(r"(>= #\a #\b)"));
}

// ---- string comparison operators --------------------------------------------

#[test]
fn string_less_and_greater_are_lexicographic() {
    assert!(b(r#"(< "abc" "abd")"#));
    assert!(b(r#"(< "ab" "abc")"#)); // prefix < longer
    assert!(!b(r#"(< "abd" "abc")"#));
    assert!(b(r#"(> "b" "a")"#));
    assert!(!b(r#"(> "a" "a")"#));
}

#[test]
fn string_less_equal_and_greater_equal() {
    assert!(b(r#"(<= "ab" "ab")"#));
    assert!(b(r#"(<= "ab" "ac")"#));
    assert!(!b(r#"(<= "b" "a")"#));
    assert!(b(r#"(>= "ab" "ab")"#));
    assert!(b(r#"(>= "b" "a")"#));
    assert!(!b(r#"(>= "a" "b")"#));
}

// ---- Equal trait (equal) ----------------------------------------------------

/// `/=` on a type variable bounded by `Equal` is the negation of `equal`.
#[test]
fn not_equal_on_a_type_variable_negates_equal() {
    let def = "(defun ne<T> ((a T) (b T)) bool (:where (Equal T)) (/= a b))";
    assert!(b(&format!("{def} (ne 1 2)")));
    assert!(!b(&format!("{def} (ne 3 3)")));
    assert!(!b(&format!(r"{def} (ne #\a #\a)")));
    assert!(b(&format!(r"{def} (ne #\a #\b)")));
    assert!(!b(&format!(r#"{def} (ne "x" "x")"#)));
    assert!(b(&format!(r#"{def} (ne "x" "y")"#)));
}

/// `Option<T>` and `Result<T,E>` compare as Rust's `PartialEq` does: the
/// same variant, and contents `equal` by their own type. A list is an
/// `Option<Sexpr>`, compared as CL's `equal` compares one.
#[test]
fn option_and_result_compare_their_variant_and_contents() {
    assert!(b("(equal (Option::Some 1) (Option::Some 1))"));
    assert!(!b("(equal (Option::Some 1) (Option::Some 2))"));
    assert!(!b("(equal (Option::Some 1) (Option::None))"));
    assert!(b("(equal (the Option<int> (Option::None)) (Option::None))"));
    assert!(!b(r#"(equal (Option::Some "a") (Option::Some "b"))"#));
    assert!(b("(equal (the Result<int,string> (Result::Ok 1)) (Result::Ok 1))"));
    assert!(b(r#"(equal (the Result<int,string> (Result::Err "e")) (Result::Err "e"))"#));
    assert!(!b(r#"(equal (the Result<int,string> (Result::Ok 1)) (Result::Err "e"))"#));
    assert!(b("(let ((l '(1 2))) (equal l l))"));
    assert!(b("(equal '(1 2) '(1 2))"));
    assert!(b("(equal '() '())"));
    assert!(!b("(equal '() '(1))"));
}

/// `eq`/`eql` take a value of any type, as CL's do: a struct, an enum's
/// value or a tuple is the same object or not.
#[test]
fn eq_and_eql_compare_any_type_by_identity() {
    let defs = "(defstruct pt (x int)) (defenum color (Red) (Rgb int))";
    for op in ["eq", "eql"] {
        assert!(!b(&format!("{defs} ({op} (make-instance 'pt :x 1) (make-instance 'pt :x 1))")));
        assert!(b(&format!("{defs} (let ((p (make-instance 'pt :x 1))) ({op} p p))")));
        assert!(!b(&format!("{defs} ({op} (color::Rgb 1) (color::Rgb 1))")));
        assert!(b(&format!("{defs} (let ((c (color::Rgb 1))) ({op} c c))")));
        assert!(!b(&format!("({op} #{{1 2}} #{{1 2}})")));
    }
}

// ---- generic use through an Ord bound ---------------------------------------

#[test]
fn a_generic_function_uses_the_expanded_ord_methods() {
    // `in-order` is generic over any `Ord` type and calls the trait's `<=`,
    // then monomorphized to int/char/string.
    let prog = "(defun in-order<T> ((a T) (b T)) bool (:where (Ord T)) (<= a b))";
    assert!(b(&format!("{} (in-order 1 2)", prog)));
    assert!(!b(&format!("{} (in-order 2 1)", prog)));
    assert!(b(&format!(r"{} (in-order #\a #\b)", prog)));
    assert!(!b(&format!(r#"{} (in-order "b" "a")"#, prog)));
}

// ---- a user type that writes only `<` ---------------------------------------

const MONEY: &str = "(defstruct money (yen int))
     (impl Equal money (defmethod equal (self (other Self)) bool (= self::yen other::yen)))
     (impl Ord money (defmethod < (self (other Self)) bool (< self::yen other::yen)))
     (defun yen ((n int)) money (make-instance 'money :yen n))";

#[test]
fn a_user_type_gets_the_other_three_comparisons_from_ord() {
    assert!(b(&format!("{MONEY} (> (yen 5) (yen 3))")));
    assert!(b(&format!("{MONEY} (<= (yen 3) (yen 3))")));
    assert!(!b(&format!("{MONEY} (>= (yen 2) (yen 3))")));
    let prog = "(defun in-order<T> ((a T) (b T)) bool (:where (Ord T)) (<= a b))";
    assert!(b(&format!("{MONEY} {prog} (in-order (yen 1) (yen 2))")));
}

/// The trait's methods are the operators: there is no second name for them.
#[test]
fn ord_has_no_spelled_out_method_names() {
    for name in ["less", "less-equal", "greater", "greater-equal"] {
        match Session::new(Load::Prelude).check_all(&format!("({name} 1 2)")) {
            Err(Error::At(_, e)) if matches!(&*e, Error::NoSuchFunction(n) if n == name) => {}
            Err(Error::NoSuchFunction(n)) if n == name => {}
            other => panic!("`{name}`: expected NoSuchFunction, got {:?}", other),
        }
    }
}
