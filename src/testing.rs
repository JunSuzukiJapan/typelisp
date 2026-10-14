//! `typl test`: load a project with its `#[test]` functions in, and run each.
//!
//! What the checker does with `#[test]` is `check::TestForms`'s business; this
//! is the driver around it — which files make up the suite, and running the
//! tests one at a time so that one failing does not stop the rest.
//!
//! A test fails by `panic` (`assert`, `assert-eq`) or by any other runtime
//! error. Both come back from `Interp::exec` as an `Err`, as they do at the
//! REPL, after which the next form runs normally — the same recovery the REPL
//! relies on after an error at its prompt.

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path as FsPath, PathBuf};
use std::rc::Rc;

use crate::project::{find_project, module_segs_for, Loader};
use crate::{Checker, Heap, Interp, Reader, TestCase, TestForms, Value};

/// The folder of a project whose files are loaded besides its sources.
pub const TESTS_DIR: &str = "tests";

/// The files `typl test` loads, each with the module it is.
pub struct Suite {
    src_root: PathBuf,
    files: Vec<(PathBuf, Vec<String>)>,
}

impl Suite {
    /// The project around `start_dir` (the nearest `typelisp.toml` above it):
    /// every `.typl` file under its source root, then every one under its
    /// `tests/` folder. A file in `tests/` is the module its path from the
    /// project folder says — `tests/parse.typl` is `tests::parse` — whether or
    /// not the source root is that folder.
    pub fn of_project(start_dir: &FsPath) -> Result<Suite, String> {
        let (project_dir, src_root) = find_project(start_dir).ok_or_else(|| {
            format!(
                "no {} in {} or any folder above it; to test one file, name it: typl test FILE.typl",
                crate::project::MANIFEST_NAME,
                start_dir.display()
            )
        })?;
        let mut files = Vec::new();
        for file in typl_files_under(&src_root)? {
            let segs = module_segs_for(&file, &src_root).map_err(|e| e.to_string())?;
            files.push((file, segs));
        }
        let tests_dir = project_dir.join(TESTS_DIR);
        if tests_dir.is_dir() && !tests_dir.starts_with(&src_root) {
            for file in typl_files_under(&tests_dir)? {
                let segs = module_segs_for(&file, &project_dir).map_err(|e| e.to_string())?;
                files.push((file, segs));
            }
        }
        Ok(Suite { src_root, files })
    }

    /// One file, and whatever it `use`s.
    pub fn of_file(file: &FsPath) -> Result<Suite, String> {
        if !file.is_file() {
            return Err(format!("{}: no such file", file.display()));
        }
        let dir = file.parent().filter(|p| !p.as_os_str().is_empty()).map(FsPath::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        let src_root = crate::project::find_src_root(&dir).unwrap_or(dir);
        let segs = module_segs_for(file, &src_root).map_err(|e| e.to_string())?;
        Ok(Suite { src_root, files: vec![(file.to_path_buf(), segs)] })
    }
}

/// Every `.typl` file under `dir`, in path order. A folder whose name starts
/// with `.` is not looked into.
fn typl_files_under(dir: &FsPath) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = std::fs::read_dir(&d).map_err(|e| format!("{}: {}", d.display(), e))?;
        for entry in entries {
            let path = entry.map_err(|e| format!("{}: {}", d.display(), e))?.path();
            if path.is_dir() {
                if !path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "typl") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// How a run went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub passed: usize,
    pub failed: usize,
    /// Tests whose name does not contain the filter.
    pub filtered_out: usize,
}

/// Load `suite` into an environment that already has the prelude in, then run
/// every test whose full name contains `filter`, reporting on `out`.
///
/// `Err` is a suite that could not be loaded — a type error anywhere in it
/// means no test runs, as a compile error does under `cargo test`.
pub fn run(
    suite: &Suite,
    filter: Option<&str>,
    heap: &mut Heap,
    reader: &Reader,
    mut checker: Checker,
    interp: &mut Interp,
    out: &mut dyn Write,
) -> Result<Outcome, String> {
    checker.set_test_forms(TestForms::Collect);
    let mut loader = Loader::new(suite.src_root.clone());
    for (file, segs) in &suite.files {
        let result = loader.load_module_file(heap, reader, &mut checker, interp, file, segs.clone());
        for w in checker.take_warnings() {
            eprintln!("{}", w);
        }
        result.map_err(|e| e.to_string())?;
    }
    for tl in loader.take_pending() {
        interp.exec(heap, tl).map_err(|e| e.to_string())?;
    }
    let tests = checker.take_tests();
    // From here on the checker checks the calls that run the tests, which are
    // top-level expressions it would otherwise leave out.
    checker.set_test_forms(TestForms::Check);
    let checker = Rc::new(RefCell::new(checker));
    interp.set_checker(Rc::clone(&checker));

    let (selected, skipped): (Vec<_>, Vec<_>) =
        tests.into_iter().partition(|t| filter.is_none_or(|f| t.path.to_string().contains(f)));
    let mut outcome = Outcome { passed: 0, failed: 0, filtered_out: skipped.len() };
    let mut failures = Vec::new();
    let io = |e: std::io::Error| e.to_string();
    writeln!(out, "running {} test{}", selected.len(), if selected.len() == 1 { "" } else { "s" }).map_err(io)?;
    out.flush().map_err(io)?;
    for t in &selected {
        let mark = heap.root_count();
        let result = run_one(heap, &checker, interp, t);
        while heap.root_count() > mark {
            heap.pop_root();
        }
        match result {
            Ok(()) => {
                outcome.passed += 1;
                writeln!(out, "test {} ... ok", t.path).map_err(io)?;
            }
            Err(e) => {
                outcome.failed += 1;
                writeln!(out, "test {} ... FAILED", t.path).map_err(io)?;
                failures.push((t.path.to_string(), e));
            }
        }
        out.flush().map_err(io)?;
    }
    if !failures.is_empty() {
        writeln!(out, "\nfailures:").map_err(io)?;
        for (name, message) in &failures {
            writeln!(out, "\n---- {} ----\n{}", name, message).map_err(io)?;
        }
    }
    writeln!(
        out,
        "\ntest result: {}. {} passed; {} failed; {} filtered out",
        if outcome.failed == 0 { "ok" } else { "FAILED" },
        outcome.passed,
        outcome.failed,
        outcome.filtered_out
    )
    .map_err(io)?;
    out.flush().map_err(io)?;
    Ok(outcome)
}

/// Check `(NAME)` inside the test's module and run it. A test returning
/// `Result<(), E>` is called as
/// `(match (NAME) ((ok _) ()) ((err e) (panic (format false "..." (print-object e true)))))`,
/// the `panic` carrying the test's own location, so that is where a failure
/// is reported. Roots are left for the caller to release.
fn run_one(heap: &mut Heap, checker: &Rc<RefCell<Checker>>, interp: &mut Interp, t: &TestCase) -> Result<(), String> {
    let err = |e: crate::Error| e.to_string();
    let mut call = list(heap, &[t.name]).map_err(err)?;
    if t.returns_result {
        let sym = |heap: &mut Heap, s: &str| heap.intern_symbol(s);
        let (e, ok, er, under) = (sym(heap, "e"), sym(heap, "ok"), sym(heap, "err"), sym(heap, "_"));
        let print = [sym(heap, "print-object"), e, Value::Bool(true)];
        let print = list(heap, &print).map_err(err)?;
        let message = heap.alloc_string("the test returned an error: ~a".to_string());
        heap.push_root(message);
        let format = [sym(heap, "format"), Value::Bool(false), message, print];
        let format = list(heap, &format).map_err(err)?;
        let panic = [sym(heap, "panic"), format];
        let panic = list(heap, &panic).map_err(err)?;
        if let (Value::Cons(cr), Some(loc)) = (panic, t.loc.clone()) {
            heap.set_cons_loc(cr, loc);
        }
        let err_pat = list(heap, &[er, e]).map_err(err)?;
        let err_arm = list(heap, &[err_pat, panic]).map_err(err)?;
        let ok_pat = list(heap, &[ok, under]).map_err(err)?;
        let ok_arm = list(heap, &[ok_pat, Value::Empty]).map_err(err)?;
        let head = sym(heap, "match");
        call = list(heap, &[head, call, ok_arm, err_arm]).map_err(err)?;
    }
    // The borrow ends before the call runs: an `eval` in the test borrows the
    // checker again.
    let tl = {
        let mut c = checker.borrow_mut();
        c.set_current_ns(t.ns.clone());
        let tl = c.check_form(heap, &*interp, call);
        for w in c.take_warnings() {
            eprintln!("{}", w);
        }
        tl.map_err(|e| e.to_string())?
    };
    interp.exec(heap, tl).map(|_| ()).map_err(|e| e.to_string())
}

/// `(ITEMS...)`, rooted, as is each cell on the way.
fn list(heap: &mut Heap, items: &[Value]) -> Result<Value, crate::Error> {
    let mut v = Value::Empty;
    for &item in items.iter().rev() {
        v = heap.cons(item, v)?;
        heap.push_root(v);
    }
    Ok(v)
}
