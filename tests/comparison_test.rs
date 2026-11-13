//! Tests for the `char`/`string` comparison operators (`< <= > >=`, overloaded
//! by receiver type like the numeric ones) and the expanded Rust-modelled
//! comparison traits — `Equal` (`equal`, ≈ `PartialEq`) and `Ord`
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
    assert!(b("(equal (Option::some 1) (Option::some 1))"));
    assert!(!b("(equal (Option::some 1) (Option::some 2))"));
    assert!(!b("(equal (Option::some 1) (Option::none))"));
    assert!(b("(equal (the Option<int> (Option::none)) (Option::none))"));
    assert!(!b(r#"(equal (Option::some "a") (Option::some "b"))"#));
    assert!(b("(equal (the Result<int,string> (Result::ok 1)) (Result::ok 1))"));
    assert!(b(r#"(equal (the Result<int,string> (Result::err "e")) (Result::err "e"))"#));
    assert!(!b(r#"(equal (the Result<int,string> (Result::ok 1)) (Result::err "e"))"#));
    assert!(b("(let ((l '(1 2))) (equal l l))"));
    assert!(b("(equal '(1 2) '(1 2))"));
    assert!(b("(equal '() '())"));
    assert!(!b("(equal '() '(1))"));
}

/// `eq`/`eql` take a value of any type, as CL's do: a struct, an enum's
/// value or a tuple is the same object or not.
#[test]
fn eq_and_eql_compare_any_type_by_identity() {
    let defs = "(defstruct pt (x int)) (defenum color (red) (rgb int))";
    for op in ["eq", "eql"] {
        assert!(!b(&format!("{defs} ({op} (make-instance 'pt :x 1) (make-instance 'pt :x 1))")));
        assert!(b(&format!("{defs} (let ((p (make-instance 'pt :x 1))) ({op} p p))")));
        assert!(!b(&format!("{defs} ({op} (color::rgb 1) (color::rgb 1))")));
        assert!(b(&format!("{defs} (let ((c (color::rgb 1))) ({op} c c))")));
        assert!(!b(&format!("({op} #{{1 2}} #{{1 2}})")));
    }
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
    let prog = "(defun in-order<T> ((a T) (b T)) bool (:where (Ord T)) (less-equal a b))";
    assert!(b(&format!("{} (in-order 1 2)", prog)));
    assert!(!b(&format!("{} (in-order 2 1)", prog)));
    assert!(b(&format!(r"{} (in-order #\a #\b)", prog)));
    assert!(!b(&format!(r#"{} (in-order "b" "a")"#, prog)));
}
