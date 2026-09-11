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
use typelisp::{load_compiler, load_prelude, Checker, Error, EvalError, Heap, Interp, Reader, Value};

fn run(src: &str) -> Result<(Heap, Value), EvalError> {
    run_with(src, false)
}

/// [`run`] with the LLVM backend and the compiler island loaded, for the tests
/// that reach `(compile ...)`.
///
/// Kept apart because it is the expensive setup: the island's own bodies have
/// to be installed before anything can be compiled, and most of this file
/// never compiles anything.
fn run_compiled(src: &str) -> Result<(Heap, Value), EvalError> {
    run_with(src, true)
}

fn run_with(src: &str, with_compiler: bool) -> Result<(Heap, Value), EvalError> {
    let mut h = Heap::with_capacity(1 << 18);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    if with_compiler {
        typelisp::compile::install_llvm_backend();
        load_compiler(&mut h, &mut chk, &mut interp);
    }
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

/// What `(compile ...)` refuses with — an evaluation error, not a check error.
fn compile_err(src: &str) -> String {
    match run_compiled(src) {
        Ok(_) => panic!("expected the compile to be refused"),
        Err(e) => e.to_string(),
    }
}

/// [`text`] for a source that compiles something.
fn text_compiled(src: &str) -> String {
    let (h, v) = run_compiled(src).expect("eval failed");
    match v {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// [`int`] for a source that compiles something.
fn int_compiled(src: &str) -> i64 {
    match run_compiled(src).expect("eval failed").1 {
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

// ---- wait ----------------------------------------------------------------

/// `(wait t)` is the task's result.
#[test]
fn wait_answers_with_the_task_s_value() {
    assert_eq!(int("(defun double ((n i32)) i32 (* n 2)) (wait (go (double 21)))"), 42);
}

/// Waiting does not consume the handle: a `Task<T>` is an ordinary value, and
/// the answer is kept, so every asker gets the same one. (Rust's
/// `JoinHandle::join` takes `self`; this deliberately does not.)
///
/// The two waits take different paths: the first suspends, and the second finds
/// the task already finished and answers without suspending at all.
#[test]
fn a_task_can_be_waited_on_more_than_once() {
    assert_eq!(
        int(r#"(defun five () i32 5)
               (let ((t (go (five)))) (+ (wait t) (wait t)))"#),
        10
    );
}

/// A task waits for another task. The waiter suspends, the inner one runs, and
/// the waiter picks up its value — the whole point of the continuation stack.
#[test]
fn a_task_can_wait_on_another_task() {
    assert_eq!(
        int(r#"(defun double ((n i32)) i32 (* n 2))
               (defun relay ((t Task<i32>)) i32 (+ (wait t) 1))
               (let ((inner (go (double 20))))
                 (wait (go (relay inner))))"#),
        41
    );
}

/// Several tasks, each waited on in turn.
#[test]
fn several_tasks_are_waited_on_in_turn() {
    assert_eq!(
        int(r#"(defun idn ((n i32)) i32 n)
               (let ((a (go (idn 1))) (b (go (idn 2))) (c (go (idn 4))))
                 (+ (wait a) (+ (wait b) (wait c))))"#),
        7
    );
}

/// A task that dies takes the program with it: a `throw` leaving a task has no
/// `catch` to reach, since tags do not cross a task boundary.
#[test]
fn a_throw_that_leaves_a_task_stops_the_program() {
    let msg = match run(r#"(defun bad () i32 (throw 'oops 1))
                           (let ((t (go (bad)))) 0)
                           1"#)
    {
        Err(e) => format!("{:?}", e),
        Ok(_) => panic!("expected the escaping throw to stop the program"),
    };
    assert!(msg.contains("left its task"), "got: {}", msg);
}

// ---- yield ---------------------------------------------------------------

/// The string a program answers with.
fn text(src: &str) -> String {
    let (h, v) = run(src).expect("eval failed");
    match v {
        Value::Str(id) => h.string(id).to_string(),
        other => panic!("expected a string, got {:?}", other),
    }
}

/// `(yield)` gives up the rest of the turn, so two tasks interleave.
///
/// Without it each task would run to its end before the other started, and the
/// trail would read `aaabbb`. This is the concurrency made visible.
#[test]
fn yield_interleaves_two_tasks() {
    assert_eq!(
        text(r#"(defvar (trail string) "")
                (defun tick ((name string) (n i32)) ()
                  (dotimes (i n)
                    (setf trail (append trail name))
                    (yield)))
                (let ((a (go (tick "a" 3))) (b (go (tick "b" 3))))
                  (progn (wait a) (wait b)))
                trail"#),
        "ababab"
    );
}

/// Without `yield`, each task runs to its end before the next one starts —
/// scheduling is cooperative, and nothing preempts a task.
#[test]
fn without_yield_each_task_runs_to_its_end() {
    assert_eq!(
        text(r#"(defvar (trail string) "")
                (defun tick ((name string) (n i32)) ()
                  (dotimes (i n) (setf trail (append trail name))))
                (let ((a (go (tick "a" 3))) (b (go (tick "b" 3))))
                  (progn (wait a) (wait b)))
                trail"#),
        "aaabbb"
    );
}

/// A `yield` with nothing else to run comes straight back.
#[test]
fn yield_with_nothing_else_ready_is_a_no_op() {
    assert_eq!(int("(progn (yield) 7)"), 7);
}

// ---- the callee can be a value ------------------------------------------

/// `go` takes a *function value* too, not only a name the checker resolved.
///
/// The checker turns that into an `apply` node, whose callee is a form at
/// field 0 rather than a resolved path — so it needs its callee evaluated
/// before there is anything to hand over. It used to type-check and then fail
/// at run time with "(go ..) wraps Some(Apply), which is not a call".
#[test]
fn go_starts_a_task_from_a_function_value() {
    assert_eq!(
        int(r#"(defun twice ((n i32)) i32 (* n 2))
               (let ((f twice))
                 (let ((t (go (f 21))))
                   (wait t)))"#),
        42
    );
}

/// `(go ((lambda () RetType body...)))` — the idiom `go`'s own error message
/// tells you to use when the body is not already a call.
#[test]
fn go_runs_an_immediately_called_lambda() {
    assert_eq!(
        text(r#"(defvar (trail string) "")
                (let ((n 7))
                  (let ((t (go ((lambda () () (when true (setf trail (append trail (to-string n)))))))))
                    (progn (wait t) trail)))"#),
        "7"
    );
}

/// The lambda captures where it is *written*, so each turn of a loop starts a
/// task over that turn's own binding — the capture trap `go`'s call-form rule
/// avoids for arguments, kept for a closure body too.
#[test]
fn each_turn_of_a_loop_starts_a_task_over_its_own_binding() {
    assert_eq!(
        text(r#"(defvar (trail string) "")
                (dotimes (i 3)
                  (let ((t (go ((lambda () () (when true (setf trail (append trail (to-string i)))))))))
                    (wait t)))
                trail"#),
        "012"
    );
}

// ---- the compiled side ---------------------------------------------------
//
// A task is an interpreter continuation stack, and compiled code runs on the
// Rust stack — so a compiled body can *start* a task but never suspend in one
// (the plan's B6). These tests pin both halves of that line.

/// `go` inside a compiled function starts a real task.
///
/// Everything up to the call happens in the compiled body — that is what `go`
/// promises anywhere — and only the call is handed over. The waiting is done
/// by the interpreted caller, which is the half that can suspend.
#[test]
fn a_compiled_body_can_start_tasks() {
    assert_eq!(
        text_compiled(r#"(defvar (trail string) "")
                (defun work ((name string)) ()
                  (when true (setf trail (append trail name))))
                (defun spawn-two () Task<()>
                  (progn (go (work "a")) (go (work "b"))))
                (compile spawn-two)
                (let ((t (spawn-two)))
                  (progn (wait t) trail))"#),
        "ab"
    );
}

/// The arguments cross as machine words in their declared representations —
/// a raw `i32`, a boxed `f64`, a tagged string — and are decoded back on the
/// interpreter's side by those same representations. A word alone cannot say
/// which it is.
#[test]
fn a_compiled_spawn_carries_arguments_of_every_representation() {
    assert_eq!(
        text_compiled(r#"(defvar (trail string) "")
                (defun mixed ((n i32) (x f64) (s string)) ()
                  (when true
                    (setf trail (append (append (append trail (to-string n)) (to-string x)) s))))
                (defun spawn-mixed ((n i32)) Task<()>
                  (go (mixed n 2.5 "hi")))
                (compile spawn-mixed)
                (let ((t (spawn-mixed 7)))
                  (progn (wait t) trail))"#),
        "72.5hi"
    );
}

/// A compiled `go` on a function *value* — an `apply` node, whose callee is a
/// form rather than a name, so it is evaluated in the compiled body and rides
/// at the head of the argument run.
#[test]
fn a_compiled_body_can_spawn_a_function_value() {
    assert_eq!(
        int_compiled(r#"(defun twice ((n i32)) i32 (* n 2))
               (defun spawn-value ((n i32)) Task<i32>
                 (let ((f twice)) (go (f n))))
               (compile spawn-value)
               (let ((t (spawn-value 21))) (wait t))"#),
        42
    );
}

/// `wait` refuses to compile, and says why.
///
/// It used to come out as the generic "a builtin method with no compiled
/// implementation", which describes the table rather than the reason.
#[test]
fn wait_refuses_to_compile_and_says_why() {
    let e = compile_err(
        r#"(defun work ((n i32)) i32 n)
           (defun waiter ((n i32)) i32 (let ((t (go (work n)))) (wait t)))
           (compile waiter)"#,
    );
    assert!(e.contains("suspends the running task"), "got {}", e);
    assert!(e.contains("task::wait"), "got {}", e);
}

/// `yield` **compiles** (Phase C3), and a compiled task really is put down.
///
/// The same program as `yield_interleaves_two_tasks`, with `tick` compiled.
/// The trail is the whole assertion: `ababab` can only be produced by a
/// compiled body stopping in the middle and being re-entered, three times
/// each, because nothing preempts a task
/// (`without_yield_each_task_runs_to_its_end`).
///
/// What makes it possible is that the frames are in the task rather than on
/// the machine stack — `Task::compiled`. Before C3 this was a refusal, and the
/// test asserted the refusal's wording.
#[test]
fn a_compiled_yield_interleaves_two_tasks() {
    assert_eq!(
        text_compiled(
            r#"(defvar (trail string) "")
               (defun tick ((name string) (n i32)) ()
                 (dotimes (i n)
                   (setf trail (append trail name))
                   (yield)))
               (compile tick)
               (let ((a (go (tick "a" 3))) (b (go (tick "b" 3))))
                 (progn (wait a) (wait b)))
               trail"#
        ),
        "ababab"
    );
}

/// A compiled `yield` with nothing else ready comes straight back, and the
/// value after it is the function's.
///
/// The narrower claim, and the one that fails first if the resume point is
/// wrong: the body has to carry on *after* the suspension rather than restart
/// or fall out of its `dotimes`.
#[test]
fn a_compiled_yield_resumes_where_it_left_off() {
    assert_eq!(
        int_compiled(
            r#"(defun counted ((n i32)) i32
                 (let ((acc 0))
                   (dotimes (i n)
                     (yield)
                     (setf acc (+ acc 1))
                     (yield))
                   acc))
               (compile counted)
               (counted 4)"#
        ),
        4
    );
}

// ---- sleep ---------------------------------------------------------------

/// `(sleep secs)` stops **the task**, not the OS thread.
///
/// The ordering is the discriminator, and it needs no clock: `a` is spawned
/// first and waited on first, but sleeps four times as long. If `sleep` held
/// the thread, `a` would run to its end before `b` ever started and the trail
/// would be "ab". Both tasks park instead, and the nearer deadline wins.
#[test]
fn sleep_suspends_only_the_calling_task() {
    assert_eq!(
        text(r#"(defvar (trail string) "")
                (defun slow ((name string) (sec f64)) ()
                  (progn (sleep sec)
                         (when true (setf trail (append trail name)))))
                (let ((a (go (slow "a" 0.20))) (b (go (slow "b" 0.05))))
                  (progn (wait a) (wait b) trail))"#),
        "ba"
    );
}

/// A deadline that has already passed is not a wait at all, so CL's yield-ish
/// `(sleep 0)` falls out with no special case: the task goes straight back on
/// the queue, which is exactly what `yield` does.
#[test]
fn sleep_zero_is_a_yield() {
    assert_eq!(
        text(r#"(defvar (trail string) "")
                (defun tick ((name string) (n i32)) ()
                  (dotimes (i n)
                    (setf trail (append trail name))
                    (sleep 0.0)))
                (let ((a (go (tick "a" 3))) (b (go (tick "b" 3))))
                  (progn (wait a) (wait b)))
                trail"#),
        "ababab"
    );
}

/// A lone task still waits — nothing else is ready, so the *program* waits,
/// and it is the scheduler that reaches the OS rather than the `sleep` call.
#[test]
fn a_lone_sleeper_still_waits_and_comes_back() {
    assert_eq!(int("(progn (sleep 0.01) 7)"), 7);
}

/// `sleep` refuses to compile, for the reason `wait` and `yield` do.
///
/// It is the one of the three that *could* have been left alone — compiled
/// code has always had `rt_sleep`. That is exactly the problem: the same
/// source would stop one task interpreted and the whole program compiled.
#[test]
fn sleep_refuses_to_compile_and_says_why() {
    let e = compile_err(
        r#"(defun nap () () (sleep 0.01))
           (compile nap)"#,
    );
    assert!(e.contains("suspends the running task"), "got {}", e);
    assert!(e.contains("`sleep`"), "got {}", e);
}
