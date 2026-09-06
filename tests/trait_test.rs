//! Tests for the trait machinery (`deftrait`/`impl`/`where`-bounded generic
//! functions) introduced for `doiter` — see `docs/dev/implementation-log.md`'s
//! "trait機構（deftrait/impl/where） + Vector<T> + doiter" for why a real trait
//! mechanism (not just a `doiter`-specific duck-typing rule) was chosen.

extern crate typelisp;
use typelisp::{Checker, Error, EvalError, Heap, Interp, Reader, Value, TopLevelForm};

fn check(src: &str) -> Result<TopLevelForm, Error> {
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

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(v) = interp.exec(&mut h, tl)? {
            last = v;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

/// The text of a `string` result.
///
/// A `string` is a heap `Value::Str` since the scalar unification, so reading
/// one needs the heap it lives in — and `run` above drops its heap on return.
/// Hence this parallel runner, which reads the text out first.
fn eval_string(src: &str) -> String {
    let mut h = Heap::with_capacity(8192);
    let mut chk = Checker::new();
    let interp = Interp::new();
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        typelisp::Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// A minimal trait/impl pair, independent of `Vector`/`Iter`, exercising
/// just `deftrait`+`impl`+ordinary (concrete-receiver) dispatch.
const COUNTER_PRELUDE: &str = "
(deftrait Counted ()
  (count ((self Self)) i32))
(defstruct box (n i32))
(impl Counted box
  (count ((self Self)) i32 self::n))
";

#[test]
fn deftrait_registers_with_no_error() {
    assert!(check("(deftrait Counted () (count ((self Self)) i32))").is_ok());
}

#[test]
fn impl_on_a_concrete_type_dispatches_as_an_ordinary_method() {
    let src = format!("{} (let ((b (box::new 42))) (count b))", COUNTER_PRELUDE);
    assert_eq!(eval_ok(&src), Value::Int(42));
}

#[test]
fn impl_on_a_primitive_type_dispatches_as_an_ordinary_method() {
    // `check_impl` resolves non-`Named` targets via `prim_type_path`, and
    // primitives are registered with real `AdtDef`s — so a user trait impl
    // on `i32` must work exactly like one on a `defstruct` (this is the
    // path the prelude's scalar `Eq`/`Ord` impls ride on). Also exercises a
    // second `Self` parameter beyond the receiver.
    let src = "
(deftrait Doubling ()
  (add-twice ((self Self) (other Self)) Self))
(impl Doubling i32
  (add-twice ((self Self) (other Self)) Self (+ self (* other 2))))
(add-twice 40 1)";
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn impl_on_an_unknown_type_is_a_type_error() {
    assert!(check("(deftrait Counted () (count ((self Self)) i32)) (impl Counted nope (count ((self Self)) i32 0))").is_err());
}

#[test]
fn calling_an_unimplemented_trait_method_on_a_concrete_type_is_a_type_error() {
    // `box` never gets a `Counted` impl here, so `(count b)` must fail to
    // resolve (no free function, no instance method) rather than silently
    // doing something else.
    let src = "(deftrait Counted () (count ((self Self)) i32))
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
        "{} (defun describe<T> ((it T)) i32 (where (Counted T)) (count it))
            (describe (box::new 7))",
        COUNTER_PRELUDE
    );
    assert_eq!(eval_ok(&src), Value::Int(7));
}

#[test]
fn where_bound_function_rejects_a_method_the_trait_does_not_declare() {
    let src = format!(
        "{} (defun describe<T> ((it T)) i32 (where (Counted T)) (nope it))
            (describe (box::new 7))",
        COUNTER_PRELUDE
    );
    assert!(check(&src).is_err());
}

#[test]
fn two_types_implementing_the_same_trait_dispatch_independently() {
    let src = "
        (deftrait Counted () (count ((self Self)) i32))
        (defstruct box-a (n i32))
        (defstruct box-b (n i32))
        (impl Counted box-a (count ((self Self)) i32 self::n))
        (impl Counted box-b (count ((self Self)) i32 (* 2 self::n)))
        (defun describe<T> ((it T)) i32 (where (Counted T)) (count it))
        (+ (describe (box-a::new 3)) (describe (box-b::new 3)))";
    assert_eq!(eval_ok(src), Value::Int(3 + 6));
}

#[test]
fn defmethod_on_a_generic_owner_can_carry_a_where_clause() {
    // A `where` clause directly on a `defmethod` (not via `impl`): the body
    // calls a trait method on the owner's own type variable. The
    // once-through diagnostic check resolves `(count self::x)` through the
    // bounds branch (`Expr::TraitCall`); the call site's specialization
    // re-checks with `A = box` concrete.
    let src = format!(
        "{} (defstruct pair<A> (x A) (y A))
            (defmethod count-both ((self pair<A>)) i32 (where (Counted A))
              (+ (count self::x) (count self::y)))
            (count-both (pair::new (box::new 20) (box::new 22)))",
        COUNTER_PRELUDE
    );
    assert_eq!(eval_ok(&src), Value::Int(42));
}

#[test]
fn impl_method_with_a_where_clause_supports_recursive_structural_dispatch() {
    // The recursive-impl shape the where clause exists for: `pr<A,B>`'s own
    // `same` requires `(Eq2 A) (Eq2 B)` and compares fields via the bounded
    // trait method — with a *nested* `pr` exercising impl-on-impl recursion.
    let src = "
        (deftrait Eq2 () (same ((self Self) (other Self)) bool))
        (impl Eq2 i32 (same ((self Self) (other Self)) bool (= self other)))
        (defstruct pr<A,B> (a A) (b B))
        (impl Eq2 pr<A,B>
          (same ((self Self) (other Self)) bool (where (Eq2 A) (Eq2 B))
            (if (same self::a other::a) (same self::b other::b) false)))
        (if (same (pr::new (pr::new 1 2) 3) (pr::new (pr::new 1 2) 3))
            (if (same (pr::new (pr::new 1 2) 3) (pr::new (pr::new 1 9) 3)) 0 1)
            0)";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn bounded_method_call_rejects_an_owner_argument_lacking_the_impl() {
    // Method-side mirror of the free-function call-site validation:
    // `pr<no-eq,i32>`'s `same` requires `(Eq2 A)`, and `no-eq` has no `Eq2`
    // impl — the *call* must fail to check with a real trait error, not an
    // opaque `NoSuchFunction` from inside the specialization drain.
    let src = "
        (deftrait Eq2 () (same ((self Self) (other Self)) bool))
        (impl Eq2 i32 (same ((self Self) (other Self)) bool (= self other)))
        (defstruct no-eq (n i32))
        (defstruct pr<A,B> (a A) (b B))
        (impl Eq2 pr<A,B>
          (same ((self Self) (other Self)) bool (where (Eq2 A) (Eq2 B))
            (if (same self::a other::a) (same self::b other::b) false)))
        (same (pr::new (no-eq::new 1) 2) (pr::new (no-eq::new 1) 2))";
    let err = check(src).expect_err("must fail to check");
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("does not implement trait"),
        "expected a trait-bound error, got: {}",
        msg
    );
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
            (defun describe<T> ((it T)) i32 (where (Counted T)) (count it))
            (describe (box2::new 5))",
        COUNTER_PRELUDE
    );
    assert!(check(&src).is_err());
}

// ---- nested generic call-site bound propagation ---------------------------
// `Checker::validate_where_bounds`'s two "concrete isn't fully resolved"
// cases, both arising only when a `where`-bounded call happens inside
// *another* generic function's own body-check (its type parameter not yet
// concrete) — see that function's doc comment.

/// A bare type parameter forwarded straight through to a bounded call, with
/// no matching `where` bound declared on the *enclosing* function: previously
/// silently skipped (deferred to a runtime `Expr::TraitCall` failure that
/// would only ever surface if `broken` were actually specialized and called
/// at a type lacking the impl); now a real check-time error, since nothing
/// about `broken`'s own signature promises its `T` implements `Counted`.
#[test]
fn forwarding_a_bare_type_parameter_without_a_matching_where_bound_is_a_type_error() {
    let src = format!(
        "{} (defun describe<T> ((it T)) i32 (where (Counted T)) (count it))
            (defun broken<T> ((it T)) i32 (describe it))",
        COUNTER_PRELUDE
    );
    let err = check(&src).expect_err("must fail to check");
    let msg = format!("{:?}", err);
    assert!(msg.contains("where"), "expected a where-bound propagation error, got: {}", msg);
}

/// The fix for the above: the enclosing function declares an equivalent
/// `where` bound on the same type variable it forwards — this must type-check
/// (validated symbolically, trait path match) and, once specialized at a
/// concrete type that really does implement `Counted`, evaluate correctly.
#[test]
fn forwarding_a_bare_type_parameter_with_a_matching_where_bound_type_checks_and_runs() {
    let src = format!(
        "{} (defun describe<T> ((it T)) i32 (where (Counted T)) (count it))
            (defun forwards<T> ((it T)) i32 (where (Counted T)) (describe it))
            (forwards (box::new 7))",
        COUNTER_PRELUDE
    );
    assert_eq!(eval_ok(&src), Value::Int(7));
}

/// A minimal trait/impl scaffold with an *associated type*, independent of
/// `Vector`/`Iter`/`doiter` (all prelude-provided — this file's `check`/
/// `eval_ok` don't load the prelude): `wrap<T>` is `Boxed` with `Item = T`,
/// so wrapping an outer function's own still-open type parameter `U` in
/// `wrap<U>` produces exactly the "assoc type resolves to an open variable"
/// shape `Checker::validate_where_bounds`'s second case exists for.
const BOXED_PRELUDE: &str = "
(deftrait Boxed ()
  (type Item)
  (unbox ((self Self)) Item))
(defstruct wrap<T> (v T))
(impl Boxed wrap<T>
  (type Item T)
  (unbox ((self Self)) T self::v))
(defun get-int<T> ((b T)) i32 (where (Boxed T (Item i32))) (unbox b))
";

/// A type parameter *wrapped* in a concrete generic type (not bare) forwarded
/// to a bounded call whose pin requirement is itself concrete but whose
/// *actual* associated-type resolution still mentions the open variable:
/// `get-int`'s `(Item i32)` pin can't be verified against `wrap<U>`'s own
/// `Item` (symbolically `U`, since `U` isn't resolved during `outer`'s own
/// body-check) — must not be rejected outright (the bug this fixes), but
/// isn't fully verified here either; real verification happens when `outer`
/// is specialized at a concrete `U` and this same call is re-checked with
/// `Item` fully resolved.
#[test]
fn forwarding_a_wrapped_open_type_variable_to_a_pinned_bound_does_not_reject_at_definition_time() {
    let src = format!("{} (defun outer<U> ((w wrap<U>)) i32 (get-int w))", BOXED_PRELUDE);
    assert!(check(&src).is_ok(), "must not reject a still-open Item pin at definition time");
}

/// The above, specialized and actually run at `U = i32` (a type that really
/// does satisfy `Item i32`) — the deferred validation this relies on
/// (`Self::specialize_defun_body`'s own re-check of `outer`'s body with `U`
/// concrete) must still catch a genuine mismatch, so this also proves the
/// skip isn't a silent hole: specializing at a non-`i32` element type is
/// exercised by the next test.
#[test]
fn forwarding_a_wrapped_open_type_variable_specializes_and_runs_at_a_satisfying_type() {
    let src = format!(
        "{} (defun outer<U> ((w wrap<U>)) i32 (get-int w))
            (outer (wrap::new 42))",
        BOXED_PRELUDE
    );
    assert_eq!(eval_ok(&src), Value::Int(42));
}

/// The deferred-validation side of the same mechanism: `outer` itself never
/// pins `U`, so it happily *defines*, but specializing it at a `wrap<T>`
/// whose `Item` is not `i32` must still fail once `U` is concrete —
/// `get-int`'s own pin requirement doesn't disappear just because the
/// outer wrapper's definition-time check couldn't see it yet.
#[test]
fn forwarding_a_wrapped_open_type_variable_rejects_at_specialization_when_the_pin_fails() {
    let src = format!(
        "{} (defun outer<U> ((w wrap<U>)) i32 (get-int w))
            (outer (wrap::new true))",
        BOXED_PRELUDE
    );
    assert!(check(&src).is_err(), "Item bool must still fail the Item i32 pin once U is concrete");
}

// ---- supertraits ---------------------------------------------------------

/// The error message of a failed `check`, for tests asserting *which* rule
/// rejected a form rather than just that something did.
fn check_err(src: &str) -> String {
    match check(src) {
        Ok(_) => panic!("expected a type error, but the form checked"),
        Err(e) => e.to_string(),
    }
}

/// `Ord`-shaped pair: a base trait and a trait inheriting it, plus a struct
/// implementing both — the arrangement every supertrait rule is stated over.
const SUPER_PRELUDE: &str = "
(deftrait Base ()
  (base-tag ((self Self)) i32))
(deftrait Derived (Base)
  (derived-tag ((self Self)) i32))
(defstruct cell (n i32))
(impl Base cell (base-tag ((self Self)) i32 self::n))
(impl Derived cell (derived-tag ((self Self)) i32 (* self::n 10)))
";

#[test]
fn a_supertrait_method_is_callable_on_a_bound_type_parameter() {
    // The bound names `Derived` only; `base-tag` comes from `Base`.
    let src = format!(
        "{}
         (defun both<T> ((x T)) i32 (where (Derived T)) (+ (base-tag x) (derived-tag x)))
         (both (cell::new 3))",
        SUPER_PRELUDE
    );
    assert_eq!(eval_ok(&src), Value::Int(33));
}

#[test]
fn a_supertrait_bound_discharges_a_callees_bound_on_the_base_trait() {
    // `outer` declares only `(Derived T)` but calls `inner`, which demands
    // `(Base T)` — the subtrait bound has to satisfy it.
    let src = format!(
        "{}
         (defun inner<T> ((x T)) i32 (where (Base T)) (base-tag x))
         (defun outer<T> ((x T)) i32 (where (Derived T)) (inner x))
         (outer (cell::new 7))",
        SUPER_PRELUDE
    );
    assert_eq!(eval_ok(&src), Value::Int(7));
}

#[test]
fn implementing_a_trait_without_its_supertrait_is_rejected() {
    let m = check_err(
        "(deftrait Base () (base-tag ((self Self)) i32))
         (deftrait Derived (Base) (derived-tag ((self Self)) i32))
         (defstruct cell (n i32))
         (impl Derived cell (derived-tag ((self Self)) i32 1))",
    );
    assert!(m.contains("requires"), "{}", m);
    assert!(m.contains("write it before this one"), "{}", m);
}

#[test]
fn a_supertrait_impl_written_after_the_subtrait_impl_is_rejected() {
    // Textual precedence: the obligation is discharged where `impl Derived`
    // is checked, so a later `impl Base` does not retroactively satisfy it.
    let m = check_err(
        "(deftrait Base () (base-tag ((self Self)) i32))
         (deftrait Derived (Base) (derived-tag ((self Self)) i32))
         (defstruct cell (n i32))
         (impl Derived cell (derived-tag ((self Self)) i32 1))
         (impl Base cell (base-tag ((self Self)) i32 2))",
    );
    assert!(m.contains("write it before this one"), "{}", m);
}

#[test]
fn an_unknown_supertrait_names_the_mandatory_slot() {
    let m = check_err("(deftrait Derived (Nope) (m ((self Self)) i32))");
    assert!(m.contains("unknown supertrait"), "{}", m);
    assert!(m.contains("write `()` for none"), "{}", m);
}

#[test]
fn omitting_the_supertrait_list_is_reported_against_that_slot() {
    // The pre-supertrait spelling: the first *method* lands in the
    // supertrait slot, so the diagnostic must point at the slot rule rather
    // than blaming the method name.
    let m = check_err("(deftrait Counted (count ((self Self)) i32))");
    assert!(m.contains("supertrait"), "{}", m);
}

#[test]
fn a_trait_cannot_redeclare_an_inherited_method() {
    let m = check_err(
        "(deftrait Base () (tag ((self Self)) i32))
         (deftrait Derived (Base) (tag ((self Self)) i32))",
    );
    assert!(m.contains("already inherited"), "{}", m);
}

#[test]
fn two_supertraits_declaring_the_same_method_are_rejected() {
    let m = check_err(
        "(deftrait A () (tag ((self Self)) i32))
         (deftrait B () (tag ((self Self)) i32))
         (deftrait C (A B) (c-only ((self Self)) i32))",
    );
    assert!(m.contains("inherited from both"), "{}", m);
}

#[test]
fn a_diamond_inherits_the_shared_method_once() {
    // D(B,C), B(A), C(A): `A::tag` reaches the linearizer twice, declared by
    // `A` both times, so it keeps exactly one slot and `D` stays usable.
    let src = "
        (deftrait A () (tag ((self Self)) i32))
        (deftrait B (A) (b-tag ((self Self)) i32))
        (deftrait C (A) (c-tag ((self Self)) i32))
        (deftrait D (B C) (d-tag ((self Self)) i32))
        (defstruct cell (n i32))
        (impl A cell (tag ((self Self)) i32 self::n))
        (impl B cell (b-tag ((self Self)) i32 2))
        (impl C cell (c-tag ((self Self)) i32 3))
        (impl D cell (d-tag ((self Self)) i32 4))
        (defun sum<T> ((x T)) i32 (where (D T))
          (+ (tag x) (+ (b-tag x) (+ (c-tag x) (d-tag x)))))
        (sum (cell::new 1))";
    assert_eq!(eval_ok(src), Value::Int(10));
}

#[test]
fn a_supertrait_listed_twice_is_rejected() {
    let m = check_err(
        "(deftrait A () (tag ((self Self)) i32))
         (deftrait B (A A) (b ((self Self)) i32))",
    );
    assert!(m.contains("twice"), "{}", m);
}

#[test]
fn a_supertrait_with_an_associated_type_must_pin_it() {
    let m = check_err(
        "(deftrait Src () (type Item) (next ((self Self)) Item))
         (deftrait CharSrc (Src) (rewind ((self Self)) ()))",
    );
    assert!(m.contains("must be pinned"), "{}", m);
}

#[test]
fn a_supertrait_associated_type_pin_resolves_an_inherited_method() {
    // `CharSrc` pins `Src`'s `Item` to `i32`, so the inherited `next`
    // returns `i32` — a `bool` context must be a type error.
    let src = "
        (deftrait Src () (type Item) (next ((self Self)) Item))
        (deftrait CharSrc ((Src (Item i32))) (rewind ((self Self)) ()))
        (defstruct counter (n i32))
        (impl Src counter (type Item i32) (next ((self Self)) i32 self::n))
        (impl CharSrc counter (rewind ((self Self)) () ()))
        (defun peek<T> ((x T)) i32 (where (CharSrc T)) (next x))
        (peek (counter::new 5))";
    assert_eq!(eval_ok(src), Value::Int(5));
}

// ---- default method bodies ----------------------------------------------

const DEFAULTED: &str = "
(deftrait Eq2 ()
  (same ((self Self) (other Self)) bool)
  (differs ((self Self) (other Self)) bool
    (if (same self other) false true)))
(defstruct point (x i32))
(impl Eq2 point (same ((self Self) (other Self)) bool (= self::x other::x)))
";

#[test]
fn an_omitted_method_uses_the_traits_default_body() {
    let src = format!("{} (differs (point::new 1) (point::new 2))", DEFAULTED);
    assert_eq!(eval_ok(&src), Value::Bool(true));
}

#[test]
fn a_default_body_calls_the_impls_own_core_method() {
    let src = format!("{} (differs (point::new 3) (point::new 3))", DEFAULTED);
    assert_eq!(eval_ok(&src), Value::Bool(false));
}

#[test]
fn an_impl_can_override_a_default_body() {
    let src = "
        (deftrait Eq2 ()
          (same ((self Self) (other Self)) bool)
          (differs ((self Self) (other Self)) bool (if (same self other) false true)))
        (defstruct point (x i32))
        (impl Eq2 point
          (same ((self Self) (other Self)) bool (= self::x other::x))
          ;; deliberately wrong, to prove the written body wins
          (differs ((self Self) (other Self)) bool false))
        (differs (point::new 1) (point::new 2))";
    assert_eq!(eval_ok(src), Value::Bool(false));
}

#[test]
fn a_default_body_resolves_names_in_the_traits_module() {
    // `helper` is private to module `m`, where the trait is declared; the
    // default body must still find it from an `impl` written outside.
    let src = "
        (module m
          (defun helper ((n i32)) i32 (* n 2))
          (deftrait Doubler ()
            (base ((self Self)) i32)
            (doubled ((self Self)) i32 (helper (base self)))))
        (defstruct box (n i32))
        (impl m::Doubler box (base ((self Self)) i32 self::n))
        (doubled (box::new 21))";
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn a_trailing_string_on_a_bodyless_signature_is_a_default_body_not_a_docstring() {
    // CL's rule, unchanged: a lone trailing string is the return value. So
    // this declares a default body returning "doc" — which is exactly why a
    // bodyless signature can never be documented separately.
    let src = "(deftrait T () (f ((self Self)) string \"doc\"))
               (defstruct s (n i32))
               (impl T s)
               (f (s::new 1))";
    assert_eq!(eval_string(src), "doc");
}

// ---- a trait's methods are as visible as the trait ----------------------

#[test]
fn a_default_body_reaches_the_impls_own_methods_across_module_lines() {
    // Regression: a default body is checked in the *trait's* namespace, so
    // when the implementing type lives in a module the trait does not
    // enclose, the sibling method it calls was invisible and the whole
    // `impl` failed to check. Nothing about an `impl` is private —  `pub`
    // cannot even be written on one.
    let src = "
        (deftrait Eq3 ()
          (same ((self Self) (other Self)) bool)
          (differs ((self Self) (other Self)) bool (if (same self other) false true)))
        (module m
          (pub defstruct point (x i32))
          (impl Eq3 point (same ((self Self) (other Self)) bool (= self::x other::x))))
        (differs (m::point::new 1) (m::point::new 2))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn a_trait_method_is_callable_from_outside_the_implementing_module() {
    let src = "
        (deftrait Named ()
          (label ((self Self)) string))
        (module m
          (pub defstruct tag (n i32))
          (impl Named tag (label ((self Self)) string \"tag\")))
        (label (m::tag::new 1))";
    assert_eq!(eval_string(src), "tag");
}

// ---- default bodies are checked at the declaration -----------------------
//
// `Checker::precheck_trait_defaults`, the `deftrait` counterpart of the
// blanket impl's declaration-time body check (tests/blanket_impl_test.rs).
// Before it, a default body was only ever checked by an `impl` that inherited
// it — so a default no `impl` omitted was never checked at all.

#[test]
fn an_unused_default_body_is_type_checked_at_the_deftrait() {
    // No `impl` anywhere, so nothing ever replays this body. Its return type
    // is still wrong, and Rust would say so at the `trait` too.
    let m = check_err(
        "(deftrait T ()
           (n ((self Self)) i32)
           (twice ((self Self)) i32 \"two\"))",
    );
    assert!(m.contains("expected I32") || m.contains("expected Str"), "{}", m);
}

#[test]
fn an_unused_default_body_may_not_call_an_unknown_function() {
    let m = check_err(
        "(deftrait T ()
           (n ((self Self)) i32)
           (twice ((self Self)) i32 (triple self)))",
    );
    assert!(m.contains("no such function"), "{}", m);
}

#[test]
fn a_default_body_may_call_a_sibling_method_on_self_at_the_declaration() {
    // `Self: T` is what makes `(n self)` callable while `Self` is still a
    // type variable — the bound the precheck puts in scope.
    assert!(check(
        "(deftrait T ()
           (n ((self Self)) i32)
           (twice ((self Self)) i32 (* 2 (n self))))"
    )
    .is_ok());
}

#[test]
fn a_default_body_may_call_an_inherited_method_on_self() {
    assert!(check(
        "(deftrait Base () (n ((self Self)) i32))
         (deftrait Sub (Base) (twice ((self Self)) i32 (* 2 (n self))))"
    )
    .is_ok());
}

#[test]
fn a_default_body_is_checked_against_the_traits_associated_types() {
    // `Item` stays a type variable, pinned to itself: a body returning what
    // the sibling returns fits, and one returning an `i32` does not.
    assert!(check(
        "(deftrait Boxed ()
           (type Item)
           (unwrap ((self Self)) Item)
           (unwrap-again ((self Self)) Item (unwrap self)))"
    )
    .is_ok());
    let m = check_err(
        "(deftrait Boxed ()
           (type Item)
           (unwrap ((self Self)) Item)
           (unwrap-again ((self Self)) Item 1))",
    );
    assert!(m.contains("item"), "{}", m);
}

#[test]
fn a_default_body_rejected_at_the_declaration_leaves_no_half_trait_behind() {
    // The error escapes `check_deftrait` after the `TraitDef` is registered
    // (the body needs the entry to resolve `self` calls against), so what a
    // driver that keeps going sees must still be a coherent trait.
    let m = check_err(
        "(deftrait T ()
           (n ((self Self)) i32)
           (twice ((self Self)) i32 \"two\"))
         (defstruct s (n i32))
         (impl T s (n ((self Self)) i32 self::n))",
    );
    assert!(m.contains("expected I32") || m.contains("expected Str"), "{}", m);
}

// ---- impl conformance ----------------------------------------------------

#[test]
fn an_impl_missing_an_undefaulted_method_is_rejected() {
    let m = check_err(
        "(deftrait T () (a ((self Self)) i32) (b ((self Self)) i32))
         (defstruct s (n i32))
         (impl T s (a ((self Self)) i32 1))",
    );
    assert!(m.contains("missing method `b`"), "{}", m);
}

#[test]
fn an_impl_of_a_method_the_trait_does_not_declare_is_rejected() {
    let m = check_err(
        "(deftrait T () (a ((self Self)) i32))
         (defstruct s (n i32))
         (impl T s (a ((self Self)) i32 1) (zz ((self Self)) i32 2))",
    );
    assert!(m.contains("is not a method of"), "{}", m);
}

#[test]
fn implementing_an_inherited_method_in_the_subtraits_impl_is_rejected() {
    let m = check_err(
        "(deftrait Base () (tag ((self Self)) i32))
         (deftrait Sub (Base) (extra ((self Self)) i32))
         (defstruct s (n i32))
         (impl Base s (tag ((self Self)) i32 1))
         (impl Sub s (extra ((self Self)) i32 2) (tag ((self Self)) i32 3))",
    );
    assert!(m.contains("supertrait's own `impl`"), "{}", m);
}

#[test]
fn an_impl_giving_the_same_method_twice_is_rejected() {
    let m = check_err(
        "(deftrait T () (a ((self Self)) i32))
         (defstruct s (n i32))
         (impl T s (a ((self Self)) i32 1) (a ((self Self)) i32 2))",
    );
    assert!(m.contains("given twice") || m.contains("already defined"), "{}", m);
}

#[test]
fn an_impl_with_the_wrong_return_type_is_rejected() {
    let m = check_err(
        "(deftrait T () (a ((self Self)) i32))
         (defstruct s (n i32))
         (impl T s (a ((self Self)) string \"x\"))",
    );
    assert!(m.contains("declares"), "{}", m);
}

#[test]
fn an_impl_with_the_wrong_parameter_type_is_rejected() {
    let m = check_err(
        "(deftrait T () (a ((self Self) (k i32)) i32))
         (defstruct s (n i32))
         (impl T s (a ((self Self) (k string)) i32 1))",
    );
    assert!(m.contains("declares"), "{}", m);
}

#[test]
fn an_impl_omitting_an_associated_type_is_rejected() {
    let m = check_err(
        "(deftrait Src () (type Item) (next ((self Self)) Item))
         (defstruct s (n i32))
         (impl Src s (next ((self Self)) i32 self::n))",
    );
    assert!(m.contains("associated type"), "{}", m);
}

/// A trait method declared to return `Self`, called on a `where`-bounded type
/// variable, must come back as *that variable* — not the literal `Self` type
/// variable the trait's template is written in.
///
/// The bounds branch of `Checker::check_instance_method` used to substitute
/// only the trait's associated types into the method's return type, so
/// `(add a b)` under `(where (Add T))` yielded `self` and the enclosing
/// generic's `T` return position rejected it. Nothing in the prelude caught
/// it because no prelude trait method returns `Self` — `Eq`/`Ord` return
/// `bool`, `Iter` returns an associated type — and it only surfaced when the
/// CL-parity plan's `Number` trait layer (Phase 1a) needed arithmetic to be
/// requestable from generic code.
#[test]
fn a_self_returning_trait_method_resolves_to_the_bounded_type_variable() {
    assert_eq!(
        eval_ok(
            "(deftrait Add () (add ((self Self) (other Self)) Self))
             (impl Add i32 (add ((self Self) (other Self)) Self (+ self other)))
             (defun sum3<T> ((a T) (b T) (c T)) T (where (Add T)) (add (add a b) c))
             (sum3 1 2 3)"
        ),
        Value::Int(6)
    );
}

/// The same substitution, one supertrait up: an *inherited* `Self`-returning
/// method reached through a subtrait bound.
#[test]
fn an_inherited_self_returning_trait_method_resolves_to_the_bounded_type_variable() {
    assert_eq!(
        eval_ok(
            "(deftrait Add () (add ((self Self) (other Self)) Self))
             (deftrait Double (Add) (twice ((self Self)) Self (add self self)))
             (impl Add i32 (add ((self Self) (other Self)) Self (+ self other)))
             (impl Double i32)
             (defun quad<T> ((a T)) T (where (Double T)) (twice (twice a)))
             (quad 3)"
        ),
        Value::Int(12)
    );
}

// ---- an associated type inside a generic name -----------------------------

/// A trait method's *default body* whose signature mentions the trait's
/// associated type inside a generic argument — `Vector<Item>`, not a bare
/// `Item`.
///
/// The reader reads `Vector<Item>` as a single symbol (the same reason
/// `impl<T>` is one), so `Checker::subst_value`'s whole-name lookup never
/// reached the `Item` inside it: the inherited signature stayed
/// `(holder vector<item>)` while the trait's own declaration substituted to
/// `(holder vector<char>)`, and the two failed to match. `Option<Item>` had
/// never shown it because no default body's signature used an associated
/// type until Stage 9d's stream layering did.
const SINK: &str = "
(defstruct holder (n i32))
(deftrait Sink ()
  (type Item)
  (size ((self Self)) i32)
  (put-all ((self Self) (vs Vector<Item>)) i32 (size self)))
";

#[test]
fn an_associated_type_inside_a_generic_is_substituted() {
    // The call is the proof: `(two)` is a `Vector<char>`, and it is only
    // accepted because the inherited `put-all` was registered taking one.
    assert_eq!(
        eval_ok(&format!(
            "{}
             (impl Sink holder
               (type Item char)
               (size ((self Self)) i32 self::n))
             (defun empty-chars () Vector<char> (Vector::new))
             (defun two () Vector<char>
               (let ((v (empty-chars)))
                 (push v #\\a)
                 (push v #\\b)
                 v))
             (put-all (holder::new 7) (two))",
            SINK
        )),
        Value::Int(7)
    );
}

/// The substitution goes through the *structure* of the type, so it reaches
/// any depth and works for `Self` as well as for an associated type.
#[test]
fn substitution_reaches_a_nested_argument_and_self() {
    check(
        "(defstruct holder (n i32))
         (deftrait Sink ()
           (type Item)
           (size ((self Self)) i32)
           (nested ((self Self) (vs Vector<Option<Item>>)) i32 (size self))
           (selves ((self Self) (vs Vector<Self>)) i32 (size self)))
         (impl Sink holder
           (type Item char)
           (size ((self Self)) i32 self::n))",
    )
    .expect("check failed");
}

/// What emitting *structure* rather than a rebuilt name buys: an `impl` may
/// bind the associated type to something the name grammar cannot spell, and
/// `Vector<(fn (i32) i32)>` has no written form at all. The substituted
/// signature comes out as the applied spelling `(vector (fn (i32) i32))`
/// instead — see `tests/type_test.rs`.
#[test]
fn an_associated_type_may_be_bound_to_a_type_no_name_can_spell() {
    check(
        "(defstruct holder (n i32))
         (deftrait Sink ()
           (type Item)
           (size ((self Self)) i32)
           (put-all ((self Self) (vs Vector<Item>)) i32 (size self)))
         (impl Sink holder
           (type Item (fn (i32) i32))
           (size ((self Self)) i32 self::n))",
    )
    .expect("check failed");
}

/// A `:dyn` binding, which the name grammar *can* spell — the two spellings
/// have to agree, since the conformance check compares parsed types.
#[test]
fn an_associated_type_may_be_bound_to_a_trait_object() {
    check(
        "(defstruct holder (n i32))
         (deftrait Drawable () (draw ((self Self)) ()))
         (deftrait Sink ()
           (type Item)
           (size ((self Self)) i32)
           (put-all ((self Self) (vs Vector<Item>)) i32 (size self)))
         (impl Sink holder
           (type Item :dyn Drawable)
           (size ((self Self)) i32 self::n))",
    )
    .expect("check failed");
}
