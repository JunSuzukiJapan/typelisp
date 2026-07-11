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
    // `x` inside `add`'s body has no binding-site location tracked — see
    // `check::locate::definition_target`'s doc comment — so a cursor there
    // resolves to the smallest node that *does* have one (the `(+ x y)`
    // call) rather than to `x` itself, and that call has no `DefLocs` entry
    // (`+` is a builtin instance method, not a registered free function).
    let src = "(defun add ((x i32) (y i32)) i32 (+ x y))\n";
    let (body, def_locs) = program(src);
    let node = locate_node(&body, FILE, 1, 36).expect("expected a located node");
    assert_eq!(definition_target(node, &def_locs), None);
}
