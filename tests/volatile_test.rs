//! `volatile-read-u8`/`-u16`/`-u32` and `volatile-write-*`: reading and
//! writing a register at an address, inside `(unsafe ...)`.
//!
//! On the host there is no register to touch, so the address is memory
//! `malloc` gave: what is written is read back, by the interpreter and by
//! compiled code. A board's registers are touched in `aot_embedded_test`.

extern crate typelisp;
mod common;
use common::{run_compiled_display_err as eval};
use typelisp::Value;

fn int(src: &str) -> i64 {
    match eval(src).expect("evaluation failed").1 {
        Value::Int(n) => n,
        other => panic!("expected an integer, got {:?}", other),
    }
}

fn err(src: &str) -> String {
    match eval(src) {
        Err(e) => e,
        Ok(_) => panic!("expected a failure, but the program ran"),
    }
}

const MEMORY: &str = r#"
(defffi (c-malloc "malloc") (c-ulong) c-ulong)
(defffi (c-free "free") (c-ulong) ())
"#;

/// Each width written, then read back, from 8 bytes `malloc` gave.
const ROUND_TRIP: &str = r#"
(defun round-trip () int
  (unsafe
    (let* ((block (c-malloc 8))
           (at (as int block)))
      (volatile-write-u32 at #x11223344)
      (volatile-write-u16 (+ at 4) #xbeef)
      (volatile-write-u8 (+ at 6) #x7f)
      (let ((answer (+ (as int (volatile-read-u32 at))
                       (* (as int (volatile-read-u16 (+ at 4))) #x100000000)
                       (* (as int (volatile-read-u8 (+ at 6))) #x1000000000000))))
        (c-free block)
        answer))))
"#;

const ROUND_TRIP_ANSWER: i64 = 0x11223344 + 0xbeef * 0x1_0000_0000 + 0x7f * 0x1_0000_0000_0000;

#[test]
fn what_is_written_is_read_back() {
    assert_eq!(int(&format!("{}{}(round-trip)", MEMORY, ROUND_TRIP)), ROUND_TRIP_ANSWER);
}

#[test]
fn compiled_code_reads_back_what_it_wrote() {
    assert_eq!(int(&format!("{}{}(compile round-trip)(round-trip)", MEMORY, ROUND_TRIP)), ROUND_TRIP_ANSWER);
}

/// A narrower read sees part of the wider word, in the machine's byte order.
#[test]
fn a_narrow_read_sees_part_of_a_wide_write() {
    let src = r#"
(defun low-byte () int
  (unsafe
    (let* ((block (c-malloc 4)) (at (as int block)))
      (volatile-write-u32 at #x11223344)
      (let ((low (as int (volatile-read-u8 at)))) (c-free block) low))))
"#;
    let low = if cfg!(target_endian = "little") { 0x44 } else { 0x11 };
    assert_eq!(int(&format!("{}{}(low-byte)", MEMORY, src)), low);
    assert_eq!(int(&format!("{}{}(compile low-byte)(low-byte)", MEMORY, src)), low);
}

#[test]
fn a_read_outside_unsafe_is_a_type_error() {
    let e = err("(defun f () u32 (volatile-read-u32 #x40004000))");
    assert!(e.contains("`volatile-read-u32`") && e.contains("needs `(unsafe ...)`"), "unexpected error: {}", e);
}

#[test]
fn a_write_outside_unsafe_is_a_type_error() {
    let e = err("(defun f () () (volatile-write-u8 #x40004000 1))");
    assert!(e.contains("`volatile-write-u8`") && e.contains("needs `(unsafe ...)`"), "unexpected error: {}", e);
}

#[test]
fn the_value_written_has_the_register_s_width() {
    let e = err(r#"(defun f () () (unsafe (volatile-write-u16 #x40004000 "x")))"#);
    assert!(e.contains("u16"), "unexpected error: {}", e);
    let e = err("(defun f () () (unsafe (volatile-write-u8 #x40004000 256)))");
    assert!(e.contains("256"), "unexpected error: {}", e);
}

#[test]
fn the_forms_take_their_arguments() {
    let e = err("(defun f () u32 (unsafe (volatile-read-u32)))");
    assert!(e.contains("(volatile-read-u32 address)"), "unexpected error: {}", e);
    let e = err("(defun f () () (unsafe (volatile-write-u32 #x40004000)))");
    assert!(e.contains("(volatile-write-u32 address value)"), "unexpected error: {}", e);
}

/// A misaligned address is refused before anything is read, by the
/// interpreter and by compiled code alike.
#[test]
fn a_misaligned_address_is_refused() {
    let src = "(defun f () u32 (unsafe (volatile-read-u32 #x40004002)))";
    for run in ["(f)", "(compile f)(f)"] {
        let e = err(&format!("{}{}", src, run));
        assert!(e.contains("0x40004002 is not aligned to 4 bytes"), "{}: unexpected error: {}", run, e);
    }
    let e = err("(defun g () () (unsafe (volatile-write-u16 #x40004001 1)))(g)");
    assert!(e.contains("0x40004001 is not aligned to 2 bytes"), "unexpected error: {}", e);
}

#[test]
fn a_negative_address_is_refused() {
    let src = "(defun f () u8 (unsafe (volatile-read-u8 -8)))";
    for run in ["(f)", "(compile f)(f)"] {
        let e = err(&format!("{}{}", src, run));
        assert!(e.contains("is not an address on this machine"), "{}: unexpected error: {}", run, e);
    }
}
