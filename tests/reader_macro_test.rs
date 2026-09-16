//! Reader macros — `set-macro-character` and friends (cl-parity-plan Stage 8c).
//!
//! The readtable lives on the `Heap`, the reader looks in it before it does
//! anything else with a character, and calling what it finds goes back
//! through the evaluator (`Interp::call_reader_macro_fn` -> the prelude's
//! `call-reader-macro`). These tests pin the three things that arrangement
//! can get wrong: *whether* the function is reached, *what* it is handed, and
//! **where the reader resumes** — the macro reads from a stream of its own,
//! so the cursor is moved by a count the macro reports rather than by the
//! reader having watched it.
//!
//! When a registration takes effect differs by path, exactly as `#.` does
//! (see `read_time_eval_test.rs`): the REPL and `(load ...)` run each form as
//! they read it, while a module file is checked as a unit — so the
//! registration form there is executed *immediately* rather than queued
//! (`project::needs_immediate_exec`), and what it registers has to already
//! exist, which a `defun` in the same file does not.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use typelisp::project::{find_src_root, Loader};
use typelisp::*;

fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("reader-macro-tmp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixture dirs");
        std::fs::write(&path, src).expect("write fixture file");
    }
    dir
}

/// Load `entry` through the module loader and run what it queued.
fn run_project(dir: &std::path::Path, entry: &str) -> Result<Option<Value>, String> {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    let entry_path = dir.join(entry);
    let entry_dir = entry_path.parent().unwrap().to_path_buf();
    let src_root = find_src_root(&entry_dir).unwrap_or(entry_dir);
    let mut loader = Loader::new(src_root);
    loader
        .load_entry(&mut heap, &reader, &mut checker, &mut interp, &entry_path)
        .map_err(|e| e.to_string())?;
    let mut last = None;
    for tl in loader.take_pending() {
        match interp.exec(&mut heap, tl) {
            Ok(Some(v)) => last = Some(v),
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(last)
}

fn repl_stdout(input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start the typl binary");
    child.stdin.take().unwrap().write_all(input.as_bytes()).expect("failed to write stdin");
    let out = child.wait_with_output().expect("failed to wait on typl");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `!x` -> `(not x)`: the smallest macro that actually reads something.
const BANG: &str = r#"(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))
"#;

// ----------------------------------------------------------------------
// the basics
// ----------------------------------------------------------------------

#[test]
fn a_macro_character_replaces_what_follows_it() {
    let dir = fixture("basic", &[("main.typl", &format!("{}\n(defvar (b bool) !false)\nb\n", BANG))]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Bool(true))));
}

#[test]
fn the_reader_resumes_exactly_where_the_macro_stopped() {
    // The macro reads `(equal 1 2)` out of a stream of its own; the `if` it
    // sits inside has to see `7` as its next element. A cursor left one
    // character short would make the `if` malformed instead.
    let dir = fixture(
        "resume",
        &[("main.typl", &format!("{}\n(defvar (n int) (if !(equal 1 2) 7 8))\nn\n", BANG))],
    );
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(7))));
}

#[test]
fn a_macro_character_reached_twice_reads_twice() {
    let dir = fixture("twice", &[("main.typl", &format!("{}\n(defvar (n int) (if !false (if !true 1 2) 3))\nn\n", BANG))]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(2))));
}

#[test]
fn an_unregistered_character_still_reads_as_itself() {
    // `!` with nothing registered is an ordinary symbol constituent, which is
    // what makes the registration observable at all.
    let dir = fixture("unregistered", &[("main.typl", "(defvar (b bool) (equal (quote !) (quote !)))\nb\n")]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Bool(true))));
}

#[test]
fn a_macro_character_wins_over_the_built_in_syntax() {
    // `'` normally means `quote`. Registering over it is the whole point of
    // looking in the table *before* the built-in dispatch.
    let src = r#"(set-macro-character #\'
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s) ((ok o) 99) ((err e) 0))))
(defvar (n int) 'whatever)
n
"#;
    let dir = fixture("override", &[("main.typl", src)]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(99))));
}

#[test]
fn a_macro_that_reads_nothing_leaves_the_cursor_where_it_was() {
    let src = r#"(set-macro-character #\@
  (lambda ((s string-input-stream) (c char)) Option<Sexpr> 7))
(defvar (n int) (+ @ 1))
n
"#;
    let dir = fixture("consumes_nothing", &[("main.typl", src)]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(8))));
}

#[test]
fn the_triggering_character_is_passed_in() {
    // Two characters, one function, told apart only by its second argument.
    // In the REPL, where a `defvar` earlier in the input has already run.
    let out = repl_stdout(concat!(
        "(defvar (f (fn (string-input-stream char) Option<Sexpr>))\n",
        "  (lambda ((s string-input-stream) (c char)) Option<Sexpr> (if (equal c #\\a) 1 2)))\n",
        "(set-macro-character #\\a f)\n",
        "(set-macro-character #\\b f)\n",
        "(+ a (* 10 b))\n",
    ));
    assert!(out.contains("21"), "got {:?}", out);
}

// ----------------------------------------------------------------------
// dispatching characters
// ----------------------------------------------------------------------

#[test]
fn a_dispatch_macro_extends_the_hash_syntax() {
    let src = r#"(set-dispatch-macro-character #\# #\{
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-delimited-list #\} s)
      ((ok l) (sexpr-cons (quote +) l))
      ((err e) (quote ())))))
(defvar (n int) #{1 2 3})
n
"#;
    let dir = fixture("hash_brace", &[("main.typl", src)]);
    // The macro turned `#{1 2 3}` into `(+ 1 2 3)`, delimiter and all.
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(6))));
}

#[test]
fn the_built_in_hash_syntax_still_works_alongside_one() {
    let src = r#"(set-dispatch-macro-character #\# #\{
  (lambda ((s string-input-stream) (c char)) Option<Sexpr> 1))
(defvar (n int) #xff)
n
"#;
    let dir = fixture("hash_coexist", &[("main.typl", src)]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(255))));
}

#[test]
fn registering_makes_the_character_dispatching() {
    // No `make-dispatch-macro-character`: `$` becomes a dispatching character
    // by having something registered under it, which is the only thing the
    // separate step would have done.
    let src = r#"(set-dispatch-macro-character #\$ #\s
  (lambda ((s string-input-stream) (c char)) Option<Sexpr> 5))
(defvar (n int) $s)
n
"#;
    let dir = fixture("own_dispatch", &[("main.typl", src)]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(5))));
}

#[test]
fn an_unregistered_sub_character_is_a_read_error() {
    let src = r#"(set-dispatch-macro-character #\$ #\s
  (lambda ((s string-input-stream) (c char)) Option<Sexpr> 5))
(defvar (n int) $t)
n
"#;
    let dir = fixture("bad_sub", &[("main.typl", src)]);
    let err = run_project(&dir, "main.typl").expect_err("`$t` has nothing registered");
    assert!(err.contains("$t"), "expected the pair in the message, got {}", err);
}

// ----------------------------------------------------------------------
// reading the table back
// ----------------------------------------------------------------------

#[test]
fn get_macro_character_answers_none_before_and_some_after() {
    let out = repl_stdout(concat!(
        "(match (get-macro-character #\\!) ((some f) \"set\") ((none) \"unset\"))\n",
        "(set-macro-character #\\! (lambda ((s string-input-stream) (c char)) Option<Sexpr> 1))\n",
        "(match (get-macro-character #\\!) ((some f) \"set\") ((none) \"unset\"))\n",
    ));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.first(), Some(&"\"unset\""), "got {:?}", lines);
    assert_eq!(lines.last(), Some(&"\"set\""), "got {:?}", lines);
}

#[test]
fn what_comes_back_is_the_function_that_was_stored() {
    let out = repl_stdout(concat!(
        "(set-macro-character #\\! (lambda ((s string-input-stream) (c char)) Option<Sexpr> 42))\n",
        "(match (get-macro-character #\\!) ((some f) (f (make-string-input-stream \"\") #\\!)) ((none) 0))\n",
    ));
    assert!(out.contains("42"), "got {:?}", out);
}

#[test]
fn get_dispatch_macro_character_answers_for_the_pair() {
    let out = repl_stdout(concat!(
        "(set-dispatch-macro-character #\\# #\\{ (lambda ((s string-input-stream) (c char)) Option<Sexpr> 1))\n",
        "(match (get-dispatch-macro-character #\\# #\\{) ((some f) \"set\") ((none) \"unset\"))\n",
        "(match (get-dispatch-macro-character #\\# #\\}) ((some f) \"set\") ((none) \"unset\"))\n",
    ));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[lines.len() - 2], "\"set\"", "got {:?}", lines);
    assert_eq!(lines[lines.len() - 1], "\"unset\"", "got {:?}", lines);
}

#[test]
fn a_second_registration_replaces_the_first() {
    let src = r#"(set-macro-character #\@
  (lambda ((s string-input-stream) (c char)) Option<Sexpr> 1))
(set-macro-character #\@
  (lambda ((s string-input-stream) (c char)) Option<Sexpr> 2))
(defvar (n int) @)
n
"#;
    let dir = fixture("replace", &[("main.typl", src)]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(2))));
}

// ----------------------------------------------------------------------
// the `read` builtin
// ----------------------------------------------------------------------

#[test]
fn the_read_builtin_consults_the_readtable_too() {
    // CL's `read` reads through `*readtable*`; a program that installed a
    // macro character means it for its own reads. This is the path with no
    // driver to hand a hook in — the interpreter registers itself instead
    // (`typelisp_read::runtime`).
    let out = repl_stdout(&format!("{}\n(match (read \"!(a b)\") ((ok v) v) ((err e) (quote ())))\n", BANG.replace('\n', " ")));
    assert!(out.contains("(not (a b))"), "got {:?}", out);
}

// ----------------------------------------------------------------------
// when the registration takes effect
// ----------------------------------------------------------------------

#[test]
fn a_registration_in_a_module_file_affects_the_rest_of_that_file() {
    // The whole reason `set-macro-character` is in `needs_immediate_exec`:
    // a module file's forms are queued, and a queued registration would not
    // exist yet when the next form is *read*.
    let dir = fixture("same_file", &[("main.typl", &format!("{}\n(defvar (b bool) !true)\nb\n", BANG))]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Bool(false))));
}

#[test]
fn a_defun_in_the_same_file_is_not_available_yet() {
    // ...and the failure says so rather than reading on as if nothing had
    // been registered. The `defun` is queued with the rest of the file; only
    // the registration itself runs early.
    let src = r#"(defun bang ((s string-input-stream) (c char)) Option<Sexpr> 1)
(set-macro-character #\! bang)
(defvar (n int) !x)
n
"#;
    let dir = fixture("defun_too_late", &[("main.typl", src)]);
    let err = run_project(&dir, "main.typl").expect_err("`bang` has not been defined yet");
    assert!(err.contains("bang"), "expected the name in the message, got {}", err);
}

#[test]
fn the_repl_reaches_a_function_defined_a_form_earlier() {
    // The REPL runs each form as it reads it, so there is nothing queued and
    // a plain `defun` works — CL's `load`, against the same code that needs
    // an `eval-when` under `compile-file`.
    let out = repl_stdout(concat!(
        "(defun bang ((s string-input-stream) (c char)) Option<Sexpr> 3)\n",
        "(set-macro-character #\\! bang)\n",
        "(+ ! 1)\n",
    ));
    assert!(out.contains('4'), "got {:?}", out);
}

// ----------------------------------------------------------------------
// failures
// ----------------------------------------------------------------------

#[test]
fn an_error_inside_a_reader_macro_surfaces_as_a_read_error() {
    let src = r#"(set-macro-character #\@
  (lambda ((s string-input-stream) (c char)) Option<Sexpr> (panic "boom")))
(defvar (n int) @)
n
"#;
    let dir = fixture("macro_panics", &[("main.typl", src)]);
    let err = run_project(&dir, "main.typl").expect_err("the macro panics");
    assert!(err.contains("boom"), "expected the panic message, got {}", err);
}

// ----------------------------------------------------------------------
// compiled
// ----------------------------------------------------------------------

/// The four builtins in a standalone executable.
///
/// The readtable lives on the `Heap`, which an AOT program has, and the four
/// builtins have `rt_*` shims like every other one — so registering and
/// querying work with nothing special. Reading *through* a macro character is
/// the part that needs an evaluator, so naming `rt_set_macro_character` is
/// what makes `compile-file` build the interpreter into the image — the same
/// rule, and the same price, `eval` has (`aot::EVAL_SHIMS`).
///
/// The registration is inside `main` rather than at top level, because
/// `compile-file` takes definitions only: an AOT program has no load phase
/// for a top-level call to happen in. Which also means a reader macro cannot
/// change how an AOT program's *own source* is read — that read happens in
/// the compiler, and the form that would register it never runs there.
#[test]
fn the_readtable_builtins_work_in_a_compiled_executable() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("reader-macro-tmp").join("aot");
    std::fs::create_dir_all(&dir).expect("create the aot dir");
    let src_path = dir.join("aot.typl");
    let out_path = dir.join("aot");
    let src = r#"(defun bang ((s string-input-stream) (c char)) Option<Sexpr>
  (match (read-sexpr s)
    ((ok o) (match o
              ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
              ((eof) (quote ()))))
    ((err e) (quote ()))))
(defun main () int
  (progn
    (set-macro-character #\! bang)
    (if (match (get-macro-character #\!) ((some f) false) ((none) true)) 2
      (if (equal (format false "~a" (match (read "!(a b)") ((ok v) v) ((err e) (quote ())))) "(not (a b))") 0 3))))
"#;
    std::fs::write(&src_path, src).expect("write the aot source");

    typelisp::compile::aot::compile_file(src_path.to_str().unwrap(), out_path.to_str().unwrap())
        .expect("compile_file failed");
    let out = Command::new(&out_path).output().expect("failed to run the compiled executable");
    assert_eq!(
        out.status.code(),
        Some(0),
        "the executable exited {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_plain_reader_with_no_evaluator_says_so() {
    // `Reader::read` on its own — a unit test, or any caller that has no
    // program behind it. The registration cannot have come from anywhere, but
    // the message is the one that would be produced if it had.
    let mut heap = Heap::with_capacity(1 << 12);
    let f = Value::Int(0); // stands in for a function; never called
    heap.set_macro_character('!', f);
    let reader = Reader::new();
    let err = reader.read(&mut heap, "!x").expect_err("no evaluator was given");
    assert!(err.to_string().contains("reader macro"), "got {}", err);
}
