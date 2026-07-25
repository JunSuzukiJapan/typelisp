//! Tests for dynamic dispatch through trait objects (`:dyn Trait`).
//!
//! A trait object is a fat box (`BoxedObj::Dyn`) pairing a vtable id with the
//! concrete value; the vtable is one table per (concrete type, trait) pair,
//! laid out in `deftrait` method order, so a call site indexes a constant
//! slot. These tests run the interpreter tier; `compile_test.rs` covers the
//! JIT tier and `compile_file_test.rs` the AOT one.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, Heap, Interp, Reader, RtValue};

fn run(src: &str) -> Result<RtValue, String> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).map_err(|e| format!("{:?}", e))?;
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| format!("{:?}", e))?;
        if let Some(val) = interp.exec(&mut h, tl).map_err(|e| format!("{:?}", e))? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

fn eval_err(src: &str) -> String {
    run(src).expect_err("expected a type error")
}

/// Two shapes implementing one trait — the setup every test below builds on.
const SHAPES: &str = r#"
(deftrait Drawable
  (draw ((self Self)) string)
  (sides ((self Self)) i32))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle
  (draw ((self Self)) string "circle")
  (sides ((self Self)) i32 0))
(impl Drawable square
  (draw ((self Self)) string "square")
  (sides ((self Self)) i32 4))
"#;

// ---- dispatch -----------------------------------------------------------

#[test]
fn a_concrete_value_is_boxed_implicitly_at_a_dyn_parameter() {
    let src = format!(
        "{SHAPES}
         (defun render ((d :dyn Drawable)) string (draw d))
         (render (circle::new 3))"
    );
    assert_eq!(eval_ok(&src), RtValue::Str("circle".into()));
}

#[test]
fn the_same_call_site_dispatches_to_each_implementation() {
    // The whole point: one `(draw d)` in one compiled body, two answers.
    let src = format!(
        "{SHAPES}
         (defun render ((d :dyn Drawable)) string (draw d))
         (append (render (circle::new 3)) (render (square::new 2)))"
    );
    assert_eq!(eval_ok(&src), RtValue::Str("circlesquare".into()));
}

#[test]
fn a_second_slot_dispatches_independently_of_the_first() {
    // Slot numbering comes from `deftrait` order, so `sides` must not be
    // confused with `draw`.
    let src = format!(
        "{SHAPES}
         (defun count-sides ((d :dyn Drawable)) i32 (sides d))
         (+ (count-sides (circle::new 3)) (count-sides (square::new 2)))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(4));
}

#[test]
fn a_method_argument_and_return_value_cross_the_vtable() {
    let src = r#"
        (deftrait Scaler (scale ((self Self) (k i32)) i32))
        (defstruct fixed (n i32))
        (impl Scaler fixed (scale ((self Self) (k i32)) i32 (* self::n k)))
        (defun apply-scale ((s :dyn Scaler) (k i32)) i32 (scale s k))
        (apply-scale (fixed::new 6) 7)"#;
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

// ---- explicit boxing ----------------------------------------------------

#[test]
fn as_boxes_a_trait_object_explicitly() {
    let src = format!(
        "{SHAPES}
         (defun render ((d :dyn Drawable)) string (draw d))
         (render (as :dyn Drawable (square::new 2)))"
    );
    assert_eq!(eval_ok(&src), RtValue::Str("square".into()));
}

#[test]
fn try_as_to_a_trait_object_is_rejected() {
    let src = format!("{SHAPES} (try-as :dyn Drawable (square::new 2))");
    assert!(eval_err(&src).contains("always succeeds or is a type error"));
}

// ---- heterogeneous collections ------------------------------------------

#[test]
fn a_vector_of_trait_objects_holds_different_concrete_types() {
    let src = format!(
        "{SHAPES}
         (defun empty-shapes () Vector<:dyn Drawable> (Vector::new))
         (defun build () Vector<:dyn Drawable>
           (let ((v (empty-shapes)))
             (push v (circle::new 3))
             (push v (square::new 2))
             v))
         (let ((v (build)) (out \"\"))
           (doiter (d (iter v)) (setf out (append out (draw d))))
           out)"
    );
    assert_eq!(eval_ok(&src), RtValue::Str("circlesquare".into()));
}

// ---- associated types ---------------------------------------------------

#[test]
fn an_associated_type_is_pinned_positionally() {
    // `:dyn Iter<i32>` is prelude's `Iter` with `Item = i32`, so `next`
    // returns `Option<i32>` and the arithmetic below type-checks.
    let src = r#"
        (defun total ((it :dyn Iter<i32>)) i32
          (let ((n 0))
            (loop
              (match (next it)
                ((Some x) (setf n (+ n x)))
                (_ (break))))
            n))
        (defun make-v () Vector<i32> (Vector::new))
        (let ((v (make-v)))
          (push v 10)
          (push v 32)
          (total (iter v)))"#;
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

#[test]
fn a_mismatched_associated_type_pin_is_rejected() {
    let src = r#"
        (defun total ((it :dyn Iter<string>)) i32 0)
        (defun make-v () Vector<i32> (Vector::new))
        (let ((v (make-v)))
          (push v 10)
          (total (iter v)))"#;
    let m = eval_err(src);
    assert!(m.contains("requires") && m.contains("item"), "{}", m);
}

#[test]
fn the_wrong_number_of_associated_type_pins_is_rejected() {
    let src = "(defun total ((it :dyn Iter)) i32 0) (total 1)";
    let m = eval_err(src);
    assert!(m.contains("associated-type argument"), "{}", m);
}

// ---- rejections ---------------------------------------------------------

#[test]
fn a_type_that_does_not_implement_the_trait_cannot_be_boxed() {
    let src = format!(
        "{SHAPES}
         (defstruct dot (x i32))
         (defun render ((d :dyn Drawable)) string (draw d))
         (render (dot::new 1))"
    );
    assert!(eval_err(&src).contains("does not implement"));
}

#[test]
fn a_primitive_cannot_be_boxed_even_when_it_implements_the_trait() {
    // A fat box holds one heap value, so a primitive can't go in one even
    // with a perfectly object-safe `impl` — a per-*type* rejection, not a
    // per-trait one, which is what lets `:dyn Speak` still work for a
    // `defstruct` that implements the same trait.
    let src = r#"
        (deftrait Speak (say ((self Self)) string))
        (impl Speak i32 (say ((self Self)) string "int"))
        (defun hear ((s :dyn Speak)) string (say s))
        (hear 1)"#;
    let m = eval_err(src);
    assert!(m.contains("no heap representation"), "{}", m);
}

#[test]
fn a_method_the_trait_does_not_declare_cannot_be_called_on_a_trait_object() {
    let src = format!(
        "{SHAPES}
         (defun radius ((d :dyn Drawable)) i32 (r d))
         (radius (circle::new 3))"
    );
    let m = eval_err(&src);
    assert!(m.contains("is not a method of"), "{}", m);
}

#[test]
fn a_trait_with_a_static_method_is_not_object_safe() {
    let src = r#"
        (deftrait Zeroed (zero ((n i32)) i32))
        (defun f ((z :dyn Zeroed)) i32 0)
        (f 1)"#;
    let m = eval_err(src);
    assert!(m.contains("no `self` receiver"), "{}", m);
}

#[test]
fn a_method_returning_self_makes_a_trait_not_object_safe() {
    let src = r#"
        (deftrait Cloneable (dup ((self Self)) Self))
        (defstruct cell (n i32))
        (impl Cloneable cell (dup ((self Self)) Self (cell::new self::n)))
        (defun f ((c :dyn Cloneable)) i32 0)
        (f (cell::new 1))"#;
    let m = eval_err(src);
    assert!(m.contains("mentions `Self` outside the receiver position"), "{}", m);
}

#[test]
fn an_unknown_trait_name_is_reported_as_such() {
    let m = eval_err("(defun f ((x :dyn Nope)) i32 0) (f 1)");
    assert!(m.contains("unknown trait"), "{}", m);
}

#[test]
fn trait_upcasting_is_rejected() {
    let src = format!(
        "{SHAPES}
         (deftrait Named (name ((self Self)) string))
         (impl Named circle (name ((self Self)) string \"c\"))
         (defun render ((d :dyn Drawable)) string (draw d))
         (defun relabel ((n :dyn Named)) string (render n))
         (relabel (circle::new 1))"
    );
    let m = eval_err(&src);
    assert!(m.contains("trait upcasting is not supported"), "{}", m);
}

// ---- `:dyn` outside a type position -------------------------------------

#[test]
fn dyn_in_a_value_position_is_a_type_error() {
    let m = eval_err("(defun f () i32 (:dyn Drawable))");
    assert!(m.contains("may only appear in a type position"), "{}", m);
}
