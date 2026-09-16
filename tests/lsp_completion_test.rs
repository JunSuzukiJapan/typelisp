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
use typelisp::{completion_candidates, completion_locals, Checker, CompletionCandidate, CompletionKind, Error, Heap, Interp, Reader, TopLevelForm};

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
/// `completion_locals` searches) instead of the `Checker` — plus the `Heap`
/// they live in, since a checked form is cons cells.
fn program(src: &str) -> (Heap, Vec<TopLevelForm>) {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all_in(&mut h, FILE, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let body = vs.into_iter().map(|v| chk.check_form(&mut h, &interp, v).expect("check failed")).collect();
    (h, body)
}

/// Like `program`, but runs the checker in error-recovery mode (the mode the
/// LSP's `diagnostics_for`/`candidates_for` use). Returns both the partial
/// checked forms `completion_locals` searches and the errors the checker
/// accumulated instead of aborting on the first one. In recover mode
/// `check_form` never returns `Err` for a readable form (type errors are
/// recorded, not propagated), so `.expect` here only guards against a malformed
/// test input.
fn program_recover(src: &str) -> (Heap, Vec<TopLevelForm>, Vec<Error>) {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all_in(&mut h, FILE, src).expect("read failed");
    let mut chk = Checker::new();
    chk.set_recover(true);
    let interp = Interp::new();
    let body: Vec<TopLevelForm> = vs
        .into_iter()
        .map(|v| chk.check_form(&mut h, &interp, v).expect("recover mode never returns Err on a readable form"))
        .collect();
    (h, body, chk.take_errors())
}

fn has(kind: CompletionKind, candidates: &[CompletionCandidate], name: &str) -> bool {
    candidates.iter().any(|c| c.kind == kind && c.name == name)
}

#[test]
fn lists_a_top_level_defun() {
    let chk = check("(defun add ((x int) (y int)) int (+ x y))\n");
    let candidates = completion_candidates(chk.registry(), &[]);
    assert!(has(CompletionKind::Function, &candidates, "add"));
}

#[test]
fn lists_a_top_level_defvar_and_defstruct() {
    let chk = check("(defvar (count int) 0)\n(defstruct point (x int) (y int))\n");
    let candidates = completion_candidates(chk.registry(), &[]);
    assert!(has(CompletionKind::Variable, &candidates, "count"));
    assert!(has(CompletionKind::Type, &candidates, "point"));
}

#[test]
fn a_submodules_own_definition_is_visible_from_inside_it_but_not_from_root() {
    let chk = check("(module m (defun helper () int 1))\n");
    let inside = completion_candidates(chk.registry(), &["m".to_string()]);
    assert!(has(CompletionKind::Function, &inside, "helper"));
    let at_root = completion_candidates(chk.registry(), &[]);
    assert!(!has(CompletionKind::Function, &at_root, "helper"));
}

#[test]
fn a_public_root_function_is_visible_from_inside_a_submodule() {
    let chk = check("(pub defun helper () int 1)\n(module m (defun main () int (helper)))\n");
    let inside = completion_candidates(chk.registry(), &["m".to_string()]);
    assert!(has(CompletionKind::Function, &inside, "helper"));
    assert!(has(CompletionKind::Function, &inside, "main"));
}

#[test]
fn a_private_root_function_is_visible_from_inside_a_submodule() {
    // Root is always an ancestor of every submodule (Rust-style module
    // privacy — see `Checker::ns_ancestors`/`in_scope`), so a non-`pub`
    // root-level function is offered inside `m` too, not just `pub` ones.
    let chk = check("(defun helper () int 1)\n(module m (defun main () int 1))\n");
    let inside = completion_candidates(chk.registry(), &["m".to_string()]);
    assert!(has(CompletionKind::Function, &inside, "helper"));
    assert!(has(CompletionKind::Function, &inside, "main"));
}

#[test]
fn completion_locals_offers_a_defun_parameter_and_a_let_binding() {
    let (h, body) = program("(defun f ((x int)) int (let ((n 1)) n))\n");
    // Column 37 is `n`'s reference inside the `let` body.
    let locals = completion_locals(&h, &body, FILE, 1, 37);
    assert!(locals.contains(&"x".to_string()));
    assert!(locals.contains(&"n".to_string()));
}

#[test]
fn completion_locals_does_not_leak_a_sibling_lets_binding() {
    let (h, body) = program("(defun f () int (progn (let ((a 1)) a) (let ((b 2)) b)))\n");
    // Column 37 is `a`'s reference inside the first `let`; column 53 is `b`'s
    // inside the second — each must see only its own binding, not the
    // sibling's (`scope_typed`'s truncate-on-scope-exit).
    let inside_first = completion_locals(&h, &body, FILE, 1, 37);
    assert!(inside_first.contains(&"a".to_string()));
    assert!(!inside_first.contains(&"b".to_string()));
    let inside_second = completion_locals(&h, &body, FILE, 1, 53);
    assert!(inside_second.contains(&"b".to_string()));
    assert!(!inside_second.contains(&"a".to_string()));
}

#[test]
fn completion_locals_offers_a_match_pattern_binding_in_its_own_arm() {
    let (h, body) = program("(defun f ((o Option<int>)) int (match o ((Some x) x) (_ 0)))\n");
    // Column 51 is the `(Some x)` arm's body (`x` reference) — see
    // `lsp_locate_test.rs`'s goto-definition test at the same position.
    let locals = completion_locals(&h, &body, FILE, 1, 51);
    assert!(locals.contains(&"o".to_string()));
    assert!(locals.contains(&"x".to_string()));
}

#[test]
fn completion_locals_does_not_leak_a_match_pattern_binding_into_a_sibling_arm() {
    let (h, body) = program("(defun f ((o Option<int>)) int (match o ((Some x) x) (_ 0)))\n");
    // Column 57 is the `_` arm's body (the literal `0`) — `x` belongs to the
    // sibling `(Some x)` arm only (`scope_typed`'s per-arm truncate).
    let locals = completion_locals(&h, &body, FILE, 1, 57);
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
    let (h, body) = program("(defun f ((x int)) int (let ((n 1)) (panic \"\")))\n");
    let locals = completion_locals(&h, &body, FILE, 1, 37);
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
    let (h, body) = program("(defun f ((x int)) () (let ((n 1)) ()))\n");
    let locals = completion_locals(&h, &body, FILE, 1, 37);
    assert!(locals.contains(&"x".to_string()));
    assert!(locals.contains(&"n".to_string()));
}

// --- Error-recovery mode (`Checker::set_recover`) --------------------------
//
// The LSP checks in recover mode so a document with type errors still yields a
// partial `Typed` tree for completion/hover/goto and reports *all* its errors,
// not just the first. These tests drive `program_recover` (recover mode) the
// same way the strict tests above drive `program`.

#[test]
fn recover_mode_records_multiple_form_errors_and_keeps_checking_the_rest() {
    // Two ill-typed `defvar`s (a bool value where `int` is declared) sandwich a
    // well-typed `defun`. Strict checking would abort at the first; recover
    // mode records both errors and still checks — and keeps — the `defun`.
    let (h, body, errors) = program_recover(
        "(defvar (x int) true)\n(defun good ((p int)) int p)\n(defvar (y int) false)\n",
    );
    assert_eq!(errors.len(), 2, "both bad defvars recorded, not just the first");
    // All three forms are still present (the bad ones as recovered holes).
    assert_eq!(body.len(), 3);
    // `good`'s body is intact: its parameter is offered to completion. Column
    // 27 is the `p` reference in `(defun good ((p int)) int p)` on line 2.
    let locals = completion_locals(&h, &body, FILE, 2, 27);
    assert!(locals.contains(&"p".to_string()));
}

#[test]
fn recover_mode_offers_locals_inside_a_non_catchall_match_arm() {
    // The motivating bug: `handle_completion` truncates the source after the
    // cursor, so a completion request inside `(Some x)`'s arm deletes the
    // catchall arm that followed and leaves a *non-exhaustive* match. Strict
    // checking errored out and dropped every local; recover mode records the
    // non-exhaustiveness (B1) but still returns the fully-typed `Match`, so the
    // arm's binding `x` and the parameter `o` are offered. This is the exact
    // patched shape (placeholder `(panic "")` where the identifier was typed).
    let (h, body, errors) =
        program_recover("(defun f ((o Option<int>)) int (match o ((Some x) (panic \"\"))))\n");
    assert_eq!(errors.len(), 1, "exactly the non-exhaustive-match error");
    // Column 51 is the placeholder inside the `(Some x)` arm body.
    let locals = completion_locals(&h, &body, FILE, 1, 51);
    assert!(locals.contains(&"o".to_string()));
    assert!(locals.contains(&"x".to_string()), "the arm's pattern binding is offered");
}

#[test]
fn recover_mode_skips_a_bad_arm_but_keeps_the_good_arms_bindings() {
    // An arm with an unknown constructor pattern is recorded and skipped (B2);
    // the surviving `(Some x)` arm's binding is still reachable, and the skip
    // suppresses the exhaustiveness check so no spurious cascade is added.
    let (h, body, errors) = program_recover(
        "(defun f ((o Option<int>)) int (match o ((Some x) x) ((Bogus y) 0) (_ 0)))\n",
    );
    assert!(!errors.is_empty(), "the unknown-constructor arm is recorded");
    // Column 51 is the `(Some x)` arm body (`x` reference).
    let locals = completion_locals(&h, &body, FILE, 1, 51);
    assert!(locals.contains(&"x".to_string()));
}

#[test]
fn recover_mode_holes_one_bad_body_form_and_keeps_the_siblings() {
    // A bad first body form (`undefined`) becomes a hole (B3); the following
    // `let` still checks, so its binding `n` and the parameter `x` are offered.
    let (h, body, errors) =
        program_recover("(defun f ((x int)) int (progn undefined (let ((n 1)) n)))\n");
    assert_eq!(errors.len(), 1, "just the unknown-variable error");
    // Column 54 is the `n` reference inside the `let` body.
    let locals = completion_locals(&h, &body, FILE, 1, 54);
    assert!(locals.contains(&"x".to_string()));
    assert!(locals.contains(&"n".to_string()));
}

#[test]
fn recover_mode_keeps_a_let_binding_whose_init_failed() {
    // A `let` binding with an ill-typed initializer (B4): the init holes but
    // the binding still enters scope (as `Never`), so `n` is offered in the
    // body alongside the parameter `x`.
    let (h, body, errors) =
        program_recover("(defun f ((x int)) int (let ((n bad_init)) n))\n");
    assert_eq!(errors.len(), 1, "just the unknown-variable error in the init");
    // Column 44 is the `n` reference inside the `let` body.
    let locals = completion_locals(&h, &body, FILE, 1, 44);
    assert!(locals.contains(&"x".to_string()));
    assert!(locals.contains(&"n".to_string()));
}

#[test]
fn recover_mode_records_each_independent_error_with_its_own_location() {
    // Three independent unknown variables in one body: each is recorded
    // separately (holes don't cascade because `Never` unifies everywhere), and
    // each error carries a distinct source location.
    let (_h, _body, errors) = program_recover("(defun f () int (progn err_a err_b err_c))\n");
    assert_eq!(errors.len(), 3);
    let cols: Vec<Option<u32>> = errors
        .iter()
        .map(|e| e.loc().map(|l| l.col))
        .collect();
    assert!(cols.iter().all(|c| c.is_some()), "every error is located");
    let mut distinct = cols.clone();
    distinct.dedup();
    assert_eq!(distinct.len(), 3, "the three errors have distinct columns");
}
