//! `:backtrace` and `:locals`: the calls that were open when the last line
//! failed, innermost first, and the arguments of one of them rendered by
//! their declared types.

use std::io::Write;
use std::process::{Command, Stdio};

/// Feeds `input` to `typl --no-init` and returns stdout and stderr.
fn repl(input: &str) -> (String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .arg("--no-init")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn typl");
    child.stdin.take().unwrap().write_all(input.as_bytes()).expect("failed to write stdin");
    let out = child.wait_with_output().expect("typl did not exit");
    (String::from_utf8(out.stdout).unwrap(), String::from_utf8(out.stderr).unwrap())
}

const DEFS: &str = "\
(defun inner ((x int)) int (if (= x 0) (panic \"boom\") x))
(defun middle ((x int) (s string)) int (+ 1 (inner x)))
(defun outer ((n int)) int (* 2 (middle n \"hi\")))
";

#[test]
fn backtrace_lists_the_open_calls_innermost_first() {
    let (out, err) = repl(&format!("{}(outer 0)\n:bt\n", DEFS));
    assert_eq!(err, "error: <stdin>:1:40: panic: boom\n");
    assert_eq!(
        out,
        "  0: (user::inner 0)        called at <stdin>:1:45\n\
         \x20 1: (user::middle 0 \"hi\")  called at <stdin>:1:33\n\
         \x20 2: (user::outer 0)        called at <stdin>:1:1\n"
    );
}

#[test]
fn locals_prints_a_calls_arguments() {
    let (out, _) = repl(&format!("{}(outer 0)\n:locals 1\n:locals\n", DEFS));
    assert_eq!(out, "  x = 0\n  s = \"hi\"\n  x = 0\n");
}

#[test]
fn locals_renders_by_the_declared_type() {
    let (out, _) = repl(&format!(
        "{}(defun opt ((o Option<int>) (p Option<int>)) int (+ 1 (inner 0)))\n(opt (None) (Some 5))\n:locals 1\n",
        DEFS
    ));
    assert_eq!(out, "  o = None\n  p = (Some 5)\n");
}

#[test]
fn a_tail_call_takes_its_callers_place() {
    // A million rounds of tail recursion leave one frame, and the call made
    // from the last round's tail is the one that is left.
    let (out, _) = repl(&format!(
        "{}(defun cnt ((n int)) int (if (= n 0) (inner 0) (cnt (- n 1))))\n(cnt 1000000)\n:bt\n",
        DEFS
    ));
    assert_eq!(out, "  0: (user::inner 0)  called at <stdin>:1:38\n");
}

#[test]
fn the_next_line_discards_the_backtrace() {
    let (out, _) = repl(&format!("{}(outer 0)\n(+ 1 2)\n:bt\n:locals\n", DEFS));
    assert_eq!(out, "3\nno backtrace: the last line did not fail inside a function\n");
}

#[test]
fn a_call_number_out_of_range_is_refused() {
    let (_, err) = repl(&format!("{}(outer 0)\n:locals 3\n:locals x\n", DEFS));
    assert_eq!(
        err,
        "error: <stdin>:1:40: panic: boom\n\
         error: :locals: :backtrace has calls 0 to 2\n\
         error: :locals: `x` is not a call number of :backtrace\n"
    );
}
