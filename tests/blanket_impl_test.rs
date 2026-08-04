//! Tests for blanket `impl`s — `(impl<T> Clamp T (where (Ord2 T)) ...)`,
//! Rust's `impl<T: Ord> Clamp for T`.
//!
//! The defining property is laziness: declaring one registers nothing on any
//! type and *generates* no method. A concrete type reaching the impl queues
//! one materialization, which replays the stored items through the ordinary
//! `check_impl` path. These tests pin both halves — that it works, and that
//! nothing happens until it is asked for.
//!
//! The bodies are still type-*checked* once at the declaration, against the
//! declared bounds with the target left abstract, exactly as Rust checks an
//! unused blanket impl — see the group at the bottom.

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

/// A trait whose blanket impl writes its method out, rather than inheriting a
/// default body — the shape whose body the declaration-time pass checks.
const DOUBLE: &str = "
(deftrait Ranked () (rank ((self Self)) i32))
(deftrait Doubled (Ranked) (doubled ((self Self)) i32))
(defstruct cell (n i32))
(impl Ranked cell (rank ((self Self)) i32 self::n))
";

#[test]
fn an_unused_blanket_impls_body_is_type_checked() {
    // Nothing ever reaches this impl, so nothing is ever materialized; the
    // body's return type is still wrong, and Rust would say so too.
    let src = format!(
        "{DOUBLE}
         (impl<T> Doubled T (where (Ranked T))
           (doubled ((self Self)) i32 \"two\"))"
    );
    let m = eval_err(&src);
    assert!(m.contains("TypeError"), "{}", m);
}

#[test]
fn an_unused_blanket_impls_body_may_call_an_unknown_function() {
    let src = format!(
        "{DOUBLE}
         (impl<T> Doubled T (where (Ranked T))
           (doubled ((self Self)) i32 (triple self)))"
    );
    let m = eval_err(&src);
    assert!(m.contains("NoSuchFunction") || m.contains("no such function"), "{}", m);
}

#[test]
fn a_blanket_impls_body_may_use_its_declared_bounds() {
    // `(rank self)` is only callable because of `(where (Ranked T))` — the
    // abstract check resolves it through the bound, like a generic `defun`.
    let src = format!(
        "{DOUBLE}
         (impl<T> Doubled T (where (Ranked T))
           (doubled ((self Self)) i32 (* 2 (rank self))))
         (doubled (cell::new 7))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(14));
}

#[test]
fn a_blanket_impls_body_may_call_a_sibling_method_on_self() {
    // `halved` is a method of the trait being implemented, so `Self: Doubled`
    // has to be in scope for the abstract check the way it is in Rust.
    let src = format!(
        "{DOUBLE}
         (deftrait Halved (Ranked)
           (halved ((self Self)) i32)
           (twice-halved ((self Self)) i32))
         (impl<T> Halved T (where (Ranked T))
           (halved ((self Self)) i32 (/ (rank self) 2))
           (twice-halved ((self Self)) i32 (/ (halved self) 2)))
         (twice-halved (cell::new 20))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(5));
}

#[test]
fn a_blanket_impls_body_is_checked_against_its_associated_type_binding() {
    // `Item` is this impl's `i32`, so the `i32`-returning body fits and the
    // string one does not — the associated types are substituted for the
    // abstract check exactly as they are for a materialization.
    let boxed = format!(
        "{DOUBLE}
         (deftrait Boxed () (type Item) (unwrap ((self Self)) Item))"
    );
    let ok = format!(
        "{boxed}
         (impl<T> Boxed T (where (Ranked T))
           (type Item i32)
           (unwrap ((self Self)) Item (rank self)))"
    );
    assert_eq!(eval_ok(&ok), RtValue::Unit);
    let bad = format!(
        "{boxed}
         (impl<T> Boxed T (where (Ranked T))
           (type Item i32)
           (unwrap ((self Self)) Item \"nope\"))"
    );
    let m = eval_err(&bad);
    assert!(m.contains("TypeError"), "{}", m);
}

#[test]
fn a_method_call_the_bounds_do_not_justify_is_rejected() {
    // `Tagged` neither bounds `Ranked` nor inherits it, so `(rank self)` has
    // no justification for *any* target — exactly what the abstract check is
    // for. (With `(where (Ranked T))`, or with `Ranked` as a supertrait, the
    // same body is fine: see the two tests above.)
    let src = format!(
        "{DOUBLE}
         (deftrait Tagged () (tag ((self Self)) i32))
         (impl<T> Tagged T
           (tag ((self Self)) i32 (rank self)))"
    );
    let m = eval_err(&src);
    assert!(m.contains("NoSuchFunction") || m.contains("no such function"), "{}", m);
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
