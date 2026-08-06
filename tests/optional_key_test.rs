//! Tests for `defun`'s `&optional`/`&key` parameters.
//!
//! `Checker::check_defun_opt_key` registers each `&optional`/`&key`
//! parameter as a `check::registry::OptKeyParam` (its declared type plus a
//! checked default expression, if written); `Checker::check_call_opt_key`
//! resolves every call site — filling in a default or `Option::none` for
//! whatever's omitted, auto-wrapping a supplied value into `Some` for a
//! defaultless parameter — into an ordinary fully-saturated `Expr::Call`, so
//! the compile pipeline (exercised by the `_compiled` variants below) never
//! needs to know either section exists.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
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

/// A `defun`d function under test, `(compile name)`d before the final call —
/// same source, so `eval_ok`/`eval_ok_compiled` on identical inputs proves
/// the interp and compile paths agree. No `COMPILE_LOCK` here: `Interp`
/// already takes it internally for the `(compile ...)` form itself (see
/// `eval::interp`'s own `COMPILE_LOCK` call sites) — locking it again around
/// this whole call would self-deadlock on the same thread.
fn eval_ok_compiled(def_and_calls: &str, compile_name: &str) -> Value {
    let src = def_and_calls.replacen("%COMPILE%", &format!("(compile {})", compile_name), 1);
    eval_ok(&src)
}

/// A checker error's message, with any source-location wrapper stripped —
/// mirrors `check_test.rs`'s `Error::into_kind` usage.
fn check_err(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let interp = Interp::new();
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut result = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e);
            break;
        }
    }
    match result.map_err(Error::into_kind) {
        Err(Error::TypeError(msg)) => msg,
        other => panic!("expected a TypeError, got {:?}", other),
    }
}

// ---- &optional, with default ---------------------------------------------

#[test]
fn optional_default_is_used_when_the_argument_is_omitted() {
    let src = "(defun f ((a i32) &optional (b i32 10)) i32 (+ a b)) (f 1)";
    assert_eq!(eval_ok(src), Value::Int(11));
}

#[test]
fn optional_supplied_value_overrides_the_default() {
    let src = "(defun f ((a i32) &optional (b i32 10)) i32 (+ a b)) (f 1 5)";
    assert_eq!(eval_ok(src), Value::Int(6));
}

#[test]
fn optional_default_and_supplied_agree_between_interp_and_compile() {
    let def = "(defun f ((a i32) &optional (b i32 10)) i32 (+ a b)) %COMPILE% (f 1)";
    assert_eq!(eval_ok_compiled(def, "f"), Value::Int(11));
    let def2 = "(defun f ((a i32) &optional (b i32 10)) i32 (+ a b)) %COMPILE% (f 1 5)";
    assert_eq!(eval_ok_compiled(def2, "f"), Value::Int(6));
}

// ---- &optional, no default (Option<T>) ------------------------------------

#[test]
fn optional_without_default_binds_option_none_when_omitted() {
    let src = "(defun f (&optional (b i32)) i32 (unwrap-or b 99)) (f)";
    assert_eq!(eval_ok(src), Value::Int(99));
}

#[test]
fn optional_without_default_auto_wraps_a_supplied_value_into_some() {
    let src = "(defun f (&optional (b i32)) i32 (unwrap-or b 99)) (f 5)";
    assert_eq!(eval_ok(src), Value::Int(5));
}

// ---- &key, with default ----------------------------------------------------

#[test]
fn key_default_is_used_when_omitted() {
    let src = "(defun make-point (&key (x i32 0) (y i32 0)) i32 (+ x y)) (make-point)";
    assert_eq!(eval_ok(src), Value::Int(0));
}

#[test]
fn key_arguments_are_matched_by_label_not_position() {
    let src = "(defun make-point (&key (x i32 0) (y i32 0)) i32 (+ x (* 10 y))) (make-point :y 4 :x 3)";
    assert_eq!(eval_ok(src), Value::Int(43));
}

#[test]
fn key_default_and_supplied_agree_between_interp_and_compile() {
    let def = "(defun make-point (&key (x i32 0) (y i32 0)) i32 (+ x (* 10 y))) %COMPILE% (make-point :y 4 :x 3)";
    assert_eq!(eval_ok_compiled(def, "make-point"), Value::Int(43));
    let def2 = "(defun make-point (&key (x i32 0) (y i32 0)) i32 (+ x (* 10 y))) %COMPILE% (make-point)";
    assert_eq!(eval_ok_compiled(def2, "make-point"), Value::Int(0));
}

// ---- &key, no default (Option<T>) ------------------------------------------

#[test]
fn key_without_default_is_none_when_omitted_and_some_when_supplied() {
    // `test`'s declared type is `symbol` (no default), so its *effective*
    // type is `Option<symbol>`; a keyword literal like `:eq` is itself
    // statically typed `symbol` (`Checker::check_inner`'s `Value::Symbol`
    // case) — the plain value a caller supplies, auto-wrapped into `Some`.
    let omitted = eval_ok("(defun f (&key (test symbol)) bool (is-some test)) (f)");
    let supplied = eval_ok("(defun f (&key (test symbol)) bool (is-some test)) (f :test :eq)");
    assert_eq!(omitted, Value::Bool(false));
    assert_eq!(supplied, Value::Bool(true));
}

// ---- &key error cases -------------------------------------------------------

#[test]
fn key_call_with_an_unknown_keyword_is_a_type_error() {
    let msg = check_err("(defun f (&key (x i32 0)) i32 x) (f :y 1)");
    assert!(msg.contains("unknown keyword"), "unexpected message: {}", msg);
}

#[test]
fn key_call_with_an_odd_number_of_trailing_arguments_is_a_type_error() {
    let msg = check_err("(defun f (&key (x i32 0)) i32 x) (f :x)");
    assert!(msg.contains(":name value"), "unexpected message: {}", msg);
}

#[test]
fn key_call_with_a_duplicate_keyword_is_a_type_error() {
    let msg = check_err("(defun f (&key (x i32 0)) i32 x) (f :x 1 :x 2)");
    assert!(msg.contains("duplicate"), "unexpected message: {}", msg);
}

// ---- &optional + &rest ------------------------------------------------------

#[test]
fn optional_combines_with_a_trailing_rest() {
    let src = "(defun f ((a i32) &optional (b i32 10) &rest (xs i32)) i32 (+ a b)) (f 1 2 3 4 5)";
    assert_eq!(eval_ok(src), Value::Int(3));
}

// ---- forbidden combinations -------------------------------------------------

#[test]
fn key_cannot_combine_with_optional_in_the_same_parameter_list() {
    let msg = check_err("(defun f (&optional (a i32 1) &key (b i32 2)) i32 (+ a b))");
    assert!(msg.contains("&key cannot be combined"), "unexpected message: {}", msg);
}

#[test]
fn optional_is_rejected_on_lambda() {
    let msg = check_err("(defun caller () i32 (let ((g (lambda (&optional (a i32 1)) i32 a))) 0))");
    assert!(msg.contains("lambda"), "unexpected message: {}", msg);
}

// ---- generic defun + &optional/&key ----------------------------------------
//
// `Checker::check_defun_opt_key` and `Checker::check_call_opt_key` run the
// same `unify`/`subst_apply`/`validate_where_bounds`/monomorphization
// machinery `Checker::check_call` uses for an ordinary generic call,
// restricted to what a call site can actually observe (required arguments,
// plus whichever `&optional`/`&key` arguments it actually supplies — an
// omitted one contributes nothing to inference). See that method's doc
// comment, and `Checker::check_defun_opt_key`'s, for why a *defaulted*
// `&optional`/`&key` parameter's declared type may never mention the
// function's own type parameter.

#[test]
fn generic_key_infers_type_param_from_required_arg() {
    // `T` appears only on the required parameter `x`; the omitted `&key`
    // `y` (no default, so effectively `Option<T>`) must still resolve to
    // `Option<i32>` from that inference, not stay abstract.
    let src = "(defun f<T> ((x T) &key (y T)) T (unwrap-or y x)) (f 5)";
    assert_eq!(eval_ok(src), Value::Int(5));
}

#[test]
fn generic_key_infers_type_param_from_supplied_key_arg() {
    // `T` appears *only* on the `&key` parameter `y` here — inference can
    // only come from actually supplying it, not from `n` (plain `i32`).
    let src = "(defun f<T> ((n i32) &key (y T)) i32 (+ n (if (is-some y) 1 0))) (f 10 :y 42)";
    assert_eq!(eval_ok(src), Value::Int(11));
}

#[test]
fn generic_key_uninferrable_type_param_is_a_type_error() {
    // `T` appears only on the `&key` parameter `y` (unused in the body,
    // which is allowed — a type parameter need not be referenced); the call
    // omits `:y` entirely, so `T` cannot be inferred from anywhere.
    let msg = check_err("(defun f<T> ((n i32) &key (y T)) i32 n) (f 10)");
    assert!(msg.contains("cannot infer type parameter"), "unexpected message: {}", msg);
}

#[test]
fn generic_optional_where_bound_is_validated() {
    let prog = "
        (deftrait eq2 () (same ((self Self) (other Self)) bool))
        (impl eq2 i32 (same ((self Self) (other Self)) bool (= self other)))
        (defstruct no-eq (n i32))
        (defun check-eq<T> ((a T) (b T) &optional (verbose bool false)) bool (where (eq2 T)) (same a b))
    ";
    assert_eq!(eval_ok(&format!("{} (check-eq 1 1)", prog)), Value::Bool(true));
    assert_eq!(eval_ok(&format!("{} (check-eq 1 2)", prog)), Value::Bool(false));
    let msg = check_err(&format!(
        "{} (check-eq (no-eq::new 1) (no-eq::new 1))",
        prog
    ));
    assert!(msg.contains("does not implement trait"), "unexpected message: {}", msg);
}

#[test]
fn generic_key_default_referencing_type_param_is_rejected() {
    // `x`'s declared type `Option<T>` mentions `f`'s own type parameter, and
    // it has a default (`(option::none)`) — rejected by `Checker::
    // check_defun_opt_key`'s `type_has_param` restriction (see the module's
    // doc comment for why: a call site that omits `x` would splice this
    // default's checked `Typed` node in verbatim, and its `.ty` would stay
    // the abstract `Option<T>` instead of the call's concrete instantiation).
    let msg = check_err("(defun f<T> (&key (x Option<T> (option::none))) Option<T> x)");
    assert!(msg.contains("may not default"), "unexpected message: {}", msg);
}

#[test]
fn generic_key_specialization_agrees_between_interp_and_compile() {
    // The same generic `&key` function (`pick`), instantiated at two
    // different concrete types in two independent programs — each proves
    // `eval_ok_compiled` (which routes the defaulted `:use-a` value through
    // the compile pipeline's `ast_bridge`) agrees with plain interpretation
    // at *that* instantiation. This is the direct regression test for the
    // compile-side `.ty` safety issue found while planning this feature
    // (see `Checker::check_defun_opt_key`'s doc comment): `use-a`'s default
    // (`bool true`, declared type independent of `T`) is checked exactly
    // once, on the unspecialized signature, and spliced verbatim into every
    // instantiation's call site — safe only because its type can never
    // depend on `T`.
    let prog_i32 = "
        (defun pick<T> ((a T) (b T) &key (use-a bool true)) T (if use-a a b))
        (defun run-i32 () i32 (pick 1 2))
        %COMPILE%
        (run-i32)
    ";
    assert_eq!(eval_ok_compiled(prog_i32, "run-i32"), Value::Int(1));

    let prog_bool = "
        (defun pick<T> ((a T) (b T) &key (use-a bool true)) T (if use-a a b))
        (defun run-bool () bool (pick true false :use-a false))
        %COMPILE%
        (run-bool)
    ";
    assert_eq!(eval_ok_compiled(prog_bool, "run-bool"), Value::Bool(false));
}

#[test]
fn generic_optional_self_recursive() {
    // `rep` calls itself at the *same* type parameter (not a growing one),
    // exercising `FnTemplate` retention for a generic `&optional` defun —
    // the template must already be registered when the body is checked so
    // the self-recursive call resolves instead of hitting `check_redef` or
    // an unregistered-function error.
    let src = "(defun rep<T> ((x T) &optional (n i32 3)) T (if (= n 0) x (rep x (- n 1)))) (rep 5)";
    assert_eq!(eval_ok(src), Value::Int(5));
}
