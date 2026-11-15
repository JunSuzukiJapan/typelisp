//! List types: `(T U)` is `cons-cell<T,cons-cell<U,()>>`, and `list`, a
//! quoted list and a quasiquoted one build its cells where it is expected.
//! A cell prints as the list it begins. Each program is run, which compiles
//! its functions, and compiled into an executable, which runs the same.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("typelisp-list-type-{}-{}", name, std::process::id()));
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

fn typl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_typl"))
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, text).expect("writing the program");
    file
}

const PROGRAM: &str = r#"
(defun by-cons<T,U> ((t T) (u U)) (T U)
  (cons t (cons u ())))
(defun by-quasiquote<T,U> ((t T) (u U)) (T U)
  `(,t ,u))
(defun by-list<T,U> ((t T) (u U)) (T U)
  (list t u))
(defun second-of<T,U> ((xs (T U))) U
  (car (cdr xs)))
(defun dotted<T,U> ((t T) (u U)) (T . U)
  `(,t . ,u))
(defun nested ((n int)) ((int int) symbol)
  `((,n ,(+ n 1)) foo))
(defun spliced ((xs (int int))) (string int int bool)
  `("a" ,@xs true))
(defun spliced-last ((xs (int int))) (string int int)
  `("a" ,@xs))
(defun data-tail ((n int)) (int . Option<Sexpr>)
  `(,n a b))
(defun quoted () (int string)
  '(1 "a"))
(defun pairs () Vector<(int string)>
  (let ((v (the Vector<(int string)> (Vector::new))))
    (push v (list 1 "one"))
    (push v (list 2 "two"))
    v))
(defun main () ()
  (println "~s ~s ~s" (by-cons 1 "a") (by-quasiquote 'x 2.5) (by-list #\c true))
  (println "~s ~s" (second-of (by-list 1 "b")) (dotted 1 2))
  (println "~s ~s ~s" (nested 1) (spliced (list 1 2)) (spliced-last (list 3 4)))
  (println "~s ~s ~s" (data-tail 0) (quoted) (pairs))
  (println "~s ~s ~s" (cons 'a '(b c)) (floor-div 7 2) `(a ,(+ 1 2))))
"#;

const EXPECTED: &str = "(1 \"a\") (x 2.5) (#\\c true)\n\
                        \"b\" (1 . 2)\n\
                        ((1 2) foo) (\"a\" 1 2 true) (\"a\" 3 4)\n\
                        (0 a b) (1 \"a\") #((1 \"one\") (2 \"two\"))\n\
                        (a b c) (3 . 1) (a 3)\n";

#[test]
fn a_run_builds_and_prints_list_types() {
    let dir = scratch("run");
    let file = write(&dir, "prog.typl", &format!("{}\n(main)\n", PROGRAM));
    let (code, out, err) = output(typl().arg(&file));
    assert_eq!((code, out.as_str()), (0, EXPECTED), "{}", err);
}

#[test]
fn an_executable_builds_and_prints_list_types() {
    let dir = scratch("compile");
    let file = write(&dir, "prog.typl", PROGRAM);
    let exe = dir.join("prog");
    let (code, _, err) = output(typl().arg("-c").arg(&file).arg("-o").arg(&exe));
    assert_eq!(code, 0, "{}", err);
    let (code, out, err) = output(&mut Command::new(&exe));
    assert_eq!((code, out.as_str()), (0, EXPECTED), "{}", err);
}

/// The error a run of `src` stops with.
fn error_of(name: &str, src: &str) -> String {
    let dir = scratch(name);
    let file = write(&dir, "prog.typl", src);
    let (code, _, err) = output(typl().arg(&file));
    assert_ne!(code, 0, "{} ran", src);
    err
}

#[test]
fn a_list_has_as_many_elements_as_its_type() {
    let err = error_of("long", "(the (int string) `(1 \"a\" 3))");
    assert!(err.contains("this list has more elements than its type `(int string)` holds"), "{}", err);
    let err = error_of("short", "(the (int string) (list 1))");
    assert!(err.contains("this list has fewer elements than its type `(int string)` holds"), "{}", err);
    let err = error_of("element", "(the (int string) '(1 2))");
    assert!(err.contains("expected `string`, found `int`"), "{}", err);
}

/// `,@` before the end of a template takes the elements out of a list whose
/// type says how many there are.
#[test]
fn a_splice_inside_a_list_needs_a_list_of_fixed_length() {
    let err = error_of("splice", "(let ((v (the Vector<int> (Vector::new)))) (the (int int) `(,@v 1)))");
    assert!(err.contains("splices in a list whose type fixes its length"), "{}", err);
}

/// `(Name A)` is a list type, not the generic type `Name<A>`.
#[test]
fn a_list_is_not_a_generic_type() {
    let err = error_of("applied", "(the (Vector int) (Vector::new))");
    assert!(err.contains("`(Vector int)` is a list type"), "{}", err);
}

/// Elsewhere the same forms build S-expression data, as before.
#[test]
fn without_a_list_type_the_forms_build_data() {
    let dir = scratch("data");
    let file = write(&dir, "prog.typl", "(let ((n 2)) (println \"~s ~s ~s\" `(a ,n) (list 1 'b) '(c \"d\")))\n");
    let (code, out, err) = output(typl().arg(&file));
    assert_eq!((code, out.as_str()), (0, "(a 2) (1 b) (c \"d\")\n"), "{}", err);
}
