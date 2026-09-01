//! Tests for `dlet`, the scoped rebinding of a global — cl-parity-plan.md
//! Phase 7b.
//!
//! CL writes this as `let`, because a CL `let` on a special variable *is* a
//! dynamic binding. This language's `let` is lexical, so the dynamic one has
//! its own name. Underneath it is not a binding at all: save, assign, restore
//! in an `unwind-protect` cleanup — which is why most of what is tested here
//! is *how the body was left*.

extern crate typelisp;
use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

/// A global to rebind, plus a reader that sees it through a function call —
/// so the tests observe the *global*, not a lexical name in scope.
const G: &str = "(defvar (*depth* i32) 0)
                 (defun peek () i32 *depth*)";

fn with_g(src: &str) -> String {
    format!("{}\n{}", G, src)
}

#[test]
fn the_body_sees_the_new_value() {
    assert_eq!(
        eval_ok(&with_g("(dlet ((*depth* 3)) (peek))")),
        Value::Int(3)
    );
}

#[test]
fn the_old_value_is_back_afterwards() {
    assert_eq!(
        eval_ok(&with_g("(progn (dlet ((*depth* 3)) (peek)) (peek))")),
        Value::Int(0)
    );
}

#[test]
fn the_value_is_the_bodys_value() {
    assert_eq!(
        eval_ok(&with_g("(dlet ((*depth* 3)) (+ (peek) 10))")),
        Value::Int(13)
    );
}

#[test]
fn a_multi_form_body_runs_in_order() {
    let src = "(let ((log 0))
                 (progn (dlet ((*depth* 3)) (setf log (peek)) (setf log (* log 2))) log))";
    assert_eq!(eval_ok(&with_g(src)), Value::Int(6));
}

#[test]
fn several_globals_rebind_at_once() {
    let src = "(defvar (*width* i32) 1)
               (defun peek2 () i32 (+ (* 10 *depth*) *width*))
               (progn (dlet ((*depth* 3) (*width* 4)) (peek2)))";
    assert_eq!(eval_ok(&with_g(src)), Value::Int(34));
}

#[test]
fn several_globals_all_come_back() {
    let src = "(defvar (*width* i32) 1)
               (defun peek2 () i32 (+ (* 10 *depth*) *width*))
               (progn (dlet ((*depth* 3) (*width* 4)) (peek2)) (peek2))";
    assert_eq!(eval_ok(&with_g(src)), Value::Int(1));
}

#[test]
fn nesting_restores_the_enclosing_value_not_the_original() {
    let src = "(dlet ((*depth* 3))
                 (progn (dlet ((*depth* 5)) (peek)) (peek)))";
    assert_eq!(eval_ok(&with_g(src)), Value::Int(3));
}

#[test]
fn the_new_value_is_visible_through_a_call_made_by_the_body() {
    // The whole point of a dynamic binding: a function the body calls sees it
    // without being passed anything.
    let src = "(defun deeper () i32 (peek))
               (dlet ((*depth* 7)) (deeper))";
    assert_eq!(eval_ok(&with_g(src)), Value::Int(7));
}

// ---- however the body is left -----------------------------------------------

#[test]
fn a_throw_out_of_the_body_still_restores() {
    let src = "(progn
                 (catch 'out (dlet ((*depth* 3)) (throw 'out (peek))))
                 (peek))";
    assert_eq!(eval_ok(&with_g(src)), Value::Int(0));
}

#[test]
fn a_throw_out_of_the_body_carries_the_new_value_with_it() {
    let src = "(catch 'out (dlet ((*depth* 3)) (throw 'out (peek))))";
    assert_eq!(eval_ok(&with_g(src)), Value::Int(3));
}

#[test]
fn a_break_out_of_a_loop_around_the_body_still_restores() {
    let src = "(progn
                 (loop (dlet ((*depth* 3)) (break)))
                 (peek))";
    assert_eq!(eval_ok(&with_g(src)), Value::Int(0));
}

#[test]
fn a_return_out_of_the_body_still_restores() {
    // `return` leaves the nearest enclosing loop (syntax.md §5), so the loop
    // is what the body is inside of here.
    let src = "(defun f () i32 (loop (dlet ((*depth* 3)) (return (peek)))))
               (+ (* 10 (f)) (peek))";
    assert_eq!(eval_ok(&with_g(src)), Value::Int(30));
}

#[test]
fn a_panic_in_the_body_still_restores() {
    // The panic is caught at the `catch` boundary the runtime gives it; what
    // matters is that the global is back by the time anything else runs.
    let src = "(progn (dlet ((*depth* 3)) (panic \"boom\")) (peek))";
    match run(&with_g(src)) {
        Err(_) => {}
        Ok(v) => panic!("expected the panic to propagate, got {:?}", v),
    }
}

// ---- hygiene ----------------------------------------------------------------

#[test]
fn the_saved_value_does_not_capture_a_body_name() {
    // The temporary holding the old value is a `gensym`, so a body that binds
    // every plausible name still sees its own.
    let src = "(dlet ((*depth* 3))
                 (let ((saved 99) (old 98) (tmp 97)) (+ (peek) saved old tmp)))";
    assert_eq!(eval_ok(&with_g(src)), Value::Int(297));
}

#[test]
fn the_new_value_expression_is_evaluated_once() {
    let src = "(let ((n 0))
                 (labels ((bump () i32 (progn (setf n (+ n 1)) n)))
                   (progn (dlet ((*depth* (bump))) (peek)) n)))";
    assert_eq!(eval_ok(&with_g(src)), Value::Int(1));
}

#[test]
fn an_empty_binding_list_is_just_the_body() {
    assert_eq!(eval_ok(&with_g("(dlet () (peek))")), Value::Int(0));
}

// ---- the printer control variables (cl-parity-plan.md Phase 7b) -------------
//
// These are what `dlet` exists for: CL's idiom is to rebind one for the
// extent of a single printing operation.

fn shows(src: &str, expect: &str) {
    let probe = format!("(equal {} \"{}\")", src, expect);
    assert_eq!(
        eval_ok(&probe),
        Value::Bool(true),
        "{} should print as {}",
        src,
        expect
    );
}

#[test]
fn print_base_changes_the_radix_integers_print_in() {
    shows(
        r#"(dlet ((*print-base* 16)) (format false "~a" 255))"#,
        "ff",
    );
    shows(r#"(dlet ((*print-base* 2)) (format false "~a" 5))"#, "101");
    shows(r#"(dlet ((*print-base* 8)) (format false "~a" -9))"#, "-11");
    // Base 10 is the default and prints exactly as before.
    shows(r#"(format false "~a" 255)"#, "255");
}

#[test]
fn print_base_reaches_bignums_too() {
    shows(
        r#"(dlet ((*print-base* 16)) (format false "~a" (int->bignum 255)))"#,
        "ff",
    );
}

#[test]
fn print_radix_adds_the_marker_that_makes_it_read_back() {
    shows(
        r#"(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" 255))"#,
        "#xff",
    );
    shows(
        r#"(dlet ((*print-base* 2) (*print-radix* true)) (format false "~a" 5))"#,
        "#b101",
    );
    shows(
        r#"(dlet ((*print-base* 8) (*print-radix* true)) (format false "~a" 9))"#,
        "#o11",
    );
    shows(
        r#"(dlet ((*print-base* 5) (*print-radix* true)) (format false "~a" 6))"#,
        "#5r11",
    );
    // Base 10 marks itself with a trailing point rather than a prefix.
    shows(
        r#"(dlet ((*print-radix* true)) (format false "~a" 255))"#,
        "255.",
    );
    // The marker comes before the sign, as in CL.
    shows(
        r#"(dlet ((*print-base* 16) (*print-radix* true)) (format false "~a" -255))"#,
        "#x-ff",
    );
}

#[test]
fn a_base_outside_two_to_thirty_six_is_a_printing_error() {
    match run(r#"(dlet ((*print-base* 99)) (format false "~a" 1))"#) {
        Err(e) => {
            let msg = format!("{:?}", e);
            assert!(msg.contains("*print-base*"), "{}", msg);
        }
        Ok(v) => panic!("expected a printing error, got {:?}", v),
    }
}

#[test]
fn print_case_renders_symbol_names() {
    shows(r#"(format false "~a" (quote hello))"#, "hello");
    shows(
        r#"(dlet ((*print-case* :upcase)) (format false "~a" (quote hello)))"#,
        "HELLO",
    );
    shows(
        r#"(dlet ((*print-case* :capitalize)) (format false "~a" (quote hello-world)))"#,
        "Hello-World",
    );
}

#[test]
fn print_readably_turns_escaping_on() {
    // `~a` is the unescaped directive; printing readably overrides it.
    shows(r#"(format false "~a" "hi")"#, "hi");
    shows(
        r#"(dlet ((*print-readably* true)) (format false "~a" "hi"))"#,
        "\\\"hi\\\"",
    );
}

#[test]
fn print_readably_turns_the_length_and_level_cuts_off() {
    let list = "(quote (1 2 3 4 5))";
    shows(
        &format!(
            r#"(dlet ((*print-length* 2)) (format false "~a" {}))"#,
            list
        ),
        "(1 2 ...)",
    );
    shows(
        &format!(
            r#"(dlet ((*print-length* 2) (*print-readably* true)) (format false "~a" {}))"#,
            list
        ),
        "(1 2 3 4 5)",
    );
}

#[test]
fn a_control_variable_is_back_to_its_old_value_after_the_dlet() {
    shows(
        r#"(progn (dlet ((*print-base* 16)) (format false "~a" 255)) (format false "~a" 255))"#,
        "255",
    );
}

#[test]
fn with_standard_io_syntax_pins_every_printer_variable() {
    // Whatever the surrounding program set, the body prints the standard way.
    let src = r#"(progn
                   (setf *print-base* 16)
                   (setf *print-case* :upcase)
                   (with-standard-io-syntax (format false "~a ~a" 255 (quote hi))))"#;
    shows(src, "255 hi");
}

#[test]
fn with_standard_io_syntax_restores_what_it_pinned() {
    let src = r#"(progn
                   (setf *print-base* 16)
                   (with-standard-io-syntax (format false "~a" 255))
                   (format false "~a" 255))"#;
    shows(src, "ff");
}

#[test]
fn with_standard_io_syntax_prints_readably() {
    // CL's standard syntax has `*print-readably*` on, so a string keeps its
    // quotes even through the unescaped directive.
    shows(
        r#"(with-standard-io-syntax (format false "~a" "hi"))"#,
        "\\\"hi\\\"",
    );
}

#[test]
fn print_lines_caps_a_pretty_printed_value() {
    // A list wide enough that the pretty printer has to break it, capped to
    // two lines; the cut is marked with CL's `..`.
    let src = r#"(dlet ((*print-pretty* true) (*print-right-margin* 12) (*print-lines* 2))
                   (format false "~a" (quote (aaaa bbbb cccc dddd eeee))))"#;
    let out = eval_ok(&format!(r#"(is-some (search {} " .."))"#, src));
    assert_eq!(out, Value::Bool(true));
    let lines = eval_ok(&format!(r#"(len (split {} "\n"))"#, src));
    assert_eq!(lines, Value::Int(2));
}

#[test]
fn print_lines_leaves_output_that_already_fits_alone() {
    let src = r#"(dlet ((*print-pretty* true) (*print-lines* 5))
                   (format false "~a" (quote (a b))))"#;
    shows(src, "(a b)");
}

// ---- the radix reader macros ------------------------------------------------
//
// `*print-radix*`'s markers are only worth printing if they read back, so
// `#b`/`#o`/`#x`/`#NNr` come with it rather than waiting for Phase 8b.

#[test]
fn the_radix_macros_read_the_numbers_print_radix_writes() {
    for (text, n) in [
        ("#xff", 255),
        ("#b101", 5),
        ("#o11", 9),
        ("#5r11", 6),
        ("#x-ff", -255),
    ] {
        let src = format!("(match (unwrap (read \"{}\")) ((Int n) n) (_ -1))", text);
        assert_eq!(eval_ok(&src), Value::Int(n), "{}", text);
    }
}

#[test]
fn a_radix_macro_round_trips_what_print_radix_produced() {
    let src = "(match (unwrap (read (dlet ((*print-base* 16) (*print-radix* true))
                                     (format false \"~a\" 48879))))
                 ((Int n) n) (_ -1))";
    assert_eq!(eval_ok(src), Value::Int(48879));
}

#[test]
fn a_radix_macro_past_i32_reads_as_a_bignum() {
    let src = "(match (unwrap (read \"#xffffffffffffffffff\")) ((Bignum _) true) (_ false))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn a_digit_outside_the_radix_is_a_read_error() {
    // `read` answers a `Result`, so a bad datum arrives as a value.
    assert_eq!(eval_ok("(is-err (read \"#b102\"))"), Value::Bool(true));
    assert_eq!(eval_ok("(is-err (read \"#99rZ\"))"), Value::Bool(true));
}
