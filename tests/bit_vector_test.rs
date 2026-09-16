//! Tests for `BitVector` — cl-parity-plan.md Phase 6c.
//!
//! A prelude `defstruct` over a `Vector<int>` of packed words plus a length.
//! What the tests here are mostly watching is the seam between those two: the
//! bits past the length in the final word must stay clear, or `lognot` leaves
//! phantom bits behind and two bit vectors of equal length stop agreeing.

extern crate typelisp;

mod common;
use common::{eval_err, eval_ok};
use typelisp::Value;

/// `(bits "1011")` builds a bit vector from a written bit pattern, and
/// `(show v)` reads one back — CL would write both as `#*1011`.
const HELPERS: &str = r#"
(defun bits ((s string)) BitVector
  (let ((v (BitVector::make (length s))) (i 0))
    (progn
      (while (< i (length s))
        (progn (set v i (equal (ref s i) #\1)) (setf i (+ i 1))))
      v)))
(defun show ((v BitVector)) string
  (let ((out "") (i 0))
    (progn
      (while (< i (len v))
        (progn (setf out (append out (if (get v i) "1" "0"))) (setf i (+ i 1))))
      out)))
"#;

fn with_helpers(src: &str) -> String {
    format!("{}\n{}", HELPERS, src)
}

fn shows(src: &str, expect: &str) {
    assert_eq!(eval_ok(&with_helpers(&format!("(equal (show {}) \"{}\")", src, expect))), Value::Bool(true), "{}", src);
}

// ---- shape ------------------------------------------------------------------

#[test]
fn make_gives_a_vector_of_the_requested_length_with_every_bit_clear() {
    assert_eq!(eval_ok("(len (BitVector::make 5))"), Value::Int(5));
    shows("(BitVector::make 5)", "00000");
}

#[test]
fn a_zero_length_bit_vector_is_allowed() {
    assert_eq!(eval_ok("(len (BitVector::make 0))"), Value::Int(0));
    shows("(BitVector::make 0)", "");
}

#[test]
fn a_negative_length_is_refused() {
    assert!(eval_err("(BitVector::make -1)").contains("negative"));
}

// ---- get / set --------------------------------------------------------------

#[test]
fn set_and_get_agree() {
    shows("(bits \"1011\")", "1011");
}

#[test]
fn clearing_a_bit_leaves_its_neighbours_alone() {
    let src = "(let ((v (bits \"1111\"))) (progn (set v 1 false) v))";
    shows(src, "1011");
}

#[test]
fn bits_are_addressed_independently_across_a_word_boundary() {
    // 31 bits to a word, so 30 and 31 are the last bit of word 0 and the
    // first of word 1. The bit at the top of a word is the one worth pinning:
    // packing 31 rather than 32 is what keeps the sign bit clear, so a word
    // is always a non-negative number (see the type's own note).
    let src = "(let ((v (BitVector::make 70)))
                 (progn (set v 30 true) (set v 31 true)
                        (+ (if (get v 30) 1 0) (* 2 (if (get v 31) 1 0))
                           (* 4 (if (get v 29) 1 0)) (* 8 (if (get v 32) 1 0)))))";
    assert_eq!(eval_ok(&with_helpers(src)), Value::Int(3));
}

#[test]
fn every_bit_of_a_multi_word_vector_can_be_set_on_its_own() {
    // One bit at a time across three words: each must come back set, and it
    // must be the only one set.
    let src = "(let ((n 70) (bad 0))
                 (progn
                   (let ((i 0))
                     (while (< i n)
                       (progn
                         (let ((v (BitVector::make n)) (seen 0))
                           (progn
                             (set v i true)
                             (let ((j 0))
                               (while (< j n)
                                 (progn
                                   (setf seen (+ seen (if (get v j) 1 0)))
                                   (setf j (+ j 1)))))
                             (setf bad (+ bad (if (and (= seen 1) (get v i)) 0 1)))))
                         (setf i (+ i 1)))))
                   bad))";
    assert_eq!(eval_ok(&with_helpers(src)), Value::Int(0));
}

#[test]
fn an_index_past_the_end_is_an_error() {
    assert!(eval_err("(get (BitVector::make 4) 4)").contains("out of range"));
    assert!(eval_err("(set (BitVector::make 4) -1 true)").contains("out of range"));
    // Past the length but still inside the allocated word: the length is
    // what decides, not the packing.
    assert!(eval_err("(get (BitVector::make 4) 40)").contains("out of range"));
}

// ---- CL's `bit` / `sbit` ----------------------------------------------------

#[test]
fn bit_and_sbit_are_the_cl_names_for_the_same_access() {
    let all = "(let ((v (bits \"1010\"))) (and (bit v 0) (not (bit v 1)) (sbit v 2) (not (sbit v 3))))";
    assert_eq!(eval_ok(&with_helpers(all)), Value::Bool(true));
    let wrong = "(let ((v (bits \"1010\"))) (bit v 1))";
    assert_eq!(eval_ok(&with_helpers(wrong)), Value::Bool(false));
}

#[test]
fn setf_bit_writes_through_the_cl_name() {
    let src = "(let ((v (bits \"0000\"))) (progn (setf (bit v 2) true) v))";
    shows(src, "0010");
    let src2 = "(let ((v (bits \"0000\"))) (progn (setf (sbit v 1) true) v))";
    shows(src2, "0100");
}

#[test]
fn setf_get_writes_through_the_container_name_too() {
    let src = "(let ((v (bits \"0000\"))) (progn (setf (get v 3) true) v))";
    shows(src, "0001");
}

// ---- the bit-wise family ----------------------------------------------------

#[test]
fn the_four_named_operations_are_word_wise() {
    shows("(bit-and (bits \"1100\") (bits \"1010\"))", "1000");
    shows("(bit-ior (bits \"1100\") (bits \"1010\"))", "1110");
    shows("(bit-xor (bits \"1100\") (bits \"1010\"))", "0110");
    shows("(bit-not (bits \"1100\"))", "0011");
}

#[test]
fn the_rest_of_cls_family_is_there_too() {
    shows("(bit-eqv (bits \"1100\") (bits \"1010\"))", "1001");
    shows("(bit-nand (bits \"1100\") (bits \"1010\"))", "0111");
    shows("(bit-nor (bits \"1100\") (bits \"1010\"))", "0001");
    shows("(bit-andc1 (bits \"1100\") (bits \"1010\"))", "0010");
    shows("(bit-andc2 (bits \"1100\") (bits \"1010\"))", "0100");
    shows("(bit-orc1 (bits \"1100\") (bits \"1010\"))", "1011");
    shows("(bit-orc2 (bits \"1100\") (bits \"1010\"))", "1101");
}

#[test]
fn a_bit_wise_operation_leaves_its_operands_alone() {
    let src = "(let ((a (bits \"1100\")) (b (bits \"1010\")))
                 (progn (bit-and a b) (append (show a) (show b))))";
    assert_eq!(
        eval_ok(&with_helpers(&format!("(equal {} \"11001010\")", src))),
        Value::Bool(true)
    );
}

#[test]
fn operands_of_different_lengths_are_refused() {
    let src = "(bit-and (bits \"1100\") (bits \"101\"))";
    assert!(eval_err(&with_helpers(src)).contains("differ in length"));
}

// ---- the padding bits in the last word --------------------------------------

#[test]
fn bit_not_does_not_leave_set_bits_past_the_length() {
    // `lognot` sets all 64 bits of the word; the bits past the length must
    // come back clear, or a later `bit-and` would see them.
    let src = "(let ((v (bit-not (BitVector::make 3))))
                 (show (bit-and v (bit-not (BitVector::make 3)))))";
    assert_eq!(eval_ok(&with_helpers(&format!("(equal {} \"111\")", src))), Value::Bool(true));
}

#[test]
fn a_complement_round_trip_is_the_identity() {
    shows("(bit-not (bit-not (bits \"1011\")))", "1011");
}

#[test]
fn the_padding_stays_clear_across_a_word_boundary() {
    // 65 bits: word 2 holds three live bits and 28 padding bits.
    let src = "(let ((v (bit-not (BitVector::make 65))) (n 0))
                 (progn
                   (let ((i 0))
                     (while (< i 65)
                       (progn (setf n (+ n (if (get v i) 1 0))) (setf i (+ i 1)))))
                   n))";
    assert_eq!(eval_ok(&with_helpers(src)), Value::Int(65));
}

#[test]
fn two_bit_vectors_with_the_same_bits_are_the_same_value() {
    // `lognot` sets all 32 bits of a word while only 31 are packed, so
    // without masking every word (not just the last) the complement of an
    // all-zero vector would carry different words from an all-ones vector
    // built bit by bit, while showing the same bits.
    let ones = "\"1111111111111111111111111111111111111111\"";  // 40 bits, two words
    let src = format!("(equalp (bit-not (BitVector::make 40)) (bits {}))", ones);
    assert_eq!(eval_ok(&with_helpers(&src)), Value::Bool(true));
}

#[test]
fn a_bit_wise_operation_leaves_the_words_canonical_too() {
    let src = "(equalp (bit-ior (BitVector::make 40) (bit-not (BitVector::make 40)))
                       (bit-not (BitVector::make 40)))";
    assert_eq!(eval_ok(&with_helpers(src)), Value::Bool(true));
}
