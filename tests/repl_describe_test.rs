//! The REPL's tools for looking at what a session holds: `:describe`,
//! `:apropos`, `:src`, `:who-calls` and `:inspect`.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Feeds `input` to `typl --no-init`, run in `dir` when one is given, and
/// returns stdout and stderr.
fn repl_in(dir: Option<&PathBuf>, input: &str) -> (String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_typl"));
    cmd.arg("--no-init").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    let mut child = cmd.spawn().expect("failed to spawn typl");
    child.stdin.take().unwrap().write_all(input.as_bytes()).expect("failed to write stdin");
    let out = child.wait_with_output().expect("typl did not exit");
    (String::from_utf8(out.stdout).unwrap(), String::from_utf8(out.stderr).unwrap())
}

fn repl(input: &str) -> (String, String) {
    repl_in(None, input)
}

const DEFS: &str = "\
(defun inner ((x int) (s string)) int \"Adds one to x.\" (+ x 1))
(defun outer ((n int)) int (* 2 (inner n \"hi\")))
(defstruct point (x int) (y Option<int>))
";

#[test]
fn describe_a_function() {
    let (out, err) = repl(&format!("{}:describe inner\n", DEFS));
    assert_eq!(err, "");
    assert_eq!(
        out,
        "user::inner is a function\n  type: (fn (int string) int)\n  parameters: x s\n  defined at <stdin>:1:1\n\nAdds one to x.\n"
    );
}

#[test]
fn describe_a_struct_and_a_builtin_type() {
    let (out, _) = repl(&format!("{}:d point\n:d Option\n", DEFS));
    assert!(out.starts_with("user::point is a struct\n  fields:\n    x: int\n    y: Option<int>\n"), "{}", out);
    assert!(out.contains("std::option::Option is a builtin type\n  type parameters: T\n  variants:\n    (Some T)\n    (None)\n"), "{}", out);
}

#[test]
fn describe_refuses_a_name_that_names_nothing() {
    let (_, err) = repl(":describe no-such-thing\n:describe\n");
    assert_eq!(
        err,
        "error: type error: :describe: no definition named `no-such-thing`\nerror: :describe: give a name, as in `:describe foo`\n"
    );
}

#[test]
fn apropos_lists_matching_names_of_every_module() {
    let (out, _) = repl(&format!("{}:apropos INNER\n:apropos zzzz\n", DEFS));
    // Case is ignored, and a type's methods are listed under the type.
    assert_eq!(
        out,
        "  std::io::peek-stream::inner      method\n  std::io::peek-stream::set-inner  method\n  user::inner                      function\n\
         no definition's name contains `zzzz`\n"
    );
}

#[test]
fn src_prints_a_definition_typed_at_the_prompt() {
    // Every line typed starts again at line 1, so both definitions are at
    // <stdin>:1:1: the one printed is the one that defines the name.
    let (out, _) = repl(&format!("{}:src inner\n:src point\n", DEFS));
    assert_eq!(out, "(defun inner ((x int) (s string)) int \"Adds one to x.\" (+ x 1))\n(defstruct point (x int) (y Option<int>))\n");
}

#[test]
fn src_prints_a_definition_from_a_file() {
    let dir = std::env::temp_dir().join(format!("typl-repl-src-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("f.typl"), ";; a comment\n(defun twice ((n int)) int\n  (* 2 n))\n(defun other () int 0)\n").unwrap();
    let (out, err) = repl_in(Some(&dir), "(load \"f.typl\")\n:src twice\n");
    assert_eq!(err, "");
    assert_eq!(out, "(defun twice ((n int)) int\n  (* 2 n))\n");
}

#[test]
fn who_calls_lists_the_callers() {
    let (out, _) = repl(&format!(
        "{}(defun id<T> ((x T)) T x)\n(defun use-id () int (id 3))\n:who-calls inner\n:who-calls outer\n:who-calls id\n",
        DEFS
    ));
    assert_eq!(out, "  user::outer\nnothing calls user::outer\n  user::use-id\n");
}

#[test]
fn inspect_opens_a_value_by_its_type() {
    let (out, err) = repl(&format!(
        "{}(defenum shape (Circle point int) (Dot))\n\
         :inspect (shape::Circle (make-instance 'point :x 1 :y (Some 2)) 5)\n0\n1\n0\nu\nu\nu\nu\nq\n(+ 1 2)\n",
        DEFS
    ));
    assert_eq!(err, "");
    let top = "(shape::Circle (make-instance 'point :x 1 :y (Some 2)) 5) = (Circle #<point x: 1 y: (Some 2)> 5)\n  type: shape\n  0. Circle 0 = #<point x: 1 y: (Some 2)>\n  1. Circle 1 = 5\n";
    let point = "Circle 0 = #<point x: 1 y: (Some 2)>\n  type: point\n  0. x = 1\n  1. y = (Some 2)\n";
    let y = "y = (Some 2)\n  type: Option<int>\n  0. Some 0 = 2\n";
    let two = "Some 0 = 2\n  type: int\n";
    assert_eq!(out, format!("{top}{point}{y}{two}{y}{point}{top}already at the top\n{top}3\n"));
}

#[test]
fn inspect_opens_a_vector_and_a_hash_table() {
    let (out, _) = repl(
        ":i (the Vector<int> #(1 2))\nq\n:i (let ((h (the HashTable<string,int> (HashTable::new)))) (set h \"a\" 1) h)\nq\n",
    );
    assert_eq!(
        out,
        "(the Vector<int> #(1 2)) = #(1 2)\n  type: Vector<int>\n  0. [0] = 1\n  1. [1] = 2\n\
         (let ((h (the HashTable<string,int> (HashTable::new)))) (set h \"a\" 1) h) = #<HashTable<string,int> count=1>\n  type: HashTable<string,int>\n  0. key 0 = \"a\"\n  1. value 0 = 1\n"
    );
}
