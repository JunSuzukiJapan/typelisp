//! `Chan<T>` — channels between tasks.
//!
//! The shape everything here leans on: a channel operation is a question for
//! the **scheduler**, whether or not it has to wait. Four of the six (`new`,
//! `len`, `cap`, and an unclosed `close`) are answered on the spot and the
//! asking task never leaves the queue; `send` and `recv` are the two that can
//! park it. One implementation answers all of them, so an interpreted `(recv
//! ch)`, a compiled one, and one reached from a print method are the same
//! code — which is what these tests check by running the same programs three
//! ways.
//!
//! Scheduling is cooperative and single-threaded (see `concurrency_test`), so
//! "who runs next" is deterministic and the interleavings below are exact,
//! not merely likely.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

fn run_with(src: &str, with_compiler: bool, stress: bool) -> Result<(Heap, Value), EvalError> {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    if with_compiler {
        typelisp::compile::install_llvm_backend();
        load_compiler(&mut h, &mut chk, &mut interp);
    }
    h.set_gc_stress(stress);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    let mut last = Value::Empty;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).expect("check failed");
        if let Some(val) = interp.exec(&mut h, tl).map_err(EvalError::into_kind)? {
            last = val;
        }
    }
    Ok((h, last))
}

fn text(src: &str) -> String {
    match run_with(src, false, false).expect("eval failed") {
        (h, Value::Str(id)) => h.string(id).to_string(),
        (_, other) => panic!("expected a string, got {:?}", other),
    }
}

/// [`text`] with the collector running at every allocation.
fn text_stressed(src: &str) -> String {
    match run_with(src, false, true).expect("eval failed") {
        (h, Value::Str(id)) => h.string(id).to_string(),
        (_, other) => panic!("expected a string, got {:?}", other),
    }
}

/// [`text`] for a source that compiles something first.
fn text_compiled(src: &str) -> String {
    match run_with(src, true, false).expect("eval failed") {
        (h, Value::Str(id)) => h.string(id).to_string(),
        (_, other) => panic!("expected a string, got {:?}", other),
    }
}

fn int(src: &str) -> i64 {
    match run_with(src, false, false).expect("eval failed").1 {
        Value::Int(n) => n,
        other => panic!("expected an integer, got {:?}", other),
    }
}

fn int_compiled(src: &str) -> i64 {
    match run_with(src, true, false).expect("eval failed").1 {
        Value::Int(n) => n,
        other => panic!("expected an integer, got {:?}", other),
    }
}

fn err(src: &str) -> String {
    match run_with(src, false, false) {
        Ok(_) => panic!("expected the program to fail"),
        Err(e) => e.to_string(),
    }
}

/// [`err`] for a source that compiles something first.
fn err_compiled(src: &str) -> String {
    match run_with(src, true, false) {
        Ok(_) => panic!("expected the program to fail"),
        Err(e) => e.to_string(),
    }
}

/// The checker's complaint about `src`.
fn check_err(src: &str) -> String {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).expect("read failed");
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Ok(tl) => {
                let _ = interp.exec(&mut h, tl);
            }
            Err(Error::At(_, inner)) => return inner.to_string(),
            Err(other) => return other.to_string(),
        }
    }
    panic!("expected a check error")
}

// ---- buffering and rendezvous -------------------------------------------

/// A buffered `send` with room does not wait: the sender runs straight on.
#[test]
fn a_buffered_send_with_room_does_not_wait() {
    assert_eq!(
        text(
            r#"(defvar (trail string) "")
               (defun fill ((ch Chan<int>)) ()
                 (send ch 1) (setf trail (append trail "a"))
                 (send ch 2) (setf trail (append trail "b"))
                 ())
               (defun main () string
                 (let ((ch (the Chan<int> (Chan::new 2))))
                   (fill ch)
                   (append trail "|")))
               (main)"#
        ),
        "ab|"
    );
}

/// An unbuffered `send` is a rendezvous: with nobody receiving yet, the
/// sender stops **inside** the `send` and the line after it does not run.
///
/// The `yield` is what makes the test about the sender. Without it the
/// receiver parks first, and then the send completes without waiting at all
/// — also right, and Go's rule too, but a different sentence.
#[test]
fn an_unbuffered_send_waits_for_a_receiver() {
    assert_eq!(
        text(
            r#"(defvar (trail string) "")
               (defun fill ((ch Chan<int>)) ()
                 (send ch 1) (setf trail (append trail "sent"))
                 ())
               (defun main () string
                 (let ((ch (the Chan<int> (Chan::new 0))))
                   (task (fill ch))
                   (yield)
                   (setf trail (append trail "before"))
                   (let ((v (unwrap (recv ch))))
                     (append (append trail "|") (to-string v)))))
               (main)"#
        ),
        "before|1"
    );
}

/// A full buffer stops the sender exactly as an unbuffered channel does, and
/// taking one out lets it go.
#[test]
fn a_full_buffer_stops_the_sender_until_room_appears() {
    assert_eq!(
        int(
            r#"(defvar (sent int) 0)
               (defun fill ((ch Chan<int>)) ()
                 (send ch 1) (setf sent (+ sent 1))
                 (send ch 2) (setf sent (+ sent 1))
                 (send ch 3) (setf sent (+ sent 1))
                 ())
               (defun main () int
                 (let ((ch (the Chan<int> (Chan::new 1))))
                   (task (fill ch))
                   (yield)
                   ;; The first send had room and ran straight on; the second
                   ;; found the buffer full and parked; the third was never
                   ;; reached. So exactly one of the three counted.
                   (let ((seen sent))
                     (unwrap (recv ch)) (unwrap (recv ch)) (unwrap (recv ch))
                     seen)))
               (main)"#
        ),
        1
    );
}

// ---- closing -------------------------------------------------------------

/// Receiving from a closed, drained channel answers `none` — and goes on
/// answering `none`.
#[test]
fn a_receive_on_a_closed_and_drained_channel_is_none() {
    assert_eq!(
        text(
            r#"(defun main () string
                 (let ((ch (the Chan<int> (Chan::new 1))))
                   (close ch)
                   (format false "~a ~a" (recv ch) (recv ch))))
               (main)"#
        ),
        "none none"
    );
}

/// Closing does not throw away what is already buffered: a receiver drains it
/// first and only then sees the end.
#[test]
fn closing_does_not_discard_what_is_buffered() {
    assert_eq!(
        text(
            r#"(defun main () string
                 (let ((ch (the Chan<int> (Chan::new 2))))
                   (send ch 1)
                   (send ch 2)
                   (close ch)
                   (format false "~a ~a ~a" (recv ch) (recv ch) (recv ch))))
               (main)"#
        ),
        "(some 1) (some 2) none"
    );
}

/// Sending on a closed channel is a panic, not a `Result` — a program bug,
/// which is Go's rule and this language's §7.1 line.
#[test]
fn sending_on_a_closed_channel_panics() {
    let e = err(
        r#"(defun main () ()
             (let ((ch (the Chan<int> (Chan::new 1))))
               (close ch)
               (send ch 1)))
           (main)"#,
    );
    assert!(e.contains("the channel is closed"), "got: {}", e);
}

/// So is closing one twice.
#[test]
fn closing_twice_panics() {
    let e = err(
        r#"(defun main () ()
             (let ((ch (the Chan<int> (Chan::new 1))))
               (close ch)
               (close ch)))
           (main)"#,
    );
    assert!(e.contains("already closed"), "got: {}", e);
}

/// Closing a channel somebody is parked in `send` on panics **that** task,
/// which is Go's rule. The panic leaves the task, and a task's escaping
/// failure stops the program.
#[test]
fn closing_while_a_sender_waits_panics_that_sender() {
    let e = err(
        r#"(defun fill ((ch Chan<int>)) () (send ch 1))
           (defun main () ()
             (let ((ch (the Chan<int> (Chan::new 0))))
               (task (fill ch))
               (yield)
               (close ch)
               (yield)))
           (main)"#,
    );
    assert!(e.contains("closed while waiting"), "got: {}", e);
}

// ---- len / cap -----------------------------------------------------------

/// `len` is what is buffered now, `cap` is what fits — CL's and Go's names.
#[test]
fn len_reports_the_buffer_and_cap_the_room() {
    assert_eq!(
        text(
            r#"(defun main () string
                 (let ((ch (the Chan<int> (Chan::new 3))))
                   (send ch 1)
                   (send ch 2)
                   (let ((a (format false "~d/~d" (len ch) (cap ch))))
                     (unwrap (recv ch))
                     (format false "~a ~d/~d" a (len ch) (cap ch)))))
               (main)"#
        ),
        "2/3 1/3"
    );
}

/// A capacity a channel cannot have is a panic, not a silently clamped zero.
#[test]
fn a_negative_capacity_panics() {
    let e = err(r#"(defun main () () (let ((ch (the Chan<int> (Chan::new -1)))) ())) (main)"#);
    assert!(e.contains("not a capacity"), "got: {}", e);
}

// ---- the iterator --------------------------------------------------------

/// `(doiter (v ch) ...)` is Go's `for v := range ch`: receive until the
/// channel is closed *and* drained. Nothing was added for it — `recv` answers
/// `Option<T>`, which is `Iter::next`'s type exactly.
#[test]
fn a_channel_is_its_own_iterator() {
    assert_eq!(
        int(
            r#"(defun produce ((ch Chan<int>) (n int)) ()
                 (dotimes (i n) (send ch (* i 2)))
                 (close ch))
               (defun main () int
                 (let ((ch (the Chan<int> (Chan::new 2))) (acc 0))
                   (task (produce ch 5))
                   (doiter (v ch) (setf acc (+ acc v)))
                   acc))
               (main)"#
        ),
        20
    );
}

// ---- the Go programs the plan checked on paper ---------------------------

/// A worker pool: N tasks take from one channel and answer on another.
#[test]
fn a_worker_pool_runs() {
    assert_eq!(
        int(
            r#"(defun worker ((jobs Chan<int>) (out Chan<int>)) ()
                 (doiter (j jobs) (send out (* j j))))
               (defun main () int
                 (let ((jobs (the Chan<int> (Chan::new 4)))
                       (out (the Chan<int> (Chan::new 4)))
                       (acc 0))
                   (dotimes (w 3) (task (worker jobs out)))
                   (dotimes (j 4) (send jobs (+ j 1)))
                   (close jobs)
                   (dotimes (k 4) (setf acc (+ acc (unwrap (recv out)))))
                   acc))
               (main)"#
        ),
        30
    );
}

/// A pipeline: each stage reads one channel and writes the next.
#[test]
fn a_pipeline_runs() {
    assert_eq!(
        int(
            r#"(defun gen ((out Chan<int>) (n int)) ()
                 (dotimes (i n) (send out (+ i 1)))
                 (close out))
               (defun square ((in Chan<int>) (out Chan<int>)) ()
                 (doiter (v in) (send out (* v v)))
                 (close out))
               (defun main () int
                 (let ((a (the Chan<int> (Chan::new 0)))
                       (b (the Chan<int> (Chan::new 0)))
                       (acc 0))
                   (task (gen a 4))
                   (task (square a b))
                   (doiter (v b) (setf acc (+ acc v)))
                   acc))
               (main)"#
        ),
        30
    );
}

/// Fan-in, written the way this language spells it: one draining task per
/// input, and the last one out closes the output.
///
/// Go's own idiom sets a closed channel to `nil` so `select` stops choosing
/// it; there is no nil channel here, deliberately (`Option<Chan<T>>` would
/// make `(recv a)` mean different things inside and outside a `select`). Go
/// recommends this shape anyway, but it is the first difference someone
/// moving from Go meets.
#[test]
fn fan_in_runs_with_a_task_per_input() {
    assert_eq!(
        int(
            r#"(defvar (left int) 0)
               (defun drain ((in Chan<int>) (out Chan<int>)) ()
                 (doiter (v in) (send out v))
                 (setf left (- left 1))
                 (if (eq left 0) (close out) ()))
               (defun feed ((ch Chan<int>) (from int) (n int)) ()
                 (dotimes (i n) (send ch (+ from i)))
                 (close ch))
               (defun main () int
                 (let ((a (the Chan<int> (Chan::new 2)))
                       (b (the Chan<int> (Chan::new 2)))
                       (out (the Chan<int> (Chan::new 2)))
                       (acc 0))
                   (setf left 2)
                   (task (feed a 1 3))
                   (task (feed b 10 3))
                   (task (drain a out))
                   (task (drain b out))
                   (doiter (v out) (setf acc (+ acc v)))
                   acc))
               (main)"#
        ),
        39
    );
}

/// `after` is a channel that receives once, `sec` from now — Go's
/// `time.After`, and the prelude's first use of `task`.
#[test]
fn after_delivers_one_value() {
    assert_eq!(
        text(
            r#"(defun main () string
                 (format false "~a" (recv (after 0.01))))
               (main)"#
        ),
        "(some ())"
    );
}

// ---- what the checker refuses -------------------------------------------

/// The element type is the channel's, and sending the wrong one is caught.
#[test]
fn the_element_type_is_checked() {
    let e = check_err(
        r#"(defun main () () (let ((ch (the Chan<int> (Chan::new 1)))) (send ch "x")))"#,
    );
    assert!(e.contains("`int`") && e.contains("`string`"), "got: {}", e);
}

/// `Chan<int>` and `Chan<string>` are different types, even though the
/// element type appears in no field of the runtime value.
#[test]
fn two_instantiations_are_different_types() {
    let e = check_err(
        r#"(defun take ((ch Chan<int>)) () ())
           (defun main () () (take (the Chan<string> (Chan::new 1))))"#,
    );
    assert!(e.contains("Chan<int>") && e.contains("Chan<string>"), "got: {}", e);
}

/// The capacity is written, always. The plan wrote it `&optional`; an omitted
/// one with no default is an `Option<int>` the callee would have to take
/// apart, and no builtin in this language has ever had one.
#[test]
fn the_capacity_is_required() {
    let e = check_err(r#"(defun main () () (let ((ch (the Chan<int> (Chan::new)))) ()))"#);
    assert!(e.contains("argument"), "got: {}", e);
}

// ---- compiled ------------------------------------------------------------

/// The same program, compiled. A compiled `send`/`recv` is a suspension, so
/// the rendezvous still parks the sender and the answer still comes back
/// through the frame's value slot.
#[test]
fn a_compiled_send_and_receive_agree_with_the_interpreter() {
    let src = r#"(defun produce ((ch Chan<int>) (n int)) ()
                   (dotimes (i n) (send ch (* i 2)))
                   (close ch))
                 (defun consume ((ch Chan<int>)) int
                   (let ((acc 0))
                     (doiter (v ch) (setf acc (+ acc v)))
                     acc))
                 (defun main () int
                   (let ((ch (the Chan<int> (Chan::new 0))))
                     (task (produce ch 5))
                     (consume ch)))"#;
    let plain = int(&format!("{}\n(main)", src));
    assert_eq!(plain, 20);
    assert_eq!(int_compiled(&format!("{}\n(compile produce)\n(compile consume)\n(main)", src)), plain);
}

/// A compiled `send` has to hand the value to the driver as one machine word,
/// and a word does not say what it is — so the suspension site tags it, per
/// the element's own representation. These three take different tagging
/// paths: an immediate, a heap pointer, and a box that has to be *allocated*.
#[test]
fn a_compiled_send_carries_every_element_representation() {
    let src = r#"(defun ints ((ch Chan<int>)) () (send ch 7) (close ch))
                 (defun strs ((ch Chan<string>)) () (send ch "hi") (close ch))
                 (defun floats ((ch Chan<f64>)) () (send ch 1.5) (close ch))
                 (defun chars ((ch Chan<char>)) () (send ch #\z) (close ch))
                 (defun main () string
                   (let ((a (the Chan<int> (Chan::new 1)))
                         (b (the Chan<string> (Chan::new 1)))
                         (c (the Chan<f64> (Chan::new 1)))
                         (d (the Chan<char> (Chan::new 1))))
                     (ints a) (strs b) (floats c) (chars d)
                     (format false "~a ~a ~a ~a" (recv a) (recv b) (recv c) (recv d))))"#;
    let plain = text(&format!("{}\n(main)", src));
    assert_eq!(plain, "(some 7) (some hi) (some 1.5) (some z)");
    assert_eq!(
        text_compiled(&format!(
            "{}\n(compile ints)(compile strs)(compile floats)(compile chars)\n(main)",
            src
        )),
        plain
    );
}

/// `len`/`cap`/`close`/`Chan::new` compile too, and they are suspensions for
/// a reason that has nothing to do with waiting: the table of channels is the
/// scheduler's, and the driver is the only way to it.
#[test]
fn the_four_operations_that_never_wait_still_compile() {
    let src = r#"(defun probe () string
                   (let ((ch (the Chan<int> (Chan::new 2))))
                     (send ch 1)
                     (let ((a (format false "~d/~d" (len ch) (cap ch))))
                       (close ch)
                       (format false "~a ~a" a (recv ch)))))"#;
    let plain = text(&format!("{}\n(probe)", src));
    assert_eq!(plain, "1/2 (some 1)");
    assert_eq!(text_compiled(&format!("{}\n(compile probe)\n(probe)", src)), plain);
}

// ---- the collector -------------------------------------------------------

/// What a channel is holding survives a collection at every allocation.
///
/// Three places have to be roots for this to pass: the ring buffer (the
/// scheduler's own root stack), the value a **parked sender** is still
/// offering (its task's state slot — nothing else points at it), and the
/// `some` box handed to a woken receiver, which exists only inside a Rust
/// enum until the receiver's state slot is written.
#[test]
fn a_channel_holds_its_values_through_gc_stress() {
    assert_eq!(
        text_stressed(
            r#"(defun fill ((ch Chan<string>) (n int)) ()
                 (dotimes (i n) (send ch (append "v" (to-string i))))
                 (close ch))
               (defun main () string
                 (let ((ch (the Chan<string> (Chan::new 1))) (acc ""))
                   (task (fill ch 6))
                   (doiter (v ch) (setf acc (append acc v)))
                   acc))
               (main)"#
        ),
        "v0v1v2v3v4v5"
    );
}

// ---- reached from a Rust caller -----------------------------------------

/// A `print-object` method is called from Rust, with a Rust frame waiting on
/// the answer — there is no continuation stack under it to put down. A
/// channel operation that **can** be answered still is: `(recv ch)` with
/// something buffered takes it, exactly as it would anywhere else.
#[test]
fn a_receive_that_needs_no_waiting_works_under_a_rust_caller() {
    assert_eq!(
        text(
            r#"(defstruct probe (ch Chan<int>))
               (impl print-object probe
                 (print-object ((self Self) (escape bool)) string
                   (format false "<~a>" (recv self::ch))))
               (defun main () string
                 (let ((ch (the Chan<int> (Chan::new 1))))
                   (send ch 3)
                   (format false "~a" (probe::new ch))))
               (main)"#
        ),
        "<(some 3)>"
    );
}

/// One that would have to wait is refused rather than deadlocked — the
/// nested-evaluation limit, and now it names `recv`.
#[test]
fn a_receive_that_would_wait_under_a_rust_caller_is_refused() {
    let e = err(
        r#"(defstruct probe (ch Chan<int>))
           (impl print-object probe
             (print-object ((self Self) (escape bool)) string
               (format false "<~a>" (recv self::ch))))
           (defun main () string
             (let ((ch (the Chan<int> (Chan::new 1))))
               (format false "~a" (probe::new ch))))
           (main)"#,
    );
    assert!(e.contains("`recv` cannot block"), "got: {}", e);
}

/// [`a_receive_that_needs_no_waiting_works_under_a_rust_caller`] for a
/// *compiled* `print-object` — the same boundary
/// (`FrameStack::run_to_end`), reached with a machine frame under it
/// instead of the interpreter's own nested evaluation. One implementation
/// answers both, so a buffered `recv` still goes through.
#[test]
fn a_compiled_receive_that_needs_no_waiting_works_under_a_rust_caller() {
    assert_eq!(
        text_compiled(
            r#"(defstruct probe (ch Chan<int>))
               (impl print-object probe
                 (print-object ((self Self) (escape bool)) string
                   (format false "<~a>" (recv self::ch))))
               (defun main () string
                 (let ((ch (the Chan<int> (Chan::new 1))))
                   (send ch 3)
                   (format false "~a" (probe::new ch))))
               (compile probe::print-object)
               (compile main)
               (main)"#
        ),
        "<(some 3)>"
    );
}

/// [`a_receive_that_would_wait_under_a_rust_caller_is_refused`] for a
/// *compiled* `print-object`: the operation would genuinely have to wait, so
/// it is refused with the same wording, not an abort.
#[test]
fn a_compiled_receive_that_would_wait_under_a_rust_caller_is_refused() {
    let e = err_compiled(
        r#"(defstruct probe (ch Chan<int>))
           (impl print-object probe
             (print-object ((self Self) (escape bool)) string
               (format false "<~a>" (recv self::ch))))
           (defun main () string
             (let ((ch (the Chan<int> (Chan::new 1))))
               (format false "~a" (probe::new ch))))
           (compile probe::print-object)
           (compile main)
           (main)"#,
    );
    assert!(e.contains("`recv` cannot block"), "got: {}", e);
}

/// `(yield)` reached from a Rust caller has nobody to yield to — there is no
/// other task this evaluation could switch to — so it simply goes on rather
/// than being refused the way a genuine wait is. True under the interpreter
/// and under a compiled body reached the same way.
#[test]
fn a_yield_under_a_rust_caller_is_a_no_op() {
    assert_eq!(
        text(
            r#"(defstruct probe (tag int))
               (impl print-object probe
                 (print-object ((self Self) (escape bool)) string
                   (progn (yield) "<ok>")))
               (defun main () string (format false "~a" (probe::new 0)))
               (main)"#
        ),
        "<ok>"
    );
}

/// [`a_yield_under_a_rust_caller_is_a_no_op`] for a *compiled* `print-object`.
#[test]
fn a_compiled_yield_under_a_rust_caller_is_a_no_op() {
    assert_eq!(
        text_compiled(
            r#"(defstruct probe (tag int))
               (impl print-object probe
                 (print-object ((self Self) (escape bool)) string
                   (progn (yield) "<ok>")))
               (defun main () string (format false "~a" (probe::new 0)))
               (compile probe::print-object)
               (compile main)
               (main)"#
        ),
        "<ok>"
    );
}
