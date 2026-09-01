//! Tests for `read-from-string`'s end position, `read-preserving-whitespace`
//! and `read-delimited-list` — cl-parity-plan.md Phase 8b.
//!
//! CL returns two values from `read-from-string`: the datum, and where
//! reading stopped. This language has no multiple values, so the pair comes
//! back as one `cons-cell` — which is the whole point of the second value,
//! since without it reading a string datum by datum means re-scanning it.
//!
//! The `preserve-whitespace` distinction is only ever observable in one
//! character: CL's `read` consumes the whitespace that terminated the datum
//! and `read-preserving-whitespace` does not. Both forms are tested here
//! against the position (for the string reader) and against the next
//! character (for the stream one), because those are the only two places it
//! shows.

extern crate typelisp;
use typelisp::{load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, EvalError> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> Value {
    run(src).expect("eval failed")
}

fn is_true(src: &str) {
    assert_eq!(eval_ok(src), Value::Bool(true), "{}", src);
}

/// A `peek-stream` over `text`, which is what `read-delimited-list` and
/// `read-sexpr` need (they put the terminating character back).
fn over(text: &str) -> String {
    format!(r#"(make-peek-stream (as :dyn CharInput (make-string-input-stream "{}")))"#, text)
}

// ---- read-from-string's second value ----------------------------------------

#[test]
fn read_from_string_reports_where_it_stopped() {
    is_true(
        r#"(match (read-from-string "12 34")
             ((ok p) (and (equal (car p) (quote 12)) (= (cdr p) 3)))
             ((err e) false))"#,
    );
}

#[test]
fn read_from_string_starts_where_it_is_told() {
    is_true(
        r#"(match (read-from-string "12 34" 3)
             ((ok p) (and (equal (car p) (quote 34)) (= (cdr p) 5)))
             ((err e) false))"#,
    );
}

#[test]
fn the_returned_index_is_where_the_next_read_begins() {
    // The reason the second value exists: reading a string datum by datum is
    // a loop over the index, not a re-scan.
    is_true(
        r#"(let ((s "1 2 3") (i (the i32 0)) (n (the i32 0)) (going true))
             (progn
               (while going
                 (match (read-from-string s i)
                   ((ok p) (progn (setf n (+ n 1)) (setf i (cdr p))
                                  (if (>= i (as i32 (length s))) (progn (setf going false) ()) ()) ()))
                   ((err e) (progn (setf going false) ()))))
               (= n 3)))"#,
    );
}

#[test]
fn preserving_whitespace_stops_one_character_earlier() {
    is_true(
        r#"(match (read-from-string-preserving-whitespace "12 34")
             ((ok p) (= (cdr p) 2))
             ((err e) false))"#,
    );
}

#[test]
fn the_two_forms_read_the_same_datum() {
    is_true(
        r#"(match (read-from-string "12 34")
             ((ok a) (match (read-from-string-preserving-whitespace "12 34")
                       ((ok b) (equal (car a) (car b)))
                       ((err e) false)))
             ((err e) false))"#,
    );
}

#[test]
fn only_whitespace_is_consumed_past_the_datum() {
    // A `)` terminated the datum but belongs to what comes next, so the index
    // stops before it in both forms.
    is_true(
        r#"(match (read-from-string "(1)x" 1)
             ((ok p) (= (cdr p) 2))
             ((err e) false))"#,
    );
}

#[test]
fn a_start_past_the_end_is_an_err_not_a_panic() {
    is_true(
        r#"(match (read-from-string "12" 99)
             ((ok p) false)
             ((err e) true))"#,
    );
}

#[test]
fn a_compound_datum_reports_the_index_past_its_close() {
    is_true(
        r#"(match (read-from-string "(1 2) tail")
             ((ok p) (= (cdr p) 6))
             ((err e) false))"#,
    );
}

#[test]
fn unreadable_text_is_an_err() {
    is_true(
        r#"(match (read-from-string "(1 2")
             ((ok p) false)
             ((err e) true))"#,
    );
}

// ---- the stream reader's whitespace rule -------------------------------------

#[test]
fn read_sexpr_consumes_the_whitespace_that_ended_the_datum() {
    is_true(
        r#"(let ((s (make-string-input-stream "1 2")))
             (progn (read-sexpr s) (equalp (read-char s) (option::some #\2))))"#,
    );
}

#[test]
fn read_sexpr_preserving_whitespace_leaves_it() {
    is_true(
        r#"(let ((s (make-string-input-stream "1 2")))
             (progn (read-sexpr-preserving-whitespace s)
                    (equalp (read-char s) (option::some #\space))))"#,
    );
}

#[test]
fn only_one_whitespace_character_is_consumed() {
    is_true(
        r#"(let ((s (make-string-input-stream "1  2")))
             (progn (read-sexpr s) (equalp (read-char s) (option::some #\space))))"#,
    );
}

#[test]
fn a_non_whitespace_terminator_is_left_alone() {
    is_true(
        r#"(let ((s (make-string-input-stream "1)2")))
             (progn (read-sexpr s) (equalp (read-char s) (option::some #\)))))"#,
    );
}

#[test]
fn both_stream_forms_read_the_same_datum() {
    is_true(
        r#"(let ((a (make-string-input-stream "42 x")) (b (make-string-input-stream "42 x")))
             (match (read-sexpr a)
               ((ok x) (match (read-sexpr-preserving-whitespace b)
                         ((ok y) (equalp x y))
                         ((err e) false)))
               ((err e) false)))"#,
    );
}

// ---- read-delimited-list -----------------------------------------------------

#[test]
fn a_delimited_list_stops_at_its_terminator() {
    is_true(&format!(
        r#"(let ((s {}))
             (match (read-delimited-list #\] s)
               ((ok l) (equal l (quote (1 2 3))))
               ((err e) false)))"#,
        over("1 2 3] rest")
    ));
}

#[test]
fn the_terminator_is_consumed() {
    is_true(&format!(
        r#"(let ((s {}))
             (progn (read-delimited-list #\] s) (equalp (read-char s) (option::some #\x))))"#,
        over("1]x")
    ));
}

#[test]
fn a_delimited_list_reads_compound_data() {
    is_true(&format!(
        r#"(let ((s {}))
             (match (read-delimited-list #\] s)
               ((ok l) (equal l (quote (1 (2 3) "s"))))
               ((err e) false)))"#,
        over(r#"1 (2 3) \"s\"]"#)
    ));
}

#[test]
fn an_empty_delimited_list_is_the_empty_list() {
    is_true(&format!(
        r#"(let ((s {}))
             (match (read-delimited-list #\] s)
               ((ok l) (sexpr-null l))
               ((err e) false)))"#,
        over("]")
    ));
}

#[test]
fn whitespace_before_the_terminator_is_skipped() {
    is_true(&format!(
        r#"(let ((s {}))
             (match (read-delimited-list #\] s)
               ((ok l) (equal l (quote (1))))
               ((err e) false)))"#,
        over("1   ]")
    ));
}

#[test]
fn running_out_of_input_is_an_err_not_a_short_list() {
    // CL signals here too: a missing terminator is a mistake, and returning
    // what was read so far would hide it.
    is_true(&format!(
        r#"(let ((s {}))
             (match (read-delimited-list #\] s)
               ((ok l) false)
               ((err e) true)))"#,
        over("1 2")
    ));
}

#[test]
fn the_error_names_the_terminator_it_wanted() {
    is_true(&format!(
        r#"(let ((s {}))
             (match (read-delimited-list #\] s)
               ((ok l) false)
               ((err e) (is-some (search (message e) "]")))))"#,
        over("1 2")
    ));
}

#[test]
fn a_different_terminator_works_the_same() {
    is_true(&format!(
        r#"(let ((s {}))
             (match (read-delimited-list #\> s)
               ((ok l) (equal l (quote (a b))))
               ((err e) false)))"#,
        over("a b>")
    ));
}
