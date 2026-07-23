//! Tests for the CL-style `format`/`print`/`println` directive special forms
//! (`Checker::check_format`/`check_print_like` + `Interp::run_format`, TODO
//! T1). `format` with a `false` destination *returns* the formatted string, so
//! the directive engine is exercised directly here without capturing stdout;
//! `print`/`println` share the exact same engine (only the destination and a
//! trailing newline differ), covered by the examples under `examples/`.

extern crate typelisp;
use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, RtValue};

/// Evaluate `src` (prelude loaded, as in real programs) and return the last
/// top-level value.
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
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

/// `(format false <src-tail>)` → the produced `string`, panicking on any
/// check/eval error (for the success cases).
fn fmt(src: &str) -> String {
    match run(src).expect("eval failed") {
        RtValue::Str(s) => s.to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

#[test]
fn literal_text_passes_through_unchanged() {
    assert_eq!(fmt(r#"(format false "hello, world")"#), "hello, world");
}

#[test]
fn aesthetic_and_decimal_directives() {
    assert_eq!(fmt(r#"(format false "~a + ~a = ~d" 1 2 3)"#), "1 + 2 = 3");
}

#[test]
fn aesthetic_vs_standard_on_strings_and_chars() {
    // ~a is princ (bare), ~s is prin1 (reader syntax).
    assert_eq!(fmt(r#"(format false "~a|~s" "x" "x")"#), "x|\"x\"");
    assert_eq!(fmt(r#"(format false "~a|~s" #\z #\z)"#), "z|#\\z");
}

#[test]
fn float_bignum_ratio_and_bool_arguments() {
    assert_eq!(fmt(r#"(format false "~a" 3.5)"#), "3.5");
    // an integral float still shows a decimal point (matches print's rule)
    assert_eq!(fmt(r#"(format false "~a" 2.0)"#), "2.0");
    assert_eq!(fmt(r#"(format false "~a" true)"#), "true");
}

#[test]
fn tilde_and_newline_directives() {
    assert_eq!(fmt(r#"(format false "50~~ off~%done")"#), "50~ off\ndone");
}

#[test]
fn directive_letters_are_case_insensitive() {
    assert_eq!(fmt(r#"(format false "~A ~D" "hi" 7)"#), "hi 7");
}

#[test]
fn aesthetic_and_standard_recurse_through_lists() {
    // the princ/prin1 choice propagates into nested list elements
    assert_eq!(fmt(r#"(format false "~a" '(1 "a" foo))"#), "(1 a foo)");
    assert_eq!(fmt(r#"(format false "~s" '(1 "a" foo))"#), "(1 \"a\" foo)");
}

#[test]
fn decimal_accepts_bignum() {
    assert_eq!(fmt(r#"(format false "~d" (int->bignum 42))"#), "42");
}

#[test]
fn extra_arguments_are_ignored() {
    assert_eq!(fmt(r#"(format false "~a" 1 2 3)"#), "1");
}

#[test]
fn too_few_arguments_is_a_recoverable_error() {
    let err = run(r#"(format false "~a ~a" 1)"#).unwrap_err();
    assert!(format!("{:?}", err).contains("ran out of arguments"), "got {:?}", err);
}

#[test]
fn unknown_directive_is_an_error() {
    let err = run(r#"(format false "~q" 1)"#).unwrap_err();
    assert!(format!("{:?}", err).contains("unknown directive"), "got {:?}", err);
}

#[test]
fn decimal_on_a_non_integer_is_an_error() {
    let err = run(r#"(format false "~d" "nope")"#).unwrap_err();
    assert!(format!("{:?}", err).contains("~d"), "got {:?}", err);
}

#[test]
fn format_result_is_a_composable_string() {
    // format returns a real string usable by other string ops
    assert_eq!(fmt(r#"(append (format false "id=~d" 7) "!")"#), "id=7!");
}

#[test]
fn a_bad_arg_type_is_a_static_type_error() {
    // an Option has no Sexpr encoding, so it can't be a format argument
    let src = r#"(defun f () string (format false "~a" (some 1)))"#;
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut saw_err = false;
    for v in vs {
        if chk.check_form(&mut h, &interp, v).is_err() {
            saw_err = true;
        }
    }
    assert!(saw_err, "expected a type error for an Option format argument");
}
