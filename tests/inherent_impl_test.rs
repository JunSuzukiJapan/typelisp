//! Inherent `impl`: `(impl Type item...)` gives a type its methods. An item
//! whose first parameter is `self` is an instance method; any other is a
//! static method, called `(Type::name args...)`.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

const DEFS: &str = r##"
(defstruct Point (pub x f64) (pub y f64))
(impl Point
  (pub at ((x f64) (y f64)) Point (make-instance 'Point :x x :y y))
  (pub origin () Self (make-instance 'Point :x 0.0 :y 0.0))
  (pub area (self) f64 (* self::x self::y))
  (pub scaled ((self Self) (k f64) &optional (dk f64 0.0)) Point
    (make-instance 'Point :x (* self::x (+ k dk)) :y (* self::y k))))
(defstruct Box<T> (v T))
(impl Box<T> (where (print-object T))
  (pub show (self) string (format false "box:~a" (print-object self::v false)))
  (pub map<U> (self (f (fn (T) U))) Box<U> (make-instance 'Box :v (f self::v))))
"##;

/// `body` (a `string`-valued expression) after [`DEFS`], interpreted and
/// inside a compiled function, both expected to give `expected`.
fn both(body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", DEFS, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("{}\n(defun go () string {})\n(compile go)\n(go)", DEFS, body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

#[test]
fn static_and_instance_methods() {
    both(r##"(format false "~a ~a" (area (Point::origin)) (area (Point::at 2.0 3.0)))"##, "0.0 6.0");
}

/// A bare `self` is `(self Self)`; `Self` is the target type; an inherent
/// method's parameters may be optional, with a default.
#[test]
fn self_and_optional_parameters() {
    both(
        r##"(format false "~a ~a" (area (scaled (Point::at 2.0 3.0) 2.0)) (area (scaled (Point::at 2.0 3.0) 2.0 1.0)))"##,
        "24.0 36.0",
    );
}

/// A generic target, an impl-level `where`, and a method's own type parameter.
#[test]
fn a_generic_target_with_a_bound_and_a_method_type_parameter() {
    both(
        r##"(show (map (make-instance 'Box :v 3) (lambda ((n int)) string (format false "<~a>" n))))"##,
        "box:<3>",
    );
}

/// An item without `pub` is usable only inside the type's module.
#[test]
fn an_item_without_pub_is_private_to_its_module() {
    let src = r##"
(module geo
  (pub defstruct pt (pub x int))
  (impl pt
    (pub mk ((x int)) pt (make-instance 'pt :x x))
    (hidden ((x int)) pt (make-instance 'pt :x x))))
(format false "~a" (x (geo::pt::mk 4)))
"##;
    assert_eq!(eval_string(src), "4");
    let msg = check_err(
        r##"
(module geo
  (pub defstruct pt (pub x int))
  (impl pt (hidden ((x int)) pt (make-instance 'pt :x x))))
(defun f () geo::pt (geo::pt::hidden 1))
"##,
    );
    assert!(msg.contains("hidden"), "{}", msg);
}

/// The standard library is one crate and the program another: a program
/// cannot add methods to a standard library type.
#[test]
fn a_program_cannot_impl_a_standard_library_type() {
    for target in ["Vector<T>", "HashSet<T>", "int", "Option<T>"] {
        let msg = check_err(&format!("(impl {} (pub twice (self) int 2))", target));
        assert!(msg.contains("is a standard library type"), "{}: {}", target, msg);
    }
}

#[test]
fn an_item_must_have_a_parameter_list_and_a_return_type() {
    let msg = check_err("(defstruct p (x int))\n(impl p (pub f))");
    assert!(msg.contains("(pub? name (params...) ReturnType body...)"), "{}", msg);
}
