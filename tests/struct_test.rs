//! Tests for `defstruct` — the redesigned, mutable user-defined struct type
//! (see `docs/TODO.md`/[[typelisp-vector-defstruct-revert]] for why the old
//! design was deleted: no field read/write, and `Vector<T>`'s asymmetry).
//!
//! `defstruct` registers a single-variant `AdtKind::Struct` type whose sole
//! constructor is named `new` — reached only as `Type::new` (no bare name),
//! consistent with the "don't pollute the namespace" rule this redesign
//! followed throughout (see also `Option`/`Result` moving to `Option::some`/
//! `Result::ok` in `namespace_test.rs`).

extern crate typelisp;
use typelisp::{Checker, Error, EvalError, Heap, Interp, Reader, RtValue, TopLevel, Value};

/// Check every form in `src` with one `Checker`; return the last result.
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

/// Check, then execute, every form in `src` with one (Heap, Checker, Interp);
/// return the last value produced.
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

/// Like [`run`], but also returns the `Heap` — needed to inspect a
/// `defstruct` instance's contents directly, since it's now `RtValue::Sexpr(
/// Value::Boxed(_))` (a boxed struct, see the `Sexpr`/`RtValue` unification
/// plan's Stage 2) rather than its own `RtValue` variant.
fn run_with_heap(src: &str) -> Result<(Heap, RtValue), EvalError> {
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
    Ok((h, last))
}

// ---- Phase 4: registration + construction -----------------------------------

#[test]
fn defstruct_registers_a_type() {
    // `check_defstruct` bundles the type registration with its synthesized
    // field-getter `defmethod`s into one `TopLevel::Module` (a pure grouping
    // device — see that function's doc comment) since `check_form` returns
    // a single `TopLevel` per form.
    let src = "(defstruct point (x i32) (y i32))";
    match check(src) {
        Ok(TopLevel::Module { body, .. }) => {
            assert!(matches!(body[0], TopLevel::Defstruct { .. }));
            assert_eq!(body.len(), 5); // Defstruct + 2 fields * (getter + setter)
        }
        other => panic!("expected a Module, got {:?}", other),
    }
}

#[test]
fn struct_new_constructs_an_instance() {
    let src = "(defstruct point (x i32) (y i32)) (point::new 1 2)";
    let (h, v) = run_with_heap(src).expect("eval failed");
    match v {
        RtValue::Sexpr(Value::Boxed(id)) => {
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
        RtValue::Sexpr(Value::Boxed(id)) => assert_eq!(h.struct_type_name(id), "point"),
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
    assert_eq!(eval_ok(src), RtValue::Int(1));
}

#[test]
fn field_path_sugar_reads_a_field() {
    let src = "(defstruct point (x i32) (y i32)) (let ((p (point::new 1 2))) p::x)";
    assert_eq!(eval_ok(src), RtValue::Int(1));
    let src2 = "(defstruct point (x i32) (y i32)) (let ((p (point::new 1 2))) p::y)";
    assert_eq!(eval_ok(src2), RtValue::Int(2));
}

#[test]
fn field_path_sugar_and_plain_call_agree() {
    let src = "(defstruct point (x i32) (y i32)) \
               (let ((p (point::new 7 9))) (= p::x (x p)))";
    assert_eq!(eval_ok(src), RtValue::Bool(true));
}

#[test]
fn field_path_sugar_works_on_a_global() {
    let src = "(defstruct point (x i32) (y i32)) (defvar p (point::new 3 4)) p::x";
    assert_eq!(eval_ok(src), RtValue::Int(3));
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
    assert_eq!(eval_ok(src), RtValue::Int(10));
}

#[test]
fn setf_field_path_does_not_touch_other_fields() {
    let src = "(defstruct point (x i32) (y i32)) \
               (let ((p (point::new 1 2))) (setf p::x 10) p::y)";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn setf_field_path_is_visible_through_aliases() {
    // The whole point of `RtValue::Struct`'s reference semantics: passing
    // the struct to a function and mutating it there is visible to the
    // caller's own binding — unlike `RtValue::Data`'s value semantics.
    let src = "(defstruct point (x i32) (y i32)) \
               (defun bump ((p point)) () (setf p::x (+ p::x 1))) \
               (let ((p (point::new 1 2))) (bump p) p::x)";
    assert_eq!(eval_ok(src), RtValue::Int(2));
}

#[test]
fn setf_field_path_returns_unit() {
    let src = "(defstruct point (x i32) (y i32)) \
               (let ((p (point::new 1 2))) (setf p::x 10))";
    assert_eq!(eval_ok(src), RtValue::Unit);
}

#[test]
fn setf_field_path_on_a_global_writes_in_place() {
    let src = "(defstruct point (x i32) (y i32)) \
               (defvar p (point::new 1 2)) (setf p::x 99) p::x";
    assert_eq!(eval_ok(src), RtValue::Int(99));
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
    assert_eq!(eval_ok(src), RtValue::Int(10));
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
    assert_eq!(eval_ok(src), RtValue::Int(12));
}

#[test]
fn defmethod_on_a_struct_can_use_the_field_path_sugar_on_self() {
    let src = "(defstruct point (x i32) (y i32)) \
               (defmethod area ((self point)) i32 (* self::x self::y)) \
               (area (point::new 3 4))";
    assert_eq!(eval_ok(src), RtValue::Int(12));
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
    assert_eq!(eval_ok(src), RtValue::Int(3));
}

#[test]
fn match_on_a_struct_binds_fields_by_position() {
    let src = "(defstruct point (x i32) (y i32)) \
               (match (point::new 5 9) ((new a b) (- a b)))";
    assert_eq!(eval_ok(src), RtValue::Int(-4));
}

// ---- Phase 8: generic defstruct ---------------------------------------------

#[test]
fn generic_defstruct_constructs_an_instance() {
    let src = "(defstruct (pair T U) (first T) (second U)) (pair::new 1 true)";
    let (h, v) = run_with_heap(src).expect("eval failed");
    match v {
        RtValue::Sexpr(Value::Boxed(id)) => {
            assert_eq!(h.struct_type_name(id), "pair");
            assert_eq!(h.struct_field(id, 0), Value::Int(1));
            assert_eq!(h.struct_field(id, 1), Value::Bool(true));
        }
        other => panic!("expected a boxed Struct, got {:?}", other),
    }
}

#[test]
fn generic_defstruct_field_accessors_work() {
    let src = "(defstruct (pair T U) (first T) (second U)) \
               (let ((p (pair::new 1 true))) (first p))";
    assert_eq!(eval_ok(src), RtValue::Int(1));
    let src2 = "(defstruct (pair T U) (first T) (second U)) \
                (let ((p (pair::new 1 true))) p::second)";
    assert_eq!(eval_ok(src2), RtValue::Bool(true));
}

#[test]
fn generic_defstruct_setf_works() {
    let src = "(defstruct (pair T U) (first T) (second U)) \
               (let ((p (pair::new 1 true))) (setf p::first 99) p::first)";
    assert_eq!(eval_ok(src), RtValue::Int(99));
}

#[test]
fn generic_defstruct_different_instantiations_coexist() {
    let src = "(defstruct (pair T U) (first T) (second U)) \
               (+ (first (pair::new 1 true)) (first (pair::new 2 \"x\")))";
    assert_eq!(eval_ok(src), RtValue::Int(3));
}

#[test]
fn generic_defstruct_inconsistent_type_argument_is_a_type_error() {
    // Both fields share the same type parameter `T` — `unify` should reject
    // an `i32` and a `bool` both claiming to be `T`, the same inference
    // discipline `Option<T>`/`HashTable<K,V>` already have.
    let src = "(defstruct (box T) (a T) (b T)) (box::new 1 true)";
    assert!(check(src).is_err());
}
