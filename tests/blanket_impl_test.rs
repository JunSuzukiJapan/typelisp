//! Tests for blanket `impl`s — `(impl<T> Clamp T (where (Ord2 T)) ...)`,
//! Rust's `impl<T: Ord> Clamp for T`.
//!
//! The defining property is laziness: declaring one registers nothing on any
//! type and checks no method body. A concrete type reaching the impl queues
//! one materialization, which replays the stored items through the ordinary
//! `check_impl` path. These tests pin both halves — that it works, and that
//! nothing happens until it is asked for.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, RtValue};

fn run(src: &str) -> Result<RtValue, String> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
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
    run(src).expect_err("expected an error")
}

/// An extension trait entirely made of default bodies, blanket-implemented
/// for every `Ranked` type — the canonical shape the feature exists for.
const CLAMP: &str = "
(deftrait Ranked ()
  (rank ((self Self)) i32))
(deftrait Clamp (Ranked)
  (clamped ((self Self) (lo Self) (hi Self)) i32
    (if (< (rank self) (rank lo))
        (rank lo)
        (if (< (rank hi) (rank self)) (rank hi) (rank self)))))
(defstruct cell (n i32))
(impl Ranked cell (rank ((self Self)) i32 self::n))
(impl<T> Clamp T (where (Ranked T)))
";

#[test]
fn a_blanket_impl_supplies_a_method_to_a_qualifying_type() {
    let src = format!("{CLAMP} (clamped (cell::new 9) (cell::new 1) (cell::new 5))");
    assert_eq!(eval_ok(&src), RtValue::Int(5));
}

#[test]
fn one_blanket_impl_covers_two_concrete_types() {
    let src = format!(
        "{CLAMP}
         (defstruct tick (t i32))
         (impl Ranked tick (rank ((self Self)) i32 (* self::t 2)))
         (+ (clamped (cell::new 9) (cell::new 1) (cell::new 5))
            (clamped (tick::new 9) (tick::new 1) (tick::new 5)))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(15));
}

#[test]
fn a_blanket_impl_satisfies_a_where_bound() {
    // The bound is discharged through the blanket, not an explicit `impl`.
    let src = format!(
        "{CLAMP}
         (defun mid<T> ((x T) (lo T) (hi T)) i32 (where (Clamp T)) (clamped x lo hi))
         (mid (cell::new 9) (cell::new 1) (cell::new 5))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(5));
}

#[test]
fn a_type_failing_the_blankets_bounds_is_not_covered() {
    let src = format!(
        "{CLAMP}
         (defstruct plain (n i32))
         (clamped (plain::new 9) (plain::new 1) (plain::new 5))"
    );
    let m = eval_err(&src);
    assert!(m.contains("NoSuchFunction") || m.contains("no such function"), "{}", m);
}

#[test]
fn an_explicit_impl_wins_over_the_blanket() {
    let src = format!(
        "{CLAMP}
         (defstruct tick (t i32))
         (impl Ranked tick (rank ((self Self)) i32 self::t))
         (impl Clamp tick (clamped ((self Self) (lo Self) (hi Self)) i32 999))
         (clamped (tick::new 9) (tick::new 1) (tick::new 5))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(999));
}

#[test]
fn a_second_blanket_impl_for_the_same_trait_is_rejected() {
    let src = format!(
        "{CLAMP}
         (impl<U> Clamp U (where (Ranked U)))"
    );
    let m = eval_err(&src);
    assert!(m.contains("at most one"), "{}", m);
}

#[test]
fn a_blanket_impl_whose_where_names_a_foreign_variable_is_rejected() {
    let src = "
        (deftrait Ranked () (rank ((self Self)) i32))
        (deftrait Clamp (Ranked) (clamped ((self Self)) i32 (rank self)))
        (impl<T> Clamp T (where (Ranked Q)))";
    let m = eval_err(src);
    assert!(m.contains("not one of the impl's type parameters"), "{}", m);
}

#[test]
fn mutually_recursive_blanket_bounds_do_not_hang() {
    // `A` requires `B` requires `A` — the coverage search must terminate and
    // simply not cover anything, rather than recurring forever.
    let src = "
        (deftrait A () (a ((self Self)) i32 1))
        (deftrait B () (b ((self Self)) i32 2))
        (defstruct s (n i32))
        (impl<T> A T (where (B T)))
        (impl<T> B T (where (A T)))
        (a (s::new 1))";
    let m = eval_err(src);
    assert!(m.contains("NoSuchFunction") || m.contains("no such function"), "{}", m);
}

#[test]
fn a_blanket_provided_method_is_reachable_through_a_trait_object() {
    // `Clamp` itself is not object-safe (`clamped` takes `Self` arguments),
    // so this uses a one-argument sibling — the point is that the vtable is
    // laid out over a materialized blanket impl, not a written one.
    let src = "
        (deftrait Ranked () (rank ((self Self)) i32))
        (deftrait Doubled (Ranked) (doubled ((self Self)) i32 (* 2 (rank self))))
        (defstruct cell (n i32))
        (impl Ranked cell (rank ((self Self)) i32 self::n))
        (impl<T> Doubled T (where (Ranked T)))
        (defun peek ((d :dyn Doubled)) i32 (doubled d))
        (peek (cell::new 7))";
    assert_eq!(eval_ok(src), RtValue::Int(14));
}

#[test]
fn a_self_taking_method_still_makes_a_trait_not_object_safe() {
    let src = format!("{CLAMP} (defun peek ((c :dyn Clamp)) i32 1) (peek (cell::new 1))");
    let m = eval_err(&src);
    assert!(m.contains("mentions `Self` outside the receiver position"), "{}", m);
}

#[test]
fn an_impl_over_a_type_constructor_is_not_a_blanket_impl() {
    // `Vector<T>` has a single owning `AdtDef`, so `impl<T>` here takes the
    // ordinary generic-owner path; the head parameters are documentation.
    let src = "
        (deftrait Sized2 () (size ((self Self)) i32))
        (impl<T> Sized2 Vector<T> (size ((self Self)) i32 7))
        (defun build () Vector<i32> (Vector::new))
        (let ((v (build))) (push v 1) (size v))";
    assert_eq!(eval_ok(src), RtValue::Int(7));
}
