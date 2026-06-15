//! Tests for the type checker: Sexpr -> typed AST + checking (step 3b/3c).
//!
//! Built-in data types `Option<T>` and `Sexpr` are pre-registered. The minimal
//! goal is that `match` over them type-checks: well-typed forms pass, and
//! non-exhaustive / type-mismatched forms are rejected.

extern crate typelisp;
use typelisp::{Checker, Error, Expr, Heap, Path, Reader, TopLevel, Type, Typed};

/// Read one datum and check it as a single top-level form.
fn form(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(1024);
    let r = Reader::new();
    let v = r.read(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    chk.check_form(&h, v)
}

/// Check a sequence of forms (e.g. a defun followed by a use of it), returning
/// the result of the LAST form. Earlier forms (defuns) populate the registry.
fn program(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(4096);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&h, v)?);
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

/// Assert that checking `src` fails with a `TypeError`.
fn assert_type_error(src: &str) {
    match form(src) {
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
    match program(src).expect("check failed") {
        TopLevel::Expr(t) => t.ty,
        other => panic!("expected expression, got {:?}", other),
    }
}

// ---- constructors -----------------------------------------------------------

#[test]
fn construct_some_infers_type_argument() {
    assert_eq!(ty("(Some 1)"), Type::Named(Path::root("option"), vec![Type::I32]));
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
    assert!(matches!(program(src), Ok(TopLevel::Defun { .. })));
}

#[test]
fn if_let_branches_must_agree() {
    let src = "(defun f ((opt Option<i32>)) i32 \
                 (if-let ((Some v) opt) v true))";
    assert!(matches!(program(src), Err(Error::TypeError(_))));
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
    let vs = r.read_all(&mut h, "(defconstant k 5) (setf k 6)").unwrap();
    let mut chk = Checker::new();
    let mut result = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&h, v) {
            result = Err(e);
        }
    }
    assert!(matches!(result, Err(Error::TypeError(_))));
}

#[test]
fn while_condition_must_be_bool_and_is_unit() {
    assert_eq!(ty("(let ((i 0)) (while (< i 0) (setf i 1)))"), Type::Unit);
    assert_type_error("(let ((i 0)) (while 1 (setf i 1)))");
}

// ---- lambda -----------------------------------------------------------------

#[test]
fn lambda_has_function_type() {
    assert_eq!(
        ty("(lambda ((x i32)) i32 x)"),
        Type::Fn(vec![Type::I32], Box::new(Type::I32))
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
        Type::Fn(vec![Type::I32], Box::new(Type::I32))
    );
}

#[test]
fn dotimes_count_must_be_i32() {
    assert_type_error("(dotimes (i true) ())");
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
    assert_eq!(ty("(dolist (x (list (Int 1) (Int 2))) x)"), Type::Unit);
}

#[test]
fn dolist_list_expr_must_be_sexpr() {
    assert_type_error("(dolist (x 5) x)");
}

// ---- typed AST shape --------------------------------------------------------

#[test]
fn typed_ast_records_expr_and_type() {
    match form("(if true 1 2)").unwrap() {
        TopLevel::Expr(Typed { expr: Expr::If(_, _, _), ty }) => assert_eq!(ty, Type::I32),
        other => panic!("unexpected: {:?}", other),
    }
}
