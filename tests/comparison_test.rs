//! Tests for the `char`/`string` comparison operators (`< <= > >=`, overloaded
//! by receiver type like the numeric ones) and the expanded Rust-modelled
//! comparison traits — `Eq` (`equals`/`not-equals`, ≈ `PartialEq`) and `Ord`
//! (`less`/`less-equal`/`greater`/`greater-equal`, ≈ `PartialOrd`).
//!
//! The operators are builtin instance methods (`registry::char_assoc`/
//! `string_assoc`, evaluated in `eval/interp.rs`); the trait methods are the
//! prelude's scalar impls that delegate to them. The prelude is loaded so the
//! trait methods are available.

extern crate typelisp;
use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(v) = interp.exec(&mut h, tl)? {
            last = v;
        }
    }
    Ok(last)
}

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

// ---- expanded Eq trait (equals / not-equals) --------------------------------

#[test]
fn not_equals_negates_equality_per_type() {
    assert!(b("(not-equals 1 2)"));
    assert!(!b("(not-equals 3 3)"));
    assert!(!b(r"(not-equals #\a #\a)"));
    assert!(b(r"(not-equals #\a #\b)"));
    assert!(!b(r#"(not-equals "x" "x")"#));
    assert!(b(r#"(not-equals "x" "y")"#));
}

// ---- expanded Ord trait (less-equal / greater / greater-equal) --------------

#[test]
fn expanded_ord_methods_on_scalars() {
    assert!(b("(less-equal 3 3)"));
    assert!(b("(greater 5 2)"));
    assert!(!b("(greater-equal 2 5)"));
    assert!(b(r"(less-equal #\a #\a)"));
    assert!(b(r#"(greater "b" "a")"#));
    assert!(b(r#"(greater-equal "ab" "ab")"#));
}

// ---- generic use through an Ord bound ---------------------------------------

#[test]
fn a_generic_function_uses_the_expanded_ord_methods() {
    // `in-order` is generic over any `Ord` type and calls the new `less-equal`
    // — exercising the trait method (not the builtin operator) generically,
    // then monomorphized to int/char/string.
    let prog = "(defun in-order<T> ((a T) (b T)) bool (where (Ord T)) (less-equal a b))";
    assert!(b(&format!("{} (in-order 1 2)", prog)));
    assert!(!b(&format!("{} (in-order 2 1)", prog)));
    assert!(b(&format!(r"{} (in-order #\a #\b)", prog)));
    assert!(!b(&format!(r#"{} (in-order "b" "a")"#, prog)));
}
