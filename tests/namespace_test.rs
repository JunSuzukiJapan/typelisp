//! Tests for namespaces (`module`/`use`), `::` paths, and methods
//! (`defmethod`, instance + static dispatch).

extern crate typelisp;
use typelisp::check::core;
use typelisp::{Checker, Error, Heap, Interp, Path, Reader, Type};

/// Check a sequence of forms with one checker; return the last form's tag
/// (`"expr"`, `"module"`, ...) and, for an `expr` form, the type the checker
/// proved for it.
///
/// A tag rather than the form itself: a checked form is cons cells in this
/// helper's own `Heap`, which dies with the call. The type is a separate
/// output for the same reason it is on the checker — see `Checker::expr_type`.
fn program(src: &str) -> Result<(String, Option<Type>), Error> {
    let mut h = Heap::with_capacity(8192);
    let mut chk = Checker::new();
    chk.publish_names(&h);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let interp = Interp::new();
    let mut tag = String::new();
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v)?;
        tag = core::op_spelling(&h, tl).expect("every top-level form is a tagged list").to_string();
    }
    Ok((tag, chk.expr_type().cloned()))
}

fn ty_program(src: &str) -> Type {
    let (tag, ty) = program(src).expect("check failed");
    ty.unwrap_or_else(|| panic!("expected expression, got a `{}` form", tag))
}

// ---- modules ----------------------------------------------------------------

#[test]
fn module_qualified_call() {
    let src = "(module math (pub defun id ((x int)) int x)) (math::id 7)";
    assert_eq!(ty_program(src), Type::Int);
}

#[test]
fn bare_name_resolves_current_then_root() {
    // Inside the module, `id` resolves to the module-local function.
    let src = "(module m \
                 (defun id ((x int)) int x) \
                 (defun use-it ((y int)) int (id y)))";
    assert_eq!(program(src).expect("check failed").0, "module");
}

#[test]
fn unknown_qualified_path_errors() {
    assert!(program("(nosuch::fn 1)").is_err());
}

// ---- methods ----------------------------------------------------------------
//
// Methods on a program's own types are covered in `tests/struct_test.rs` and
// `tests/inherent_impl_test.rs`; a program cannot add methods to a standard
// library type such as `int` (`tests/inherent_impl_test.rs`).

// ---- use --------------------------------------------------------------------

#[test]
fn use_injects_name_into_current_scope() {
    let src = "(module geo (pub defun area ((r int)) int r)) \
               (use geo::area) \
               (area 5)";
    assert_eq!(ty_program(src), Type::Int);
}

// ---- pub use: re-exports ----------------------------------------------------

#[test]
fn a_re_exported_function_is_reached_through_the_re_exporting_module() {
    let src = "(module geo (pub defun area ((r int)) int r)) \
               (module shapes (pub use geo::area)) \
               (shapes::area 5)";
    assert_eq!(ty_program(src), Type::Int);
}

#[test]
fn a_re_exported_function_can_be_brought_in_from_the_re_exporting_module() {
    let src = "(module geo (pub defun area ((r int)) int r)) \
               (module shapes (pub use geo::area)) \
               (use shapes::area) \
               (area 5)";
    assert_eq!(ty_program(src), Type::Int);
}

#[test]
fn a_re_export_of_a_re_export_reaches_the_definition() {
    let src = "(module geo (pub defun area ((r int)) int r)) \
               (module shapes (pub use geo::area)) \
               (module all (pub use shapes::area)) \
               (all::area 5)";
    assert_eq!(ty_program(src), Type::Int);
}

#[test]
fn a_re_exported_type_is_reached_through_the_re_exporting_module() {
    let src = "(module geo (pub defstruct point (pub x int))) \
               (module shapes (pub use geo::point)) \
               (defun x-of ((p shapes::point)) int p::x) \
               (x-of (make-instance 'geo::point :x 3))";
    assert_eq!(ty_program(src), Type::Int);
}

#[test]
fn a_use_without_pub_is_not_reached_through_the_module() {
    let src = "(module geo (pub defun area ((r int)) int r)) \
               (module shapes (use geo::area)) \
               (shapes::area 5)";
    assert!(program(src).is_err());
}

#[test]
fn a_module_is_not_re_exported() {
    let src = "(module geo (pub defun area ((r int)) int r)) \
               (module shapes (pub use geo))";
    let err = program(src).expect_err("a module is not re-exported");
    assert!(err.to_string().contains("only a definition is re-exported"), "{}", err);
}

// `cross_module_static_method_and_construct`/`cross_module_qualified_constructor_and_match`
// used to define a type + static method inside a module via `defstruct` and
// resolve both via fully-qualified paths — dropped when `defstruct` was
// deleted on 2026-06-23, and not restored here after it came back the next
// day; cross-module *function* resolution is still covered below
// (`use_module_alias`/`use_module_then_item`/`absolute_path_*`).

// ---- regression: builtins still resolve at root ----------------------------

#[test]
fn builtins_still_resolve() {
    // `Option`'s constructors are type-scoped (`Option::some`, not a bare
    // `some`) — see `Registry::add_type`/`Checker::check_use` — but the
    // type itself, and `Type::ctor` construction, still resolve with no
    // setup needed.
    assert_eq!(
        ty_program("(Option::some 1)"),
        Type::Named(Path::std("Option"), vec![Type::Int])
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
        ty_program("(use Option) (some 1)"),
        Type::Named(Path::std("Option"), vec![Type::Int])
    );
    // `none` has no fields, so — same limitation as `Option::none` alone,
    // see `prelude_test.rs`'s comment on this — it needs a declared return
    // type to seed `T` from; bare `(none)` with no context can't infer it.
    let src = "(use Option) (defun f () Option<int> (none)) (f)";
    assert_eq!(ty_program(src), Type::Named(Path::std("Option"), vec![Type::Int]));
}

#[test]
fn use_hashtable_makes_its_static_new_callable_bare() {
    // `new` is a static (`instance: false`) associated function, not a
    // constructor — exercises `Namespace::static_uses`/`resolve_static_use`,
    // not `ctors`. A declared return type seeds `K`/`V` the same way any
    // `HashTable::new`/`Vector::new` call needs one (`let`'s binding value
    // is checked with `expected: None`, so `new` can't be inferred there).
    let src = "(use HashTable) (defun f () HashTable<int,int> (new)) (f)";
    assert_eq!(program(src).expect("check failed").0, "expr");
}

/// A type brought in by `use` is one name wherever a type name is written:
/// the annotation, the `Type::` of a constructor or static function, and a
/// pattern head all resolve `p` to `m::p`.
#[test]
fn a_used_type_is_the_same_name_in_type_member_position() {
    let def = "(module m (pub defstruct p (pub x i32)) (impl p (pub origin () p (make-instance 'p :x 0)))) (use m::p)";
    let m_p = Type::Named(Path::of(&["m", "p"]), vec![]);
    assert_eq!(ty_program(&format!("{} (make-instance 'p :x 1)", def)), m_p);
    assert_eq!(ty_program(&format!("{} (p::origin)", def)), m_p);
    assert_eq!(ty_program(&format!("{} (defun f ((v p)) p v) (f (make-instance 'p :x 2))", def)), m_p);
    let pattern = format!("{} (match (the Sexpr (make-instance 'p :x 3)) ((p :x) x) (_ 0))", def);
    assert_eq!(ty_program(&pattern), Type::I32);
}

#[test]
fn use_is_scoped_to_its_own_namespace() {
    // `use`d inside `m`, `some` stays bare-unresolved at the root.
    let src = "(module m (use Option) (defun f () Option<int> (some 1))) (some 1)";
    assert!(program(src).is_err());
}

#[test]
fn use_inside_a_module_does_not_leak_to_a_sibling_module() {
    let src = "(module a (use Option)) (module b (defun f () Option<int> (some 1)))";
    assert!(program(src).is_err());
}

// ---- visibility: private items are inaccessible from outside ----------------

#[test]
fn private_fn_inaccessible_cross_module() {
    // defun without `pub` is private — cannot call from outside
    let src = "(module secret (defun hidden ((x int)) int x)) (secret::hidden 1)";
    assert!(program(src).is_err());
}


#[test]
fn pub_fn_accessible_cross_module() {
    let src = "(module m (pub defun add1 ((x int)) int (+ x 1))) (m::add1 5)";
    assert_eq!(ty_program(src), Type::Int);
}

#[test]
fn private_fn_accessible_within_same_module() {
    // private function is still callable from within the same module
    let src = "(module m \
                 (defun helper ((x int)) int (+ x 1)) \
                 (pub defun outer ((x int)) int (helper x)))";
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
    let src = "(module geo (pub defstruct point (x int) (y int))) \
               (let ((p (make-instance 'geo::point :x 1 :y 2))) (x p))";
    assert!(program(src).is_err());
}

#[test]
fn struct_field_path_sugar_without_pub_is_inaccessible_cross_module() {
    let src = "(module geo (pub defstruct point (x int) (y int))) \
               (let ((p (make-instance 'geo::point :x 1 :y 2))) p::x)";
    assert!(program(src).is_err());
}

#[test]
fn struct_field_marked_pub_is_accessible_cross_module() {
    let src = "(module geo (pub defstruct point (pub x int) (y int)) (impl point (pub new ((x int) (y int)) point (make-instance 'point :x x :y y)))) \
               (let ((p (geo::point::new 1 2))) p::x)";
    assert_eq!(ty_program(src), Type::Int);
}

#[test]
fn struct_field_marked_pub_setter_is_accessible_cross_module() {
    let src = "(module geo (pub defstruct point (pub x int) (y int)) (impl point (pub new ((x int) (y int)) point (make-instance 'point :x x :y y)))) \
               (let ((p (geo::point::new 1 2))) (setf p::x 9))";
    assert_eq!(ty_program(src), Type::Unit);
}

#[test]
fn struct_field_not_marked_pub_setter_is_inaccessible_cross_module() {
    let src = "(module geo (pub defstruct point (x int) (pub y int)) (impl point (pub new ((x int) (y int)) point (make-instance 'point :x x :y y)))) \
               (let ((p (geo::point::new 1 2))) (setf p::x 9))";
    assert!(program(src).is_err());
}

#[test]
fn struct_field_without_pub_is_accessible_within_its_own_module() {
    let src = "(module geo \
                 (pub defstruct point (x int) (y int)) \
                 (pub defun get-x ((p point)) int p::x))";
    assert!(program(src).is_ok());
}

#[test]
fn struct_pub_field_on_a_non_pub_struct_is_still_accessible_cross_module() {
    // The type itself is private (unnameable/unconstructible outside `geo`),
    // but a value the module hands out through its own public API can still
    // expose individual `pub` fields to outside code.
    let src = "(module geo \
                 (defstruct point (pub x int) (y int)) \
                 (pub defun origin () point (make-instance 'point :x 0 :y 0))) \
               (x (geo::origin))";
    assert_eq!(ty_program(src), Type::Int);
}

// ---- use: module alias (`use std::math` makes `math` resolve to `std::math`) ---

#[test]
fn use_module_alias() {
    // `use geo` makes `geo::area` reachable after `use math` (re-aliased)
    let src = "(module std \
                 (module math \
                   (pub defun square ((x int)) int (* x x)))) \
               (use std::math) \
               (math::square 4)";
    assert_eq!(ty_program(src), Type::Int);
}

#[test]
fn use_module_then_item() {
    // After `(use std::math)`, both `math::square` and `(use math::square)` work.
    let src = "(module std \
                 (module math \
                   (pub defun double ((x int)) int (* x 2)))) \
               (use std::math) \
               (use math::double) \
               (double 3)";
    assert_eq!(ty_program(src), Type::Int);
}

// ---- absolute paths (`::foo`) -----------------------------------------------

#[test]
fn absolute_path_resolves_from_root() {
    // Inside a module, `::add1` unambiguously refers to the root-level `add1`.
    let src = "(pub defun add1 ((x int)) int (+ x 1)) \
               (module m \
                 (pub defun call-root ((x int)) int (::add1 x))) \
               (m::call-root 7)";
    assert_eq!(ty_program(src), Type::Int);
}

#[test]
fn absolute_path_shadows_local() {
    // Even if the module has its own `id`, `::id` picks the root one.
    let src = "(pub defun id ((x int)) int x) \
               (module m \
                 (defun id ((x int)) int (+ x 99)) \
                 (pub defun use-root ((x int)) int (::id x))) \
               (m::use-root 5)";
    assert_eq!(ty_program(src), Type::Int);
}
