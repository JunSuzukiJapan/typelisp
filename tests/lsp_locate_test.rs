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
fn no_definition_for_a_local_variable_reference() {
    // `x` is now locatable in its own right (see
    // `hover_on_a_local_variable_reference_finds_the_variable_not_its_enclosing_call`
    // below), but `definition_target` doesn't resolve `Expr::Var` to a
    // binding-site location yet (see its doc comment) — so goto-definition
    // on it still returns `None`, same as any other node this pass doesn't
    // recognize as a resolvable reference.
    let src = "(defun add ((x i32) (y i32)) i32 (+ x y))\n";
    let (body, def_locs) = program(src);
    let node = locate_node(&body, FILE, 1, 37).expect("expected a located node");
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
