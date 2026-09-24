//! Tasks on several OS threads under the interpreter (`typl`) —
//! `docs/dev/os-threads-design.md` §8 "typl", Phase 5.
//!
//! Every script here is run by the `typl` binary twice, with
//! `TYPELISP_THREADS=1` and with `=4`, and both runs must exit the same way
//! and print the same thing — except where a test is *about* the thread
//! count, and says so. What a script prints is chosen to be the same whatever
//! order its tasks ran in.
//!
//! What is checked is where a task runs and that it gets there with the right
//! answer: an interpreted task stays on the interpreter's thread, a compiled
//! one may be stepped by a worker, and one that needs the interpreter part of
//! the way through moves to the interpreter's thread and finishes there.

use std::path::PathBuf;
use std::process::Command;

fn tmp_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("typl-threads-test-tmp");
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    dir
}

/// Writes `source` as `<scratch>/<name>.typl`, runs it with `typl` on
/// `threads` threads, and answers with the exit code, stdout and stderr.
fn run_on(name: &str, source: &str, threads: &str) -> (Option<i32>, String, String) {
    let path = tmp_dir().join(format!("{}.typl", name));
    std::fs::write(&path, source).expect("failed to write the script");
    let out = Command::new(env!("CARGO_BIN_EXE_typl"))
        .arg(&path)
        .env("TYPELISP_THREADS", threads)
        .output()
        .expect("failed to run typl");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Runs `source` on one thread and on four, asserting each run exits with
/// `code` and prints `stdout`.
fn same_on_one_and_four_threads(name: &str, source: &str, code: i32, stdout: &str) {
    for threads in ["1", "4"] {
        let (got_code, got_stdout, stderr) = run_on(name, source, threads);
        assert_eq!(got_code, Some(code), "TYPELISP_THREADS={}: stderr was: {}", threads, stderr);
        assert_eq!(got_stdout, stdout, "TYPELISP_THREADS={}: stderr was: {}", threads, stderr);
    }
}

/// (j) An interpreted `go`'s task needs the interpreter, so it runs on the
/// thread the interpreter is on — however many threads there are.
#[test]
fn an_interpreted_task_runs_on_the_interpreter_s_thread() {
    same_on_one_and_four_threads(
        "typl_interpreted_go",
        r#"(defun whoami () int (Thread::current-id))
           (let* ((me (Thread::current-id))
                  (tasks (the Vector<Task<int>> (Vector::new)))
                  (elsewhere 0))
             (dotimes (i 16) (push tasks (go (whoami))))
             (doiter (t (iter tasks)) (if (eq (wait t) me) 0 (setf elsewhere (+ elsewhere 1))))
             (println "~a" elsewhere))"#,
        0,
        "0\n",
    );
}

/// (k) A compiled `go`'s task needs no interpreter, so the workers take such
/// tasks too: with four threads they are seen on more than one, with one
/// thread only on the interpreter's. A `thread` runs on one of its own
/// either way. Like the executable's test of the same thing, the one here
/// whose answer depends on the thread count.
#[test]
fn compiled_tasks_run_on_workers_and_a_thread_on_its_own() {
    let source = r#"(defun work-then-report ((out Chan<int>)) ()
                      (let ((acc 0))
                        (dotimes (i 20000) (setf acc (+ acc (mod i 7))))
                        (send out (Thread::current-id))))
                    (defun whoami () int (Thread::current-id))
                    (defun spread () int
                      (let ((out (the Chan<int> (Chan::new 64)))
                            (seen (the Vector<int> (Vector::new))))
                        (dotimes (i 64) (go (work-then-report out)))
                        (dotimes (i 64)
                          (let ((id (unwrap (recv out))))
                            (match (find id (iter seen))
                              ((some x) ())
                              ((none) (push seen id)))))
                        (len seen)))
                    (defun own-thread () int (join (thread (whoami))))
                    (compile spread)
                    (compile own-thread)
                    (let ((me (Thread::current-id)))
                      (println "~a ~a" (spread) (eq (own-thread) me)))"#;
    let (code, out, err) = run_on("typl_compiled_go", source, "1");
    assert_eq!(code, Some(0), "stderr was: {}", err);
    assert_eq!(out, "1 false\n", "on one thread every task runs on the interpreter's; stderr was: {}", err);
    let (code, out, err) = run_on("typl_compiled_go", source, "4");
    assert_eq!(code, Some(0), "stderr was: {}", err);
    let mut words = out.split_whitespace();
    let distinct: usize = words.next().and_then(|n| n.parse().ok()).expect("a count");
    assert!((2..=4).contains(&distinct), "expected the tasks on 2 to 4 threads, got {:?}; stderr was: {}", out, err);
    assert_eq!(words.next(), Some("false"), "a thread ran on the interpreter's thread; stderr was: {}", err);
}

/// (l) A compiled task that applies an interpreted closure moves to the
/// interpreter's thread to run it, gets the right answer back into its chain,
/// and stays there: everything it does afterwards reports the interpreter's
/// thread too.
#[test]
fn a_compiled_task_that_needs_the_interpreter_moves_to_its_thread() {
    same_on_one_and_four_threads(
        "typl_needs_main",
        r#"(defun work ((cb (fn (int) int))) int
             (let ((acc 0))
               (dotimes (i 20000) (setf acc (+ acc (mod i 7))))
               (let* ((v (cb 41))
                      (after (Thread::current-id)))
                 (+ (* v 1000000) (if (eq after (cb 0)) 1 0)))))
           (defun start ((cb (fn (int) int))) Task<int> (go (work cb)))
           (compile start)
           (let* ((me (Thread::current-id))
                  (cb (lambda ((x int)) int (if (eq x 0) (Thread::current-id) (+ x (if (eq (Thread::current-id) me) 1 0)))))
                  (tasks (the Vector<Task<int>> (Vector::new)))
                  (right 0))
             (dotimes (i 8) (push tasks (start cb)))
             (doiter (t (iter tasks)) (if (eq (wait t) 42000001) (setf right (+ right 1)) right))
             (println "~a" right))"#,
        0,
        "8\n",
    );
}

/// (l) The `:dyn` way into the same move: a compiled task calls a trait
/// method through `:dyn` whose implementation nobody compiled.
#[test]
fn a_dyn_call_to_an_interpreted_method_moves_the_task_too() {
    same_on_one_and_four_threads(
        "typl_needs_main_dyn",
        r#"(deftrait Answer () (answer ((self Self)) int))
           (defstruct fixed (n int))
           (impl Answer fixed (answer ((self Self)) int (+ self::n (Thread::current-id))))
           (defun ask ((a :dyn Answer)) int (- (answer a) (Thread::current-id)))
           (defun start ((a :dyn Answer)) Task<int> (go (ask a)))
           (compile start)
           (let ((tasks (the Vector<Task<int>> (Vector::new)))
                 (right 0))
             (dotimes (i 8) (push tasks (start (fixed::new 42))))
             (doiter (t (iter tasks)) (if (eq (wait t) 42) (setf right (+ right 1)) right))
             (println "~a" right))"#,
        0,
        "8\n",
    );
}

/// (m) Redefining and recompiling a function while a compiled task is parked
/// in the middle of its old body leaves the old body where the task can come
/// back to it: the retired code is kept while any chain stands.
#[test]
fn a_parked_task_keeps_the_body_it_is_in_across_a_recompile() {
    same_on_one_and_four_threads(
        "typl_recompile_parked",
        r#"(defun slow () int (sleep 0.2) 1)
           (defun start () Task<int> (go (slow)))
           (compile start)
           (defvar (pending Task<int>) (start))
           (sleep 0.05)
           (defun slow () int 2)
           (compile slow)
           (defun other () int 3)
           (compile other)
           (println "~a ~a" (wait pending) (slow))"#,
        0,
        "1 2\n",
    );
}

/// (n) A worker prints what the interpreter's thread prints: enum variant
/// names, a niche-represented `Option` in a field, a compiled `print-object`,
/// and the printer control variables as the program last set them. The
/// `thread` makes the printing happen off the interpreter's thread whatever
/// the thread count.
#[test]
fn a_worker_prints_what_the_interpreter_s_thread_prints() {
    let source = r#"(defenum shape (tri int int int) (dot))
                    (defstruct point (x int) (y int))
                    (impl print-object point
                      (print-object ((self Self) (escape bool)) string (format false "<~a,~a>" self::x self::y)))
                    (defstruct holder (o Option<int>))
                    (defun show () string
                      (format false "~a ~a ~a ~a" (shape::tri 3 4 5) (point::new 1 2) (holder::new (Option::some 7)) (shape::dot)))
                    (defun elsewhere () string (join (thread (show))))
                    (compile point::print-object)
                    (compile elsewhere)
                    (setf *print-length* 2)
                    (println "~a" (show))
                    (println "~a" (elsewhere))"#;
    for threads in ["1", "4"] {
        let (code, out, err) = run_on("typl_worker_print", source, threads);
        assert_eq!(code, Some(0), "TYPELISP_THREADS={}: stderr was: {}", threads, err);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "TYPELISP_THREADS={}: stdout was: {}", threads, out);
        assert!(lines[0].contains("(tri 3 4 ...)") && lines[0].contains("<1,2>"), "the interpreter's own rendering: {}", lines[0]);
        assert_eq!(lines[1], lines[0], "TYPELISP_THREADS={}: a worker rendered differently", threads);
    }
}

/// (n) A `print-object` nobody compiled cannot run off the interpreter's
/// thread, and printing such a value there says so rather than ignoring the
/// method.
#[test]
fn an_interpreted_print_object_off_the_interpreter_s_thread_is_a_panic() {
    let source = r#"(defstruct blob (n int))
                    (impl print-object blob
                      (print-object ((self Self) (escape bool)) string "blob!"))
                    (defun show () string (format false "~a" (blob::new 1)))
                    (defun elsewhere () string (join (thread (show))))
                    (compile elsewhere)
                    (println "~a" (elsewhere))"#;
    for threads in ["1", "4"] {
        let (code, out, err) = run_on("typl_worker_print_interpreted", source, threads);
        assert_ne!(code, Some(0), "TYPELISP_THREADS={}: stdout was: {}", threads, out);
        assert!(
            err.contains("print-object of `") && err.contains("blob` is not compiled"),
            "TYPELISP_THREADS={}: stderr was: {}",
            threads,
            err
        );
    }
}

/// (l) The other way into the move: a compiled call only the interpreter can
/// answer — `eval`, here, and `read` behind it — made by a task on another
/// thread. The task moves to the interpreter's thread before the call, gets
/// the same answer it would have got there, and stays: the thread id it
/// reports afterwards is the interpreter's.
#[test]
fn a_compiled_eval_off_the_interpreter_s_thread_moves_there_first() {
    let source = r#"(defun via-eval () string
                      (let ((r (eval (unwrap (read "(+ 40 2)")))))
                        (format false "~a ~a" r (Thread::current-id))))
                    (defun elsewhere () string (join (thread (via-eval))))
                    (compile elsewhere)
                    (println "~a" (via-eval))
                    (println "~a" (elsewhere))"#;
    for threads in ["1", "4"] {
        let (code, out, err) = run_on("typl_eval_moves", source, threads);
        assert_eq!(code, Some(0), "TYPELISP_THREADS={}: stderr was: {}", threads, err);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "TYPELISP_THREADS={}: stdout was: {}", threads, out);
        assert!(lines[0].starts_with("(ok 42) "), "the interpreter's own answer: {}", lines[0]);
        assert_eq!(lines[1], lines[0], "TYPELISP_THREADS={}: the moved task answered differently", threads);
    }
}

/// An interpreted `thread` compiles its callee — and what that calls — and
/// runs it on an OS thread of its own, with the arguments the interpreter
/// evaluated and the answer read back by the callee's declared result: an
/// `int`, a `bool` (a raw word, not a tagged one), a `string`.
#[test]
fn an_interpreted_thread_runs_its_callee_compiled_on_its_own_thread() {
    same_on_one_and_four_threads(
        "typl_interpreted_thread",
        r#"(defun whoami () int (Thread::current-id))
           (defun add ((a int) (b int)) int (+ a b))
           (defun positive ((a int)) bool (> a 0))
           (defun greet ((who string)) string (format false "hello ~a" who))
           (let ((me (Thread::current-id)))
             (println "~a ~a ~a ~a ~a"
                      (eq (join (thread (whoami))) me)
                      (join (thread (add 40 2)))
                      (join (thread (positive 3)))
                      (join (thread (positive -3)))
                      (join (thread (greet "there")))))"#,
        0,
        "false 42 true false hello there\n",
    );
}
