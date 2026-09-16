//! The per-form read→check→eval loop.
//!
//! Every driver that loads source used to read it to the end first and only
//! then check and run what it had read. That made the text a fixed thing,
//! decided before any of it ran: the form that would change how the rest is
//! read (a reader macro, a `#+` feature the file itself adds) has not run
//! when the rest is read, so there is nowhere to put one. Each driver now
//! reads one form, checks it, runs it, and only then reads the next
//! (`Reader::forms_in` / `Forms::next_form`).
//!
//! What that is worth is tested here in the only way it is observable before
//! reader macros exist (Phase 8c): **a form after a broken one is not needed
//! to run the forms before it**. A syntax error at the end of a batch used to
//! stop the batch entirely; the forms ahead of it have now already run, which
//! is what `load` and a REPL have always meant in Common Lisp.
//!
//! Two drivers are checked directly here — the flat loader (`(load ...)`, the
//! REPL's own file loading) and the `typl` REPL over piped stdin. The module
//! loader (`project::Loader`) reads per form too, but deliberately does *not*
//! run a module file's forms as it goes: a module is checked as a unit and
//! executed by whoever `use`s it, the way CL's `compile-file` processes a
//! file without evaluating it, with `needs_immediate_exec` playing the part
//! of `(eval-when (:compile-toplevel) ...)`. The LSP depends on that — it
//! checks documents for diagnostics and must never run them.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use typelisp::project::{find_src_root, load_file_flat, Loader};
use typelisp::*;

/// A fresh directory under `target/read-check-eval-tmp/<name>` holding
/// `files` — one per test, so parallel test threads never collide (the same
/// scheme `module_file_test.rs` uses).
fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("read-check-eval-tmp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixture dirs");
        std::fs::write(&path, src).expect("write fixture file");
    }
    dir
}

/// A session with the prelude loaded.
fn session() -> (Heap, Reader, Checker, Interp) {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    (heap, reader, checker, interp)
}

/// Check and run one source form in an existing session, returning its value.
fn eval_in(heap: &mut Heap, reader: &Reader, checker: &mut Checker, interp: &mut Interp, src: &str) -> Option<Value> {
    let forms = reader.read_all(heap, src).expect("read failed");
    let mut last = None;
    for v in forms {
        let tl = checker.check_form(heap, &*interp, v).expect("check failed");
        last = interp.exec(heap, tl).expect("exec failed");
    }
    last
}

/// Feed `input` to the `typl` REPL over stdin and return its stdout.
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
// The reader's streaming API
// ----------------------------------------------------------------------

#[test]
fn next_form_hands_over_one_datum_at_a_time_then_reports_the_end() {
    let mut heap = Heap::with_capacity(1 << 12);
    let reader = Reader::new();
    let mut forms = reader.forms("1 2 3");

    let mut seen = Vec::new();
    while let Some((v, _)) = forms.next_form(&mut heap).expect("read failed") {
        seen.push(v);
    }
    assert_eq!(seen, vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    // And it keeps saying so.
    assert!(forms.next_form(&mut heap).expect("read failed").is_none());
}

#[test]
fn the_text_after_the_current_form_has_not_been_looked_at() {
    // The whole point: `(garbage` is never read, because reading stops after
    // the first datum and nothing asks for another.
    let mut heap = Heap::with_capacity(1 << 12);
    let reader = Reader::new();
    let mut forms = reader.forms("42 (garbage");
    assert_eq!(forms.next_form(&mut heap).expect("read failed").map(|(v, _)| v), Some(Value::Int(42)));
}

#[test]
fn pos_splits_the_source_into_what_was_read_and_what_was_not() {
    let mut heap = Heap::with_capacity(1 << 12);
    let reader = Reader::new();
    let src = "(a b) (c";
    let mut forms = reader.forms(src);
    forms.next_form(&mut heap).expect("read failed").expect("a first form");
    // The datum ends where it ends: the whitespace after it has not been
    // consumed, so it belongs to the unread tail. That is what the REPL keeps
    // when the last form turns out to be incomplete.
    let tail: String = src.chars().skip(forms.pos()).collect();
    assert_eq!(tail, " (c");
}

#[test]
fn each_form_is_rooted_as_it_is_handed_over() {
    // Same contract `read_all_in_spanned` has: a form stays alive across the
    // checking of the forms before it, and the caller pops.
    let mut heap = Heap::with_capacity(1 << 12);
    let reader = Reader::new();
    let mark = heap.root_count();
    let mut forms = reader.forms("(a) (b) (c)");
    for expected in 1..=3 {
        forms.next_form(&mut heap).expect("read failed").expect("a form");
        assert_eq!(heap.root_count(), mark + expected);
    }
}

#[test]
fn a_broken_form_is_reported_only_when_it_is_reached() {
    let mut heap = Heap::with_capacity(1 << 12);
    let reader = Reader::new();
    let mut forms = reader.forms("1 (unterminated");
    assert!(forms.next_form(&mut heap).expect("read failed").is_some());
    let err = forms.next_form(&mut heap).expect_err("the second form is incomplete");
    assert!(
        matches!(err.kind(), Error::IllegalEndWhileReadingList),
        "expected an incomplete-list read error, got {}",
        err
    );
}

#[test]
fn read_all_returns_exactly_what_the_stream_does() {
    // `read_all_in_spanned` is built on `forms`; one implementation, so the
    // two cannot drift.
    let mut heap = Heap::with_capacity(1 << 12);
    let reader = Reader::new();
    let src = "(defun f () int 1) 2 \"three\"";

    let all: Vec<Value> = reader.read_all(&mut heap, src).expect("read failed");
    let mut forms = reader.forms(src);
    let mut streamed = Vec::new();
    while let Some((v, _)) = forms.next_form(&mut heap).expect("read failed") {
        streamed.push(v);
    }
    assert_eq!(all.len(), streamed.len());
    for (a, b) in all.iter().zip(streamed.iter()) {
        assert_eq!(check::core::print(&heap, *a), check::core::print(&heap, *b));
    }
}

// ----------------------------------------------------------------------
// The flat loader: `(load ...)` runs each form before reading the next
// ----------------------------------------------------------------------

#[test]
fn a_loaded_file_runs_the_forms_before_a_syntax_error() {
    let dir = fixture(
        "flat_syntax_error",
        &[("broken.typl", "(defvar (probe int) 0)\n(setf probe 42)\n(this-form-never-closes\n")],
    );
    let (mut heap, reader, mut checker, mut interp) = session();

    let err = load_file_flat(&mut heap, &reader, &mut checker, &mut interp, &dir, "broken.typl")
        .expect_err("the last form is incomplete");
    assert!(format!("{}", err).contains("end"), "expected an end-of-input read error, got {}", err);

    // The two forms ahead of the broken one have already run — which is only
    // possible because the file is not read to the end before it is executed.
    assert_eq!(eval_in(&mut heap, &reader, &mut checker, &mut interp, "probe"), Some(Value::Int(42)));
}

#[test]
fn a_loaded_file_runs_the_forms_before_a_type_error() {
    let dir = fixture(
        "flat_type_error",
        &[("broken.typl", "(defvar (probe int) 0)\n(setf probe 7)\n(+ 1 \"not a number\")\n")],
    );
    let (mut heap, reader, mut checker, mut interp) = session();

    load_file_flat(&mut heap, &reader, &mut checker, &mut interp, &dir, "broken.typl")
        .expect_err("the last form does not type-check");
    assert_eq!(eval_in(&mut heap, &reader, &mut checker, &mut interp, "probe"), Some(Value::Int(7)));
}

#[test]
fn a_loaded_file_still_runs_every_form_when_nothing_is_broken() {
    let dir = fixture("flat_ok", &[("fine.typl", "(defvar (probe int) 0)\n(setf probe 1)\n(setf probe 2)\n")]);
    let (mut heap, reader, mut checker, mut interp) = session();

    load_file_flat(&mut heap, &reader, &mut checker, &mut interp, &dir, "fine.typl").expect("load failed");
    assert_eq!(eval_in(&mut heap, &reader, &mut checker, &mut interp, "probe"), Some(Value::Int(2)));
}

// ----------------------------------------------------------------------
// The module loader: `use` is a dependency of what follows it
// ----------------------------------------------------------------------

/// Load `entry` in `dir` through the real module loader and run what it
/// queued, returning the last top-level value or the first error's text.
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

#[test]
fn a_use_above_the_reference_resolves() {
    let dir = fixture(
        "use_above",
        &[
            ("helper.typl", "(pub defun twice ((n int)) int (* n 2))\n"),
            ("main.typl", "(use helper)\n(helper::twice 21)\n"),
        ],
    );
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(42))));
}

#[test]
fn a_use_below_the_reference_no_longer_reaches_back() {
    // The deliberate consequence of scanning dependencies per form instead of
    // in a pass over the whole file: a `use` is a dependency of the forms
    // after it. Every convention already puts them at the top.
    let dir = fixture(
        "use_below",
        &[
            ("helper.typl", "(pub defun twice ((n int)) int (* n 2))\n"),
            ("main.typl", "(defun main () int (helper::twice 21))\n(use helper)\n"),
        ],
    );
    let err = run_project(&dir, "main.typl").expect_err("the reference precedes its `use`");
    assert!(err.contains("unresolved path: helper::twice"), "unexpected error: {}", err);
}

#[test]
fn a_macro_defined_earlier_in_the_file_is_expanded_when_a_later_form_is_scanned() {
    // The dependency scan sees this file's own macros now, because it runs
    // per form rather than before any of them is checked.
    let dir = fixture(
        "macro_then_use",
        &[
            ("helper.typl", "(pub defun twice ((n int)) int (* n 2))\n"),
            (
                "main.typl",
                "(defmacro bring () `(use helper))\n(bring)\n(defun main () int (helper::twice 4))\n(main)\n",
            ),
        ],
    );
    assert_eq!(run_project(&dir, "main.typl"), Ok(Some(Value::Int(8))));
}

// ----------------------------------------------------------------------
// The REPL
// ----------------------------------------------------------------------

#[test]
fn the_repl_runs_a_form_before_the_broken_one_after_it() {
    let out = repl_stdout("(println \"ran\") (\n");
    assert!(out.contains("ran"), "expected the first form to have run, got {:?}", out);
}

#[test]
fn the_repl_runs_a_form_before_a_type_error_in_the_same_batch() {
    let out = repl_stdout("(println \"ran\") (+ 1 \"x\")\n");
    assert!(out.contains("ran"), "expected the first form to have run, got {:?}", out);
}

#[test]
fn an_incomplete_trailing_form_does_not_replay_the_ones_already_run() {
    // The forms before it have *run*; keeping them in the pending buffer
    // while waiting for the rest of the last one would run them twice.
    let out = repl_stdout("(println \"once\") (+ 1\n2)\n");
    assert_eq!(out.matches("once").count(), 1, "expected exactly one run, got {:?}", out);
    assert!(out.contains('3'), "expected the completed form's value, got {:?}", out);
}

#[test]
fn the_repl_still_runs_a_whole_good_batch_in_order() {
    let out = repl_stdout("(println \"a\") (println \"b\") (println \"c\")\n");
    let a = out.find('a').expect("a");
    let b = out.find('b').expect("b");
    let c = out.find('c').expect("c");
    assert!(a < b && b < c, "expected declaration order, got {:?}", out);
}
