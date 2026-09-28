//! Tests for the CL-style `format`/`print`/`println` directive special forms
//! (`Checker::check_format`/`check_print_like` + `Interp::run_format`, TODO
//! T1). `format` with a `false` destination *returns* the formatted string, so
//! the directive engine is exercised directly here without capturing stdout;
//! `print`/`println` share the exact same engine (only the destination and a
//! trailing newline differ), covered by the examples under `examples/`.

extern crate typelisp;

mod common;
use common::{eval_string, run};
use typelisp::{load_prelude, Checker, Heap, Interp, Reader};

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
    assert_eq!(fmt(r#"(format false "~d" 100000000000000000000)"#), "100000000000000000000");
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
    assert_eq!(fmt(r#"(defun no-int () Option<int> (option::none)) (format false "~a" (no-int))"#), "none");
    assert_eq!(
        fmt(r#"(defun okv () Result<int,ParseIntError> (result::ok 7)) (format false "~a" (okv))"#),
        "(ok 7)"
    );
}

#[test]
fn a_built_in_error_value_prints_like_an_sbcl_condition() {
    // SBCL's two answers for a condition: `~s` names the type around the
    // report, `~a` is the report alone. The message text is the builtin's
    // own wording; only the shape matters.
    let s = fmt(r#"(format false "~s" (parse-int "zz"))"#);
    assert!(s.starts_with("(err #<parseinterror \"parse-int: "), "got {}", s);
    let a = fmt(r#"(format false "~a" (parse-int "zz"))"#);
    assert!(a.starts_with("(err parse-int: "), "got {}", a);
    assert_eq!(fmt(r#"(format false "~s" (simple-error "boom"))"#), "#<simpleerror \"boom\">");
    assert_eq!(fmt(r#"(format false "~a" (simple-error "boom"))"#), "boom");
    assert_eq!(fmt(r#"(format false "~s" (wrap-error "outer" (simple-error "inner")))"#), "#<wrappederror \"outer\">");
}

#[test]
fn a_pathname_prints_as_sbcl_does() {
    assert_eq!(fmt(r#"(format false "~s" (to-pathname "/tmp/a.txt"))"#), "#P\"/tmp/a.txt\"");
    assert_eq!(fmt(r#"(format false "~a" (to-pathname "/tmp/a.txt"))"#), "/tmp/a.txt");
    assert_eq!(fmt(r#"(format false "~s" (make-pathname :name "a" :type "txt"))"#), "#P\"a.txt\"");
}

#[test]
fn a_time_prints_as_the_integer_cl_has_for_it() {
    assert_eq!(fmt(r#"(format false "~s" (universal-time::new 2 5))"#), "172805");
    assert_eq!(fmt(r#"(format false "~a" (internal-time::new 3 250))"#), "3000250");
}

#[test]
fn a_struct_prints_its_field_names() {
    // `x: 1`, not `:x 1`, which would read as a keyword argument.
    assert_eq!(fmt(r#"(defstruct point (x int) (y int)) (format false "~s" (point::new 1 2))"#), "#<point x: 1 y: 2>");
    assert_eq!(
        fmt(r#"(defstruct tag (name string)) (format false "~s ~a" (tag::new "a") (tag::new "b"))"#),
        "#<tag name: \"a\"> #<tag name: b>"
    );
    // A `Vector<T>`'s fields are its elements, which have no names.
    assert_eq!(fmt(r#"(format false "~s" (loop :for i :from 1 :to 2 :collect i))"#), "#<vector<int> 1 2>");
}

#[test]
fn a_stream_prints_as_sbcl_does() {
    let std_out = fmt(r#"(format false "~s" *standard-output*)"#);
    assert!(std_out.starts_with("#<standard-stream for \"standard output\" {"), "got {}", std_out);
    let sos = fmt(r#"(format false "~s" (make-string-output-stream))"#);
    assert!(sos.starts_with("#<string-output-stream {") && sos.ends_with("}>"), "got {}", sos);
    let two = fmt(r#"(format false "~s" (make-two-way-stream (make-string-input-stream "a") (make-string-output-stream)))"#);
    assert!(
        two.starts_with("#<two-way-stream :input-stream #<string-input-stream {") && two.contains(":output-stream #<string-output-stream {"),
        "got {}",
        two
    );
}

#[test]
fn a_decoded_time_prints_as_a_date_and_a_clock() {
    assert_eq!(
        fmt(r#"(format false "~s" (decode-universal-time (universal-time::new 45000 3723) 0))"#),
        "#<decoded-time 2023-03-17 01:02:03 +00:00 Fri>"
    );
    // CL's zone is hours *west*, without daylight saving; the offset shown is
    // the clock's own.
    assert_eq!(
        fmt(r#"(format false "~s" (decoded-time::new 5 4 3 2 1 2000 6 true 5.0))"#),
        "#<decoded-time 2000-01-02 03:04:05 -04:00 Sun dst>"
    );
}

#[test]
fn heap_info_prints_how_full_the_arena_is() {
    assert_eq!(
        fmt(r#"(format false "~s" (heap-info::new 100 25 75 1 2 3 4 true))"#),
        "#<heap-info 25 of 100 cells live (25%), 75 free, 1 symbols, 2 strings, 3 boxes, 4 collections, growable>"
    );
}

#[test]
fn a_generic_handle_prints_its_type_arguments() {
    assert_eq!(
        fmt(r#"(format false "~s" (the HashTable<string,int> (HashTable::new)))"#),
        "#<hashtable<string,int> count=0>"
    );
    let chan = fmt(r#"(format false "~s" (the Chan<string> (Chan::new 1)))"#);
    assert!(chan.starts_with("#<chan<string> "), "got {}", chan);
    let task = fmt(r#"(defun three () int 3) (format false "~s" (task (three)))"#);
    assert!(task.starts_with("#<task<int> "), "got {}", task);
}

#[test]
fn variant_names_resolve_inside_a_nested_structure() {
    // `Option<int>` is an ordinary boxed enum inside the list, so it prints
    // with its variant name. It has to be spelled out: a bare
    // `(option::some 1)` in an element position would take the element's own
    // `Option<Sexpr>` expectation instead, and *that* is the niche.
    assert_eq!(
        fmt(r#"(format false "~a" (list (the Option<int> (option::some 1)) 2))"#),
        "((some 1) 2)"
    );
    // The niche prints transparently: `Option<Sexpr>` *is* the S-expression,
    // so there is no wrapper to name. This is the visible spec change from
    // moving the empty list into `Option` (`docs/ja/reference/functions/printing.md` §1).
    assert_eq!(fmt(r#"(format false "~a" (list (option::some 1) 2))"#), "(1 2)");
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
