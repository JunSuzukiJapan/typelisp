//! The resources an executable built by `typl -c` may use:
//! `--heap-cells N` fixes its cons arena at `N` cells, and
//! `--max-memory SIZE` bounds what the whole process allocates
//! (`compile::aot::BuildOptions`). Driven through the `typl` binary, since
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

/// Compiles `source` with `flags` after `-c` and runs the executable.
fn build_and_run(name: &str, source: &str, flags: &[&str]) -> Output {
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
    assert!(out.status.success(), "typl -c failed: {}", String::from_utf8_lossy(&out.stderr));
    Command::new(&exe).output().expect("failed to run the compiled executable")
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
