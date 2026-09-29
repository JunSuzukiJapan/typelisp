//! Tests for putting a user-defined ADT instance (`defstruct`/`defenum`) into
//! a `Sexpr` — CL's cons cells hold arbitrary objects (`(list (make-point ..)
//! 42)` is valid, printed `#S(POINT :X 1 :Y 2)`); `typelisp`'s `Sexpr` was a
//! closed built-in sum type with no such constructor. See
//! `~/.claude/plans/async-conjuring-hanrahan.md` Stage 1: `Checker::check_inner`'s
//! expected-type fallback and `Checker::wrap_rest_elem` now widen any
//! `is_heap_repr` ADT into `Sexpr` as a cost-free retype (its runtime
//! representation is already `RtValue::Sexpr(Value::Boxed(_))`), and wrap a
//! scalar (`int`/`f64`/.../`Str`) into its `Sexpr` constructor exactly as
//! `&rest`/`format` args already did.

extern crate typelisp;
use typelisp::{load_compiler, Checker, Error, EvalError, Heap, Interp, Reader, Type, Value};

/// Check every form; return the last one's expression type — `None` if that
/// form was a definition. The type is no longer part of the checked form (see
/// `check::core::Checked`), so it comes from `Checker::expr_type`.
fn check(src: &str) -> Result<Option<Type>, Error> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    for v in vs {
        chk.check_form(&mut h, &interp, v)?;
    }
    Ok(chk.expr_type().cloned())
}

fn run_with_heap(src: &str) -> Result<(Heap, Value), EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
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
    Ok((h, last))
}

fn eval_ok(src: &str) -> Value {
    run_with_heap(src).expect("eval failed").1
}

// ---- insertion: struct/enum retype into Sexpr -------------------------------

#[test]
fn list_of_a_struct_instance_and_an_int_type_checks_as_an_option_sexpr() {
    // `(list ...)` is S-expression data, and that type is `Option<Sexpr>`
    // since the empty list moved there — `(list)` has to be spellable.
    let src = "(defstruct point (x int) (y int)) (list (point::new 1 2) 42)";
    assert_eq!(
        check(src).expect("check failed"),
        Some(Type::Named(
            typelisp::Path::root("option"),
            vec![Type::Named(typelisp::Path::root("sexpr"), vec![])]
        ))
    );
}

#[test]
fn list_mixes_a_struct_instance_with_a_scalar() {
    let (h, v) = run_with_heap("(defstruct point (x int) (y int)) (list (point::new 1 2) 42)")
        .expect("eval failed");
    let car = h.car(v).expect("cons");
    match car {
        Value::Boxed(id) => {
            assert_eq!(&*h.struct_type_name(id), "point");
            assert_eq!(h.struct_field(id, 0), Value::Int(1));
            assert_eq!(h.struct_field(id, 1), Value::Int(2));
        }
        other => panic!("expected boxed struct in car, got {:?}", other),
    }
    let cdr = h.cdr(v).expect("cons");
    let cadr = h.car(cdr).expect("cons");
    // The scalar `42` went through the auto-wrap (real `int` ctor,
    // not a retype), so it decodes as a `Sexpr::i32` payload.
    assert_eq!(cadr, Value::Int(42));
}

#[test]
fn list_holds_an_enum_variant() {
    let (h, v) = run_with_heap(
        "(defenum color (red) (green) (blue)) (list (color::red) (color::blue))",
    )
    .expect("eval failed");
    let car = h.car(v).expect("cons");
    match car {
        Value::Boxed(id) => {
            assert_eq!(&*h.enum_type_name(id), "color");
            assert_eq!(h.enum_variant(id), 0);
        }
        other => panic!("expected boxed enum in car, got {:?}", other),
    }
}

#[test]
fn setf_on_a_struct_shared_via_a_list_is_visible_through_the_list() {
    // The retype is representation-free: the struct instance stored in the
    // `Sexpr` list is the exact same heap box the outer `p` binding refers
    // to, so mutating one is visible through the other.
    let (h, v) = run_with_heap(
        "(defstruct point (x int) (y int))
         (let ((p (point::new 1 2)))
           (let ((l (list p 42)))
             (setf p::x 99)
             l))",
    )
    .expect("eval failed");
    let car_cell = v;
    let car = h.car(car_cell).expect("cons");
    let Value::Boxed(id) = car else { panic!("expected boxed struct") };
    assert_eq!(h.struct_field(id, 0), Value::Int(99));
}

#[test]
fn println_format_accepts_a_struct_argument_via_a_tilde_a_directive() {
    // `wrap_rest_elem`'s heap-repr retype: `println`'s `&rest` args go
    // through the same path `&rest`/`format` args always did, now widened to
    // accept a heap-repr ADT with no `sexpr_ctor_for` wrap. Printing reads
    // the prelude's printer control variables, so this one needs a prelude.
    let src = "(defstruct point (x int) (y int)) (println \"~a\" (point::new 1 2))";
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    typelisp::load_prelude(&mut h, &mut chk, &mut interp);
    for v in Reader::new().read_all(&mut h, src).expect("read failed") {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        interp.exec(&mut h, tl).expect("eval failed");
    }
}

#[test]
fn a_struct_instance_flows_through_a_compiled_function_still_shared() {
    // Stage 1's runtime-cost-free claim: the retype needs no new compiled
    // node, so it must work identically once the enclosing function is
    // explicitly compiled (JIT) — `compile`'s own construct-sexpr lowering
    // handles the `int`-wrap `Expr::Construct` unchanged. No manual
    // `COMPILE_LOCK` guard here — `Interp::exec`'s own `(compile name)`
    // handling already takes it internally (see `compile_test.rs`'s
    // `run_with_compiler`/`compile_dispatches_a_defun_call_to_native_code`,
    // which call it exactly this way); wrapping this whole call in an
    // *outer* guard self-deadlocks the very first time `load_compiler`'s
    // own `install_island_bitcode` tries to acquire the same non-reentrant
    // `Mutex` from this same thread.
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_compiler(&mut h, &mut chk, &mut interp);
    let src = "(defstruct point (x int) (y int))
         (defun wrap ((p point)) Option<Sexpr> (list p 42))
         (compile wrap)
         (let ((p (point::new 1 2)))
           (let ((l (wrap p)))
             (setf p::x 7)
             l))";
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind).expect("eval failed") {
            last = val;
        }
    }
    let car_cell = last;
    let car = h.car(car_cell).expect("cons");
    let Value::Boxed(id) = car else { panic!("expected boxed struct") };
    assert_eq!(h.struct_field(id, 0), Value::Int(7));
}

// ---- extraction: match downcast patterns -------------------------------------

#[test]
fn match_destructures_a_struct_downcast_pattern() {
    let v = eval_ok(
        "(defstruct point (x int) (y int))
         (match (sexpr-car (list (point::new 1 2)))
           ((point a b) (+ a b))
           (_ 0))",
    );
    assert_eq!(v, Value::Int(3));
}

#[test]
fn match_destructures_a_bare_enum_variant_after_use() {
    let v = eval_ok(
        "(defenum color (red) (green) (blue))
         (use color)
         (match (sexpr-car (list (color::green)))
           ((red) 0)
           ((green) 1)
           ((blue) 2)
           (_ 99))",
    );
    assert_eq!(v, Value::Int(1));
}

#[test]
fn match_destructures_a_qualified_enum_variant_without_use() {
    let v = eval_ok(
        "(defenum color (red) (green) (blue))
         (match (sexpr-car (list (color::blue)))
           ((color::red) 0)
           ((color::blue) 2)
           (_ 99))",
    );
    assert_eq!(v, Value::Int(2));
}

#[test]
fn the_pattern_binds_the_whole_value_preserving_struct_identity() {
    // `(the point p)` extracts the boxed struct itself (not its fields), so
    // a `setf` through the match-bound `p` must be visible through the
    // original list too — the same shared-identity property Stage 1's
    // `setf_on_a_struct_shared_via_a_list_is_visible_through_the_list`
    // exercises for plain retype, now through a match extraction.
    let (h, v) = run_with_heap(
        "(defstruct point (x int) (y int))
         (let ((l (list (point::new 1 2))))
           (match (sexpr-car l)
             ((the point p) (setf p::x 42))
             (_ ()))
           l)",
    )
    .expect("eval failed");
    let car_cell = v;
    let car = h.car(car_cell).expect("cons");
    let Value::Boxed(id) = car else { panic!("expected boxed struct") };
    assert_eq!(h.struct_field(id, 0), Value::Int(42));
}

#[test]
fn a_struct_downcast_pattern_does_not_false_match_nil() {
    // Regression for the exact scenario the design plan calls out: a
    // struct's sole variant index (`0`) collides with `Sexpr::Nil`'s own
    // variant index (`SEXPR_NIL = 0`) — a downcast `(point x y)` pattern
    // must never spuriously match `nil`.
    let v = eval_ok(
        "(defstruct point (x int) (y int))
         (match (sexpr-car (list ()))
           ((point a b) (+ a b))
           (_ -1))",
    );
    assert_eq!(v, Value::Int(-1));
}

#[test]
fn a_struct_downcast_pattern_does_not_false_match_a_different_same_shape_struct() {
    // Two distinct structs sharing a field count/type shape: without the
    // `type_name` guard (design plan §2), `(point x y)` could spuriously
    // match a boxed `pair` value purely by field count.
    let v = eval_ok(
        "(defstruct point (x int) (y int))
         (defstruct pair (a int) (b int))
         (match (sexpr-car (list (pair::new 9 9)))
           ((point x y) (+ x y))
           (_ -1))",
    );
    assert_eq!(v, Value::Int(-1));
}

#[test]
fn equalp_recursively_compares_two_distinct_struct_instances() {
    // CL's `equalp` on structures is a slot-by-slot recursive comparison
    // (design plan §3), unlike `equal`'s identity — two separately
    // allocated but same-shaped `point`s must compare equal under `equalp`.
    let v = eval_ok(
        "(defstruct point (x int) (y int))
         (equalp (point::new 1 2) (point::new 1 2))",
    );
    assert_eq!(v, Value::Bool(true));
}

/// Comparing two *different* struct types is a type error, not `false`.
///
/// It used to be `false`: `equal`/`equalp` were free builtins typed over
/// S-expression data, so both arguments widened into it independently and any
/// two values of any two types compared. They are generic now — `equal<T>(T,
/// T)` — which is the rule the language already applied everywhere a type
/// carried its own `equal` method (`(equalp 1 "a")` has always been a type
/// error).
#[test]
fn equalp_between_two_struct_types_is_a_type_error() {
    let err = check(
        "(defstruct point (x int) (y int))
         (defstruct pair (a int) (b int))
         (equalp (point::new 1 2) (pair::new 1 2))",
    )
    .expect_err("comparing two struct types should not check");
    assert!(format!("{:?}", err).contains("type mismatch"), "unexpected error: {:?}", err);
}

/// The runtime rule that used to be visible above — `equalp` tells two
/// same-shaped structs apart by type name — still holds where two struct
/// types can still legitimately meet: inside S-expression data, where both
/// sides really do have one type.
#[test]
fn equalp_distinguishes_structs_by_type_name_inside_sexpr_data() {
    let src = "(defstruct point (x int) (y int))
               (defstruct pair (a int) (b int))";
    assert_eq!(
        eval_ok(&format!("{src}\n(equalp (list (point::new 1 2)) (list (pair::new 1 2)))")),
        Value::Bool(false)
    );
    // Same type, same fields: equal.
    assert_eq!(
        eval_ok(&format!("{src}\n(equalp (list (point::new 1 2)) (list (point::new 1 2)))")),
        Value::Bool(true)
    );
}

#[test]
fn a_generic_downcast_pattern_head_is_a_type_error() {
    let src = "(defstruct box<T> (v T))
         (defun f ((s sexpr)) int
           (match s ((box v) 0) (_ 1)))";
    let err = check(src).unwrap_err();
    let msg = format!("{:?}", err);
    assert!(msg.contains("generic"), "expected a generic-downcast error, got {}", msg);
}
