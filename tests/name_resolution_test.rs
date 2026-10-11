//! Name resolution: one rule, used everywhere a name is resolved.
//!
//! Functions and global variables share one name space — a name has one
//! value, whichever kind it is.

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
