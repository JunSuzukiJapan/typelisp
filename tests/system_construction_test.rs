//! Stage 9b — what a program can say about its own construction:
//! `defvar` versus `defparameter`, and where the source it was read from is.
//!
//! Two of the Stage's items are not here, and the reasons are worth as much as
//! the code. `compile-file-pathname` computes the name of the `.fasl` a CL
//! `compile-file` writes; here `compile-file` links a **native executable**
//! whose name the caller chooses, and there is no compiled-module format at
//! all (language-design.md §0), so there is no name to derive. And `require` /
//! `provide` / `*modules*` describe a registry of loaded modules that `use`
//! already is: it loads a module's file on demand, exactly once, keyed by the
//! module path. See docs/dev/cl-parity-plan.md Stage 9b.

use std::path::PathBuf;

use typelisp::project::load_file_flat;
use typelisp::*;

fn session() -> (Heap, Reader, Checker, Interp) {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    (heap, reader, checker, interp)
}

/// Check and run `src`'s forms in an existing session, returning the last
/// value. Errors are reported as their display string.
fn eval_in(
    heap: &mut Heap,
    reader: &Reader,
    checker: &mut Checker,
    interp: &mut Interp,
    src: &str,
) -> Result<Option<Value>, String> {
    let forms = reader.read_all(heap, src).map_err(|e| e.to_string())?;
    let mut last = None;
    for v in forms {
        let tl = checker.check_form(heap, &*interp, v).map_err(|e| e.to_string())?;
        let _ = checker.take_warnings();
        last = interp.exec(heap, tl).map_err(|e| e.to_string())?;
    }
    Ok(last)
}

/// A fresh directory of this test's own holding `files`.
fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("system-construction-tmp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixture dirs");
        std::fs::write(&path, src).expect("write fixture file");
    }
    dir
}

// ----------------------------------------------------------------------
// defvar / defparameter
// ----------------------------------------------------------------------

#[test]
fn defvar_leaves_an_already_bound_global_alone() {
    let (mut heap, reader, mut checker, mut interp) = session();
    let run = |h: &mut Heap, c: &mut Checker, i: &mut Interp, s: &str| eval_in(h, &reader, c, i, s);

    run(&mut heap, &mut checker, &mut interp, "(defvar (x i32) 1)").expect("first defvar");
    run(&mut heap, &mut checker, &mut interp, "(setf x 99)").expect("assignment");
    run(&mut heap, &mut checker, &mut interp, "(defvar (x i32) 1)").expect("second defvar");
    assert_eq!(run(&mut heap, &mut checker, &mut interp, "x"), Ok(Some(Value::Int(99))));
}

#[test]
fn defparameter_assigns_every_time() {
    let (mut heap, reader, mut checker, mut interp) = session();
    let run = |h: &mut Heap, c: &mut Checker, i: &mut Interp, s: &str| eval_in(h, &reader, c, i, s);

    run(&mut heap, &mut checker, &mut interp, "(defparameter (x i32) 1)").expect("first");
    run(&mut heap, &mut checker, &mut interp, "(setf x 99)").expect("assignment");
    run(&mut heap, &mut checker, &mut interp, "(defparameter (x i32) 1)").expect("second");
    assert_eq!(run(&mut heap, &mut checker, &mut interp, "x"), Ok(Some(Value::Int(1))));
}

#[test]
fn a_second_defvar_does_not_even_evaluate_its_initializer() {
    // Not just "the value is kept": the initializer's side effects must not
    // happen again either, which is the half that matters for an initializer
    // that opens a file or counts something.
    let (mut heap, reader, mut checker, mut interp) = session();
    let run = |h: &mut Heap, c: &mut Checker, i: &mut Interp, s: &str| eval_in(h, &reader, c, i, s);

    run(&mut heap, &mut checker, &mut interp, "(defvar (calls i32) 0)").expect("counter");
    run(&mut heap, &mut checker, &mut interp, "(defun bump () i32 (progn (setf calls (+ calls 1)) 7))")
        .expect("bump");
    run(&mut heap, &mut checker, &mut interp, "(defvar (v i32) (bump))").expect("first");
    run(&mut heap, &mut checker, &mut interp, "(defvar (v i32) (bump))").expect("second");
    assert_eq!(run(&mut heap, &mut checker, &mut interp, "calls"), Ok(Some(Value::Int(1))));
}

#[test]
fn a_second_defparameter_does_evaluate_its_initializer() {
    let (mut heap, reader, mut checker, mut interp) = session();
    let run = |h: &mut Heap, c: &mut Checker, i: &mut Interp, s: &str| eval_in(h, &reader, c, i, s);

    run(&mut heap, &mut checker, &mut interp, "(defvar (calls i32) 0)").expect("counter");
    run(&mut heap, &mut checker, &mut interp, "(defun bump () i32 (progn (setf calls (+ calls 1)) 7))")
        .expect("bump");
    run(&mut heap, &mut checker, &mut interp, "(defparameter (v i32) (bump))").expect("first");
    run(&mut heap, &mut checker, &mut interp, "(defparameter (v i32) (bump))").expect("second");
    assert_eq!(run(&mut heap, &mut checker, &mut interp, "calls"), Ok(Some(Value::Int(2))));
}

#[test]
fn loading_the_same_file_twice_keeps_what_the_session_stored() {
    // The reason CL draws the distinction at all: a file of settings can be
    // re-loaded after an edit without throwing away the ones already changed.
    let dir = fixture(
        "load_twice",
        &[("config.typl", "(defvar (verbose bool) false)\n(defparameter (reset bool) false)\n")],
    );
    let (mut heap, reader, mut checker, mut interp) = session();

    load_file_flat(&mut heap, &reader, &mut checker, &mut interp, &dir, "config.typl").expect("first load");
    eval_in(&mut heap, &reader, &mut checker, &mut interp, "(setf verbose true)").expect("customize");
    eval_in(&mut heap, &reader, &mut checker, &mut interp, "(setf reset true)").expect("customize");
    load_file_flat(&mut heap, &reader, &mut checker, &mut interp, &dir, "config.typl").expect("second load");

    assert_eq!(
        eval_in(&mut heap, &reader, &mut checker, &mut interp, "verbose"),
        Ok(Some(Value::Bool(true))),
        "a `defvar` keeps the session's value"
    );
    assert_eq!(
        eval_in(&mut heap, &reader, &mut checker, &mut interp, "reset"),
        Ok(Some(Value::Bool(false))),
        "a `defparameter` is put back"
    );
}

#[test]
fn defparameter_can_be_public() {
    let (mut heap, reader, mut checker, mut interp) = session();
    assert_eq!(
        eval_in(&mut heap, &reader, &mut checker, &mut interp, "(pub defparameter (n i32) 5)\nn"),
        Ok(Some(Value::Int(5)))
    );
}

#[test]
fn defconstant_still_refuses_assignment() {
    let (mut heap, reader, mut checker, mut interp) = session();
    eval_in(&mut heap, &reader, &mut checker, &mut interp, "(defconstant (k i32) 5)").expect("defconstant");
    let err = eval_in(&mut heap, &reader, &mut checker, &mut interp, "(setf k 6)")
        .expect_err("a constant cannot be assigned");
    assert!(!err.is_empty(), "expected a message");
}

// ----------------------------------------------------------------------
// source-file
// ----------------------------------------------------------------------

#[test]
fn source_file_names_the_file_the_form_was_read_from() {
    let dir = fixture("source_file", &[("where.typl", "(defvar (here string) (source-file))\n")]);
    let (mut heap, reader, mut checker, mut interp) = session();
    load_file_flat(&mut heap, &reader, &mut checker, &mut interp, &dir, "where.typl").expect("load");

    let v = eval_in(&mut heap, &reader, &mut checker, &mut interp, "here").expect("read it back");
    let Some(Value::Str(id)) = v else { panic!("expected a string, got {:?}", v) };
    let text = heap.string(id).to_string();
    assert!(text.ends_with("where.typl"), "expected the file's own path, got {:?}", text);
}

#[test]
fn source_file_composes_with_the_pathname_functions() {
    let dir = fixture(
        "source_dir",
        &[("where.typl", "(defvar (dir string) (directory-namestring (source-file)))\n")],
    );
    let (mut heap, reader, mut checker, mut interp) = session();
    load_file_flat(&mut heap, &reader, &mut checker, &mut interp, &dir, "where.typl").expect("load");

    let v = eval_in(&mut heap, &reader, &mut checker, &mut interp, "dir").expect("read it back");
    let Some(Value::Str(id)) = v else { panic!("expected a string, got {:?}", v) };
    let text = heap.string(id).to_string();
    assert!(text.ends_with('/'), "a directory namestring keeps its slash: {:?}", text);
    assert!(!text.ends_with("where.typl"), "the file name is gone: {:?}", text);
}

#[test]
fn source_file_is_a_string_at_check_time() {
    // It is folded to a literal, so it can be used where a literal is
    // required — `println`'s control string is the strictest such place.
    let dir = fixture("source_literal", &[("where.typl", "(defvar (n i32) (length (source-file)))\n")]);
    let (mut heap, reader, mut checker, mut interp) = session();
    load_file_flat(&mut heap, &reader, &mut checker, &mut interp, &dir, "where.typl").expect("load");
    let v = eval_in(&mut heap, &reader, &mut checker, &mut interp, "n").expect("read it back");
    assert!(matches!(v, Some(Value::Int(n)) if n > 0), "expected a positive length, got {:?}", v);
}

#[test]
fn source_file_takes_no_arguments() {
    let (mut heap, reader, mut checker, mut interp) = session();
    let err = eval_in(&mut heap, &reader, &mut checker, &mut interp, "(source-file \"x\")")
        .expect_err("an argument");
    assert!(err.contains("takes no arguments"), "unexpected error: {}", err);
}
