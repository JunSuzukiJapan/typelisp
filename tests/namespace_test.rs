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
    assert_eq!(
        ty_program("(Some 1)"),
        Type::Named(Path::root("option"), vec![Type::I32])
    );
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
