//! `(make-instance 'Type :slot value...)`: CLOS's `make-instance`, the way a
//! `defstruct` value is built. Every case runs interpreted and compiled.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

const DEFS: &str = r##"
(defstruct Point (pub x f64) (pub y f64 0.5))
(defstruct Box<T> (v T))
(defstruct (Point3 (:include Point)) (pub z f64 9.0))
(defstruct Holder (pub o Option<int>) (pub n int))
"##;

/// `body` (a `string`-valued expression) after [`DEFS`], interpreted and
/// inside a compiled function, both expected to give `expected`.
fn both(body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", DEFS, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("{}\n(defun go () string {})\n(compile go)\n(go)", DEFS, body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

/// Slots are given by keyword, in any order.
#[test]
fn slots_are_given_by_keyword() {
    both(
        r##"(let ((p (make-instance 'Point :y 2.0 :x 1.0))) (format false "~a ~a" p::x p::y))"##,
        "1.0 2.0",
    );
}

/// A slot not given takes its default, including one inherited through
/// `:include`.
#[test]
fn a_slot_not_given_takes_its_default() {
    both(
        r##"(let ((p (make-instance 'Point :x 3.0)) (q (make-instance 'Point3 :x 1.0)))
             (format false "~a ~a ~a" p::y q::y q::z))"##,
        "0.5 0.5 9.0",
    );
}

/// A generic struct's type arguments come from the values or the expected
/// type, or are written in the quoted name.
#[test]
fn type_arguments_are_inferred_or_written() {
    both(
        r##"(format false "~s ~s ~s"
             (make-instance 'Box :v "s")
             (the Box<f64> (make-instance 'Box :v 1.5))
             (make-instance 'Box<int> :v 7))"##,
        r##"#<box<string> v: "s"> #<box<f64> v: 1.5> #<box<int> v: 7>"##,
    );
}

/// The values are evaluated once each, in the order written.
#[test]
fn values_are_evaluated_in_the_order_written() {
    both(
        r##"(let ((log ""))
             (make-instance 'Point :y (progn (setf log (format false "~ay" log)) 1.0)
                                   :x (progn (setf log (format false "~ax" log)) 2.0))
             log)"##,
        "yx",
    );
}

/// A value is typed by its slot, written in slot order or not: a bare
/// `(option::none)` needs the slot's type to know what it holds.
#[test]
fn a_value_is_typed_by_its_slot() {
    both(
        r##"(let ((a (make-instance 'Holder :o (option::none) :n 1))
                  (b (make-instance 'Holder :n 2 :o (option::none))))
             (format false "~s ~s" a::o b::n))"##,
        "none 2",
    );
}

#[test]
fn the_errors_name_the_slot() {
    let cases = [
        ("(make-instance 'Point :x 1.0 :x 2.0)", "slot `x` is given twice"),
        ("(make-instance 'Point :z 1.0)", "no slot `z` (its slots are `x`, `y`)"),
        ("(make-instance 'Point :y 1.0)", "slot `x` is not given and has no default"),
        ("(make-instance 'Point :x)", "`:x` has no value"),
        ("(make-instance 'Point 1.0)", "pairs of a slot keyword and a value"),
        ("(make-instance Point :x 1.0)", "(make-instance 'Type :slot value...)"),
        ("(make-instance 'int :x 1)", "`int` is not a struct type"),
        ("(make-instance 'Vector<int>)", "is not a struct type"),
    ];
    for (src, want) in cases {
        let msg = check_err(&format!("{}\n(defun f () () {} ())", DEFS, src));
        assert!(msg.contains(want), "{}: {}", src, msg);
    }
}

/// Outside the type's module only `pub` slots may be given; inside, any.
#[test]
fn a_slot_that_is_not_pub_is_given_only_inside_its_module() {
    let src = r##"
(module geo
  (pub defstruct pt (pub x int) (secret int 0))
  (pub defun inside () int (secret (make-instance 'pt :x 1 :secret 5))))
(format false "~a ~a" (geo::inside) (x (make-instance 'geo::pt :x 2)))
"##;
    assert_eq!(eval_string(src), "5 2");
    let msg = check_err(
        r##"
(module geo (pub defstruct pt (pub x int) (secret int 0)))
(defun f () geo::pt (make-instance 'geo::pt :x 1 :secret 2))
"##,
    );
    assert!(msg.contains("slot `secret` is not `pub`, so it can be given only inside module `geo`"), "{}", msg);
}
