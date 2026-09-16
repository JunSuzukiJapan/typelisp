//! Stage 9d — the stream operations CL has and this had not: `listen`,
//! `read-char-no-hang`, and the bulk transfers `read-sequence` /
//! `write-sequence`.
//!
//! Where they sit is the interesting part. `listen` is on `InputStream`,
//! because readiness is about items and says nothing about what an item is.
//! The bulk transfers are one layer down, on `CharInput`/`CharOutput` and
//! `ByteInput`/`ByteOutput`, because they need to name a `Vector` **of the
//! item type** — and the item type is settled exactly there, which is what
//! those layers exist for. Declaring them on `InputStream` as
//! `Vector<Item>` does not work: a generic name is one symbol to the reader,
//! so the associated-type substitution an `impl` performs does not reach
//! inside `Vector<Item>`, and the inherited default keeps the unsubstituted
//! signature. See docs/dev/cl-parity-plan.md Stage 9d.

use typelisp::*;

/// Check and run one expression against a fresh session, as a string.
fn eval_string(src: &str) -> String {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);

    let forms = reader.read_all(&mut heap, src).expect("read failed");
    let mut last = None;
    for v in forms {
        let tl = checker.check_form(&mut heap, &interp, v).expect("check failed");
        last = interp.exec(&mut heap, tl).expect("exec failed");
    }
    match last {
        Some(Value::Str(id)) => heap.string(id).to_string(),
        other => format!("{:?}", other),
    }
}

// ----------------------------------------------------------------------
// listen / read-char-no-hang
// ----------------------------------------------------------------------

#[test]
fn a_string_stream_is_ready_until_it_is_empty() {
    assert_eq!(
        eval_string(
            r#"(let ((s (make-string-input-stream "ab")))
                 (let ((a (listen s)))
                   (read-char s)
                   (read-char s)
                   (format false "~a ~a" a (listen s))))"#
        ),
        "true false"
    );
}

#[test]
fn read_char_no_hang_yields_what_is_there() {
    assert_eq!(
        eval_string(
            r#"(let ((s (make-string-input-stream "x")))
                 (format false "~a ~a" (read-char-no-hang s) (read-char-no-hang s)))"#
        ),
        "(some x) none"
    );
}

#[test]
fn a_pushed_back_character_counts_as_ready() {
    // The peek stream holds it itself, so it can answer without asking what
    // it wraps — and must, or a `read-char-no-hang` after an `unread-char`
    // would report nothing available when a character is in hand.
    assert_eq!(
        eval_string(
            r#"(let ((p (make-peek-stream (make-string-input-stream ""))))
                 (unread-char p #\z)
                 (format false "~a ~a" (listen p) (read-char-no-hang p)))"#
        ),
        "true (some z)"
    );
}

#[test]
fn a_user_stream_that_says_nothing_reports_nothing_ready() {
    // The documented cost of the default: `false` is never a lie, but a
    // stream that does not override it never reports anything ready.
    assert_eq!(
        eval_string(
            r#"(defstruct fixed (text string) (at int))
               (impl Stream fixed
                 (open-stream-p ((self Self)) bool true)
                 (close ((self Self)) () ()))
               (impl InputStream fixed
                 (type Item char)
                 (read-item ((self Self)) Option<char>
                   (if (>= self::at (length self::text))
                       (option::none)
                       (let ((c (ref self::text self::at)))
                         (setf self::at (+ self::at 1))
                         (option::some c)))))
               (impl CharInput fixed)
               (let ((s (fixed::new "abc" 0)))
                 (format false "~a ~a ~a" (listen s) (read-char-no-hang s) (read-char s)))"#
        ),
        "false none (some a)"
    );
}

#[test]
fn a_user_stream_that_answers_is_believed() {
    assert_eq!(
        eval_string(
            r#"(defstruct always (n int))
               (impl Stream always
                 (open-stream-p ((self Self)) bool true)
                 (close ((self Self)) () ()))
               (impl InputStream always
                 (type Item char)
                 (read-item ((self Self)) Option<char> (option::some #\q))
                 (listen ((self Self)) bool true))
               (impl CharInput always)
               (let ((s (always::new 0)))
                 (format false "~a ~a" (listen s) (read-char-no-hang s)))"#
        ),
        "true (some q)"
    );
}

// ----------------------------------------------------------------------
// read-sequence / write-sequence
// ----------------------------------------------------------------------

#[test]
fn read_sequence_appends_up_to_the_count_asked_for() {
    assert_eq!(
        eval_string(
            r#"(let ((s (make-string-input-stream "hello")) (v (the Vector<char> (Vector::new))))
                 (let ((got (read-sequence s v 3)))
                   (format false "~a ~a ~a" got (len v) (get v 0))))"#
        ),
        "3 3 h"
    );
}

#[test]
fn read_sequence_stops_short_at_end_of_input() {
    assert_eq!(
        eval_string(
            r#"(let ((s (make-string-input-stream "hi")) (v (the Vector<char> (Vector::new))))
                 (format false "~a ~a" (read-sequence s v 10) (len v)))"#
        ),
        "2 2"
    );
}

#[test]
fn read_sequence_leaves_the_rest_of_the_stream_where_it_was() {
    assert_eq!(
        eval_string(
            r#"(let ((s (make-string-input-stream "abcd")) (v (the Vector<char> (Vector::new))))
                 (read-sequence s v 2)
                 (format false "~a" (read-char s)))"#
        ),
        "(some c)"
    );
}

#[test]
fn write_sequence_writes_every_item_in_order() {
    assert_eq!(
        eval_string(
            r#"(let ((o (make-string-output-stream)) (v (the Vector<char> (Vector::new))))
                 (push v #\o)
                 (push v #\k)
                 (write-sequence o v)
                 (get-output-stream-string o))"#
        ),
        "ok"
    );
}

#[test]
fn read_sequence_into_an_empty_vector_of_nothing_reads_nothing() {
    assert_eq!(
        eval_string(
            r#"(let ((s (make-string-input-stream "abc")) (v (the Vector<char> (Vector::new))))
                 (format false "~a ~a" (read-sequence s v 0) (len v)))"#
        ),
        "0 0"
    );
}

#[test]
fn the_byte_layer_has_the_same_pair() {
    // `ByteInput`/`ByteOutput` get their own copies rather than inheriting
    // one: the item type is what the signature names, and theirs is `int`.
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("stream-remainder-tmp");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("bytes.bin");
    let path_str = path.to_string_lossy().replace('\\', "/");
    let src = format!(
        r#"(match (open-binary-output "{p}")
             ((ok o) (let ((v (the Vector<int> (Vector::new))))
                       (push v 65)
                       (push v 66)
                       (write-sequence o v)
                       (close o)))
             ((err e) (panic (message e))))
           (match (open-binary-input "{p}")
             ((ok i) (let ((w (the Vector<int> (Vector::new))))
                       (let ((got (read-sequence i w 8)))
                         (close i)
                         (format false "~a ~a ~a" got (get w 0) (get w 1)))))
             ((err e) (message e)))"#,
        p = path_str
    );
    assert_eq!(eval_string(&src), "2 65 66");
}
