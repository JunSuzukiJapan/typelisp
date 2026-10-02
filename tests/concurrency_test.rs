//! Tasks — `task` and what it hands back.
//!
//! The rule that shapes these tests: **`(task (f a b))` evaluates `f` and every
//! argument where it is written**, in that order, and only the call itself
//! happens in the new task. That is Go's own rule for `go f(x)`, and it is why
//! `task` takes a call form rather than a thunk — a thunk would capture the
//! arguments instead of evaluating them.
//!
//! Scheduling here is cooperative and single-threaded: nothing preempts a
//! task, and the interpreter's one OS thread runs all of them. An executable
//! runs its tasks on several threads — `tests/os_threads_test.rs`.

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
    run_with_stress(src, with_compiler, false)
}

/// [`run_compiled`] with every `cons` collecting first (`Heap::set_gc_stress`)
/// from the program's first form on — the environment is loaded unstressed.
fn run_compiled_stressed(src: &str) -> Result<(Heap, Value), EvalError> {
    run_with_stress(src, true, true)
}

fn run_with_stress(src: &str, with_compiler: bool, stress: bool) -> Result<(Heap, Value), EvalError> {
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

fn int(src: &str) -> i64 {
    match run(src).expect("eval failed").1 {
        Value::Int(n) => n,
        other => panic!("expected an integer, got {:?}", other),
    }
}

/// What a *running* compiled program fails with, as distinct from what
/// `(compile ...)` refuses with.
fn compile_err_at_runtime(src: &str) -> String {
    match run_compiled(src) {
        Ok(_) => panic!("expected the program to fail"),
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
        int(r#"(defvar (counter int) 0)
               (defun work ((n int)) int (setf counter n))
               (task (work 7))
               counter"#),
        7
    );
}

/// Two tasks both run, and both effects land.
#[test]
fn two_spawned_calls_both_run() {
    assert_eq!(
        int(r#"(defvar (a int) 0)
               (defvar (b int) 0)
               (defun set-a ((n int)) int (setf a n))
               (defun set-b ((n int)) int (setf b n))
               (task (set-a 3))
               (task (set-b 4))
               (+ a b)"#),
        7
    );
}

/// The arguments are evaluated **where the `task` is written**, not inside the
/// task. A loop that spawns with its own index therefore gives each task a
/// different one — Go's notorious capture pitfall does not exist here, because
/// there is nothing captured to change.
#[test]
fn arguments_are_evaluated_at_the_spawn() {
    assert_eq!(
        int(r#"(defvar (total int) 0)
               (defun bump ((n int)) int (setf total (+ total n)))
               (dotimes (i 4) (task (bump i)))
               total"#),
        6
    );
}

/// A method call is a call: `task` takes one just as happily as a free function.
#[test]
fn a_method_call_can_be_spawned() {
    assert_eq!(
        int(r#"(defvar (seen int) 0)
               (defstruct crate (v int))
               (defmethod stash ((self crate)) int (setf seen self::v))
               (task (stash (crate::new 9)))
               seen"#),
        9
    );
}

// ---- what `task` refuses ---------------------------------------------------

/// A special form is not a call. The message says so and points at the lambda
/// that *is* the way to run an arbitrary body.
#[test]
fn task_refuses_a_special_form() {
    let e = check_err("(task (if true 1 2))");
    assert!(e.contains("`task` cannot start `if`"), "got: {}", e);
    assert!(e.contains("lambda"), "the message should name the way out: {}", e);
}

/// Nor a bare value.
#[test]
fn task_refuses_a_non_call() {
    let e = check_err("(task 1)");
    assert!(e.contains("task"), "got: {}", e);
}

/// One argument, not two.
#[test]
fn task_refuses_extra_arguments() {
    let e = check_err("(defun f () int 1) (task (f) (f))");
    assert!(e.contains("task"), "got: {}", e);
}

// ---- `thread` --------------------------------------------------------------

/// `thread` is `task`'s shape, and its refusals name it.
#[test]
fn thread_refuses_what_task_refuses() {
    let e = check_err("(thread (if true 1 2))");
    assert!(e.contains("`thread` cannot start `if`"), "got: {}", e);
    let e = check_err("(thread 1)");
    assert!(e.contains("`thread` takes a call form: (thread (f args...))"), "got: {}", e);
}

/// `(thread (f))` is a `Thread<T>`, and `join` answers with the `T`.
#[test]
fn thread_is_a_thread_of_the_call_type() {
    let e = check_err("(defun f () int 1) (defun g () string (join (thread (f))))");
    assert!(e.contains("mismatch"), "got: {}", e);
}

/// `thread` is closed as a name the way `task` is: a variable by that name
/// could be bound but never called.
#[test]
fn thread_cannot_be_bound() {
    let e = check_err("(let ((thread 1)) thread)");
    assert!(e.contains("`thread` is a reserved word"), "got: {}", e);
}

/// `thread` runs in the interpreter, interpreted or compiled: the call moves
/// to an OS thread of its own as compiled code — an interpreted `thread`
/// compiles its callee first.
#[test]
fn thread_runs_in_the_interpreter_interpreted_or_compiled() {
    match run_compiled("(defun f () int 1) (join (thread (f)))") {
        Ok((_, v)) => assert_eq!(v, Value::Int(1)),
        Err(e) => panic!("interpreted: {}", e),
    }
    match run_compiled("(defun f () int 1) (defun g () int (join (thread (f)))) (compile g) (g)") {
        Ok((_, v)) => assert_eq!(v, Value::Int(1)),
        Err(e) => panic!("compiled: {}", e),
    }
}

/// What a thread's OS thread cannot run is refused before the thread starts,
/// as a panic the program can catch: an interpreted function value, which
/// cannot be compiled on its own.
#[test]
fn thread_of_an_interpreted_lambda_is_refused() {
    let e = compile_err_at_runtime("(let ((f (lambda () int 1))) (join (thread (f))))");
    assert!(e.contains("thread: the function value is not compiled code"), "got: {}", e);
}

/// The two static functions answer in the interpreter too — neither needs
/// another thread — and a call to either leaves `Thread<T>`'s `T` alone.
#[test]
fn thread_statics_answer_in_the_interpreter() {
    assert_eq!(int("(if (> (Thread::current-id) 0) 1 0)"), 1);
    assert_eq!(int("(if (eq (Thread::current-id) (Thread::current-id)) 1 0)"), 1);
    assert_eq!(int("(if (> (Thread::available-parallelism) 0) 1 0)"), 1);
}

// ---- the type ------------------------------------------------------------

/// `(task (f ...))` is a `Task<T>` where `T` is the call's own return type, so a
/// mismatched annotation is a type error naming both.
#[test]
fn a_spawn_is_a_task_of_the_call_s_return_type() {
    let e = check_err(r#"(defun f () int 1)
                         (let ((t (the Task<string> (task (f))))) ())"#);
    assert!(e.contains("mismatch"), "got: {}", e);
}

/// And the right annotation is accepted. `hit` is read in a *later* form
/// because nothing switches tasks inside one: the `task` queues the task, and the
/// form that queued it runs on to its own end first.
#[test]
fn a_task_can_be_annotated_with_its_own_type() {
    assert_eq!(
        int(r#"(defvar (hit int) 0)
               (defun f () int (setf hit 5))
               (let ((t (the Task<int> (task (f))))) 0)
               hit"#),
        5
    );
}

// ---- wait ----------------------------------------------------------------

/// `(wait t)` is the task's result.
#[test]
fn wait_answers_with_the_task_s_value() {
    assert_eq!(int("(defun double ((n int)) int (* n 2)) (wait (task (double 21)))"), 42);
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
        int(r#"(defun five () int 5)
               (let ((t (task (five)))) (+ (wait t) (wait t)))"#),
        10
    );
}

/// A task waits for another task. The waiter suspends, the inner one runs, and
/// the waiter picks up its value — the whole point of the continuation stack.
#[test]
fn a_task_can_wait_on_another_task() {
    assert_eq!(
        int(r#"(defun double ((n int)) int (* n 2))
               (defun relay ((t Task<int>)) int (+ (wait t) 1))
               (let ((inner (task (double 20))))
                 (wait (task (relay inner))))"#),
        41
    );
}

/// Several tasks, each waited on in turn.
#[test]
fn several_tasks_are_waited_on_in_turn() {
    assert_eq!(
        int(r#"(defun idn ((n int)) int n)
               (let ((a (task (idn 1))) (b (task (idn 2))) (c (task (idn 4))))
                 (+ (wait a) (+ (wait b) (wait c))))"#),
        7
    );
}

/// A task that dies takes the program with it: a `throw` leaving a task has no
/// `catch` to reach, since tags do not cross a task boundary.
#[test]
fn a_throw_that_leaves_a_task_stops_the_program() {
    let msg = match run(r#"(defun bad () int (throw 'oops 1))
                           (let ((t (task (bad)))) 0)
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
                (defun tick ((name string) (n int)) ()
                  (dotimes (i n)
                    (setf trail (append trail name))
                    (yield)))
                (let ((a (task (tick "a" 3))) (b (task (tick "b" 3))))
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
                (defun tick ((name string) (n int)) ()
                  (dotimes (i n) (setf trail (append trail name))))
                (let ((a (task (tick "a" 3))) (b (task (tick "b" 3))))
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

/// `task` takes a *function value* too, not only a name the checker resolved.
///
/// The checker turns that into an `apply` node, whose callee is a form at
/// field 0 rather than a resolved path — so it needs its callee evaluated
/// before there is anything to hand over. It used to type-check and then fail
/// at run time with "(task ..) wraps Some(Apply), which is not a call".
#[test]
fn task_starts_a_task_from_a_function_value() {
    assert_eq!(
        int(r#"(defun twice ((n int)) int (* n 2))
               (let ((f twice))
                 (let ((t (task (f 21))))
                   (wait t)))"#),
        42
    );
}

/// `(task ((lambda () RetType body...)))` — the idiom `task`'s own error message
/// tells you to use when the body is not already a call.
#[test]
fn task_runs_an_immediately_called_lambda() {
    assert_eq!(
        text(r#"(defvar (trail string) "")
                (let ((n 7))
                  (let ((t (task ((lambda () () (when true (setf trail (append trail (to-string n)))))))))
                    (progn (wait t) trail)))"#),
        "7"
    );
}

/// The lambda captures where it is *written*, so each turn of a loop starts a
/// task over that turn's own binding — the capture trap `task`'s call-form rule
/// avoids for arguments, kept for a closure body too.
#[test]
fn each_turn_of_a_loop_starts_a_task_over_its_own_binding() {
    assert_eq!(
        text(r#"(defvar (trail string) "")
                (dotimes (i 3)
                  (let ((t (task ((lambda () () (when true (setf trail (append trail (to-string i)))))))))
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

/// `task` inside a compiled function starts a real task.
///
/// Everything up to the call happens in the compiled body — that is what `task`
/// promises anywhere — and only the call is handed over. The waiting is done
/// by the interpreted caller, which is the half that can suspend.
///
/// Each task answers its own result and the caller waits for both, so the
/// outcome does not depend on how the two are scheduled: they may run on
/// different OS threads, in either order.
#[test]
fn a_compiled_body_can_start_tasks() {
    assert_eq!(
        text_compiled(r#"(defun work ((name string)) string name)
                (defun spawn-two () cons-cell<Task<string>,Task<string>>
                  (cons (task (work "a")) (task (work "b"))))
                (compile spawn-two)
                (let ((ts (spawn-two)))
                  (append (wait ts::car) (wait ts::cdr)))"#),
        "ab"
    );
}

/// The arguments cross as machine words in their declared representations —
/// a raw `int`, a boxed `f64`, a tagged string — and are decoded back on the
/// interpreter's side by those same representations. A word alone cannot say
/// which it is.
#[test]
fn a_compiled_spawn_carries_arguments_of_every_representation() {
    assert_eq!(
        text_compiled(r#"(defvar (trail string) "")
                (defun mixed ((n int) (x f64) (s string)) ()
                  (when true
                    (setf trail (append (append (append trail (to-string n)) (to-string x)) s))))
                (defun spawn-mixed ((n int)) Task<()>
                  (task (mixed n 2.5 "hi")))
                (compile spawn-mixed)
                (let ((t (spawn-mixed 7)))
                  (progn (wait t) trail))"#),
        "72.5hi"
    );
}

/// A compiled `task` on a function *value* — an `apply` node, whose callee is a
/// form rather than a name, so it is evaluated in the compiled body and rides
/// at the head of the argument run.
#[test]
fn a_compiled_body_can_spawn_a_function_value() {
    assert_eq!(
        int_compiled(r#"(defun twice ((n int)) int (* n 2))
               (defun spawn-value ((n int)) Task<int>
                 (let ((f twice)) (task (f n))))
               (compile spawn-value)
               (let ((t (spawn-value 21))) (wait t))"#),
        42
    );
}

/// A compiled `wait` **gets the awaited task's value back** (Phase C3c).
///
/// The one of the three whose answer is typed: `Task<T>` -> `T`. It comes
/// across as a tagged `Sexpr` whatever `T` is and the resume block decodes it
/// with the kind the bridge baked in, exactly as a thrown value does — which
/// is why suspension is a node carrying a result representation rather than a
/// name the island recognises.
#[test]
fn a_compiled_wait_answers_with_the_awaited_value() {
    assert_eq!(
        int_compiled(
            r#"(defun work ((n int)) int (* n 10))
               (defun waiter ((n int)) int (let ((h (task (work n)))) (+ (wait h) 1)))
               (compile waiter)
               (waiter 4)"#
        ),
        41
    );
}

/// The same for a heap-backed `T`, which is the half a raw-word answer would
/// pass by accident: a `string` crosses as a tagged pointer and the decode has
/// to leave it tagged, where an `int` has to be untagged.
#[test]
fn a_compiled_wait_answers_with_a_heap_value() {
    assert_eq!(
        text_compiled(
            r#"(defun greet ((name string)) string (append "hi " name))
               (defun waiter ((name string)) string
                 (let ((h (task (greet name)))) (append (wait h) "!")))
               (compile waiter)
               (waiter "ada")"#
        ),
        "hi ada!"
    );
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
               (defun tick ((name string) (n int)) ()
                 (dotimes (i n)
                   (setf trail (append trail name))
                   (yield)))
               (compile tick)
               (let ((a (task (tick "a" 3))) (b (task (tick "b" 3))))
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
            r#"(defun counted ((n int)) int
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
                (let ((a (task (slow "a" 0.20))) (b (task (slow "b" 0.05))))
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
                (defun tick ((name string) (n int)) ()
                  (dotimes (i n)
                    (setf trail (append trail name))
                    (sleep 0.0)))
                (let ((a (task (tick "a" 3))) (b (task (tick "b" 3))))
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

/// A compiled `sleep` stops **the task**, not the thread (Phase C3c).
///
/// `sleep_suspends_only_the_calling_task`'s program with `slow` compiled, and
/// the same discriminator: `a` is spawned first and waited on first but sleeps
/// four times as long, so a `sleep` that held the thread would give `"ab"`.
///
/// This is the one of the three that *could* have been left alone — compiled
/// code has always had a thread-sleeping `rt_sleep`. That was exactly the
/// problem, and why the shim is gone rather than kept beside the new one: the
/// same source would stop one task interpreted and the whole program compiled,
/// silently.
#[test]
fn a_compiled_sleep_suspends_only_the_calling_task() {
    assert_eq!(
        text_compiled(
            r#"(defvar (trail string) "")
               (defun slow ((name string) (sec f64)) ()
                 (progn (sleep sec)
                        (when true (setf trail (append trail name)))))
               (compile slow)
               (let ((a (task (slow "a" 0.20))) (b (task (slow "b" 0.05))))
                 (progn (wait a) (wait b) trail))"#
        ),
        "ba"
    );
}

/// A negative wait is an error on both sides, in the same words.
#[test]
fn a_compiled_sleep_refuses_a_negative_wait() {
    let e = compile_err_at_runtime(
        r#"(defun nap () () (sleep -1.0))
           (compile nap)
           (nap)"#,
    );
    assert!(e.contains("not a non-negative number of seconds"), "got {}", e);
}

// ---- suspension inside a protected region (C4) ---------------------------

/// A compiled `yield` **inside a `catch`** suspends.
///
/// Before C4 this was a compile error naming the form: a call written inside a
/// region was driven to completion on a nested driver standing on the machine
/// stack, and a machine frame is exactly what cannot be put down. C4 made the
/// region a slot on the frame (`FRAME_HANDLER_SLOT`), so a call in one is an
/// ordinary driver round trip and suspends like any other.
///
/// `ababab` is `a_compiled_yield_interleaves_two_tasks`'s assertion with the
/// suspension one region deeper.
#[test]
fn a_compiled_yield_inside_a_catch_suspends() {
    assert_eq!(
        text_compiled(
            r#"(defvar (trail string) "")
               (defun tick ((name string) (n int)) int
                 (catch 'unused
                   (progn
                     (dotimes (i n)
                       (setf trail (append trail name))
                       (yield))
                     0)))
               (compile tick)
               (let ((a (task (tick "a" 3))) (b (task (tick "b" 3))))
                 (progn (wait a) (wait b)))
               trail"#
        ),
        "ababab"
    );
}

/// A compiled `sleep` inside an `unwind-protect` still runs the cleanup, and
/// runs it when the task comes *back* rather than when it left.
///
/// The trail is what separates those two: `b` gets its whole turn while `a`
/// sleeps, so `cleanup` landing after it means the cleanup ran on the resumed
/// activation. A cleanup that ran at the suspension would read
/// `a cleanup b`.
#[test]
fn a_compiled_sleep_inside_an_unwind_protect_runs_the_cleanup_after() {
    assert_eq!(
        text_compiled(
            r#"(defvar (trail string) "")
               (defun slow () int
                 (unwind-protect
                   (progn (setf trail (append trail "a")) (sleep 0.02) 1)
                   (setf trail (append trail " cleanup"))))
               (defun quick () int (progn (setf trail (append trail " b")) 2))
               (compile slow)
               (let ((a (task (slow))) (b (task (quick))))
                 (progn (wait a) (wait b)))
               trail"#
        ),
        "a b cleanup"
    );
}

/// The bug `docs/dev/os-threads-design.md` §3 named: a compiled
/// `unwind-protect` cleanup that suspends (here, `sleep`) leaves its throw's
/// tag and caught-unwind payload parked *while it is carrying one* — the
/// protected form throws, the driver catches it at the activation boundary,
/// and only then does the cleanup pad run and put the task down. Before
/// `IN_FLIGHT_TAG`/`CAUGHT_UNWIND` moved onto the task (and
/// `Heap::in_flight_throw` onto each task's own `RootStack`), those were bare
/// thread-locals/a single `Heap` field, so `b`'s own throw — running on the
/// same OS thread while `a` is down — clobbered them, and `a`'s resumed
/// unwind would have carried `b`'s tag and value instead of its own (or
/// found nothing parked at all).
///
/// Ordered by channels, not by how long anything sleeps: `a`'s cleanup hands
/// `b` the go-ahead on `ready` and then waits on `go`, so `b` starts its own
/// catch/throw round trip only once `a` is parked mid-unwind, and `a` cannot
/// finish its cleanup until `b` has. (It used to be `b` sleeping 10 ms against
/// `a`'s cleanup sleeping 50 ms, which a loaded CI runner did not keep apart.)
/// `a`'s own throw must still resolve to its own tag and value once its
/// cleanup finishes, and the trail must show `b` finished its turn before
/// `a`'s cleanup did — `a`'s throw truly waited out `b`'s.
#[test]
fn a_task_switch_during_a_cleanup_does_not_corrupt_the_parked_throw() {
    let src = r#"(defvar (trail string) "")
                 (defun a ((ready Chan<int>) (go Chan<int>)) int
                   (catch 'a-tag
                     (unwind-protect
                       (progn (setf trail (append trail "a1")) (throw 'a-tag 111))
                       (progn
                         (send ready 0)
                         (unwrap (recv go))
                         (setf trail (append trail " a2"))))))
                 (defun b ((ready Chan<int>) (go Chan<int>)) int
                   (progn
                     (unwrap (recv ready))
                     (catch 'b-tag (throw 'b-tag 222))
                     (setf trail (append trail " b"))
                     (send go 0)
                     0))
                 (compile a)
                 (compile b)
                 (let ((ready (the Chan<int> (Chan::new 0))) (go (the Chan<int> (Chan::new 0))))
                   (let ((ta (task (a ready go))) (tb (task (b ready go))))
                     (+ (wait ta) (wait tb))))"#;
    assert_eq!(int_compiled(src), 111, "a's own throw must survive b's throw running while a was parked");
    assert_eq!(text_compiled(&format!("{} trail", src)), "a1 b a2", "b must finish its own throw before a's cleanup resumes");
}

/// A `throw` raised *after* a suspension is still caught by the `catch` the
/// task suspended inside.
///
/// The handler is a frame slot and the frame outlives the activation, so this
/// is the assertion that it does. A handler kept in a machine register, or a
/// landing pad belonging to the activation that suspended, would both lose it
/// here.
#[test]
fn a_compiled_throw_after_a_suspension_is_still_caught() {
    assert_eq!(
        int_compiled(
            r#"(defun guarded () int
                 (catch 'done (progn (yield) (throw 'done 41))))
               (compile guarded)
               (wait (task (guarded)))"#
        ),
        41
    );
}

// ---- applying a function value from compiled code (C5) -------------------

/// A compiled body applies a **compiled** function value whose body yields,
/// and the task is put down mid-apply.
///
/// Before C5 an `apply` was a plain call to `rt_apply_any`, a C function that
/// had to come back with an answer — so everything underneath it stood on a
/// machine frame that could not be put down. The callee now joins the
/// caller's own chain: the `apply` names its callee to the driver, and the
/// driver is the one that looks at the value.
#[test]
fn a_compiled_apply_of_a_compiled_value_can_suspend() {
    assert_eq!(
        text_compiled(
            r#"(defvar (trail string) "")
               (defun tick ((name string) (n int)) int
                 (progn (dotimes (i n) (setf trail (append trail name)) (yield)) 0))
               (defun via-value ((name string)) int
                 (let ((f tick)) (f name 3)))
               (compile via-value)
               (let ((a (task (via-value "a"))) (b (task (via-value "b"))))
                 (progn (wait a) (wait b)))
               trail"#
        ),
        "ababab"
    );
}

/// A compiled body applies an **interpreted** closure that yields.
///
/// This is the refusal C5 removes. The closure runs on the task's own
/// continuation stack — not on a Rust frame inside the compiled activation —
/// so it can block, and the compiled frame waiting for its answer is a heap
/// object that goes down with the task.
#[test]
fn a_compiled_apply_of_an_interpreted_closure_can_suspend() {
    assert_eq!(
        text_compiled(
            r#"(defvar (trail string) "")
               (defun call-thrice ((f (fn () int))) int
                 (progn (dotimes (i 3) (f)) 0))
               (compile call-thrice)
               (let ((a (lambda () int (progn (setf trail (append trail "a")) (yield) 0)))
                     (b (lambda () int (progn (setf trail (append trail "b")) (yield) 0))))
                 (let ((ta (task (call-thrice a))) (tb (task (call-thrice b))))
                   (progn (wait ta) (wait tb))))
               trail"#
        ),
        "ababab"
    );
}

/// A `throw` raised by an interpreted closure that compiled code applied is
/// claimed by a `catch` in the **compiled** frame that applied it.
///
/// The exit has no machine frames to unwind through any more, so it travels
/// up the continuation stack and is handed to the chain as a status
/// (`FrameStack::raise`) — landing in the same pad a compiled `throw` would.
/// What the pad finds is what an unwinding call used to leave it, which is
/// why `park_for_compiled` writes both channels.
#[test]
fn a_throw_from_an_applied_interpreted_closure_reaches_a_compiled_catch() {
    assert_eq!(
        int_compiled(
            r#"(defun guarded ((f (fn () int))) int (catch 'boom (f)))
               (compile guarded)
               (let ((f (lambda () int (throw 'boom 41)))) (guarded f))"#
        ),
        41
    );
}

/// And an `unwind-protect` in the compiled frame runs its cleanup on the way
/// past — the exit is walked through the chain rather than around it.
#[test]
fn an_applied_closure_s_throw_runs_a_compiled_cleanup() {
    assert_eq!(
        text_compiled(
            r#"(defvar (trail string) "")
               (defun guarded ((f (fn () int))) int
                 (catch 'boom (unwind-protect (f) (setf trail (append trail "cleanup")))))
               (compile guarded)
               (let ((f (lambda () int (throw 'boom 1))))
                 (progn (guarded f) (setf trail (append trail "!"))))
               trail"#
        ),
        "cleanup!"
    );
}

/// A compiled body calls a `:dyn` method whose implementation is **ordinary
/// interpreted code**, and it yields.
///
/// The slot behind a trait object need not hold a compiled address at all:
/// the concrete type can be the user's own struct whose method nothing ever
/// compiled. That was `rt_dyn_call`'s whole reason for existing at the
/// runtime rather than the call site — and since C5 the same reason puts the
/// decision in the driver,
/// which can hand the call to the continuation stack instead of running it on
/// a machine frame.
#[test]
fn a_compiled_dyn_call_of_an_interpreted_method_can_suspend() {
    assert_eq!(
        text_compiled(
            r#"(defvar (trail string) "")
               (deftrait Ticker ()
                 (tick ((self Self) (n int)) int))
               (defstruct marker (name string))
               (impl Ticker marker
                 (tick ((self Self) (n int)) int
                   (progn (dotimes (i n) (setf trail (append trail self::name)) (yield)) 0)))
               (defun drive-it ((t :dyn Ticker)) int (tick t 3))
               (compile drive-it)
               (let ((a (task (drive-it (marker::new "a"))))
                     (b (task (drive-it (marker::new "b")))))
                 (progn (wait a) (wait b)))
               trail"#
        ),
        "ababab"
    );
}

// ---- a loop's own safepoint (C7) -----------------------------------------

/// **A compiled loop with no call in it still lets another task run.**
///
/// `spin-then-mark`'s loop body is `>=`, `+` and `setf` — all lowered inline
/// by the island, so there is no call anywhere in it. Before C7 the task held
/// the only thread for all thousand iterations and `mark` could not run until
/// it was done. The back edge now polls (`emit-loop-safepoint`), and the poll
/// hands control back often enough that `b` lands first.
///
/// `ba`, not `ab`, is the whole assertion: `a` is spawned first and would
/// finish first if nothing interrupted it.
///
/// **A driver round trip is not a scheduling point**, which is worth stating
/// because the plan's C7 reasons as though it were ("a tight loop with no
/// call in it gets no driver round trip"). `FrameStack::drive` loops on the
/// status word: `STATUS_CALL` enters the callee and keeps going, and only
/// `STATUS_SUSPEND` returns to whoever can reschedule. So for *starvation*
/// every loop was a gap, not just a call-less one. The plan's framing is the
/// right one for a **collector** — there, a round trip is an opportunity to
/// stop the thread, so a call-less loop really is the only blind spot.
#[test]
fn a_compiled_loop_with_no_calls_still_yields_to_another_task() {
    assert_eq!(
        text_compiled(
            r#"(defvar (trail string) "")
               (defun spin-then-mark ((n int)) int
                 (let ((i 0))
                   (loop (if (>= i n) (break) ()) (setf i (+ i 1)))
                   (setf trail (append trail "a"))
                   0))
               (defun mark () int (progn (setf trail (append trail "b")) 0))
               (compile spin-then-mark)
               (compile mark)
               (let ((a (task (spin-then-mark 1000))) (b (task (mark))))
                 (progn (wait a) (wait b)))
               trail"#
        ),
        "ba"
    );
}

/// **The same loop, applied as a closure value, still just runs.**
///
/// An interpreted caller applying a *compiled closure value* used to be a
/// Rust frame waiting for the word with nowhere to park a suspended
/// chain — this is now driven by the task instead
/// (`State::CompiledEnter`/`DriveCallee::Closure`, closure unification
/// Phase 2 of the AOT-scheduler work), so a real `sleep` or `wait` inside
/// one now genuinely suspends (see
/// `an_interpreted_caller_applying_a_compiled_closure_may_really_wait`
/// below). A safepoint offer is not a real wait — nothing is being waited
/// for — so it still resumes at once either way; this test is the
/// computation-only case, kept for its own sake (a call-less loop's back
/// edge polling something).
///
/// A driver that genuinely *cannot* park a chain still exists
/// (`FrameStack::run_to_end`'s doc comment): the printer's door
/// (`rt_drive_body`) and the C FFI thunk. There, a safepoint still just
/// resumes and a real wait is answered on the spot or refused — never
/// parked, because there is no task standing under either.
#[test]
fn a_compiled_loop_safepoint_under_a_machine_frame_driver_just_resumes() {
    assert_eq!(
        int_compiled(
            r#"(defun make-spin () (fn (int) int)
                 (lambda ((n int)) int
                   (let ((i 0))
                     (loop (if (>= i n) (break) ()) (setf i (+ i 1)))
                     i)))
               (compile make-spin)
               (let ((f (make-spin))) (f 1000))"#
        ),
        1000
    );
}

/// **An interpreted caller applying a compiled closure value may really
/// wait** — the boundary the previous test's revised doc comment describes.
/// `main` is the interpreter's own task: it starts `mark` under `task`, then
/// calls the closure `f` directly (not through `task`) and the closure waits
/// on a channel that only `mark` sends on. `ba` proves it genuinely parked
/// the calling task and let `mark` run in the meantime — the same proof
/// `a_compiled_loop_with_no_calls_still_yields_to_another_task` makes for a
/// named compiled call. A call that could not park would never see the send:
/// the program would stop on the wait instead of answering at all. (It used
/// to be the closure sleeping 50 ms against `mark` sleeping 10 ms, which a
/// loaded CI runner did not keep apart — and a late `mark` reads `ab`, the
/// same as the bug this test is for.)
#[test]
fn an_interpreted_caller_applying_a_compiled_closure_may_really_wait() {
    assert_eq!(
        text_compiled(
            r#"(defvar (trail string) "")
               (defun make-worker ((ch Chan<int>)) (fn () int)
                 (lambda () int (progn (unwrap (recv ch)) (setf trail (append trail "a")) 0)))
               (defun mark ((ch Chan<int>)) int (progn (setf trail (append trail "b")) (send ch 0) 0))
               (compile make-worker)
               (compile mark)
               (let ((ch (the Chan<int> (Chan::new 0))))
                 (let ((f (make-worker ch)) (b (task (mark ch))))
                   (progn (f) (wait b))))
               trail"#
        ),
        "ba"
    );
}

/// (o) A compiled task that moves to the interpreter's thread in the middle
/// of its chain (it applies an interpreted closure) keeps everything it had —
/// the chain's frames, the closure and its arguments, the values it built
/// before the move — through a collection on every `cons`, with the other
/// tasks allocating on other threads meanwhile. A root lost across the move
/// shows up here as a wrong total or a crash.
#[test]
fn a_task_that_moves_to_the_interpreter_s_thread_keeps_its_roots_under_gc_stress() {
    let src = r#"(defun work ((cb (fn (int) int)) (n int)) int
                   (let ((xs (list n (+ n 1) (+ n 2))))
                     (+ (cb (sexpr-list-length xs)) (* 3 n) 3)))
                 (defun start ((cb (fn (int) int)) (n int)) Task<int> (task (work cb n)))
                 (compile start)
                 (let ((cb (lambda ((k int)) int (sexpr-list-length (list k k k k))))
                       (tasks (the Vector<Task<int>> (Vector::new)))
                       (total 0))
                   (dotimes (i 4) (push tasks (start cb (* i 10))))
                   (doiter (t (iter tasks)) (setf total (+ total (wait t))))
                   total)"#;
    // Each task: 4 from the closure (whatever `k` is), plus 3n + 3, for
    // n = 0, 10, 20, 30.
    let expected: i64 = (0..4).map(|i| 4 + 3 * (i * 10) + 3).sum();
    match run_compiled_stressed(src) {
        Ok((_, v)) => assert_eq!(v, Value::Int(expected)),
        Err(e) => panic!("{}", e),
    }
}
