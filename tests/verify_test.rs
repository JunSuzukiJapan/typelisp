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
