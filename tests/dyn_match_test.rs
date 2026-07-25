//! Tests for a trait object's interaction with the rest of the language:
//! `match`ing it back down to concrete types, printing, and comparison.
//!
//! The unifying rule is that the fat box is *transparent* — it adds dispatch
//! and nothing else. `match` unwraps it and reuses the existing `Sexpr`
//! downcast patterns (`Expr::DynValue`); printing and `eq`/`equal`/`equalp`
//! see straight through it.

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

const SHAPES: &str = r#"
(deftrait Drawable (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))
"#;

// ---- match: back down to the concrete type ------------------------------

#[test]
fn matching_a_trait_object_recovers_the_concrete_type_by_name() {
    let src = format!(
        "{SHAPES}
         (defun area ((d :dyn Drawable)) i32
           (match d
             ((circle r) (* 3 (* r r)))
             ((square s) (* s s))
             (_ 0)))
         (+ (area (circle::new 2)) (area (square::new 5)))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(37));
}

#[test]
fn a_the_pattern_binds_the_whole_concrete_value() {
    let src = format!(
        "{SHAPES}
         (defun side-of ((d :dyn Drawable)) i32
           (match d
             ((the square s) s::side)
             (_ 0)))
         (side-of (square::new 7))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(7));
}

#[test]
fn an_unmatched_concrete_type_falls_through_to_the_catchall() {
    // The set of implementing types is open, so a `match` on a trait object
    // is non-exhaustive by nature — the catch-all is doing real work here.
    let src = format!(
        "{SHAPES}
         (defun area ((d :dyn Drawable)) i32
           (match d ((circle r) r) (_ -1)))
         (area (square::new 5))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(-1));
}

#[test]
fn a_trait_object_match_still_dispatches_dynamically_in_the_same_function() {
    // Both mechanisms over one value: the vtable call and the downcast.
    let src = format!(
        "{SHAPES}
         (defun describe ((d :dyn Drawable)) string
           (match d
             ((circle r) (append (draw d) \"!\"))
             (_ (draw d))))
         (append (describe (circle::new 1)) (describe (square::new 1)))"
    );
    assert_eq!(eval_ok(&src), RtValue::Str("circle!square".into()));
}

// ---- printing -----------------------------------------------------------

#[test]
fn a_trait_object_prints_as_the_value_it_wraps() {
    let src = format!(
        "{SHAPES}
         (defun show ((d :dyn Drawable)) string (format false \"~a\" d))
         (show (circle::new 3))"
    );
    let plain = format!("{SHAPES} (format false \"~a\" (circle::new 3))");
    assert_eq!(eval_ok(&src), eval_ok(&plain));
}

// ---- comparison ---------------------------------------------------------

#[test]
fn comparison_sees_through_the_box() {
    // The box comes from an *implicit* coercion, so it must not change the
    // answer: `equalp` compares the structs it wraps.
    let src = format!(
        "{SHAPES}
         (defun same ((a :dyn Drawable) (b :dyn Drawable)) bool (equalp a b))
         (same (circle::new 3) (circle::new 3))"
    );
    assert_eq!(eval_ok(&src), RtValue::Bool(true));

    let differing = format!(
        "{SHAPES}
         (defun same ((a :dyn Drawable) (b :dyn Drawable)) bool (equalp a b))
         (same (circle::new 3) (circle::new 4))"
    );
    assert_eq!(eval_ok(&differing), RtValue::Bool(false));
}

#[test]
fn identity_is_the_wrapped_object_not_the_box() {
    // `eq` on a boxed value and the same value boxed again: one object.
    let src = format!(
        "{SHAPES}
         (defun same ((a :dyn Drawable) (b :dyn Drawable)) bool (eq a b))
         (let ((c (circle::new 3))) (same c c))"
    );
    assert_eq!(eval_ok(&src), RtValue::Bool(true));
}

// ---- storing in a Sexpr -------------------------------------------------

#[test]
fn a_trait_object_can_be_stored_in_a_sexpr_list() {
    let src = format!(
        "{SHAPES}
         (defun box-it ((d :dyn Drawable)) Sexpr (list d))
         (defun first-draw ((xs Sexpr)) string
           (match (sexpr-car xs) ((circle r) \"circle\") (_ \"other\")))
         (first-draw (box-it (circle::new 3)))"
    );
    assert_eq!(eval_ok(&src), RtValue::Str("circle".into()));
}
