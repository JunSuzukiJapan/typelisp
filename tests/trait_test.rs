//! Tests for the trait machinery (`deftrait`/`impl`/`where`-bounded generic
//! functions) introduced for `doiter` — see `docs/TODO.md`'s entry on
//! `doiter` for why a real trait mechanism (not just a `doiter`-specific
//! duck-typing rule) was chosen.

extern crate typelisp;
use typelisp::{Checker, Error, EvalError, Heap, Interp, Reader, RtValue, TopLevel};

fn check(src: &str) -> Result<TopLevel, Error> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v)?);
    }
    Ok(last.expect("no forms"))
}

fn run(src: &str) -> Result<RtValue, EvalError> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(v) = interp.exec(&mut h, tl)? {
            last = v;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

/// A minimal trait/impl pair, independent of `Vector`/`Iter`, exercising
/// just `deftrait`+`impl`+ordinary (concrete-receiver) dispatch.
const COUNTER_PRELUDE: &str = "
(deftrait Counted
  (count ((self Self)) i32))
(defstruct box (n i32))
(impl Counted box
  (count ((self Self)) i32 self::n))
";

#[test]
fn deftrait_registers_with_no_error() {
    assert!(check("(deftrait Counted (count ((self Self)) i32))").is_ok());
}

#[test]
fn impl_on_a_concrete_type_dispatches_as_an_ordinary_method() {
    let src = format!("{} (let ((b (box::new 42))) (count b))", COUNTER_PRELUDE);
    assert_eq!(eval_ok(&src), RtValue::Int(42));
}

#[test]
fn impl_on_a_primitive_type_dispatches_as_an_ordinary_method() {
    // `check_impl` resolves non-`Named` targets via `prim_type_path`, and
    // primitives are registered with real `AdtDef`s — so a user trait impl
    // on `i32` must work exactly like one on a `defstruct` (this is the
    // path the prelude's scalar `Eq`/`Ord` impls ride on). Also exercises a
    // second `Self` parameter beyond the receiver.
    let src = "
(deftrait Doubling
  (add-twice ((self Self) (other Self)) Self))
(impl Doubling i32
  (add-twice ((self Self) (other Self)) Self (+ self (* other 2))))
(add-twice 40 1)";
    assert_eq!(eval_ok(src), RtValue::Int(42));
}

#[test]
fn impl_on_an_unknown_type_is_a_type_error() {
    assert!(check("(deftrait Counted (count ((self Self)) i32)) (impl Counted nope (count ((self Self)) i32 0))").is_err());
}

#[test]
fn calling_an_unimplemented_trait_method_on_a_concrete_type_is_a_type_error() {
    // `box` never gets a `Counted` impl here, so `(count b)` must fail to
    // resolve (no free function, no instance method) rather than silently
    // doing something else.
    let src = "(deftrait Counted (count ((self Self)) i32))
               (defstruct box (n i32))
               (let ((b (box::new 1))) (count b))";
    assert!(check(src).is_err());
}

#[test]
fn generic_function_calls_a_trait_method_via_where_bound() {
    // `it: T`, `(where (Counted T))` — `describe`'s own generic body checks
    // `count` as `Expr::TraitCall` (no concrete `AdtDef` for `t`), but this
    // call site is `describe(box::new 7)`: monomorphization re-checks the
    // specialization with `T = box` concrete, so `count` resolves statically
    // to `Expr::Assoc` there — `TraitCall` itself never executes.
    let src = format!(
        "{} (defun (describe T) ((it T)) i32 (where (Counted T)) (count it))
            (describe (box::new 7))",
        COUNTER_PRELUDE
    );
    assert_eq!(eval_ok(&src), RtValue::Int(7));
}

#[test]
fn where_bound_function_rejects_a_method_the_trait_does_not_declare() {
    let src = format!(
        "{} (defun (describe T) ((it T)) i32 (where (Counted T)) (nope it))
            (describe (box::new 7))",
        COUNTER_PRELUDE
    );
    assert!(check(&src).is_err());
}

#[test]
fn two_types_implementing_the_same_trait_dispatch_independently() {
    let src = "
        (deftrait Counted (count ((self Self)) i32))
        (defstruct box-a (n i32))
        (defstruct box-b (n i32))
        (impl Counted box-a (count ((self Self)) i32 self::n))
        (impl Counted box-b (count ((self Self)) i32 (* 2 self::n)))
        (defun (describe T) ((it T)) i32 (where (Counted T)) (count it))
        (+ (describe (box-a::new 3)) (describe (box-b::new 3)))";
    assert_eq!(eval_ok(src), RtValue::Int(3 + 6));
}

#[test]
fn defmethod_on_a_generic_owner_can_carry_a_where_clause() {
    // A `where` clause directly on a `defmethod` (not via `impl`): the body
    // calls a trait method on the owner's own type variable. The
    // once-through diagnostic check resolves `(count self::x)` through the
    // bounds branch (`Expr::TraitCall`); the call site's specialization
    // re-checks with `A = box` concrete.
    let src = format!(
        "{} (defstruct (pair A) (x A) (y A))
            (defmethod count-both ((self pair<A>)) i32 (where (Counted A))
              (+ (count self::x) (count self::y)))
            (count-both (pair::new (box::new 20) (box::new 22)))",
        COUNTER_PRELUDE
    );
    assert_eq!(eval_ok(&src), RtValue::Int(42));
}

#[test]
fn impl_method_with_a_where_clause_supports_recursive_structural_dispatch() {
    // The recursive-impl shape the where clause exists for: `pr<A,B>`'s own
    // `same` requires `(Eq2 A) (Eq2 B)` and compares fields via the bounded
    // trait method — with a *nested* `pr` exercising impl-on-impl recursion.
    let src = "
        (deftrait Eq2 (same ((self Self) (other Self)) bool))
        (impl Eq2 i32 (same ((self Self) (other Self)) bool (= self other)))
        (defstruct (pr A B) (a A) (b B))
        (impl Eq2 pr<A,B>
          (same ((self Self) (other Self)) bool (where (Eq2 A) (Eq2 B))
            (if (same self::a other::a) (same self::b other::b) false)))
        (if (same (pr::new (pr::new 1 2) 3) (pr::new (pr::new 1 2) 3))
            (if (same (pr::new (pr::new 1 2) 3) (pr::new (pr::new 1 9) 3)) 0 1)
            0)";
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn call_site_rejects_a_type_that_does_not_implement_the_required_trait() {
    // `box2` never gets a `Counted` impl, so `(describe (box2::new 5))` must
    // fail to *check* now — previously this type-checked fine (the `where`
    // clause was never consulted at the call site) and only panicked if
    // actually evaluated, via `Expr::TraitCall`'s "no implementation"
    // runtime fallback.
    let src = format!(
        "{} (defstruct box2 (n i32))
            (defun (describe T) ((it T)) i32 (where (Counted T)) (count it))
            (describe (box2::new 5))",
        COUNTER_PRELUDE
    );
    assert!(check(&src).is_err());
}
