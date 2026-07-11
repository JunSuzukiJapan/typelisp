//! Tests for the file-to-module mapping and on-demand module loading
//! (`typelisp::project` — see its doc comment for the rules): a file's path
//! relative to the source root is its module path, `use` loads the file it
//! maps to, cycles are a hard error.
//!
//! Every test builds its own project directory under
//! `target/module-test-tmp/<test name>` (unique per test so parallel `cargo
//! test` threads never collide, the same scheme `compile_file_test.rs` uses)
//! and drives the real `Loader` + `Interp::exec` pipeline, asserting on the
//! last executed top-level expression's value.

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

#[test]
fn use_loads_a_sibling_file_and_calls_into_it() {
    let result = run_project(
        "basic",
        &[
            ("geo/point.typl", "(pub defun origin-x () i32 42)"),
            ("main.typl", "(use geo::point)\n(point::origin-x)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(42))));
}

#[test]
fn nested_directories_become_nested_path_segments() {
    let result = run_project(
        "nested-dirs",
        &[
            ("a/b/c.typl", "(pub defun f () i32 7)"),
            ("main.typl", "(use a::b::c)\n(c::f)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(7))));
}

#[test]
fn explicit_module_nests_inside_the_derived_file_module() {
    // `(module inner ...)` in util.typl lives at util::inner — inside the
    // file's derived path, never beside or above it.
    let result = run_project(
        "nested-module",
        &[
            ("util.typl", "(module inner (pub defun g () i32 11))"),
            ("main.typl", "(use util::inner)\n(inner::g)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(11))));
}

#[test]
fn item_level_use_finds_the_file_by_longest_prefix() {
    // `use geo::point::origin-x` names an *item*; no geo/point/origin-x.typl
    // exists, so the loader falls back to geo/point.typl and the checker
    // then resolves the item as a bare-name alias.
    let result = run_project(
        "item-use",
        &[
            ("geo/point.typl", "(pub defun origin-x () i32 5)"),
            ("main.typl", "(use geo::point::origin-x)\n(origin-x)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(5))));
}

#[test]
fn circular_dependency_is_a_hard_error_with_the_chain() {
    let result = run_project(
        "cycle",
        &[
            ("a.typl", "(use b)\n(pub defun fa () i32 1)"),
            ("b.typl", "(use a)\n(pub defun fb () i32 2)"),
            ("main.typl", "(use a)\n(a::fa)"),
        ],
        "main.typl",
    );
    let err = result.unwrap_err();
    assert!(err.contains("circular module dependency: a -> b -> a"), "got: {}", err);
}

#[test]
fn use_of_a_module_with_no_file_reports_unresolved() {
    let result = run_project("unresolved", &[("main.typl", "(use nosuch)")], "main.typl");
    let err = result.unwrap_err();
    assert!(err.contains("use: unresolved `nosuch`"), "got: {}", err);
}

#[test]
fn defvar_initializers_run_deferred_but_before_the_dependent() {
    // `xs` is a heap-allocated Sexpr list: its initializer must not run
    // mid-load (while read roots are still on the stack) but must have run
    // by the time main's expression executes. Getting `1` back proves both.
    let result = run_project(
        "defvar-defer",
        &[
            (
                "dep.typl",
                "(pub defvar (xs Sexpr) '(1 2 3))\n(pub defun head () i64 (sexpr-int (sexpr-car xs)))",
            ),
            ("main.typl", "(use dep)\n(dep::head)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(1))));
}

#[test]
fn manifest_src_key_moves_the_source_root() {
    // With `src = "src"`, files under src/ get root-level module paths
    // (`lib`, not `src::lib`).
    let result = run_project(
        "manifest-src",
        &[
            ("typelisp.toml", "src = \"src\"\n"),
            ("src/lib.typl", "(pub defun answer () i32 40)"),
            ("src/main.typl", "(use lib)\n(+ (lib::answer) 2)"),
        ],
        "src/main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(42))));
}

#[test]
fn without_a_manifest_the_entry_directory_is_the_root() {
    let result = run_project(
        "no-manifest",
        &[
            ("helper.typl", "(pub defun three () i32 3)"),
            ("main.typl", "(use helper)\n(helper::three)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(3))));
}
