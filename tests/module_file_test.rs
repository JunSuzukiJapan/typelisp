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
fn run_project(name: &str, files: &[(&str, &str)], entry: &str) -> Result<Option<Value>, String> {
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

/// Like [`run_project`], but over a project directory the caller already
/// wrote — so a test can load the *same* directory twice, in two independent
/// sessions (fresh `Heap`/`Checker`/`Interp` each, as every separate LSP
/// `diagnostics_for` pass is), with an edit in between.
fn run_project_in_dir(dir: &std::path::Path, entry: &str) -> Result<Option<Value>, String> {
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
        .map_err(|e| e.to_string())
        .and_then(|()| {
            let mut last = None;
            for tl in loader.take_pending() {
                match interp.exec(&mut heap, tl) {
                    Ok(Some(v)) => last = Some(v),
                    Ok(None) => {}
                    Err(e) => return Err(e.to_string()),
                }
            }
            Ok(last)
        })
}

/// Write `files` (`(relative path, source)`) under a fresh
/// `target/module-test-tmp/<name>` directory, returning that directory.
fn write_project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("module-test-tmp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixture dirs");
        std::fs::write(&path, src).expect("write fixture file");
    }
    dir
}

/// A second, independent load session over the same project directory sees a
/// dependency edited in between — nothing from the first session survives to
/// serve a stale definition.
#[test]
fn a_second_load_session_sees_an_edited_dependency() {
    let dir = write_project(
        "reload-edited-dep",
        &[
            ("geo/point.typl", "(pub defun origin-x () int 42)"),
            ("main.typl", "(use geo::point)\n(point::origin-x)"),
        ],
    );

    assert_eq!(run_project_in_dir(&dir, "main.typl"), Ok(Some(Value::Int(42))));

    std::fs::write(dir.join("geo/point.typl"), "(pub defun origin-x () int 99)").unwrap();

    assert_eq!(
        run_project_in_dir(&dir, "main.typl"),
        Ok(Some(Value::Int(99))),
        "edited dependency's new definition must be seen"
    );
}

/// A dependency reached through `use` may itself contain a `(load ...)` form:
/// the loaded file's definitions land in the dependency's own environment and
/// its functions are callable from the entry file, in a fresh session as well
/// as the first.
#[test]
fn a_dependency_with_a_nested_load_works_in_every_session() {
    let dir = write_project(
        "nested-load-dep",
        &[
            ("loaded.typl", "(pub defun helper () int 7)"),
            ("geo/point.typl", "(load \"../loaded\")\n(pub defun origin-x () int (helper))"),
            ("main.typl", "(use geo::point)\n(point::origin-x)"),
        ],
    );

    assert_eq!(run_project_in_dir(&dir, "main.typl"), Ok(Some(Value::Int(7))));
    assert_eq!(run_project_in_dir(&dir, "main.typl"), Ok(Some(Value::Int(7))));
}

/// A *transitive* dependency's change is seen by a later session even though
/// the file the entry `use`s directly is untouched: `main` only ever `use`s
/// `mid`, and `leaf` is reached solely through `mid`'s own `use`.
#[test]
fn a_later_session_sees_a_changed_transitive_dependency() {
    let dir = write_project(
        "reload-transitive-dep",
        &[
            ("leaf.typl", "(pub defun leaf-value () int 1)"),
            ("mid.typl", "(use leaf)\n(pub defun call-it () int (leaf::leaf-value))"),
            ("main.typl", "(use mid)\n(mid::call-it)"),
        ],
    );

    assert_eq!(run_project_in_dir(&dir, "main.typl"), Ok(Some(Value::Int(1))));

    std::fs::write(dir.join("leaf.typl"), "(pub defun leaf-value () int 2)").unwrap();

    assert_eq!(
        run_project_in_dir(&dir, "main.typl"),
        Ok(Some(Value::Int(2))),
        "leaf's new definition must be seen through mid"
    );
}

#[test]
fn use_loads_a_sibling_file_and_calls_into_it() {
    let result = run_project(
        "basic",
        &[
            ("geo/point.typl", "(pub defun origin-x () int 42)"),
            ("main.typl", "(use geo::point)\n(point::origin-x)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(42))));
}

#[test]
fn nested_directories_become_nested_path_segments() {
    let result = run_project(
        "nested-dirs",
        &[
            ("a/b/c.typl", "(pub defun f () int 7)"),
            ("main.typl", "(use a::b::c)\n(c::f)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(7))));
}

#[test]
fn explicit_module_nests_inside_the_derived_file_module() {
    // `(module inner ...)` in util.typl lives at util::inner — inside the
    // file's derived path, never beside or above it.
    let result = run_project(
        "nested-module",
        &[
            ("util.typl", "(module inner (pub defun g () int 11))"),
            ("main.typl", "(use util::inner)\n(inner::g)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(11))));
}

#[test]
fn item_level_use_finds_the_file_by_longest_prefix() {
    // `use geo::point::origin-x` names an *item*; no geo/point/origin-x.typl
    // exists, so the loader falls back to geo/point.typl and the checker
    // then resolves the item as a bare-name alias.
    let result = run_project(
        "item-use",
        &[
            ("geo/point.typl", "(pub defun origin-x () int 5)"),
            ("main.typl", "(use geo::point::origin-x)\n(origin-x)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(5))));
}

#[test]
fn circular_dependency_is_a_hard_error_with_the_chain() {
    let result = run_project(
        "cycle",
        &[
            ("a.typl", "(use b)\n(pub defun fa () int 1)"),
            ("b.typl", "(use a)\n(pub defun fb () int 2)"),
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
                "(pub defvar (xs Option<Sexpr>) '(1 2 3))\n(pub defun head () int (sexpr-int (sexpr-car xs)))",
            ),
            ("main.typl", "(use dep)\n(dep::head)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(1))));
}

#[test]
fn manifest_src_key_moves_the_source_root() {
    // With `src = "src"`, files under src/ get root-level module paths
    // (`lib`, not `src::lib`).
    let result = run_project(
        "manifest-src",
        &[
            ("typelisp.toml", "src = \"src\"\n"),
            ("src/lib.typl", "(pub defun answer () int 40)"),
            ("src/main.typl", "(use lib)\n(+ (lib::answer) 2)"),
        ],
        "src/main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(42))));
}

#[test]
fn without_a_manifest_the_entry_directory_is_the_root() {
    let result = run_project(
        "no-manifest",
        &[
            ("helper.typl", "(pub defun three () int 3)"),
            ("main.typl", "(use helper)\n(helper::three)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(3))));
}

/// A `use` inside `geo/point.typl` names a sibling `geo/vector.typl` by its
/// bare name (`vector`), without spelling out the `geo::` directory prefix —
/// the sibling-relative resolution `Loader::ensure_loaded`'s doc comment and
/// `Checker::find_module`'s file-sibling tier add.
#[test]
fn use_resolves_a_sibling_file_by_its_bare_name() {
    let result = run_project(
        "sibling-bare-name",
        &[
            ("geo/vector.typl", "(pub defun unit-x () int 1)"),
            ("geo/point.typl", "(use vector)\n(pub defun call-it () int (vector::unit-x))"),
            ("main.typl", "(use geo::point)\n(point::call-it)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(1))));
}

/// Sibling-relative resolution is a fallback tried only *after* root-relative
/// resolution fails — an existing root-relative `use` naming a top-level
/// module keeps working unchanged even from inside a subdirectory, and is
/// never shadowed by a same-named sibling file.
#[test]
fn use_prefers_a_root_relative_module_over_a_same_named_sibling() {
    let result = run_project(
        "sibling-vs-root",
        &[
            ("helper.typl", "(pub defun which () int 100)"), // root-relative
            ("geo/helper.typl", "(pub defun which () int 200)"), // same-named sibling
            ("geo/point.typl", "(use helper)\n(pub defun call-it () int (helper::which))"),
            ("main.typl", "(use geo::point)\n(point::call-it)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(100))), "root-relative `helper` wins over the sibling");
}

/// A `use` inside a nested `(module inner ...)` block still resolves a
/// sibling by the *enclosing file's* directory, not that inner block's own
/// (filesystem-less) namespace — `Checker::file_ns` stays pinned to the
/// file's own path across nested `module` forms.
#[test]
fn use_inside_a_nested_module_still_resolves_against_the_files_directory() {
    let result = run_project(
        "sibling-nested-module",
        &[
            ("geo/vector.typl", "(pub defun unit-x () int 5)"),
            ("geo/point.typl", "(module inner (use vector)\n(pub defun call-it () int (vector::unit-x)))"),
            ("main.typl", "(use geo::point::inner)\n(inner::call-it)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(5))));
}

// ---- prelude/builtin visibility from a bare entry file -----------------
//
// `typl <file>` wraps its entry file in a module named after the file
// (`module_segs_for`) while the prelude and Rust builtins live at the root
// namespace, all non-`pub`. Bare-name/assoc-fn resolution used to only check
// "current module or literal root" — root was never reachable from *inside*
// a submodule, so every prelude/builtin call from a standalone file failed
// with "no such function". Fixed by walking the full ancestor chain
// (`Checker::ns_ancestors`/`in_scope`) instead — root is always an ancestor.

#[test]
fn bare_prelude_fn_is_reachable_from_a_wrapped_entry_file() {
    // `not` is a plain (non-pub) `defun` in the prelude, at root. `main.typl`
    // is wrapped into module `main` — `not` must still resolve there.
    let result = run_project("prelude-not", &[("main.typl", "(not false)")], "main.typl");
    assert_eq!(result, Ok(Some(Value::Bool(true))));
}

#[test]
fn bare_prelude_fn_and_struct_field_accessor_are_reachable_from_a_wrapped_entry_file() {
    // `cons` (a non-pub free function) builds a `cons-cell<A,B>`, and `car`
    // (a non-pub `defstruct` field accessor) reads it back — the former
    // exercises `resolve_fn`'s ancestor-chain walk, the latter
    // `assoc_visible`'s root-is-always-in-scope case.
    let result = run_project("prelude-cons", &[("main.typl", "(car (cons 1 2))")], "main.typl");
    assert_eq!(result, Ok(Some(Value::Int(1))));
}

#[test]
fn builtin_option_ctor_and_method_are_reachable_from_a_wrapped_entry_file() {
    // `Option::some` is a builtin enum constructor (`Registry::with_builtins`,
    // never `pub`), and `unwrap` is a `defmethod` in the prelude dispatched
    // through the receiver's type — both live at root.
    let result = run_project("prelude-option", &[("main.typl", "(unwrap (Option::some 5))")], "main.typl");
    assert_eq!(result, Ok(Some(Value::Int(5))));
}

#[test]
fn a_grandchild_module_sees_a_non_pub_ancestors_definitions() {
    // Rust-style module privacy: a private item is visible to its defining
    // module and every descendant, not just call sites in the exact same
    // module. `a::b` (nested two levels under the file module `main`) calls
    // a non-pub helper defined directly in `a`.
    let result = run_project(
        "nested-ancestor-privacy",
        &[(
            "main.typl",
            "(module a \
               (defun helper () int 42) \
               (module b (pub defun call-it () int (helper)))) \
             (a::b::call-it)",
        )],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(42))));
}

#[test]
fn sibling_modules_still_cannot_see_each_others_private_items() {
    // Regression guard: the ancestor-chain relaxation must not leak private
    // items sideways between modules that aren't in an ancestor/descendant
    // relationship with each other.
    let result = run_project(
        "sibling-privacy-regression",
        &[(
            "main.typl",
            "(module a (defun hidden () int 1)) \
             (module b (pub defun call-it () int (a::hidden))) \
             (b::call-it)",
        )],
        "main.typl",
    );
    assert!(result.is_err(), "sibling module `b` must not see `a`'s private `hidden`");
}

/// `cond`'s and `case`'s `else` are recognised by symbol identity in the core
/// macros, and a file module reads its own symbols: unless `else` is in the
/// vocabulary every module imports, a script's `(else ...)` is a different
/// symbol from the macro's and the clause is checked as a variable
/// reference. This used to fail with `unbound variable: else` in every
/// `typl file.typl` run while working at the REPL.
#[test]
fn cond_and_case_else_work_inside_a_file_module() {
    let result = run_project(
        "cond-else",
        &[
            ("util.typl", "(pub defun pick ((n int)) int (cond ((< n 0) -1) (else (case n (0 0) (else 1)))))"),
            ("main.typl", "(use util::pick)\n(+ (pick -5) (pick 0) (pick 7))"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(Value::Int(0))));
}
