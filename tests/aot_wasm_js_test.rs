//! `typl -c --target wasm32-unknown-unknown`: a WebAssembly module whose host
//! is JavaScript, and the loader written beside it (`prog.mjs`). Run under
//! Node, from the command line and through the loader's `run`, and compared
//! with what the native executable built from the same program does; and the
//! programs it refuses at compile time.
//!
//! Needs the WebAssembly runtime built (`scripts/build-wasm-runtime.sh`, which
//! the test scripts run) and `node` on `PATH`.
//!
//! Scratch files go under `target/aot-test-tmp/`, one name per test, for the
//! reason `tests/compile_file_test.rs`'s module doc comment gives.

use std::path::PathBuf;
use std::process::{Command, Output};

const TYPL: &str = env!("CARGO_BIN_EXE_typl");
const TARGET: [&str; 2] = ["--target", "wasm32-unknown-unknown"];

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(name);
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    dir
}

/// Compiles `source` with `flags` after `-c` into `output`: `typl`'s own
/// output, and the output's path.
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

/// The module `source` compiles to, with `flags` after the target.
fn module(name: &str, source: &str, flags: &[&str]) -> PathBuf {
    let mut all = TARGET.to_vec();
    all.extend_from_slice(flags);
    built(name, source, "prog.wasm", &all)
}

/// Runs the loader beside `wasm` with Node, as `node prog.mjs`.
fn run_loader(wasm: &PathBuf) -> Output {
    Command::new("node")
        .arg(wasm.with_extension("mjs"))
        .output()
        .expect("failed to start node (the wasm32-unknown-unknown tests run the loader under Node)")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// What `source` prints and exits with as a module, after checking that the
/// native executable prints and exits the same.
fn same_as_native(name: &str, source: &str) -> (String, String, Option<i32>) {
    let wasm = module(name, source, &[]);
    let native = built(name, source, "prog", &[]);
    let w = run_loader(&wasm);
    let n = Command::new(&native).output().expect("failed to run the native executable");
    assert_eq!(text(&w.stdout), text(&n.stdout), "stdout differs from the native executable's");
    assert_eq!(text(&w.stderr), text(&n.stderr), "stderr differs from the native executable's");
    assert_eq!(w.status.code(), n.status.code(), "exit code differs from the native executable's");
    (text(&w.stdout), text(&w.stderr), w.status.code())
}

/// What `typl -c --target wasm32-unknown-unknown` said when it refused
/// `source`.
fn refused(name: &str, source: &str) -> String {
    let (out, _) = build(name, source, "prog.wasm", &TARGET);
    assert_eq!(out.status.code(), Some(1), "typl -c built it: {}", text(&out.stdout));
    text(&out.stderr)
}

#[test]
fn hello_world_prints_and_exits_as_natively() {
    let (out, _, code) = same_as_native("wasmjs_hello", r#"(defun main () () (println "hello ~a" 42))"#);
    assert_eq!(out, "hello 42\n");
    assert_eq!(code, Some(0));
}

/// The module imports only what the loader gives it.
#[test]
fn the_module_imports_only_the_loader_s_functions() {
    let wasm = module("wasmjs_imports", r#"(defun main () () (println "x"))"#, &[]);
    let script = format!(
        "const m = await WebAssembly.compile(require('fs').readFileSync({:?})); \
         console.log(WebAssembly.Module.imports(m).map(i => i.module + '.' + i.name).sort().join(' '))",
        wasm.to_str().unwrap()
    );
    let out = Command::new("node")
        .args(["--input-type=commonjs", "-e", &format!("(async () => {{ {} }})()", script)])
        .output()
        .expect("failed to start node");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "typelisp.exit typelisp.now typelisp.sleep typelisp.unix_time typelisp.write\n");
}

#[test]
fn main_s_answer_and_exit_are_the_exit_code() {
    let (_, _, code) = same_as_native("wasmjs_main_code", "(defun main () int 3)");
    assert_eq!(code, Some(3));
    let (out, _, code) = same_as_native(
        "wasmjs_exit",
        r#"(use std::process::exit) (defun main () () (println "before") (exit 5) (println "after"))"#,
    );
    assert_eq!(out, "before\n");
    assert_eq!(code, Some(5));
}

#[test]
fn a_throw_reaches_its_catch_and_a_panic_ends_the_program() {
    let (out, err, code) = same_as_native(
        "wasmjs_unwind",
        r#"
(defun deep ((n int)) int (throw 'done (* n 2)))
(defun middle ((n int)) int (+ 1 (deep n)))
(defun main () ()
  (println "~a" (catch 'done (middle 21)))
  (panic "boom"))
"#,
    );
    assert_eq!(out, "42\n");
    assert_eq!(err, "panic: boom\n");
    assert_eq!(code, Some(1));
}

/// Tasks, a channel, and `sleep` (the loader's).
#[test]
fn tasks_and_sleep_run_as_natively() {
    let (out, _, _) = same_as_native(
        "wasmjs_tasks",
        r#"
(use std::sync::Chan)
(defun fill ((ch Chan<int>)) () (send ch 7) ())
(defun main () ()
  (let ((ch (the Chan<int> (Chan::new 0))))
    (task (fill ch))
    (sleep 0.01)
    (println "~a" (unwrap (recv ch)))))
"#,
    );
    assert_eq!(out, "7\n");
}

/// The time is the host's: `get-universal-time` reads the loader's clock.
#[test]
fn the_universal_time_is_the_host_s() {
    let wasm = module(
        "wasmjs_time",
        r#"(use std::time::get-universal-time) (defun main () () (println "~a" (get-universal-time)))"#,
        &[],
    );
    let out = run_loader(&wasm);
    let secs: i64 = text(&out.stdout).trim().parse().unwrap_or_else(|_| panic!("not a number: {}", text(&out.stdout)));
    // 2023-09-25 in CL's universal time: any clock running today is past it.
    assert!(secs > 3_904_000_000, "{}", secs);
}

/// The local time zone is the host's: with no zone, `decode-universal-time`
/// asks the loader, which reads JavaScript's `Date`. Run with the same `TZ`
/// as the native executable, the two decode a winter and a summer instant
/// (daylight saving in New York) the same.
#[test]
fn the_local_time_zone_is_the_host_s() {
    let source = r#"
(use std::time::decode-universal-time)
(defun main () ()
  ;; 2026-01-15 and 2026-07-15, 12:00 UTC.
  (println "~a" (decode-universal-time (make-instance 'universal-time :day 46035 :second 43200)))
  (println "~a" (decode-universal-time (make-instance 'universal-time :day 46216 :second 43200)))
  (println "~a" (decode-universal-time (make-instance 'universal-time :day 46216 :second 43200) 0)))
"#;
    let wasm = module("wasmjs_local_zone", source, &[]);
    let native = built("wasmjs_local_zone", source, "prog", &[]);
    let w = Command::new("node").arg(wasm.with_extension("mjs")).env("TZ", "America/New_York").output().expect("failed to start node");
    let n = Command::new(&native).env("TZ", "America/New_York").output().expect("failed to run the native executable");
    assert!(w.status.success(), "{}", text(&w.stderr));
    assert_eq!(text(&w.stdout), text(&n.stdout));
    assert_eq!(
        text(&w.stdout),
        "#<decoded-time 2026-01-15 07:00:00 -05:00 Thu>\n\
         #<decoded-time 2026-07-15 08:00:00 -04:00 Wed dst>\n\
         #<decoded-time 2026-07-15 12:00:00 +00:00 Wed>\n"
    );
}

#[test]
fn eval_and_arithmetic_run_in_the_module() {
    let (out, _, _) = same_as_native(
        "wasmjs_eval",
        r#"
(defun fact ((n int)) int (if (= n 0) 1 (* n (fact (- n 1)))))
(defun main () ()
  (println "~a" (fact 30))
  (println "~a" (eval (quote (+ 2 3)))))
"#,
    );
    assert_eq!(out, "265252859812191058636308480000000\n(Ok 5)\n");
}

#[test]
fn an_allocation_past_the_memory_limit_is_a_runtime_error() {
    let wasm = module(
        "wasmjs_max_memory",
        "(defun main () int (if (= (len (Vector::filled 2000000 0)) 2000000) 0 2))",
        &["--max-memory", "1M"],
    );
    let out = run_loader(&wasm);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("memory limit exceeded"), "stderr: {}", text(&out.stderr));
}

/// The loader's `run`, for a page or a program of its own: output to the
/// `write` it is given, and the exit code as its answer.
#[test]
fn run_gives_the_output_to_write_and_answers_the_exit_code() {
    let wasm = module(
        "wasmjs_run",
        r#"(defun main () int (println "out") (eprint "err") 4)"#,
        &[],
    );
    let loader = wasm.with_extension("mjs");
    let script = format!(
        "import {{ run }} from {:?}; let seen = ''; \
         const code = await run({{ write: (fd, text) => {{ seen += fd + ':' + text + '|'; }} }}); \
         console.log(JSON.stringify(seen), code);",
        loader.to_str().unwrap()
    );
    let out = Command::new("node").args(["--input-type=module", "-e", &script]).output().expect("failed to start node");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "\"1:out\\n|2:err|\" 4\n");
}

#[test]
fn a_file_is_refused_at_compile_time() {
    let err = refused(
        "wasmjs_refuse_file",
        r#"
(use std::fs::write-file-string)
(defun main () () (unwrap (write-file-string "out.txt" "x")))
"#,
    );
    assert!(err.contains("--target wasm32-unknown-unknown: the program uses what the target does not have:"), "{}", err);
    assert!(err.contains("opening a file (reached by main -> prog::main -> std::fs::write-file-string"), "{}", err);
}

/// What a WASI module is refused, this one is too.
#[test]
fn a_socket_is_refused_at_compile_time() {
    let err = refused(
        "wasmjs_refuse_socket",
        r#"
(use std::net::tcp-listen)
(defun main () ()
  (match (tcp-listen "127.0.0.1" 0) ((Ok _) (println "ok")) ((Err _) (println "err"))))
"#,
    );
    assert!(err.contains("a socket (reached by main -> prog::main -> std::net::tcp-listen"), "{}", err);
}

/// `-o` naming the module `.mjs` would put the loader over it.
#[test]
fn a_module_named_like_its_loader_is_refused() {
    let (out, _) = build("wasmjs_mjs_name", "(defun main () int 0)", "prog.mjs", &TARGET);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("which is the loader's name"), "{}", text(&out.stderr));
}
