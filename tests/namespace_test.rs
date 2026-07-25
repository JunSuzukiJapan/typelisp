//! Tests for namespaces (`module`/`use`), `::` paths, and methods
//! (`defmethod`, instance + static dispatch).

extern crate typelisp;
use typelisp::{Checker, Error, Heap, Interp, Path, Reader, TopLevel, Type};

/// Check a sequence of forms with one checker; return the last form's result.
fn program(src: &str) -> Result<TopLevel, Error> {
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

fn ty_program(src: &str) -> Type {
    match program(src).expect("check failed") {
        TopLevel::Expr(t) => t.ty,
        other => panic!("expected expression, got {:?}", other),
    }
}

// ---- modules ----------------------------------------------------------------

#[test]
fn module_qualified_call() {
    let src = "(module math (pub defun id ((x i32)) i32 x)) (math::id 7)";
    assert_eq!(ty_program(src), Type::I32);
}

#[test]
fn bare_name_resolves_current_then_root() {
    // Inside the module, `id` resolves to the module-local function.
    let src = "(module m \
                 (defun id ((x i32)) i32 x) \
                 (defun use-it ((y i32)) i32 (id y)))";
    assert!(matches!(program(src), Ok(TopLevel::Module { .. })));
}

#[test]
fn unknown_qualified_path_errors() {
    assert!(program("(nosuch::fn 1)").is_err());
}

// ---- instance methods -------------------------------------------------------
//
// `defmethod` on a user-defined type's own dedicated instance-method-
// dispatch test used `defstruct` as its receiver type — `defstruct` no
// longer exists (see docs/TODO.md), so that scenario is no longer
// expressible; `defmethod` on built-in/primitive receivers (below) still
// exercises the same dispatch machinery.

// ---- defmethod on primitive receivers ---------------------------------------
// `i32`/`i64`/`f64`/`char`/`bool`/`Str`/etc. are primitive `Type` variants, not
// `Type::Named`, but are still valid `defmethod` receivers (see
// `crate::prim_type_path`).

#[test]
fn instance_method_on_i32() {
    let src = "(defmethod double ((self i32)) i32 (+ self self)) (double 3)";
    assert_eq!(ty_program(src), Type::I32);
}

#[test]
fn instance_method_on_bool() {
    let src = "(defmethod id ((self bool)) bool self) (id true)";
    assert_eq!(ty_program(src), Type::Bool);
}

#[test]
fn instance_method_on_char() {
    let src = r"(defmethod id ((self char)) char self) (id #\a)";
    assert_eq!(ty_program(src), Type::Char);
}

#[test]
fn instance_method_on_str() {
    let src = r#"(defmethod id ((self string)) string self) (id "hi")"#;
    assert_eq!(ty_program(src), Type::Str);
}

#[test]
fn static_method_on_primitive_via_path() {
    let src = "(defmethod zero (i32) i32 0) (i32::zero)";
    assert_eq!(ty_program(src), Type::I32);
}

#[test]
fn instance_method_on_primitive_wrong_type_errors() {
    let src = "(defmethod double ((self i32)) i32 (+ self self)) (double true)";
    assert!(program(src).is_err());
}

// ---- static / associated methods -------------------------------------------

// `static_method_on_primitive_via_path` above already exercises a static
// method invoked via `Type::method` path syntax without needing `defstruct`.

// ---- use --------------------------------------------------------------------

#[test]
fn use_injects_name_into_current_scope() {
    let src = "(module geo (pub defun area ((r i32)) i32 r)) \
               (use geo::area) \
               (area 5)";
    assert_eq!(ty_program(src), Type::I32);
}

// `cross_module_static_method_and_construct`/`cross_module_qualified_constructor_and_match`
// used to define a type + static method inside a module via `defstruct` and
// resolve both via fully-qualified paths — `defstruct` no longer exists (see
// docs/TODO.md); cross-module *function* resolution is still covered below
// (`use_module_alias`/`use_module_then_item`/`absolute_path_*`).

// ---- regression: builtins still resolve at root ----------------------------

#[test]
fn builtins_still_resolve() {
    // `Option`'s constructors are type-scoped (`Option::some`, not a bare
    // `some`) — see `Registry::add_type`/`Checker::check_use` — but the
    // type itself, and `Type::ctor` construction, still resolve with no
    // setup needed.
    assert_eq!(
        ty_program("(option::some 1)"),
        Type::Named(Path::root("option"), vec![Type::I32])
    );
}

#[test]
fn bare_option_constructors_no_longer_resolve() {
    // The flip side of `builtins_still_resolve`: a bare `some`/`none` is
    // *not* a free function/global, so it's the "unbound variable" error
    // path, not the construct-then-typecheck path `Some` used to take.
    assert!(program("(some 1)").is_err());
    assert!(program("(none)").is_err());
}

#[test]
fn bare_result_and_error_constructors_no_longer_resolve() {
    assert!(program("(ok 1)").is_err());
    assert!(program(r#"(err (ParseIntError::ParseIntError "x"))"#).is_err());
    assert!(program(r#"(ParseIntError "x")"#).is_err());
}

// ---- use Type: snapshotting a type's constructors/static methods bare -----

#[test]
fn use_option_makes_constructors_callable_bare() {
    assert_eq!(
        ty_program("(use option) (some 1)"),
        Type::Named(Path::root("option"), vec![Type::I32])
    );
    // `none` has no fields, so — same limitation as `option::none` alone,
    // see `prelude_test.rs`'s comment on this — it needs a declared return
    // type to seed `T` from; bare `(none)` with no context can't infer it.
    let src = "(use option) (defun f () Option<i32> (none)) (f)";
    assert_eq!(ty_program(src), Type::Named(Path::root("option"), vec![Type::I32]));
}

#[test]
fn use_hashtable_makes_its_static_new_callable_bare() {
    // `new` is a static (`instance: false`) associated function, not a
    // constructor — exercises `Namespace::static_uses`/`resolve_static_use`,
    // not `ctors`. A declared return type seeds `K`/`V` the same way any
    // `HashTable::new`/`Vector::new` call needs one (`let`'s binding value
    // is checked with `expected: None`, so `new` can't be inferred there).
    let src = "(use hashtable) (defun f () HashTable<i32,i32> (new)) (f)";
    assert!(matches!(program(src), Ok(TopLevel::Expr(_))));
}

#[test]
fn use_is_scoped_to_its_own_namespace() {
    // `use`d inside `m`, `some` stays bare-unresolved at the root.
    let src = "(module m (use option) (defun f () Option<i32> (some 1))) (some 1)";
    assert!(program(src).is_err());
}

#[test]
fn use_inside_a_module_does_not_leak_to_a_sibling_module() {
    let src = "(module a (use option)) (module b (defun f () Option<i32> (some 1)))";
    assert!(program(src).is_err());
}

// ---- visibility: private items are inaccessible from outside ----------------

#[test]
fn private_fn_inaccessible_cross_module() {
    // defun without `pub` is private — cannot call from outside
    let src = "(module secret (defun hidden ((x i32)) i32 x)) (secret::hidden 1)";
    assert!(program(src).is_err());
}


#[test]
fn pub_fn_accessible_cross_module() {
    let src = "(module m (pub defun add1 ((x i32)) i32 (+ x 1))) (m::add1 5)";
    assert_eq!(ty_program(src), Type::I32);
}

#[test]
fn private_fn_accessible_within_same_module() {
    // private function is still callable from within the same module
    let src = "(module m \
                 (defun helper ((x i32)) i32 (+ x 1)) \
                 (pub defun outer ((x i32)) i32 (helper x)))";
    assert!(program(src).is_ok());
}

// ---- visibility: defstruct field accessors are private by default,
// independent of the struct's own `pub` -----------------------------------

#[test]
fn struct_field_without_pub_is_inaccessible_cross_module_even_on_a_pub_struct() {
    // `geo::point` itself is `pub` (constructible from outside), but its `x`
    // field wasn't declared `pub` — the accessor stays module-private. `(x p)`
    // is the bare instance-method call form (dispatch is by `p`'s type, not
    // by namespace lookup, so it works the same from any module).
    let src = "(module geo (pub defstruct point (x i32) (y i32))) \
               (let ((p (geo::point::new 1 2))) (x p))";
    assert!(program(src).is_err());
}

#[test]
fn struct_field_path_sugar_without_pub_is_inaccessible_cross_module() {
    let src = "(module geo (pub defstruct point (x i32) (y i32))) \
               (let ((p (geo::point::new 1 2))) p::x)";
    assert!(program(src).is_err());
}

#[test]
fn struct_field_marked_pub_is_accessible_cross_module() {
    let src = "(module geo (pub defstruct point (pub x i32) (y i32))) \
               (let ((p (geo::point::new 1 2))) p::x)";
    assert_eq!(ty_program(src), Type::I32);
}

#[test]
fn struct_field_marked_pub_setter_is_accessible_cross_module() {
    let src = "(module geo (pub defstruct point (pub x i32) (y i32))) \
               (let ((p (geo::point::new 1 2))) (setf p::x 9))";
    assert_eq!(ty_program(src), Type::Unit);
}

#[test]
fn struct_field_not_marked_pub_setter_is_inaccessible_cross_module() {
    let src = "(module geo (pub defstruct point (x i32) (pub y i32))) \
               (let ((p (geo::point::new 1 2))) (setf p::x 9))";
    assert!(program(src).is_err());
}

#[test]
fn struct_field_without_pub_is_accessible_within_its_own_module() {
    let src = "(module geo \
                 (pub defstruct point (x i32) (y i32)) \
                 (pub defun get-x ((p point)) i32 p::x))";
    assert!(program(src).is_ok());
}

#[test]
fn struct_pub_field_on_a_non_pub_struct_is_still_accessible_cross_module() {
    // The type itself is private (unnameable/unconstructible outside `geo`),
    // but a value the module hands out through its own public API can still
    // expose individual `pub` fields to outside code.
    let src = "(module geo \
                 (defstruct point (pub x i32) (y i32)) \
                 (pub defun origin () point (point::new 0 0))) \
               (x (geo::origin))";
    assert_eq!(ty_program(src), Type::I32);
}

// ---- use: module alias (`use std::math` makes `math` resolve to `std::math`) ---

#[test]
fn use_module_alias() {
    // `use geo` makes `geo::area` reachable after `use math` (re-aliased)
    let src = "(module std \
                 (module math \
                   (pub defun square ((x i32)) i32 (* x x)))) \
               (use std::math) \
               (math::square 4)";
    assert_eq!(ty_program(src), Type::I32);
}

#[test]
fn use_module_then_item() {
    // After `(use std::math)`, both `math::square` and `(use math::square)` work.
    let src = "(module std \
                 (module math \
                   (pub defun double ((x i32)) i32 (* x 2)))) \
               (use std::math) \
               (use math::double) \
               (double 3)";
    assert_eq!(ty_program(src), Type::I32);
}

// ---- absolute paths (`::foo`) -----------------------------------------------

#[test]
fn absolute_path_resolves_from_root() {
    // Inside a module, `::add1` unambiguously refers to the root-level `add1`.
    let src = "(pub defun add1 ((x i32)) i32 (+ x 1)) \
               (module m \
                 (pub defun call-root ((x i32)) i32 (::add1 x))) \
               (m::call-root 7)";
    assert_eq!(ty_program(src), Type::I32);
}

#[test]
fn absolute_path_shadows_local() {
    // Even if the module has its own `id`, `::id` picks the root one.
    let src = "(pub defun id ((x i32)) i32 x) \
               (module m \
                 (defun id ((x i32)) i32 (+ x 99)) \
                 (pub defun use-root ((x i32)) i32 (::id x))) \
               (m::use-root 5)";
    assert_eq!(ty_program(src), Type::I32);
}
