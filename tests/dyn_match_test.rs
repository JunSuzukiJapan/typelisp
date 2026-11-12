//! Tests for a trait object's interaction with the rest of the language:
//! `match`ing it back down to concrete types, printing, and comparison.
//!
//! The unifying rule is that the fat box is *transparent* — it adds dispatch
//! and nothing else. `match` unwraps it and reuses the existing `Sexpr`
//! downcast patterns (`Expr::DynValue`); printing and `eq`/`equal`/`equalp`
//! see straight through it.

extern crate typelisp;

mod common;
use common::{eval_string_compiled as eval_string};
use typelisp::{load_compiler, load_prelude, Checker, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, String> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).map_err(|e| format!("{:?}", e))?;
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| format!("{:?}", e))?;
        if let Some(val) = interp.exec(&mut h, tl).map_err(|e| format!("{:?}", e))? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

const SHAPES: &str = r#"
(deftrait Drawable () (defmethod draw (self) string))
(defstruct circle (r int))
(defstruct square (side int))
(impl Drawable circle (defmethod draw (self) string "circle"))
(impl Drawable square (defmethod draw (self) string "square"))
"#;

// ---- match: back down to the concrete type ------------------------------

#[test]
fn matching_a_trait_object_recovers_the_concrete_type_by_name() {
    let src = format!(
        "{SHAPES}
         (defun area ((d :dyn Drawable)) int
           (match d
             ((circle :r) (* 3 (* r r)))
             ((square :side s) (* s s))
             (_ 0)))
         (+ (area (make-instance 'circle :r 2)) (area (make-instance 'square :side 5)))"
    );
    assert_eq!(eval_ok(&src), Value::Int(37));
}

/// A trait object inside another value — a struct's field, a variant's —
/// is matched the same way, by the patterns of the value it holds, and its
/// types are open, so a catch-all arm is needed; a compiled function matches
/// it as the interpreter does.
#[test]
fn a_trait_object_inside_a_pattern_is_matched_as_the_value_it_holds() {
    let src = format!(
        "{SHAPES}
         (defstruct holder (d :dyn Drawable))
         (defenum maybe (one :dyn Drawable) (nothing))
         (defun size ((h holder)) int
           (match h
             ((holder :d (circle :r)) (* 10 r))
             ((holder :d (square :side s)) s)
             (_ 0)))
         (defun held ((m maybe)) int
           (match m ((one (circle :r)) r) (_ 0)))
         (defun total () int
           (+ (size (make-instance 'holder :d (make-instance 'circle :r 2)))
              (size (make-instance 'holder :d (make-instance 'square :side 5)))
              (held (maybe::one (make-instance 'circle :r 300)))
              (held (maybe::one (make-instance 'square :side 7)))))"
    );
    assert_eq!(eval_ok(&format!("{src} (total)")), Value::Int(325));
    assert_eq!(eval_ok(&format!("{src} (compile size) (compile held) (total)")), Value::Int(325));
}

#[test]
fn a_the_pattern_binds_the_whole_concrete_value() {
    let src = format!(
        "{SHAPES}
         (defun side-of ((d :dyn Drawable)) int
           (match d
             ((the square s) s::side)
             (_ 0)))
         (side-of (make-instance 'square :side 7))"
    );
    assert_eq!(eval_ok(&src), Value::Int(7));
}

#[test]
fn an_unmatched_concrete_type_falls_through_to_the_catchall() {
    // The set of implementing types is open, so a `match` on a trait object
    // is non-exhaustive by nature — the catch-all is doing real work here.
    let src = format!(
        "{SHAPES}
         (defun area ((d :dyn Drawable)) int
           (match d ((circle :r) r) (_ -1)))
         (area (make-instance 'square :side 5))"
    );
    assert_eq!(eval_ok(&src), Value::Int(-1));
}

#[test]
fn a_trait_object_match_still_dispatches_dynamically_in_the_same_function() {
    // Both mechanisms over one value: the vtable call and the downcast.
    let src = format!(
        "{SHAPES}
         (defun describe ((d :dyn Drawable)) string
           (match d
             ((circle :r) (append (draw d) \"!\"))
             (_ (draw d))))
         (append (describe (make-instance 'circle :r 1)) (describe (make-instance 'square :side 1)))"
    );
    assert_eq!(eval_string(&src), "circle!square");
}

// ---- printing -----------------------------------------------------------

#[test]
fn a_trait_object_prints_as_the_value_it_wraps() {
    let src = format!(
        "{SHAPES}
         (defun show ((d :dyn Drawable)) string (format false \"~a\" d))
         (show (make-instance 'circle :r 3))"
    );
    let plain = format!("{SHAPES} (format false \"~a\" (make-instance 'circle :r 3))");
    assert_eq!(eval_string(&src), eval_string(&plain));
}

// ---- comparison ---------------------------------------------------------

#[test]
fn comparison_sees_through_the_box() {
    // The box comes from an *implicit* coercion, so it must not change the
    // answer: `equalp` compares the structs it wraps.
    let src = format!(
        "{SHAPES}
         (defun same ((a :dyn Drawable) (b :dyn Drawable)) bool (equalp a b))
         (same (make-instance 'circle :r 3) (make-instance 'circle :r 3))"
    );
    assert_eq!(eval_ok(&src), Value::Bool(true));

    let differing = format!(
        "{SHAPES}
         (defun same ((a :dyn Drawable) (b :dyn Drawable)) bool (equalp a b))
         (same (make-instance 'circle :r 3) (make-instance 'circle :r 4))"
    );
    assert_eq!(eval_ok(&differing), Value::Bool(false));
}

#[test]
fn identity_is_the_wrapped_object_not_the_box() {
    // `eq` on a boxed value and the same value boxed again: one object.
    let src = format!(
        "{SHAPES}
         (defun same ((a :dyn Drawable) (b :dyn Drawable)) bool (eq a b))
         (let ((c (make-instance 'circle :r 3))) (same c c))"
    );
    assert_eq!(eval_ok(&src), Value::Bool(true));
}

// ---- storing in a Sexpr -------------------------------------------------

#[test]
fn a_trait_object_can_be_stored_in_a_sexpr_list() {
    let src = format!(
        "{SHAPES}
         (defun box-it ((d :dyn Drawable)) Option<Sexpr> (list d))
         (defun first-draw ((xs Option<Sexpr>)) string
           (match (sexpr-car xs) ((circle :r) \"circle\") (_ \"other\")))
         (first-draw (box-it (make-instance 'circle :r 3)))"
    );
    assert_eq!(eval_string(&src), "circle");
}
