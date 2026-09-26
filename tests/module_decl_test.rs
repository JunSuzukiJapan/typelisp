//! Stage 9a — the module-declaration forms: `in-module`, the variadic
//! `use`/`import`, and what happens when an import wants a bare name that is
//! already taken.
//!
//! Two things here are not CL, deliberately. There is no `in-package`: a file
//! already *is* a module (its path derives one), so there is nothing for a
//! form to select, and what a form can usefully do — nest further — is a
//! module operation and carries a module's name. And an import brings in one
//! named item rather than a package's whole exported set, so CL's `shadow`
//! and `unuse-package`, which exist to manage bulk inheritance, have nothing
//! to manage. See docs/dev/cl-parity-plan.md Stage 9a.

use std::path::PathBuf;

use typelisp::project::{find_src_root, Loader};
use typelisp::*;

/// Write `files` into a fresh directory of this test's own, load `entry`
/// through the real loader, run everything, and report the last top-level
/// value together with every warning the checker raised.
fn run(name: &str, files: &[(&str, &str)], entry: &str) -> Result<(Option<Value>, Vec<String>), String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("module-decl-tmp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
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
    let _ = checker.take_warnings(); // the prelude's own, if any

    let entry_path = dir.join(entry);
    let entry_dir = entry_path.parent().unwrap().to_path_buf();
    let src_root = find_src_root(&entry_dir).unwrap_or(entry_dir);
    let mut loader = Loader::new(src_root);

    let loaded = loader.load_entry(&mut heap, &reader, &mut checker, &mut interp, &entry_path);
    let warnings = checker.take_warnings();
    loaded.map_err(|e| e.to_string())?;

    let mut last = None;
    for tl in loader.take_pending() {
        match interp.exec(&mut heap, tl) {
            Ok(Some(v)) => last = Some(v),
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok((last, warnings))
}

/// [`run`] for the common case of one entry file and no expected warnings.
fn run_one(name: &str, src: &str) -> Result<Option<Value>, String> {
    let (v, warnings) = run(name, &[("main.typl", src)], "main.typl")?;
    assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
    Ok(v)
}

// ----------------------------------------------------------------------
// in-module
// ----------------------------------------------------------------------

#[test]
fn in_module_puts_the_rest_of_the_file_in_a_nested_module() {
    assert_eq!(
        run_one(
            "in_module_basic",
            "(in-module geometry)\n(defun area ((w int) (h int)) int (* w h))\n(area 3 4)\n",
        ),
        Ok(Some(Value::Int(12)))
    );
}

#[test]
fn what_in_module_entered_is_reachable_by_its_full_path() {
    // The file `main.typl` is module `main`, so `(in-module geometry)` nests
    // to `main::geometry` — the same path a written `(module geometry ...)`
    // would have produced.
    assert_eq!(
        run_one(
            "in_module_path",
            "(in-module geometry)\n(pub defun area ((w int) (h int)) int (* w h))\n(main::geometry::area 5 6)\n",
        ),
        Ok(Some(Value::Int(30)))
    );
}

#[test]
fn a_second_in_module_nests_inside_the_first() {
    assert_eq!(
        run_one(
            "in_module_nested",
            "(in-module a)\n(in-module b)\n(pub defun here () int 7)\n(main::a::b::here)\n",
        ),
        Ok(Some(Value::Int(7)))
    );
}

#[test]
fn in_module_does_not_leak_out_of_its_file() {
    // `dep.typl` enters a module of its own; `main.typl` must still be in
    // `main`, or its own `(pub defun ...)` would land somewhere else.
    let files = [
        ("dep.typl", "(in-module inner)\n(pub defun hidden () int 1)\n"),
        ("main.typl", "(use dep)\n(pub defun top () int 2)\n(main::top)\n"),
    ];
    let (v, warnings) = run("in_module_scope", &files, "main.typl").expect("load failed");
    assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
    assert_eq!(v, Some(Value::Int(2)));
}

#[test]
fn in_module_inside_a_written_module_ends_with_that_body() {
    assert_eq!(
        run_one(
            "in_module_in_module",
            "(module outer (in-module inner) (pub defun deep () int 3))\n\
             (pub defun shallow () int 4)\n\
             (+ (main::outer::inner::deep) (main::shallow))\n",
        ),
        Ok(Some(Value::Int(7)))
    );
}

#[test]
fn in_module_is_a_top_level_form_only() {
    let err = run_one("in_module_expr", "(defun f () int (progn (in-module x) 1))\n(f)\n")
        .expect_err("in-module in expression position");
    assert!(err.contains("only allowed at top level"), "unexpected error: {}", err);
}

#[test]
fn in_module_takes_exactly_one_path() {
    let err = run_one("in_module_arity", "(in-module a b)\n1\n").expect_err("two paths");
    assert!(err.contains("(in-module path)"), "unexpected error: {}", err);
}

// ----------------------------------------------------------------------
// use / import, one or many
// ----------------------------------------------------------------------

const TWO_HELPERS: [(&str, &str); 2] = [
    ("one.typl", "(pub defun twice ((n int)) int (* n 2))\n"),
    ("two.typl", "(pub defun quad ((n int)) int (* n 4))\n"),
];

#[test]
fn one_use_may_name_several_paths() {
    let mut files = TWO_HELPERS.to_vec();
    files.push(("main.typl", "(use one::twice two::quad)\n(+ (twice 10) (quad 10))\n"));
    let (v, warnings) = run("use_variadic", &files, "main.typl").expect("load failed");
    assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
    // Both files load: the dependency scan reads *every* path in the form,
    // not just the first.
    assert_eq!(v, Some(Value::Int(60)));
}

#[test]
fn import_is_the_same_act_as_use() {
    let mut files = TWO_HELPERS.to_vec();
    files.push(("main.typl", "(import one::twice)\n(twice 21)\n"));
    let (v, warnings) = run("import_alias", &files, "main.typl").expect("load failed");
    assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
    assert_eq!(v, Some(Value::Int(42)));
}

#[test]
fn import_also_loads_the_file_behind_the_path() {
    // The dependency scan has to know every spelling, or `import` resolves
    // only when something else happened to load the file first.
    let mut files = TWO_HELPERS.to_vec();
    files.push(("main.typl", "(import two::quad)\n(quad 5)\n"));
    let (v, _) = run("import_loads", &files, "main.typl").expect("load failed");
    assert_eq!(v, Some(Value::Int(20)));
}

// ----------------------------------------------------------------------
// collisions
// ----------------------------------------------------------------------

#[test]
fn an_import_shadowed_by_a_local_definition_is_reported() {
    // It used to be silent, and the import is the loser: bare-name resolution
    // puts this namespace's own definitions ahead of its aliases.
    let mut files = TWO_HELPERS.to_vec();
    files.push((
        "main.typl",
        "(defun twice ((n int)) int (+ n 1000))\n(use one::twice)\n(twice 21)\n",
    ));
    let (v, warnings) = run("clash_local", &files, "main.typl").expect("load failed");
    assert_eq!(v, Some(Value::Int(1021)), "the local definition still wins");
    assert_eq!(warnings.len(), 1, "expected one warning, got {:?}", warnings);
    assert!(warnings[0].contains("does nothing"), "unexpected warning: {}", warnings[0]);
    assert!(warnings[0].contains("twice"), "the warning names the name: {}", warnings[0]);
}

#[test]
fn using_a_type_defined_here_for_its_constructors_is_not_a_collision() {
    // `(use tree)` right after `(defenum tree ...)` is how a type's variants
    // become bare names. The alias names the very definition it would
    // "lose" to, so there is nothing to report — it used to say the import
    // did nothing, while removing it broke every bare `(node ...)`.
    let files = vec![(
        "main.typl",
        "(defenum tree (leaf) (node int))\n(use tree)\n(match (node 5) ((node n) n) ((leaf) 0))\n",
    )];
    let (v, warnings) = run("use_own_type", &files, "main.typl").expect("load failed");
    assert_eq!(v, Some(Value::Int(5)));
    assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
}

#[test]
fn the_order_of_the_two_does_not_change_who_wins() {
    let mut files = TWO_HELPERS.to_vec();
    files.push((
        "main.typl",
        "(use one::twice)\n(defun twice ((n int)) int (+ n 1000))\n(twice 21)\n",
    ));
    let (v, _) = run("clash_local_reversed", &files, "main.typl").expect("load failed");
    assert_eq!(v, Some(Value::Int(1021)));
}

#[test]
fn shadowing_import_says_the_collision_is_meant() {
    let mut files = TWO_HELPERS.to_vec();
    files.push((
        "main.typl",
        "(defun twice ((n int)) int (+ n 1000))\n(shadowing-import one::twice)\n(twice 21)\n",
    ));
    let (v, warnings) = run("clash_shadowing", &files, "main.typl").expect("load failed");
    assert!(warnings.is_empty(), "shadowing-import must be quiet: {:?}", warnings);
    // It still cannot beat a definition — nothing un-defines one. What it
    // buys is saying so.
    assert_eq!(v, Some(Value::Int(1021)));
}

#[test]
fn an_import_replacing_an_earlier_import_is_reported_too() {
    let files = [
        ("one.typl", "(pub defun same ((n int)) int (* n 2))\n"),
        ("two.typl", "(pub defun same ((n int)) int (* n 4))\n"),
        ("main.typl", "(use one::same)\n(use two::same)\n(same 10)\n"),
    ];
    let (v, warnings) = run("clash_import", &files, "main.typl").expect("load failed");
    // Here the *new* import does win — it replaces the alias.
    assert_eq!(v, Some(Value::Int(40)));
    assert_eq!(warnings.len(), 1, "expected one warning, got {:?}", warnings);
    assert!(warnings[0].contains("replaces an earlier import"), "unexpected warning: {}", warnings[0]);
}

#[test]
fn importing_the_same_path_twice_is_not_a_collision() {
    let mut files = TWO_HELPERS.to_vec();
    files.push(("main.typl", "(use one::twice)\n(use one::twice)\n(twice 21)\n"));
    let (v, warnings) = run("import_idempotent", &files, "main.typl").expect("load failed");
    assert!(warnings.is_empty(), "re-importing the same path must be quiet: {:?}", warnings);
    assert_eq!(v, Some(Value::Int(42)));
}

#[test]
fn a_module_reached_from_two_places_is_not_a_collision() {
    // `(use one)` binds the module name `one`; doing it twice — which happens
    // whenever two forms in a file need the same module — must stay quiet.
    let mut files = TWO_HELPERS.to_vec();
    files.push(("main.typl", "(use one)\n(use one)\n(one::twice 21)\n"));
    let (v, warnings) = run("module_twice", &files, "main.typl").expect("load failed");
    assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
    assert_eq!(v, Some(Value::Int(42)));
}
