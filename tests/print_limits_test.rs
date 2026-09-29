//! Tests for the three CLHS 22.1.1 printer controls that bound *what* gets
//! printed — `*print-level*`, `*print-length*`, `*print-circle*` — as opposed
//! to the layout controls (`*print-pretty*`/`*print-right-margin*`/
//! `*print-miser-width*`, covered by `pprint_test.rs`).
//!
//! `*print-circle*` is the one with teeth: a `defstruct` whose field is
//! `setf`-ed to point back at the value itself is genuinely circular, and
//! printing it without labels recurses until the process dies. Every test
//! here that builds such a value therefore *must* set `*print-circle*` first.
//!
//! `format` with a `false` destination returns its string, so almost
//! everything is observable in-process; the one `pprint` test shells out
//! because `pprint` writes to stdout by definition.

extern crate typelisp;

mod common;
use common::{eval_err, eval_string};

/// The string the last form produced, panicking on any check/eval error.
fn fmt(src: &str) -> String {
    eval_string(src)
}

/// A self-referential type plus a constructor that ties the knot: `(cycle)`
/// returns a `node` whose `next` points back at itself. Prepended to the
/// sources that need one.
const CYCLE_DEFS: &str = r#"
    (defstruct node (val int) (next Option<node>))
    (defun cycle () node
      (let ((a (node::new 1 (Option::none))))
        (setf a::next (Option::some a))
        a))
"#;

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

#[test]
fn every_limit_is_off_by_default() {
    // CL's initial state for all three, and what keeps existing programs'
    // output byte-for-byte what it was.
    assert_eq!(fmt(r#"(format false "~a" '(1 (2 (3 (4 (5))))))"#), "(1 (2 (3 (4 (5)))))");
    assert_eq!(fmt(r#"(format false "~a" '(1 2 3 4 5 6 7 8 9 10))"#), "(1 2 3 4 5 6 7 8 9 10)");
}

#[test]
fn zero_means_unlimited() {
    // typelisp has no `nil`, so 0 stands in for it — the same convention
    // `*print-right-margin*`/`*print-miser-width*` use.
    let src = r#"
        (setf *print-level* 0)
        (setf *print-length* 0)
        (format false "~a" '(1 (2 (3 (4)))))
    "#;
    assert_eq!(fmt(src), "(1 (2 (3 (4))))");
}

#[test]
fn a_negative_limit_is_a_printing_error() {
    // A negative limit means nothing, so it is refused rather than read as
    // "no limit".
    for var in ["*print-level*", "*print-length*", "*print-right-margin*", "*print-miser-width*", "*print-lines*"] {
        let err = eval_err(&format!(r#"(setf {} -1) (format false "~a" 1)"#, var));
        assert!(err.contains(var) && err.contains("got -1"), "{}: {}", var, err);
    }
}

#[test]
fn an_unknown_print_case_is_a_printing_error() {
    // `*print-case*` is a `symbol`, so the checker lets any keyword in; the
    // printer takes only CL's three and says so for anything else.
    let err = eval_err(r#"(setf *print-case* :bogus) (format false "~a" 1)"#);
    assert!(err.contains("*print-case*") && err.contains(":bogus"), "{}", err);
}

// ---------------------------------------------------------------------------
// `*print-level*`
// ---------------------------------------------------------------------------

#[test]
fn print_level_replaces_deeper_objects_with_hash() {
    let src = r#"
        (setf *print-level* 2)
        (format false "~a" '(1 (2 (3 (4)))))
    "#;
    // The object handed to the printer is at level 0, so with a limit of 2
    // the third list down is the one that collapses.
    assert_eq!(fmt(src), "(1 (2 #))");
}

#[test]
fn print_level_one_keeps_only_the_outermost_list() {
    let src = r#"
        (setf *print-level* 1)
        (format false "~a" '(1 (2) (3)))
    "#;
    assert_eq!(fmt(src), "(1 # #)");
}

#[test]
fn print_level_leaves_atoms_alone() {
    // A number/symbol/string prints at any depth: `#` stands for a nested
    // *object*, never for a scalar (CL behaves the same).
    let src = r#"
        (setf *print-level* 1)
        (format false "~a" '(1 two "three" #\f))
    "#;
    assert_eq!(fmt(src), "(1 two three f)");
}

#[test]
fn print_level_applies_to_struct_and_enum_values() {
    let src = r#"
        (defstruct pt (x int) (y int))
        (defstruct pair (a pt) (b pt))
        (setf *print-level* 1)
        (format false "~a" (pair::new (pt::new 1 2) (pt::new 3 4)))
    "#;
    assert_eq!(fmt(src), "#<pair a: # b: #>");
}

// ---------------------------------------------------------------------------
// `*print-length*`
// ---------------------------------------------------------------------------

#[test]
fn print_length_truncates_a_list_with_an_ellipsis() {
    let src = r#"
        (setf *print-length* 4)
        (format false "~a" '(1 2 3 4 5 6 7 8))
    "#;
    assert_eq!(fmt(src), "(1 2 3 4 ...)");
}

#[test]
fn print_length_applies_at_every_level() {
    let src = r#"
        (setf *print-length* 2)
        (format false "~a" '(1 2 3 (4 5 6)))
    "#;
    assert_eq!(fmt(src), "(1 2 ...)");
}

#[test]
fn a_dotted_tail_within_the_limit_still_prints() {
    // Only elements count against `*print-length*`; both walkers agree. `cons`
    // is the generic `cons-cell<A,B>` struct — a *dotted `Sexpr` list*, the
    // thing `*print-length*` actually walks, is written `'(1 2 . 3)`.
    let flat = r#"
        (setf *print-length* 2)
        (format false "~a" '(1 2 . 3))
    "#;
    let pretty = r#"
        (setf *print-pretty* true)
        (setf *print-length* 2)
        (format false "~a" '(1 2 . 3))
    "#;
    assert_eq!(fmt(flat), "(1 2 . 3)");
    assert_eq!(fmt(pretty), "(1 2 . 3)");
}

#[test]
fn a_list_shorter_than_the_limit_is_untouched() {
    let src = r#"
        (setf *print-length* 5)
        (format false "~a" '(1 2 3))
    "#;
    assert_eq!(fmt(src), "(1 2 3)");
}

#[test]
fn print_length_truncates_struct_fields() {
    let src = r#"
        (defstruct quad (a int) (b int) (c int) (d int))
        (setf *print-length* 2)
        (format false "~a" (quad::new 1 2 3 4))
    "#;
    assert_eq!(fmt(src), "#<quad a: 1 b: 2 ...>");
}

#[test]
fn print_length_truncates_enum_fields() {
    let src = r#"
        (defenum shape (tri int int int))
        (setf *print-length* 2)
        (format false "~a" (shape::tri 3 4 5))
    "#;
    assert_eq!(fmt(src), "(tri 3 4 ...)");
}

#[test]
fn level_and_length_compose() {
    let src = r#"
        (setf *print-level* 1)
        (setf *print-length* 2)
        (format false "~a" '(1 (2 3) 4 5))
    "#;
    assert_eq!(fmt(src), "(1 # ...)");
}

// ---------------------------------------------------------------------------
// `*print-circle*`
// ---------------------------------------------------------------------------

#[test]
fn a_circular_structure_prints_with_labels_instead_of_recursing() {
    let src = format!(
        r#"
        {CYCLE_DEFS}
        (setf *print-circle* true)
        (format false "~a" (cycle))
    "#
    );
    // `#1=` marks the definition, `#1#` the reference back to it.
    assert_eq!(fmt(&src), "#1=#<node val: 1 next: (some #1#)>");
}

#[test]
fn print_circle_labels_substructure_that_is_merely_shared() {
    // Not circular, just reached twice — CL labels this too, so that the
    // printed form records that the two are the *same* object.
    let src = r#"
        (setf *print-circle* true)
        (let ((x '(1 2)))
          (format false "~a" (list x x)))
    "#;
    assert_eq!(fmt(src), "(#1=(1 2) #1#)");
}

#[test]
fn print_circle_leaves_an_unshared_value_exactly_as_it_was() {
    // The scan finds nothing reached twice, so no label appears anywhere —
    // turning the flag on must not perturb ordinary output.
    let plain = fmt(r#"(format false "~a" '(1 (2 3) 4))"#);
    let circled = fmt(
        r#"
        (setf *print-circle* true)
        (format false "~a" '(1 (2 3) 4))
    "#,
    );
    assert_eq!(circled, plain);
    assert_eq!(circled, "(1 (2 3) 4)");
}

#[test]
fn each_argument_is_labelled_from_one() {
    // CL numbers labels per printed object; each `~a` argument is its own.
    let src = r#"
        (setf *print-circle* true)
        (let ((x '(1 2)))
          (format false "~a ~a" (list x x) (list x x)))
    "#;
    assert_eq!(fmt(src), "(#1=(1 2) #1#) (#1=(1 2) #1#)");
}

#[test]
fn labels_work_under_standard_syntax_too() {
    // `~s` (CL `prin1`) goes through the same renderer as `~a`.
    let src = r#"
        (setf *print-circle* true)
        (let ((x '("a")))
          (format false "~s" (list x x)))
    "#;
    assert_eq!(fmt(src), "(#1=(\"a\") #1#)");
}

#[test]
fn a_cycle_reached_through_two_paths_gets_one_label() {
    let src = format!(
        r#"
        {CYCLE_DEFS}
        (setf *print-circle* true)
        (let ((a (cycle)))
          (format false "~a" (list a a)))
    "#
    );
    assert_eq!(fmt(&src), "(#1=#<node val: 1 next: (some #1#)> #1#)");
}

// ---------------------------------------------------------------------------
// The limits reach the other printing paths
// ---------------------------------------------------------------------------

#[test]
fn the_pretty_path_of_a_honors_the_limits_too() {
    // With `*print-pretty*` on, `~a` hands lists to the pretty printer's own
    // walker; it must apply the same cuts rather than print in full.
    let src = r#"
        (setf *print-pretty* true)
        (setf *print-level* 2)
        (setf *print-length* 3)
        (format false "~a" '(1 2 3 4 (5 (6))))
    "#;
    assert_eq!(fmt(src), "(1 2 3 ...)");
}

#[test]
fn the_ellipsis_wraps_with_the_elements_it_replaces() {
    // `...` is emitted on the same conditional-newline footing as a real
    // element: it fills onto the current line when it fits there…
    let fits = r#"
        (setf *print-pretty* true)
        (setf *print-right-margin* 8)
        (setf *print-length* 4)
        (format false "~a" '(111 222 333 444 555 666))
    "#;
    assert_eq!(fmt(fits), "(111\n 222\n 333\n 444 ...)");

    // …and breaks onto the next line when it does not.
    let breaks = r#"
        (setf *print-pretty* true)
        (setf *print-right-margin* 8)
        (setf *print-length* 1)
        (format false "~a" '(111111 222222))
    "#;
    assert_eq!(fmt(breaks), "(111111\n ...)");
}

#[test]
fn circle_labels_survive_the_pretty_walker() {
    // The label table has to span both walkers: the shared sublist is found
    // by the scan, labelled by the list layout, and referred back to later.
    let src = r#"
        (setf *print-pretty* true)
        (setf *print-circle* true)
        (let ((x '(1 2)))
          (format false "~a" (list x x)))
    "#;
    assert_eq!(fmt(src), "(#1=(1 2) #1#)");
}

#[test]
fn println_honors_the_limits() {
    let out = stdout_of(
        r#"
        (setf *print-length* 3)
        (println "~a" '(1 2 3 4 5))
    "#,
    );
    assert_eq!(out, "(1 2 3 ...)\n");
}

#[test]
fn pprint_honors_the_limits() {
    // `pprint` renders through the same `render_value`, so a limit set for
    // `~a` applies there as well.
    let out = stdout_of(
        r#"
        (setf *print-level* 1)
        (pprint '(1 (2 3)))
    "#,
    );
    // `pprint` emits a leading newline (CLHS) and none after.
    assert_eq!(out, "\n(1 #)");
}

/// Runs `src` through the `typl` binary and returns its stdout — the only way
/// to observe the operators that print rather than return a string.
fn stdout_of(src: &str) -> String {
    use std::io::Write;
    let dir = std::env::temp_dir()
        .join(format!("typelisp-print-limits-{}-{:?}", std::process::id(), std::thread::current().id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("prog.typl");
    let mut f = std::fs::File::create(&path).expect("create source");
    f.write_all(src.as_bytes()).expect("write source");
    drop(f);
    let exe = env!("CARGO_BIN_EXE_typl");
    let out = std::process::Command::new(exe).arg(&path).output().expect("run typl");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "typl failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}
