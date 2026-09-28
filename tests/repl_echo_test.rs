//! The `typl` REPL echoes each result the way `~s` prints it.
//!
//! The echo once had a renderer of its own, which drifted from the one
//! `format` uses: it dropped a generic type's arguments (`#<vector 1>` where
//! `println` said `#<vector<int> 1>`) and never consulted a `print-object`
//! method. Each test here pairs a value's echo with `(println "~s" ...)` of
//! the same expression, so a second renderer cannot come back unnoticed.

use std::io::Write;
use std::process::{Command, Stdio};

/// Feed `input` to the `typl` REPL over stdin and return its stdout.
fn repl_stdout(input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn typl");
    child.stdin.take().unwrap().write_all(input.as_bytes()).expect("failed to write stdin");
    let out = child.wait_with_output().expect("typl did not exit");
    String::from_utf8(out.stdout).expect("stdout is not UTF-8")
}

/// The REPL's echo of `expr` and `~s`'s rendering of it, after `defs`.
fn echo_and_s(defs: &str, expr: &str) -> (String, String) {
    let out = repl_stdout(&format!("{}\n{}\n(println \"~s\" {})\n", defs, expr, expr));
    let lines: Vec<&str> = out.lines().collect();
    // The echo, then `println`'s line, then the echo of `println`'s own `()`.
    assert!(lines.len() >= 3, "unexpected REPL output: {:?}", out);
    let n = lines.len();
    (lines[n - 3].to_string(), lines[n - 2].to_string())
}

fn assert_echo(defs: &str, expr: &str, expected: &str) {
    let (echo, s) = echo_and_s(defs, expr);
    assert_eq!(echo, s, "the REPL echo differs from ~s for {}", expr);
    assert_eq!(echo, expected);
}

#[test]
fn a_vector_echo_names_its_element_type() {
    assert_echo(
        "",
        "(loop :for i :from 1 :to 3 :collect (* i i))",
        "#<vector<int> 1 4 9>",
    );
    assert_echo("", "(the Vector<string> (Vector::new))", "#<vector<string>>");
}

#[test]
fn a_generic_struct_echo_names_its_type_arguments() {
    assert_echo("(defstruct pair<A,B> (left A) (right B))", "(pair::new 1 \"one\")", "#<pair<int,string> left: 1 right: \"one\">");
    assert_echo("", "(cons 1 \"a\")", "#<cons-cell<int,string> car: 1 cdr: \"a\">");
}

#[test]
fn the_echo_uses_a_print_object_method() {
    let defs = "(defstruct money (yen int))\n\
                (impl print-object money (print-object ((self Self) (escape bool)) string (format false \"~a yen\" self::yen)))";
    assert_echo(defs, "(money::new 5)", "5 yen");
    assert_echo(defs, "(list 1 (money::new 7))", "(1 7 yen)");
}

#[test]
fn a_niched_option_echoes_with_its_wrapper() {
    assert_echo("", "(Option::some \"x\")", "(some \"x\")");
    assert_echo("", "(the Option<int> (Option::none))", "none");
    assert_echo("(defstruct holder (o Option<int>))", "(holder::new (Option::some 3))", "#<holder o: (some 3)>");
}

#[test]
fn atoms_echo_readably() {
    assert_echo("", "\"a\\nb\"", "\"a\\nb\"");
    assert_echo("", "#\\a", "#\\a");
    assert_echo("", "2.0", "2.0");
    assert_echo("", "1/3", "1/3");
    assert_echo("", "'(1 \"a\" b)", "(1 \"a\" b)");
}
