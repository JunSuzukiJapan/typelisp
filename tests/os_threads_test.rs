//! Tasks on several OS threads in a standalone executable
//! (`docs/dev/os-threads-design.md` §8, Phase 3).
//!
//! Every program here is compiled once and run twice — with
//! `TYPELISP_THREADS=1` (every task on the thread that runs `main`) and with
//! `TYPELISP_THREADS=4` (three workers beside it) — and both runs must give
//! the same exit code and print the same thing. What they print is chosen to
//! be the same whatever order the tasks ran in: totals, not traces.
//!
//! That tasks really do run on several threads at once is checked below the
//! language, in `crates/typelisp-rt/tests/sched_threads_test.rs`; a program
//! cannot name its thread until `Thread::current-id` exists (Phase 4).

use std::path::PathBuf;
use std::process::Command;

fn tmp_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp");
    std::fs::create_dir_all(&dir).expect("failed to create the AOT test scratch dir");
    dir
}

/// Compiles `source` as `<scratch>/<name>` and runs it with 1 and with 4
/// threads, asserting each run exits with `code` and prints `stdout`. Returns
/// the 4-thread run's stderr.
fn same_on_one_and_four_threads(name: &str, source: &str, code: i32, stdout: &str) -> String {
    let dir = tmp_dir();
    let src_path = dir.join(format!("{}.typl", name));
    let out_path = dir.join(name);
    std::fs::write(&src_path, source).expect("failed to write test source file");
    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");
    let mut stderr = String::new();
    for threads in ["1", "4"] {
        let out = Command::new(&out_path)
            .env("TYPELISP_THREADS", threads)
            .output()
            .expect("failed to run the compiled executable");
        stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(code), "TYPELISP_THREADS={}: stderr was: {}", threads, stderr);
        assert_eq!(String::from_utf8_lossy(&out.stdout), stdout, "TYPELISP_THREADS={}: stderr was: {}", threads, stderr);
    }
    stderr
}

/// Eight tasks increment one `Mutex<int>` ten thousand times each, and a
/// `WaitGroup` says when they are done. With the tasks on several threads
/// the lock is the only thing keeping each increment whole, so a lost update
/// shows up as a total short of 80000.
#[test]
fn a_mutex_counts_every_increment_from_eight_tasks() {
    same_on_one_and_four_threads(
        "threads_mutex",
        r#"(defun bump ((m Mutex<int>) (wg WaitGroup) (times int)) ()
             (dotimes (i times)
               (with-lock (n m) (setf n (+ n 1))))
             (done wg))
           (defun main () int
             (let ((m (the Mutex<int> (Mutex::make 0)))
                   (wg (the WaitGroup (WaitGroup::make))))
               (add wg 8)
               (dotimes (i 8) (go (bump m wg 10000)))
               (wait wg)
               (println "~a" (with-lock (n m) n))
               0))"#,
        0,
        "80000\n",
    );
}

/// Every task allocates lists, strings and structs as fast as it can, in a
/// heap small enough that it has to be collected many times over while they
/// do — so collections start on whichever thread runs out, and every other
/// thread has to be stopped with its values rooted. A value lost to a
/// collection shows up as a wrong length or a wrong sum.
#[test]
fn tasks_allocating_on_every_thread_survive_collections() {
    same_on_one_and_four_threads(
        "threads_gc",
        r#"(defstruct pair (a int) (b string))
           (defun build ((n int)) Option<Sexpr>
             (let ((acc (the Option<Sexpr> ())))
               (dotimes (i n) (setf acc (sexpr-cons (quote x) acc)))
               acc))
           (defun len ((l Option<Sexpr>)) int
             (let ((k 0) (c l))
               (loop (if (sexpr-null c)
                         (return k)
                         (progn (setf k (+ k 1)) (setf c (sexpr-cdr c)))))))
           (defun churn ((rounds int)) int
             (let ((total 0))
               (dotimes (r rounds)
                 (let* ((l (build 500))
                        (p (pair::new r (format false "r~a" r))))
                   (setf total (+ total (len l)))
                   (if (equal p::b (format false "r~a" p::a)) () (panic "a struct lost its string"))))
               total))
           (defun churn-into ((out Chan<int>) (rounds int)) () (send out (churn rounds)))
           (defun main () int
             (let ((before (let ((h (heap-info))) h::gc-count))
                   (results (the Chan<int> (Chan::new 8))))
               (dotimes (i 8) (go (churn-into results 200)))
               (let ((sum 0))
                 (dotimes (i 8) (setf sum (+ sum (unwrap (recv results)))))
                 (println "~a ~a" sum (> (let ((h (heap-info))) h::gc-count) (+ before 1))))
               0))"#,
        0,
        "800000 true\n",
    );
}

/// Lists built by several producers cross a channel to one consumer while
/// the heap is being collected: a value in a channel's buffer, and one being
/// handed from a parked sender, has to stay rooted whichever thread is
/// collecting.
#[test]
fn values_crossing_a_channel_survive_collections() {
    same_on_one_and_four_threads(
        "threads_chan_gc",
        r#"(defun build ((n int)) Option<Sexpr>
             (let ((acc (the Option<Sexpr> ())))
               (dotimes (i n) (setf acc (sexpr-cons (quote x) acc)))
               acc))
           (defun len ((l Option<Sexpr>)) int
             (let ((k 0) (c l))
               (loop (if (sexpr-null c)
                         (return k)
                         (progn (setf k (+ k 1)) (setf c (sexpr-cdr c)))))))
           (defun produce ((ch Chan<Option<Sexpr>>) (wg WaitGroup) (n int)) ()
             (dotimes (i 300) (send ch (build n)))
             (done wg))
           (defun close-when-done ((wg WaitGroup) (ch Chan<Option<Sexpr>>)) () (wait wg) (close ch))
           (defun main () int
             (let ((ch (the Chan<Option<Sexpr>> (Chan::new 4)))
                   (wg (the WaitGroup (WaitGroup::make))))
               (add wg 4)
               (dotimes (i 4) (go (produce ch wg (+ 100 i))))
               (go (close-when-done wg ch))
               (let ((total 0))
                 (loop (match (recv ch)
                         ((none) (break))
                         ((some l) (setf total (+ total (len l))))))
                 (println "~a" total))
               0))"#,
        0,
        // 300 lists each of 100, 101, 102 and 103 cells.
        "121800\n",
    );
}

/// A pipeline of tasks joined by channels, a `select` with a timeout arm
/// from `after`, and a `WaitGroup` — the same answers whichever threads the
/// stages land on.
#[test]
fn a_pipeline_with_select_and_after_gives_the_same_answer() {
    same_on_one_and_four_threads(
        "threads_pipeline",
        r#"(defun numbers ((out Chan<int>) (n int)) ()
             (dotimes (i n) (send out i))
             (close out))
           (defun square ((in Chan<int>) (out Chan<int>) (wg WaitGroup)) ()
             (loop (match (recv in)
                     ((none) (break))
                     ((some v) (send out (* v v)))))
             (done wg))
           (defun close-when-done ((wg WaitGroup) (ch Chan<int>)) () (wait wg) (close ch))
           (defun main () int
             (let ((a (the Chan<int> (Chan::new 0)))
                   (b (the Chan<int> (Chan::new 8)))
                   (wg (the WaitGroup (WaitGroup::make))))
               (go (numbers a 1000))
               (add wg 3)
               (dotimes (i 3) (go (square a b wg)))
               (go (close-when-done wg b))
               (let ((sum 0) (timeouts 0))
                 (loop
                   (select
                     ((v (recv b)) (match v
                                     ((none) (break))
                                     ((some x) (setf sum (+ sum x)))))
                     ((t (recv (after 5.0))) (setf timeouts (+ timeouts 1)))))
                 (let ((quiet (the Chan<int> (Chan::new 0))))
                   (select
                     ((v (recv quiet)) (println "impossible"))
                     ((t (recv (after 0.05))) (println "~a ~a timed-out" sum timeouts)))))
               0))"#,
        0,
        "332833500 0 timed-out\n",
    );
}

/// A task that panics — wherever it runs — stops the program with the
/// panic, even while `main` is parked waiting for something else.
#[test]
fn a_panic_in_a_task_stops_the_program_from_any_thread() {
    let err = same_on_one_and_four_threads(
        "threads_panic",
        r#"(defun boom ((n int)) int
             (progn (sleep 0.01) (if (> n 0) (panic "boom in a task") n)))
           (defun main () int
             (let ((never (the Chan<int> (Chan::new 0))))
               (dotimes (i 8) (go (boom i)))
               (recv never)
               0))"#,
        1,
        "",
    );
    assert!(err.contains("panic: boom in a task"), "stderr was: {}", err);
}

/// With every task parked on something nothing will ever do, the program
/// says so — also when idle workers are waiting beside `main`.
#[test]
fn a_deadlock_is_reported_with_workers_idle() {
    let err = same_on_one_and_four_threads(
        "threads_deadlock",
        r#"(defun stuck ((ch Chan<int>)) int (unwrap (recv ch)))
           (defun main () int
             (let ((ch (the Chan<int> (Chan::new 0))))
               (let ((t (go (stuck ch))))
                 (wait t))))"#,
        1,
        "",
    );
    assert!(err.contains("every task is blocked"), "stderr was: {}", err);
}

/// Tasks a `defvar` initialiser starts keep running on the workers while
/// the next initialiser and then `main` run, and `main` sees what they did.
#[test]
fn tasks_started_by_an_initialiser_are_there_for_main() {
    same_on_one_and_four_threads(
        "threads_defvar",
        r#"(defun count-to ((n int)) int
             (let ((acc 0)) (dotimes (i n) (setf acc (+ acc i))) acc))
           (defvar (big Task<int>) (go (count-to 100000)))
           (defvar (small Task<int>) (go (count-to 1000)))
           (defun main () int
             (progn (println "~a ~a" (wait big) (wait small)) 0))"#,
        0,
        "4999950000 499500\n",
    );
}

/// `TYPELISP_THREADS` that is not a thread count stops the program before it
/// runs, rather than being quietly replaced by some other number.
#[test]
fn a_thread_count_that_is_not_one_is_refused() {
    let dir = tmp_dir();
    let src_path = dir.join("threads_bad_env.typl");
    let out_path = dir.join("threads_bad_env");
    std::fs::write(&src_path, "(defun main () int (progn (println \"ran\") 0))").unwrap();
    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");
    for bad in ["0", "four", "-1"] {
        let out = Command::new(&out_path).env("TYPELISP_THREADS", bad).output().unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "TYPELISP_THREADS={}: stderr was: {}", bad, err);
        assert_eq!(String::from_utf8_lossy(&out.stdout), "", "TYPELISP_THREADS={}", bad);
        assert!(err.contains("TYPELISP_THREADS"), "TYPELISP_THREADS={}: stderr was: {}", bad, err);
    }
}
