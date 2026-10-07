//! `typl --help` and `typl --version`: they print and exit before anything
//! else runs, but only among `typl`'s own options — after a script's name
//! they are the script's arguments.

use std::process::{Command, Output};

const TYPL: &str = env!("CARGO_BIN_EXE_typl");

fn typl(args: &[&str]) -> Output {
    Command::new(TYPL).args(args).output().expect("failed to start the typl binary")
}

#[test]
fn version_prints_the_version() {
    let out = typl(&["--version"]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "typl 0.2.0\n");
}

#[test]
fn help_lists_every_option() {
    let out = typl(&["--help"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for flag in ["--compile", "-o ", "--lib-dir", "--image", "--heap-cells", "--feature", "--on-redefine", "--help", "--version"] {
        assert!(text.contains(flag), "{} missing from:\n{}", flag, text);
    }
}

/// Answered before `--lib-dir` checks its folder, so a bad one does not
/// stand between a person and the help text.
#[test]
fn help_wins_over_other_options() {
    let out = typl(&["--heap-cells", "10", "--lib-dir", "/nonexistent", "--help"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("Usage:"));
}

#[test]
fn after_the_script_name_they_belong_to_the_script() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("aot-test-tmp").join("typl-help-after-script");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("args.typl");
    std::fs::write(&src, "(println \"~a\" (command-line-args))\n").unwrap();

    let out = typl(&[src.to_str().unwrap(), "--help", "--version"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).trim_end().ends_with("--help --version>"));
}
