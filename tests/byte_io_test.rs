//! Tests for `read-byte`/`write-byte` and the byte stream types —
//! cl-parity-plan.md Phase 8a.
//!
//! CL picks byte I/O with `:element-type '(unsigned-byte 8)` at `open` time,
//! which makes the element type a property of the *call*. Here it is a
//! property of the type: `binary-file-stream` implements `ByteInput`/
//! `ByteOutput`, which pin `InputStream`/`OutputStream`'s open `Item` to
//! `int` the way `CharInput`/`CharOutput` pin it to `char`. So there is no
//! `:element-type` to get wrong, and no way to read bytes from something that
//! is not a byte stream — the two questions this file is mostly about.

extern crate typelisp;

mod common;
use common::{eval_err, is_true};
use typelisp::{load_prelude, Checker, Heap, Interp, Reader};

fn check_err(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    for v in vs {
        if let Err(e) = chk.check_form(&mut h, &interp, v) {
            return format!("{:?}", e);
        }
    }
    panic!("expected a type error");
}

/// A path in the OS temp directory, unique per test so the suite can run
/// them in any order.
fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("typelisp-byte-io-{}-{}.bin", std::process::id(), name));
    let _ = std::fs::remove_file(&p);
    p.to_string_lossy().to_string()
}

// ---- the round trip ---------------------------------------------------------

#[test]
fn bytes_written_come_back_in_order() {
    let path = tmp("order");
    is_true(&format!(
        r#"(defun byte-or ((d int) (o Option<int>)) int (match o ((some b) b) ((none) d)))
           (progn
             (match (open-binary-output "{p}")
               ((ok s) (progn (write-byte s 1) (write-byte s 2) (write-byte s 3) (close s)))
               ((err e) (panic (message e))))
             (match (open-binary-input "{p}")
               ((ok s) (let ((out (the Vector<int> (Vector::new))))
                         (progn (loop (match (read-byte s)
                                        ((none) (break))
                                        ((some b) (push out b))))
                                (close s)
                                (and (= (len out) 3)
                                     (and (= (get out 0) 1)
                                          (and (= (get out 1) 2) (= (get out 2) 3)))))))
               ((err e) (panic (message e)))))"#,
        p = path
    ));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn read_byte_is_none_at_end_of_file() {
    let path = tmp("eof");
    is_true(&format!(
        r#"(defun byte-or ((d int) (o Option<int>)) int (match o ((some b) b) ((none) d)))
           (progn
             (match (open-binary-output "{p}")
               ((ok s) (progn (write-byte s 9) (close s)))
               ((err e) (panic (message e))))
             (match (open-binary-input "{p}")
               ((ok s) (progn (read-byte s) (is-none (read-byte s))))
               ((err e) (panic (message e)))))"#,
        p = path
    ));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn every_byte_value_survives_the_round_trip() {
    // 0 and 255 are the ends, and 128 is past the point where a byte would be
    // a multi-byte character if anything were decoding it.
    let path = tmp("range");
    is_true(&format!(
        r#"(defun byte-or ((d int) (o Option<int>)) int (match o ((some b) b) ((none) d)))
           (progn
             (match (open-binary-output "{p}")
               ((ok s) (progn (write-byte s 0) (write-byte s 128) (write-byte s 255) (close s)))
               ((err e) (panic (message e))))
             (match (open-binary-input "{p}")
               ((ok s) (let ((a (byte-or -1 (read-byte s))) (b (byte-or -1 (read-byte s))) (c (byte-or -1 (read-byte s))))
                         (progn (close s) (and (= a 0) (and (= b 128) (= c 255))))))
               ((err e) (panic (message e)))))"#,
        p = path
    ));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn bytes_and_characters_agree_on_the_same_file() {
    // What `write-byte` puts down is what `read-char` reads back, for the
    // bytes that are ASCII — the two views are of one file, not two encodings.
    let path = tmp("ascii");
    is_true(&format!(
        r#"(defun byte-or ((d int) (o Option<int>)) int (match o ((some b) b) ((none) d)))
           (progn
             (match (open-binary-output "{p}")
               ((ok s) (progn (write-byte s 72) (write-byte s 105) (close s)))
               ((err e) (panic (message e))))
             (match (open-input "{p}")
               ((ok s) (let ((l (match (read-line s) ((some x) x) ((none) "")))) (progn (close s) (equal l "Hi"))))
               ((err e) (panic (message e)))))"#,
        p = path
    ));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn appending_adds_to_what_is_there() {
    let path = tmp("append");
    is_true(&format!(
        r#"(defun byte-or ((d int) (o Option<int>)) int (match o ((some b) b) ((none) d)))
           (progn
             (match (open-binary-output "{p}")
               ((ok s) (progn (write-byte s 1) (close s)))
               ((err e) (panic (message e))))
             (match (open-binary "{p}" direction-append)
               ((ok s) (progn (write-byte s 2) (close s)))
               ((err e) (panic (message e))))
             (match (open-binary-input "{p}")
               ((ok s) (let ((a (byte-or -1 (read-byte s))) (b (byte-or -1 (read-byte s))) (c (read-byte s)))
                         (progn (close s) (and (= a 1) (and (= b 2) (is-none c))))))
               ((err e) (panic (message e)))))"#,
        p = path
    ));
    let _ = std::fs::remove_file(&path);
}

// ---- the boundaries ---------------------------------------------------------

#[test]
fn a_value_above_255_is_not_a_byte() {
    let path = tmp("over");
    let e = eval_err(&format!(
        r#"(match (open-binary-output "{p}")
             ((ok s) (progn (write-byte s 256) (close s)))
             ((err e) (panic (message e))))"#,
        p = path
    ));
    assert!(e.contains("is not a byte"), "{}", e);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_negative_value_is_not_a_byte() {
    let path = tmp("neg");
    let e = eval_err(&format!(
        r#"(match (open-binary-output "{p}")
             ((ok s) (progn (write-byte s -1) (close s)))
             ((err e) (panic (message e))))"#,
        p = path
    ));
    assert!(e.contains("is not a byte"), "{}", e);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_character_stream_is_not_a_byte_stream() {
    // The type says so: `string-input-stream` implements `CharInput`, not
    // `ByteInput`, so this never reaches run time.
    let e = check_err(r#"(read-byte (make-string-input-stream "abc"))"#);
    assert!(!e.is_empty(), "{}", e);
}

#[test]
fn a_byte_stream_is_not_a_character_stream() {
    let path = tmp("notchar");
    let e = check_err(&format!(
        r#"(match (open-binary-input "{p}")
             ((ok s) (progn (read-char s) ()))
             ((err e) ()))"#,
        p = path
    ));
    assert!(!e.is_empty(), "{}", e);
}

#[test]
fn opening_a_missing_file_for_bytes_is_an_ordinary_err() {
    is_true(
        r#"(match (open-binary-input "/nonexistent/typelisp/byte/path")
             ((ok s) (progn (close s) false))
             ((err e) true))"#,
    );
}

// ---- the trait layer --------------------------------------------------------

#[test]
fn a_byte_stream_is_a_stream() {
    let path = tmp("isstream");
    is_true(&format!(
        r#"(match (open-binary-output "{p}")
             ((ok s) (let ((open-before (open-stream-p s)))
                       (progn (close s) (and open-before (not (open-stream-p s))))))
             ((err e) (panic (message e))))"#,
        p = path
    ));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn read_item_is_the_same_operation_as_read_byte() {
    let path = tmp("readitem");
    is_true(&format!(
        r#"(defun byte-or ((d int) (o Option<int>)) int (match o ((some b) b) ((none) d)))
           (progn
             (match (open-binary-output "{p}")
               ((ok s) (progn (write-byte s 5) (close s)))
               ((err e) (panic (message e))))
             (match (open-binary-input "{p}")
               ((ok s) (let ((b (byte-or -1 (read-item s)))) (progn (close s) (= b 5))))
               ((err e) (panic (message e)))))"#,
        p = path
    ));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_generic_function_over_byteinput_accepts_a_binary_file_stream() {
    // The point of the trait layer: a caller writes the bound, not the type.
    let path = tmp("generic");
    is_true(&format!(
        r#"(defun total<S> ((s S)) int (where (ByteInput S))
             (let ((n (the int 0)))
               (progn (loop (match (read-byte s) ((none) (break)) ((some b) (setf n (+ n b))))) n)))
           (progn
             (match (open-binary-output "{p}")
               ((ok s) (progn (write-byte s 10) (write-byte s 20) (close s)))
               ((err e) (panic (message e))))
             (match (open-binary-input "{p}")
               ((ok s) (let ((sum (total s))) (progn (close s) (= sum 30))))
               ((err e) (panic (message e)))))"#,
        p = path
    ));
    let _ = std::fs::remove_file(&path);
}
