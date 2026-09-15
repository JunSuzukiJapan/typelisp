//! `int` — CL's `integer`: a 63-bit fixnum that promotes to a heap bignum
//! when a result outgrows it and demotes back when it fits, with the box
//! never holding a fixnum-range value (`Heap::canonical_int`).
//!
//! Every arithmetic test runs its function twice, interpreted and compiled,
//! and asserts the two agree: the compiled fast path (`compile-assoc`'s `int`
//! branch, an overflow-checked instruction on the tagged words) and the
//! interpreter's `i128` path are two implementations of one number, and the
//! boundary between the fixnum and the box is exactly where they would
//! disagree if either were wrong.

mod common;

use common::{check_err, eval_ok, eval_string, eval_string_compiled};
use typelisp::Value;

/// `expr` (an `int`) compared with `=` against the literal `expected`,
/// interpreted and compiled, asserting both agree and both say `true`.
///
/// Compared rather than printed because an `int` has no `Sexpr` encoding
/// yet (that arrives with the `Sexpr` variant reshuffle), so `~a` cannot
/// take one; `=` on `int` is numeric equality across the fixnum/bignum
/// boundary, which is exactly the property under test.
fn both(defs: &str, names: &[&str], expr: &str, expected: &str) {
    both_true(defs, names, &format!("(= {} (the int {}))", expr, expected));
}

/// `expr` (a `bool`) rendered with `~a`, interpreted and compiled, asserting
/// both agree and both are `true`.
fn both_true(defs: &str, names: &[&str], expr: &str) {
    let interpreted = eval_string(&format!("{}\n(format false \"~a\" {})", defs, expr));
    let compiles: String = names.iter().map(|n| format!("(compile {})\n", n)).collect();
    let compiled = eval_string_compiled(&format!("{}\n{}(format false \"~a\" {})", defs, compiles, expr));
    assert_eq!(interpreted, "true", "interpreted: {}", expr);
    assert_eq!(compiled, "true", "compiled: {}", expr);
}

const ARITH: &str = r#"
    (defun iadd ((a int) (b int)) int (+ a b))
    (defun isub ((a int) (b int)) int (- a b))
    (defun imul ((a int) (b int)) int (* a b))
    (defun idiv ((a int) (b int)) int (/ a b))
    (defun irem ((a int) (b int)) int (mod a b))
    (defun big () int (the int 4611686018427387903))
"#;
const NAMES: &[&str] = &["iadd", "isub", "imul", "idiv", "irem", "big"];

#[test]
fn addition_past_the_fixnum_range_promotes_to_a_bignum() {
    // FIXNUM_MAX + 1 has no fixnum; the answer is the number, boxed.
    both(ARITH, NAMES, "(iadd (big) 1)", "4611686018427387904");
    both(ARITH, NAMES, "(iadd (big) (big))", "9223372036854775806");
    both(ARITH, NAMES, "(isub (isub 0 (big)) 2)", "-4611686018427387905");
}

#[test]
fn results_that_fit_demote_back_to_a_fixnum() {
    // Up and back: the sum is a box, the difference a fixnum again — and
    // `eq` on the fixnum is identity, so this is only true if the box
    // actually went away.
    both(ARITH, NAMES, "(isub (iadd (big) 1) 1)", "4611686018427387903");
    both_true(ARITH, NAMES, "(eq (isub (iadd (big) 1) 1) (big))");
}

#[test]
fn min_over_minus_one_promotes_instead_of_panicking() {
    // The one quotient a fixnum cannot hold. The fixed widths raise here.
    both(ARITH, NAMES, "(idiv (isub 0 (iadd (big) 1)) -1)", "4611686018427387904");
    both(ARITH, NAMES, "(irem (isub 0 (iadd (big) 1)) -1)", "0");
    both(ARITH, NAMES, "(irem -7 3)", "2");
    both(ARITH, NAMES, "(irem 7 -3)", "-2");
    both(ARITH, NAMES, "(idiv -7 3)", "-2");
}

#[test]
fn multiplication_overflow_is_caught_on_both_sides_of_the_fixnum_boundary() {
    // 2^31 * 2^31 = 2^62 = FIXNUM_MAX + 1: the product of two small fixnums
    // that lands exactly one past the range.
    both(ARITH, NAMES, "(imul 2147483648 2147483648)", "4611686018427387904");
    both(ARITH, NAMES, "(imul 2147483648 2147483647)", "4611686016279904256");
    both(ARITH, NAMES, "(imul (isub 0 2147483648) 2147483648)", "-4611686018427387904");
    both(ARITH, NAMES, "(imul (big) (big))", "21267647932558653957237540927630737409");
    both(ARITH, NAMES, "(imul (big) 0)", "0");
    both(ARITH, NAMES, "(imul (iadd (big) 1) -1)", "-4611686018427387904");
}

#[test]
fn mixed_fixnum_and_bignum_operands_compare_and_hash_consistently() {
    both_true(ARITH, NAMES, "(< (big) (iadd (big) 1))");
    both_true(ARITH, NAMES, "(not (> (big) (iadd (big) 1)))");
    both_true(ARITH, NAMES, "(= (iadd (big) 1) (iadd (big) 1))");
    both_true(ARITH, NAMES, "(= (isub (iadd (big) 1) 1) (big))");
    both(ARITH, NAMES, "(max (big) (iadd (big) 1))", "4611686018427387904");
    both(ARITH, NAMES, "(min (isub 0 (iadd (big) 1)) 0)", "-4611686018427387904");
    // A fixnum and a bignum can never be the same number, so `eql` on a
    // demoted result is a fixnum comparison, never a box identity.
    both_true(ARITH, NAMES, "(eql (isub (iadd (big) 1) 1) (big))");
}

#[test]
fn narrow_widens_to_int_with_as_and_narrows_back_with_try_as() {
    assert_eq!(eval_ok("(as int (the i32 -5))"), Value::Int(-5));
    assert_eq!(eval_ok("(as int (the u32 4000000000))"), Value::Int(4_000_000_000));
    assert_eq!(eval_ok("(as i32 (the int 70000))"), Value::Int(70000));
    // Truncation is what a width cast means; the question form says so.
    assert_eq!(eval_ok("(as u8 (the int 300))"), Value::Int(44));
    assert_eq!(eval_string("(format false \"~a\" (try-as u8 (the int 300)))"), "none");
    assert_eq!(eval_string("(format false \"~a\" (try-as u8 (the int 200)))"), "(some 200)");
    assert_eq!(
        eval_string("(format false \"~a\" (try-as i32 (+ (the int 4611686018427387903) 1)))"),
        "none"
    );
    assert_eq!(eval_string("(format false \"~a\" (as f64 (the int 3)))"), "3.0");
}

#[test]
fn an_int_literal_takes_its_expected_type_at_any_size() {
    // Past `i32` the reader boxes the literal; in an `int` position it is an
    // `int` whether or not it fits a fixnum.
    assert_eq!(eval_ok("(the int 3000000000)"), Value::Int(3_000_000_000));
    both(ARITH, NAMES, "(the int 100000000000000000000)", "100000000000000000000");
    both(ARITH, NAMES, "(iadd (the int 100000000000000000000) (the int 100000000000000000000))", "200000000000000000000");
}

#[test]
fn int_and_the_fixed_widths_do_not_mix_silently() {
    let msg = check_err("(+ (the int 1) (the i32 2))");
    assert!(msg.contains("Int") && msg.contains("I32"), "{}", msg);
}

/// The conversions and the bitwise catalog, compiled and interpreted: every
/// `int` method the island lowers has an interpreter twin, and this is the
/// one place both are run on the same inputs.
#[test]
fn conversions_and_bit_ops_agree_in_both_tiers() {
    const DEFS: &str = r#"
        (defun big () int (the int 4611686018427387903))
        (defun widen ((x i32)) int (as int x))
        (defun narrow ((x int)) u8 (as u8 x))
        (defun fits ((x int)) bool (match (try-as i32 x) ((some _) true) ((none) false)))
        (defun as-char ((x int)) bool (match (try-as char x) ((some c) (equal c #\A)) ((none) false)))
        (defun to-float ((x int)) f64 (as f64 x))
        (defun band ((a int) (b int)) int (logand a b))
        (defun bor ((a int) (b int)) int (logior a b))
        (defun bnot ((a int)) int (lognot a))
        (defun shl ((a int) (n i32)) int (ash a n))
        (defun bit ((a int) (n i32)) bool (logbitp a n))
        (defun bits ((a int)) int (logcount a))
        (defun len ((a int)) int (integer-length a))
    "#;
    const N: &[&str] = &["big", "widen", "narrow", "fits", "as-char", "to-float", "band", "bor", "bnot", "shl", "bit", "bits", "len"];
    both_true(DEFS, N, "(= (widen -5) (the int -5))");
    both_true(DEFS, N, "(= (narrow (the int 300)) (the u8 44))");
    both_true(DEFS, N, "(fits (the int 100))");
    both_true(DEFS, N, "(not (fits (big)))");
    both_true(DEFS, N, "(not (fits (+ (big) 1)))");
    both_true(DEFS, N, "(as-char (the int 65))");
    both_true(DEFS, N, "(not (as-char (the int 55296)))");
    both_true(DEFS, N, "(= (to-float (the int 3)) 3.0)");
    // A bignum and a fixnum operand: the slow path on both sides.
    both_true(DEFS, N, "(= (band (+ (big) 1) 1) 0)");
    both_true(DEFS, N, "(= (bor (+ (big) 1) 1) (+ (big) 2))");
    both_true(DEFS, N, "(= (band 12 10) 8)");
    both_true(DEFS, N, "(= (bnot 0) -1)");
    both_true(DEFS, N, "(= (bnot (+ (big) 1)) (- (the int 0) (+ (big) 2)))");
    both_true(DEFS, N, "(= (shl 1 62) (+ (big) 1))");
    both_true(DEFS, N, "(= (shl (+ (big) 1) -62) 1)");
    both_true(DEFS, N, "(bit (+ (big) 1) 62)");
    both_true(DEFS, N, "(not (bit (+ (big) 1) 61))");
    both_true(DEFS, N, "(= (bits (+ (big) 1)) 1)");
    both_true(DEFS, N, "(= (len (+ (big) 1)) 63)");
    both_true(DEFS, N, "(= (len 255) 8)");
}
