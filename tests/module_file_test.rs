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

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use typelisp::project::{find_src_root, Loader, ModuleCache};
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

/// Like [`run_project`], but drives its own fresh `Heap`/`Checker`/`Interp`
/// (as every separate LSP `diagnostics_for` pass would) while sharing the
/// caller-supplied `cache` across the call — so a test can call this twice
/// with the same `cache` and observe the second pass's `Loader::cache_stats`.
/// Returns `(result, cache hits, cache misses)`.
fn run_project_with_cache(
    dir: &std::path::Path,
    entry: &str,
    cache: &ModuleCache,
) -> (Result<Option<RtValue>, String>, usize, usize) {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);

    let entry_path = dir.join(entry);
    let entry_dir = entry_path.parent().unwrap().to_path_buf();
    let src_root = find_src_root(&entry_dir).unwrap_or(entry_dir);
    let mut loader = Loader::new(src_root);
    loader.set_module_cache(cache.clone());

    let load_result = loader.load_entry(&mut heap, &reader, &mut checker, &mut interp, &entry_path).map_err(|e| e.to_string());
    let (hits, misses) = loader.cache_stats();
    let result = load_result.and_then(|()| {
        let mut last = None;
        for tl in loader.take_pending() {
            match interp.exec(&mut heap, tl) {
                Ok(Some(v)) => last = Some(v),
                Ok(None) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(last)
    });
    (result, hits, misses)
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

/// A second, independent load session (fresh heap/checker/interp, same
/// project directory) sharing one `ModuleCache` with the first hits the
/// cache for its dependency and skips re-checking it — while still
/// producing the exact same evaluated result.
#[test]
fn a_second_load_session_hits_the_module_cache() {
    let dir = write_project(
        "cache-hit",
        &[
            ("geo/point.typl", "(pub defun origin-x () i32 42)"),
            ("main.typl", "(use geo::point)\n(point::origin-x)"),
        ],
    );
    let cache: ModuleCache = Rc::new(RefCell::new(HashMap::new()));

    let (r1, hits1, misses1) = run_project_with_cache(&dir, "main.typl", &cache);
    assert_eq!(r1, Ok(Some(RtValue::Int(42))));
    assert_eq!((hits1, misses1), (0, 1), "first pass: miss then populate the cache");

    let (r2, hits2, misses2) = run_project_with_cache(&dir, "main.typl", &cache);
    assert_eq!(r2, Ok(Some(RtValue::Int(42))));
    assert_eq!((hits2, misses2), (1, 0), "second pass: cache hit, no re-check");
}

/// Editing the cached dependency invalidates the cache entry (by content
/// hash, not mtime) — the next load re-checks it from source and sees the
/// new definition, not a stale cached one.
#[test]
fn editing_a_cached_dependency_invalidates_its_cache_entry() {
    let dir = write_project(
        "cache-invalidate",
        &[
            ("geo/point.typl", "(pub defun origin-x () i32 42)"),
            ("main.typl", "(use geo::point)\n(point::origin-x)"),
        ],
    );
    let cache: ModuleCache = Rc::new(RefCell::new(HashMap::new()));

    let (r1, ..) = run_project_with_cache(&dir, "main.typl", &cache);
    assert_eq!(r1, Ok(Some(RtValue::Int(42))));

    std::fs::write(dir.join("geo/point.typl"), "(pub defun origin-x () i32 99)").unwrap();

    let (r2, hits2, misses2) = run_project_with_cache(&dir, "main.typl", &cache);
    assert_eq!(r2, Ok(Some(RtValue::Int(99))), "edited dependency's new definition must be seen");
    assert_eq!((hits2, misses2), (0, 1), "stale entry must miss, not silently serve old code");
}

/// A dependency that itself has a `(load ...)` form opts out of caching (see
/// `LoadOutcome::cacheable`'s doc comment) — every pass is a miss, but
/// correctness (not caching) is what actually matters here.
#[test]
fn a_dependency_with_a_nested_load_is_never_cached() {
    let dir = write_project(
        "cache-load-form",
        &[
            ("loaded.typl", "(pub defun helper () i32 7)"),
            ("geo/point.typl", "(load \"../loaded\")\n(pub defun origin-x () i32 (helper))"),
            ("main.typl", "(use geo::point)\n(point::origin-x)"),
        ],
    );
    let cache: ModuleCache = Rc::new(RefCell::new(HashMap::new()));

    let (r1, ..) = run_project_with_cache(&dir, "main.typl", &cache);
    assert_eq!(r1, Ok(Some(RtValue::Int(7))));

    let (r2, hits2, misses2) = run_project_with_cache(&dir, "main.typl", &cache);
    assert_eq!(r2, Ok(Some(RtValue::Int(7))));
    assert_eq!((hits2, misses2), (0, 1), "a nested (load) dependency must never be cached");
}

/// A transitive dependency's own change invalidates the *cache entry that
/// pulled it in*, even though that entry's own file is untouched — proving
/// `ModuleCacheEntry::deps` tracks the whole transitive closure, not just
/// the cached file's own hash. `main` only ever `use`s `mid` directly; `leaf`
/// is reached solely through `mid`'s own `use`.
#[test]
fn changing_a_transitive_dependency_invalidates_the_cache_entry_that_pulled_it_in() {
    let dir = write_project(
        "cache-transitive",
        &[
            ("leaf.typl", "(pub defun leaf-value () i32 1)"),
            ("mid.typl", "(use leaf)\n(pub defun call-it () i32 (leaf::leaf-value))"),
            ("main.typl", "(use mid)\n(mid::call-it)"),
        ],
    );
    let cache: ModuleCache = Rc::new(RefCell::new(HashMap::new()));

    let (r1, hits1, misses1) = run_project_with_cache(&dir, "main.typl", &cache);
    assert_eq!(r1, Ok(Some(RtValue::Int(1))));
    assert_eq!((hits1, misses1), (0, 2), "first pass: miss for both mid and leaf");

    let (r2, hits2, misses2) = run_project_with_cache(&dir, "main.typl", &cache);
    assert_eq!(r2, Ok(Some(RtValue::Int(1))));
    assert_eq!((hits2, misses2), (1, 0), "second pass: one hit for `mid` covers `leaf` transitively");

    std::fs::write(dir.join("leaf.typl"), "(pub defun leaf-value () i32 2)").unwrap();

    let (r3, hits3, misses3) = run_project_with_cache(&dir, "main.typl", &cache);
    assert_eq!(r3, Ok(Some(RtValue::Int(2))), "leaf's new definition must be seen through mid");
    assert_eq!((hits3, misses3), (0, 2), "leaf's change must invalidate mid's cache entry too");
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

/// A `use` inside `geo/point.typl` names a sibling `geo/vector.typl` by its
/// bare name (`vector`), without spelling out the `geo::` directory prefix —
/// the sibling-relative resolution `Loader::ensure_loaded`'s doc comment and
/// `Checker::find_module`'s file-sibling tier add.
#[test]
fn use_resolves_a_sibling_file_by_its_bare_name() {
    let result = run_project(
        "sibling-bare-name",
        &[
            ("geo/vector.typl", "(pub defun unit-x () i32 1)"),
            ("geo/point.typl", "(use vector)\n(pub defun call-it () i32 (vector::unit-x))"),
            ("main.typl", "(use geo::point)\n(point::call-it)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(1))));
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
            ("helper.typl", "(pub defun which () i32 100)"), // root-relative
            ("geo/helper.typl", "(pub defun which () i32 200)"), // same-named sibling
            ("geo/point.typl", "(use helper)\n(pub defun call-it () i32 (helper::which))"),
            ("main.typl", "(use geo::point)\n(point::call-it)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(100))), "root-relative `helper` wins over the sibling");
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
            ("geo/vector.typl", "(pub defun unit-x () i32 5)"),
            ("geo/point.typl", "(module inner (use vector)\n(pub defun call-it () i32 (vector::unit-x)))"),
            ("main.typl", "(use geo::point::inner)\n(inner::call-it)"),
        ],
        "main.typl",
    );
    assert_eq!(result, Ok(Some(RtValue::Int(5))));
}
