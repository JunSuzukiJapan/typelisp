//! Keeps cons-building on the core-IR paths going through `check::core`.
//!
//! `Heap::cons` collects whenever the free list is empty, so every value built
//! but not yet reachable from a root can be freed by the *next* allocation.
//! Nothing in the type system says so: `heap.cons(a, b)` compiles fine whether
//! or not `a` survived being built. The consequences are real and were live in
//! this repo — `Checker::subst_value` and `forms::list_from_vec_locs` both dropped
//! freshly rebuilt syntax on the floor, which `tests/checker_gc_stress_test.rs`
//! reproduces as a mangled `impl` receiver and a runaway `setf` expansion.
//!
//! `check::core`'s builders (`pair`, `list`, `tagged`, `Items`) root as they
//! go, so this test keeps new call sites pointed at them: a raw `.cons(` in a
//! scanned file has to say why it is not using them. See [`scanned_files`] for
//! which files those are and why the set is not just the checker.
//!
//! **When this test fails**, the question to ask is what the site is building:
//!
//! | building | use |
//! |---|---|
//! | a core form node `(tag field...)` | `core::tagged` / `core::Items` |
//! | a bare list of already-built values | `core::list` |
//! | a `(a . b)` pair that is not a list | `core::pair` |
//! | read *syntax*, user data, or a runtime environment | root it yourself, and mark it |
//!
//! A site in the last row opts out with a `// core-build-ok: <reason>` comment
//! on the offending line or in the comment block directly above it — and the
//! reason has to say how the intermediates stay rooted, because that is what a
//! later reader needs to check.
//!
//! Modelled on `tests/type_identity_guard_test.rs`, which polices the other
//! invariant that the type system cannot.

mod common;
use common::{repo_root};
use std::path::PathBuf;

/// The opt-out marker, honoured on the offending line or the block above it.
const MARKER: &str = "core-build-ok:";

struct Hit {
    file: String,
    line: usize,
    text: String,
}

/// Every `.rs` file that builds or restores core IR: the checker that emits
/// it, and the evaluator and bridge that consume and rebuild it.
///
/// `check/core.rs` is excluded — it is the one place whose whole job is to
/// cons correctly. So is the `typelisp-read` crate: the reader is where cells
/// come from, and its subject is syntax rather than core forms.
///
/// The scan started at `check/` alone, which was narrower than the hazard.
/// `Heap::cons` does not care which module calls it, and the two biggest
/// consumers of the IR — `compile::core_bridge` and `eval::interp::core_eval`
/// — were building the same nodes by hand, outside it.
///
/// The roots are spelled out rather than derived, and a missing one panics:
/// when the checker and the interpreter moved into `typelisp-front`, that
/// panic is what said so.
fn scanned_files() -> Vec<PathBuf> {
    let front = repo_root().join("crates/typelisp-front/src");
    let mut out = Vec::new();
    let mut stack = vec![front.join("check"), repo_root().join("src/compile"), front.join("eval")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{} is readable: {}", dir.display(), e)) {
            let p = entry.expect("readable dir entry").path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "rs") && p.file_name().is_some_and(|n| n != "core.rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    assert!(out.len() >= 12, "the scan found only {} files — is the walk broken?", out.len());
    out
}

#[test]
fn cons_calls_go_through_check_core() {
    let mut hits: Vec<Hit> = Vec::new();

    for path in scanned_files() {
        let rel = path.strip_prefix(repo_root()).unwrap().to_string_lossy().replace('\\', "/");
        let src = std::fs::read_to_string(&path).expect("source is readable");
        let lines: Vec<&str> = src.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            // A test module builds fixtures on purpose, and its conses are read
            // next to the assertions that depend on them. The rule is about the
            // lowering paths, so the scan stops where those end.
            if trimmed.starts_with("#[cfg(test)]") {
                break;
            }
            // Comments describe the rule at least as often as they break it.
            if trimmed.starts_with("//") {
                continue;
            }
            // `heap.cons(`, `s.cons(`, ... but not `list_to_vec`/`cons_loc`.
            if !line.contains(".cons(") {
                continue;
            }

            // The marker may sit on the line itself or anywhere in the comment
            // block directly above it, so a multi-line reason reads naturally.
            let mut exempt = line.contains(MARKER);
            let mut j = i;
            while !exempt && j > 0 && lines[j - 1].trim_start().starts_with("//") {
                exempt = lines[j - 1].contains(MARKER);
                j -= 1;
            }
            if !exempt {
                hits.push(Hit { file: rel.clone(), line: i + 1, text: line.trim().to_string() });
            }
        }
    }

    if hits.is_empty() {
        return;
    }
    let mut msg = String::from(
        "\nA cons is not going through `check::core`.\n\n\
         `Heap::cons` can collect, so a value built here dies at the next \
         allocation unless something roots it. Build core form nodes with \
         `core::tagged`/`core::Items`, and plain lists with `core::list` — they \
         root as they go. If this site is rewriting read syntax instead, root \
         the intermediates yourself (see `check::forms::list_from_vec_locs`) and add \
         a `// core-build-ok: <how they stay rooted>` comment.\n\n",
    );
    for h in &hits {
        msg.push_str(&format!("  {}:{}\n      {}\n", h.file, h.line, h.text));
    }
    panic!("{}", msg);
}
