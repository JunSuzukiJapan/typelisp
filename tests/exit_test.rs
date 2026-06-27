//! Integration test for the `exit` builtin (cl-equivalence-catalog.md §1.2):
//! actual process termination can only be observed out-of-process, since
//! calling `std::process::exit` in-process would kill the test runner itself
//! — see `tests/compile_file_test.rs`'s module doc comment for the same
//! reasoning applied to AOT-compiled executables. This drives the `typl`
//! REPL binary directly over piped stdin rather than the library API, since
//! `exit` has no AOT/JIT compile-time counterpart to test through instead.

use std::io::Write;
use std::process::{Command, Stdio};

fn run_typl(input: &str) -> i32 {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
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
fn exit_terminates_the_process_with_the_given_code() {
    assert_eq!(run_typl("(exit 7)\n"), 7);
}

#[test]
fn exit_code_is_truncated_to_a_byte_on_posix() {
    // Process exit codes are a byte on POSIX (`status.code()` already
    // reports the OS's truncated view) — 256 wraps to 0.
    assert_eq!(run_typl("(exit 256)\n"), 0);
}

#[test]
fn exit_runs_from_inside_an_ordinary_function_body() {
    assert_eq!(run_typl("(defun boom () i32 (exit 42)) (boom)\n"), 42);
}
