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

/// The text of a `string` result.
///
/// A `string` is a heap `Value::Str` since the scalar unification, so reading
/// one needs the heap it lives in — and `run` above drops its heap on return.
/// Hence this parallel runner, which reads the text out first.
fn eval_string(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        RtValue::Sexpr(typelisp::Value::Str(id)) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// `(format false <src-tail>)` → the produced `string`, panicking on any
/// check/eval error (for the success cases).
fn fmt(src: &str) -> String {
    eval_string(src)
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
fn decimal_on_a_non_integer_falls_back_to_aesthetic() {
    // CL's rule: a non-integer ~D argument is printed in ~A form.
    assert_eq!(fmt(r#"(format false "~d" "nope")"#), "nope");
}

// ---- padding / justification -------------------------------------------------

#[test]
fn mincol_padding_left_and_right() {
    assert_eq!(fmt(r#"(format false "[~10a]" "hi")"#), "[hi        ]");
    assert_eq!(fmt(r#"(format false "[~10@a]" "hi")"#), "[        hi]");
}

#[test]
fn decimal_width_and_pad_char() {
    assert_eq!(fmt(r#"(format false "[~5d]" 42)"#), "[   42]");
    assert_eq!(fmt(r#"(format false "[~5,'0d]" 42)"#), "[00042]");
}

#[test]
fn decimal_commas_and_sign() {
    assert_eq!(fmt(r#"(format false "~:d" 1234567)"#), "1,234,567");
    assert_eq!(fmt(r#"(format false "~@d" 42)"#), "+42");
    assert_eq!(fmt(r#"(format false "~:d" -1234)"#), "-1,234");
}

#[test]
fn justification_spreads_segments() {
    assert_eq!(fmt(r#"(format false "[~20<~a~;~a~;~a~>]" "L" "M" "R")"#), "[L         M        R]");
}

// ---- radix -------------------------------------------------------------------

#[test]
fn binary_octal_hex() {
    assert_eq!(fmt(r#"(format false "~b ~o ~x" 255 255 255)"#), "11111111 377 ff");
}

#[test]
fn radix_parameter_and_roman() {
    assert_eq!(fmt(r#"(format false "~7r" 100)"#), "202");
    assert_eq!(fmt(r#"(format false "~@r" 2024)"#), "MMXXIV");
}

#[test]
fn english_cardinal_and_ordinal() {
    assert_eq!(fmt(r#"(format false "~r" 42)"#), "forty-two");
    assert_eq!(fmt(r#"(format false "~:r" 21)"#), "twenty-first");
    assert_eq!(fmt(r#"(format false "~r" 1000000)"#), "one million");
    assert_eq!(fmt(r#"(format false "~:r" 100)"#), "one hundredth");
}

// ---- ~C, ~P ------------------------------------------------------------------

#[test]
fn character_directive_variants() {
    assert_eq!(fmt(r#"(format false "~c|~:c|~@c" #\A #\Space #\A)"#), "A|Space|#\\A");
}

#[test]
fn plural_directive() {
    assert_eq!(fmt(r#"(format false "~d cat~p" 1 1)"#), "1 cat");
    assert_eq!(fmt(r#"(format false "~d cat~p" 3 3)"#), "3 cats");
    assert_eq!(fmt(r#"(format false "~d bab~:@p" 2 2)"#), "2 babies");
}

// ---- floats ------------------------------------------------------------------

#[test]
fn fixed_and_dollar_floats() {
    assert_eq!(fmt(r#"(format false "~,2f" 3.14159)"#), "3.14");
    assert_eq!(fmt(r#"(format false "~$" 9.5)"#), "9.50");
    assert_eq!(fmt(r#"(format false "~,3f" 2)"#), "2.000");
}

// ---- case conversion ---------------------------------------------------------

#[test]
fn case_conversion_all_four() {
    assert_eq!(fmt(r#"(format false "~(HELLO World~)")"#), "hello world");
    assert_eq!(fmt(r#"(format false "~:(hello world~)")"#), "Hello World");
    assert_eq!(fmt(r#"(format false "~@(hello world~)")"#), "Hello world");
    assert_eq!(fmt(r#"(format false "~:@(hello~)")"#), "HELLO");
}

// ---- conditional -------------------------------------------------------------

#[test]
fn conditional_by_index() {
    assert_eq!(fmt(r#"(format false "~[zero~;one~;two~]" 1)"#), "one");
}

#[test]
fn conditional_default_clause() {
    assert_eq!(fmt(r#"(format false "~[a~;b~:;other~]" 9)"#), "other");
}

#[test]
fn conditional_boolean_and_at() {
    assert_eq!(fmt(r#"(format false "~:[no~;yes~]" true)"#), "yes");
    assert_eq!(fmt(r#"(format false "~:[no~;yes~]" false)"#), "no");
    assert_eq!(fmt(r#"(format false "x~@[=~d~]" 7)"#), "x=7");
    assert_eq!(fmt(r#"(format false "x~@[=~d~]" false)"#), "x");
}

// ---- iteration ---------------------------------------------------------------

#[test]
fn iteration_over_a_list() {
    assert_eq!(fmt(r#"(format false "~{[~a]~}" '(1 2 3))"#), "[1][2][3]");
}

#[test]
fn iteration_with_escape_separator() {
    assert_eq!(fmt(r#"(format false "~{~a~^, ~}" '(a b c))"#), "a, b, c");
    assert_eq!(fmt(r#"(format false "~{~a~^, ~}" '())"#), "");
}

#[test]
fn iteration_over_remaining_args() {
    assert_eq!(fmt(r#"(format false "~@{~a ~}" 1 2 3)"#), "1 2 3 ");
}

#[test]
fn nested_iteration_over_sublists() {
    assert_eq!(fmt(r#"(format false "~:{(~a ~a)~}" '((1 2) (3 4)))"#), "(1 2)(3 4)");
}

// ---- skip / indirection ------------------------------------------------------

#[test]
fn skip_directive() {
    assert_eq!(fmt(r#"(format false "~a ~* ~a" 1 2 3)"#), "1  3");
}

#[test]
fn indirection_directive() {
    assert_eq!(fmt(r#"(format false "~?" "~d-~d" '(4 5))"#), "4-5");
    assert_eq!(fmt(r#"(format false "~@?" "~d-~d" 4 5)"#), "4-5");
}

// ---- newline family ----------------------------------------------------------

#[test]
fn repeated_tilde_and_newlines() {
    assert_eq!(fmt(r#"(format false "~3~")"#), "~~~");
    assert_eq!(fmt(r#"(format false "a~2%b")"#), "a\n\nb");
}

#[test]
fn fresh_line_only_breaks_when_needed() {
    // already at beginning-of-line: ~& emits nothing
    assert_eq!(fmt(r#"(format false "~&x")"#), "x");
    assert_eq!(fmt(r#"(format false "x~&y")"#), "x\ny");
}

#[test]
fn v_parameter_reads_width_from_args() {
    assert_eq!(fmt(r#"(format false "[~v,'*d]" 6 42)"#), "[****42]");
}

#[test]
fn format_result_is_a_composable_string() {
    // format returns a real string usable by other string ops
    assert_eq!(fmt(r#"(append (format false "id=~d" 7) "!")"#), "id=7!");
}

#[test]
fn built_in_enum_values_print_their_variant_names() {
    // `Option`/`Result`/the error types are defined in the checker's registry
    // rather than by a `defenum` the interpreter exec'd, so their names used
    // to never reach the interpreter's own enum table and printed as
    // `(<unknown-variant> 1)`. `Interp::new` now seeds that table from the
    // same `registry::builtin_sum_defs` the checker registers from, so the
    // renderer's lookup finds them without a second table to fall back to.
    assert_eq!(fmt(r#"(format false "~a" (option::some 1))"#), "(some 1)");
    assert_eq!(fmt(r#"(defun no-int () Option<i32> (option::none)) (format false "~a" (no-int))"#), "none");
    assert_eq!(
        fmt(r#"(defun okv () Result<i32,ParseIntError> (result::ok 7)) (format false "~a" (okv))"#),
        "(ok 7)"
    );
}

#[test]
fn a_built_in_error_value_prints_its_type_name() {
    // The message text is the builtin's own wording; only the shape matters.
    let s = fmt(r#"(format false "~a" (parse-int "zz"))"#);
    assert!(s.starts_with("(err (parseinterror "), "got {}", s);
}

#[test]
fn variant_names_resolve_inside_a_nested_structure() {
    assert_eq!(fmt(r#"(format false "~a" (list (option::some 1) 2))"#), "((some 1) 2)");
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
