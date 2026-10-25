//! `#[test]` and `typl test`.
//!
//! The attribute is read as `(%internal::attribute (test) FORM)` and decided
//! by the checker (`check::TestForms`): left out of an ordinary run, defined
//! and recorded under `typl test`. Running the tests is the CLI's, so most of
//! this drives the `typl` binary over files in a scratch folder.

use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::{check_err, eval_string};

/// A fresh folder for one test; the process id keeps parallel runs apart.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("typelisp-test-attr-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creating the scratch directory");
    dir
}

fn write(dir: &Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).expect("creating the file's folder");
    std::fs::write(&path, source).expect("writing a source file");
    path
}

/// `typl ARGS...` run in `dir`: exit code, stdout, stderr.
fn typl(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_typl")).args(args).current_dir(dir).output().expect("running typl");
    (
        out.status.code().expect("typl did not exit normally"),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const MATH: &str = r#"
(shadow add)
(pub defun add ((a int) (b int)) int (+ a b))

#[test]
(defun adds () ()
  (assert-eq (add 1 2) 3))

#[test]
(defun adds-wrong () ()
  (assert-eq (add 1 2) 4))
"#;

#[test]
fn the_reader_puts_the_attributes_in_front_of_the_form() {
    let read = |text: &str| eval_string(&format!("(format false \"~s\" (read-from-string {:?}))", text));
    let ok = read("#[test] (defun f () ())");
    assert!(ok.contains("(%internal::attribute (test) (defun f () ()))"), "{}", ok);
    assert!(read("#[] (defun f () ())").contains("names no attribute"));
    assert!(read("#[test]").contains("must be followed by the form"));
}

#[test]
fn an_ordinary_run_leaves_a_test_out_unchecked() {
    let dir = scratch("ordinary");
    let file = write(
        &dir,
        "app.typl",
        "#[test]\n(defun broken () () (no-such-function))\n(println \"ran\")\n",
    );
    let (code, out, err) = typl(&dir, &[file.to_str().unwrap()]);
    assert_eq!((code, out.as_str()), (0, "ran\n"), "stderr: {}", err);
}

#[test]
fn typl_test_runs_every_test_and_reports_each_failure() {
    let dir = scratch("project");
    write(&dir, "typelisp.toml", "src = \"src\"\n");
    write(&dir, "src/math.typl", MATH);
    // The program's own work is not done under `typl test`.
    write(&dir, "src/main.typl", "(use math)\n(println \"main ran\")\n");
    write(
        &dir,
        "tests/more.typl",
        r#"
(use math)
#[test]
(defun boom () () (panic "boom"))
#[test]
(defun out-of-range () () (println "~a" (get #(1 2) 10)))
#[test]
(defun after-the-failures () () (assert-eq (math::add 2 2) 4 "still runs"))
"#,
    );
    let (code, out, err) = typl(&dir, &["test"]);
    assert_eq!(code, 1, "stdout: {}\nstderr: {}", out, err);
    assert!(!out.contains("main ran"), "{}", out);
    for line in [
        "running 5 tests",
        "test math::adds ... ok",
        "test math::adds-wrong ... FAILED",
        "test tests::more::boom ... FAILED",
        "test tests::more::out-of-range ... FAILED",
        "test tests::more::after-the-failures ... ok",
        "test result: FAILED. 2 passed; 3 failed; 0 filtered out",
    ] {
        assert!(out.lines().any(|l| l == line), "missing `{}` in:\n{}", line, out);
    }
    assert!(out.contains("assertion failed: (assert-eq (add 1 2) 4)\n  left: 3\n right: 4"), "{}", out);
    assert!(out.contains("panic: boom"), "{}", out);
    assert!(out.contains("index 10 out of bounds"), "{}", out);
}

#[test]
fn a_filter_picks_tests_by_part_of_their_name() {
    let dir = scratch("filter");
    write(&dir, "typelisp.toml", "");
    write(&dir, "math.typl", MATH);
    let (code, out, err) = typl(&dir, &["test", "adds-w"]);
    assert_eq!(code, 1, "stderr: {}", err);
    assert!(out.contains("running 1 test\n"), "{}", out);
    assert!(out.contains("test result: FAILED. 0 passed; 1 failed; 1 filtered out"), "{}", out);

    let (code, out, err) = typl(&dir, &["test", "math.typl", "math::adds"]);
    assert_eq!(code, 1, "stderr: {}", err);
    assert!(out.contains("running 2 tests\n"), "{}", out);

    let (code, out, _) = typl(&dir, &["test", "nothing-matches"]);
    assert_eq!(code, 0);
    assert!(out.contains("test result: ok. 0 passed; 0 failed; 2 filtered out"), "{}", out);
}

#[test]
fn a_test_in_a_module_of_the_file_is_found_there() {
    let dir = scratch("module");
    let file = write(
        &dir,
        "geo.typl",
        r#"
(pub defun area ((w int) (h int)) int (* w h))
(module tests
  #[test]
  (defun square () () (assert-eq (geo::area 3 3) 9)))
"#,
    );
    let (code, out, err) = typl(&dir, &["test", file.to_str().unwrap()]);
    assert_eq!(code, 0, "stdout: {}\nstderr: {}", out, err);
    assert!(out.contains("test geo::tests::square ... ok"), "{}", out);
}

#[test]
fn a_misplaced_attribute_is_an_error() {
    let dir = scratch("errors");
    for (src, message) in [
        ("#[bench]\n(defun f () () ())\n", "unknown attribute `bench`"),
        ("#[test test]\n(defun f () () ())\n", "`#[test]` is given twice"),
        ("#[test]\n(defvar *x* 1)\n", "`#[test]` goes on a `defun`"),
        ("#[test]\n(defun f ((x int)) () ())\n", "cannot take parameters"),
        ("#[test]\n(defun f () int 1)\n", "must return `()` or `Result<(), E>`, not `int`"),
        ("#[test]\n(defun f () Result<int,string> (Result::ok 1))\n", "must return `()` or `Result<(), E>`"),
        ("(defstruct Bad (n int))\n#[test]\n(defun f () Result<(),Bad> (Result::ok ()))\n", "implements neither `Error` nor `print-object`"),
        ("#[test]\n(defun f<T> () () ())\n", "cannot take type parameters"),
    ] {
        let file = write(&dir, "bad.typl", src);
        let (code, _, err) = typl(&dir, &["test", file.to_str().unwrap()]);
        assert_eq!(code, 1, "{}", src);
        assert!(err.contains(message), "`{}`: expected `{}` in: {}", src, message, err);
    }
}

#[test]
fn assert_eq_wants_eq_and_print_object() {
    let err = check_err("(defstruct P (x int))\n(assert-eq (make-instance 'P :x 1) (make-instance 'P :x 1))");
    assert!(err.contains("does not implement trait `std::cmp::eq`"), "{}", err);
    assert_eq!(eval_string("(progn (assert-eq #{1 \"a\"} #{1 \"a\"}) \"same\")"), "same");
}

#[test]
fn compile_file_leaves_a_test_out() {
    let dir = scratch("aot");
    let file = write(
        &dir,
        "app.typl",
        "(defun twice ((x int)) int (* 2 x))\n#[test]\n(defun twice-works () () (assert-eq (twice 2) 4))\n(defun main () () (println \"~a\" (twice 21)))\n",
    );
    let exe = dir.join("app");
    let (code, _, err) = typl(&dir, &["-c", file.to_str().unwrap(), "-o", exe.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", err);
    let out = Command::new(&exe).output().expect("running the executable");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "42\n");
}

#[test]
fn a_test_returning_result_fails_on_err_at_its_own_definition() {
    let dir = scratch("result");
    let file = write(
        &dir,
        "res.typl",
        r#"(defun parse ((s string)) Result<int,string>
  (if (equal s "1") (Result::ok 1) (Result::err (format false "bad: ~a" s))))
#[test]
(defun parses () Result<(),string>
  (match (parse "1") ((ok _) (Result::ok ())) ((err e) (Result::err e))))
#[test]
(defun rejects () Result<(),string>
  (match (parse "x") ((ok _) (Result::ok ())) ((err e) (Result::err e))))
"#,
    );
    let (code, out, err) = typl(&dir, &["test", file.to_str().unwrap()]);
    assert_eq!(code, 1, "stderr: {}", err);
    assert!(out.contains("test res::parses ... ok"), "{}", out);
    assert!(out.contains("test res::rejects ... FAILED"), "{}", out);
    assert!(out.contains("res.typl:7:1: panic: the test returned an error: \"bad: x\""), "{}", out);
}

#[test]
fn a_test_may_return_any_error_and_is_reported_by_its_message() {
    let dir = scratch("dyn-error");
    let file = write(
        &dir,
        "dyn.typl",
        r#"#[test]
(defun concrete () Result<(),ParseIntError>
  (match (parse-int "x") ((ok _) (Result::ok ())) ((err e) (Result::err e))))
#[test]
(defun boxed () Result<(),:dyn Error>
  (match (as-dyn-error (parse-int "y")) ((ok _) (Result::ok ())) ((err e) (Result::err e))))
#[test]
(defun boxed-ok () Result<(),:dyn Error>
  (match (as-dyn-error (parse-int "1")) ((ok _) (Result::ok ())) ((err e) (Result::err e))))
"#,
    );
    let (code, out, err) = typl(&dir, &["test", file.to_str().unwrap()]);
    assert_eq!(code, 1, "stderr: {}", err);
    assert!(out.contains("test dyn::boxed-ok ... ok"), "{}", out);
    // An `Error` is shown by `describe-error`, its message, not `print-object`.
    assert!(out.contains("dyn.typl:2:1: panic: the test returned an error: parse-int: invalid integer literal: \"x\""), "{}", out);
    assert!(out.contains("dyn.typl:5:1: panic: the test returned an error: parse-int: invalid integer literal: \"y\""), "{}", out);
}
