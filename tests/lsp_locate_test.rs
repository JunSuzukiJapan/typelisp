//! Tests for `check::locate` — the cursor-position lookup the LSP's hover
//! and goto-definition (`src/bin/lsp.rs`) are built on. Drives
//! `Checker::check_form` directly (the same core pipeline `check_test.rs`
//! uses) rather than the LSP's stdio transport: the transport is a thin,
//! already-standard `lsp-server` wrapper, while this lookup logic is the
//! part actually worth covering.

extern crate typelisp;
use typelisp::{definition_target, hover_text, locate_node, Checker, DefLocs, Heap, Interp, Reader, TopLevel, Type};

const FILE: &str = "test.typl";

/// Check every top-level form in `src` (in the root namespace, no prelude —
/// matches `check_test.rs`'s `program` helper), returning them alongside a
/// snapshot of the registry's definition-location table — everything
/// [`locate_node`]/[`definition_target`] need, self-contained once the
/// `Heap`/`Checker` this built them from goes out of scope.
fn program(src: &str) -> (Vec<TopLevel>, DefLocs) {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all_in(&mut h, FILE, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let body = vs.into_iter().map(|v| chk.check_form(&mut h, &interp, v).expect("check failed")).collect();
    (body, chk.registry().def_locs.clone())
}

#[test]
fn locates_the_smallest_enclosing_call() {
    let src = "(defun add ((x i32) (y i32)) i32 (+ x y))\n(defun main () i32 (add 1 2))\n";
    let (body, _) = program(src);
    // Line 2: `(defun main () i32 (add 1 2))` — the `(add 1 2)` call form
    // starts at column 20; a cursor anywhere from there up to just before
    // its closing paren should resolve to that `Call` node, since its
    // integer-literal arguments carry no location of their own (see
    // `check::locate`'s module doc comment).
    let node = locate_node(&body, FILE, 2, 22).expect("expected a located node");
    assert_eq!(hover_text(node), format!("{:?}", Type::I32));
}

#[test]
fn goto_definition_resolves_a_call_to_its_defun() {
    let src = "(defun add ((x i32) (y i32)) i32 (+ x y))\n(defun main () i32 (add 1 2))\n";
    let (body, def_locs) = program(src);
    let node = locate_node(&body, FILE, 2, 22).expect("expected a located node");
    let target = definition_target(node, &def_locs).expect("expected a resolvable reference");
    // `(defun add ...)` is the very first form: line 1, column 1.
    assert_eq!(&*target.file, FILE);
    assert_eq!(target.line, 1);
    assert_eq!(target.col, 1);
}

#[test]
fn goto_definition_on_a_parameter_reference_resolves_to_the_parameter() {
    // `x` at column 37 is the reference inside `(+ x y)`; its parameter
    // declaration `(x i32)` is at column 14 on the same line (the `defun`'s
    // parameter list). `DefLocs::local_refs` resolves the former to the
    // latter without any scope search at query time — see `check_at`'s doc
    // comment on where that resolution actually happens (once, at check
    // time).
    let src = "(defun add ((x i32) (y i32)) i32 (+ x y))\n";
    let (body, def_locs) = program(src);
    let node = locate_node(&body, FILE, 1, 37).expect("expected a located node");
    let target = definition_target(node, &def_locs).expect("expected a resolvable reference");
    assert_eq!(&*target.file, FILE);
    assert_eq!(target.line, 1);
    assert_eq!(target.col, 14);
}

#[test]
fn goto_definition_on_a_let_bound_reference_resolves_to_the_binding() {
    let src = "(defun f () i32 (let ((n 1)) n))\n";
    let (body, def_locs) = program(src);
    // `(let ((n 1)) n))` — `n`'s binding name is at column 24, its trailing
    // reference (the `let`'s body) at column 30.
    let node = locate_node(&body, FILE, 1, 30).expect("expected a located node");
    let target = definition_target(node, &def_locs).expect("expected a resolvable reference");
    assert_eq!(target.line, 1);
    assert_eq!(target.col, 24);
}

#[test]
fn goto_definition_on_a_lambda_parameter_reference_resolves_to_the_parameter() {
    let src = "(defun f () i32 ((lambda ((y i32)) i32 y) 1))\n";
    let (body, def_locs) = program(src);
    // Column 40 is `y`'s reference in the lambda body; its parameter
    // declaration `(y i32)` is at column 28.
    let node = locate_node(&body, FILE, 1, 40).expect("expected a located node");
    let target = definition_target(node, &def_locs).expect("expected a resolvable reference");
    assert_eq!(target.line, 1);
    assert_eq!(target.col, 28);
}

#[test]
fn goto_definition_on_a_labels_function_and_parameter_resolves_to_their_bindings() {
    let src = "(defun f () i32 (labels ((g ((z i32)) i32 z)) (g 1)))\n";
    let (body, def_locs) = program(src);
    // `z`'s reference inside `g`'s own body (column 43) resolves to its
    // parameter declaration (column 31).
    let z_ref = locate_node(&body, FILE, 1, 43).expect("expected a located node");
    let z_target = definition_target(z_ref, &def_locs).expect("expected a resolvable reference");
    assert_eq!(z_target.col, 31);
    // `g`'s call in the trailing `(g 1)` (column 48) — a local function value
    // applied directly (`check_list`'s "local variable holding a function
    // value" branch builds an `Expr::Var`, not `Expr::Call`) — resolves to
    // its own `labels` binding (column 27).
    let g_ref = locate_node(&body, FILE, 1, 48).expect("expected a located node");
    let g_target = definition_target(g_ref, &def_locs).expect("expected a resolvable reference");
    assert_eq!(g_target.col, 27);
}

#[test]
fn no_definition_for_a_match_bound_variable() {
    // Match-pattern bindings are a deliberately out-of-scope residual (see
    // `Env::extended`'s doc comment) — a pattern-bound name still resolves
    // via `env.get`/hover (its `Var` node gets a `Loc` from `check_at` like
    // any other bare-atom reference), but has no recorded binding-site
    // position, so goto-definition on it stays `None`.
    let src = "(defun f ((o Option<i32>)) i32 (match o ((Some x) x) (_ 0)))\n";
    let (body, def_locs) = program(src);
    // Column 51 is the arm body's `x` reference (after `(Some x)`'s own,
    // pattern-bound `x` at column 48).
    let node = locate_node(&body, FILE, 1, 51).expect("expected a located node");
    assert_eq!(definition_target(node, &def_locs), None);
}

#[test]
fn hover_on_a_local_variable_reference_finds_the_variable_not_its_enclosing_call() {
    // `x` (a `bool` parameter) sits inside the `if`'s condition position;
    // the enclosing `if` expression's own type is `i32` (its branches), a
    // different type from `x`'s own (`bool`) — so if hover resolved to the
    // wrong (enclosing) node, this assertion would catch it. Before atom
    // source locations were tracked (`Heap::elem_locs`), a bare `Var`
    // reference had no `Loc` of its own and a cursor here would have
    // resolved to the smallest node that *did* have one instead (the whole
    // `if` form).
    let src = "(defun f ((x bool)) i32 (if x 1 2))\n";
    let (body, _) = program(src);
    // Column 29 is `x` itself in `(if x 1 2)`.
    let node = locate_node(&body, FILE, 1, 29).expect("expected a located node");
    assert_eq!(hover_text(node), format!("{:?}", Type::Bool));
}

#[test]
fn hover_on_a_let_bound_local_finds_its_own_type() {
    // `n` is bound to a `bool` value; the `let`'s own body/result type is
    // `i32` (the trailing `1`) — a mismatch that would surface if hover
    // resolved to the enclosing `let` instead of the `n` reference itself.
    let src = "(defun f () i32 (let ((n true)) (if n 1 1)))\n";
    let (body, _) = program(src);
    // `(let ((n true)) (if n 1 1))` — `n` inside the `if` condition.
    let node = locate_node(&body, FILE, 1, 38).expect("expected a located node");
    assert_eq!(hover_text(node), format!("{:?}", Type::Bool));
}
