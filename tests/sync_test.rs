//! `WaitGroup`, `Mutex<T>` and `with-lock` — Go's `sync` package, written in
//! the language rather than in Rust.
//!
//! Both are channels underneath, which is the point: a wait group's gate is
//! an unbuffered channel nobody sends on (closing it releases everyone parked
//! at once), and a mutex is a capacity-1 channel holding one token. A channel
//! already is a queue of waiters, so neither needed anything new.

extern crate typelisp;
use typelisp::{load_compiler, load_prelude, Checker, EvalError, Heap, Interp, Reader, Value};

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

fn int(src: &str) -> i64 {
    match run_with(src, false, false).expect("eval failed").1 {
        Value::Int(n) => n,
        other => panic!("expected an integer, got {:?}", other),
    }
}

fn int_stressed(src: &str) -> i64 {
    match run_with(src, false, true).expect("eval failed").1 {
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

fn text(src: &str) -> String {
    match run_with(src, false, false).expect("eval failed") {
        (h, Value::Str(id)) => h.string(id).to_string(),
        (_, other) => panic!("expected a string, got {:?}", other),
    }
}

fn err(src: &str) -> String {
    match run_with(src, false, false) {
        Ok(_) => panic!("expected the program to fail"),
        Err(e) => e.to_string(),
    }
}

// ---- WaitGroup -----------------------------------------------------------

/// The counted work all finishes before `wait` returns.
#[test]
fn a_wait_group_waits_for_every_counted_task() {
    assert_eq!(
        int(
            r#"(defvar (done-count int) 0)
               (defun work ((wg WaitGroup)) ()
                 (yield)
                 (setf done-count (+ done-count 1))
                 (done wg))
               (defun main () int
                 (let ((wg (the WaitGroup (WaitGroup::make))))
                   (add wg 3)
                   (dotimes (i 3) (go (work wg)))
                   (wait wg)
                   done-count))
               (main)"#
        ),
        3
    );
}

/// Waiting on a group that is already at zero returns at once — the counter
/// is tested before the gate, so a closed gate is never reached for nothing.
#[test]
fn waiting_on_a_finished_group_returns_at_once() {
    assert_eq!(
        int(
            r#"(defun main () int
                 (let ((wg (the WaitGroup (WaitGroup::make))))
                   (add wg 1)
                   (done wg)
                   (wait wg)
                   (wait wg)
                   7))
               (main)"#
        ),
        7
    );
}

/// A group that has finished can be counted up again, as Go's can: leaving
/// zero puts a fresh gate in place, so the second round's `done` does not
/// close the first round's gate a second time, and the second `wait` really
/// waits for the second round.
#[test]
fn a_finished_group_can_be_used_again() {
    assert_eq!(
        int(
            r#"(defvar (finished int) 0)
               (defun work ((wg WaitGroup)) ()
                 (yield)
                 (setf finished (+ finished 1))
                 (done wg))
               (defun main () int
                 (let ((wg (the WaitGroup (WaitGroup::make))))
                   (add wg 1)
                   (go (work wg))
                   (wait wg)
                   (add wg 2)
                   (go (work wg))
                   (go (work wg))
                   (wait wg)
                   finished))
               (main)"#
        ),
        3
    );
}

/// Closing the gate releases everyone parked on it, not just the first.
#[test]
fn several_tasks_can_wait_on_one_group() {
    assert_eq!(
        int(
            r#"(defvar (woke int) 0)
               (defun watcher ((wg WaitGroup)) ()
                 (wait wg)
                 (setf woke (+ woke 1))
                 ())
               (defun main () int
                 (let ((wg (the WaitGroup (WaitGroup::make))))
                   (add wg 1)
                   (go (watcher wg))
                   (go (watcher wg))
                   (go (watcher wg))
                   (yield)
                   (done wg)
                   (yield)
                   woke))
               (main)"#
        ),
        3
    );
}

/// One `done` too many is a misuse, and says so rather than wrapping around.
#[test]
fn one_done_too_many_panics() {
    let e = err(
        r#"(defun main () ()
             (let ((wg (the WaitGroup (WaitGroup::make))))
               (add wg 1)
               (done wg)
               (done wg)))
           (main)"#,
    );
    assert!(e.contains("below zero"), "got: {}", e);
}

/// So is a negative `add`.
#[test]
fn a_negative_add_panics() {
    let e = err(
        r#"(defun main () ()
             (let ((wg (the WaitGroup (WaitGroup::make))))
               (add wg -1)))
           (main)"#,
    );
    assert!(e.contains("below zero"), "got: {}", e);
}

/// `(wait wg)` and `(wait t)` are the same word on different receivers.
#[test]
fn wait_is_shared_with_the_task_handle() {
    assert_eq!(
        int(
            r#"(defun answer () int 42)
               (defun main () int
                 (let ((wg (the WaitGroup (WaitGroup::make)))
                       (t1 (go (answer))))
                   (wait wg)
                   (wait t1)))
               (main)"#
        ),
        42
    );
}

// ---- Mutex ---------------------------------------------------------------

/// The lock really excludes: each task reads, yields, and writes back, which
/// loses updates without one and does not with it.
#[test]
fn a_mutex_keeps_a_read_modify_write_whole() {
    assert_eq!(
        int(
            r#"(defun bump ((m Mutex<int>) (wg WaitGroup)) ()
                 (with-lock (n m)
                   (let ((seen n))
                     (yield)
                     (setf n (+ seen 1))))
                 (done wg))
               (defun main () int
                 (let ((m (the Mutex<int> (Mutex::make 0)))
                       (wg (the WaitGroup (WaitGroup::make))))
                   (add wg 4)
                   (dotimes (i 4) (go (bump m wg)))
                   (wait wg)
                   (with-lock (n m) n)))
               (main)"#
        ),
        4
    );
}

/// `x` is the place, not a copy: `(setf x ...)` writes into the mutex, which
/// is what `symbol-macrolet` is for.
#[test]
fn the_alias_is_a_place() {
    assert_eq!(
        int(
            r#"(defun main () int
                 (let ((m (the Mutex<int> (Mutex::make 5))))
                   (with-lock (n m) (setf n (* n 3)))
                   (with-lock (n m) n)))
               (main)"#
        ),
        15
    );
}

/// The lock comes back however the body is left — here through a `throw`,
/// which the `unwind-protect` cleanup catches on the way past.
#[test]
fn the_lock_is_released_when_the_body_throws() {
    assert_eq!(
        int(
            r#"(defun main () int
                 (let ((m (the Mutex<int> (Mutex::make 1))))
                   (catch 'out (with-lock (n m) (throw 'out 9)))
                   ;; Still lockable: the cleanup put the token back.
                   (with-lock (n m) (+ n 1))))
               (main)"#
        ),
        2
    );
}

/// And through a `return-from`, the static exit.
#[test]
fn the_lock_is_released_on_a_static_exit() {
    assert_eq!(
        int(
            r#"(defun leave ((m Mutex<int>)) int
                 (with-lock (n m) (return-from leave 3))
                 0)
               (defun main () int
                 (let ((m (the Mutex<int> (Mutex::make 1))))
                   (leave m)
                   (with-lock (n m) (+ n 10))))
               (main)"#
        ),
        11
    );
}

/// Giving back a lock nobody took is a misuse, not a silent park.
#[test]
fn unlocking_an_unlocked_mutex_panics() {
    let e = err(
        r#"(defun main () ()
             (let ((m (the Mutex<int> (Mutex::make 1))))
               (unlock m)))
           (main)"#,
    );
    assert!(e.contains("not locked"), "got: {}", e);
}

/// The temporary `with-lock` binds is `gensym`-fresh, so a body that uses its
/// own `g`/`m`/`tmp` is untouched. The alias is built with `Sexpr::path`
/// precisely so the name can be one nothing else can write.
#[test]
fn the_expansion_does_not_capture_the_bodys_names() {
    assert_eq!(
        int(
            r#"(defun main () int
                 (let ((m (the Mutex<int> (Mutex::make 4)))
                       (g 100)
                       (tmp 20))
                   (with-lock (n m) (+ n (+ g tmp)))))
               (main)"#
        ),
        124
    );
}

/// The mutex expression is evaluated exactly once, however many times the
/// alias is written.
#[test]
fn the_mutex_expression_is_evaluated_once() {
    assert_eq!(
        int(
            r#"(defvar (calls int) 0)
               (defun the-mutex ((m Mutex<int>)) Mutex<int>
                 (setf calls (+ calls 1))
                 m)
               (defun main () int
                 (let ((m (the Mutex<int> (Mutex::make 0))))
                   (with-lock (n (the-mutex m)) (setf n (+ n 1)) (setf n (+ n 1)))
                   calls))
               (main)"#
        ),
        1
    );
}

/// A mutex can hold something bigger than a machine word, and the place
/// still names the field rather than a copy.
#[test]
fn a_mutex_can_hold_a_heap_value() {
    assert_eq!(
        text(
            r#"(defun main () string
                 (let ((m (the Mutex<string> (Mutex::make "a"))))
                   (with-lock (s m) (setf s (append s "b")))
                   (with-lock (s m) (append s "!"))))
               (main)"#
        ),
        "ab!"
    );
}

// ---- compiled ------------------------------------------------------------

/// The same program with the bodies compiled. `lock`/`unlock`/`add`/`done`
/// are prelude methods over channel operations, so each is a suspension in
/// compiled code too.
#[test]
fn compiled_bodies_agree_with_the_interpreter() {
    let src = r#"(defun bump ((m Mutex<int>) (wg WaitGroup)) ()
                   (with-lock (n m)
                     (let ((seen n))
                       (yield)
                       (setf n (+ seen 1))))
                   (done wg))
                 (defun main () int
                   (let ((m (the Mutex<int> (Mutex::make 0)))
                         (wg (the WaitGroup (WaitGroup::make))))
                     (add wg 4)
                     (dotimes (i 4) (go (bump m wg)))
                     (wait wg)
                     (with-lock (n m) n)))"#;
    assert_eq!(int(&format!("{}\n(main)", src)), 4);
    assert_eq!(int_compiled(&format!("{}\n(compile bump)\n(compile main)\n(main)", src)), 4);
}

// ---- the collector -------------------------------------------------------

/// The gate channels, the tokens and the guarded value all survive a
/// collection at every allocation.
#[test]
fn the_sync_types_survive_gc_stress() {
    assert_eq!(
        int_stressed(
            r#"(defun bump ((m Mutex<string>) (wg WaitGroup)) ()
                 (with-lock (s m) (setf s (append s "x")))
                 (done wg))
               (defun main () int
                 (let ((m (the Mutex<string> (Mutex::make "")))
                       (wg (the WaitGroup (WaitGroup::make))))
                   (add wg 5)
                   (dotimes (i 5) (go (bump m wg)))
                   (wait wg)
                   (with-lock (s m) (length s))))
               (main)"#
        ),
        5
    );
}
