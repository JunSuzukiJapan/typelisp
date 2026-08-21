//! Every integer width and both float widths as *numbers*, and `as`/`try-as`
//! between all of them —
//! [cl-parity-plan.md](../docs/dev/cl-parity-plan.md) Phase 1a/1b.
//!
//! Before this, `i8`/`i16`/`isize`/`u8`/`u16`/`u32`/`u64`/`usize`/`f32` were
//! registered as types a `defmethod` could name and had no methods at all —
//! not even `+`. They now carry the same built-in catalog `i32`/`f64` do, the
//! same `Eq`/`Ord` impls, and the arithmetic traits that let a generic
//! function ask for a type it can add.
//!
//! Width is a static distinction and nothing else here: the runtime value is
//! an `i64` or an `f64` whichever type labels it, which is exactly why
//! crossing widths with `as` is a relabel. The tests that pin that down say so
//! by name.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, Value};

/// Runs `src`'s forms with the prelude loaded and renders the last value with
/// `~a`.
fn show(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).expect("eval failed") {
            last = val;
        }
    }
    match last {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// The same, but expecting the check to fail — returns the message.
fn check_error(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut err = None;
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Ok(tl) => {
                interp.exec(&mut h, tl).expect("eval failed");
            }
            Err(e) => {
                err = Some(e.to_string());
                break;
            }
        }
    }
    err.expect("expected a check error")
}

/// A literal takes the width it is asked for, and the arithmetic follows.
#[test]
fn every_integer_width_can_do_arithmetic() {
    assert_eq!(show(r#"(format false "~a" (+ (the u8 40) (the u8 2)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (* (the i8 6) (the i8 7)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (- (the usize 50) (the usize 8)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (/ (the u64 84) (the u64 2)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (mod (the u32 142) (the u32 100)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (rem (the i16 -7) (the i16 3)))"#), "-1");
    assert_eq!(show(r#"(format false "~a" (max (the isize 42) (the isize 7)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (logand (the u16 63) (the u16 42)))"#), "42");
}

#[test]
fn f32_has_the_f64_catalog() {
    assert_eq!(show(r#"(format false "~a" (+ (the f32 40.0) (the f32 2.0)))"#), "42.0");
    assert_eq!(show(r#"(format false "~a" (sqrt (the f32 16.0)))"#), "4.0");
    assert_eq!(show(r#"(format false "~a" (< (the f32 1.5) (the f32 2.5)))"#), "true");
}

/// The runtime value does not change when the static width does: an `i64`
/// holding 300 keeps holding 300 after `(as u8 ...)`. Pinned as a test because
/// it is the one thing about these types a reader is most likely to assume
/// wrongly.
#[test]
fn crossing_integer_widths_is_a_relabel_and_does_not_truncate() {
    assert_eq!(show(r#"(format false "~a" (as u8 (the i32 300)))"#), "300");
    assert_eq!(show(r#"(format false "~a" (as i64 (the u8 42)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (as isize (the u16 42)))"#), "42");
    // ...and `try-as` between two widths therefore always succeeds.
    assert_eq!(show(r#"(format false "~a" (unwrap (try-as u8 (the i64 300))))"#), "300");
}

#[test]
fn every_width_converts_across_the_numeric_families() {
    assert_eq!(show(r#"(format false "~a" (as f64 (the u8 42)))"#), "42.0");
    assert_eq!(show(r#"(format false "~a" (as f32 (the u8 42)))"#), "42.0");
    assert_eq!(show(r#"(format false "~a" (as u8 (the f64 42.9)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (as bignum (the u32 42)))"#), "42");
    // `ratio` prints as a ratio whatever it was made from — `42/1`, not `42`.
    assert_eq!(show(r#"(format false "~a" (as ratio (the i8 42)))"#), "42/1");
    assert_eq!(show(r#"(format false "~a" (as char (the u32 65)))"#), "A");
    assert_eq!(show(r#"(format false "~a" (as f64 (the f32 1.5)))"#), "1.5");
}

/// `Eq`/`Ord` reach the new widths, which is what makes them usable as
/// elements: `member`/`sort`/`case` are all bounded by one or the other.
#[test]
fn the_new_widths_are_eq_and_ord() {
    let src = r#"
        (defun main () string
          (let ((v (the Vector<u8> (Vector::new))))
            (push v (the u8 9))
            (push v (the u8 3))
            (push v (the u8 7))
            (format false "~a ~a"
              (member (the u8 7) (iter v))
              (unwrap (first (iter (sort (iter v) (lambda ((a u8) (b u8)) bool (< a b)))))))))
        (format false "~a" (main))
    "#;
    assert_eq!(show(src), "true 3");
}

/// The point of the trait layer: a generic function that adds its own
/// arguments. Written with the operator, which the checker spells as the
/// bound's method.
#[test]
fn a_generic_function_can_add_its_own_arguments() {
    let src = r#"
        (defun sum3<T> ((a T) (b T) (c T)) T (where (Number T))
          (+ a b c))
        (format false "~a ~a ~a"
          (sum3 (the u8 1) (the u8 2) (the u8 3))
          (sum3 1.5 2.5 3.5)
          (sum3 (the i64 10) (the i64 20) (the i64 12)))
    "#;
    assert_eq!(show(src), "6 7.5 42");
}

/// Every operator the spelling table covers, through one bound.
#[test]
fn the_operators_reach_the_trait_methods_through_a_bound() {
    let src = r#"
        (defun mix<T> ((a T) (b T)) T (where (Number T))
          (if (< a b) (- b a) (rem a b)))
        (format false "~a ~a" (mix 3 10) (mix 10 3))
    "#;
    assert_eq!(show(src), "7 1");
}

#[test]
fn the_bitwise_trait_is_reachable_through_a_bound_too() {
    let src = r#"
        (defun mask<T> ((a T) (b T)) T (where (Bits T))
          (logand a b))
        (format false "~a" (mask 63 42))
    "#;
    assert_eq!(show(src), "42");
}

/// A bound that does not provide the operator is still an error — the
/// spelling table is a fallback for a name no bound declares, not a licence to
/// add anything to anything.
#[test]
fn an_operator_without_the_bound_is_still_an_error() {
    let msg = check_error(
        r#"
        (defun bad<T> ((a T) (b T)) T (where (Eq T))
          (+ a b))
        "#,
    );
    assert!(msg.contains("+") || msg.contains("add"), "unexpected message: {}", msg);
}
