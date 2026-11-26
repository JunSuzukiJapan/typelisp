//! Ctrl-C at the REPL stops the evaluation it is running and returns to the
//! prompt, with the session's definitions kept: an interpreted loop, a loop
//! in compiled code (through its safepoint), and a task waiting on the
//! clock. `unwind-protect` cleanups run on the way out.
//!
//! Each test sends SIGINT to a `typl` reading from a pipe, once the form
//! has said it started, and then gives it one more line.

#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

/// Runs `setup` and then `form` (which prints `started` before it runs
/// forever), interrupts it, and then runs `after`. Returns what was printed
/// after `started`, and stderr.
fn interrupt(setup: &str, form: &str, after: &str) -> (String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .arg("--no-init")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn typl");
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{}\n{}", setup, form).unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert!(stdout.read_line(&mut line).unwrap() > 0, "typl ended before the form started");
        if line == "started\n" {
            break;
        }
    }
    // SAFETY: `kill` with the child's own pid and a standard signal.
    assert_eq!(unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT) }, 0);
    writeln!(stdin, "{}", after).unwrap();
    drop(stdin);
    let mut rest = String::new();
    stdout.read_to_string(&mut rest).unwrap();
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    assert!(child.wait().unwrap().success());
    (rest, err)
}

#[test]
fn an_interpreted_loop_stops_and_the_session_goes_on() {
    let (out, err) = interrupt(
        "(defvar (n int) 0)",
        "(progn (println \"started\") (loop (setf n (+ n 1))))",
        // The signal may land before the first round: what is checked is
        // that the definition survived and the next line runs.
        "(>= n 0)",
    );
    assert_eq!(err, "error: interrupted\n");
    assert_eq!(out, "true\n");
}

#[test]
fn a_compiled_loop_stops_and_its_cleanup_runs() {
    let (out, err) = interrupt(
        "(defun spin () () (unwind-protect (progn (println \"started\") (loop)) (println \"cleanup\")))\n(compile spin)",
        "(spin)",
        "(+ 1 2)",
    );
    assert_eq!(err, "error: interrupted\n");
    assert_eq!(out, "cleanup\n3\n");
}

#[test]
fn a_wait_on_the_clock_stops_and_its_cleanup_runs() {
    let (out, err) = interrupt(
        "",
        "(unwind-protect (progn (println \"started\") (sleep 100.0)) (println \"cleanup\"))",
        "(+ 1 2)",
    );
    assert_eq!(err, "error: interrupted\n");
    assert_eq!(out, "cleanup\n3\n");
}

#[test]
fn a_wait_on_a_task_stops() {
    let (out, err) = interrupt("", "(progn (println \"started\") (wait (task (sleep 100.0))))", "(+ 1 2)");
    assert_eq!(err, "error: interrupted\n");
    assert_eq!(out, "3\n");
}
