//! `macroexpand`/`macroexpand-1`, `gensym`'s prefix and `*gensym-counter*`,
//! and `complement` — [cl-parity-plan.md](../docs/dev/cl-parity-plan.md)
//! Stage 4c.

extern crate typelisp;
use std::cell::RefCell;
use std::rc::Rc;
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, Value};

fn show(src: &str) -> String {
    show_after("", src)
}

/// `defs` as top-level forms first, then `expr`. `defmacro` is top-level-only
/// and strictly define-before-use, so a macro under test cannot be wrapped in
/// the same form as the expression that expands it.
fn show_after(defs: &str, expr: &str) -> String {
    let src = format!("{}\n(format false \"~a\" {})", defs, expr);
    let mut h = Heap::with_capacity(1 << 18);
    // `macroexpand` needs the live `Checker` wired into the `Interp`, the way
    // the driver wires it (`main.rs`'s `set_checker`) — its expansion step is
    // the checker's own. The borrow is never held across `exec`, which is
    // where a re-entrant `macroexpand` call would take its own.
    let chk = Rc::new(RefCell::new(Checker::new()));
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk.borrow_mut(), &mut interp);
    interp.set_checker(Rc::clone(&chk));
    let r = Reader::new();
    let vs = r.read_all(&mut h, &src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.borrow_mut().check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// `expr` with `(defmacro twice (x) `(+ ,x ,x))` already defined.
fn with_twice(expr: &str) -> String {
    show_after("(defmacro twice (x) `(+ ,x ,x))", expr)
}

#[test]
fn macroexpand_1_expands_one_step_and_says_none_for_a_non_macro() {
    assert_eq!(with_twice("(macroexpand-1 '(twice 5))"), "(ok (some (+ 5 5)))");
    // CL's second return value, carried in the `Option` instead — and the
    // `none` says "not a macro call" without a boolean the caller could
    // confuse with a macro that expands to its own call form.
    assert_eq!(with_twice("(macroexpand-1 '(+ 1 2))"), "(ok none)");
    assert_eq!(with_twice("(macroexpand-1 'foo)"), "(ok none)");
    assert_eq!(with_twice("(macroexpand-1 '())"), "(ok none)");
}

#[test]
fn macroexpand_repeats_until_the_form_is_no_longer_a_macro_call() {
    // `when` is a prelude macro over `if`, so one call is two steps.
    assert_eq!(show("(macroexpand '(when true 1))"), "(ok (if true (progn 1 ()) ()))");
    assert_eq!(with_twice("(macroexpand '(twice 5))"), "(ok (+ 5 5))");
    assert_eq!(with_twice("(macroexpand '(+ 1 2))"), "(ok (+ 1 2))");
}

#[test]
fn the_step_is_the_checkers_own_so_a_bad_call_reports_like_one() {
    // A macro called with the wrong number of arguments fails the same
    // arity check the checker applies, and the failure arrives as `Err`
    // rather than as a panic.
    let out = with_twice("(macroexpand-1 '(twice 1 2))");
    assert!(out.starts_with("(err "), "{}", out);
}

#[test]
fn gensym_takes_a_prefix_and_counts_through_the_counter() {
    // The name starts with a space, which no source can write.
    assert_eq!(show("(symbol->string (gensym))"), " g0");
    assert_eq!(show("(symbol->string (gensym \"tmp\"))"), " tmp0");
    assert_eq!(
        show("(progn (gensym) (gensym) (symbol->string (gensym)))"),
        " g2"
    );
}

#[test]
fn the_counter_is_a_variable_a_program_can_read_and_set() {
    assert_eq!(show("(progn (gensym) *gensym-counter*)"), "1");
    assert_eq!(
        show("(progn (setf *gensym-counter* 100) (symbol->string (gensym)))"),
        " g100"
    );
}

#[test]
fn a_macro_still_gets_fresh_names_from_the_prelude_gensym() {
    // `dotimes` calls `gensym` for its limit binding. Two nested ones must
    // not collide — the same guarantee the built-in used to give, now from
    // one prelude definition shared by the interpreted and compiled paths.
    assert_eq!(
        show("(let ((n 0)) (progn (dotimes (i 3) (dotimes (j 2) (setf n (+ n 1)))) n))"),
        "6"
    );
}

#[test]
fn complement_answers_the_opposite_of_its_predicate() {
    assert_eq!(
        show("(let ((v (the Vector<i32> (Vector::new))))
                (progn (push v 1) (push v 2) (push v 3)
                  (filter (iter v) (complement (lambda ((n i32)) bool (> n 1))))))"),
        "#<vector<i32> 1>"
    );
}
