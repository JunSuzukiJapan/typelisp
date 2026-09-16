//! `select` — waiting on several channel operations at once.
//!
//! What shapes these tests: **choosing an arm and performing it are one
//! step.** Splitting them — asking which arm is ready and then doing it —
//! would read correctly here, because this scheduler is cooperative and
//! nothing runs in between; building a language feature on that would be
//! making a promise the scheduler happens to keep rather than one `select`
//! makes.
//!
//! The other shape is that **every operand is hoisted into a `let`** the
//! checker wraps around the node. So each channel expression and each value
//! to send is evaluated exactly once, left to right, whichever arm wins — and
//! the node itself evaluates nothing that can suspend.

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

fn text_stressed(src: &str) -> String {
    match run_with(src, false, true).expect("eval failed") {
        (h, Value::Str(id)) => h.string(id).to_string(),
        (_, other) => panic!("expected a string, got {:?}", other),
    }
}

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

// ---- choosing ------------------------------------------------------------

/// A receive arm whose channel has something takes it, and the bound name
/// sees the whole `Option<T>`.
#[test]
fn a_ready_receive_arm_wins() {
    assert_eq!(
        text(
            r#"(defun main () string
                 (let ((a (the Chan<int> (Chan::new 1)))
                       (b (the Chan<int> (Chan::new 0))))
                   (send a 7)
                   (select
                     ((v (recv a)) (format false "a=~a" v))
                     ((v (recv b)) "b"))))
               (main)"#
        ),
        "a=(some 7)"
    );
}

/// A send arm with room goes, and the value really lands in the channel.
#[test]
fn a_ready_send_arm_wins() {
    assert_eq!(
        text(
            r#"(defun main () string
                 (let ((a (the Chan<int> (Chan::new 0)))
                       (b (the Chan<string> (Chan::new 1))))
                   (let ((r (select
                              ((v (recv a)) "received")
                              ((send b "x") "sent"))))
                     (format false "~a ~a" r (recv b)))))
               (main)"#
        ),
        "sent (some x)"
    );
}

/// With nothing ready and an `else`, the `else` runs and nothing waits.
#[test]
fn else_runs_when_nothing_is_ready() {
    assert_eq!(
        text(
            r#"(defun main () string
                 (let ((a (the Chan<int> (Chan::new 0)))
                       (b (the Chan<int> (Chan::new 0))))
                   (select
                     ((v (recv a)) "a")
                     ((send b 1) "b")
                     (else "nothing"))))
               (main)"#
        ),
        "nothing"
    );
}

/// With no `else`, it waits — here for a task that sends later.
#[test]
fn without_else_it_waits() {
    assert_eq!(
        int(
            r#"(defun feed ((ch Chan<int>)) () (send ch 42) ())
               (defun main () int
                 (let ((a (the Chan<int> (Chan::new 0)))
                       (b (the Chan<int> (Chan::new 0))))
                   (go (feed a))
                   (select
                     ((v (recv a)) (unwrap v))
                     ((v (recv b)) 0))))
               (main)"#
        ),
        42
    );
}

/// A waiting `select` is woken by whichever of its channels moves — here the
/// second arm, so the arm index really is carried back rather than assumed.
#[test]
fn the_arm_that_wakes_it_is_the_one_that_runs() {
    assert_eq!(
        text(
            r#"(defun feed ((ch Chan<string>)) () (send ch "second") ())
               (defun main () string
                 (let ((a (the Chan<string> (Chan::new 0)))
                       (b (the Chan<string> (Chan::new 0))))
                   (go (feed b))
                   (select
                     ((v (recv a)) (format false "a ~a" v))
                     ((v (recv b)) (format false "b ~a" v)))))
               (main)"#
        ),
        "b (some second)"
    );
}

/// A parked `select`'s **send** arm is what another task's receive takes
/// from — the wake travels the other way too.
#[test]
fn a_parked_send_arm_is_taken_by_a_receiver() {
    assert_eq!(
        text(
            r#"(defvar (trail string) "")
               (defun taker ((ch Chan<int>)) ()
                 (setf trail (append trail (to-string (unwrap (recv ch)))))
                 ())
               (defun main () string
                 (let ((a (the Chan<int> (Chan::new 0)))
                       (b (the Chan<int> (Chan::new 0))))
                   (go (taker b))
                   (let ((r (select
                              ((v (recv a)) "recv")
                              ((send b 9) "sent"))))
                     (yield)
                     (format false "~a ~a" r trail))))
               (main)"#
        ),
        "sent 9"
    );
}

/// A closed channel makes its receive arm ready, with `none` as the answer —
/// so a `select` over a finished channel does not wait forever.
#[test]
fn a_closed_channel_makes_its_receive_arm_ready() {
    assert_eq!(
        text(
            r#"(defun main () string
                 (let ((a (the Chan<int> (Chan::new 0)))
                       (b (the Chan<int> (Chan::new 0))))
                   (close a)
                   (select
                     ((v (recv a)) (format false "a=~a" v))
                     ((v (recv b)) "b"))))
               (main)"#
        ),
        "a=none"
    );
}

// ---- evaluating the operands --------------------------------------------

/// Every channel expression and every value to send is evaluated **once**,
/// left to right, whichever arm ends up running. That is `case`'s rule for
/// its key, and it is why the checker hoists them into a `let`.
#[test]
fn every_operand_is_evaluated_once_in_order() {
    assert_eq!(
        text(
            r#"(defvar (trail string) "")
               (defun note ((tag string) (n int)) int
                 (setf trail (append trail tag))
                 n)
               (defun chan-of ((tag string) (ch Chan<int>)) Chan<int>
                 (setf trail (append trail tag))
                 ch)
               (defun main () string
                 (let ((a (the Chan<int> (Chan::new 1)))
                       (b (the Chan<int> (Chan::new 1))))
                   (send a 1)
                   (select
                     ((v (recv (chan-of "A" a))) ())
                     ((send (chan-of "B" b) (note "V" 2)) ()))
                   trail))
               (main)"#
        ),
        "ABV"
    );
}

// ---- what the checker refuses -------------------------------------------

/// No arms at all would wait forever, so it is a type error rather than a
/// program that hangs — Go's `select{}` is not adopted.
#[test]
fn a_select_with_no_arms_is_refused() {
    let e = check_err(r#"(defun main () () (select))"#);
    assert!(e.contains("no arms"), "got: {}", e);
}

/// `else` alone is just its body written the long way.
#[test]
fn else_alone_is_refused() {
    let e = check_err(r#"(defun main () int (select (else 1)))"#);
    assert!(e.contains("at least one channel arm"), "got: {}", e);
}

/// `else` must be last: every arm after it could never run.
#[test]
fn else_must_be_last() {
    let e = check_err(
        r#"(defun main () int
             (let ((a (the Chan<int> (Chan::new 0))))
               (select (else 1) ((v (recv a)) 2))))"#,
    );
    assert!(e.contains("last arm"), "got: {}", e);
}

/// The arms' bodies are joined, so two that cannot agree are refused.
#[test]
fn the_arms_types_are_joined() {
    let e = check_err(
        r#"(defun main () int
             (let ((a (the Chan<int> (Chan::new 0)))
                   (b (the Chan<int> (Chan::new 0))))
               (select ((v (recv a)) 1) ((v (recv b)) "two"))))"#,
    );
    assert!(e.contains("I32") || e.contains("Str"), "got: {}", e);
}

/// A receive arm needs a channel, and says so rather than complaining about
/// whatever `recv` would have complained about.
#[test]
fn a_non_channel_is_refused_by_name() {
    let e = check_err(r#"(defun main () int (select ((v (recv 1)) 2)))"#);
    assert!(e.contains("needs a Chan<T>"), "got: {}", e);
}

/// A send arm's value has to be the channel's element type.
#[test]
fn a_send_arms_value_is_checked() {
    let e = check_err(
        r#"(defun main () int
             (let ((a (the Chan<int> (Chan::new 1))))
               (select ((send a "x") 1))))"#,
    );
    assert!(e.contains("I32") || e.contains("Str"), "got: {}", e);
}

// ---- the timeout idiom ---------------------------------------------------

/// `after` is what makes a timeout one more arm — Go's
/// `case <-time.After(d)`.
#[test]
fn a_timeout_arm_wins_when_nothing_else_comes() {
    assert_eq!(
        text(
            r#"(defun main () string
                 (let ((a (the Chan<int> (Chan::new 0))))
                   (select
                     ((v (recv a)) "data")
                     ((z (recv (after 0.01))) "timeout"))))
               (main)"#
        ),
        "timeout"
    );
}

// ---- compiled ------------------------------------------------------------

/// The same four outcomes, compiled. A compiled `select` builds its whole
/// descriptor before suspending — every operand is a `var`, so nothing in it
/// can stop — and the answer comes back as one `(ARM . VALUE)` cons.
#[test]
fn compiled_select_agrees_with_the_interpreter() {
    let src = r#"(defun feed ((ch Chan<int>) (n int)) () (send ch n) ())
                 (defun pick ((a Chan<int>) (b Chan<string>)) string
                   (select
                     ((v (recv a)) (format false "a=~a" v))
                     ((send b "x") "sent")
                     (else "nothing")))
                 (defun waited ((a Chan<int>)) int
                   (select ((v (recv a)) (unwrap v))))
                 (defun main () string
                   (let ((out ""))
                     (let ((a (the Chan<int> (Chan::new 0)))
                           (b (the Chan<string> (Chan::new 1))))
                       (setf out (append out (pick a b))))
                     (let ((a (the Chan<int> (Chan::new 1)))
                           (b (the Chan<string> (Chan::new 0))))
                       (send a 7)
                       (setf out (append out (append "|" (pick a b)))))
                     (let ((a (the Chan<int> (Chan::new 0)))
                           (b (the Chan<string> (Chan::new 0))))
                       (setf out (append out (append "|" (pick a b)))))
                     (let ((a (the Chan<int> (Chan::new 0))))
                       (go (feed a 42))
                       (append out (append "|" (to-string (waited a)))))))"#;
    let plain = text(&format!("{}\n(main)", src));
    assert_eq!(plain, "sent|a=(some 7)|nothing|42");
    assert_eq!(text_compiled(&format!("{}\n(compile pick)\n(compile waited)\n(main)", src)), plain);
}

/// A compiled send arm hands its value to the driver as one machine word, so
/// the site tags it. `f64` is the case that has to *allocate* to do so.
#[test]
fn a_compiled_send_arm_carries_a_float() {
    let src = r#"(defun offer ((b Chan<f64>)) string
                   (select ((send b 2.5) "ok")))
                 (defun main () string
                   (let ((f (the Chan<f64> (Chan::new 1))))
                     (format false "~a ~a" (offer f) (recv f))))"#;
    let plain = text(&format!("{}\n(main)", src));
    assert_eq!(plain, "ok (some 2.5)");
    assert_eq!(text_compiled(&format!("{}\n(compile offer)\n(main)", src)), plain);
}

/// A compiled arm body that itself suspends: the merge slot is written only
/// after the body is done, so the suspension happens under it rather than
/// across it.
#[test]
fn a_compiled_arm_body_may_suspend() {
    let src = r#"(defun feed ((ch Chan<int>) (n int)) () (send ch n) ())
                 (defun relay ((a Chan<int>) (out Chan<int>)) int
                   (select ((v (recv a)) (send out (* (unwrap v) 2)) (unwrap v))))
                 (defun main () int
                   (let ((a (the Chan<int> (Chan::new 1)))
                         (out (the Chan<int> (Chan::new 0))))
                     (send a 5)
                     (go (relay a out))
                     (unwrap (recv out))))"#;
    assert_eq!(int(&format!("{}\n(main)", src)), 10);
    assert_eq!(int_compiled(&format!("{}\n(compile relay)\n(main)", src)), 10);
}

// ---- the collector -------------------------------------------------------

/// A parked `select` holds channels and values to send, and every one of them
/// is a binding of the `let` around it — which is what its frame roots.
#[test]
fn a_parked_select_holds_its_operands_through_gc_stress() {
    assert_eq!(
        text_stressed(
            r#"(defun feed ((ch Chan<string>) (n int)) ()
                 (dotimes (i n) (send ch (append "v" (to-string i))))
                 (close ch)
                 ())
               (defun main () string
                 (let ((a (the Chan<string> (Chan::new 0)))
                       (b (the Chan<string> (Chan::new 0)))
                       (acc ""))
                   (go (feed a 5))
                   (loop
                     (select
                       ((v (recv a))
                        (match v
                          ((some s) (setf acc (append acc s)) ())
                          ((none) (break))))
                       ((send b "never") ())))
                   acc))
               (main)"#
        ),
        "v0v1v2v3v4"
    );
}
