//! The resources an executable built by `typl -c` may use:
//! `--heap-cells N` fixes its cons arena at `N` cells, and
//! `--max-memory SIZE` bounds what the whole process allocates,
//! `--frame-memory SIZE` what the calls in progress of each task take, and
//! `--no-cons` refuses at compile time a program that would take a cons
//! cell (`compile::aot::BuildOptions`). Driven through the `typl` binary, since
//! the flags are the CLI's, and checked by running what it builds.
//!
//! Scratch files go under `target/aot-test-tmp/`, one name per test, for the
//! reason `tests/compile_file_test.rs`'s module doc comment gives.

use std::path::PathBuf;
use std::process::{Command, Output};

const TYPL: &str = env!("CARGO_BIN_EXE_typl");

/// Keeps a list of `(arg 1)` cells alive until it answers, so the arena
/// must hold them all at once.
const LIVE_LIST: &str = r#"
(defun build ((n int) (acc Option<Sexpr>)) Option<Sexpr>
  (if (= n 0) acc (build (- n 1) (Some (Sexpr::Cons (Sexpr::Int n) acc)))))
(defun main () int
  (if (is-some (build 1000 ())) 0 2))
"#;

/// Allocates a vector of two million words.
const BIG_VECTOR: &str = r#"
(defun main () int
  (if (= (len (Vector::filled 2000000 0)) 2000000) 0 2))
"#;

/// Compiles `source` with `flags` after `-c`: `typl`'s own output, and the
/// executable's path.
fn build(name: &str, source: &str, flags: &[&str]) -> (Output, PathBuf) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join(name);
    std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
    let src = dir.join("prog.typl");
    let exe = dir.join("prog");
    std::fs::write(&src, source).unwrap();
    let out = Command::new(TYPL)
        .args(["-c", src.to_str().unwrap(), "-o", exe.to_str().unwrap()])
        .args(flags)
        .output()
        .expect("failed to start the typl binary");
    (out, exe)
}

/// Compiles `source` with `flags` after `-c` and runs the executable.
fn build_and_run(name: &str, source: &str, flags: &[&str]) -> Output {
    let (out, exe) = build(name, source, flags);
    assert!(out.status.success(), "typl -c failed: {}", String::from_utf8_lossy(&out.stderr));
    Command::new(&exe).output().expect("failed to run the compiled executable")
}

/// What `typl -c` said when it refused to build `source`.
fn refused(name: &str, source: &str, flags: &[&str]) -> String {
    let (out, _) = build(name, source, flags);
    assert_eq!(out.status.code(), Some(1), "typl -c built it: {}", String::from_utf8_lossy(&out.stdout));
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn heap_cells_that_hold_the_live_set_run_the_program() {
    let run = build_and_run("limits-cells-enough", LIVE_LIST, &["--heap-cells", "5000"]);
    assert_eq!(run.status.code(), Some(0), "{}", String::from_utf8_lossy(&run.stderr));
}

/// The arena is fixed at what was given: it does not grow to hold the list.
#[test]
fn heap_cells_below_the_live_set_exhaust_the_arena() {
    let run = build_and_run("limits-cells-short", LIVE_LIST, &["--heap-cells", "100"]);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("heap exhausted"), "{}", String::from_utf8_lossy(&run.stderr));
}

#[test]
fn a_memory_limit_above_the_programs_use_runs_it() {
    let run = build_and_run("limits-memory-enough", BIG_VECTOR, &["--max-memory", "64M"]);
    assert_eq!(run.status.code(), Some(0), "{}", String::from_utf8_lossy(&run.stderr));
}

#[test]
fn an_allocation_past_the_memory_limit_is_a_runtime_error() {
    let run = build_and_run("limits-memory-short", BIG_VECTOR, &["--max-memory=8M"]);
    assert_eq!(run.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&run.stderr).contains("memory limit exceeded"), "{}", String::from_utf8_lossy(&run.stderr));
}

/// A bound smaller than the cons arena itself refuses the arena, before
/// the program starts.
#[test]
fn a_memory_limit_below_the_arena_stops_at_startup() {
    let run = build_and_run("limits-memory-arena", BIG_VECTOR, &["--max-memory", "100K"]);
    assert_eq!(run.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&run.stderr).contains("memory limit exceeded"), "{}", String::from_utf8_lossy(&run.stderr));
}

/// A recursion 100000 deep: its calls' records are on the heap, and grow
/// with it.
const DEEP_RECURSION: &str = r#"
(defun depth ((n int)) int (if (= n 0) 0 (+ 1 (depth (- n 1)))))
(defun main () int (println "~a" (depth 100000)) 0)
"#;

#[test]
fn calls_within_the_frame_memory_run() {
    let run = build_and_run("limits-frames-enough", DEEP_RECURSION, &["--frame-memory", "64M"]);
    assert_eq!(String::from_utf8_lossy(&run.stdout), "100000\n");
    assert_eq!(run.status.code(), Some(0), "{}", String::from_utf8_lossy(&run.stderr));
}

/// A call that would take the records past the bound ends the program,
/// saying so, before the recursion has taken more.
#[test]
fn a_call_past_the_frame_memory_ends_the_program() {
    let run = build_and_run("limits-frames-short", DEEP_RECURSION, &["--frame-memory", "64K"]);
    assert_eq!(String::from_utf8_lossy(&run.stdout), "");
    assert_eq!(run.status.code(), Some(1));
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(err.contains("typelisp runtime error: the calls in progress take more than the 65536 bytes --frame-memory allows a task ("), "{}", err);
}

#[test]
fn a_frame_memory_is_a_size() {
    let err = refused("limits-frames-bad", DEEP_RECURSION, &["--frame-memory", "deep"]);
    assert!(err.contains("--frame-memory: `deep` is not a positive size"), "{}", err);
}

/// Printing with arguments, arithmetic, strings, vectors, a hash table and
/// a `select` over channels, all without a cons cell: the executable runs
/// with an empty arena.
#[test]
fn a_program_without_cons_cells_builds_and_runs_without_an_arena() {
    let run = build_and_run(
        "limits-no-cons-runs",
        r#"
(defun fib ((n int)) int (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
(defun main () ()
  (let ((v (Vector::filled 3 1))
        (h (the HashTable<int, int> (HashTable::new)))
        (c (the Chan<int> (Chan::new 1))))
    (set h 1 2)
    (send c 5)
    (select ((x (recv c)) (println "got ~a" x)))
    (println "fib ~a, ~a ~a, ~a" (fib 15) (len v) (get h 1) (upcase "ok"))))
"#,
        &["--no-cons"],
    );
    assert_eq!(run.status.code(), Some(0), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "got (Some 5)\nfib 610, 3 (Some 2), OK\n");
}

/// The error names the operation and the functions it is reached by.
#[test]
fn a_program_that_conses_is_refused_with_the_chain_that_reaches_the_cons() {
    let err = refused("limits-no-cons-refused", LIVE_LIST, &["--no-cons"]);
    assert!(err.contains("--no-cons"), "{}", err);
    assert!(err.contains("building a `Sexpr` cons (reached by main -> prog::main -> prog::build)"), "{}", err);
}

#[test]
fn a_quoted_list_is_a_use_of_cons_cells() {
    let err = refused("limits-no-cons-quote", "(defun main () () (println \"~a\" '(1 2 3)))\n", &["--no-cons"]);
    assert!(err.contains("--no-cons: the program uses cons cells"), "{}", err);
}

#[test]
fn no_cons_with_heap_cells_is_refused() {
    let err = refused("limits-no-cons-cells", BIG_VECTOR, &["--no-cons", "--heap-cells", "100"]);
    assert!(err.contains("cannot be combined"), "{}", err);
}
