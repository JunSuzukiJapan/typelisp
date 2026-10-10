//! `Context`: cooperative cancellation. `cancel` closes the context's `done`
//! channel and reaches every context made from it; `with-timeout` cancels by
//! itself. Every case runs interpreted and compiled.

mod common;
use common::{eval_string, eval_string_compiled};

/// `body` (a `string`-valued expression), interpreted and inside a compiled
/// function, both expected to give `expected`.
fn both(body: &str, expected: &str) {
    let interpreted = eval_string(body);
    assert_eq!(interpreted, expected, "interpreted: {}", body);
    let compiled = eval_string_compiled(&format!("(defun go () string {})\n(compile go)\n(go)", body));
    assert_eq!(compiled, expected, "compiled: {}", body);
}

/// A cancel reaches the descendants, not the parent or the siblings.
#[test]
fn cancel_reaches_what_was_made_from_the_context() {
    both(
        r##"(let* ((root (Context::background))
                   (a (Context::with-cancel root))
                   (b (Context::with-cancel a))
                   (side (Context::with-cancel root)))
             (let ((before (list (is-cancelled a) (is-cancelled b))))
               (cancel a)
               (format false "~s ~a ~a ~a ~a"
                 before (is-cancelled a) (is-cancelled b) (is-cancelled side) (is-cancelled root))))"##,
        "(false false) true true false false",
    );
}

/// `done` is closed once cancelled, so a receive returns `none` at once.
#[test]
fn done_is_closed_by_cancel() {
    both(
        r##"(let ((c (Context::with-cancel (Context::background))))
             (cancel c)
             (format false "~s ~s" (recv (done c)) (recv (done c))))"##,
        "none none",
    );
}

/// Cancelling twice is allowed, and the second does nothing.
#[test]
fn cancel_can_be_called_again() {
    both(
        r##"(let ((c (Context::background)))
             (cancel c) (cancel c)
             (format false "~a ~s" (is-cancelled c) c))"##,
        "true #<context cancelled>",
    );
}

/// A child made from a context already cancelled starts cancelled.
#[test]
fn a_child_of_a_cancelled_context_is_cancelled() {
    both(
        r##"(let ((p (Context::background)))
             (cancel p)
             (format false "~a" (is-cancelled (Context::with-cancel p))))"##,
        "true",
    );
}

#[test]
fn a_cancelled_child_leaves_its_parent() {
    both(
        r##"(let* ((root (Context::background)) (a (Context::with-cancel root)) (b (Context::with-cancel root)))
             (cancel a)
             (format false "~a ~a" (len root::children) (is-cancelled b)))"##,
        "1 false",
    );
}

/// The timeout closes `done`, so a `select` on it wins over a later `after`.
#[test]
fn with_timeout_cancels_by_itself() {
    both(
        r##"(let ((c (Context::with-timeout (Context::background) 0.05)))
             (let ((first (is-cancelled c)))
               (select
                 ((v (recv (done c))) (format false "~a cancelled ~a" first (is-cancelled c)))
                 ((v (recv (after 5.0))) "the timeout did not fire"))))"##,
        "false cancelled true",
    );
}

/// Cancelling a timed context early stops it; the timeout then has nothing
/// left to do.
#[test]
fn a_timed_context_can_be_cancelled_early() {
    both(
        r##"(let ((c (Context::with-timeout (Context::background) 5.0)))
             (cancel c)
             (format false "~s" (recv (done c))))"##,
        "none",
    );
}

/// A task waiting on `done` wakes when the context is cancelled.
#[test]
fn a_task_waiting_on_done_wakes() {
    both(
        r##"(let ((c (Context::with-cancel (Context::background)))
                  (out (the Chan<string> (Chan::new 1))))
             (task ((lambda () () (recv (done c)) (send out "woke"))))
             (yield)
             (cancel c)
             (unwrap (recv out)))"##,
        "woke",
    );
}

/// A worker on its own OS thread polls `is-cancelled`.
#[test]
fn a_thread_polling_is_cancelled_stops() {
    let src = r##"
(defun spin ((ctx Context)) int
  (let ((n 0))
    (while (not (is-cancelled ctx)) (setf n (+ n 1)) (sleep 0.001))
    1))
(let* ((ctx (Context::with-timeout (Context::background) 0.05)) (th (thread (spin ctx))))
  (format false "~a" (join th)))
"##;
    // `thread` compiles its body, so this needs the compiler loaded.
    assert_eq!(eval_string_compiled(src), "1");
}
