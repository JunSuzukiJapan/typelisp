//! Every integer width and both float widths as *numbers*, and `as`/`try-as`
//! between all of them —
//! [cl-parity-plan.md](../docs/dev/cl-parity-plan.md) Phase 1a/1b.
//!
//! Before this, `i8`/`i16`/`u8`/`u16`/`u32`/`f32` were registered as types a
//! `defmethod` could name and had no methods at all — not even `+`. They now
//! carry the same built-in catalog `i32`/`f64` do, the same `Eq`/`Ord` impls,
//! and the arithmetic traits that let a generic function ask for a type it
//! can add.
//!
//! Width is *not* a static distinction and nothing else: a type name means
//! its width and its signedness (`types::int_width_signed`), so arithmetic
//! wraps at that width and `f32` rounds to binary32. Crossing widths with
//! `as` is a real conversion. The tests that pin that down say so by name.

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
    assert_eq!(show(r#"(format false "~a" (- (the u16 50) (the u16 8)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (/ (the u32 84) (the u32 2)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (mod (the u32 142) (the u32 100)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (rem (the i16 -7) (the i16 3)))"#), "-1");
    assert_eq!(show(r#"(format false "~a" (max (the i16 42) (the i16 7)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (logand (the u16 63) (the u16 42)))"#), "42");
}

#[test]
fn f32_has_the_f64_catalog() {
    assert_eq!(show(r#"(format false "~a" (+ (the f32 40.0) (the f32 2.0)))"#), "42.0");
    assert_eq!(show(r#"(format false "~a" (sqrt (the f32 16.0)))"#), "4.0");
    assert_eq!(show(r#"(format false "~a" (< (the f32 1.5) (the f32 2.5)))"#), "true");
}

/// Crossing widths converts. `300` does not fit a `u8`, so `as` truncates it
/// to `44` and `try-as` answers `none` — the two halves of what a width cast
/// means. Pinned as a test because this is the one thing about these types a
/// reader is most likely to assume wrongly; it used to be a relabel, and a
/// `u8` could hold `300`.
#[test]
fn crossing_integer_widths_converts() {
    assert_eq!(show(r#"(format false "~a" (as u8 (the i32 300)))"#), "44");
    assert_eq!(show(r#"(format false "~a" (as i32 (the u8 42)))"#), "42");
    assert_eq!(show(r#"(format false "~a" (as i16 (the u16 42)))"#), "42");
    // A value that does fit crosses unchanged, and `try-as` says so.
    assert_eq!(show(r#"(format false "~a" (unwrap (try-as u8 (the i32 200))))"#), "200");
    // One that does not is `none`, not a truncated `some`. Answered as text
    // rather than a sentinel number: a sentinel would have to be a `u8` too,
    // and the out-of-range one this test used to use (`-1`) is exactly the
    // thing `int_lit_in_range` now refuses.
    assert_eq!(
        show(r#"(match (try-as u8 (the i32 300)) ((some n) (format false "some ~a" n)) ((none) "none"))"#),
        "none"
    );
}

/// Arithmetic wraps at the receiver's own width and signedness, which is the
/// whole content of a type name.
#[test]
fn arithmetic_wraps_at_the_types_own_width() {
    assert_eq!(show(r#"(format false "~a" (+ (the u8 200) (the u8 100)))"#), "44");
    assert_eq!(show(r#"(format false "~a" (* (the i8 100) (the i8 3)))"#), "44");
    assert_eq!(show(r#"(format false "~a" (+ (the i32 2147483647) 1))"#), "-2147483648");
    assert_eq!(show(r#"(format false "~a" (lognot (the u32 0)))"#), "4294967295");
    assert_eq!(show(r#"(format false "~a" (lognot (the i32 0)))"#), "-1");
    // A shift past the type's own width is `0`, not the 64-bit register's.
    assert_eq!(show(r#"(format false "~a" (ash (the u16 1) 20))"#), "0");
}

/// `ash`'s second operand is a shift **distance**, not another value of the
/// receiver's type — so it is an `i32` at every width, and a right shift is
/// writable everywhere.
///
/// It used to be the receiver's own type, which made this whole test
/// impossible to write on an unsigned width: `(ash (the u8 x) -3)` was
/// rejected with "integer literal -3 is out of range for u8 (0..=255)", and
/// there was no other spelling of "shift right" to reach for. Half the
/// integer types simply had no right shift. Nothing under the checker had to
/// change to fix it — `eval_int_builtin` and `rt_int_ash` both already read
/// the operand as a signed count — which is the sign that the *type* was the
/// only thing that had ever been wrong.
#[test]
fn a_shift_distance_is_an_i32_at_every_width() {
    // Unsigned right shifts are logical: the sign bit is not a sign here.
    assert_eq!(show(r#"(format false "~a" (ash (the u8 200) -3))"#), "25");
    assert_eq!(show(r#"(format false "~a" (ash (the u16 60000) -4))"#), "3750");
    assert_eq!(show(r#"(format false "~a" (ash (the u32 4000000000) -8))"#), "15625000");
    // Signed right shifts are arithmetic, and round toward negative infinity
    // the way CL's `ash` does (`-100 >> 4` is `-7`, not `-6`).
    assert_eq!(show(r#"(format false "~a" (ash (the i8 -16) -2))"#), "-4");
    assert_eq!(show(r#"(format false "~a" (ash (the i32 -100) -4))"#), "-7");
    // Left shifts still wrap at the receiver's own width.
    assert_eq!(show(r#"(format false "~a" (ash (the u8 30) 3))"#), "240");
    assert_eq!(show(r#"(format false "~a" (ash (the u8 200) 3))"#), "64");
    // Past the width in either direction: `0`, or all-ones for a negative
    // signed value, never the 64-bit register's answer.
    assert_eq!(show(r#"(format false "~a" (ash (the u16 1) 20))"#), "0");
    assert_eq!(show(r#"(format false "~a" (ash (the u16 65535) -20))"#), "0");
    assert_eq!(show(r#"(format false "~a" (ash (the i8 -1) -20))"#), "-1");
}

/// `f32` is binary32, not a label on an `f64` — in the arithmetic *and* in
/// the printing.
///
/// These three used to read `0.3333333432674408` and `0.10000000149011612`:
/// the arithmetic was already binary32, but every float went into one
/// `BoxedObj::Float` that could not say which width it held, so the printer
/// had no choice but to print binary64's shortest round-trip digits for a
/// number that is not a binary64 value. Splitting the box (`Float32`/
/// `Float64`) is what makes the text read back as the same `f32`.
#[test]
fn f32_arithmetic_rounds_to_binary32() {
    assert_eq!(show(r#"(format false "~a" (/ (the f32 1.0) (the f32 3.0)))"#), "0.33333334");
    assert_eq!(show(r#"(format false "~a" (/ 1.0 3.0))"#), "0.3333333333333333");
    assert_eq!(show(r#"(format false "~a" (as f32 0.1))"#), "0.1");
    // `try-as` reports whether the rounding lost anything.
    assert_eq!(
        show(r#"(format false "~a" (match (try-as f32 0.1) ((some _) "some") ((none) "none")))"#),
        "none"
    );
    assert_eq!(
        show(r#"(format false "~a" (match (try-as f32 0.5) ((some _) "some") ((none) "none")))"#),
        "some"
    );
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
          (sum3 (the i16 10) (the i16 20) (the i16 12)))
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

// ---- complex numbers (Phase 1d) ------------------------------------------

/// The components are `f64` and the arithmetic is the ordinary complex
/// arithmetic — `(1+2i)(3+4i) = -5+10i`.
#[test]
fn complex_arithmetic_works() {
    assert_eq!(show(r#"(format false "~a" (+ (complex 1.0 2.0) (complex 3.0 4.0)))"#), "#C(4.0 6.0)");
    assert_eq!(show(r#"(format false "~a" (- (complex 1.0 2.0) (complex 3.0 4.0)))"#), "#C(-2.0 -2.0)");
    assert_eq!(show(r#"(format false "~a" (* (complex 1.0 2.0) (complex 3.0 4.0)))"#), "#C(-5.0 10.0)");
    assert_eq!(show(r#"(format false "~a" (/ (complex -5.0 10.0) (complex 3.0 4.0)))"#), "#C(1.0 2.0)");
}

#[test]
fn the_complex_accessors_answer_for_reals_too() {
    assert_eq!(show(r#"(format false "~a ~a" (realpart (complex 1.0 2.0)) (imagpart (complex 1.0 2.0)))"#), "1.0 2.0");
    // CL: every real is a complex whose imaginary part is zero.
    assert_eq!(show(r#"(format false "~a ~a" (realpart 3.0) (imagpart 3.0))"#), "3.0 0.0");
    assert_eq!(show(r#"(format false "~a" (conjugate (complex 1.0 2.0)))"#), "#C(1.0 -2.0)");
}

/// `i^2 = -1`, reached through the principal square root.
#[test]
fn the_principal_square_root_of_minus_one_is_i() {
    let src = r#"
        (defvar (i complex) (sqrt (complex -1.0 0.0)))
        (format false "~a ~a" (round (imagpart i)) (round (realpart (* i i))))
    "#;
    assert_eq!(show(src), "1.0 -1.0");
}

/// Euler: `e^(i*pi) = -1`.
#[test]
fn eulers_identity_holds_to_rounding() {
    let src = r#"(format false "~a ~a" (round (realpart (exp (complex 0.0 pi)))) (round (imagpart (exp (complex 0.0 pi)))))"#;
    assert_eq!(show(src), "-1.0 0.0");
}

#[test]
fn phase_and_abs_are_the_polar_pair() {
    assert_eq!(show(r#"(format false "~a" (abs (complex 3.0 4.0)))"#), "5.0");
    assert_eq!(show(r#"(format false "~a" (round (* 4.0 (phase (complex 0.0 1.0)))))"#), "6.0");
    // `(cis theta)` is the unit complex at `theta`.
    assert_eq!(show(r#"(format false "~a" (round (realpart (cis 0.0))))"#), "1.0");
}

/// CL's two-argument `(atan y x)`, which `phase` is built on.
#[test]
fn atan_takes_two_arguments_as_in_cl() {
    assert_eq!(show(r#"(format false "~a" (round (* 4.0 (atan 1.0 1.0))))"#), "3.0");
    assert_eq!(show(r#"(format false "~a" (round (* 4.0 (atan 1.0))))"#), "3.0");
}

// ---- integer literals ---------------------------------------------------
//
// A literal is the one place a program states a number and a type
// independently of each other, so it is the one place the invariant behind
// every width-aware operation — a value *is* the number its type names — can
// be broken just by writing it down. These pin both halves of the rule the
// checker applies: a literal takes the type its context names, and only if
// that type can hold it.

/// The top half of `u32` is writable. The reader cannot know the type a token
/// lands in, so it turns everything past `i32` into a `bignum`; the checker
/// knows, and takes it back as the width the context names. Without this,
/// `u32`'s upper half had no literal syntax at all.
#[test]
fn a_literal_past_i32_takes_the_width_its_context_names() {
    assert_eq!(show(r#"(format false "~a" (the u32 4294967295))"#), "4294967295");
    assert_eq!(show(r#"(format false "~a" (the u32 #xFFFFFFFF))"#), "4294967295");
    // And it is a real `u32`, not a relabelled `bignum`: it wraps.
    assert_eq!(show(r#"(format false "~a" (+ (the u32 4294967295) (the u32 1)))"#), "0");
}

/// With no context to name a width, a literal past `i32` is still a `bignum`
/// — which is what keeps `*print-radix*` round-tripping (`#x...` printed for
/// a `bignum` reads back as one).
#[test]
fn a_literal_past_i32_is_still_a_bignum_without_a_context() {
    assert_eq!(show(r#"(format false "~a" #x10000000000000000)"#), "18446744073709551616");
}

/// A literal outside its type's range is refused rather than cut. The cut is
/// a thing a caller can ask for and mean, and it is spelled `as`.
#[test]
fn a_literal_outside_its_types_range_is_a_type_error() {
    let e = check_error("(the u8 300)");
    assert!(e.contains("out of range for u8 (0..=255)"), "{}", e);
    assert!(e.contains("(as u8 300)"), "{}", e);

    let e = check_error("(the u32 -5)");
    assert!(e.contains("out of range for u32 (0..=4294967295)"), "{}", e);

    let e = check_error("(the i8 128)");
    assert!(e.contains("out of range for i8 (-128..=127)"), "{}", e);

    // The boundaries themselves are fine.
    assert_eq!(show(r#"(format false "~a" (the u8 255))"#), "255");
    assert_eq!(show(r#"(format false "~a" (the i8 -128))"#), "-128");
}

/// A `bignum` literal too big for any fixed-width type says so, and does
/// *not* suggest `as`: a `bignum` reaches the width cast through
/// `bignum->int`, which is itself `i32`-wide, so `as` would fail a second
/// time rather than do what the message promised.
#[test]
fn a_bignum_literal_in_a_width_position_does_not_suggest_as() {
    let e = check_error("(the u32 #x10000000000000000)");
    assert!(e.contains("out of range for u32"), "{}", e);
    assert!(e.contains("read as a `bignum`"), "{}", e);
    assert!(!e.contains("(as u32"), "{}", e);
}

/// The cut-back is still available, spelled out.
#[test]
fn as_still_cuts_a_literal_to_width() {
    assert_eq!(show(r#"(format false "~a" (as u8 300))"#), "44");
}
