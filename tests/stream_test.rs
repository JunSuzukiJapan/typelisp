//! Tests for the trait-based stream subsystem.
//!
//! The design's claim is that direction and element type are *static* —
//! `CharInput`/`CharOutput` are traits, not a run-time tag on a single
//! `stream` type — and that composite streams therefore need no support from
//! the native layer at all. These tests exercise both halves: the leaf
//! streams that do touch the OS, and the composites that are ordinary
//! typelisp structs over `:dyn CharOutput`.
//!
//! Every test writes to a string stream or its own temporary file, never to
//! the process's stdout, so the suite is safe to run in parallel.

extern crate typelisp;

mod common;
use common::{eval_string};
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<Value, String> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).map_err(|e| format!("{:?}", e))?;
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| format!("{:?}", e))?;
        if let Some(val) = interp.exec(&mut h, tl).map_err(|e| format!("{:?}", e))? {
            last = val;
        }
    }
    Ok(last)
}

/// A directory that removes itself, so a failing test cannot leave files
/// behind and two tests can never collide on a name.
struct TmpDir(std::path::PathBuf);

impl TmpDir {
    fn new(tag: &str) -> TmpDir {
        let mut p = std::env::temp_dir();
        p.push(format!("typelisp-stream-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("create temp dir");
        TmpDir(p)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().replace('\\', "/")
    }
}

impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---- string input -------------------------------------------------------

#[test]
fn read_char_yields_characters_then_none() {
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "ab")))
             (let ((a (read-char s)) (b (read-char s)) (c (read-char s)))
               (format false "~a~a~a" a b c)))"#,
    );
    assert_eq!(v, "(some a)(some b)none");
}

#[test]
fn read_line_splits_on_newlines_and_keeps_a_final_unterminated_line() {
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "one
two")))
             (format false "~a|~a|~a" (read-line s) (read-line s) (read-line s)))"#,
    );
    assert_eq!(v, "(some one)|(some two)|none");
}

#[test]
fn read_all_returns_everything_left() {
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "abcd")))
             (read-char s)
             (read-all s))"#,
    );
    assert_eq!(v, "bcd");
}

#[test]
fn a_default_method_is_inherited_by_every_stream_type() {
    // `read-all` has one body, in `CharInput`; no stream type implements it.
    let v = eval_string(
        r#"(let ((a (make-string-input-stream "xy"))
                 (b (make-string-input-stream "z")))
             (append (read-all a) (read-all b)))"#,
    );
    assert_eq!(v, "xyz");
}

// ---- string output ------------------------------------------------------

#[test]
fn writes_accumulate_and_are_drained_by_get_output_stream_string() {
    let v = eval_string(
        r#"(let ((s (make-string-output-stream)))
             (write-string s "ab")
             (write-char s #\c)
             (write-line s "!")
             (get-output-stream-string s))"#,
    );
    assert_eq!(v, "abc!\n");
}

#[test]
fn draining_a_string_output_stream_clears_it() {
    let v = eval_string(
        r#"(let ((s (make-string-output-stream)))
             (write-string s "first")
             (let ((a (get-output-stream-string s)))
               (write-string s "second")
               (append a (append "/" (get-output-stream-string s)))))"#,
    );
    assert_eq!(v, "first/second");
}

#[test]
fn with_output_to_string_returns_what_was_written() {
    let v = eval_string(r#"(with-output-to-string (o) (write-string o "hi") (terpri o))"#);
    assert_eq!(v, "hi\n");
}

#[test]
fn with_input_from_string_binds_a_readable_stream() {
    let v = eval_string(r#"(with-input-from-string (i "abc") (read-all i))"#);
    assert_eq!(v, "abc");
}

// ---- composite streams --------------------------------------------------

#[test]
fn a_broadcast_stream_writes_to_every_component() {
    // The payoff: `broadcast-stream` is a typelisp struct over
    // `Vector<:dyn CharOutput>` and needs nothing from the native layer.
    let v = eval_string(
        r#"(let ((a (make-string-output-stream))
                 (b (make-string-output-stream))
                 (v (the Vector<:dyn CharOutput> (Vector::new))))
             (push v a)
             (push v b)
             (let ((bc (make-broadcast-stream v)))
               (write-line bc "both"))
             (append (get-output-stream-string a) (get-output-stream-string b)))"#,
    );
    assert_eq!(v, "both\nboth\n");
}

#[test]
fn a_concatenated_stream_runs_through_its_parts_in_order() {
    let v = eval_string(
        r#"(let ((v (the Vector<:dyn CharInput> (Vector::new))))
             (push v (make-string-input-stream "ab"))
             (push v (make-string-input-stream ""))
             (push v (make-string-input-stream "cd"))
             (read-all (make-concatenated-stream v)))"#,
    );
    assert_eq!(v, "abcd");
}

#[test]
fn an_echo_stream_copies_what_is_read_to_its_output() {
    let v = eval_string(
        r#"(let ((src (make-string-input-stream "log"))
                 (sink (make-string-output-stream)))
             (let ((e (make-echo-stream src sink)))
               (read-all e))
             (get-output-stream-string sink))"#,
    );
    assert_eq!(v, "log");
}

#[test]
fn a_two_way_stream_reads_from_one_side_and_writes_to_the_other() {
    let v = eval_string(
        r#"(let ((src (make-string-input-stream "in"))
                 (sink (make-string-output-stream)))
             (let ((tw (make-two-way-stream src sink)))
               (write-string tw (read-all tw)))
             (get-output-stream-string sink))"#,
    );
    assert_eq!(v, "in");
}

#[test]
fn composites_nest() {
    // A broadcast over another broadcast: nothing special is needed, because
    // a composite is just another `:dyn CharOutput`.
    let v = eval_string(
        r#"(let ((a (make-string-output-stream))
                 (b (make-string-output-stream))
                 (inner (the Vector<:dyn CharOutput> (Vector::new)))
                 (outer (the Vector<:dyn CharOutput> (Vector::new))))
             (push inner a)
             (push inner b)
             (push outer (make-broadcast-stream inner))
             (let ((bc (make-broadcast-stream outer)))
               (write-string bc "x"))
             (append (get-output-stream-string a) (get-output-stream-string b)))"#,
    );
    assert_eq!(v, "xx");
}

// ---- generic code over the traits ---------------------------------------

#[test]
fn copy_stream_works_between_any_input_and_any_output() {
    let v = eval_string(
        r#"(let ((src (make-string-input-stream "payload"))
                 (dst (make-string-output-stream)))
             (copy-stream src dst)
             (get-output-stream-string dst))"#,
    );
    assert_eq!(v, "payload");
}

#[test]
fn read_lines_collects_every_line() {
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "a
b
c")))
             (let ((out ""))
               (doiter (l (iter (read-lines s))) (setf out (append out l)))
               out))"#,
    );
    assert_eq!(v, "abc");
}

#[test]
fn a_user_defined_type_can_be_a_stream() {
    // The other half of the trait claim: nothing about being a stream is
    // reserved to the built-in types.
    let v = eval_string(
        r#"(defstruct counter (n int))
           (impl Stream counter
             (open-stream-p ((self Self)) bool true)
             (close ((self Self)) () ()))
           (impl OutputStream counter
             (type Item char)
             (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
           (impl CharOutput counter)
           (let ((c (counter::new 0)))
             (write-line c "four")
             (format false "~a" c::n))"#,
    );
    // "four" plus the newline `write-line`'s default body appends.
    assert_eq!(v, "5");
}

#[test]
fn a_user_stream_can_go_into_a_broadcast_alongside_a_builtin_one() {
    let v = eval_string(
        r#"(defstruct counter (n int))
           (impl Stream counter
             (open-stream-p ((self Self)) bool true)
             (close ((self Self)) () ()))
           (impl OutputStream counter
             (type Item char)
             (write-item ((self Self) (c char)) () (setf self::n (+ self::n 1))))
           (impl CharOutput counter)
           (let ((c (counter::new 0))
                 (s (make-string-output-stream))
                 (v (the Vector<:dyn CharOutput> (Vector::new))))
             (push v c)
             (push v s)
             (write-string (make-broadcast-stream v) "abc")
             (format false "~a~a" c::n (get-output-stream-string s)))"#,
    );
    assert_eq!(v, "3abc");
}

// ---- files --------------------------------------------------------------

#[test]
fn a_file_round_trips_through_write_and_read() {
    let d = TmpDir::new("roundtrip");
    let p = d.path("a.txt");
    let v = eval_string(&format!(
        r#"(match (write-file-string "{p}" "contents")
             ((ok _) (match (read-file-string "{p}") ((ok s) s) ((err e) (message e))))
             ((err e) (message e)))"#
    ));
    assert_eq!(v, "contents");
}

#[test]
fn read_file_lines_splits_the_file() {
    let d = TmpDir::new("lines");
    let p = d.path("b.txt");
    std::fs::write(&p, "x\ny\n").unwrap();
    let v = eval_string(&format!(
        r#"(match (read-file-lines "{p}")
             ((ok ls) (let ((out "")) (doiter (l (iter ls)) (setf out (append out l))) out))
             ((err e) (message e)))"#
    ));
    assert_eq!(v, "xy");
}

#[test]
fn with_open_file_closes_and_yields_the_bodys_value() {
    let d = TmpDir::new("withopen");
    let p = d.path("c.txt");
    std::fs::write(&p, "data").unwrap();
    let v = eval_string(&format!(
        r#"(match (with-open-file (f "{p}" direction-input) (read-all f))
             ((ok s) s)
             ((err e) (message e)))"#
    ));
    assert_eq!(v, "data");
}

#[test]
fn with_open_file_types_itself_where_nothing_expects_a_type() {
    // The expansion's `ok` arm is a bare `(result::ok ...)`, whose `FileError`
    // only the sibling `err` arm knows. Bound to a `let` there is no expected
    // type to supply it either, so this checks only because the two arms pool
    // what they know (`Checker::check_match`'s probe) — before that, the
    // prelude had to route the arm through an `io-ok` helper whose declared
    // return type pinned `E`.
    let d = TmpDir::new("withopenlet");
    let p = d.path("c2.txt");
    std::fs::write(&p, "data").unwrap();
    let v = eval_string(&format!(
        r#"(let ((r (with-open-file (f "{p}" direction-input) (read-all f))))
             (match r ((ok s) s) ((err e) (message e))))"#
    ));
    assert_eq!(v, "data");
}

#[test]
fn opening_a_missing_file_is_an_error_value_not_a_panic() {
    let d = TmpDir::new("missing");
    let p = d.path("nope.txt");
    let v = eval_string(&format!(
        r#"(match (open-input "{p}") ((ok _) "opened") ((err e) (message e)))"#
    ));
    let m = v;
    assert!(m.contains("nope.txt"), "{}", m);
}

#[test]
fn appending_adds_to_an_existing_file() {
    let d = TmpDir::new("append");
    let p = d.path("d.txt");
    std::fs::write(&p, "one").unwrap();
    let v = eval_string(&format!(
        r#"(match (with-open-file (f "{p}" direction-append) (write-string f "two"))
             ((ok _) (match (read-file-string "{p}") ((ok s) s) ((err e) (message e))))
             ((err e) (message e)))"#
    ));
    assert_eq!(v, "onetwo");
}

#[test]
fn probe_and_delete_report_and_remove() {
    let d = TmpDir::new("probe");
    let p = d.path("e.txt");
    std::fs::write(&p, "x").unwrap();
    let v = eval_string(&format!(
        r#"(let ((before (probe-file "{p}")))
             (delete-file "{p}")
             (format false "~a~a" before (probe-file "{p}")))"#
    ));
    assert_eq!(v, "truefalse");
}

// ---- lifetime -----------------------------------------------------------

#[test]
fn close_makes_a_stream_report_itself_closed() {
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "x")))
             (let ((before (open-stream-p s)))
               (close s)
               (format false "~a~a" before (open-stream-p s))))"#,
    );
    assert_eq!(v, "truefalse");
}

#[test]
fn a_string_output_stream_can_still_be_drained_after_close() {
    // CL allows `get-output-stream-string` after `close`; the text is the
    // stream's value, not a resource the close released.
    let v = eval_string(
        r#"(let ((s (make-string-output-stream)))
             (write-string s "kept")
             (close s)
             (get-output-stream-string s))"#,
    );
    assert_eq!(v, "kept");
}

// ---- `Result<(), FileError>` --------------------------------------------
//
// The three effect-only file functions return `Ok(())`, not the meaningless
// `Ok(true)` they carried while `Result<(), E>` was unwritable. Binding the
// payload (rather than discarding it with `_`) is what pins the type down:
// these would not compile against a `Result<bool, _>`.

#[test]
fn write_file_string_succeeds_with_a_unit_payload() {
    let d = TmpDir::new("unitwrite");
    let p = d.path("u.txt");
    let v = eval_string(&format!(
        r#"(match (write-file-string "{p}" "contents")
             ((ok u) (progn u (match (read-file-string "{p}") ((ok s) s) ((err e) (message e)))))
             ((err e) (message e)))"#
    ));
    assert_eq!(v, "contents");
}

#[test]
fn delete_and_rename_succeed_with_a_unit_payload() {
    let d = TmpDir::new("unitdelete");
    let from = d.path("from.txt");
    let to = d.path("to.txt");
    std::fs::write(&from, "x").unwrap();
    let v = eval_string(&format!(
        r#"(match (rename-file "{from}" "{to}")
             ((ok u) (progn u
               (match (delete-file "{to}")
                 ((ok u2) (progn u2 (format false "~a~a" (probe-file "{from}") (probe-file "{to}"))))
                 ((err e) (message e)))))
             ((err e) (message e)))"#
    ));
    assert_eq!(v, "falsefalse");
}

// ---- fresh-line ---------------------------------------------------------

#[test]
fn fresh_line_writes_a_newline_only_when_one_is_needed() {
    let v = eval_string(
        r#"(let ((s (make-string-output-stream)))
             (write-string s "a")
             (fresh-line s)
             (fresh-line s)
             (write-string s "b")
             (get-output-stream-string s))"#,
    );
    assert_eq!(v, "a\nb");
}

#[test]
fn fresh_line_on_an_untouched_stream_writes_nothing() {
    let v = eval_string(
        r#"(let ((s (make-string-output-stream)))
             (fresh-line s)
             (format false "~d" (length (get-output-stream-string s))))"#,
    );
    assert_eq!(v, "0");
}

#[test]
fn a_stream_that_cannot_tell_gets_its_newline() {
    // The `at-line-start` default is `false`, so `fresh-line` on a stream
    // with no memory of its own always writes — the safe direction.
    let v = eval_string(
        r#"(defstruct sink (text string))
           (impl Stream sink
             (open-stream-p ((self Self)) bool true)
             (close ((self Self)) () ()))
           (impl OutputStream sink
             (type Item char)
             (write-item ((self Self) (c char)) () (setf self::text (append self::text (char->string c)))))
           (impl CharOutput sink)
           (let ((k (sink::new "")))
             (fresh-line k)
             (fresh-line k)
             (format false "~d" (length k::text)))"#,
    );
    assert_eq!(v, "2");
}

#[test]
fn a_two_way_stream_takes_its_line_position_from_its_output_side() {
    let v = eval_string(
        r#"(let ((out (make-string-output-stream)))
             (let ((tw (make-two-way-stream (make-string-input-stream "") out)))
               (write-string tw "x")
               (fresh-line tw)
               (fresh-line tw)
               (get-output-stream-string out)))"#,
    );
    assert_eq!(v, "x\n");
}

#[test]
fn a_broadcast_stream_asks_each_component_for_itself() {
    // `a` is mid-line and `b` is not, so exactly one of them gets a newline.
    let v = eval_string(
        r#"(let ((a (make-string-output-stream))
                 (b (make-string-output-stream))
                 (v (the Vector<:dyn CharOutput> (Vector::new))))
             (write-string a "mid")
             (push v a)
             (push v b)
             (fresh-line (make-broadcast-stream v))
             (format false "~s ~s" (get-output-stream-string a) (get-output-stream-string b)))"#,
    );
    assert_eq!(v, r#""mid\n" """#);
}

// ---- format to a stream -------------------------------------------------

#[test]
fn format_writes_to_a_stream_destination() {
    let v = eval_string(
        r#"(let ((s (make-string-output-stream)))
             (format s "id=~d name=~a" 42 "x")
             (get-output-stream-string s))"#,
    );
    assert_eq!(v, "id=42 name=x");
}

#[test]
fn format_to_a_stream_works_through_a_trait_bound_and_a_trait_object() {
    let v = eval_string(
        r#"(defun via-bound<S> ((s S)) () (where (CharOutput S)) (format s "[~d]" 1))
           (defun via-dyn ((s :dyn CharOutput)) () (format s "{~a}" true))
           (let ((s (make-string-output-stream)))
             (via-bound s)
             (via-dyn s)
             (get-output-stream-string s))"#,
    );
    assert_eq!(v, "[1]{true}");
}

#[test]
fn a_bool_destination_still_yields_the_string() {
    let v = eval_string(r#"(format false "~d-~d" 1 2)"#);
    assert_eq!(v, "1-2");
}

#[test]
fn a_destination_that_is_neither_bool_nor_a_stream_is_rejected() {
    let e = run(r#"(format 3 "~d" 1)"#).expect_err("a number is not a destination");
    assert!(e.contains("destination"), "unexpected error: {}", e);
}

#[test]
fn format_to_a_file_stream_goes_to_the_file() {
    let d = TmpDir::new("formatfile");
    let p = d.path("f.txt");
    let v = eval_string(&format!(
        r#"(match (open-output "{p}")
             ((ok f) (progn (format f "~d~%" 7) (close f)
                            (match (read-file-string "{p}") ((ok s) s) ((err e) (message e)))))
             ((err e) (message e)))"#
    ));
    assert_eq!(v, "7\n");
}

// ---- read over a stream -------------------------------------------------

#[test]
fn read_sexpr_reads_one_datum_at_a_time() {
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "(+ 1 2) foo \"bar\" 42"))
                 (out ""))
             (loop
               (match (read-sexpr s)
                 ((ok o) (match o ((eof) (break)) ((datum d) (setf out (append out (format false "~s|" d))))))
                 ((err e) (progn (setf out (append out (message e))) (break)))))
             out)"#,
    );
    assert_eq!(v, "(+ 1 2)|foo|\"bar\"|42|");
}

#[test]
fn read_sexpr_consumes_the_datum_and_the_whitespace_that_ended_it() {
    // CL's `read` consumes the delimiting character when it is whitespace —
    // which is what makes a form typed at a terminal take its newline with
    // it. This used to keep the space (it was `read-preserving-whitespace`'s
    // behaviour under `read`'s name); cl-parity-plan.md Phase 8b split the
    // two, and the preserving one is tested just below.
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "(a b) rest")))
             (read-sexpr s)
             (read-all s))"#,
    );
    assert_eq!(v, "rest");
}

#[test]
fn read_sexpr_preserving_whitespace_leaves_what_ended_the_datum() {
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "(a b) rest")))
             (read-sexpr-preserving-whitespace s)
             (read-all s))"#,
    );
    assert_eq!(v, " rest");
}

#[test]
fn read_sexpr_leaves_a_non_whitespace_terminator_alone() {
    // Only whitespace is consumed: a `)` that ended the datum belongs to
    // whatever comes next.
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "a)b")))
             (read-sexpr s)
             (read-all s))"#,
    );
    assert_eq!(v, ")b");
}

#[test]
fn read_sexpr_handles_quotes_comments_strings_and_character_literals() {
    let v = eval_string(
        r##"(defun one ((text string)) string
             (let ((s (make-string-input-stream text)))
               (match (read-sexpr s)
                 ((ok o) (match o ((eof) "<eof>") ((datum d) (format false "~s" d))))
                 ((err e) (message e)))))
           (format false "~a ~a ~a ~a ~a"
             (one "'(a b)")
             (one "`(c ,d ,@e)")
             (one "; skipped
                   #| also #| nested |# skipped |# 7")
             (one "\"a)b\"")
             (one "#\\("))"##,
    );
    assert_eq!(
        v,
        r##"(quote (a b)) (quasiquote (c (unquote d) (unquote-splicing e))) 7 "a)b" #\("##
    );
}

#[test]
fn an_atom_ends_at_a_paren_without_losing_it() {
    // The pushback is the whole reason `read-sexpr` wants `PeekInput`: the
    // `(` that ends `1` has to still be there for the next read.
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "1(2 3)"))
                 (out ""))
             (loop
               (match (read-sexpr s)
                 ((ok o) (match o ((eof) (break)) ((datum d) (setf out (append out (format false "~s|" d))))))
                 ((err e) (break))))
             out)"#,
    );
    assert_eq!(v, "1|(2 3)|");
}

#[test]
fn read_sexpr_reports_an_incomplete_datum_as_an_error() {
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "(1 2")))
             (match (read-sexpr s)
               ((ok _) "no error")
               ((err e) (message e))))"#,
    );
    assert!(v.contains("end of input"));
}

#[test]
fn end_of_input_is_a_value_not_an_error() {
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "   ")))
             (match (read-sexpr s)
               ((ok o) (match o ((eof) "eof") ((datum _) "datum")))
               ((err e) (message e))))"#,
    );
    assert_eq!(v, "eof");
}

#[test]
fn read_sexpr_reads_a_file_form_by_form() {
    let d = TmpDir::new("readfile");
    let p = d.path("forms.typl");
    std::fs::write(&p, "(one 1)\n; a comment\n(two 2)\n").unwrap();
    let v = eval_string(&format!(
        r#"(match (open-input "{p}")
             ((ok f)
              (let ((out ""))
                (loop
                  (match (read-sexpr f)
                    ((ok o) (match o ((eof) (break)) ((datum d) (setf out (append out (format false "~s" d))))))
                    ((err e) (break))))
                (close f)
                out))
             ((err e) (message e)))"#
    ));
    assert_eq!(v, "(one 1)(two 2)");
}

#[test]
fn peek_char_looks_without_consuming_and_unread_char_puts_back() {
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "xyz")))
             (let ((p (peek-char s)) (r (read-char s)))
               (unread-char s #\Q)
               (format false "~a~a~a" p r (read-all s))))"#,
    );
    assert_eq!(v, "(some x)(some x)Qyz");
}

#[test]
fn a_string_stream_that_unread_more_than_it_read_has_no_position() {
    // One character read, two pushed back: the position would be -1, which
    // is no position at all, so the answer is an error rather than 0.
    let v = eval_string(
        r#"(let ((s (make-string-input-stream "xyz")))
             (read-char s)
             (unread-char s #\x)
             (let ((one (stream-position s::h)))
               (unread-char s #\w)
               (format false "~a ~a" (unwrap one) (is-err (stream-position s::h)))))"#,
    );
    assert_eq!(v, "0 true");
}

#[test]
fn a_stream_without_pushback_gets_it_by_wrapping() {
    // A composite has no pushback of its own; `peek-stream` adds it, which is
    // what lets `read-sexpr` run over one.
    let v = eval_string(
        r#"(let ((parts (the Vector<:dyn CharInput> (Vector::new))))
             (push parts (make-string-input-stream "(1 2)"))
             (push parts (make-string-input-stream " 3"))
             (let ((p (make-peek-stream (make-concatenated-stream parts)))
                   (out ""))
               (loop
                 (match (read-sexpr p)
                   ((ok o) (match o ((eof) (break)) ((datum d) (setf out (append out (format false "~s|" d))))))
                   ((err e) (break))))
               out))"#,
    );
    assert_eq!(v, "(1 2)|3|");
}

#[test]
fn a_user_defined_input_stream_can_be_read_from() {
    let v = eval_string(
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
           (let ((p (make-peek-stream (fixed::new "(from a user stream)" 0))))
             (match (read-sexpr p)
               ((ok o) (match o ((eof) "<eof>") ((datum d) (format false "~s" d))))
               ((err e) (message e))))"#,
    );
    assert_eq!(v, "(from a user stream)");
}
