//! Tests for macro-generated top-level forms — most importantly a `defmacro`
//! whose expansion is a `(use ...)`: the expansion must both load the
//! dependency file (the pre-check scan cannot see it, so
//! `Loader::load_source_inner`'s check loop pre-expands and re-scans) and be
//! dispatched as a *top-level* form (`Checker::check_form_dispatch`'s macro
//! re-dispatch), not as an expression. Previously such a `use` failed with a
//! misleading "unbound variable" — the known limitation recorded in
//! `docs/dev/TODO.md` until 2026-07-15.
//!
//! Project-driving tests reuse `module_file_test.rs`'s fixture scheme:
//! each test builds its own directory under
//! `target/module-test-tmp/<test name>` and runs the real `Loader` +
//! `Interp::exec` pipeline.

use std::path::PathBuf;

use typelisp::project::{find_src_root, Loader};
use typelisp::*;

/// Create a fresh project dir containing `files` (`(relative path, source)`,
/// parent dirs created as needed), then load `entry` through the real
/// loader pipeline and execute everything. Returns the last top-level
/// expression value, or the first error as its display string.
fn run_project(name: &str, files: &[(&str, &str)], entry: &str) -> Result<Option<RtValue>, String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("module-test-tmp").join(name);
    let _ = std::fs::remove_dir_all(&dir); // stale fixtures from a previous run
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixture dirs");
        std::fs::write(&path, src).expect("write fixture file");
    }

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

/// Read, check, and execute `src` with no file/loader involved (the REPL's
/// shape: `Checker::check_form` driven directly in the root namespace).
/// Returns the last expression's value or the first error's display string.
fn run_forms(src: &str) -> Result<RtValue, String> {
    let mut h = Heap::with_capacity(1 << 16);
    let mut chk = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut h, &mut chk, &mut interp);
    let r = Reader::new();
    let vs = r.read_all(&mut h, src).map_err(|e| e.to_string())?;
    let mut last = RtValue::Unit;
    for v in vs {
        let tl = chk.check_form(&mut h, &interp, v).map_err(|e| e.to_string())?;
        if let Some(val) = interp.exec(&mut h, tl).map_err(|e| e.to_string())? {
            last = val;
        }
    }
    Ok(last)
}

/// The headline case: a macro defined earlier in the same file expands to
/// `(use dep)`. The dependency file must get loaded (invisible to the
/// pre-check scan) and the alias must work for the rest of the file.
#[test]
fn macro_generated_use_loads_the_dependency() {
    let result = run_project(
        "macro-use-basic",
        &[
            ("dep.typl", "(pub defun head () i32 9)"),
            (
                "main.typl",
                "(defmacro import-dep () '(use dep))\n(import-dep)\n(dep::head)",
            ),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(9))));
}

/// A macro expanding to another macro call that finally yields the `use` —
/// the loader's pre-expansion loop must follow the whole chain.
#[test]
fn macro_chain_ending_in_use() {
    let result = run_project(
        "macro-use-chain",
        &[
            ("dep.typl", "(pub defun head () i32 13)"),
            (
                "main.typl",
                "(defmacro import-dep () '(use dep))\n\
                 (defmacro import-indirect () '(import-dep))\n\
                 (import-indirect)\n\
                 (dep::head)",
            ),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(13))));
}

/// A macro expanding to a whole `(module ...)` form whose body contains the
/// `use` — `scan_form`'s recursion into module bodies must see the expansion.
/// The final `(dep::head)` resolves only because the module-body `use` got
/// its file loaded (a fully-qualified path needs no alias, just the loaded
/// module) — and the `use` itself checking cleanly inside `inner` proves the
/// same. (`dep::head` can't appear inside the quoted expansion itself:
/// `::`-paths inside quoted data are a separate, pre-existing limitation.)
#[test]
fn macro_generated_module_containing_use() {
    let result = run_project(
        "macro-use-module",
        &[
            ("dep.typl", "(pub defun head () i32 21)"),
            (
                "main.typl",
                "(defmacro make-inner () '(module inner (use dep)))\n\
                 (make-inner)\n\
                 (dep::head)",
            ),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(21))));
}

/// A macro-generated `use` inside a subdirectory file resolves a sibling by
/// its bare name, exactly like a literal `use` — and the dependency load
/// happens mid-check of `geo/point.typl`, exercising the namespace-context
/// suspension (`Checker::suspend_ns_context`) with a non-empty saved context.
#[test]
fn macro_generated_use_resolves_a_sibling_file() {
    let result = run_project(
        "macro-use-sibling",
        &[
            ("geo/vector.typl", "(pub defun unit-x () i32 8)"),
            (
                "geo/point.typl",
                "(defmacro import-vec () '(use vector))\n\
                 (import-vec)\n\
                 (pub defun call-it () i32 (vector::unit-x))",
            ),
            ("main.typl", "(use geo::point)\n(point::call-it)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(8))));
}

/// A macro-generated `use` of a module with no file behind it reports the
/// same clean "unresolved" error a literal one does.
#[test]
fn macro_generated_use_of_a_missing_module_reports_unresolved() {
    let result = run_project(
        "macro-use-unresolved",
        &[("main.typl", "(defmacro imp () '(use nosuch))\n(imp)")],
        "main.typl",
    );
    let err = result.unwrap_err();
    assert!(err.contains("use: unresolved `nosuch`"), "got: {}", err);
}

/// An expansion *error* (here: arity) is ignored by the loader's scan-side
/// pre-expansion and reported once, by the checker, as the macro's own error.
#[test]
fn failing_macro_expansion_reports_at_check_time() {
    let result = run_project(
        "macro-use-arity",
        &[
            ("dep.typl", "(pub defun head () i32 9)"),
            ("main.typl", "(defmacro imp () '(use dep))\n(imp 1)"),
        ],
        "main.typl",
    );
    let err = result.unwrap_err();
    assert!(err.contains("macro `imp` expects 0 argument(s), got 1"), "got: {}", err);
}

/// `use` reaching expression position (e.g. via a function body) is a clear
/// dedicated error, not the misleading "unbound variable" it used to be.
#[test]
fn use_in_expression_position_is_a_clear_error() {
    let result = run_project(
        "macro-use-expr-pos",
        &[("main.typl", "(defun f () i32 (use dep))")],
        "main.typl",
    );
    let err = result.unwrap_err();
    assert!(err.contains("use: only allowed at top level"), "got: {}", err);
}

/// The checker-side half in isolation (no loader): a top-level macro call
/// expanding to a *definition* form is dispatched as a top-level form — a
/// macro can generate a `defun`, not just an expression.
#[test]
fn macro_generated_defun_defines_a_callable_function() {
    let result = run_forms(
        "(defmacro make-forty () '(defun forty () i32 40))\n(make-forty)\n(forty)",
    );
    assert_eq!(result, Ok(RtValue::Int(40)));
}
