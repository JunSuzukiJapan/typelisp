//! Tests for the CL-style `format`/`print`/`println` directive special forms
//! (`Checker::check_format`/`check_print_like` + `Interp::run_format`, TODO
//! T1). `format` with a `false` destination *returns* the formatted string, so
//! the directive engine is exercised directly here without capturing stdout;
//! `print`/`println` share the exact same engine (only the destination and a
//! trailing newline differ), covered by the examples under `examples/`.

extern crate typelisp;

mod common;
use common::{check_err, eval_string, run};
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
fn too_few_arguments_is_a_type_error() {
    let msg = check_err(r#"(format false "~a ~a" 1)"#);
    assert!(msg.contains("no argument left"), "got {:?}", msg);
}

/// The elements of a list argument are `Sexpr`s whose count the type does not
/// say, so running out of them is found when it happens — as an error.
#[test]
fn too_few_list_elements_is_a_recoverable_error() {
    let err = run(r#"(format false "~{~a ~a~}" '(1 2 3))"#).unwrap_err();
    assert!(format!("{:?}", err).contains("ran out of arguments"), "got {:?}", err);
}

#[test]
fn unknown_directive_is_a_type_error() {
    let msg = check_err(r#"(format false "~q" 1)"#);
    assert!(msg.contains("unknown directive"), "got {:?}", msg);
}

/// CL prints a non-integer `~D` argument as `~A`. This language checks it.
#[test]
fn decimal_on_a_non_integer_is_a_type_error() {
    let msg = check_err(r#"(format false "~d" "nope")"#);
    assert!(msg.contains("~d needs an integer, but argument 1 is `string`"), "got {:?}", msg);
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
fn conditional_boolean() {
    assert_eq!(fmt(r#"(format false "~:[no~;yes~]" true)"#), "yes");
    assert_eq!(fmt(r#"(format false "~:[no~;yes~]" false)"#), "no");
}

/// `~@[` tests for non-nil, and there is no nil to test for.
#[test]
fn conditional_at_is_rejected() {
    let msg = check_err(r#"(format false "x~@[=~d~]" 7)"#);
    assert!(msg.contains("~@[ is not supported"), "got {:?}", msg);
}

/// A prefix parameter selects the clause in place of an argument; `~#[`
/// selects by how many arguments remain.
#[test]
fn conditional_selector_from_a_parameter() {
    assert_eq!(fmt(r#"(format false "~1[a~;b~;c~]")"#), "b");
    assert_eq!(fmt(r#"(format false "~#[none~;one~;two~:;many~]" 1 2)"#), "two");
    assert_eq!(fmt(r#"(format false "~v[a~;b~;c~]" 2)"#), "c");
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

/// `~?` reads its control string at run time, where nothing can check the
/// arguments it consumes.
#[test]
fn indirection_is_rejected() {
    for src in [r#"(format false "~?" "~d-~d" '(4 5))"#, r#"(format false "~@?" "~d-~d" 4 5)"#] {
        let msg = check_err(src);
        assert!(msg.contains("~? is not supported"), "{}: got {:?}", src, msg);
    }
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
    assert_eq!(fmt(r#"(format false "~s" (loop :for i :from 1 :to 2 :collect i))"#), "#(1 2)");
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

// ---- checking arguments against the directives ---------------------------------

/// Every directive that consumes an argument checks its type, at check time.
#[test]
fn each_directive_checks_its_argument_type() {
    let cases = [
        (r#"(format false "~x" 1.5)"#, "~x needs an integer, but argument 1 is `f64`"),
        (r#"(format false "~r" "x")"#, "~r needs an integer, but argument 1 is `string`"),
        (r#"(format false "~c" 5)"#, "~c needs a char, but argument 1 is `int`"),
        (r#"(format false "~f" "s")"#, "~f needs a number, but argument 1 is `string`"),
        (r#"(format false "~$" #\a)"#, "~$ needs a number, but argument 1 is `char`"),
        (r#"(format false "~p" "s")"#, "~p needs an integer, but argument 1 is `string`"),
        (r#"(format false "~[a~;b~]" "s")"#, "~[ needs an integer, but argument 1 is `string`"),
        (r#"(format false "~:[a~;b~]" 3)"#, "~:[ needs a bool, but argument 1 is `int`"),
        (r#"(format false "~{~a~}" 5)"#, "~{ needs a list (`Option<Sexpr>`), but argument 1 is `int`"),
        (r#"(format false "~{~a~}" (cons 1 2))"#, "~{ needs a list (`Option<Sexpr>`), but argument 1 is `cons-cell<int,int>`"),
        (r#"(format false "~<~a~:>" 1)"#, "~<…~:> needs a list (`Option<Sexpr>`), but argument 1 is `int`"),
        (r#"(format false "~va" "w" 1)"#, "~a's `v` parameter needs an integer, but argument 1 is `string`"),
        (r#"(format false "~5,,,va" 1 1)"#, "~a's `v` parameter needs a char, but argument 1 is `int`"),
    ];
    for (src, want) in cases {
        let msg = check_err(src);
        assert!(msg.contains(want), "{}\n  want: {}\n  got:  {}", src, want, msg);
    }
}

/// The cursor is followed through jumps, conditionals and iteration, so the
/// argument a directive lands on is the one checked.
#[test]
fn the_argument_cursor_is_followed() {
    assert_eq!(fmt(r#"(format false "~a~:*~d" 7)"#), "77");
    assert_eq!(fmt(r#"(format false "~@{~a~^,~}" 1 2 3)"#), "1,2,3");
    assert_eq!(fmt(r#"(format false "~2@*~a~0@*~a" 1 2 3)"#), "31");
    let cases = [
        // `~*` skips the string, so `~d` lands on it only without the skip.
        (r#"(format false "~a~:*~d" "s")"#, "~d needs an integer, but argument 1 is `string`"),
        // Either clause may run, so both are checked.
        (r#"(format false "~:[~d~;~a~]" true "s")"#, "~d needs an integer, but argument 2 is `string`"),
        // Each round of `~@{` lands on the next argument.
        (r#"(format false "~@{~d~}" 1 2 "s")"#, "~d needs an integer, but argument 3 is `string`"),
        (r#"(format false "~2*~a" 1)"#, "~* moves to argument 2, outside the 1 given"),
        (r#"(format false "~:*~a" 1)"#, "~* moves to argument -1, outside the 1 given"),
        (r#"(format false "~:p" 1)"#, "~:p reuses the previous argument, and there is none"),
        (r#"(format false "~@{~a~:*~}" 1)"#, "an iteration of ~@{ can consume no argument"),
        (r#"(format false "~v*~a" 1 2)"#, "~v* moves the argument cursor by an amount known only at run time"),
    ];
    for (src, want) in cases {
        let msg = check_err(src);
        assert!(msg.contains(want), "{}\n  want: {}\n  got:  {}", src, want, msg);
    }
}

/// A `~^` that is sure to fire ends the format; the arguments it saves are not
/// asked for.
#[test]
fn an_escape_that_fires_ends_the_check_too() {
    assert_eq!(fmt(r#"(format false "x~^ ~a")"#), "x");
    assert_eq!(fmt(r#"(format false "~a~^, ~a" 1)"#), "1");
}

/// The elements of a list argument are `Sexpr`s: what a directive demands of
/// one is checked when it arrives, and a mismatch is an error, never a
/// different rendering.
#[test]
fn a_list_element_of_the_wrong_type_is_a_recoverable_error() {
    for (src, want) in [
        (r#"(format false "~{~d~}" '(1 "a"))"#, "~d requires an integer argument"),
        (r#"(format false "~{~c~}" '(1))"#, "~c requires a character argument"),
        (r#"(format false "~:{~a~}" '(1 2))"#, "is not a proper list"),
        (r#"(format false "~{~:[a~;b~]~}" '(1))"#, "~:[ requires a bool argument"),
        (r#"(format false "~{~a~:*~}" '(1 2))"#, "consumed no argument"),
    ] {
        let err = run(src).unwrap_err();
        assert!(format!("{:?}", err).contains(want), "{}\n  want: {}\n  got:  {:?}", src, want, err);
    }
}

/// A parameter, modifier or structure the engine would ignore or bend is an
/// error in the control string.
#[test]
fn a_directive_that_would_be_ignored_or_bent_is_rejected() {
    let cases = [
        (r#"(format false "~5,-1a" 1)"#, "~a, parameter 2: -1 is less than 1"),
        (r#"(format false "~-3%")"#, "~%, parameter 1: -3 is negative"),
        (r#"(format false "~40r" 1)"#, "~r, parameter 1: radix 40 is outside 2..36"),
        (r#"(format false "~1,2,3,4,5a" 1)"#, "~a takes 4 parameters, and 5 were given"),
        (r#"(format false "~5,65d" 1)"#, "~d, parameter 2: 65 is an integer; this parameter takes a character"),
        (r#"(format false "~,,3e" 1.0)"#, "~e, parameter 3: the exponent-digits parameter is not supported"),
        (r#"(format false "~5g" 1.0)"#, "~g takes 0 parameters"),
        (r#"(format false "~:%")"#, "~% does not take the `:` modifier"),
        (r#"(format false "~:a" 1)"#, "~a does not take the `:` modifier"),
        (r#"(format false "~:@*" 1)"#, "~* does not take `:` and `@` together"),
        (r#"(format false "~,5r" 1)"#, "~r with parameters needs the radix as its first one"),
        (r#"(format false "~:[a~]" true)"#, "~:[ has exactly two clauses"),
        (r#"(format false "~[a~:;b~;c~]" 1)"#, "~:; marks the default clause, which has to be the last one"),
        (r#"(format false "~<a~;b~;c~;d~:>" '(1))"#, "at most three segments"),
        (r#"(format false "~{~}" '(1))"#, "~{~} with an empty body"),
        (r#"(format false "~5/show/" 1)"#, "~/show/ takes 0 parameters"),
    ];
    for (src, want) in cases {
        let msg = check_err(src);
        assert!(msg.contains(want), "{}\n  want: {}\n  got:  {}", src, want, msg);
    }
}

/// `~colnum,colincT` follows CLHS 22.3.6.1: at or past `colnum`, it moves on
/// by the fewest (at least one) `colinc` steps reaching the cursor, and a
/// `colinc` of 0 moves nowhere. `~:T` is `pprint-tab`, which does nothing
/// when `*print-pretty*` is false.
#[test]
fn tabulate_follows_the_standard() {
    assert_eq!(fmt(r#"(format false "a~4t|")"#), "a   |");
    assert_eq!(fmt(r#"(format false "a~1,4t|")"#), "a    |");
    assert_eq!(fmt(r#"(format false "abcdef~1,4t|")"#), "abcdef   |");
    assert_eq!(fmt(r#"(format false "abcde~1,4t|")"#), "abcde|");
    assert_eq!(fmt(r#"(format false "ab~1,0t|")"#), "ab|");
    assert_eq!(fmt(r#"(format false "ab~3:t|")"#), "ab|");
}
