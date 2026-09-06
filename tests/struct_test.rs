//! Tests for `defstruct` — the redesigned, mutable user-defined struct type
//! (see `docs/dev/implementation-log.md`'s 2026-06-23 entry and
//! [[typelisp-vector-defstruct-revert]] for why the old design was deleted:
//! no field read/write, and `Vector<T>`'s asymmetry).
//!
//! `defstruct` registers a single-variant `AdtKind::Struct` type whose sole
//! constructor is named `new` — reached only as `Type::new` (no bare name),
//! consistent with the "don't pollute the namespace" rule this redesign
//! followed throughout (see also `Option`/`Result` moving to `Option::some`/
//! `Result::ok` in `namespace_test.rs`).

extern crate typelisp;
use typelisp::check::core;
use typelisp::{Checker, Error, EvalError, Heap, Interp, Reader, Value};

/// Check every form in `src` with one `Checker`; return the last form's tag and
/// — when that form is a `(module PATH BODY...)` — the tags of the forms it
/// groups.
///
/// Tags rather than the forms themselves: a checked form is cons cells in this
/// helper's own `Heap`, which dies with the call.
fn check(src: &str) -> Result<(String, Vec<String>), Error> {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let mut last = None;
    for v in vs {
        last = Some(chk.check_form(&mut h, &interp, v)?);
    }
    let tl = last.expect("no forms");
    let tag = core::op(&h, tl).expect("every top-level form is a tagged list").to_string();
    let inner = if tag == "module" {
        // `(module PATH BODY...)` — skip the path.
        core::fields(&h, tl)
            .expect("a module's fields are a proper list")[1..]
            .iter()
            .map(|f| core::op(&h, *f).unwrap_or("<atom>").to_string())
            .collect()
    } else {
        Vec::new()
    };
    Ok((tag, inner))
}

/// Check, then execute, every form in `src` with one (Heap, Checker, Interp);
/// return the last value produced.
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


/// Like [`run`], but also returns the `Heap` — needed to inspect a
/// `defstruct` instance's contents directly, since it's now `RtValue::Sexpr(
/// Value::Boxed(_))` (a boxed struct, see the `Sexpr`/`RtValue` unification
/// plan's Stage 2) rather than its own `RtValue` variant.
fn run_with_heap(src: &str) -> Result<(Heap, Value), EvalError> {
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
    Ok((h, last))
}

// ---- Phase 4: registration + construction -----------------------------------

#[test]
fn defstruct_registers_a_type() {
    // `check_defstruct` bundles the type registration with its synthesized
    // field-getter `defmethod`s into one `(module ...)` (a pure grouping
    // device — see that function's doc comment) since `check_form` returns
    // a single top-level form per source form.
    let src = "(defstruct point (x i32) (y i32))";
    let (tag, inner) = check(src).expect("check failed");
    assert_eq!(tag, "module");
    // defstruct + 2 fields * (getter + setter)
    assert_eq!(inner, ["defstruct", "defmethod", "defmethod", "defmethod", "defmethod"]);
}

#[test]
fn struct_new_constructs_an_instance() {
    let src = "(defstruct point (x i32) (y i32)) (point::new 1 2)";
    let (h, v) = run_with_heap(src).expect("eval failed");
    match v {
        Value::Boxed(id) => {
            assert_eq!(h.struct_type_name(id), "point");
            assert_eq!(h.struct_field_count(id), 2);
            assert_eq!(h.struct_field(id, 0), Value::Int(1));
            assert_eq!(h.struct_field(id, 1), Value::Int(2));
        }
        other => panic!("expected a boxed Struct, got {:?}", other),
    }
}

#[test]
fn struct_new_is_not_reachable_as_a_bare_name() {
    // Consistent with `Option`/`Result`'s constructors (`namespace_test.rs`)
    // — a struct's constructor lives only under its type, never bare.
    let src = "(defstruct point (x i32) (y i32)) (new 1 2)";
    assert!(check(src).is_err());
}

#[test]
fn struct_new_wrong_field_count_is_a_type_error() {
    assert!(check("(defstruct point (x i32) (y i32)) (point::new 1)").is_err());
}

#[test]
fn struct_new_wrong_field_type_is_a_type_error() {
    assert!(check("(defstruct point (x i32) (y i32)) (point::new 1 true)").is_err());
}

#[test]
fn defstruct_needs_at_least_one_field() {
    assert!(check("(defstruct empty)").is_err());
}

#[test]
fn redefining_a_struct_type_warns_by_default() {
    // Same redefinition-policy safety net as `defun`/`defmethod`/etc. (see
    // `tests/redefine_test.rs`) — exercised here through `defstruct`'s own
    // `check_redef("type", ...)` call.
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r
        .read_all(&mut h, "(defstruct point (x i32)) (defstruct point (x i32) (y i32))")
        .expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    for v in vs {
        chk.check_form(&mut h, &interp, v).expect("check failed");
    }
    assert_eq!(chk.take_warnings().len(), 1);
}

#[test]
fn two_different_struct_types_do_not_collide() {
    let src = "(defstruct point (x i32) (y i32)) \
               (defstruct rect (w i32) (h i32)) \
               (point::new 1 2)";
    let (h, v) = run_with_heap(src).expect("eval failed");
    match v {
        Value::Boxed(id) => assert_eq!(h.struct_type_name(id), "point"),
        other => panic!("expected a boxed Struct, got {:?}", other),
    }
}

#[test]
fn defstruct_rejects_duplicate_field_names() {
    assert!(check("(defstruct point (x i32) (x i32))").is_err());
}

// ---- Phase 5: field read (`p::x` and the plain `(x p)` call it sugars to) --

#[test]
fn field_accessor_call_reads_a_field() {
    let src = "(defstruct point (x i32) (y i32)) (let ((p (point::new 1 2))) (x p))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

#[test]
fn field_path_sugar_reads_a_field() {
    let src = "(defstruct point (x i32) (y i32)) (let ((p (point::new 1 2))) p::x)";
    assert_eq!(eval_ok(src), Value::Int(1));
    let src2 = "(defstruct point (x i32) (y i32)) (let ((p (point::new 1 2))) p::y)";
    assert_eq!(eval_ok(src2), Value::Int(2));
}

#[test]
fn field_path_sugar_and_plain_call_agree() {
    let src = "(defstruct point (x i32) (y i32)) \
               (let ((p (point::new 7 9))) (= p::x (x p)))";
    assert_eq!(eval_ok(src), Value::Bool(true));
}

#[test]
fn field_path_sugar_works_on_a_global() {
    let src = "(defstruct point (x i32) (y i32)) (defvar (p point) (point::new 3 4)) p::x";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn field_path_sugar_on_a_nonexistent_field_is_a_type_error() {
    let src = "(defstruct point (x i32) (y i32)) (let ((p (point::new 1 2))) p::z)";
    assert!(check(src).is_err());
}

#[test]
fn field_path_sugar_on_an_unbound_name_falls_back_to_path_resolution_error() {
    // `q` isn't bound at all, so this isn't field access on anything — it
    // should report the ordinary "unresolved path" error, not panic/misfire.
    assert!(check("(defstruct point (x i32)) q::x").is_err());
}

// ---- Phase 6: field write (`(setf p::x v)`) ---------------------------------

#[test]
fn setf_field_path_writes_in_place() {
    let src = "(defstruct point (x i32) (y i32)) \
               (let ((p (point::new 1 2))) (setf p::x 10) p::x)";
    assert_eq!(eval_ok(src), Value::Int(10));
}

#[test]
fn setf_field_path_does_not_touch_other_fields() {
    let src = "(defstruct point (x i32) (y i32)) \
               (let ((p (point::new 1 2))) (setf p::x 10) p::y)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn setf_field_path_is_visible_through_aliases() {
    // The whole point of `RtValue::Struct`'s reference semantics: passing
    // the struct to a function and mutating it there is visible to the
    // caller's own binding — unlike `RtValue::Data`'s value semantics.
    let src = "(defstruct point (x i32) (y i32)) \
               (defun bump ((p point)) () (setf p::x (+ p::x 1))) \
               (let ((p (point::new 1 2))) (bump p) p::x)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

#[test]
fn setf_field_path_returns_unit() {
    let src = "(defstruct point (x i32) (y i32)) \
               (let ((p (point::new 1 2))) (setf p::x 10))";
    assert_eq!(eval_ok(src), Value::Empty);
}

#[test]
fn setf_field_path_on_a_global_writes_in_place() {
    let src = "(defstruct point (x i32) (y i32)) \
               (defvar (p point) (point::new 1 2)) (setf p::x 99) p::x";
    assert_eq!(eval_ok(src), Value::Int(99));
}

#[test]
fn setf_field_path_wrong_value_type_is_a_type_error() {
    let src = "(defstruct point (x i32) (y i32)) (let ((p (point::new 1 2))) (setf p::x true))";
    assert!(check(src).is_err());
}

#[test]
fn setf_field_path_on_a_nonexistent_field_is_a_type_error() {
    let src = "(defstruct point (x i32) (y i32)) (let ((p (point::new 1 2))) (setf p::z 1))";
    assert!(check(src).is_err());
}

#[test]
fn set_field_method_call_works_without_the_path_sugar() {
    // `(setf p::x v)` is sugar for `(set-x p v)` — the underlying setter
    // method is an ordinary callable, like the getter.
    let src = "(defstruct point (x i32) (y i32)) \
               (let ((p (point::new 1 2))) (set-x p 10) p::x)";
    assert_eq!(eval_ok(src), Value::Int(10));
}

// ---- Phase 7: defmethod compatibility + match destructuring -----------------

#[test]
fn defmethod_on_a_struct_receiver_works_unmodified() {
    // `assoc`/`check_instance_method` are type-agnostic — a user `defmethod`
    // on a `defstruct` receiver needs no new code, just like on `Option`/
    // `HashTable`/a primitive.
    let src = "(defstruct point (x i32) (y i32)) \
               (defmethod area ((self point)) i32 (* (x self) (y self))) \
               (area (point::new 3 4))";
    assert_eq!(eval_ok(src), Value::Int(12));
}

#[test]
fn defmethod_on_a_struct_can_use_the_field_path_sugar_on_self() {
    let src = "(defstruct point (x i32) (y i32)) \
               (defmethod area ((self point)) i32 (* self::x self::y)) \
               (area (point::new 3 4))";
    assert_eq!(eval_ok(src), Value::Int(12));
}

#[test]
fn defmethod_redefinition_on_a_struct_warns_by_default() {
    let mut h = Heap::with_capacity(8192);
    let r = Reader::new();
    let vs = r
        .read_all(
            &mut h,
            "(defstruct point (x i32)) \
             (defmethod tag ((self point)) i32 1) \
             (defmethod tag ((self point)) i32 2)",
        )
        .expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    for v in vs {
        chk.check_form(&mut h, &interp, v).expect("check failed");
    }
    assert_eq!(chk.take_warnings().len(), 1);
}

#[test]
fn match_destructures_a_struct_instance() {
    let src = "(defstruct point (x i32) (y i32)) \
               (match (point::new 1 2) ((new a b) (+ a b)))";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn match_on_a_struct_binds_fields_by_position() {
    let src = "(defstruct point (x i32) (y i32)) \
               (match (point::new 5 9) ((new a b) (- a b)))";
    assert_eq!(eval_ok(src), Value::Int(-4));
}

// ---- Phase 8: generic defstruct ---------------------------------------------

#[test]
fn generic_defstruct_constructs_an_instance() {
    let src = "(defstruct pair<T,U> (first T) (second U)) (pair::new 1 true)";
    let (h, v) = run_with_heap(src).expect("eval failed");
    match v {
        Value::Boxed(id) => {
            // The *instantiation*, not just the type: a value carries which one
            // it is, which is what lets a generic type's `print-object` be
            // found and what stops `(the pair<i32,i32> x)` from accepting this
            // one (docs/dev/type-identity-instantiation-plan.md).
            assert_eq!(h.struct_type_name(id), "pair<i32,bool>");
            assert_eq!(h.struct_field(id, 0), Value::Int(1));
            assert_eq!(h.struct_field(id, 1), Value::Bool(true));
        }
        other => panic!("expected a boxed Struct, got {:?}", other),
    }
}

#[test]
fn generic_defstruct_field_accessors_work() {
    let src = "(defstruct pair<T,U> (first T) (second U)) \
               (let ((p (pair::new 1 true))) (first p))";
    assert_eq!(eval_ok(src), Value::Int(1));
    let src2 = "(defstruct pair<T,U> (first T) (second U)) \
                (let ((p (pair::new 1 true))) p::second)";
    assert_eq!(eval_ok(src2), Value::Bool(true));
}

#[test]
fn generic_defstruct_setf_works() {
    let src = "(defstruct pair<T,U> (first T) (second U)) \
               (let ((p (pair::new 1 true))) (setf p::first 99) p::first)";
    assert_eq!(eval_ok(src), Value::Int(99));
}

#[test]
fn generic_defstruct_different_instantiations_coexist() {
    let src = "(defstruct pair<T,U> (first T) (second U)) \
               (+ (first (pair::new 1 true)) (first (pair::new 2 \"x\")))";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn generic_defstruct_inconsistent_type_argument_is_a_type_error() {
    // Both fields share the same type parameter `T` — `unify` should reject
    // an `i32` and a `bool` both claiming to be `T`, the same inference
    // discipline `Option<T>`/`HashTable<K,V>` already have.
    let src = "(defstruct box<T> (a T) (b T)) (box::new 1 true)";
    assert!(check(src).is_err());
}

// ---- Sexpr-declared fields decode by their static type ----------------------
//
// A `Sexpr`-declared field's slot stores the datum's raw `mem::Value` (a
// quoted `42` is a `Value::Int`), which is shape-identical to what a plain
// `i64` field stores — only the *static* type tells them apart, and the
// checker always had it (`Expr::FieldGet`'s own node type / the pattern's
// field types). These tests pin that the static type wins over
// `decode_struct_field`'s shape heuristic on every read path: accessor,
// `setf`-then-read, and `match` destructuring.

#[test]
fn sexpr_typed_field_reads_back_as_a_sexpr_not_a_scalar() {
    // Before the static-type-driven decode, `(content h)` returned
    // `RtValue::Int(42)` — which no `Sexpr` constructor pattern matches —
    // so this fell through to the wildcard arm.
    // `match` on a `Sexpr` is fenced off (Symbol/Sexpr redesign Phase 5); the
    // stored node is read back with `sexpr-i32`, which still panics (rather
    // than silently succeeding) if `content` decoded to a bare scalar.
    let src = "(defstruct holder (content Option<Sexpr>)) \
               (let ((h (holder::new '42))) \
                 (sexpr-i32 (content h)))";
    assert_eq!(eval_ok(src), Value::Int(42));
}

#[test]
fn sexpr_typed_field_holding_a_quoted_float_reads_back_as_a_sexpr() {
    // The float case is the one `decode_struct_field`'s old doc comment
    // called out as its known ambiguity (a boxed float is also what an
    // `f64` field stores).
    let src = "(defstruct holder (content Option<Sexpr>)) \
               (let ((h (holder::new '2.5))) \
                 (sexpr-f64 (content h)))";
    // A float is a `BoxedObj::Float` since the scalar unification, so reading
    // the result needs the heap it lives in.
    let (h, v) = run_with_heap(src).expect("eval failed");
    match v {
        typelisp::Value::Boxed(id) if h.is_f64(id) => assert_eq!(h.f64_value(id), 2.5),
        other => panic!("expected an f64, got {:?}", other),
    }
}

#[test]
fn setf_then_read_of_a_sexpr_typed_field_round_trips() {
    let src = "(defstruct holder (content Option<Sexpr>)) \
               (let ((h (holder::new '1))) \
                 (setf h::content '99) \
                 (sexpr-i32 h::content))";
    assert_eq!(eval_ok(src), Value::Int(99));
}

#[test]
fn match_on_a_struct_binds_a_sexpr_typed_field_as_a_sexpr() {
    // The `match` destructuring path decodes fields itself
    // (`Pattern::Ctor::sexpr_fields`, baked at check time), independently
    // of the accessor path the tests above cover.
    let src = "(defstruct holder (content Option<Sexpr>) (k i32)) \
               (match (holder::new '7 3) \
                 ((new c n) (+ (sexpr-i32 c) n)))";
    assert_eq!(eval_ok(src), Value::Int(10));
}

// ---- `()`-typed fields --------------------------------------------------
//
// A unit-typed slot is the one field encoding whose *stored shape* doesn't
// identify it: it holds a `Value::Empty`, exactly what an
// `Option<Sexpr>`-declared slot holding the empty list would — the two have
// been the same word since the empty list became `none` under the niche. Both read paths therefore decide from
// the declared type — the accessor via `decode_field_typed`, `match` via
// `Pattern::Ctor::field_types` — and these tests pin that down by asserting
// on `RtValue::Unit` itself, which a shape-driven decode would return as
// `RtValue::Sexpr(Value::Empty)` instead.

#[test]
fn a_unit_typed_field_reads_back_as_unit() {
    let src = "(defstruct holder (u ()) (k i32)) \
               (let ((h (holder::new () 5))) h::u)";
    assert_eq!(eval_ok(src), Value::Empty);
}

#[test]
fn a_unit_typed_field_does_not_disturb_its_neighbours() {
    let src = "(defstruct holder (u ()) (k i32)) \
               (let ((h (holder::new () 5))) h::k)";
    assert_eq!(eval_ok(src), Value::Int(5));
}

#[test]
fn match_binds_a_unit_typed_field_as_unit() {
    let src = "(defstruct holder (u ()) (k i32)) \
               (match (holder::new () 3) ((new u n) u))";
    assert_eq!(eval_ok(src), Value::Empty);
}

#[test]
fn match_reads_the_fields_beside_a_unit_one_correctly() {
    let src = "(defstruct holder (u ()) (k i32)) \
               (match (holder::new () 3) ((new u n) n))";
    assert_eq!(eval_ok(src), Value::Int(3));
}

#[test]
fn a_unit_payload_keeps_an_enum_heap_representable() {
    // The motivating case. Before `()` had a field encoding,
    // `build_enum_value` couldn't convert the payload and fell back to the
    // native `RtValue::Data` — which is what kept `Result<(), E>` from
    // crossing into compiled code. A heap-repr enum is `RtValue::Sexpr`.
    let src = "(defun f () Result<(), string> (result::ok ())) (f)";
    assert!(
        matches!(eval_ok(src), Value::Boxed(_)),
        "a `()` payload must not send the enum down the native-repr fallback"
    );
}

#[test]
fn a_unit_payload_matches_and_binds() {
    let src = "(defun f () Result<(), string> (result::ok ())) \
               (match (f) ((ok u) u) ((err _) ()))";
    assert_eq!(eval_ok(src), Value::Empty);
}
