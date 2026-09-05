//! Integration tests for the global `--heap-cells N` flag (sizes the fixed
//! cons arena — see `main.rs`'s `parse_heap_cells` / `typelisp_mem::Heap`).
//!
//! These drive the `typl` binary out-of-process (like `tests/exit_test.rs`),
//! since the flag lives in the CLI front end, not the library API. They cover
//! the argument *parsing* — the observable, deterministic part of the feature.
//! They deliberately do not force an under-sized arena to exhaust: the prelude
//! is loaded with `.expect(...)` (see `prelude::load_cached`), so a capacity
//! too small even for the prelude aborts with a panic rather than a clean
//! exit, which is pre-existing behavior unrelated to this flag.

use std::io::Write;
use std::process::{Command, Stdio};

/// Runs `typl` with `flags` and feeds `input` on stdin (REPL mode), returning
/// the process exit code.
fn run_typl(flags: &[&str], input: &str) -> i32 {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start the typl binary");
    child.stdin.take().unwrap().write_all(input.as_bytes()).expect("failed to write stdin");
    let status = child.wait().expect("failed to wait on typl");
    status.code().expect("typl did not exit normally")
}

#[test]
fn a_valid_heap_cells_value_is_accepted_and_the_repl_runs() {
    // A capacity comfortably above the prelude's footprint: the flag is parsed,
    // threaded to `Heap::with_capacity`, and a normal session exits cleanly.
    assert_eq!(run_typl(&["--heap-cells", "200000"], "(+ 1 2)\n"), 0);
}

#[test]
fn the_equals_form_is_also_accepted() {
    assert_eq!(run_typl(&["--heap-cells=200000"], "(+ 1 2)\n"), 0);
}

#[test]
fn a_non_numeric_value_is_rejected_before_any_work() {
    assert_eq!(run_typl(&["--heap-cells", "lots"], ""), 1);
}

#[test]
fn zero_is_rejected() {
    // An empty arena would exhaust on the first `cons`, so it is never useful.
    assert_eq!(run_typl(&["--heap-cells", "0"], ""), 1);
}

#[test]
fn a_missing_value_is_rejected() {
    assert_eq!(run_typl(&["--heap-cells"], ""), 1);
}

/// Like [`run_typl`], but keeps stderr: a rejection that does not say *why*
/// is not much better than a silent one.
fn run_typl_stderr(flags: &[&str], input: &str) -> (i32, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start the typl binary");
    child.stdin.take().unwrap().write_all(input.as_bytes()).expect("failed to write stdin");
    let out = child.wait_with_output().expect("failed to wait on typl");
    (out.status.code().expect("typl did not exit normally"), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// A capacity above `i32::MAX` is refused, and the message says what the
/// ceiling is rather than just that the number is wrong.
///
/// The ceiling is not arbitrary: `room` answers with a `heap-info` whose cell
/// counts are `i32` (`gc-count` alone is a `bignum`, being the one field an
/// arena size does not bound). A capacity the report could not *name* would
/// have to be truncated there, and a truncated count is a lie told at the far
/// end from where it could still be explained. Refusing at the flag says it
/// once, while the number is still the one the user typed.
#[test]
fn a_capacity_beyond_what_room_can_report_is_rejected_with_its_ceiling() {
    let (code, err) = run_typl_stderr(&["--heap-cells", "2147483648"], "");
    assert_eq!(code, 1, "stderr was: {}", err);
    assert!(err.contains("more cells than this implementation can count"), "stderr was: {}", err);
    assert!(err.contains("2147483647"), "the message names the ceiling; stderr was: {}", err);
}
