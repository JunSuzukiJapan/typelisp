//! A disagreement the analysis records in the tree instead of stopping at is
//! still reported, including in a tree the checker throws away after checking
//! it: a trait's default method body and the arguments of a call through a
//! `where` bound are checked only for their diagnostics.

extern crate typelisp;
use typelisp::{Checker, Error, Heap, Interp, Reader};

fn program(src: &str) -> Result<(), Error> {
    let mut h = Heap::with_capacity(8192);
    let mut chk = Checker::new();
    chk.publish_names(&h);
    let r = Reader::new();
    let interp = Interp::new();
    let mut forms = r.forms(src);
    while let Some((v, _)) = forms.next_form(&mut h).expect("read failed") {
        chk.check_form(&mut h, &interp, v)?;
    }
    Ok(())
}

fn mismatch(src: &str) -> String {
    match program(src) {
        Err(e) => e.to_string(),
        Ok(()) => panic!("expected a type error in: {}", src),
    }
}

#[test]
fn a_mismatch_in_a_function_body_is_reported() {
    let e = mismatch(r#"(defun f () i32 "no")"#);
    assert!(e.contains("type mismatch: expected `i32`, found `string`"), "{}", e);
}

#[test]
fn a_mismatch_in_a_generic_function_body_is_reported() {
    let e = mismatch(r#"(defun f<T> ((x T)) i32 "no")"#);
    assert!(e.contains("type mismatch: expected `i32`, found `string`"), "{}", e);
}

#[test]
fn a_mismatch_in_a_trait_default_body_is_reported() {
    let e = mismatch(r#"(deftrait greet () (hello ((self Self)) i32 "x"))"#);
    assert!(e.contains("type mismatch: expected `i32`, found `string`"), "{}", e);
}

#[test]
fn the_inner_mismatch_is_reported_first() {
    let e = mismatch(r#"(defun g ((x i32)) i32 x) (defun f () i32 (g "no"))"#);
    assert!(e.contains("type mismatch: expected `i32`, found `string`"), "{}", e);
}

#[test]
fn a_mismatch_between_branches_is_reported() {
    let e = mismatch(r#"(if true 1 "no")"#);
    assert!(e.contains("type mismatch: expected `int`, found `string`"), "{}", e);
}

#[test]
fn incompatible_block_exits_are_reported() {
    let e = mismatch(r#"(block b (if true (return-from b 1) 2) "no")"#);
    assert!(e.contains("branches have incompatible types"), "{}", e);
}

#[test]
fn an_argument_that_does_not_fit_a_generic_parameter_is_reported() {
    let e = mismatch(r#"(defun first<T> ((v Vector<T>)) i32 1) (first 5)"#);
    assert!(e.contains("type mismatch: expected `Vector<t>`, found `int`"), "{}", e);
}

#[test]
fn an_undecided_type_parameter_is_reported() {
    let e = mismatch(r#"(defun make<T> () Vector<T> (vector::new)) (make)"#);
    assert!(e.contains("cannot infer type parameter `t` for `make`"), "{}", e);
}

#[test]
fn an_unmet_where_bound_is_reported() {
    let e = mismatch(
        r#"(deftrait shout () (loud ((self Self)) i32))
           (defun f<T> ((x T)) i32 (:where (shout T)) 1)
           (f 5)"#,
    );
    assert!(e.contains("f: type `int` does not implement trait `shout` required by `:where` clause on type parameter `t`"), "{}", e);
}

#[test]
fn a_non_exhaustive_match_is_reported() {
    let e = mismatch(r#"(match true (true 1))"#);
    assert!(e.contains("non-exhaustive match on Bool: `false` is not covered"), "{}", e);
}

#[test]
fn a_definition_unlike_its_signature_is_reported() {
    let e = mismatch(r#"(defsignature f (i32) i32) (defun f ((x i32)) string "no")"#);
    assert!(e.contains("`f` does not match its `defsignature`: return type declared as `i32`, defined as `string`"), "{}", e);
}

#[test]
fn a_top_level_form_in_expression_position_is_reported() {
    let e = mismatch(r#"(defun f () i32 (defun g () i32 1) 2)"#);
    assert!(e.contains("defun: only allowed at top level, not in expression position — define a local function with `labels`"), "{}", e);
}

#[test]
fn an_assignment_to_a_constant_is_reported() {
    let e = mismatch(r#"(defconstant (k i32) 5) (setf k 6)"#);
    assert!(e.contains("setf: cannot assign to constant `k`"), "{}", e);
}

#[test]
fn an_assignment_to_a_struct_held_by_value_is_reported() {
    let e = mismatch(
        r#"(unsafe (def-c-struct point (x i32) (y i32))
                   (def-c-struct seg (a point) (b point)))
           (unsafe (let ((s (c-alloc seg 1))) (setf s::a (panic "x")) 0))"#,
    );
    assert!(e.contains("setf: a field holding a struct by value (`point`) cannot be assigned as a whole"), "{}", e);
}
