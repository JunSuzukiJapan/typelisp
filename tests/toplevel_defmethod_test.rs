//! `defmethod` outside an `impl`: a method of its first parameter's type,
//! which may be any type with methods — one the standard library defines, or
//! another module's. Methods are looked up from the first argument's type
//! alone, so one name can have a method on each type.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

/// `body` (a `string`-valued expression) after `defs`, interpreted and
/// inside a compiled function, both expected to give `expected`.
fn both(defs: &str, body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", defs, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("{}\n(defun go () string {})\n(compile go)\n(go)", defs, body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

/// One name, a method on each of two standard library types, chosen by the
/// first argument's type.
#[test]
fn one_name_on_two_standard_library_types() {
    let defs = r##"
(defmethod some-method ((x int) (y int)) int (+ x y))
(defmethod some-method ((x f64) (y f64)) f64 (* x y))
"##;
    both(defs, r##"(format false "~a ~a" (some-method 2 3) (some-method 2.0 3.0))"##, "5 6.0");
}

/// A program's own type and a generic standard library type.
#[test]
fn on_a_user_type_and_a_generic_type() {
    let defs = r##"
(defstruct dog (pub name string))
(defmethod hello ((d dog) (to string)) string (append d::name " greets " to))
(defmethod second-elem ((v Vector<T>)) T (get v 1))
"##;
    both(
        defs,
        r##"(format false "~a / ~a" (hello (make-instance 'dog :name "pochi") "tama") (second-elem #(1 2 3)))"##,
        "pochi greets tama / 2",
    );
}

/// `pub defmethod` is usable from another module; without `pub` it is not.
#[test]
fn pub_decides_whether_another_module_sees_it() {
    let src = r##"
(module m
  (pub defmethod twice ((x int)) int (* 2 x))
  (defmethod thrice ((x int)) int (* 3 x)))
(format false "~a" (twice 21))
"##;
    assert_eq!(eval_string(src), "42");
    let msg = check_err(
        r##"
(module m (defmethod thrice ((x int)) int (* 3 x)))
(defun f () int (thrice 1))
"##,
    );
    assert!(msg.contains("thrice"), "{}", msg);
}

/// `self` belongs to an `impl`; outside one the receiver is named and typed.
#[test]
fn self_is_refused_outside_an_impl() {
    for src in ["(defmethod f (self) int 1)", "(defmethod f ((self int)) int 1)"] {
        let msg = check_err(src);
        assert!(msg.contains("`self` is written only inside an `impl`"), "{}: {}", src, msg);
    }
}

/// The first parameter is the receiver and must say its type.
#[test]
fn the_receiver_needs_a_type() {
    for src in ["(defmethod f () int 1)", "(defmethod f (x) int 1)"] {
        let msg = check_err(src);
        assert!(msg.contains("defmethod `f`"), "{}: {}", src, msg);
    }
}
