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
use typelisp::{completion_candidates, completion_locals, Checker, CompletionCandidate, CompletionKind, Heap, Interp, Reader, TopLevel};

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

/// Like `check`, but returns the checked top-level forms (what
/// `completion_locals` searches) instead of the `Checker`.
fn program(src: &str) -> Vec<TopLevel> {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all_in(&mut h, FILE, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    vs.into_iter().map(|v| chk.check_form(&mut h, &interp, v).expect("check failed")).collect()
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

#[test]
fn completion_locals_offers_a_defun_parameter_and_a_let_binding() {
    let body = program("(defun f ((x i32)) i32 (let ((n 1)) n))\n");
    // Column 37 is `n`'s reference inside the `let` body.
    let locals = completion_locals(&body, FILE, 1, 37);
    assert!(locals.contains(&"x".to_string()));
    assert!(locals.contains(&"n".to_string()));
}

#[test]
fn completion_locals_does_not_leak_a_sibling_lets_binding() {
    let body = program("(defun f () i32 (progn (let ((a 1)) a) (let ((b 2)) b)))\n");
    // Column 37 is `a`'s reference inside the first `let`; column 53 is `b`'s
    // inside the second — each must see only its own binding, not the
    // sibling's (`scope_typed`'s truncate-on-scope-exit).
    let inside_first = completion_locals(&body, FILE, 1, 37);
    assert!(inside_first.contains(&"a".to_string()));
    assert!(!inside_first.contains(&"b".to_string()));
    let inside_second = completion_locals(&body, FILE, 1, 53);
    assert!(inside_second.contains(&"b".to_string()));
    assert!(!inside_second.contains(&"a".to_string()));
}

#[test]
fn completion_locals_offers_a_match_pattern_binding_in_its_own_arm() {
    let body = program("(defun f ((o Option<i32>)) i32 (match o ((Some x) x) (_ 0)))\n");
    // Column 51 is the `(Some x)` arm's body (`x` reference) — see
    // `lsp_locate_test.rs`'s goto-definition test at the same position.
    let locals = completion_locals(&body, FILE, 1, 51);
    assert!(locals.contains(&"o".to_string()));
    assert!(locals.contains(&"x".to_string()));
}

#[test]
fn completion_locals_does_not_leak_a_match_pattern_binding_into_a_sibling_arm() {
    let body = program("(defun f ((o Option<i32>)) i32 (match o ((Some x) x) (_ 0)))\n");
    // Column 57 is the `_` arm's body (the literal `0`) — `x` belongs to the
    // sibling `(Some x)` arm only (`scope_typed`'s per-arm truncate).
    let locals = completion_locals(&body, FILE, 1, 57);
    assert!(locals.contains(&"o".to_string()));
    assert!(!locals.contains(&"x".to_string()));
}

#[test]
fn completion_locals_sees_a_let_binding_from_the_placeholder_as_the_first_body_form() {
    // The shape `handle_completion` produces when the very first form of a
    // `let` body is being typed: the in-progress identifier is replaced by
    // `(panic "")` at exactly the cursor's position (column 37, the `(`).
    // The placeholder node itself is the cursor's located node, inside the
    // `let`'s scope — so `n` (and the parameter `x`) must be offered.
    let body = program("(defun f ((x i32)) i32 (let ((n 1)) (panic \"\")))\n");
    let locals = completion_locals(&body, FILE, 1, 37);
    assert!(locals.contains(&"x".to_string()));
    assert!(locals.contains(&"n".to_string()));
}

#[test]
fn completion_locals_sees_a_let_binding_from_an_empty_list_as_the_first_body_form() {
    // The no-placeholder shape (`needs_completion_placeholder` declines when
    // the identifier follows a fresh `(`): truncation leaves `()` as the
    // `let`'s whole body. The `Unit` node keeps the `(`'s recorded element
    // position (column 36), so the cursor one column later — where the
    // in-progress callee identifier starts in the real flow — still
    // resolves inside the `let`'s scope.
    let body = program("(defun f ((x i32)) () (let ((n 1)) ()))\n");
    let locals = completion_locals(&body, FILE, 1, 37);
    assert!(locals.contains(&"x".to_string()));
    assert!(locals.contains(&"n".to_string()));
}
