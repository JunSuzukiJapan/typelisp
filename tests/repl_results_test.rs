//! `:*`, `:**` and `:***` at the REPL: the last three values echoed, each
//! with the type it was checked to have. Quoted, in a file, and in the
//! startup files, they are keywords like any other.

use std::io::Write;
use std::process::{Command, Stdio};

/// Feeds `input` to `typl --no-init ARGS` and returns stdout and stderr.
fn repl(args: &[&str], input: &str) -> (String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .arg("--no-init")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn typl");
    child.stdin.take().unwrap().write_all(input.as_bytes()).expect("failed to write stdin");
    let out = child.wait_with_output().expect("typl did not exit");
    (String::from_utf8(out.stdout).unwrap(), String::from_utf8(out.stderr).unwrap())
}

#[test]
fn the_last_three_results_keep_their_types() {
    let (out, err) = repl(&[], "1\n\"a\"\n2\n(+ :*** :*)\n(equal :*** \"a\")\n");
    assert_eq!(err, "");
    assert_eq!(out, "1\n\"a\"\n2\n3\ntrue\n");
}

#[test]
fn a_failure_and_a_definition_leave_the_results_alone() {
    let (out, _) = repl(&[], "1\n(+ 1 \"x\")\n(defvar (v int) 9)\n:*\n");
    assert_eq!(out, "1\n1\n");
}

#[test]
fn quoted_it_is_a_keyword() {
    let (out, _) = repl(&[], "1\n':*\n(list ':** 2)\n");
    assert_eq!(out, "1\n:*\n(:** 2)\n");
}

#[test]
fn one_with_no_result_yet_is_an_error() {
    let (out, err) = repl(&[], "1\n:**\n");
    assert_eq!(out, "1\n");
    assert_eq!(err, "error: <stdin>:1:1: type error: `:**` has no value yet: there have not been that many results\n");
}

#[test]
fn a_definition_reads_the_result_it_was_written_with() {
    // CL's `*` is read when the function runs; here the function is fixed to
    // the result it was checked against, which keeps its type.
    let (out, err) = repl(&[], "40\n(defun f () int (+ :* 2))\n(f)\n\"later\"\n1\n2\n3\n(f)\n");
    assert_eq!(err, "");
    assert_eq!(out, "40\n42\n\"later\"\n1\n2\n3\n42\n");
}

#[test]
fn startup_files_read_it_as_a_keyword() {
    let dir = std::env::temp_dir().join(format!("typl-repl-results-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("kw.typl");
    std::fs::write(&file, "(println \"~s\" (list :* 1))\n").unwrap();
    let (out, err) = repl(&["--load", file.to_str().unwrap(), "--eval", "(println \"~s\" :**)"], "");
    assert_eq!(err, "");
    assert_eq!(out, "(:* 1)\n:**\n");
}
