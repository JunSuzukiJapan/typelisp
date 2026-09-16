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

/// The first check-time error `src` raises, as a string. For the forms whose
/// whole behaviour is a scoping rule, where "this does not resolve" *is* the
/// observation.
fn check_error(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 18);
    let chk = Rc::new(RefCell::new(Checker::new()));
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk.borrow_mut(), &mut interp);
    interp.set_checker(Rc::clone(&chk));
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    for v in vs {
        let checked = chk.borrow_mut().check_form(&mut h, &interp, v);
        match checked {
            Err(e) => return format!("{:?}", e),
            Ok(tl) => {
                interp.exec(&mut h, tl).expect("eval failed");
            }
        }
    }
    panic!("expected a check error, got none");
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
        show("(let ((v (the Vector<int> (Vector::new))))
                (progn (push v 1) (push v 2) (push v 3)
                  (filter (iter v) (complement (lambda ((n int)) bool (> n 1))))))"),
        "#<vector<int> 1>"
    );
}

// ---- `macrolet` / `symbol-macrolet` -----------------------------------------
//
// The rest of Stage 4c. Both are lexically scoped *bindings* of a name that
// is not a value: `macrolet` binds a macro, `symbol-macrolet` binds a form a
// bare name stands for. Neither exists at run time — what the body compiles
// to is the expansion — so every test below is about what the checker sees.

#[test]
fn macrolet_binds_a_macro_for_its_body() {
    assert_eq!(show("(macrolet ((twice (x) `(+ ,x ,x))) (twice 21))"), "42");
}

/// A local binding shadows a global macro of the same name, and only for the
/// extent of its body.
#[test]
fn macrolet_shadows_a_global_macro_and_gives_it_back() {
    assert_eq!(
        show_after(
            "(defmacro sq (x) `(* ,x ,x))",
            "(list (macrolet ((sq (x) `(+ ,x 100))) (sq 5)) (sq 5))"
        ),
        "(105 25)"
    );
}

/// Nested `macrolet`s see the enclosing ones, so a local macro can expand
/// into another local macro's call.
#[test]
fn a_local_macro_can_expand_into_an_enclosing_one() {
    assert_eq!(
        show("(macrolet ((inc (x) `(+ ,x 1))) (macrolet ((inc2 (x) `(inc (inc ,x)))) (inc2 10)))"),
        "12"
    );
}

/// CL's rule, and the difference from `labels`: a binding's *body* does not
/// see its siblings — each is checked in the scope that was open before the
/// `macrolet`, and the new scope opens only once all of them are checked.
///
/// What a sibling name *does* reach is the expansion: `(earlier 1)` expanding
/// to `(later 1)` works, because the expansion is checked at the use site,
/// where both are in scope. CL says the same, and
/// [`a_local_macro_can_expand_into_an_enclosing_one`] is the nested version.
#[test]
fn a_macrolet_body_does_not_see_its_siblings() {
    let err = check_error("(macrolet ((later () `1) (earlier () (later))) (earlier))");
    assert!(err.contains("NoSuchFunction(\"later\")"), "{}", err);
    // The expansion path, by contrast, resolves.
    assert_eq!(show("(macrolet ((later (x) `(+ ,x 1)) (earlier (x) `(later ,x))) (earlier 1))"), "2");
}

/// A `macrolet` binding takes the same lambda list a `defmacro` does — it is
/// checked by the same code, which is the point of splitting that function
/// rather than writing a second one.
#[test]
fn a_local_macro_takes_the_full_lambda_list() {
    assert_eq!(
        show("(macrolet ((sum (a &optional (b 10) &rest more) `(+ ,a ,b))) (sum 1))"),
        "11"
    );
    assert_eq!(
        show("(macrolet ((sum (a &optional (b 10)) `(+ ,a ,b))) (sum 1 2))"),
        "3"
    );
}

#[test]
fn symbol_macrolet_makes_a_name_stand_for_a_form() {
    assert_eq!(
        show("(let ((v (the Vector<int> (Vector::new))))
                (progn (push v 7)
                  (symbol-macrolet ((head (get v 0))) head)))"),
        "7"
    );
}

/// The reason the form exists: an alias you can assign through. `(setf head
/// 42)` is `(setf (get v 0) 42)`.
#[test]
fn setf_through_a_symbol_macro_assigns_to_the_form() {
    assert_eq!(
        show("(let ((v (the Vector<int> (Vector::new))))
                (progn (push v 7)
                  (symbol-macrolet ((head (get v 0)))
                    (progn (setf head 42) head))))"),
        "42"
    );
}

/// CL's shadowing, both directions, and neither is a rule of its own: the
/// name is an ordinary environment binding, so an inner `let` hides the
/// symbol macro and the symbol macro hides an outer variable.
#[test]
fn a_symbol_macro_shadows_and_is_shadowed_like_any_binding() {
    assert_eq!(show("(let ((x 1)) (symbol-macrolet ((x 99)) (let ((x 5)) x)))"), "5");
    assert_eq!(show("(let ((x 1)) (symbol-macrolet ((x 99)) x))"), "99");
    // And it is over when the body is.
    assert_eq!(show("(let ((x 1)) (progn (symbol-macrolet ((x 99)) x) x))"), "1");
}

/// The expansion is checked where the *use* is, not where the binding is —
/// CL says the same, and it is what lets an alias name a local of the body.
#[test]
fn a_symbol_macro_expansion_is_checked_at_the_use_site() {
    assert_eq!(
        show("(symbol-macrolet ((twice-n (* n 2))) (let ((n 21)) twice-n))"),
        "42"
    );
}
