//! Tasks on several OS threads in a standalone executable
//! (`docs/dev/os-threads-design.md` §8, Phase 3).
//!
//! Every program here is compiled once and run twice — with
//! `TYPELISP_THREADS=1` (every task on the thread that runs `main`) and with
//! `TYPELISP_THREADS=4` (three workers beside it) — and both runs must give
//! the same exit code and print the same thing. What they print is chosen to
//! be the same whatever order the tasks ran in: totals, not traces.
//!
//! That tasks really do run on several threads at once is checked twice: below
//! the language, in `crates/typelisp-rt/tests/sched_threads_test.rs`, and here
//! with `Thread::current-id` — the one test whose answer depends on the thread
//! count, and says so.
//!
//! `thread` (Phase 4) starts a task on an OS thread of its own, whatever the
//! count: the tests for it give the same answers on one thread and on four.

use std::path::PathBuf;
use std::process::Command;

fn tmp_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp");
    std::fs::create_dir_all(&dir).expect("failed to create the AOT test scratch dir");
    dir
}

/// Compiles `source` as `<scratch>/<name>` and answers with the executable's
/// path.
fn compile(name: &str, source: &str) -> PathBuf {
    let dir = tmp_dir();
    let src_path = dir.join(format!("{}.typl", name));
    let out_path = dir.join(name);
    std::fs::write(&src_path, source).expect("failed to write test source file");
    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");
    out_path
}

/// Runs `exe` with `TYPELISP_THREADS=threads`: exit code, stdout, stderr.
fn run_on(exe: &PathBuf, threads: &str) -> (Option<i32>, String, String) {
    let out = Command::new(exe)
        .env("TYPELISP_THREADS", threads)
        .output()
        .expect("failed to run the compiled executable");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Compiles `source` as `<scratch>/<name>` and runs it with 1 and with 4
/// threads, asserting each run exits with `code` and prints `stdout`. Returns
/// the 4-thread run's stderr.
fn same_on_one_and_four_threads(name: &str, source: &str, code: i32, stdout: &str) -> String {
    let exe = compile(name, source);
    let mut stderr = String::new();
    for threads in ["1", "4"] {
        let (got_code, got_stdout, got_stderr) = run_on(&exe, threads);
        stderr = got_stderr;
        assert_eq!(got_code, Some(code), "TYPELISP_THREADS={}: stderr was: {}", threads, stderr);
        assert_eq!(got_stdout, stdout, "TYPELISP_THREADS={}: stderr was: {}", threads, stderr);
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
               (dotimes (i 8) (task (bump m wg 10000)))
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
               (dotimes (i 8) (task (churn-into results 200)))
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
               (dotimes (i 4) (task (produce ch wg (+ 100 i))))
               (task (close-when-done wg ch))
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
               (task (numbers a 1000))
               (add wg 3)
               (dotimes (i 3) (task (square a b wg)))
               (task (close-when-done wg b))
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
               (dotimes (i 8) (task (boom i)))
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
               (let ((t (task (stuck ch))))
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
           (defvar (big Task<int>) (task (count-to 100000)))
           (defvar (small Task<int>) (task (count-to 1000)))
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

// ---- which thread a task ran on (Phase 4) ------------------------------------

/// Sixty-four tasks that each do some work and then report the OS thread they
/// finished on. With four threads the reports name more than one thread —
/// the language-level view of what `sched_threads_test` observes in Rust —
/// and with one they all name the thread `main` runs on.
///
/// The only test here whose answer depends on the thread count, and the only
/// one that leans on timing: sixty-four tasks, all ready at once, with three
/// idle workers woken by the first of them, finishing on one thread would take
/// the workers never getting the lock in the time the main thread spends on
/// sixty-four loops of work.
#[test]
fn tasks_report_the_os_thread_they_ran_on() {
    let exe = compile(
        "threads_current_id",
        r#"(defun work-then-report ((out Chan<int>)) ()
             (let ((acc 0))
               (dotimes (i 20000) (setf acc (+ acc (mod i 7))))
               (send out (Thread::current-id))))
           (defun main () int
             (let ((out (the Chan<int> (Chan::new 64)))
                   (me (Thread::current-id))
                   (seen (the Vector<int> (Vector::new))))
               (dotimes (i 64) (task (work-then-report out)))
               (dotimes (i 64)
                 (let ((id (unwrap (recv out))))
                   (match (find id (iter seen))
                     ((some x) ())
                     ((none) (push seen id)))))
               (println "~a ~a" (len seen) (if (eq (len seen) 1) (eq (get seen 0) me) true))
               0))"#,
    );
    let (code, out, err) = run_on(&exe, "1");
    assert_eq!(code, Some(0), "stderr was: {}", err);
    assert_eq!(out, "1 true\n", "on one thread every task runs where `main` does; stderr was: {}", err);
    let (code, out, err) = run_on(&exe, "4");
    assert_eq!(code, Some(0), "stderr was: {}", err);
    let distinct: usize = out.split_whitespace().next().and_then(|n| n.parse().ok()).expect("a count");
    assert!((2..=4).contains(&distinct), "expected the tasks on 2 to 4 threads, got {:?}; stderr was: {}", out, err);
}

/// (f) A `thread` runs on an OS thread of its own: not the one `main` runs on,
/// and none that runs `task` tasks — on one thread and on four.
#[test]
fn a_thread_runs_on_an_os_thread_of_its_own() {
    same_on_one_and_four_threads(
        "threads_own_thread",
        r#"(defun whoami () int (Thread::current-id))
           (defun main () int
             (let* ((me (Thread::current-id))
                    (th (thread (whoami)))
                    (tasks (the Vector<Task<int>> (Vector::new))))
               (dotimes (i 16) (push tasks (task (whoami))))
               (let ((other (join th))
                     (clash false))
                 (doiter (t (iter tasks)) (if (eq (wait t) other) (setf clash true) clash))
                 (println "~a ~a" (eq other me) clash))
               0))"#,
        0,
        "false false\n",
    );
}

/// (g) `join` answers with what the call returned, and asking again gives the
/// same answer. `Thread::spawn` is the same thing for a closure.
#[test]
fn join_answers_with_the_result_every_time_it_is_asked() {
    same_on_one_and_four_threads(
        "threads_join",
        r#"(defun square ((n int)) int (* n n))
           (defun greeting ((name string)) string (format false "hello, ~a" name))
           (defun main () int
             (let ((a (thread (square 7)))
                   (b (thread (greeting "thread")))
                   (c (Thread::spawn (lambda () int (+ 40 2)))))
               (println "~a ~a ~a" (join a) (join a) (join b))
               (println "~a ~a" (join c) (join c))
               (println "~a" (> (Thread::available-parallelism) 0))
               0))"#,
        0,
        "49 49 hello, thread\n42 42\ntrue\n",
    );
}

/// (h) A thread that blocks in a C call holds up only itself. While it sleeps
/// in `usleep`, `main` and a task hand a hundred values back and forth — on
/// one thread too, because the thread in `usleep` is not that one — and only
/// then does the sleeper report in.
#[test]
fn a_thread_blocked_in_a_c_call_holds_up_no_task() {
    same_on_one_and_four_threads(
        "threads_blocking_ffi",
        r#"(defffi (c-usleep "usleep") (u32) i32)
           (defun sleeper ((done Chan<()>)) int
             (progn (unsafe (c-usleep (as u32 500000))) (send done ()) 1))
           (defun echo ((in Chan<int>) (out Chan<int>)) ()
             (loop (match (recv in)
                     ((none) (break))
                     ((some v) (send out (+ v 1))))))
           (defun main () int
             (let* ((done (the Chan<()> (Chan::new 1)))
                    (th (thread (sleeper done)))
                    (to (the Chan<int> (Chan::new 0)))
                    (from (the Chan<int> (Chan::new 0)))
                    (n 0))
               (task (echo to from))
               (dotimes (i 100) (send to n) (setf n (unwrap (recv from))))
               (close to)
               (println "~a ~a" n (len done))
               (println "~a" (join th))
               0))"#,
        0,
        "100 0\n1\n",
    );
}

/// (i) Code on a `thread` can start tasks and talk to the rest of the program
/// over channels, as any task can.
#[test]
fn a_thread_can_start_tasks_and_send() {
    same_on_one_and_four_threads(
        "threads_start_tasks_from_thread",
        r#"(defun double-into ((out Chan<int>) (n int)) () (send out (* 2 n)))
           (defun fan-out ((out Chan<int>) (n int)) int
             (progn (dotimes (i n) (task (double-into out i))) n))
           (defun main () int
             (let* ((out (the Chan<int> (Chan::new 0)))
                    (th (thread (fan-out out 10)))
                    (sum 0))
               (dotimes (i 10) (setf sum (+ sum (unwrap (recv out)))))
               (println "~a ~a" sum (join th))
               0))"#,
        0,
        "90 10\n",
    );
}

/// A `thread` that panics stops the program, as a task's panic does.
#[test]
fn a_panic_on_a_thread_stops_the_program() {
    let err = same_on_one_and_four_threads(
        "threads_thread_panic",
        r#"(defun boom () int (panic "boom on a thread"))
           (defun main () int
             (let ((never (the Chan<int> (Chan::new 0))))
               (thread (boom))
               (recv never)
               0))"#,
        1,
        "",
    );
    assert!(err.contains("panic: boom on a thread"), "stderr was: {}", err);
}
