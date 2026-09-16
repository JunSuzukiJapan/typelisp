//! The loader's own GC-root discipline.
//!
//! `Loader::load_source_inner` accumulates one file's checked top-level forms
//! in a `Vec<Value>` and only wraps them into a `(module PATH BODY...)` bundle
//! at the end. A `Vec<Value>` is invisible to the collector, so every member
//! has to be rooted as it lands — and stay rooted until the bundle that will
//! hold them exists and is itself rooted.
//!
//! Getting that wrong is *silent* and load-dependent: an earlier form is
//! collected, its cell recycled into whatever the next form's checking
//! allocates, and the bundle ends up holding a fragment. It surfaces much
//! later, at exec, as `not a top-level core form: (())` — the same signature
//! `Checker::check_impl`'s own `body` vector produced before it was fixed.
//!
//! Found on 2026-08-18 by an unrelated change shifting allocation just enough
//! to cross the threshold on a default-sized heap, which is exactly why this
//! test pins it down with a *deliberately tight* one instead of hoping the
//! default stays on the right side.

use std::path::PathBuf;

use typelisp::project::{find_src_root, Loader};
use typelisp::*;

/// Loads `src` as a one-file project on a heap of `cells` cons cells and runs
/// every queued form, returning the error string if any stage fails.
fn load_and_run(name: &str, src: &str, cells: usize) -> Result<(), String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("module-test-tmp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    let entry = dir.join("main.typl");
    std::fs::write(&entry, src).expect("write fixture");

    let mut heap = Heap::with_capacity(cells);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    typelisp::load_compiler_aot(&mut heap, &mut checker, &mut interp);

    let entry_dir = entry.parent().unwrap().to_path_buf();
    let src_root = find_src_root(&entry_dir).unwrap_or(entry_dir);
    let mut loader = Loader::new(src_root);
    loader
        .load_entry(&mut heap, &reader, &mut checker, &mut interp, &entry)
        .map_err(|e| e.to_string())?;
    for tl in loader.take_pending() {
        interp.exec(&mut heap, tl).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// A file whose *later* forms allocate heavily while its earlier ones sit in
/// the pending `body` vector, on a heap small enough that a collection is
/// certain to happen in between.
///
/// The bodies are deliberately allocation-hungry at *check* time (nested
/// calls, each building its own node) rather than at run time: the window
/// this guards is between one form being checked and the whole file's bundle
/// being built.
#[test]
fn a_files_earlier_forms_survive_the_checking_of_its_later_ones() {
    let mut src = String::from("(defun to-s ((b bool)) string (if b \"T\" \"F\"))\n");
    for i in 0..12 {
        src.push_str(&format!(
            "(defun f{i} ((a int) (b int)) string\n  \
               (append (append (append (to-s (eq a b)) (to-s (eql a b))) \
                               (to-s (equal a b))) (to-s (equalp a b))))\n"
        ));
    }
    src.push_str("(defun main-report () string (append (f0 1 1) (f11 1 2)))\n");

    // 1 << 14 cells is well under what checking this file allocates in total,
    // so the collector runs several times mid-load.
    load_and_run("loader_gc_tight", &src, 1 << 14).expect("a tight heap must not corrupt the pending bundle");
}

/// The same file at the default heap size, so a regression that only shows up
/// with more headroom (a root released too early rather than never taken) is
/// still caught.
#[test]
fn the_same_file_loads_on_a_default_sized_heap() {
    let mut src = String::from("(defun to-s ((b bool)) string (if b \"T\" \"F\"))\n");
    for i in 0..12 {
        src.push_str(&format!(
            "(defun g{i} ((a int) (b int)) string\n  \
               (append (append (append (to-s (eq a b)) (to-s (eql a b))) \
                               (to-s (equal a b))) (to-s (equalp a b))))\n"
        ));
    }
    load_and_run("loader_gc_default", &src, 1 << 16).expect("default heap");
}
