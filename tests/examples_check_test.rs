//! Every program under `examples/` type-checks, with no warnings.
//!
//! Nothing else runs them. When the default integer type became `int`, five
//! of the single-file examples and four of the projects stopped checking
//! (`dotimes`'s counter is an `int`, their bounds were `i32`), and no test
//! noticed: the only suite that reads example files is the LSP's, which
//! checks in error-recovery mode and so reports diagnostics instead of
//! failing.
//!
//! Checked, not run. Several examples are servers or read standard input, so
//! executing them would hang or depend on the environment; `Loader::load_entry`
//! checks each file and its `use`d dependencies and leaves execution to the
//! caller, which here simply does not do it.

extern crate typelisp;

mod common;
use common::repo_root;
use std::path::{Path as FsPath, PathBuf};
use typelisp::project::{find_src_root, Loader};
use typelisp::{load_prelude, Checker, Heap, Interp, Reader};

/// Every `.typl` file under `dir`, recursively, in a stable order.
fn typl_files(dir: &FsPath, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("readable directory").map(|e| e.unwrap().path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            typl_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "typl") {
            out.push(p);
        }
    }
}

/// Check `file` as an entry in a fresh session; the error or the warnings.
fn check(file: &FsPath) -> Result<Vec<String>, String> {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    let _ = checker.take_warnings();
    let dir = file.parent().expect("file has a parent").to_path_buf();
    let mut loader = Loader::new(find_src_root(&dir).unwrap_or(dir));
    let loaded = loader.load_entry(&mut heap, &reader, &mut checker, &mut interp, file);
    let warnings = checker.take_warnings();
    loaded.map_err(|e| e.to_string())?;
    Ok(warnings)
}

#[test]
fn every_example_checks_without_warnings() {
    let mut files = Vec::new();
    typl_files(&repo_root().join("examples"), &mut files);
    assert!(files.len() > 10, "expected the examples tree, found {:?}", files);
    let failures: Vec<String> = files
        .iter()
        .filter_map(|f| match check(f) {
            Ok(w) if w.is_empty() => None,
            Ok(w) => Some(format!("{}: {}", f.display(), w.join("; "))),
            Err(e) => Some(format!("{}: {}", f.display(), e)),
        })
        .collect();
    assert!(failures.is_empty(), "examples that do not check cleanly:\n{}", failures.join("\n"));
}
