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

/// The text of a `string` result.
///
/// A `string` is a heap `Value::Str` since the scalar unification, so reading
/// one needs the heap it lives in — and `run` above drops its heap on return.
/// Hence this parallel runner, which reads the text out first.
fn eval_string(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    load_compiler(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        RtValue::Sexpr(typelisp::Value::Str(id)) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

fn eval_err(src: &str) -> String {
    run(src).expect_err("expected a type error")
}

/// Two shapes implementing one trait — the setup every test below builds on.
const SHAPES: &str = r#"
(deftrait Drawable ()
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
    assert_eq!(eval_string(&src), "circle");
}

#[test]
fn the_same_call_site_dispatches_to_each_implementation() {
    // The whole point: one `(draw d)` in one compiled body, two answers.
    let src = format!(
        "{SHAPES}
         (defun render ((d :dyn Drawable)) string (draw d))
         (append (render (circle::new 3)) (render (square::new 2)))"
    );
    assert_eq!(eval_string(&src), "circlesquare");
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
        (deftrait Scaler () (scale ((self Self) (k i32)) i32))
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
    assert_eq!(eval_string(&src), "square");
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
    assert_eq!(eval_string(&src), "circlesquare");
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
        (deftrait Speak () (say ((self Self)) string))
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
        (deftrait Zeroed () (zero ((n i32)) i32))
        (defun f ((z :dyn Zeroed)) i32 0)
        (f 1)"#;
    let m = eval_err(src);
    assert!(m.contains("no `self` receiver"), "{}", m);
}

#[test]
fn a_method_returning_self_makes_a_trait_not_object_safe() {
    let src = r#"
        (deftrait Cloneable () (dup ((self Self)) Self))
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
fn upcasting_to_a_non_supertrait_is_rejected() {
    let src = format!(
        "{SHAPES}
         (deftrait Named () (name ((self Self)) string))
         (impl Named circle (name ((self Self)) string \"c\"))
         (defun render ((d :dyn Drawable)) string (draw d))
         (defun relabel ((n :dyn Named)) string (render n))
         (relabel (circle::new 1))"
    );
    let m = eval_err(&src);
    assert!(m.contains("does not inherit"), "{}", m);
}

// ---- `:dyn` outside a type position -------------------------------------

#[test]
fn dyn_in_a_value_position_is_a_type_error() {
    let m = eval_err("(defun f () i32 (:dyn Drawable))");
    assert!(m.contains("may only appear in a type position"), "{}", m);
}

// ---- supertraits through a trait object ---------------------------------

/// A two-level trait chain, both levels implemented by two concrete types —
/// enough to show that an inherited slot dispatches per implementation.
const CHAIN: &str = r#"
(deftrait Named ()
  (name ((self Self)) string))
(deftrait Greeter (Named)
  (greeting ((self Self)) string))
(defstruct dog (n i32))
(defstruct cat (n i32))
(impl Named dog (name ((self Self)) string "dog"))
(impl Greeter dog (greeting ((self Self)) string "woof"))
(impl Named cat (name ((self Self)) string "cat"))
(impl Greeter cat (greeting ((self Self)) string "meow"))
"#;

#[test]
fn a_dyn_subtrait_can_call_an_inherited_method() {
    let src = format!(
        "{CHAIN}
         (defun describe ((g :dyn Greeter)) string (name g))
         (describe (dog::new 1))"
    );
    assert_eq!(eval_string(&src), "dog");
}

#[test]
fn an_inherited_slot_dispatches_per_implementation() {
    let src = format!(
        "{CHAIN}
         (defun describe ((g :dyn Greeter)) string
           (append (name g) (append \":\" (greeting g))))
         (append (describe (dog::new 1)) (append \"/\" (describe (cat::new 2))))"
    );
    assert_eq!(eval_string(&src), "dog:woof/cat:meow");
}

#[test]
fn inherited_slots_precede_the_subtraits_own() {
    // The layout contract: `Named`'s methods occupy the low slots of
    // `Greeter`'s vtable, in `Named`'s own order, before `Greeter`'s. If the
    // two ever swapped, `name` would call `greeting`'s body and vice versa.
    let src = format!(
        "{CHAIN}
         (defun who ((g :dyn Greeter)) string (name g))
         (defun what ((x :dyn Greeter)) string (greeting x))
         (append (who (cat::new 1)) (what (cat::new 1)))"
    );
    assert_eq!(eval_string(&src), "catmeow");
}

#[test]
fn a_method_of_neither_the_trait_nor_its_supertraits_is_rejected() {
    let src = format!(
        "{CHAIN}
         (defun describe ((g :dyn Greeter)) string (nope g))
         (describe (dog::new 1))"
    );
    let m = eval_err(&src);
    assert!(m.contains("own or inherited methods"), "{}", m);
}

#[test]
fn a_dyn_of_the_supertrait_still_sees_only_the_supertraits_methods() {
    let src = format!(
        "{CHAIN}
         (defun describe ((n :dyn Named)) string (greeting n))
         (describe (dog::new 1))"
    );
    let m = eval_err(&src);
    assert!(m.contains("own or inherited methods"), "{}", m);
}

// ---- upcasting ----------------------------------------------------------

#[test]
fn a_dyn_subtrait_upcasts_to_its_supertrait() {
    let src = format!(
        "{CHAIN}
         (defun label ((n :dyn Named)) string (name n))
         (defun via ((g :dyn Greeter)) string (label g))
         (append (via (dog::new 1)) (via (cat::new 2)))"
    );
    assert_eq!(eval_string(&src), "dogcat");
}

#[test]
fn an_upcast_value_still_dispatches_to_its_own_concrete_type() {
    // The box is reused unchanged, so the vtable still belongs to the
    // original concrete type — an upcast must not flatten to one impl.
    let src = format!(
        "{CHAIN}
         (defun label ((n :dyn Named)) string (name n))
         (defun via ((g :dyn Greeter)) string (append (label g) (greeting g)))
         (append (via (dog::new 1)) (via (cat::new 2)))"
    );
    assert_eq!(eval_string(&src), "dogwoofcatmeow");
}

#[test]
fn an_explicit_as_upcasts_a_trait_object() {
    let src = format!(
        "{CHAIN}
         (defun label ((n :dyn Named)) string (name n))
         (defun via ((g :dyn Greeter)) string (label (as :dyn Named g)))
         (via (dog::new 1))"
    );
    assert_eq!(eval_string(&src), "dog");
}

#[test]
fn downcasting_to_a_subtrait_is_rejected() {
    let src = format!(
        "{CHAIN}
         (defun shout ((g :dyn Greeter)) string (greeting g))
         (defun via ((n :dyn Named)) string (shout n))
         (via (dog::new 1))"
    );
    let m = eval_err(&src);
    assert!(m.contains("does not inherit"), "{}", m);
}

/// `D(B,C)`: `C`'s slots sit after `B`'s in `D`'s vtable, so the layouts
/// share no prefix and the box has to be re-made around `C`'s own table —
/// the one case `Expr::DynUpcast` exists for. Getting `2` (and not `1`)
/// proves the swap happened: reusing `D`'s table would have called `b-tag`,
/// which occupies slot 0 there.
const DIAMOND: &str = r#"
(deftrait B () (b-tag ((self Self)) i32))
(deftrait C () (c-tag ((self Self)) i32))
(deftrait D (B C) (d-tag ((self Self)) i32))
(defstruct cell (n i32))
(defstruct pair (n i32))
(impl B cell (b-tag ((self Self)) i32 1))
(impl C cell (c-tag ((self Self)) i32 2))
(impl D cell (d-tag ((self Self)) i32 3))
(impl B pair (b-tag ((self Self)) i32 4))
(impl C pair (c-tag ((self Self)) i32 5))
(impl D pair (d-tag ((self Self)) i32 6))
"#;

#[test]
fn upcasting_to_a_non_first_supertrait_switches_to_that_supertraits_vtable() {
    let src = format!(
        "{DIAMOND}
         (defun only-c ((c :dyn C)) i32 (c-tag c))
         (defun via ((d :dyn D)) i32 (only-c d))
         (via (cell::new 0))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(2));
}

#[test]
fn a_non_first_supertrait_upcast_still_dispatches_per_concrete_type() {
    // The swapped-in table belongs to the *same* concrete type, so two
    // implementations must stay distinguishable after the conversion.
    let src = format!(
        "{DIAMOND}
         (defun only-c ((c :dyn C)) i32 (c-tag c))
         (defun via ((d :dyn D)) i32 (only-c d))
         (+ (* 10 (via (cell::new 0))) (via (pair::new 0)))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(25));
}

#[test]
fn an_explicit_as_upcasts_to_a_non_first_supertrait() {
    let src = format!(
        "{DIAMOND}
         (defun only-c ((c :dyn C)) i32 (c-tag c))
         (defun via ((d :dyn D)) i32 (only-c (as :dyn C d)))
         (via (cell::new 0))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(2));
}

#[test]
fn upcasting_twice_reaches_a_supertrait_of_the_supertrait() {
    // `E(D)` boxed, upcast to `C` (non-prefix, a real conversion), then the
    // result upcast again to `A` — which is not a prefix of `C` either, so
    // the second conversion has to find a table registered for a box the
    // *first* conversion produced.
    let src = r#"
        (deftrait A () (a-tag ((self Self)) i32))
        (deftrait X () (x-tag ((self Self)) i32))
        (deftrait B () (b-tag ((self Self)) i32))
        (deftrait C (X A) (c-tag ((self Self)) i32))
        (deftrait D (B C) (d-tag ((self Self)) i32))
        (defstruct cell (n i32))
        (impl A cell (a-tag ((self Self)) i32 1))
        (impl X cell (x-tag ((self Self)) i32 2))
        (impl B cell (b-tag ((self Self)) i32 3))
        (impl C cell (c-tag ((self Self)) i32 4))
        (impl D cell (d-tag ((self Self)) i32 5))
        (defun only-a ((a :dyn A)) i32 (a-tag a))
        (defun only-c ((c :dyn C)) i32 (only-a c))
        (defun via ((d :dyn D)) i32 (only-c d))
        (via (cell::new 0))"#;
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn upcasting_reaches_a_trait_that_is_a_prefix_of_the_source_but_not_of_the_step() {
    // Why `Expr::DynBox::supers` records the *whole* supertrait closure and
    // not only the traits whose layout is not a prefix of the boxed one.
    // `B(A)` puts `A`'s methods at the front of `D`'s vtable, so `D -> A`
    // alone would be a free retype — but the route taken is `D -> C -> A`,
    // and `C(X,A)` has `X`'s methods first, so the second step is a real
    // conversion asking for a table `D`'s own prefix test would have
    // considered unnecessary.
    let src = r#"
        (deftrait A () (a-tag ((self Self)) i32))
        (deftrait X () (x-tag ((self Self)) i32))
        (deftrait B (A) (b-tag ((self Self)) i32))
        (deftrait C (X A) (c-tag ((self Self)) i32))
        (deftrait D (B C) (d-tag ((self Self)) i32))
        (defstruct cell (n i32))
        (impl A cell (a-tag ((self Self)) i32 1))
        (impl X cell (x-tag ((self Self)) i32 2))
        (impl B cell (b-tag ((self Self)) i32 3))
        (impl C cell (c-tag ((self Self)) i32 4))
        (impl D cell (d-tag ((self Self)) i32 5))
        (defun only-a ((a :dyn A)) i32 (a-tag a))
        (defun only-c ((c :dyn C)) i32 (only-a c))
        (defun via ((d :dyn D)) i32 (only-c d))
        (via (cell::new 0))"#;
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn an_upcast_box_is_still_a_trait_object_for_match() {
    // Box transparency survives the conversion: the re-made box holds the
    // same concrete value, so `match` still downcasts to it.
    let src = format!(
        "{DIAMOND}
         (defun name-of ((c :dyn C)) i32
           (match c ((cell _) 10) ((pair _) 20) (_ 0)))
         (defun via ((d :dyn D)) i32 (name-of d))
         (+ (via (cell::new 0)) (via (pair::new 0)))"
    );
    assert_eq!(eval_ok(&src), RtValue::Int(30));
}

#[test]
fn upcasting_to_the_first_supertrait_of_a_multi_supertrait_chain_works() {
    let src = r#"
        (deftrait B () (b-tag ((self Self)) i32))
        (deftrait C () (c-tag ((self Self)) i32))
        (deftrait D (B C) (d-tag ((self Self)) i32))
        (defstruct cell (n i32))
        (impl B cell (b-tag ((self Self)) i32 1))
        (impl C cell (c-tag ((self Self)) i32 2))
        (impl D cell (d-tag ((self Self)) i32 3))
        (defun only-b ((b :dyn B)) i32 (b-tag b))
        (defun via ((d :dyn D)) i32 (only-b d))
        (via (cell::new 0))"#;
    assert_eq!(eval_ok(src), RtValue::Int(1));
}
