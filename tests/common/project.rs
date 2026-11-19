//! Programs loaded through the real module loader, the way `typl` loads a
//! project.

use std::path::{Path, PathBuf};

use typelisp::project::{find_src_root, Loader};
use typelisp::{load_prelude, Checker, Heap, Interp, Reader, Value};

/// A heap, reader, checker and interpreter with the prelude loaded.
pub fn session() -> (Heap, Reader, Checker, Interp) {
    let mut heap = Heap::with_capacity(1 << 16);
    let reader = Reader::new();
    let mut checker = Checker::new();
    let mut interp = Interp::new();
    load_prelude(&mut heap, &mut checker, &mut interp);
    (heap, reader, checker, interp)
}

/// Load `entry` in `dir` through the real module loader and run what it
/// queued, returning the last top-level value or the first error's text.
pub fn run_project(dir: &Path, entry: &str) -> Result<Option<Value>, String> {
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

/// Create a fresh project dir named `name` containing `files`
/// (`(relative path, source)`, parent dirs created as needed), then
/// [`run_project`] it from `entry`.
pub fn run_fixture_project(name: &str, files: &[(&str, &str)], entry: &str) -> Result<Option<Value>, String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("module-test-tmp").join(name);
    let _ = std::fs::remove_dir_all(&dir); // stale fixtures from a previous run
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixture dirs");
        std::fs::write(&path, src).expect("write fixture file");
    }
    run_project(&dir, entry)
}
