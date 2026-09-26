//! A written type that names nothing is an error where it is written.
//!
//! A bare type name `canon` cannot resolve looks exactly like a generic's type
//! variable, so it used to be accepted as one. `(defenum link (no-link) (to
//! node))` ahead of `(defstruct node ...)` therefore checked, and failed only
//! later, far away, as "expected `node`, found `main::node`" — two spellings
//! of the same name. The checker now knows which type variables each
//! definition binds, and anything else must be a visible type.
//!
//! Also here: type mismatches print types the way a program writes them, not
//! as the checker's internal representation.

extern crate typelisp;

mod common;
use common::{check_err, eval_ok};
use typelisp::{Path, Type, Value};

fn assert_unknown(src: &str, name: &str) {
    let err = check_err(src);
    assert!(err.contains(&format!("unknown type `{}`", name)), "src: {}\nerror: {}", src, err);
}

#[test]
fn a_type_defined_later_is_reported_by_name() {
    assert_unknown("(defenum link (no-link) (to node)) (defstruct node (v int) (next link))", "node");
}

#[test]
fn every_definition_that_writes_types_rejects_an_unknown_name() {
    assert_unknown("(defstruct a (x nosuch))", "nosuch");
    assert_unknown("(defenum e (v nosuch))", "nosuch");
    assert_unknown("(defun f ((x nosuch)) int 1)", "nosuch");
    assert_unknown("(defun f () nosuch 1)", "nosuch");
    assert_unknown("(defvar (x nosuch) 1)", "nosuch");
    assert_unknown("(deftype alias Vector<nosuch>)", "nosuch");
    assert_unknown("(defsignature f (nosuch) int)", "nosuch");
}

#[test]
fn an_unknown_name_inside_a_body_is_reported_too() {
    assert_unknown("(defun f () int (the nosuch 1))", "nosuch");
    assert_unknown("(defun f () int (let ((g (lambda ((x nosuch)) int 1))) 2))", "nosuch");
}

#[test]
fn type_variables_declared_in_the_name_are_not_unknown() {
    let src = "(defun first-of<T> ((v Vector<T>)) Option<T> (pop v))
               (defstruct box<T> (v T))
               (defenum may<T> (j T) (n))
               (deftype pair<A> cons-cell<A,A>)
               (defun g<T> ((x T)) int (let ((f (lambda ((y T)) int 1))) (f x)))
               (g (box::new 1))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn a_method_s_type_variables_are_the_ones_its_receiver_writes() {
    assert_unknown("(defstruct box<T> (v T)) (defmethod bad ((self box<T>) (x U)) T self::v)", "u");
    assert_unknown("(defstruct pt (x int)) (defmethod m ((self pt) (y nosuch)) int 1)", "nosuch");
    assert_unknown("(defstruct pt (x int)) (defmethod m ((self pt)) int (the nosuch 1))", "nosuch");
    assert_unknown("(defstruct box<T> (v T)) (defmethod mk (box<T> (x U)) box<T> (box::new x))", "u");
    let src = "(defstruct box<T> (v T))
               (defmethod get-v ((self box<T>)) T self::v)
               (defmethod wrap (box<T> (x T)) box<T> (box::new x))
               (get-v (box::wrap 7))";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn an_impl_s_type_variables_are_its_target_s_and_its_own() {
    let defs = "(defstruct box<T> (v T)) (deftrait Show () (show ((self Self)) string))";
    assert_unknown(&format!("{defs} (impl Show box<T> (show ((self Self)) string (let ((y (the U 1))) \"b\")))"), "u");
    assert_unknown(&format!("{defs} (impl<T> Show Vector<T> (show ((self Self)) string (let ((y (the U 1))) \"v\")))"), "u");
    let src = format!(
        "{defs}
         (impl Show box<T> (show ((self Self)) string (let ((x (the T self::v))) \"b\")))
         (deftrait Clamp (Ord) (clamp ((self Self) (lo Self) (hi Self)) Self
           (if (less self lo) lo (if (less hi self) hi self))))
         (impl<T> Clamp T (where (Ord T)))
         (+ (length (show (box::new 1))) (clamp 5 1 3))"
    );
    assert_eq!(eval_ok(&src), Value::Int(4));
}

#[test]
fn a_trait_s_type_variables_are_self_and_its_associated_types() {
    assert_unknown("(deftrait Shape () (type Unit2) (area ((self Self)) Unit2) (bad ((self Self)) Oops))", "oops");
}

#[test]
fn a_type_may_name_itself() {
    let src = "(defstruct node (v int) (next Option<node>))
               (let ((a (node::new 1 (Option::none)))) (progn (setf a::next (Option::some a)) a::v))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn a_mismatch_names_types_as_they_are_written() {
    let err = check_err("(defun f () int true)");
    assert!(err.contains("expected `int`, found `bool`"), "{}", err);
    let err = check_err("(defun f () Vector<int> (the Option<string> (Option::none)))");
    assert!(err.contains("expected `Vector<int>`, found `Option<string>`"), "{}", err);
}

#[test]
fn types_display_in_source_syntax() {
    let named = |n: &str, args: Vec<Type>| Type::Named(Path::root(n), args);
    assert_eq!(named("vector", vec![named("option", vec![Type::Int])]).to_string(), "Vector<Option<int>>");
    assert_eq!(named("hashtable", vec![Type::Str, Type::I32]).to_string(), "HashTable<string,i32>");
    assert_eq!(Type::Named(Path::of(&["geo", "point"]), vec![]).to_string(), "geo::point");
    assert_eq!(Type::Dyn(Path::root("error"), vec![]).to_string(), ":dyn error");
    assert_eq!(Type::Fn(vec![Type::Int, Type::Char], None, Box::new(Type::Bool)).to_string(), "(fn (int char) bool)");
    assert_eq!(Type::Fn(vec![], Some(Box::new(Type::Int)), Box::new(Type::Unit)).to_string(), "(fn (&rest int) ())");
}
