//! Tests for the type checker: Sexpr -> typed AST + checking (step 3b/3c).
//!
//! Built-in data types `Option<T>` and `Sexpr` are pre-registered. The minimal
//! goal is that `match` over them type-checks: well-typed forms pass, and
//! non-exhaustive / type-mismatched forms are rejected.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, Expr, Heap, Interp, Path, Reader, TopLevel, Type, Typed, MONO_BUNDLE_MODULE};

/// Peels the checker's synthetic monomorphization bundle (if any), returning
/// the primary form — always the bundle's *last* element (the
/// specializations it needs come first). A form that instantiates no generic
/// function is returned unchanged.
fn primary(tl: TopLevel) -> TopLevel {
    match tl {
        TopLevel::Module { path, mut body } if path == Path::root(MONO_BUNDLE_MODULE) => {
            body.pop().expect("a monomorph bundle always ends with its primary form")
        }
        other => other,
    }
}

/// Read one datum and check it as a single top-level form.
fn form(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(1024);
    let r = Reader::new();
    let v = r.read(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    chk.check_form(&mut h, &interp, v)
}

/// Check a sequence of forms (e.g. a defun followed by a use of it), returning
/// the result of the LAST form. Earlier forms (defuns) populate the registry.
fn program(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v)?);
    }
    Ok(last.expect("no forms"))
}

/// Like [`program`], but with the prelude loaded first — needed for `while`/
/// `dotimes`/`dolist`/`when`/`unless`/`and`/`or`/`cond`/`if-let`, which are
/// `defmacro`s in `src/prelude.rs` rather than checker-native special forms
/// (see that file's "loop/branch primitive reduction" comment).
fn program_with_prelude(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v)?);
    }
    Ok(last.expect("no forms"))
}

/// The synthesized type of a single expression form.
fn ty(src: &str) -> Type {
    match form(src).expect("check failed") {
        TopLevel::Expr(t) => t.ty,
        other => panic!("expected expression, got {:?}", other),
    }
}

/// Like [`ty`], but with the prelude loaded first — see [`program_with_prelude`].
fn ty_with_prelude(src: &str) -> Type {
    match primary(program_with_prelude(src).expect("check failed")) {
        TopLevel::Expr(t) => t.ty,
        other => panic!("expected expression, got {:?}", other),
    }
}

/// Assert that checking `src` fails with a `TypeError`.
fn assert_type_error(src: &str) {
    match form(src) {
        Err(Error::TypeError(_)) => {}
        other => panic!("expected TypeError, got {:?}", other),
    }
}

/// Like [`assert_type_error`], but with the prelude loaded first — see
/// [`program_with_prelude`].
fn assert_type_error_with_prelude(src: &str) {
    match program_with_prelude(src) {
        Err(Error::TypeError(_)) => {}
        other => panic!("expected TypeError, got {:?}", other),
    }
}

// ---- literals ---------------------------------------------------------------

#[test]
fn literal_types() {
    assert_eq!(ty("42"), Type::I32); // integer literals default to i32
    assert_eq!(ty("3.14"), Type::F64);
    assert_eq!(ty("true"), Type::Bool);
    assert_eq!(ty("#\\a"), Type::Char);
    assert_eq!(ty("\"hi\""), Type::Str);
    assert_eq!(ty("()"), Type::Unit);
}

// ---- if ---------------------------------------------------------------------

#[test]
fn if_expression() {
    assert_eq!(ty("(if true 1 2)"), Type::I32);
}

#[test]
fn if_condition_must_be_bool() {
    assert_type_error("(if 1 2 3)");
}

#[test]
fn if_branches_must_agree() {
    assert_type_error("(if true 1 true)");
}

// ---- let --------------------------------------------------------------------

#[test]
fn let_binding_infers_and_uses() {
    assert_eq!(ty("(let ((x 1)) x)"), Type::I32);
    assert_eq!(ty("(let ((x true) (y 2)) y)"), Type::I32);
}

#[test]
fn unbound_variable_errors() {
    assert_type_error("nope");
}

// ---- defun + call -----------------------------------------------------------

#[test]
fn defun_registers_and_checks_body() {
    match program("(defun id ((x i32)) i32 x)").unwrap() {
        TopLevel::Defun { name, ret, .. } => {
            assert_eq!(name, Path::root("id"));
            assert_eq!(ret, Type::I32);
        }
        other => panic!("expected defun, got {:?}", other),
    }
}

#[test]
fn defun_body_must_match_return_type() {
    assert!(matches!(
        program("(defun bad ((x i32)) bool x)"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn call_checks_argument_and_result_types() {
    assert_eq!(ty_program("(defun id ((x i32)) i32 x) (id 7)"), Type::I32);
}

#[test]
fn call_rejects_wrong_argument_type() {
    assert!(matches!(
        program("(defun id ((x i32)) i32 x) (id true)"),
        Err(Error::TypeError(_))
    ));
}

/// Like `ty` but over a multi-form program (last form's type).
fn ty_program(src: &str) -> Type {
    match primary(program(src).expect("check failed")) {
        TopLevel::Expr(t) => t.ty,
        other => panic!("expected expression, got {:?}", other),
    }
}

// ---- constructors -----------------------------------------------------------

#[test]
fn construct_some_infers_type_argument() {
    assert_eq!(ty("(option::some 1)"), Type::Named(Path::root("option"), vec![Type::I32]));
}

#[test]
fn construct_cons_yields_sexpr() {
    // (Cons (Int 1) ()) : car and cdr are both Sexpr; `()` adopts `Nil`.
    let sexpr = Type::Named(Path::root("sexpr"), vec![]);
    assert_eq!(ty("(Cons (Int 1) ())"), sexpr);
}

#[test]
fn nil_constructs_sexpr() {
    let sexpr = Type::Named(Path::root("sexpr"), vec![]);
    assert_eq!(ty("(Nil)"), sexpr);
}

#[test]
fn empty_list_as_sexpr_is_nil() {
    // `()` adopts `Sexpr::Nil` when a Sexpr is expected, just like it adopts
    // `Option::None` when an Option<T> is expected.
    assert_eq!(ty("(Cons () ())"), ty("(Cons (Nil) (Nil))"));
}

#[test]
fn construct_int_field_adopts_i64() {
    // Sexpr::Int holds an i64; the integer literal must adopt that type.
    assert_eq!(ty("(Int 5)"), Type::Named(Path::root("sexpr"), vec![]));
}

// ---- match ------------------------------------------------------------------

#[test]
fn match_option_exhaustive_unwrap_or() {
    // The canonical minimal goal: unwrap-or over Option<i32> type-checks.
    let src = "(defun unwrap-or ((opt Option<i32>) (default i32)) i32 \
                 (match opt \
                   ((Some v) v) \
                   ((None) default)))";
    match program(src).unwrap() {
        TopLevel::Defun { name, ret, .. } => {
            assert_eq!(name, Path::root("unwrap-or"));
            assert_eq!(ret, Type::I32);
        }
        other => panic!("expected defun, got {:?}", other),
    }
}

#[test]
fn match_non_exhaustive_is_rejected() {
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (match opt ((Some v) v)))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
}

#[test]
fn match_wildcard_makes_exhaustive() {
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (match opt ((Some v) v) (_ 0)))";
    assert!(matches!(program(src), Ok(TopLevel::Defun { .. })));
}

#[test]
fn match_arms_must_share_result_type() {
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (match opt ((Some v) v) ((None) true)))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
}

#[test]
fn match_binds_constructor_fields() {
    // `v` is bound at i32 inside the Some arm, so returning it as i32 is fine.
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (match opt ((Some v) v) ((None) 0)))";
    assert!(matches!(program(src), Ok(TopLevel::Defun { .. })));
}

#[test]
fn match_scrutinee_must_be_a_data_type() {
    assert_type_error("(match 1 (_ 0))");
}

// ---- if-let -----------------------------------------------------------------

#[test]
fn if_let_binds_in_then_branch() {
    // if-let binding is `(pattern value)`: here pattern `(Some v)`, value `opt`.
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (if-let ((Some v) opt) v 0))";
    assert!(matches!(program_with_prelude(src), Ok(TopLevel::Defun { .. })));
}

#[test]
fn if_let_branches_must_agree() {
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (if-let ((Some v) opt) v true))";
    assert!(matches!(program_with_prelude(src), Err(Error::TypeError(_))));
}

// ---- setf / while -----------------------------------------------------------

#[test]
fn setf_checks_against_variable_type() {
    assert_eq!(ty("(let ((x 0)) (setf x 9))"), Type::I32);
    assert_type_error("(let ((x 0)) (setf x true))");
}

#[test]
fn setf_unbound_variable_errors() {
    assert_type_error("(setf nope 1)");
}

#[test]
fn cannot_assign_to_constant() {
    let mut h = typelisp::Heap::with_capacity(1024);
    let r = Reader::new();
    let vs = r.read_all(&mut h, "(defconstant (k i32) 5) (setf k 6)").unwrap();
    let mut chk = Checker::new();
    let interp = typelisp::Interp::new();
    let mut result = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e);
        }
    }
    assert!(matches!(result, Err(Error::TypeError(_))));
}

#[test]
fn while_condition_must_be_bool_and_is_unit() {
    assert_eq!(ty_with_prelude("(let ((i 0)) (while (< i 0) (setf i 1)))"), Type::Unit);
    assert_type_error_with_prelude("(let ((i 0)) (while 1 (setf i 1)))");
}

// ---- lambda -----------------------------------------------------------------

#[test]
fn lambda_has_function_type() {
    assert_eq!(
        ty("(lambda ((x i32)) i32 x)"),
        Type::Fn(vec![Type::I32], None, Box::new(Type::I32))
    );
}

#[test]
fn calling_a_non_function_errors() {
    assert_type_error("(let ((x 5)) (x 1))");
}

#[test]
fn apply_checks_argument_types() {
    assert_type_error("((lambda ((x i32)) i32 x) true)");
}

#[test]
fn named_function_has_function_type() {
    assert_eq!(
        ty_program("(defun inc ((x i32)) i32 (+ x 1)) inc"),
        Type::Fn(vec![Type::I32], None, Box::new(Type::I32))
    );
}

#[test]
fn dotimes_count_must_be_i32() {
    assert_type_error_with_prelude("(dotimes (i true) ())");
}

// ---- labels -------------------------------------------------------------------

#[test]
fn labels_function_has_function_type_in_its_own_body() {
    // `fact` calling itself recursively type-checks, proving its own name is
    // visible (with a function type) inside its own body — the gap a bare
    // `lambda` can't close.
    assert_eq!(
        ty("(labels ((fact ((n i32)) i32 (if (= n 0) 1 (* n (fact (- n 1)))))) fact)"),
        Type::Fn(vec![Type::I32], None, Box::new(Type::I32))
    );
}

#[test]
fn labels_rejects_a_call_with_the_wrong_argument_type() {
    assert_type_error("(labels ((f ((n i32)) i32 n)) (f true))");
}

#[test]
fn break_does_not_cross_labels_boundary() {
    assert_type_error("(loop (labels ((f () () (break))) (f)))");
}

// ---- &rest / apply (variadic functions) --------------------------------------

#[test]
fn defun_rest_has_a_variadic_function_type() {
    assert_eq!(
        ty_program("(defun f ((a i32) &rest (xs i32)) i32 a) f"),
        Type::Fn(vec![Type::I32], Some(Box::new(Type::I32)), Box::new(Type::I32))
    );
}

#[test]
fn defun_rest_with_no_fixed_params_has_a_variadic_function_type() {
    assert_eq!(
        ty_program("(defun f (&rest (xs i32)) i32 0) f"),
        Type::Fn(vec![], Some(Box::new(Type::I32)), Box::new(Type::I32))
    );
}

#[test]
fn lambda_rest_has_a_variadic_function_type() {
    assert_eq!(
        ty("(lambda ((a i32) &rest (xs i32)) i32 a)"),
        Type::Fn(vec![Type::I32], Some(Box::new(Type::I32)), Box::new(Type::I32))
    );
}

#[test]
fn rest_param_is_seen_as_a_sexpr_inside_the_body() {
    // `car` only accepts a `Sexpr` argument — type-checking succeeds,
    // proving `xs` is bound to plain `Sexpr` (an ordinary Lisp list) inside
    // the body, not some homogeneous array type.
    assert!(matches!(
        form("(defun f ((a i32) &rest (xs i32)) Sexpr (car xs))"),
        Ok(TopLevel::Defun { .. })
    ));
}

#[test]
fn calling_a_rest_function_with_only_the_fixed_arguments_is_fine() {
    assert!(program("(defun f ((a i32) &rest (xs i32)) i32 a) (f 1)").is_ok());
}

#[test]
fn calling_a_rest_function_with_extra_arguments_is_fine() {
    assert!(program("(defun f ((a i32) &rest (xs i32)) i32 a) (f 1 2 3)").is_ok());
}

#[test]
fn calling_a_rest_function_with_too_few_fixed_arguments_is_a_type_error() {
    assert!(matches!(
        program("(defun f ((a i32) &rest (xs i32)) i32 a) (f)"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn calling_a_rest_function_with_a_wrong_typed_extra_argument_is_a_type_error() {
    assert!(matches!(
        program("(defun f ((a i32) &rest (xs i32)) i32 a) (f 1 true)"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn rest_must_be_followed_by_exactly_one_parameter_in_a_defun() {
    assert!(matches!(
        program("(defun f (&rest (xs i32) (y i32)) i32 0)"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn generic_rest_function_infers_the_element_type() {
    assert_eq!(ty_program("(defun (firstn T) ((a T) &rest (xs T)) T a) (firstn 1 2 3)"), Type::I32);
}

#[test]
fn apply_calls_a_variadic_function_with_a_runtime_sexpr_list() {
    let src = "(defun f ((a i32) &rest (xs i32)) i32 a) \
               (apply f 1 (quote (2 3)))";
    assert_eq!(ty_program(src), Type::I32);
}

#[test]
fn apply_on_a_non_variadic_function_is_a_type_error() {
    assert!(matches!(
        program("(apply (lambda ((a i32)) i32 a) 1)"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn apply_with_the_wrong_number_of_fixed_arguments_is_a_type_error() {
    // The lambda needs exactly one fixed argument (`a`) before the rest
    // list; this supplies zero.
    assert!(matches!(
        program("(apply (lambda ((a i32) &rest (xs i32)) i32 a) (quote ()))"),
        Err(Error::TypeError(_))
    ));
}

#[test]
fn apply_with_a_non_sexpr_rest_argument_is_a_type_error() {
    assert!(matches!(
        program("(apply (lambda ((a i32) &rest (xs i32)) i32 a) 1 2)"),
        Err(Error::TypeError(_))
    ));
}

// ---- cons / car / cdr / list / dolist ----------------------------------------

#[test]
fn car_and_cdr_yield_sexpr() {
    let sexpr = Type::Named(Path::root("sexpr"), vec![]);
    assert_eq!(ty("(car (Cons (Int 1) (Nil)))"), sexpr.clone());
    assert_eq!(ty("(cdr (Cons (Int 1) (Nil)))"), sexpr);
}

#[test]
fn car_argument_must_be_sexpr() {
    assert_type_error("(car 1)");
}

#[test]
fn cons_usable_as_function_value() {
    let sexpr = Type::Named(Path::root("sexpr"), vec![]);
    let src = "(defun apply2 ((f (fn (Sexpr Sexpr) Sexpr)) (a Sexpr) (b Sexpr)) Sexpr (f a b)) \
               (apply2 cons (Int 1) (Nil))";
    assert_eq!(ty_program(src), sexpr);
}

#[test]
fn list_builds_sexpr_cons_chain() {
    let sexpr = Type::Named(Path::root("sexpr"), vec![]);
    assert_eq!(ty("(list (Int 1) (Int 2))"), sexpr.clone());
    assert_eq!(ty("(list)"), sexpr);
}

#[test]
fn list_elements_must_be_sexpr() {
    assert_type_error("(list 1 2)");
}

#[test]
fn dolist_var_is_sexpr_and_result_is_unit() {
    assert_eq!(ty_with_prelude("(dolist (x (list (Int 1) (Int 2))) x)"), Type::Unit);
}

#[test]
fn dolist_list_expr_must_be_sexpr() {
    assert_type_error_with_prelude("(dolist (x 5) x)");
}

// ---- loop / break / return ---------------------------------------------------

#[test]
fn loop_with_only_break_is_unit() {
    assert_eq!(ty("(loop (break))"), Type::Unit);
}

#[test]
fn loop_with_return_value_takes_that_type() {
    assert_eq!(ty("(loop (return 5))"), Type::I32);
}

#[test]
fn loop_with_no_exit_is_never() {
    assert_eq!(ty("(loop 1)"), Type::Never);
}

#[test]
fn loop_break_and_return_must_agree() {
    assert_type_error("(if true (loop (break)) (loop (return 5)))");
}

#[test]
fn break_outside_loop_errors() {
    assert_type_error("(break)");
}

#[test]
fn return_outside_loop_errors() {
    assert_type_error("(return 1)");
}

#[test]
fn break_takes_no_arguments() {
    assert_type_error("(loop (break 1))");
}

#[test]
fn return_inside_while_must_be_unit() {
    assert_type_error_with_prelude("(while true (return 1))");
    assert_eq!(ty_with_prelude("(while true (return))"), Type::Unit);
}

#[test]
fn break_does_not_cross_lambda_boundary() {
    assert_type_error("(loop ((lambda () () (break))))");
}

#[test]
fn nested_loop_break_targets_innermost() {
    // The inner loop's `break` exits the inner loop only; it must not
    // contribute to the outer loop's exit type (which here comes solely from
    // the outer `return`). If it leaked, this would be a type error (Unit vs
    // i32).
    assert_eq!(ty("(loop (loop (break)) (return 5))"), Type::I32);
}

// ---- typed AST shape --------------------------------------------------------

#[test]
fn typed_ast_records_expr_and_type() {
    match form("(if true 1 2)").unwrap() {
        TopLevel::Expr(Typed { expr: Expr::If(_, _, _), ty }) => assert_eq!(ty, Type::I32),
        other => panic!("unexpected: {:?}", other),
    }
}

// ---- the (type annotation) ---------------------------------------------------

#[test]
fn the_overrides_an_integer_literals_default_type() {
    assert_eq!(ty("(the i64 5)"), Type::I64);
    assert_eq!(ty("(the f32 1.5)"), Type::F32);
}

#[test]
fn the_produces_the_same_expr_as_its_inner_form() {
    // `the` contributes no AST node of its own — checking `(the i32 5)`
    // yields exactly the same `Expr::Int` the bare literal would.
    match form("(the i32 5)").unwrap() {
        TopLevel::Expr(Typed { expr: Expr::Int(5), ty: Type::I32 }) => {}
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn the_mismatch_is_a_type_error() {
    assert_type_error("(the bool 5)");
}

#[test]
fn the_rejects_wrong_arity() {
    assert_type_error("(the i32)");
    assert_type_error("(the i32 5 6)");
}

#[test]
fn the_pins_a_generic_calls_type_argument() {
    // `identity` is a generic `defun` (T -> T); annotating its argument's
    // type with `the` is enough to resolve `T`, the same as an outer
    // `expected` type would.
    assert_eq!(ty_with_prelude("(identity (the i64 5))"), Type::I64);
}

// ---- exit ---------------------------------------------------------------------

#[test]
fn exit_type_checks_as_never() {
    // `exit`'s actual process termination can only be observed
    // out-of-process — see `tests/exit_test.rs`. This only checks the type
    // level: `Never` satisfies any expected type, like `panic`.
    match program("(defun f () i32 (if true 1 (exit 1)))").unwrap() {
        TopLevel::Defun { ret, .. } => assert_eq!(ret, Type::I32),
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn exit_rejects_wrong_arity() {
    assert_type_error("(exit)");
    assert_type_error("(exit 1 2)");
}

// ---- unreachable / todo (prelude macros) ---------------------------------------

#[test]
fn unreachable_and_todo_type_check_as_never() {
    match program_with_prelude("(defun f () i32 (if true 1 (unreachable)))").unwrap() {
        TopLevel::Defun { ret, .. } => assert_eq!(ret, Type::I32),
        other => panic!("unexpected: {:?}", other),
    }
    match program_with_prelude("(defun g () i32 (if true 1 (todo)))").unwrap() {
        TopLevel::Defun { ret, .. } => assert_eq!(ret, Type::I32),
        other => panic!("unexpected: {:?}", other),
    }
}
