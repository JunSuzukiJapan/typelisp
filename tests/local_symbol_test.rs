//! A local binding is found by the symbol it was written with, not by its
//! spelling: a name a macro of another module binds is that module's symbol,
//! so it neither captures nor is captured by the caller's name of the same
//! spelling.

mod common;
use common::eval_ok;
use typelisp::Value;

/// The macro's `tmp` is `a`'s symbol; the body's `tmp` is the root's, bound
/// by the caller's `let`.
#[test]
fn a_macro_of_another_module_does_not_capture_the_callers_local() {
    let src = "(module a (pub defmacro with-one (&rest body) `(let ((tmp 1)) ,@body)))
               (use a::with-one)
               (let ((tmp 10)) (with-one (+ tmp 0)))";
    assert_eq!(eval_ok(src), Value::Int(10));
}

/// In the macro's own module the two are one symbol, so the body sees the
/// macro's binding — what an anaphoric macro relies on.
#[test]
fn a_macro_of_the_same_module_binds_the_name_its_body_uses() {
    let src = "(defmacro with-one (&rest body) `(let ((tmp 1)) ,@body))
               (let ((tmp 10)) (with-one (+ tmp 0)))";
    assert_eq!(eval_ok(src), Value::Int(1));
}

/// A `block` a macro of another module makes is that module's `done`: the
/// caller's `(return-from done ...)` leaves the caller's own block.
#[test]
fn a_block_of_another_modules_macro_is_not_the_callers() {
    let src = "(module a (pub defmacro in-done (&rest body) `(progn (block done ,@body) 1)))
               (use a::in-done)
               (block done (in-done (return-from done 2)) 3)";
    assert_eq!(eval_ok(src), Value::Int(2));
}

/// In the macro's own module the names are one symbol, so the innermost
/// block of that name — the macro's — is the one left.
#[test]
fn a_block_of_the_same_modules_macro_is_the_innermost() {
    let src = "(defmacro in-done (&rest body) `(progn (block done ,@body) 1))
               (block done (in-done (return-from done 2)) 3)";
    assert_eq!(eval_ok(src), Value::Int(3));
}
