//! Integration tests for the CL-style `eval` builtin (`Interp::eval_form`).
//!
//! `eval` type-checks and runs a runtime `Sexpr` against the program's current
//! global environment, so it only works once the driver has wired the live
//! `Checker` into the `Interp` (`set_checker`) — the plain library `run`
//! helper in `eval_test.rs` deliberately doesn't do that. These tests
//! therefore drive the real `typl` REPL binary over piped stdin (the same
//! approach `exit_test.rs` uses for a builtin with no in-process API),
//! capturing stdout so the printed results can be asserted. The REPL echoes
//! each top-level expression's value in reader syntax, and — because it checks
//! and executes one line at a time — a name defined by `eval` on one line is
//! callable directly on the next, exercising the "definitions become visible
//! immediately" half of CL conformance.

use std::io::Write;
use std::process::{Command, Stdio};

/// Feed `input` to the `typl` REPL over stdin and return its captured stdout.
fn repl_stdout(input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start the typl binary");
    child.stdin.take().unwrap().write_all(input.as_bytes()).expect("failed to write stdin");
    let out = child.wait_with_output().expect("failed to wait on typl");
    String::from_utf8(out.stdout).expect("stdout was not utf-8")
}

#[test]
fn evaluates_an_expression_and_returns_its_value_as_a_sexpr() {
    // `(eval (read "(+ 40 2)"))` => `Ok(42)`; the REPL prints it in reader
    // syntax as `(ok 42)`.
    let out = repl_stdout("(eval (unwrap (read \"(+ 40 2)\")))\n:quit\n");
    assert!(out.contains("(ok 42)"), "stdout was:\n{}", out);
}

#[test]
fn sees_a_global_variable_defined_earlier() {
    let out = repl_stdout("(defvar (x i32) 10)\n(eval (unwrap (read \"(+ x 5)\")))\n:quit\n");
    assert!(out.contains("(ok 15)"), "stdout was:\n{}", out);
}

#[test]
fn a_definition_is_visible_to_a_direct_call_on_a_later_repl_line() {
    // CL conformance: `(eval '(defun ...))` registers the function immediately.
    // At the REPL, later lines are checked after it runs, so the direct call
    // `(sq 9)` type-checks and returns 81.
    let out = repl_stdout(
        "(eval (unwrap (read \"(defun sq ((n i32)) i32 (* n n))\")))\n(sq 9)\n:quit\n",
    );
    // The define returns the name symbol; the REPL prints `(ok sq)`.
    assert!(out.contains("(ok sq)"), "stdout was:\n{}", out);
    assert!(out.contains("81"), "stdout was:\n{}", out);
}

#[test]
fn a_definition_is_visible_to_a_subsequent_eval() {
    // The same, but the *call* also goes through `eval` — proving the
    // definition landed in the shared environment the next `eval` checks
    // against, not just the REPL's own registry.
    let out = repl_stdout(
        "(eval (unwrap (read \"(defun cube ((n i32)) i32 (* n (* n n)))\")))\n(eval (unwrap (read \"(cube 3)\")))\n:quit\n",
    );
    assert!(out.contains("(ok 27)"), "stdout was:\n{}", out);
}

#[test]
fn an_ill_typed_form_is_a_recoverable_err_not_a_panic() {
    // `(+ 1 #\a)` is a type error; `eval` surfaces it as `Err`, and the
    // program keeps running (the trailing marker prints).
    let out = repl_stdout(
        "(is-err (eval (unwrap (read \"(+ 1 #\\\\a)\"))))\n(println \"survived\")\n:quit\n",
    );
    assert!(out.contains("true"), "stdout was:\n{}", out);
    assert!(out.contains("survived"), "stdout was:\n{}", out);
}

#[test]
fn evaluates_in_the_null_lexical_environment() {
    // CL: `eval` does not see the caller's lexical locals. A local `y` is
    // invisible, so `(eval (read "y"))` is an `Err` (unbound), not `99`.
    let out = repl_stdout(
        "(let ((y 99)) (is-err (eval (unwrap (read \"y\")))))\n:quit\n",
    );
    assert!(out.contains("true"), "stdout was:\n{}", out);
}

#[test]
fn a_global_shadowed_by_a_local_is_still_seen_by_its_global_value() {
    // The CLHS example: a local binding of the same name does not change what
    // `eval` sees — it uses the global. Global `g` = 7; a local `g` = 99 does
    // not leak into the evaluated form.
    let out = repl_stdout(
        "(defvar (g i32) 7)\n(let ((g 99)) (eval (unwrap (read \"g\"))))\n:quit\n",
    );
    assert!(out.contains("(ok 7)"), "stdout was:\n{}", out);
}
