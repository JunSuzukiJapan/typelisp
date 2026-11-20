//! `typl -c --target wasm32-wasip1`: a WASI module built from the same
//! program a native executable is, run under Node's WASI
//! (`scripts/run-wasi.mjs`) and compared with what the native one does, and
//! the programs it refuses at compile time.
//!
//! Needs the WebAssembly runtime built (`scripts/build-wasm-runtime.sh`, which
//! the test scripts run) and `node` on `PATH`.
//!
//! Scratch files go under `target/aot-test-tmp/`, one name per test, for the
//! reason `tests/compile_file_test.rs`'s module doc comment gives.

use std::path::PathBuf;
use std::process::{Command, Output};

const TYPL: &str = env!("CARGO_BIN_EXE_typl");

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(name);
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    dir
}

/// Compiles `source` with `flags` after `-c`: `typl`'s own output, and the
/// output's path.
fn build(name: &str, source: &str, output: &str, flags: &[&str]) -> (Output, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("prog.typl");
    let out_path = dir.join(output);
    std::fs::write(&src, source).unwrap();
    let out = Command::new(TYPL)
        .args(["-c", src.to_str().unwrap(), "-o", out_path.to_str().unwrap()])
        .args(flags)
        .output()
        .expect("failed to start the typl binary");
    (out, out_path)
}

fn built(name: &str, source: &str, output: &str, flags: &[&str]) -> PathBuf {
    let (out, path) = build(name, source, output, flags);
    assert!(out.status.success(), "typl -c {:?} failed: {}", flags, String::from_utf8_lossy(&out.stderr));
    path
}

/// Runs the WASI module at `wasm` with `args`.
fn run_wasm(wasm: &PathBuf, args: &[&str]) -> Output {
    let runner = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts").join("run-wasi.mjs");
    Command::new("node")
        .arg("--no-warnings")
        .arg(runner)
        .arg(wasm)
        .args(args)
        .output()
        .expect("failed to start node (the wasm32-wasip1 tests run modules under Node's WASI)")
}

/// What `source` prints and exits with as a WASI module, after checking that
/// the native executable prints and exits the same.
fn same_as_native(name: &str, source: &str, flags: &[&str]) -> (String, String, Option<i32>) {
    let mut wasm_flags = vec!["--target", "wasm32-wasip1"];
    wasm_flags.extend_from_slice(flags);
    let wasm = built(name, source, "prog.wasm", &wasm_flags);
    let native = built(name, source, "prog", flags);
    let w = run_wasm(&wasm, &[]);
    let n = Command::new(&native).output().expect("failed to run the native executable");
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    assert_eq!(text(&w.stdout), text(&n.stdout), "stdout differs from the native executable's");
    assert_eq!(text(&w.stderr), text(&n.stderr), "stderr differs from the native executable's");
    assert_eq!(w.status.code(), n.status.code(), "exit code differs from the native executable's");
    (text(&w.stdout), text(&w.stderr), w.status.code())
}

/// What `typl -c --target wasm32-wasip1` said when it refused `source`.
fn refused(name: &str, source: &str) -> String {
    let (out, _) = build(name, source, "prog.wasm", &["--target", "wasm32-wasip1"]);
    assert_eq!(out.status.code(), Some(1), "typl -c built it: {}", String::from_utf8_lossy(&out.stdout));
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn hello_world_prints_and_exits_as_natively() {
    let (out, _, code) = same_as_native(
        "wasm_hello",
        r#"(defun main () () (println "hello ~a" 42))"#,
        &[],
    );
    assert_eq!(out, "hello 42\n");
    assert_eq!(code, Some(0));
}

/// `main`'s `int` is the exit code there too.
#[test]
fn main_s_answer_is_the_exit_code() {
    let (_, _, code) = same_as_native("wasm_exit_code", "(defun main () int 3)", &[]);
    assert_eq!(code, Some(3));
}

/// A throw crosses compiled frames to its catch, and an uncaught panic ends
/// the program the native way: both are status words, not Rust unwinding,
/// which WebAssembly has not got.
#[test]
fn a_throw_reaches_its_catch_and_a_panic_ends_the_program() {
    let (out, err, code) = same_as_native(
        "wasm_unwind",
        r#"
(defun deep ((n int)) int (throw 'done (* n 2)))
(defun middle ((n int)) int (+ 1 (deep n)))
(defun main () ()
  (println "~a" (catch 'done (middle 21)))
  (panic "boom"))
"#,
        &[],
    );
    assert_eq!(out, "42\n");
    assert!(err.starts_with("panic: "), "stderr: {}", err);
    assert_eq!(code, Some(1));
}

/// Integers past a fixnum, ratios, deep recursion, and a vector.
#[test]
fn arithmetic_recursion_and_vectors_run_as_natively() {
    let (out, _, _) = same_as_native(
        "wasm_arith",
        r#"
(defun fact ((n int)) int (if (= n 0) 1 (* n (fact (- n 1)))))
(defun depth ((n int)) int (if (= n 0) 0 (+ 1 (depth (- n 1)))))
(defun main () ()
  (println "~a" (fact 30))
  (println "~a" (/ 1 3))
  (println "~a" (depth 100000))
  (let ((v (the Vector<int> (Vector::new))))
    (push v 1) (push v 2)
    (println "~a" (get v 1))))
"#,
        &[],
    );
    assert_eq!(out, "265252859812191058636308480000000\n1/3\n100000\n2\n");
}

/// Tasks, a channel between them, and `sleep`, on the one thread there is.
#[test]
fn tasks_channels_and_sleep_run_on_one_thread() {
    let (out, _, _) = same_as_native(
        "wasm_tasks",
        r#"
(use std::sync::Chan)
(defun fill ((ch Chan<int>)) () (send ch 7) ())
(defun main () ()
  (let ((ch (the Chan<int> (Chan::new 0))))
    (task (fill ch))
    (sleep 0.01)
    (println "~a" (unwrap (recv ch)))))
"#,
        &[],
    );
    assert_eq!(out, "7\n");
}

/// `eval` brings the interpreter into the module, as it does into an
/// executable.
#[test]
fn eval_runs_in_the_module() {
    let (out, _, _) = same_as_native("wasm_eval", r#"(defun main () () (println "~a" (eval (quote (+ 2 3)))))"#, &[]);
    assert_eq!(out, "(Ok 5)\n");
}

/// A file written and read back through WASI's file system.
#[test]
fn a_file_is_written_and_read_back() {
    let dir = scratch("wasm_file");
    let file = dir.join("out.txt");
    let source = format!(
        r#"
(use std::fs::write-file-string std::fs::read-file-lines)
(defun main () ()
  (unwrap (write-file-string "{0}" "file ok\n"))
  (println "~a" (read-file-lines "{0}")))
"#,
        file.display()
    );
    let (out, _, _) = same_as_native("wasm_file", &source, &[]);
    assert_eq!(out, "(Ok #(file ok))\n");
}

/// `--max-memory` bounds a module's allocations as it does an executable's.
#[test]
fn an_allocation_past_the_memory_limit_is_a_runtime_error() {
    let wasm = built(
        "wasm_max_memory",
        "(defun main () int (if (= (len (Vector::filled 2000000 0)) 2000000) 0 2))",
        "prog.wasm",
        &["--target", "wasm32-wasip1", "--max-memory", "1M"],
    );
    let out = run_wasm(&wasm, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("memory limit exceeded"), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

/// The module's arguments are its `command-line-args`, after its own name.
#[test]
fn command_line_args_are_the_module_s_arguments() {
    let wasm = built(
        "wasm_args",
        r#"(use std::env::command-line-args) (defun main () () (println "~a" (len (command-line-args))))"#,
        "prog.wasm",
        &["--target", "wasm32-wasip1"],
    );
    let out = run_wasm(&wasm, &["a", "b"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "3\n");
}

#[test]
fn a_socket_is_refused_at_compile_time_with_the_chain_that_reaches_it() {
    let err = refused(
        "wasm_refuse_socket",
        r#"
(use std::net::tcp-listen)
(defun main () ()
  (match (tcp-listen "127.0.0.1" 0) ((Ok _) (println "ok")) ((Err _) (println "err"))))
"#,
    );
    assert!(err.contains("--target wasm32-wasip1: the program uses what the target does not have:"), "{}", err);
    assert!(err.contains("a socket (reached by main -> prog::main -> std::net::tcp-listen"), "{}", err);
}

#[test]
fn an_os_thread_is_refused_at_compile_time() {
    let err = refused(
        "wasm_refuse_thread",
        r#"
(use std::thread::Thread)
(defun main () () (println "~a" (join (Thread::spawn (lambda () int 1)))))
"#,
    );
    assert!(err.contains("an OS thread (`thread`)"), "{}", err);
}

/// A C function is the host's, not the module's.
#[test]
fn a_c_function_is_refused_at_compile_time() {
    let err = refused(
        "wasm_refuse_ffi",
        r#"
(defffi (c-abs "abs") (i32) i32)
(defun main () int (as int (unsafe (c-abs -7))))
"#,
    );
    assert!(err.contains("the C function `abs`"), "{}", err);
}

#[test]
fn an_unknown_target_is_refused() {
    let (out, _) = build("wasm_unknown_target", "(defun main () int 0)", "prog", &["--target", "riscv64"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--target: unknown target `riscv64`"));
}
