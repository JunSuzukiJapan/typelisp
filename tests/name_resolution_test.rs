//! Name resolution: one rule, used everywhere a name is resolved.
//!
//! Functions, global variables and macros share one name space — a name has
//! one meaning, whichever kind it is.

mod common;
use common::{check_err, eval_ok};
use typelisp::Value;

/// A `defun` and a `defvar` in the same module may not share a name, in
/// either order.
#[test]
fn a_function_and_a_variable_in_one_module_cannot_share_a_name() {
    let msg = check_err("(defun f () int 1)\n(defvar (f int) 2)");
    assert!(msg.contains("cannot define variable `f`: a function of that name already exists here"), "{}", msg);
    let msg = check_err("(defvar (f int) 2)\n(defun f () int 1)");
    assert!(msg.contains("cannot define function `f`: a variable of that name already exists here"), "{}", msg);
    let msg = check_err("(defconstant (f int) 2)\n(defsignature f () int)");
    assert!(msg.contains("cannot define function `f`"), "{}", msg);
}

/// A variable in a nearer module hides a function of the same name further
/// out — in a call as well as in a value.
#[test]
fn a_nearer_variable_hides_a_function_further_out() {
    let src = "(defun f () int 1)
               (module m
                 (defvar (f int) 5)
                 (pub defun g () int f))
               (m::g)";
    assert_eq!(eval_ok(src), Value::Int(5));
    let msg = check_err(
        "(defun f () int 1)
         (module m
           (defvar (f int) 5)
           (defun g () int (f)))",
    );
    assert!(msg.contains("value is not callable: `int`"), "{}", msg);
}

/// And a function in a nearer module hides a variable further out.
#[test]
fn a_nearer_function_hides_a_variable_further_out() {
    let src = "(defvar (f int) 5)
               (module m
                 (defun f () int 1)
                 (pub defun g () int (f)))
               (m::g)";
    assert_eq!(eval_ok(src), Value::Int(1));
    let msg = check_err(
        "(defvar (f int) 5)
         (module m
           (defun f () int 1)
           (defun g () int f))",
    );
    assert!(msg.contains("expected `int`, found `(fn () int)`"), "{}", msg);
}

/// A `defmacro` shares the name space too: it may not share a name with a
/// `defun` or a `defvar` in the same module, in either order.
#[test]
fn a_macro_and_a_function_or_variable_in_one_module_cannot_share_a_name() {
    let msg = check_err("(defun f () int 1)\n(defmacro f (x) x)");
    assert!(msg.contains("cannot define macro `f`: a function of that name already exists here"), "{}", msg);
    let msg = check_err("(defmacro f (x) x)\n(defun f () int 1)");
    assert!(msg.contains("cannot define function `f`: a macro of that name already exists here"), "{}", msg);
    let msg = check_err("(defvar (f int) 2)\n(defmacro f (x) x)");
    assert!(msg.contains("cannot define macro `f`: a variable of that name already exists here"), "{}", msg);
    let msg = check_err("(defmacro f (x) x)\n(defvar (f int) 2)");
    assert!(msg.contains("cannot define variable `f`: a macro of that name already exists here"), "{}", msg);
}

/// A function in a nearer module hides a macro further out: inside `m`,
/// `(twice 5)` calls `m`'s function, not the root macro.
#[test]
fn a_nearer_function_hides_a_macro_further_out() {
    let src = "(defmacro twice (x) `(+ ,x ,x 1))
               (module m
                 (defun twice ((x int)) int (* x 2))
                 (pub defun g () int (twice 5)))
               (m::g)";
    assert_eq!(eval_ok(src), Value::Int(10));
}

/// And a macro in a nearer module hides a function further out: as a value,
/// `twice` inside `m` is the macro, which has none.
#[test]
fn a_nearer_macro_hides_a_function_further_out() {
    let msg = check_err(
        "(defun twice ((x int)) int (* x 2))
         (module m
           (defmacro twice (x) `(+ ,x ,x 1))
           (defun g () (fn (int) int) twice))",
    );
    assert!(msg.contains("`twice` is a macro, which has no value"), "{}", msg);
}

/// A `use` that imports a type says nothing about functions, globals or
/// macros of that name: the lookup goes on outward and finds the root
/// function, as a type lookup goes on past an import that names no type.
#[test]
fn an_import_of_a_type_does_not_hide_a_function_further_out() {
    let src = "(defun circle () int 7)
               (module geo (pub defstruct circle (r int)))
               (module m
                 (use geo::circle)
                 (pub defun g () int (circle)))
               (m::g)";
    assert_eq!(eval_ok(src), Value::Int(7));
}
