//! `#.` — the reader running code while it reads (cl-parity-plan Stage 8c).
//!
//! This is the first thing the reader does that is not a function of its text
//! alone, and it is only possible because the drivers read one form at a time
//! now (Stage 8b-2): the form before it has already been checked, and on the
//! per-form-eval paths already *run*, when `#.` is reached.
//!
//! Which is also why what `#.` can call differs by path, exactly as it does
//! in CL:
//!
//! * `(load ...)` and the REPL evaluate each form as they read it, so a `#.`
//!   can call a function defined earlier in the same text — CL's `load`.
//! * A module file is checked as a unit and run by whoever `use`s it, so a
//!   `#.` there reaches only what has already run: the prelude and `use`d
//!   modules. This is CL's `compile-file`, where the same call needs an
//!   `(eval-when (:compile-toplevel) ...)` around the definition.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use typelisp::project::{find_src_root, load_file_flat, Loader};
use typelisp::*;

fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("read-time-eval-tmp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixture dirs");
        std::fs::write(&path, src).expect("write fixture file");
    }
    dir
}

fn session() -> (Heap, Reader, Checker, Interp) {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    (heap, reader, checker, interp)
}

fn eval_in(h: &mut Heap, r: &Reader, c: &mut Checker, i: &mut Interp, src: &str) -> Result<Option<Value>, String> {
    let forms = r.read_all(h, src).map_err(|e| e.to_string())?;
    let mut last = None;
    for v in forms {
        let tl = c.check_form(h, &*i, v).map_err(|e| e.to_string())?;
        let _ = c.take_warnings();
        last = i.exec(h, tl).map_err(|e| e.to_string())?;
    }
    Ok(last)
}

/// Load `entry` through the module loader and run what it queued.
fn run_project(dir: &std::path::Path, entry: &str) -> Result<Option<Value>, String> {
    let (mut heap, reader, mut checker, mut interp) = session();
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

// ----------------------------------------------------------------------
// the basics
// ----------------------------------------------------------------------

#[test]
fn a_read_time_form_is_replaced_by_its_value() {
    let dir = fixture("basic", &[("main.typl", "(defvar (n int) #.(+ 1 2))\nn\n")]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(3))));
}

#[test]
fn it_runs_while_reading_not_where_it_appears() {
    // The value is spliced in as a *datum*, so it can sit where no expression
    // could — here as the literal control string `println` insists on.
    let dir = fixture("as_literal", &[("main.typl", "(println #.(format false \"~a-~a\" \"a\" \"b\"))\n1\n")]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(1))));
}

#[test]
fn it_nests_inside_a_larger_form() {
    let dir = fixture("nested", &[("main.typl", "(defvar (n int) (+ 10 #.(* 2 5)))\nn\n")]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(20))));
}

#[test]
fn it_can_call_the_prelude() {
    let dir = fixture("prelude_call", &[("main.typl", "(defvar (n int) #.(abs -7))\nn\n")]);
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(7))));
}

#[test]
fn a_form_that_does_not_check_is_a_read_error_with_a_position() {
    let dir = fixture("bad_form", &[("main.typl", "(defvar (n int) #.(+ 1 \"two\"))\nn\n")]);
    let err = run_project(&dir, "main.typl").expect_err("the form does not type-check");
    assert!(err.contains("read error"), "expected a read error, got {}", err);
    assert!(err.contains("main.typl:1:"), "expected the position, got {}", err);
}

// ----------------------------------------------------------------------
// what it can reach, per path
// ----------------------------------------------------------------------

#[test]
fn on_the_load_path_it_reaches_a_definition_from_earlier_in_the_file() {
    // `(load ...)` runs each form as it reads it — CL's `load`.
    let dir = fixture(
        "load_path",
        &[("f.typl", "(defun triple ((n int)) int (* n 3))\n(defvar (baked int) #.(triple 5))\n")],
    );
    let (mut heap, reader, mut checker, mut interp) = session();
    load_file_flat(&mut heap, &reader, &mut checker, &mut interp, &dir, "f.typl").expect("load failed");
    assert_eq!(eval_in(&mut heap, &reader, &mut checker, &mut interp, "baked"), Ok(Some(Value::Int(15))));
}

#[test]
fn in_a_module_file_it_does_not_reach_that_file_s_own_definitions() {
    // The module is checked as a unit and run by its user, so the `defun`
    // above has been *checked* but not *run* — CL's `compile-file`, where the
    // same call needs an `eval-when` around the definition.
    let dir = fixture(
        "module_path",
        &[("main.typl", "(defun triple ((n int)) int (* n 3))\n(defvar (baked int) #.(triple 5))\nbaked\n")],
    );
    let err = run_project(&dir, "main.typl").expect_err("the definition has not run yet");
    assert!(err.contains("no such function"), "unexpected error: {}", err);
}

#[test]
fn in_a_module_file_it_does_not_reach_a_used_module_either() {
    // The dependency has been *checked* by now — the scan loaded it before
    // this form — but its forms have not run: the loader queues every
    // module's body and the driver runs the queue afterwards. It has to be
    // that way round, because the LSP is one of the drivers and must never
    // run the document it is checking.
    //
    // So in a module file a `#.` reaches the prelude and whatever the session
    // ran before this load. The paths that evaluate as they read — `load` and
    // the REPL — are where it reaches more.
    let dir = fixture(
        "used_module",
        &[
            ("helper.typl", "(pub defun triple ((n int)) int (* n 3))\n"),
            ("main.typl", "(use helper)\n(defvar (baked int) #.(helper::triple 5))\nbaked\n"),
        ],
    );
    let err = run_project(&dir, "main.typl").expect_err("the dependency has not run yet");
    assert!(err.contains("no such function"), "unexpected error: {}", err);
}

#[test]
fn the_repl_reaches_a_used_module() {
    // Here it does: the REPL runs each queued module as the form that `use`d
    // it is processed, so the next line's `#.` finds it.
    let dir = fixture("repl_use", &[("helper.typl", "(pub defun triple ((n int)) int (* n 3))\n")]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_typl"))
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start the typl binary");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"(use helper)\n#.(helper::triple 5)\n")
        .expect("failed to write stdin");
    let out = child.wait_with_output().expect("failed to wait on typl");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(text.contains("15"), "expected the read-time call's value, got {:?}", text);
}

#[test]
fn the_repl_reaches_what_earlier_lines_defined() {
    let out = repl_stdout("(defun quad ((n int)) int (* n 4))\n#.(quad 5)\n");
    assert!(out.contains("20"), "expected the read-time call's value, got {:?}", out);
}

// ----------------------------------------------------------------------
// where there is no evaluator
// ----------------------------------------------------------------------

#[test]
fn a_plain_read_says_it_cannot_run_anything() {
    // `Reader::read_all` has no evaluator — a `read-from-string` is not a
    // load. Saying so beats pretending the text is unreadable.
    let mut heap = Heap::with_capacity(1 << 12);
    let reader = Reader::new();
    let err = reader.read_all(&mut heap, "#.(+ 1 2)").expect_err("no evaluator");
    assert!(err.to_string().contains("no evaluator"), "unexpected error: {}", err);
}

#[test]
fn an_ordinary_hash_form_still_reads_without_an_evaluator() {
    let mut heap = Heap::with_capacity(1 << 12);
    let reader = Reader::new();
    let vs = reader.read_all(&mut heap, "#x10 #\\a").expect("read failed");
    assert_eq!(vs.len(), 2);
    assert_eq!(vs[0], Value::Int(16));
    assert_eq!(vs[1], Value::Char('a'));
}
