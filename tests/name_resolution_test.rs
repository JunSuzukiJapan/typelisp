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

/// A module's variable is what its name means there — in a call as well as
/// in a value — whatever the root defines under the name.
#[test]
fn a_modules_variable_is_what_its_name_means_there() {
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

/// And a module's function, whatever the root defines under the name.
#[test]
fn a_modules_function_is_what_its_name_means_there() {
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

/// A module's function is what its name means there even when the root
/// defines a macro of the name: inside `m`, `(twice 5)` calls `m`'s function.
#[test]
fn a_modules_function_is_what_its_name_means_over_a_root_macro() {
    let src = "(defmacro twice (x) `(+ ,x ,x 1))
               (module m
                 (defun twice ((x int)) int (* x 2))
                 (pub defun g () int (twice 5)))
               (m::g)";
    assert_eq!(eval_ok(src), Value::Int(10));
}

/// And a module's macro, over a root function of the name — in a call, where
/// it expands, and as a value, where a macro has none.
#[test]
fn a_modules_macro_is_what_its_name_means_over_a_root_function() {
    let src = "(defun twice ((x int)) int (* x 2))
               (module m
                 (defmacro twice (x) `(+ ,x ,x 1))
                 (pub defun g () int (twice 5)))
               (m::g)";
    assert_eq!(eval_ok(src), Value::Int(11));
    let msg = check_err(
        "(defun twice ((x int)) int (* x 2))
         (module m
           (defmacro twice (x) `(+ ,x ,x 1))
           (defun g () (fn (int) int) twice))",
    );
    assert!(msg.contains("`twice` is a macro, which has no value"), "{}", msg);
}
