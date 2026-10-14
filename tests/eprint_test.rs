//! `eprint`/`eprintln`: `print`/`println` to `*error-output*`, left out of a
//! release build (`typl -c --release`) together with their arguments. The
//! split between the two outputs, and the executables, can only be seen from
//! outside, so this drives the `typl` binary.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("typelisp-eprint-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creating the scratch directory");
    dir
}

/// Exit code, stdout, stderr.
fn output(cmd: &mut Command) -> (i32, String, String) {
    let out = cmd.output().expect("running a command");
    (
        out.status.code().expect("did not exit normally"),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `side` prints when it runs, so a left-out argument shows as a missing line.
const PROGRAM: &str = r#"
(defun side () int (progn (println "side ran") 7))
(defun main () ()
  (eprint "a=~a " 1)
  (eprintln "b=~a" (side))
  (println "out")
  (println "~a" (eval '(progn (eprintln "from eval") 5))))
(main)
"#;

fn write_program(dir: &Path) -> PathBuf {
    let file = dir.join("prog.typl");
    std::fs::write(&file, PROGRAM).expect("writing the program");
    file
}

#[test]
fn eprint_writes_to_standard_error_only() {
    let dir = scratch("run");
    let file = write_program(&dir);
    let (code, out, err) = output(Command::new(env!("CARGO_BIN_EXE_typl")).arg(&file));
    assert_eq!(code, 0, "{}", err);
    assert_eq!(out, "side ran\nout\n(ok 5)\n");
    assert_eq!(err, "a=1 b=7\nfrom eval\n");
}

#[test]
fn a_release_build_leaves_eprint_and_its_arguments_out() {
    let dir = scratch("aot");
    let file = write_program(&dir);
    for (release, expected_out, expected_err) in [
        (false, "side ran\nout\n(ok 5)\n", "a=1 b=7\nfrom eval\n"),
        (true, "out\n(ok 5)\n", ""),
    ] {
        let exe = dir.join(if release { "prog-release" } else { "prog-debug" });
        let mut build = Command::new(env!("CARGO_BIN_EXE_typl"));
        build.arg("-c").arg(&file).arg("-o").arg(&exe);
        if release {
            build.arg("--release");
        }
        let (code, _, err) = output(&mut build);
        assert_eq!(code, 0, "release={}: {}", release, err);
        let (code, out, err) = output(&mut Command::new(&exe));
        assert_eq!((code, out.as_str(), err.as_str()), (0, expected_out, expected_err), "release={}", release);
    }
}

#[test]
fn a_release_build_still_checks_what_it_leaves_out() {
    let dir = scratch("checked");
    let file = dir.join("bad.typl");
    std::fs::write(&file, "(defun main () () (eprintln \"~a\" (no-such-function)))\n").unwrap();
    let (code, _, err) = output(
        Command::new(env!("CARGO_BIN_EXE_typl")).arg("-c").arg(&file).arg("-o").arg(dir.join("bad")).arg("--release"),
    );
    assert_eq!(code, 1);
    assert!(err.contains("no such function: no-such-function"), "{}", err);
}

#[test]
fn the_control_string_must_be_a_literal() {
    let dir = scratch("literal");
    let file = dir.join("f.typl");
    std::fs::write(&file, "(let ((c \"x\")) (eprintln c))\n").unwrap();
    let (code, _, err) = output(Command::new(env!("CARGO_BIN_EXE_typl")).arg(&file));
    assert_eq!(code, 1);
    assert!(err.contains("eprintln: the control string must be a literal"), "{}", err);
}
