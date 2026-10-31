//! A method's receiver is written `self` alone, as in Rust, wherever a
//! method is defined: `deftrait`, a trait's `impl`, a blanket `impl`, and an
//! inherent `impl`. A parameter named `self` with a type next to it is
//! refused, whatever the type.

mod common;
use common::{check_err, eval_string, eval_string_compiled};

const DEFS: &str = r##"
(deftrait Named ()
  (name (self) string)
  (greet (self (to string)) string (format false "~a greets ~a" (name self) to)))
(defstruct Dog (nick string))
(impl Named Dog (name (self) string self::nick))
(deftrait Loud () (shout (self) string))
(impl<T> Loud T (:where (Named T)) (shout (self) string (format false "~a!" (name self))))
(impl Dog (pub twice (self) string (format false "~a~a" self::nick self::nick)))
"##;

/// `body` after [`DEFS`], interpreted and inside a compiled function.
fn both(body: &str, expected: &str) {
    let interpreted = eval_string(&format!("{}\n{}", DEFS, body));
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("{}\n(defun go () string {})\n(compile go)\n(go)", DEFS, body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

#[test]
fn a_bare_self_is_the_receiver_everywhere() {
    both(
        r##"(let ((d (make-instance 'Dog :nick "rex"))) (format false "~a/~a/~a" (greet d "bob") (shout d) (twice d)))"##,
        "rex greets bob/rex!/rexrex",
    );
}

fn assert_refused(src: &str) {
    let msg = check_err(src);
    assert!(msg.contains("the receiver is written `self` alone"), "{}", msg);
}

#[test]
fn a_typed_self_is_refused_in_deftrait() {
    assert_refused("(deftrait T1 () (m ((self Self)) int))");
    assert_refused("(deftrait T1 () (m ((self Self)) int 1))");
}

#[test]
fn a_typed_self_is_refused_in_a_trait_impl() {
    assert_refused("(deftrait T1 () (m (self) int)) (defstruct P (x int)) (impl T1 P (m ((self Self)) int 1))");
    assert_refused("(deftrait T1 () (m (self) int)) (defstruct P (x int)) (impl T1 P (m ((self P)) int 1))");
}

#[test]
fn a_typed_self_is_refused_in_a_blanket_impl() {
    assert_refused("(deftrait T1 () (m (self) int)) (impl<T> T1 T (m ((self T)) int 1))");
}

#[test]
fn a_typed_self_is_refused_in_an_inherent_impl() {
    assert_refused("(defstruct P (x int)) (impl P (m ((self Self)) int 1))");
    assert_refused("(defstruct P (x int)) (impl P (m ((self P)) int 1))");
}

#[test]
fn a_typed_self_is_refused_after_the_first_parameter() {
    assert_refused("(defstruct P (x int)) (impl P (m ((k int) (self P)) int 1))");
}
