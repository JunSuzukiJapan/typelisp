//! Tests for the pretty printer (TODO T5 — CLHS §22.2): the `*print-*`
//! controls, `format`'s pretty directives (`~_`, `~i`, `~<…~:>`, `~:t`, and
//! the `*print-pretty*` path of `~a`/`~s`/`~w`), the `pprint` family, and the
//! user-callable `pprint-logical-block`/`pprint-newline`/`pprint-indent`/
//! `pprint-tab`/`pprint-pop` operators.
//!
//! `format` with a `false` destination returns its string, so the directive
//! half needs no stdout capture. The `pprint`/`pprint-logical-block` half
//! writes to stdout by definition (CL's write to a stream), so those tests
//! shell out to the `typl` binary and read what it printed.

extern crate typelisp;

mod common;
use common::{check_err, eval_err, eval_string, run};

/// The string the last form produced, panicking on any check/eval error.
fn fmt(src: &str) -> String {
    eval_string(src)
}

/// Runs `src` through the `typl` binary and returns its stdout — the only way
/// to observe the operators that print rather than return a string.
fn stdout_of(src: &str) -> String {
    stdout_of_with(&[], src)
}

/// [`stdout_of`] with extra `typl` flags (`--heap-cells`, to force collections
/// during printing).
fn stdout_of_with(flags: &[&str], src: &str) -> String {
    use std::io::Write;
    let dir = std::env::temp_dir().join(format!("typelisp-pprint-{}-{:?}", std::process::id(), std::thread::current().id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("prog.typl");
    let mut f = std::fs::File::create(&path).expect("create source");
    f.write_all(src.as_bytes()).expect("write source");
    drop(f);
    let exe = env!("CARGO_BIN_EXE_typl");
    let out = std::process::Command::new(exe).args(flags).arg(&path).output().expect("run typl");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "typl failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// ---------------------------------------------------------------------------
// The `*print-*` controls
// ---------------------------------------------------------------------------

#[test]
fn pretty_printing_is_off_by_default() {
    // Without `*print-pretty*`, a long list is one long line exactly as before
    // the pretty printer existed.
    let src = r#"
        (setf *print-right-margin* 10)
        (format false "~a" '(1 2 3 4 5 6 7 8 9 10))
    "#;
    assert_eq!(fmt(src), "(1 2 3 4 5 6 7 8 9 10)");
}

#[test]
fn print_pretty_makes_a_over_long_list_wrap_at_the_right_margin() {
    let src = r#"
        (setf *print-pretty* true)
        (setf *print-right-margin* 10)
        (format false "~a" '(1 2 3 4 5 6 7 8 9 10))
    "#;
    // The trailing space before each conditional newline is part of the
    // section being measured (it is trimmed only if the break is taken), which
    // is why `4` rather than `5` ends the first line.
    assert_eq!(fmt(src), "(1 2 3 4\n 5 6 7 8\n 9 10)");
}

#[test]
fn a_wide_right_margin_leaves_everything_on_one_line() {
    let src = r#"
        (setf *print-pretty* true)
        (setf *print-right-margin* 200)
        (format false "~a" '(1 2 3 4 5 6 7 8 9 10))
    "#;
    assert_eq!(fmt(src), "(1 2 3 4 5 6 7 8 9 10)");
}

#[test]
fn miser_width_turns_fill_newlines_into_linear_ones() {
    // The block starts at column 0 with a margin of 12, so a miser width of 12
    // puts it in miser style; every conditional newline then behaves linearly.
    let src = r#"
        (setf *print-pretty* true)
        (setf *print-right-margin* 12)
        (setf *print-miser-width* 12)
        (format false "~a" '(11 22 33 44))
    "#;
    assert_eq!(fmt(src), "(11\n 22\n 33\n 44)");
}

// ---------------------------------------------------------------------------
// `format`'s pretty directives
// ---------------------------------------------------------------------------

#[test]
fn logical_block_directive_wraps_its_body() {
    let src = r#"
        (setf *print-pretty* true)
        (setf *print-right-margin* 12)
        (format false "~:<~a ~:_~a ~:_~a~:>" '(alpha beta gamma))
    "#;
    // `~:<` supplies the `(`/`)` prefix and suffix; `~:_` is a fill newline.
    assert_eq!(fmt(src), "(alpha beta\n gamma)");
}

#[test]
fn logical_block_directive_takes_explicit_prefix_and_suffix_segments() {
    let src = r#"
        (setf *print-pretty* true)
        (setf *print-right-margin* 10)
        (format false "~<[~;~a ~_~a~;]~:>" '(alpha beta))
    "#;
    assert_eq!(fmt(src), "[alpha\n beta]");
}

#[test]
fn a_per_line_prefix_segment_starts_every_line() {
    let src = r#"
        (setf *print-pretty* true)
        (format false "~<;; ~@;~a~:@_~a~:>" '(one two))
    "#;
    assert_eq!(fmt(src), ";; one\n;; two");
}

#[test]
fn indent_directive_moves_the_continuation_lines() {
    let src = r#"
        (setf *print-pretty* true)
        (format false "~<(~;~4i~a~:@_~a~;)~:>" '(head body))
    "#;
    assert_eq!(fmt(src), "(head\n    body)");
}

#[test]
fn the_justification_form_of_the_same_opener_is_untouched() {
    // `~<…~>` (no `:` on the closer) is still justification, not a block.
    assert_eq!(fmt(r#"(format false "~10<~a~;~a~>" "ab" "cd")"#), "ab      cd");
}

#[test]
fn pretty_directives_are_no_ops_when_print_pretty_is_false() {
    let src = r#"(format false "~:<~a ~:_~a~:>" '(alpha beta))"#;
    assert_eq!(fmt(src), "(alpha beta)");
}

#[test]
fn a_non_literal_block_prefix_is_rejected() {
    let msg = check_err(r#"(format false "~<~a~;~a~;x~:>" '(a b))"#);
    assert!(msg.contains("literal text"), "unexpected error: {:?}", msg);
}

#[test]
fn w_directive_pretty_prints_and_keeps_reader_syntax() {
    let src = r#"
        (setf *print-pretty* true)
        (setf *print-right-margin* 12)
        (format false "~w" '("aa" "bb" "cc"))
    "#;
    assert_eq!(fmt(src), "(\"aa\" \"bb\"\n \"cc\")");
}

#[test]
fn an_explicitly_padded_a_directive_stays_flat() {
    // Padding parameters fix a width, which a layout would be free to change;
    // the directive keeps its flat rendering rather than silently ignoring one
    // of the two requests.
    let src = r#"
        (setf *print-pretty* true)
        (setf *print-right-margin* 4)
        (format false "~12a|" '(1 2 3 4))
    "#;
    assert_eq!(fmt(src), "(1 2 3 4)   |");
}

// ---------------------------------------------------------------------------
// The `pprint` family
// ---------------------------------------------------------------------------

#[test]
fn pprint_fills_a_list_and_emits_a_leading_newline() {
    // CLHS defines `pprint` as `(progn (terpri) (write obj :pretty t))`.
    let out = stdout_of(
        r#"
        (setf *print-right-margin* 20)
        (pprint '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
    "#,
    );
    assert_eq!(out, "\n(1 2 3 4 5 6 7 8 9\n 10 11 12 13 14 15)");
}

#[test]
fn pprint_linear_puts_every_element_on_its_own_line_or_none() {
    let out = stdout_of(
        r#"
        (setf *print-right-margin* 8)
        (pprint-linear '(11 22 33))
        (println "")
        (setf *print-right-margin* 80)
        (pprint-linear '(11 22 33))
    "#,
    );
    assert_eq!(out, "(11\n 22\n 33)\n(11 22 33)");
}

#[test]
fn pprint_tabular_lines_its_columns_up_across_breaks() {
    let out = stdout_of(
        r#"
        (setf *print-right-margin* 20)
        (pprint-tabular '(1 2 3 4 5 6 7 8 9 10 11 12) 5)
    "#,
    );
    assert_eq!(out, "(1    2    3    4\n 5    6    7    8\n 9    10   11   12)");
}

#[test]
fn pprint_lays_code_shaped_forms_out_with_a_body_indent() {
    let out = stdout_of(
        r#"
        (setf *print-right-margin* 30)
        (pprint-fill '(defun f (x) int (+ x 1) (* x 2)))
    "#,
    );
    // `pprint-fill` is asked for explicitly, so the code layout does not apply.
    assert_eq!(out, "(defun f (x) int (+ x 1)\n (* x 2))");
    let out = stdout_of(
        r#"
        (setf *print-right-margin* 30)
        (pprint '(defun f (x) int (+ x 1) (* x 2)))
    "#,
    );
    // The default dispatch recognizes `defun`: head plus three distinguished
    // arguments, then one body form per line.
    assert_eq!(out, "\n(defun f (x) int\n  (+ x 1)\n  (* x 2))");
}

#[test]
fn pprint_abbreviates_quote() {
    let out = stdout_of("(pprint '(quote x))");
    assert_eq!(out, "\n'x");
}

#[test]
fn pprint_accepts_any_type_not_only_lists() {
    let out = stdout_of(r#"(pprint-fill 42) (println "") (pprint-fill "hi")"#);
    assert_eq!(out, "42\n\"hi\"");
}

// ---------------------------------------------------------------------------
// The user-callable operators
// ---------------------------------------------------------------------------

#[test]
fn a_logical_block_collects_ordinary_print_calls() {
    let out = stdout_of(
        r#"
        (setf *print-right-margin* 24)
        (pprint-logical-block ('(alpha beta gamma delta epsilon zeta) :prefix "(" :suffix ")")
          (loop (pprint-exit-if-list-exhausted)
                (print "~w" (pprint-pop))
                (if (pprint-list-exhausted) () (progn (print " ") (pprint-newline :fill)))))
    "#,
    );
    assert_eq!(out, "(alpha beta gamma delta\n epsilon zeta)");
}

#[test]
fn a_logical_block_supports_a_per_line_prefix() {
    let out = stdout_of(
        r#"
        (pprint-logical-block (() :per-line-prefix ";; ")
          (print "first")
          (pprint-newline :mandatory)
          (print "second"))
    "#,
    );
    assert_eq!(out, ";; first\n;; second");
}

#[test]
fn pprint_indent_and_tab_place_the_continuation_lines() {
    let out = stdout_of(
        r#"
        (pprint-logical-block (() :prefix "[" :suffix "]")
          (pprint-indent :block 4)
          (print "aaaa")
          (pprint-newline :mandatory)
          (print "bb")
          (pprint-tab :line 12 1)
          (print "cc"))
    "#,
    );
    assert_eq!(out, "[aaaa\n    bb      cc]");
}

#[test]
fn nested_logical_blocks_flush_only_once_at_the_outermost_close() {
    let out = stdout_of(
        r#"
        (setf *print-right-margin* 12)
        (pprint-logical-block (() :prefix "(" :suffix ")")
          (print "outer")
          (print " ")
          (pprint-newline :linear)
          (pprint-logical-block (() :prefix "<" :suffix ">")
            (print "a")
            (print " ")
            (pprint-newline :linear)
            (print "b")))
    "#,
    );
    assert_eq!(out, "(outer\n <a b>)");
}

#[test]
fn the_layout_operators_are_no_ops_outside_a_block() {
    // CL's are too, on a stream that is not a pretty stream.
    let out = stdout_of(r#"(pprint-newline :mandatory) (print "x") (pprint-indent :block 4)"#);
    assert_eq!(out, "x");
}

#[test]
fn a_break_out_of_a_block_still_flushes_what_it_printed() {
    // A non-local exit skips the block's own close, so the session would
    // otherwise stay open and swallow every later `print` — `exec` closes any
    // still-open session at the end of the top-level form.
    let out = stdout_of(
        r#"
        (loop
          (pprint-logical-block (() :prefix "(" :suffix ")")
            (print "partial")
            (break)))
        (println "after")
    "#,
    );
    assert_eq!(out, "(partial)after\n");
}

#[test]
fn pprint_newline_rejects_an_unknown_keyword() {
    let err = run(r#"(pprint-logical-block (()) (pprint-newline :sideways))"#).expect_err("should fail");
    assert!(format!("{:?}", err).contains(":linear"), "unexpected error: {:?}", err);
}

// ---------------------------------------------------------------------------
// `print-object` — a type's own printed representation (TODO T5-b)
// ---------------------------------------------------------------------------

/// A `point` whose `print-object` renders differently under `~a` and `~s`, so
/// one impl exercises both the dispatch and CL's `*print-escape*`.
const POINT: &str = r##"
(defstruct point (x int) (y int))
(impl print-object point
  (print-object ((self Self) (escape bool)) string
    (if escape (format false "#S(point :x ~d :y ~d)" self::x self::y)
               (format false "(~d,~d)" self::x self::y))))
(defvar (p point) (point::new 1 2))
(defvar (q point) (point::new 3 4))
"##;

#[test]
fn a_type_with_a_print_object_impl_prints_its_own_way() {
    assert_eq!(stdout_of(&format!(r#"{} (print "~a" p)"#, POINT)), "(1,2)");
}

#[test]
fn the_escape_flag_distinguishes_aesthetic_from_standard() {
    // `~a` is CL's `princ` (escape false), `~s` its `prin1` (escape true).
    // Only the renderer knows which directive is asking — which is why the
    // dispatch lives there rather than at the call site.
    assert_eq!(
        stdout_of(&format!(r#"{} (print "~a|~s" p p)"#, POINT)),
        "(1,2)|#S(point :x 1 :y 2)"
    );
}

#[test]
fn a_nested_value_dispatches_too() {
    // The element's static type is gone (a list is `Sexpr`), so this is the
    // case that needs the runtime dispatch at all.
    assert_eq!(stdout_of(&format!(r#"{} (print "~a" (list p q))"#, POINT)), "((1,2) (3,4))");
}

#[test]
fn a_type_without_an_impl_keeps_the_built_in_representation() {
    let out = stdout_of(r#"(defstruct plain (n int)) (print "~a" (plain::new 7))"#);
    assert!(out.ends_with("plain n: 7>"), "unexpected output: {}", out);
}

#[test]
fn a_method_named_print_object_that_is_not_the_trait_is_ignored() {
    // The signature check keeps an unrelated `defmethod` of the same name from
    // being mistaken for an implementation of the trait.
    let out = stdout_of(
        r#"
        (defstruct thing (n int))
        (defmethod print-object ((self thing)) int self::n)
        (print "~a" (thing::new 7))
    "#,
    );
    assert!(out.ends_with("thing n: 7>"), "unexpected output: {}", out);
}

#[test]
fn a_print_object_that_prints_itself_falls_back_instead_of_looping() {
    // The re-entry guard renders the inner occurrence the built-in way.
    let out = stdout_of(
        r#"
        (defstruct loopy (n int))
        (impl print-object loopy
          (print-object ((self Self) (escape bool)) string (format false "<~a>" self)))
        (print "~a" (loopy::new 1))
    "#,
    );
    assert!(out.starts_with("<#<") && out.ends_with("loopy n: 1>>"), "unexpected output: {}", out);
}

#[test]
fn a_custom_representation_composes_with_the_pretty_printer() {
    let out = stdout_of(&format!(
        r#"{} (setf *print-pretty* true) (setf *print-right-margin* 12) (print "~a" (list p q p))"#,
        POINT
    ));
    assert_eq!(out, "((1,2)\n (3,4) (1,2))");
}

#[test]
fn a_print_object_that_conses_heavily_survives_collections() {
    // The point of the whole GC-rooting design: user code now runs *inside*
    // the renderer, so the value being printed must stay rooted across
    // collections triggered by that code. The arena is small and each printer
    // call conses thousands of cells, so this collects many times over.
    let out = stdout_of_with(
        &["--heap-cells", "20000"],
        r#"
        (defstruct heavy (n int))
        (impl print-object heavy
          (print-object ((self Self) (escape bool)) string
            (let ((acc "x"))
              (dotimes (i 200)
                (setf acc (format false "~a" (list 1 2 3 4 5 6 7 8 9 10))))
              (format false "H~d" self::n))))
        (dotimes (i 100)
          (println "~a" (list (heavy::new (as int i)) (heavy::new (as int i)))))
    "#,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 100, "unexpected line count: {:?}", &lines[..lines.len().min(5)]);
    assert_eq!(lines[0], "(H0 H0)");
    assert_eq!(lines[99], "(H99 H99)");
}

#[test]
fn pprint_tabular_refuses_a_negative_column_width() {
    let err = eval_err("(pprint-tabular '(1 2) -1)");
    assert!(err.contains("pprint-tabular") && err.contains("-1"), "{}", err);
}

#[test]
fn pprint_tab_refuses_negative_columns() {
    // CL requires both to be non-negative; a negative one used to reach the
    // layout pass and abort the process there.
    let err = eval_err(r#"(pprint-logical-block (() :prefix "(" :suffix ")") (pprint-tab :line -1 1))"#);
    assert!(err.contains("pprint-tab") && err.contains("-1"), "{}", err);
}
