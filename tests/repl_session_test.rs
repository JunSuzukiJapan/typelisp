//! The REPL's session: its own commands (`:help`, `:cd`, `:pwd`, `:quit`),
//! and what runs before the first prompt — `~/.typlrc`, then `--eval` and
//! `--load` in the order given.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A fresh, empty folder to stand in for `HOME`.
fn temp_home(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("typl-repl-session-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("cannot make a temporary home");
    dir
}

/// Runs `typl ARGS` with `HOME` at `home`, feeding `input` on stdin; returns
/// its stdout, its stderr and whether it exited 0.
fn typl(home: &Path, args: &[&str], input: &str) -> (String, String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .args(args)
        .env("HOME", home)
        .current_dir(home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn typl");
    child.stdin.take().unwrap().write_all(input.as_bytes()).expect("failed to write stdin");
    let out = child.wait_with_output().expect("typl did not exit");
    (
        String::from_utf8(out.stdout).expect("stdout is not UTF-8"),
        String::from_utf8(out.stderr).expect("stderr is not UTF-8"),
        out.status.success(),
    )
}

#[test]
fn help_lists_every_command() {
    let home = temp_home("help");
    let (out, err, _) = typl(&home, &[], ":help\n");
    assert_eq!(err, "");
    for name in [":help", ":type FORM", ":cd [DIR]", ":pwd", ":quit"] {
        assert!(out.contains(name), "{} is missing from :help:\n{}", name, out);
    }
}

#[test]
fn init_file_runs_before_eval_and_load_in_order() {
    let home = temp_home("order");
    std::fs::write(home.join(".typlrc"), "(println \"rc\")\n(defvar (x int) 1)\n").unwrap();
    std::fs::write(home.join("more.typl"), "(println \"load ~a\" x)\n(setf x 3)\n").unwrap();
    let (out, err, _) = typl(
        &home,
        &["--eval", "(println \"eval ~a\" x)", "--eval=(setf x 2)", "--load", "more.typl", "--eval", "(println \"last ~a\" x)"],
        "x\n",
    );
    assert_eq!(err, "");
    // A startup form's value is not echoed; the typed `x` is.
    assert_eq!(out, "rc\neval 1\nload 2\nlast 3\n3\n");
}

#[test]
fn no_init_skips_the_init_file() {
    let home = temp_home("no-init");
    std::fs::write(home.join(".typlrc"), "(println \"rc\")\n").unwrap();
    let (out, _, _) = typl(&home, &["--no-init"], "(+ 1 2)\n");
    assert_eq!(out, "3\n");
}

#[test]
fn a_failing_startup_form_is_reported_and_the_session_goes_on() {
    let home = temp_home("failing");
    let (out, err, ok) = typl(&home, &["--eval", "(+ 1 \"a\")", "--eval", "(println \"after\")"], "(+ 1 2)\n");
    assert!(ok);
    assert!(err.contains("--eval:1:"), "{}", err);
    assert_eq!(out, "after\n3\n");
}

#[test]
fn startup_options_are_refused_beside_a_script() {
    let home = temp_home("script");
    std::fs::write(home.join("s.typl"), "(println \"ran\")\n").unwrap();
    for args in [&["--eval", "1", "s.typl"][..], &["--load", "x.typl", "s.typl"], &["--no-init", "s.typl"]] {
        let (out, err, ok) = typl(&home, args, "");
        assert!(!ok, "{:?} was accepted", args);
        assert_eq!(out, "");
        assert!(err.contains("only for the REPL"), "{:?}: {}", args, err);
    }
    // After the script's name they are the script's own arguments.
    let (out, _, ok) = typl(&home, &["s.typl", "--eval", "1"], "");
    assert!(ok);
    assert_eq!(out, "ran\n");
}

#[test]
fn cd_moves_the_current_folder_and_pwd_prints_it() {
    let home = temp_home("cd");
    std::fs::create_dir_all(home.join("sub")).unwrap();
    std::fs::write(home.join("sub").join("f.typl"), "(println \"loaded\")\n").unwrap();
    let (out, err, _) = typl(&home, &[], ":cd sub\n:pwd\n(load \"f.typl\")\n:cd\n:pwd\n");
    assert_eq!(err, "");
    let home = home.canonicalize().unwrap();
    let sub = home.join("sub");
    assert_eq!(out, format!("{0}\n{0}\nloaded\n{1}\n{1}\n", sub.display(), home.display()));
}

#[test]
fn quit_ends_the_session_and_an_unknown_name_is_a_keyword() {
    let home = temp_home("quit");
    let (out, _, ok) = typl(&home, &[], ":foo\n:q\n(println \"not reached\")\n");
    assert!(ok);
    assert_eq!(out, ":foo\n");
}
