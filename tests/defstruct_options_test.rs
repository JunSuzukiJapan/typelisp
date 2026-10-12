//! Tests for `defstruct`'s option list: `(defstruct (Name option...) ...)`.
//!
//! `Checker::check_defstruct` synthesizes the copier as method **source**
//! and runs it through `Checker::check_defmethod_in`, which gives it
//! visibility and, for a generic owner, the `MethodTemplate::Form` retention
//! that makes it specializable.

extern crate typelisp;

mod common;
use common::eval_ok_compiled as eval_ok;
use typelisp::{load_prelude, Checker, Error, Heap, Interp, Reader, Value};

fn check_err(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut result = Ok(());
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            result = Err(e);
            break;
        }
    }
    match result.map_err(Error::into_kind) {
        Err(Error::TypeError(msg)) => msg,
        other => panic!("expected a TypeError, got {:?}", other),
    }
}

// ---- the option list itself -------------------------------------------------

#[test]
fn an_option_list_with_no_options_behaves_like_a_bare_name() {
    let src = "
        (defstruct (point) (x int) (y int))
        (x (make-instance 'point :x 3 :y 4))
    ";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn a_docstring_still_follows_the_option_list() {
    let src = "
        (defstruct (point) \"a place\" (x int) (y int))
        (match (documentation point) ((some s) (length s)) ((none) 0))
    ";
    assert_eq!(eval_ok(src), Value::Int(7));
}

// ---- :constructor ------------------------------------------------------------

#[test]
fn the_constructor_option_points_at_impl() {
    let msg = check_err("(defstruct (point (:constructor make-point)) (x int) (y int 0))");
    assert!(msg.contains("(:constructor ...) is gone") && msg.contains("(impl Point"), "unexpected message: {}", msg);
}

// ---- :copier -----------------------------------------------------------------

#[test]
fn a_copier_returns_an_independent_value() {
    let src = "
        (defstruct (point (:copier copy-point)) (x int) (y int))
        (let ((a (make-instance 'point :x 1 :y 2)))
          (let ((b (copy-point a)))
            (progn (set-x b 9) (+ (x a) (x b)))))
    ";
    assert_eq!(eval_ok(src), Value::Int(10));
}

// ---- :include ------------------------------------------------------------------

#[test]
fn include_prepends_the_parents_slots() {
    let src = "
        (defstruct base (id int))
        (defstruct (derived (:include base)) (extra int))
        (+ (id (make-instance 'derived :id 1 :extra 2)) (extra (make-instance 'derived :id 1 :extra 2)))
    ";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn include_carries_the_parents_slot_defaults() {
    let src = "
        (defstruct base (id int 7))
        (defstruct (derived (:include base)) (extra int))
        (id (make-instance 'derived :extra 1))
    ";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn include_rejects_a_duplicated_slot_name() {
    let src = "
        (defstruct base (id int))
        (defstruct (derived (:include base)) (id int))
    ";
    assert!(check_err(src).contains("duplicate field"), "{}", check_err(src));
}

#[test]
fn include_rejects_a_non_struct() {
    let msg = check_err("(defenum e (a) (b)) (defstruct (d (:include e)) (x int))");
    assert!(msg.contains("only a `defstruct` has slots"), "unexpected message: {}", msg);
}

#[test]
fn a_child_is_not_a_subtype_of_its_parent() {
    // `:include` copies the slot list and nothing else — there is no
    // subtyping in this language, so a `derived` is not a `base`.
    let src = "
        (defstruct base (id int))
        (defstruct (derived (:include base)) (extra int))
        (defun takes-base ((b base)) int (id b))
        (takes-base (make-instance 'derived :id 1 :extra 2))
    ";
    let msg = check_err(src);
    assert!(msg.contains("mismatch") || msg.contains("expected"), "unexpected message: {}", msg);
}

// ---- generic owners -------------------------------------------------------------

#[test]
fn a_generic_structs_copier_works() {
    let src = "
        (defstruct (cell<T> (:copier dup)) (v T))
        (v (dup (make-instance 'cell :v 8)))
    ";
    assert_eq!(eval_ok(src), Value::Int(8));
}

// ---- what is refused, and why ----------------------------------------------------

#[test]
fn conc_name_is_refused_with_its_reason() {
    let msg = check_err("(defstruct (point (:conc-name p-)) (x int))");
    assert!(msg.contains("nothing to do here"), "unexpected message: {}", msg);
}

#[test]
fn predicate_is_refused_with_its_reason() {
    let msg = check_err("(defstruct (point (:predicate pointp)) (x int))");
    assert!(msg.contains("nothing to answer"), "unexpected message: {}", msg);
}

#[test]
fn type_and_friends_are_refused_with_their_reason() {
    for opt in ["(:type list)", "(:initial-offset 1)", "(:named)"] {
        let msg = check_err(&format!("(defstruct (point {}) (x int))", opt));
        assert!(msg.contains("belongs to the compiler"), "unexpected message for {}: {}", opt, msg);
    }
}

#[test]
fn an_unknown_option_names_the_known_ones() {
    let msg = check_err("(defstruct (point (:nope 1)) (x int))");
    assert!(msg.contains(":copier, :include"), "unexpected message: {}", msg);
}

// ---- the compile path --------------------------------------------------------------

#[test]
fn slot_defaults_compile() {
    let src = "
        (defstruct point (x int 1) (y int 2))
        (defun run () int (let ((p (make-instance 'point :y 5))) (+ p::x p::y)))
        (compile run)
        (run)
    ";
    assert_eq!(eval_ok(src), Value::Int(6));
}
