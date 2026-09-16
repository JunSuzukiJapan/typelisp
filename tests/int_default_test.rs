//! `int` as the language's integer: the type of an unannotated literal, the
//! type every counting/indexing builtin answers with, and the type a
//! Rust-implemented builtin's `int` parameters cross the compiled boundary
//! at — as raw machine words, wrapped by the checker in `untag-int`/`tag-int`
//! (`Checker::int_boundary_raw`).
//!
//! Every boundary case below runs interpreted and compiled and asserts the
//! two agree, because the boundary is the one place the two tiers hold the
//! same value in different shapes (a tagged word in a register, a raw word in
//! a shim), and a wrapper missing on either side is a wrong answer in exactly
//! one of them.

mod common;

use common::{check_err, eval_ok, eval_string, eval_string_compiled};
use typelisp::Value;

/// `expr` rendered with `~a`, interpreted and compiled (after `(compile
/// NAME)` for each of `names`), asserting the two agree; returns the text.
fn both(defs: &str, names: &[&str], expr: &str) -> String {
    let interpreted = eval_string(&format!("{}\n(format false \"~a\" {})", defs, expr));
    let compiles: String = names.iter().map(|n| format!("(compile {})\n", n)).collect();
    let compiled = eval_string_compiled(&format!("{}\n{}(format false \"~a\" {})", defs, compiles, expr));
    assert_eq!(interpreted, compiled, "interpreted and compiled disagree for {}", expr);
    interpreted
}

#[test]
fn an_unannotated_literal_is_an_int() {
    // `(the int ..)` is a no-op on an `int`; on anything else it is a type
    // error, so this is the literal's type being asked directly.
    assert_eq!(eval_ok("(the int 42)"), Value::Int(42));
    let msg = check_err("(the i32 (let ((x 42)) x))");
    assert!(msg.contains("Int") && msg.contains("I32"), "{}", msg);
    // Past `i32`, past `i64`: still one type, and the reader made the datum
    // in the shape its size demands.
    assert_eq!(eval_string("(format false \"~a\" 3000000000)"), "3000000000");
    assert_eq!(eval_string("(format false \"~a\" 100000000000000000000)"), "100000000000000000000");
}

#[test]
fn a_literal_still_takes_the_expected_narrow_type() {
    assert_eq!(eval_ok("(the u8 200)"), Value::Int(200));
    assert_eq!(eval_string("(format false \"~a\" (+ (the u8 200) 100))"), "44");
    let msg = check_err("(the u8 300)");
    assert!(msg.contains("u8"), "{}", msg);
}

#[test]
fn a_literal_past_63_bits_is_an_int_bignum_in_both_tiers() {
    const DEFS: &str = "(defun big () int 100000000000000000000)";
    assert_eq!(both(DEFS, &["big"], "(big)"), "100000000000000000000");
    assert_eq!(both(DEFS, &["big"], "(+ (big) (big))"), "200000000000000000000");
    assert_eq!(both(DEFS, &["big"], "(- (big) 99999999999999999999)"), "1");
}

/// The raw boundary, builtin by builtin: an `int` argument is untagged on
/// the way in and an `int` result tagged on the way out, and the value is
/// the same in both tiers.
#[test]
fn int_typed_builtins_cross_the_boundary_in_both_tiers() {
    const DEFS: &str = r#"
        (defun len ((s string)) int (length s))
        (defun sub ((s string) (a int) (b int)) string (substring s a b))
        (defun at ((s string) (i int)) char (ref s i))
        (defun code ((c char)) int (char->int c))
        (defun vlen ((v Vector<int>)) int (len v))
        (defun vget ((v Vector<int>) (i int)) int (get v i))
        (defun vset ((v Vector<int>) (i int) (x int)) () (set v i x))
        (defun hcount ((h HashTable<int,int>)) int (count h))
        (defun mk () Vector<int> (let ((v (the Vector<int> (Vector::new)))) (progn (push v 10) (push v 20) (push v 30) v)))
        (defun shifted ((x int) (n int)) int (ash x n))
        (defun bit ((x int) (n int)) bool (logbitp x n))
        (defun narrow-shift ((x u8) (n int)) u8 (ash x n))
        (defun ratio-parts ((r ratio)) int (+ (numerator r) (denominator r)))
        (defun trunc ((f f64)) int (as int f))
        (defun total () int
          (let ((h (the HashTable<int,int> (HashTable::new))))
            (progn (set h 1 2) (set h 3 4)
                   (+ (+ (len "hello") (code #\A))
                      (+ (+ (vlen (mk)) (vget (mk) 2)) (+ (hcount h) (ratio-parts 7/2)))))))
    "#;
    const N: &[&str] = &["len", "sub", "at", "code", "vlen", "vget", "vset", "hcount", "mk", "shifted", "bit", "narrow-shift", "ratio-parts", "trunc", "total"];
    assert_eq!(both(DEFS, N, "(len \"hello\")"), "5");
    assert_eq!(both(DEFS, N, "(sub \"hello\" 1 3)"), "el");
    assert_eq!(both(DEFS, N, "(at \"hello\" 4)"), "o");
    assert_eq!(both(DEFS, N, "(code #\\A)"), "65");
    assert_eq!(both(DEFS, N, "(vget (mk) 1)"), "20");
    assert_eq!(both(DEFS, N, "(let ((v (mk))) (progn (vset v 0 7) (vget v 0)))"), "7");
    assert_eq!(both(DEFS, N, "(shifted 1 62)"), "4611686018427387904");
    assert_eq!(both(DEFS, N, "(shifted 4611686018427387904 -62)"), "1");
    assert_eq!(both(DEFS, N, "(bit 5 2)"), "true");
    assert_eq!(both(DEFS, N, "(narrow-shift (the u8 200) -3)"), "25");
    assert_eq!(both(DEFS, N, "(ratio-parts 7/2)"), "9");
    assert_eq!(both(DEFS, N, "(trunc 1.0e30)"), "1000000000000000019884624838656");
    // 5 + 65 + 3 + 30 + 2 + 9
    assert_eq!(both(DEFS, N, "(total)"), "114");
}

/// An `int` that is a bignum in a machine-word position — an index, a
/// count, a shift distance — is a language-level error in both tiers, not a
/// truncation and not an abort.
#[test]
fn an_index_that_is_not_a_fixnum_raises_rather_than_truncating() {
    const DEFS: &str = r#"
        (defun sub-at ((s string) (i int)) string (substring s 0 i))
        (defun guarded ((s string) (i int)) string
          (catch 'oops (sub-at s i)))
    "#;
    let src = format!("{}\n(sub-at \"hello\" (+ 4611686018427387903 1))", DEFS);
    let interpreted = common::run(&src).expect_err("a bignum index is an error");
    assert!(format!("{:?}", interpreted).contains("does not fit a fixnum"), "{:?}", interpreted);
    let compiled = common::run_compiled(&format!("{}\n(compile sub-at)\n(sub-at \"hello\" (+ 4611686018427387903 1))", DEFS))
        .expect_err("a bignum index is an error when compiled too");
    assert!(format!("{:?}", compiled).contains("does not fit a fixnum"), "{:?}", compiled);
}
