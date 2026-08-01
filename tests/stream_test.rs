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
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, RtValue};

fn run(src: &str) -> Result<RtValue, String> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).map_err(|e| format!("{:?}", e))?;
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| format!("{:?}", e))?;
        if let Some(val) = interp.exec(&mut h, tl).map_err(|e| format!("{:?}", e))? {
            last = val;
        }
    }
    Ok(last)
}

fn eval_ok(src: &str) -> RtValue {
    run(src).expect("eval failed")
}

fn str_of(v: RtValue) -> String {
    match v {
        RtValue::Str(s) => s.to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
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
    let v = eval_ok(
        r#"(let ((s (make-string-input-stream "ab")))
             (let ((a (read-char s)) (b (read-char s)) (c (read-char s)))
               (format false "~a~a~a" a b c)))"#,
    );
    assert_eq!(str_of(v), "(some a)(some b)none");
}

#[test]
fn read_line_splits_on_newlines_and_keeps_a_final_unterminated_line() {
    let v = eval_ok(
        r#"(let ((s (make-string-input-stream "one
two")))
             (format false "~a|~a|~a" (read-line s) (read-line s) (read-line s)))"#,
    );
    assert_eq!(str_of(v), "(some one)|(some two)|none");
}

#[test]
fn read_all_returns_everything_left() {
    let v = eval_ok(
        r#"(let ((s (make-string-input-stream "abcd")))
             (read-char s)
             (read-all s))"#,
    );
    assert_eq!(str_of(v), "bcd");
}

#[test]
fn a_default_method_is_inherited_by_every_stream_type() {
    // `read-all` has one body, in `CharInput`; no stream type implements it.
    let v = eval_ok(
        r#"(let ((a (make-string-input-stream "xy"))
                 (b (make-string-input-stream "z")))
             (append (read-all a) (read-all b)))"#,
    );
    assert_eq!(str_of(v), "xyz");
}

// ---- string output ------------------------------------------------------

#[test]
fn writes_accumulate_and_are_drained_by_get_output_stream_string() {
    let v = eval_ok(
        r#"(let ((s (make-string-output-stream)))
             (write-string s "ab")
             (write-char s #\c)
             (write-line s "!")
             (get-output-stream-string s))"#,
    );
    assert_eq!(str_of(v), "abc!\n");
}

#[test]
fn draining_a_string_output_stream_clears_it() {
    let v = eval_ok(
        r#"(let ((s (make-string-output-stream)))
             (write-string s "first")
             (let ((a (get-output-stream-string s)))
               (write-string s "second")
               (append a (append "/" (get-output-stream-string s)))))"#,
    );
    assert_eq!(str_of(v), "first/second");
}

#[test]
fn with_output_to_string_returns_what_was_written() {
    let v = eval_ok(r#"(with-output-to-string (o) (write-string o "hi") (terpri o))"#);
    assert_eq!(str_of(v), "hi\n");
}

#[test]
fn with_input_from_string_binds_a_readable_stream() {
    let v = eval_ok(r#"(with-input-from-string (i "abc") (read-all i))"#);
    assert_eq!(str_of(v), "abc");
}

// ---- composite streams --------------------------------------------------

#[test]
fn a_broadcast_stream_writes_to_every_component() {
    // The payoff: `broadcast-stream` is a typelisp struct over
    // `Vector<:dyn CharOutput>` and needs nothing from the native layer.
    let v = eval_ok(
        r#"(let ((a (make-string-output-stream))
                 (b (make-string-output-stream))
                 (v (the Vector<:dyn CharOutput> (Vector::new))))
             (push v a)
             (push v b)
             (let ((bc (make-broadcast-stream v)))
               (write-line bc "both"))
             (append (get-output-stream-string a) (get-output-stream-string b)))"#,
    );
    assert_eq!(str_of(v), "both\nboth\n");
}

#[test]
fn a_concatenated_stream_runs_through_its_parts_in_order() {
    let v = eval_ok(
        r#"(let ((v (the Vector<:dyn CharInput> (Vector::new))))
             (push v (make-string-input-stream "ab"))
             (push v (make-string-input-stream ""))
             (push v (make-string-input-stream "cd"))
             (read-all (make-concatenated-stream v)))"#,
    );
    assert_eq!(str_of(v), "abcd");
}

#[test]
fn an_echo_stream_copies_what_is_read_to_its_output() {
    let v = eval_ok(
        r#"(let ((src (make-string-input-stream "log"))
                 (sink (make-string-output-stream)))
             (let ((e (make-echo-stream src sink)))
               (read-all e))
             (get-output-stream-string sink))"#,
    );
    assert_eq!(str_of(v), "log");
}

#[test]
fn a_two_way_stream_reads_from_one_side_and_writes_to_the_other() {
    let v = eval_ok(
        r#"(let ((src (make-string-input-stream "in"))
                 (sink (make-string-output-stream)))
             (let ((tw (make-two-way-stream src sink)))
               (write-string tw (read-all tw)))
             (get-output-stream-string sink))"#,
    );
    assert_eq!(str_of(v), "in");
}

#[test]
fn composites_nest() {
    // A broadcast over another broadcast: nothing special is needed, because
    // a composite is just another `:dyn CharOutput`.
    let v = eval_ok(
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
    assert_eq!(str_of(v), "xx");
}

// ---- generic code over the traits ---------------------------------------

#[test]
fn copy_stream_works_between_any_input_and_any_output() {
    let v = eval_ok(
        r#"(let ((src (make-string-input-stream "payload"))
                 (dst (make-string-output-stream)))
             (copy-stream src dst)
             (get-output-stream-string dst))"#,
    );
    assert_eq!(str_of(v), "payload");
}

#[test]
fn read_lines_collects_every_line() {
    let v = eval_ok(
        r#"(let ((s (make-string-input-stream "a
b
c")))
             (let ((out ""))
               (doiter (l (iter (read-lines s))) (setf out (append out l)))
               out))"#,
    );
    assert_eq!(str_of(v), "abc");
}

#[test]
fn a_user_defined_type_can_be_a_stream() {
    // The other half of the trait claim: nothing about being a stream is
    // reserved to the built-in types.
    let v = eval_ok(
        r#"(defstruct counter (n i32))
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
    assert_eq!(str_of(v), "5");
}

#[test]
fn a_user_stream_can_go_into_a_broadcast_alongside_a_builtin_one() {
    let v = eval_ok(
        r#"(defstruct counter (n i32))
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
    assert_eq!(str_of(v), "3abc");
}

// ---- files --------------------------------------------------------------

#[test]
fn a_file_round_trips_through_write_and_read() {
    let d = TmpDir::new("roundtrip");
    let p = d.path("a.txt");
    let v = eval_ok(&format!(
        r#"(match (write-file-string "{p}" "contents")
             ((ok _) (match (read-file-string "{p}") ((ok s) s) ((err e) (message e))))
             ((err e) (message e)))"#
    ));
    assert_eq!(str_of(v), "contents");
}

#[test]
fn read_file_lines_splits_the_file() {
    let d = TmpDir::new("lines");
    let p = d.path("b.txt");
    std::fs::write(&p, "x\ny\n").unwrap();
    let v = eval_ok(&format!(
        r#"(match (read-file-lines "{p}")
             ((ok ls) (let ((out "")) (doiter (l (iter ls)) (setf out (append out l))) out))
             ((err e) (message e)))"#
    ));
    assert_eq!(str_of(v), "xy");
}

#[test]
fn with_open_file_closes_and_yields_the_bodys_value() {
    let d = TmpDir::new("withopen");
    let p = d.path("c.txt");
    std::fs::write(&p, "data").unwrap();
    let v = eval_ok(&format!(
        r#"(match (with-open-file (f "{p}" direction-input) (read-all f))
             ((ok s) s)
             ((err e) (message e)))"#
    ));
    assert_eq!(str_of(v), "data");
}

#[test]
fn opening_a_missing_file_is_an_error_value_not_a_panic() {
    let d = TmpDir::new("missing");
    let p = d.path("nope.txt");
    let v = eval_ok(&format!(
        r#"(match (open-input "{p}") ((ok _) "opened") ((err e) (message e)))"#
    ));
    let m = str_of(v);
    assert!(m.contains("nope.txt"), "{}", m);
}

#[test]
fn appending_adds_to_an_existing_file() {
    let d = TmpDir::new("append");
    let p = d.path("d.txt");
    std::fs::write(&p, "one").unwrap();
    let v = eval_ok(&format!(
        r#"(match (with-open-file (f "{p}" direction-append) (write-string f "two"))
             ((ok _) (match (read-file-string "{p}") ((ok s) s) ((err e) (message e))))
             ((err e) (message e)))"#
    ));
    assert_eq!(str_of(v), "onetwo");
}

#[test]
fn probe_and_delete_report_and_remove() {
    let d = TmpDir::new("probe");
    let p = d.path("e.txt");
    std::fs::write(&p, "x").unwrap();
    let v = eval_ok(&format!(
        r#"(let ((before (probe-file "{p}")))
             (delete-file "{p}")
             (format false "~a~a" before (probe-file "{p}")))"#
    ));
    assert_eq!(str_of(v), "truefalse");
}

// ---- lifetime -----------------------------------------------------------

#[test]
fn close_makes_a_stream_report_itself_closed() {
    let v = eval_ok(
        r#"(let ((s (make-string-input-stream "x")))
             (let ((before (open-stream-p s)))
               (close s)
               (format false "~a~a" before (open-stream-p s))))"#,
    );
    assert_eq!(str_of(v), "truefalse");
}

#[test]
fn a_string_output_stream_can_still_be_drained_after_close() {
    // CL allows `get-output-stream-string` after `close`; the text is the
    // stream's value, not a resource the close released.
    let v = eval_ok(
        r#"(let ((s (make-string-output-stream)))
             (write-string s "kept")
             (close s)
             (get-output-stream-string s))"#,
    );
    assert_eq!(str_of(v), "kept");
}
