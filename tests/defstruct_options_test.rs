//! Tests for `defstruct`'s option list: `(defstruct (Name option...) ...)`.
//!
//! `Checker::check_defstruct` synthesizes each generated constructor and the
//! copier as `defmethod` **source** and runs it through
//! `Checker::check_defmethod_in`, which is what buys them the whole method
//! pipeline — the `&key`/`&optional` filling added in Phase 5b, visibility,
//! and, for a generic owner, the `MethodTemplate::Form` retention that makes
//! them specializable. The body is always `(Name::new slot...)`: `new` stays
//! the one structural constructor and everything here is a way of calling it.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

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
        (defstruct (point) (x i32) (y i32))
        (x (point::new 3 4))
    ";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn a_docstring_still_follows_the_option_list() {
    let src = "
        (defstruct (point) \"a place\" (x i32) (y i32))
        (match (documentation point) ((some s) (length s)) ((none) 0))
    ";
    assert_eq!(eval_ok(src), Value::Int(7));
}

// ---- :constructor, keyword style --------------------------------------------

#[test]
fn a_keyword_constructor_fills_omitted_slots_from_their_defaults() {
    let src = "
        (defstruct (point (:constructor make-point))
          (x i32 0)
          (y i32 0))
        (+ (x (point::make-point :y 7)) (y (point::make-point :y 7)))
    ";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn a_keyword_constructor_takes_every_slot_by_label() {
    let src = "
        (defstruct (point (:constructor make-point))
          (x i32 0)
          (y i32 0))
        (+ (x (point::make-point :x 2 :y 3)) (y (point::make-point :x 2 :y 3)))
    ";
    assert_eq!(eval_ok(src), Value::Int(5));
}

#[test]
fn a_keyword_constructor_needs_a_default_on_every_slot() {
    let msg = check_err("(defstruct (point (:constructor make-point)) (x i32) (y i32 0))");
    assert!(msg.contains("needs a default"), "unexpected message: {}", msg);
}

// ---- :constructor, BOA style -------------------------------------------------

#[test]
fn a_boa_constructor_takes_the_slots_it_names_positionally() {
    let src = "
        (defstruct (point (:constructor of-x (x)))
          (x i32)
          (y i32 100))
        (+ (x (point::of-x 5)) (y (point::of-x 5)))
    ";
    assert_eq!(eval_ok(src), Value::Int(105));
}

#[test]
fn a_boa_constructor_may_reorder_the_slots() {
    let src = "
        (defstruct (point (:constructor yx (y x)))
          (x i32)
          (y i32))
        (- (x (point::yx 1 9)) (y (point::yx 1 9)))
    ";
    assert_eq!(eval_ok(src), Value::Int(8));
}

#[test]
fn a_boa_constructor_takes_an_optional_slot() {
    let src = "
        (defstruct (point (:constructor at (x &optional y)))
          (x i32)
          (y i32 50))
        (+ (y (point::at 1)) (y (point::at 1 2)))
    ";
    assert_eq!(eval_ok(src), Value::Int(52));
}

#[test]
fn several_constructors_may_be_declared() {
    let src = "
        (defstruct (point (:constructor origin) (:constructor of-x (x)))
          (x i32 0)
          (y i32 0))
        (+ (x (point::origin)) (x (point::of-x 4)))
    ";
    assert_eq!(eval_ok(src), Value::Int(4));
}

#[test]
fn a_boa_constructor_may_not_name_a_non_slot() {
    let msg = check_err("(defstruct (point (:constructor f (z))) (x i32))");
    assert!(msg.contains("not a slot"), "unexpected message: {}", msg);
}

#[test]
fn a_boa_constructor_must_be_able_to_fill_every_slot() {
    let msg = check_err("(defstruct (point (:constructor f (x))) (x i32) (y i32))");
    assert!(msg.contains("could not fill"), "unexpected message: {}", msg);
}

#[test]
fn an_optional_boa_slot_needs_a_default() {
    let msg = check_err("(defstruct (point (:constructor f (x &optional y))) (x i32) (y i32))");
    assert!(msg.contains("needs a default"), "unexpected message: {}", msg);
}

// ---- :copier -----------------------------------------------------------------

#[test]
fn a_copier_returns_an_independent_value() {
    let src = "
        (defstruct (point (:copier copy-point)) (x i32) (y i32))
        (let ((a (point::new 1 2)))
          (let ((b (copy-point a)))
            (progn (set-x b 9) (+ (x a) (x b)))))
    ";
    assert_eq!(eval_ok(src), Value::Int(10));
}

// ---- :include ------------------------------------------------------------------

#[test]
fn include_prepends_the_parents_slots() {
    let src = "
        (defstruct base (id i32))
        (defstruct (derived (:include base)) (extra i32))
        (+ (id (derived::new 1 2)) (extra (derived::new 1 2)))
    ";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn include_carries_the_parents_slot_defaults() {
    let src = "
        (defstruct (base (:constructor mk-base)) (id i32 7))
        (defstruct (derived (:include base) (:constructor mk (extra))) (extra i32))
        (id (derived::mk 1))
    ";
    assert_eq!(eval_ok(src), Value::Int(7));
}

#[test]
fn include_rejects_a_duplicated_slot_name() {
    let src = "
        (defstruct base (id i32))
        (defstruct (derived (:include base)) (id i32))
    ";
    assert!(check_err(src).contains("duplicate field"), "{}", check_err(src));
}

#[test]
fn include_rejects_a_non_struct() {
    let msg = check_err("(defenum e (a) (b)) (defstruct (d (:include e)) (x i32))");
    assert!(msg.contains("only a `defstruct` has slots"), "unexpected message: {}", msg);
}

#[test]
fn a_child_is_not_a_subtype_of_its_parent() {
    // `:include` copies the slot list and nothing else — there is no
    // subtyping in this language, so a `derived` is not a `base`.
    let src = "
        (defstruct base (id i32))
        (defstruct (derived (:include base)) (extra i32))
        (defun takes-base ((b base)) i32 (id b))
        (takes-base (derived::new 1 2))
    ";
    let msg = check_err(src);
    assert!(msg.contains("mismatch") || msg.contains("expected"), "unexpected message: {}", msg);
}

// ---- generic owners -------------------------------------------------------------

#[test]
fn a_generic_struct_gets_a_generated_constructor_too() {
    let src = "
        (defstruct (cell<T> (:constructor of (v))) (v T))
        (v (cell::of 5))
    ";
    assert_eq!(eval_ok(src), Value::Int(5));
}

#[test]
fn a_generic_structs_copier_works() {
    let src = "
        (defstruct (cell<T> (:copier dup)) (v T))
        (v (dup (cell::new 8)))
    ";
    assert_eq!(eval_ok(src), Value::Int(8));
}

// ---- what is refused, and why ----------------------------------------------------

#[test]
fn conc_name_is_refused_with_its_reason() {
    let msg = check_err("(defstruct (point (:conc-name p-)) (x i32))");
    assert!(msg.contains("nothing to do here"), "unexpected message: {}", msg);
}

#[test]
fn predicate_is_refused_with_its_reason() {
    let msg = check_err("(defstruct (point (:predicate pointp)) (x i32))");
    assert!(msg.contains("nothing to answer"), "unexpected message: {}", msg);
}

#[test]
fn type_and_friends_are_refused_with_their_reason() {
    for opt in ["(:type list)", "(:initial-offset 1)", "(:named)"] {
        let msg = check_err(&format!("(defstruct (point {}) (x i32))", opt));
        assert!(msg.contains("belongs to the compiler"), "unexpected message for {}: {}", opt, msg);
    }
}

#[test]
fn an_unknown_option_names_the_known_ones() {
    let msg = check_err("(defstruct (point (:nope 1)) (x i32))");
    assert!(msg.contains(":constructor, :copier, :include"), "unexpected message: {}", msg);
}

#[test]
fn a_slot_default_without_a_constructor_is_refused() {
    let msg = check_err("(defstruct point (x i32 0))");
    assert!(msg.contains("nothing would use it"), "unexpected message: {}", msg);
}

// ---- the compile path --------------------------------------------------------------

#[test]
fn a_generated_constructor_compiles() {
    let src = "
        (defstruct (point (:constructor make-point)) (x i32 1) (y i32 2))
        (defun run () i32 (+ (x (point::make-point :y 5)) (y (point::make-point :y 5))))
        (compile run)
        (run)
    ";
    assert_eq!(eval_ok(src), Value::Int(6));
}
