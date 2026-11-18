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
    assert_eq!(with_twice("(macroexpand-1 '(twice 5))"), "(Ok (+ 5 5))");
    // CL's second return value, carried in the `Option` instead — and the
    // `None` says "not a macro call" without a boolean the caller could
    // confuse with a macro that expands to its own call form. It prints as
    // `()`: an `Option<Sexpr>` is S-expression data and prints as the datum
    // it is, the empty list included (`docs/ja/reference/functions/printing.md` §1) — this used to print
    // `None` only because the interpreter built a box the type did not ask
    // for.
    assert_eq!(with_twice("(macroexpand-1 '(+ 1 2))"), "(Ok ())");
    assert_eq!(with_twice("(macroexpand-1 'foo)"), "(Ok ())");
    assert_eq!(with_twice("(macroexpand-1 '())"), "(Ok ())");
}

#[test]
fn macroexpand_repeats_until_the_form_is_no_longer_a_macro_call() {
    // `when` is a prelude macro over `if`, so one call is two steps.
    assert_eq!(show("(macroexpand '(when true 1))"), "(Ok (if true (progn 1 ()) ()))");
    assert_eq!(with_twice("(macroexpand '(twice 5))"), "(Ok (+ 5 5))");
    assert_eq!(with_twice("(macroexpand '(+ 1 2))"), "(Ok (+ 1 2))");
}

#[test]
fn the_step_is_the_checkers_own_so_a_bad_call_reports_like_one() {
    // A macro called with the wrong number of arguments fails the same
    // arity check the checker applies, and the failure arrives as `Err`
    // rather than as a panic.
    let out = with_twice("(macroexpand-1 '(twice 1 2))");
    assert!(out.starts_with("(Err "), "{}", out);
}

#[test]
fn gensym_takes_a_prefix_and_counts_through_the_counter() {
    // The name starts with a space, which no source can write.
    assert_eq!(show("(std::symbol::symbol->string (gensym))"), " g0");
    assert_eq!(show("(std::symbol::symbol->string (gensym \"tmp\"))"), " tmp0");
    assert_eq!(
        show("(progn (gensym) (gensym) (std::symbol::symbol->string (gensym)))"),
        " g2"
    );
}

#[test]
fn the_counter_is_a_variable_a_program_can_read_and_set() {
    assert_eq!(show("(progn (gensym) *gensym-counter*)"), "1");
    assert_eq!(
        show("(progn (setf *gensym-counter* 100) (std::symbol::symbol->string (gensym)))"),
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
        "#(1)"
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

// ---- `macroexpand` inside a macro, `&environment` ----------------------------
//
// CL's use of `macroexpand` is in a macro's own body, while the checker is
// expanding it. The checker is busy then, so the expansion goes through the
// place the call is being expanded at, which the evaluator holds for as long
// as the macro runs; `&environment` names that place.

/// What `(macroexpand form)` gives, quoted, as the macro's expansion.
const SHOW: &str = "(defmacro show (form) (match (macroexpand form) ((Ok e) `(quote ,e)) ((Err _) ''err)))";
/// The same, in the macro call's environment.
const SHOW_HERE: &str =
    "(defmacro show-here (form &environment env) (match (macroexpand form env) ((Ok e) `(quote ,e)) ((Err _) ''err)))";

#[test]
fn a_macro_body_can_macroexpand() {
    let defs = format!("(defmacro twice (x) `(+ ,x ,x))\n{}", SHOW);
    assert_eq!(show_after(&defs, "(show (twice 3))"), "(+ 3 3)");
}

/// With the call's environment, the local macros and symbol macros around
/// it are expanded too; without one, CL's null lexical environment, they are
/// not.
#[test]
fn the_environment_sees_the_local_macros_around_the_call() {
    let defs = format!("{}\n{}", SHOW, SHOW_HERE);
    assert_eq!(show_after(&defs, "(macrolet ((sq (x) `(* ,x ,x))) (show-here (sq 4)))"), "(* 4 4)");
    assert_eq!(show_after(&defs, "(macrolet ((sq (x) `(* ,x ,x))) (show (sq 4)))"), "(sq 4)");
    assert_eq!(show_after(&defs, "(symbol-macrolet ((p (car q))) (show-here p))"), "(car q)");
    assert_eq!(show_after(&defs, "(symbol-macrolet ((p (car q))) (show p))"), "p");
}

/// A local binding hides a module-level macro in the environment as it does
/// where the form is checked.
#[test]
fn a_local_variable_hides_a_macro_in_the_environment() {
    let defs = format!("(defmacro twice (x) `(+ ,x ,x))\n{}", SHOW_HERE);
    assert_eq!(show_after(&defs, "(let ((twice 1)) (show-here (twice 2)))"), "(twice 2)");
    assert_eq!(show_after(&defs, "(show-here (twice 2))"), "(+ 2 2)");
}

/// `&environment` may stand anywhere in the lambda list, and takes no
/// argument of the call.
#[test]
fn the_environment_parameter_takes_no_argument() {
    assert_eq!(
        show_after("(defmacro m (a &environment e &optional (b ''z)) `(list ',a ',b))", "(m x)"),
        "(x (quote z))"
    );
    assert_eq!(show_after("(defmacro m (a &environment e &optional (b ''z)) `(list ',a ',b))", "(m x y)"), "(x y)");
    let err = check_error("(defmacro m (&environment) 1)");
    assert!(err.contains("&environment must be followed by a parameter name"), "{}", err);
}

/// An environment is good only while its macro runs, as CL's is: one kept
/// past it is refused, not followed.
#[test]
fn an_environment_kept_past_its_expansion_is_refused() {
    let defs = "(defmacro twice (x) `(+ ,x ,x))\n\
                (defvar (kept Option<std::eval::Environment>) (None))\n\
                (defmacro keep (&environment env) (setf kept (Some env)) ''ok)\n\
                (keep)";
    let out = show_after(defs, "(macroexpand '(twice 1) (match kept ((Some e) e) ((None) (panic \"none\"))))");
    assert!(out.contains("macro expansion that has ended"), "{}", out);
}

// ---- `define-symbol-macro` ----------------------------------------------------

#[test]
fn a_global_symbol_macro_is_its_form() {
    assert_eq!(show_after("(define-symbol-macro answer (+ 40 2))", "answer"), "42");
    assert_eq!(show_after("(define-symbol-macro answer (+ 40 2))", "(macroexpand-1 'answer)"), "(Ok (+ 40 2))");
    // A local binding of the name hides it.
    assert_eq!(show_after("(define-symbol-macro answer (+ 40 2))", "(let ((answer 1)) answer)"), "1");
}

/// The expansion is checked where the name is written, so it sees what is
/// defined after the symbol macro is.
#[test]
fn a_global_symbol_macro_is_a_place() {
    let defs = "(define-symbol-macro c cell)\n(defvar (cell int) 0)";
    assert_eq!(show_after(defs, "(progn (setf c 5) (incf c) cell)"), "6");
}

#[test]
fn a_global_symbol_macro_is_a_module_level_name() {
    let defs = "(module mm (pub define-symbol-macro three 3) (define-symbol-macro four 4) (pub defun h () int four))";
    assert_eq!(show_after(defs, "(list mm::three (mm::h))"), "(3 4)");
    let err = check_error(&format!("{}\nmm::four", defs));
    assert!(err.contains("unresolved path: mm::four"), "{}", err);
    let err = check_error("(define-symbol-macro c 1)\n(defun c () int 1)");
    assert!(err.contains("a symbol macro of that name already exists here"), "{}", err);
    let err = check_error("(define-symbol-macro c 1)\n(c)");
    assert!(err.contains("is a symbol macro, which names a form and is not called"), "{}", err);
}
