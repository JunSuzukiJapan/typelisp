//! `(shadow name...)`: a name the standard library offers every module reads
//! as the library's symbol, so a module that defines one of those names makes
//! it its own first.

mod common;
use common::{check_err, eval_ok};
use typelisp::{Heap, Reader, Value};

/// Defining an offered name without `shadow` is an error, whatever the
/// definition form — a type as well as a function.
#[test]
fn defining_an_offered_name_without_shadow_is_an_error() {
    let msg = check_err("(defstruct Error (message str))");
    assert!(msg.contains("defstruct: `error` is a name the standard library offers every module"), "{}", msg);
    assert!(msg.contains("(shadow error)"), "{}", msg);
    let msg = check_err("(defun map ((x int)) int x)");
    assert!(msg.contains("defun: `map` is a name the standard library offers every module"), "{}", msg);
    let msg = check_err("(defvar (length int) 3)");
    assert!(msg.contains("defvar: `length`"), "{}", msg);
    let msg = check_err("(defmacro when (c x) x)");
    assert!(msg.contains("defmacro: `when`"), "{}", msg);
}

/// After `shadow`, the name is the module's own: the definition is allowed,
/// and the module's uses of the name mean it.
#[test]
fn a_shadowed_name_can_be_defined_and_means_the_modules_own() {
    assert_eq!(eval_ok("(shadow map)\n(defun map ((x int)) int (* x 2))\n(map 21)"), Value::Int(42));
    assert_eq!(eval_ok("(shadow error)\n(defstruct Error (code int))\n(let ((e (make-instance 'Error :code 7))) e::code)"), Value::Int(7));
}

/// A module's `shadow` is that module's: outside it, the name is still the
/// library's.
#[test]
fn shadow_is_local_to_its_module() {
    let src = "(module m
                 (shadow abs)
                 (pub defun abs () int 7))
               (+ (m::abs) (abs -3))";
    assert_eq!(eval_ok(src), Value::Int(10));
}

/// A local binding may take an offered name without `shadow`: it is not a
/// definition of the module.
#[test]
fn a_local_binding_needs_no_shadow() {
    assert_eq!(eval_ok("(let ((map 5)) (+ map 1))"), Value::Int(6));
}

/// A reserved word is every module's, so no module can make it its own.
#[test]
fn a_reserved_word_cannot_be_shadowed() {
    let mut heap = Heap::with_capacity(1 << 12);
    let err = format!("{:?}", Reader::new().read_all(&mut heap, "(shadow defun)").expect_err("shadowing a reserved word must fail"));
    assert!(err.contains("shadow: `defun` is a reserved word"), "{}", err);
}
