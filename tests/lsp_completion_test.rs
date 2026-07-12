//! Tests for `check::locate::completion_candidates` — the name-enumeration
//! logic behind the LSP's `textDocument/completion` (`src/bin/lsp.rs`).
//! Drives `Checker::check_form` directly, the same core pipeline
//! `lsp_locate_test.rs` uses for hover/goto-definition, rather than the
//! LSP's stdio transport or its paren-balancing heuristic (which is
//! LSP-transport-specific and covered by `src/bin/lsp.rs`'s own unit tests).
//!
//! `Checker::new()` (unlike a real document's checker, which loads the
//! prelude) still registers every Rust-implemented builtin (`with_builtins`)
//! — so assertions check for the presence/absence of the user-defined names
//! each test cares about (`contains`/`!contains`), not an exact candidate
//! list, the same way a real completion response would mix builtins in
//! alongside user code.

extern crate typelisp;
use typelisp::{completion_candidates, Checker, CompletionCandidate, CompletionKind, Heap, Interp, Reader};

const FILE: &str = "test.typl";

/// Check every top-level form in `src` (root namespace, no prelude — matches
/// `lsp_locate_test.rs`'s `program` helper) and return the resulting
/// `Checker` so its `Registry` can be queried.
fn check(src: &str) -> Checker {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all_in(&mut h, FILE, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    for v in vs {
        chk.check_form(&mut h, &interp, v).expect("check failed");
    }
    chk
}

fn has(kind: CompletionKind, candidates: &[CompletionCandidate], name: &str) -> bool {
    candidates.iter().any(|c| c.kind == kind && c.name == name)
}

#[test]
fn lists_a_top_level_defun() {
    let chk = check("(defun add ((x i32) (y i32)) i32 (+ x y))\n");
    let candidates = completion_candidates(chk.registry(), &[]);
    assert!(has(CompletionKind::Function, &candidates, "add"));
}

#[test]
fn lists_a_top_level_defvar_and_defstruct() {
    let chk = check("(defvar (count i32) 0)\n(defstruct point (x i32) (y i32))\n");
    let candidates = completion_candidates(chk.registry(), &[]);
    assert!(has(CompletionKind::Variable, &candidates, "count"));
    assert!(has(CompletionKind::Type, &candidates, "point"));
}

#[test]
fn a_submodules_own_definition_is_visible_from_inside_it_but_not_from_root() {
    let chk = check("(module m (defun helper () i32 1))\n");
    let inside = completion_candidates(chk.registry(), &["m".to_string()]);
    assert!(has(CompletionKind::Function, &inside, "helper"));
    let at_root = completion_candidates(chk.registry(), &[]);
    assert!(!has(CompletionKind::Function, &at_root, "helper"));
}

#[test]
fn a_public_root_function_is_visible_from_inside_a_submodule() {
    let chk = check("(pub defun helper () i32 1)\n(module m (defun main () i32 (helper)))\n");
    let inside = completion_candidates(chk.registry(), &["m".to_string()]);
    assert!(has(CompletionKind::Function, &inside, "helper"));
    assert!(has(CompletionKind::Function, &inside, "main"));
}

#[test]
fn a_private_root_function_is_not_visible_from_inside_a_submodule() {
    let chk = check("(defun helper () i32 1)\n(module m (defun main () i32 1))\n");
    let inside = completion_candidates(chk.registry(), &["m".to_string()]);
    assert!(!has(CompletionKind::Function, &inside, "helper"));
    assert!(has(CompletionKind::Function, &inside, "main"));
}
