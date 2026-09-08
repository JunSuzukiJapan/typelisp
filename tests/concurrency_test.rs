//! Tasks — `go` and what it hands back.
//!
//! The rule that shapes these tests: **`(go (f a b))` evaluates `f` and every
//! argument where it is written**, in that order, and only the call itself
//! happens in the new task. That is Go's own rule for `go f(x)`, and it is why
//! `go` takes a call form rather than a thunk — a thunk would capture the
//! arguments instead of evaluating them.
//!
//! Scheduling is cooperative and single-threaded: nothing preempts a task, and
//! one OS thread runs all of them. `ACTIVE_HEAP` has to be a single
//! thread-local and `Heap` is `!Send`, so tasks cannot be spread across threads
//! without making the heap shareable first.

extern crate typelisp;
use typelisp::{load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<(Heap, Value), EvalError> {
    let mut h = Heap::with_capacity(1 << 18);
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
    Ok((h, last))
}

fn int(src: &str) -> i64 {
    match run(src).expect("eval failed").1 {
        Value::Int(n) => n,
        other => panic!("expected an integer, got {:?}", other),
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
    let mut err = None;
    for v in vs {
        match chk.check_form(&mut h, &interp, v) {
            Ok(tl) => {
                let _ = interp.exec(&mut h, tl);
            }
            Err(e) => {
                err = Some(match e {
                    Error::At(_, inner) => inner.to_string(),
                    other => other.to_string(),
                });
                break;
            }
        }
    }
    err.expect("expected a check error")
}

// ---- the task runs -------------------------------------------------------

/// A spawned call really happens: the task runs and its effect is visible.
#[test]
fn a_spawned_call_runs() {
    assert_eq!(
        int(r#"(defvar (counter i32) 0)
               (defun work ((n i32)) i32 (setf counter n))
               (go (work 7))
               counter"#),
        7
    );
}

/// Two tasks both run, and both effects land.
#[test]
fn two_spawned_calls_both_run() {
    assert_eq!(
        int(r#"(defvar (a i32) 0)
               (defvar (b i32) 0)
               (defun set-a ((n i32)) i32 (setf a n))
               (defun set-b ((n i32)) i32 (setf b n))
               (go (set-a 3))
               (go (set-b 4))
               (+ a b)"#),
        7
    );
}

/// The arguments are evaluated **where the `go` is written**, not inside the
/// task. A loop that spawns with its own index therefore gives each task a
/// different one — Go's notorious capture pitfall does not exist here, because
/// there is nothing captured to change.
#[test]
fn arguments_are_evaluated_at_the_spawn() {
    assert_eq!(
        int(r#"(defvar (total i32) 0)
               (defun bump ((n i32)) i32 (setf total (+ total n)))
               (dotimes (i 4) (go (bump i)))
               total"#),
        6
    );
}

/// A method call is a call: `go` takes one just as happily as a free function.
#[test]
fn a_method_call_can_be_spawned() {
    assert_eq!(
        int(r#"(defvar (seen i32) 0)
               (defstruct crate (v i32))
               (defmethod stash ((self crate)) i32 (setf seen self::v))
               (go (stash (crate::new 9)))
               seen"#),
        9
    );
}

// ---- what `go` refuses ---------------------------------------------------

/// A special form is not a call. The message says so and points at the lambda
/// that *is* the way to run an arbitrary body.
#[test]
fn go_refuses_a_special_form() {
    let e = check_err("(go (if true 1 2))");
    assert!(e.contains("`go` cannot start `if`"), "got: {}", e);
    assert!(e.contains("lambda"), "the message should name the way out: {}", e);
}

/// Nor a bare value.
#[test]
fn go_refuses_a_non_call() {
    let e = check_err("(go 1)");
    assert!(e.contains("go"), "got: {}", e);
}

/// One argument, not two.
#[test]
fn go_refuses_extra_arguments() {
    let e = check_err("(defun f () i32 1) (go (f) (f))");
    assert!(e.contains("go"), "got: {}", e);
}

// ---- the type ------------------------------------------------------------

/// `(go (f ...))` is a `Task<T>` where `T` is the call's own return type, so a
/// mismatched annotation is a type error naming both.
#[test]
fn a_spawn_is_a_task_of_the_call_s_return_type() {
    let e = check_err(r#"(defun f () i32 1)
                         (let ((t (the Task<string> (go (f))))) ())"#);
    assert!(e.contains("mismatch"), "got: {}", e);
}

/// And the right annotation is accepted. `hit` is read in a *later* form
/// because nothing switches tasks inside one: the `go` queues the task, and the
/// form that queued it runs on to its own end first.
#[test]
fn a_task_can_be_annotated_with_its_own_type() {
    assert_eq!(
        int(r#"(defvar (hit i32) 0)
               (defun f () i32 (setf hit 5))
               (let ((t (the Task<i32> (go (f))))) 0)
               hit"#),
        5
    );
}
