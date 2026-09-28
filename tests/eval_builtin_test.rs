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
    // `(eval (read "(+ 40 2)"))` => `Ok(Some(42))`; the REPL prints it in
    // reader syntax as `(ok 42)` — an `Option<Sexpr>` is the S-expression
    // itself and prints as such, inside a `Result` as anywhere else.
    let out = repl_stdout("(eval (unwrap (read \"(+ 40 2)\")))\n:quit\n");
    assert!(out.contains("(ok 42)"), "stdout was:\n{}", out);
}

#[test]
fn sees_a_global_variable_defined_earlier() {
    let out = repl_stdout("(defvar (x int) 10)\n(eval (unwrap (read \"(+ x 5)\")))\n:quit\n");
    assert!(out.contains("(ok 15)"), "stdout was:\n{}", out);
}

#[test]
fn a_definition_is_visible_to_a_direct_call_on_a_later_repl_line() {
    // CL conformance: `(eval '(defun ...))` registers the function immediately.
    // At the REPL, later lines are checked after it runs, so the direct call
    // `(sq 9)` type-checks and returns 81.
    let out = repl_stdout(
        "(eval (unwrap (read \"(defun sq ((n int)) int (* n n))\")))\n(sq 9)\n:quit\n",
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
        "(eval (unwrap (read \"(defun cube ((n int)) int (* n (* n n)))\")))\n(eval (unwrap (read \"(cube 3)\")))\n:quit\n",
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
        "(defvar (g int) 7)\n(let ((g 99)) (eval (unwrap (read \"g\"))))\n:quit\n",
    );
    assert!(out.contains("(ok 7)"), "stdout was:\n{}", out);
}

// ---- `eval` from inside compiled code ----------------------------------
//
// Until 2026-08-19 a `defun` that called `eval` could not be compiled at all
// (the table the syntax reference's §10 carried then, the last entry in it). It lowers to
// `rt_eval` now — `typelisp_front::shim` — which under JIT finds the running
// interpreter through the same thread-local the printer's hooks use, so the
// compiled body evaluates against the *program's* environment rather than one
// of its own.

/// The compile itself is what used to be refused; `(compile ev)` returning
/// the name is half the point, and the compiled body agreeing with the
/// interpreted one is the other half.
#[test]
fn a_compiled_function_can_call_eval() {
    let out = repl_stdout(
        "(defun ev () int (match (eval (quote (+ 40 2))) ((ok v) (sexpr-int v)) ((err _) -1)))\n\
         (ev)\n(compile ev)\n(ev)\n:quit\n",
    );
    assert_eq!(out.matches("42").count(), 2, "stdout was:\n{}", out);
}

/// And the environment it evaluates against is the live one: a global the
/// interpreter defined is visible from inside the compiled body.
#[test]
fn a_compiled_function_evaluating_a_form_sees_the_programs_globals() {
    let out = repl_stdout(
        "(defvar (g int) 7)\n\
         (defun ev () int (match (eval (quote g)) ((ok v) (sexpr-int v)) ((err _) -1)))\n\
         (compile ev)\n(ev)\n:quit\n",
    );
    assert!(out.contains('7'), "stdout was:\n{}", out);
}

/// A definition made by an eval'd form inside a compiled body is registered
/// in that same environment, so the next line can call it.
#[test]
fn a_definition_made_by_a_compiled_functions_eval_survives_the_call() {
    let out = repl_stdout(
        "(defun define-it () Option<Sexpr> \
           (match (eval (quote (defun sq ((n int)) int (* n n)))) ((ok v) v) ((err _) ())))\n\
         (compile define-it)\n(define-it)\n(sq 7)\n:quit\n",
    );
    assert!(out.contains("49"), "stdout was:\n{}", out);
}

/// A struct evaluates to itself (CL's self-evaluating objects, CLHS
/// 3.1.2.1.3): `eval` answers with the very object, so a write through the
/// result is seen through the original. It used to be taken for a float
/// literal and fail with an internal error.
#[test]
fn a_struct_evaluates_to_itself() {
    let out = repl_stdout(
        "(defstruct q (x i32))\n\
         (defvar (v q) (q::new 7))\n\
         (match (eval v) ((ok s) (match s ((the q w) (progn (setf w::x 8) v::x)) (_ -1))) ((err _) -2))\n\
         :quit\n",
    );
    assert!(out.contains('8'), "stdout was:\n{}", out);
}

/// So does any other heap value that reaches `eval` as data — here the
/// `Result` `read` answers with, passed without being unwrapped.
#[test]
fn a_result_evaluates_to_itself() {
    let out = repl_stdout("(eval (read \"(+ 1 2)\"))\n:quit\n");
    assert!(out.contains("(ok (ok (+ 1 2)))"), "stdout was:\n{}", out);
}

/// A function holding such an object as a literal — a macro spliced it into
/// the body — runs interpreted, and `compile` refuses it by name rather than
/// compiling a copy that would not be the same object.
#[test]
fn compiling_code_that_holds_a_struct_literal_is_refused_by_name() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start the typl binary");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"(defstruct q (x i32))\n\
              (defun keep ((s Sexpr)) Sexpr s)\n\
              (defmacro lit-q () (list (quote keep) (q::new 5)))\n\
              (defun f () Sexpr (lit-q))\n\
              (f)\n\
              (compile f)\n\
              :quit\n",
        )
        .expect("failed to write stdin");
    let out = child.wait_with_output().expect("failed to wait on typl");
    let stdout = String::from_utf8(out.stdout).expect("stdout was not utf-8");
    let stderr = String::from_utf8(out.stderr).expect("stderr was not utf-8");
    assert!(stdout.contains("#<q x: 5>"), "stdout was:\n{}", stdout);
    assert!(stderr.contains("holds a `q` object as a literal"), "stderr was:\n{}", stderr);
}
