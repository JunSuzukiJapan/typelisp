//! Tests for `check::locate` — the cursor-position lookup the LSP's hover
//! and goto-definition (`src/bin/lsp.rs`) are built on. Drives
//! `Checker::check_form` directly (the same core pipeline `check_test.rs`
//! uses) rather than the LSP's stdio transport: the transport is a thin,
//! already-standard `lsp-server` wrapper, while this lookup logic is the
//! part actually worth covering.

extern crate typelisp;
use typelisp::check::core;
use typelisp::{
    definition_target, hover_text, locate_node, Checker, DefLocs, Docs, Heap, Interp, Reader, TopLevelForm, Type,
};

const FILE: &str = "test.typl";

/// Check every top-level form in `src` (in the root namespace, no prelude —
/// matches `check_test.rs`'s `program` helper), returning them alongside a
/// snapshot of the registry's definition-location and docstring tables —
/// everything [`locate_node`]/[`definition_target`]/[`hover_text`] need.
///
/// The `Heap` comes back too: a checked form is cons cells, so the tree means
/// nothing without the heap it lives in. `src/bin/lsp.rs`'s `Analysis` keeps
/// the heap for exactly this reason.
fn program(src: &str) -> (Heap, Vec<TopLevelForm>, DefLocs, Docs) {
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let vs = r.read_all_in(&mut h, FILE, src).expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let body = vs.into_iter().map(|v| chk.check_form(&mut h, &interp, v).expect("check failed")).collect();
    (h, body, chk.registry().def_locs.clone(), chk.registry().docs.clone())
}

#[test]
fn locates_the_smallest_enclosing_call() {
    let src = "(defun add ((x int) (y int)) int (+ x y))\n(defun main () int (add 1 2))\n";
    let (h, body, def_locs, docs) = program(src);
    // Line 2: `(defun main () int (add 1 2))` — the `(add 1 2)` call form
    // starts at column 20; a cursor anywhere from there up to just before
    // its closing paren should resolve to that `Call` node, since its
    // integer-literal arguments carry no location of their own (see
    // `check::locate`'s module doc comment).
    let node = locate_node(&h, &body, FILE, 2, 22).expect("expected a located node");
    assert_eq!(hover_text(&h, node, &def_locs, &docs), format!("{:?}", Type::Int));
}

#[test]
fn goto_definition_resolves_a_call_to_its_defun() {
    let src = "(defun add ((x int) (y int)) int (+ x y))\n(defun main () int (add 1 2))\n";
    let (h, body, def_locs, _docs) = program(src);
    let node = locate_node(&h, &body, FILE, 2, 22).expect("expected a located node");
    let target = definition_target(&h, node, &def_locs).expect("expected a resolvable reference");
    // `(defun add ...)` is the very first form: line 1, column 1.
    assert_eq!(&*target.file, FILE);
    assert_eq!(target.line, 1);
    assert_eq!(target.col, 1);
}

#[test]
fn goto_definition_on_a_parameter_reference_resolves_to_the_parameter() {
    // `x` at column 37 is the reference inside `(+ x y)`; its parameter
    // declaration `(x int)` is at column 14 on the same line (the `defun`'s
    // parameter list). `DefLocs::local_refs` resolves the former to the
    // latter without any scope search at query time — see `check_at`'s doc
    // comment on where that resolution actually happens (once, at check
    // time).
    let src = "(defun add ((x int) (y int)) int (+ x y))\n";
    let (h, body, def_locs, _docs) = program(src);
    let node = locate_node(&h, &body, FILE, 1, 37).expect("expected a located node");
    let target = definition_target(&h, node, &def_locs).expect("expected a resolvable reference");
    assert_eq!(&*target.file, FILE);
    assert_eq!(target.line, 1);
    assert_eq!(target.col, 14);
}

#[test]
fn goto_definition_on_a_let_bound_reference_resolves_to_the_binding() {
    let src = "(defun f () int (let ((n 1)) n))\n";
    let (h, body, def_locs, _docs) = program(src);
    // `(let ((n 1)) n))` — `n`'s binding name is at column 24, its trailing
    // reference (the `let`'s body) at column 30.
    let node = locate_node(&h, &body, FILE, 1, 30).expect("expected a located node");
    let target = definition_target(&h, node, &def_locs).expect("expected a resolvable reference");
    assert_eq!(target.line, 1);
    assert_eq!(target.col, 24);
}

#[test]
fn goto_definition_on_a_lambda_parameter_reference_resolves_to_the_parameter() {
    let src = "(defun f () int ((lambda ((y int)) int y) 1))\n";
    let (h, body, def_locs, _docs) = program(src);
    // Column 40 is `y`'s reference in the lambda body; its parameter
    // declaration `(y int)` is at column 28.
    let node = locate_node(&h, &body, FILE, 1, 40).expect("expected a located node");
    let target = definition_target(&h, node, &def_locs).expect("expected a resolvable reference");
    assert_eq!(target.line, 1);
    assert_eq!(target.col, 28);
}

#[test]
fn goto_definition_on_a_labels_function_and_parameter_resolves_to_their_bindings() {
    let src = "(defun f () int (labels ((g ((z int)) int z)) (g 1)))\n";
    let (h, body, def_locs, _docs) = program(src);
    // `z`'s reference inside `g`'s own body (column 43) resolves to its
    // parameter declaration (column 31).
    let z_ref = locate_node(&h, &body, FILE, 1, 43).expect("expected a located node");
    let z_target = definition_target(&h, z_ref, &def_locs).expect("expected a resolvable reference");
    assert_eq!(z_target.col, 31);
    // `g`'s call in the trailing `(g 1)` (column 48) — a local function value
    // applied directly (`check_list`'s "local variable holding a function
    // value" branch builds an `Expr::Var`, not `Expr::Call`) — resolves to
    // its own `labels` binding (column 27).
    let g_ref = locate_node(&h, &body, FILE, 1, 48).expect("expected a located node");
    let g_target = definition_target(&h, g_ref, &def_locs).expect("expected a resolvable reference");
    assert_eq!(g_target.col, 27);
}

#[test]
fn goto_definition_on_a_match_pattern_bound_reference_resolves_to_the_pattern() {
    let src = "(defun f ((o Option<int>)) int (match o ((Some x) x) (_ 0)))\n";
    let (h, body, def_locs, _docs) = program(src);
    // Column 51 is the arm body's `x` reference; its pattern binding — the
    // `x` inside `(Some x)` — is at column 48 (`check_ctor_pattern`'s
    // per-field element positions).
    let node = locate_node(&h, &body, FILE, 1, 51).expect("expected a located node");
    let target = definition_target(&h, node, &def_locs).expect("expected a resolvable reference");
    assert_eq!(&*target.file, FILE);
    assert_eq!(target.line, 1);
    assert_eq!(target.col, 48);
}

#[test]
fn goto_definition_on_a_whole_arm_bind_pattern_reference_resolves_to_the_pattern() {
    let src = "(defun g ((o Option<int>)) Option<int> (match o (v v)))\n";
    let (h, body, def_locs, _docs) = program(src);
    // Column 52 is the arm body's `v` reference; the whole-arm variable
    // pattern `v` (the arm's own first element) is at column 50.
    let node = locate_node(&h, &body, FILE, 1, 52).expect("expected a located node");
    let target = definition_target(&h, node, &def_locs).expect("expected a resolvable reference");
    assert_eq!(target.line, 1);
    assert_eq!(target.col, 50);
}

#[test]
fn hover_on_a_local_variable_reference_finds_the_variable_not_its_enclosing_call() {
    // `x` (a `bool` parameter) sits inside the `if`'s condition position;
    // the enclosing `if` expression's own type is `int` (its branches), a
    // different type from `x`'s own (`bool`) — so if hover resolved to the
    // wrong (enclosing) node, this assertion would catch it. Before atom
    // source locations were tracked (`Heap::elem_locs`), a bare `Var`
    // reference had no `Loc` of its own and a cursor here would have
    // resolved to the smallest node that *did* have one instead (the whole
    // `if` form).
    let src = "(defun f ((x bool)) int (if x 1 2))\n";
    let (h, body, def_locs, docs) = program(src);
    // Column 29 is `x` itself in `(if x 1 2)`.
    let node = locate_node(&h, &body, FILE, 1, 29).expect("expected a located node");
    assert_eq!(hover_text(&h, node, &def_locs, &docs), format!("{:?}", Type::Bool));
}

#[test]
fn hover_on_a_let_bound_local_finds_its_own_type() {
    // `n` is bound to a `bool` value; the `let`'s own body/result type is
    // `int` (the trailing `1`) — a mismatch that would surface if hover
    // resolved to the enclosing `let` instead of the `n` reference itself.
    let src = "(defun f () int (let ((n true)) (if n 1 1)))\n";
    let (h, body, def_locs, docs) = program(src);
    // `(let ((n true)) (if n 1 1))` — `n` inside the `if` condition is at
    // column 37 exactly. (Column 38 — the space after it — used to resolve
    // to `n` too under the pre-span closest-preceding-start search, but with
    // true containment it correctly resolves to the enclosing `if` instead.)
    let node = locate_node(&h, &body, FILE, 1, 37).expect("expected a located node");
    assert_eq!(hover_text(&h, node, &def_locs, &docs), format!("{:?}", Type::Bool));
}

#[test]
fn bare_toplevel_atom_gets_its_span_from_the_reader() {
    // A lone atom at top level has no cons cell to key a location on in the
    // heap's tables — its span travels alongside the value from
    // `read_all_in_spanned` into `check_form_at`'s `loc_hint`.
    let mut h = Heap::with_capacity(1 << 16);
    let r = Reader::new();
    let forms = r.read_all_in_spanned(&mut h, FILE, "42\n").expect("read failed");
    let mut chk = Checker::new();
    let interp = Interp::new();
    let (v, loc) = forms[0].clone();
    let tl = chk.check_form_at(&mut h, &interp, v, Some(loc)).expect("check failed");
    assert_eq!(core::op(&h, tl), Some("expr"));
    // The span lands on the wrapped node itself — `(int-any-width 42)` — since that is
    // what `check_at` builds and records `def_loc` on.
    let node = core::field(&h, tl, 0).expect("`(expr FORM)` always has its form");
    let loc = h.cons_loc(node).expect("bare atom should carry the reader's span");
    assert_eq!((loc.line, loc.col, loc.end_line, loc.end_col), (1, 1, 1, 3));
}

#[test]
fn locate_prefers_true_containment_over_a_closer_preceding_start() {
    // Two sibling calls, both arguments to a user-defined `wrap` (unlike
    // `+`, `wrap` has its own `def_locs` entry to resolve against). A
    // cursor *between* the siblings — past `(add 1 2)`'s closing paren,
    // before `(add 3 4)` — is contained by neither sibling but by the
    // enclosing `(wrap ...)` call, whose start is *further* from the
    // cursor than the first sibling's. The pre-span point search resolved
    // this to the first sibling (`add`, line 1); containment resolves it
    // to the enclosing call (`wrap`, line 2).
    let src = "(defun add ((x int) (y int)) int (+ x y))\n(defun wrap ((a int) (b int)) int (+ a b))\n(defun main () int (wrap (add 1 2)  (add 3 4)))\n";
    let (h, body, def_locs, _docs) = program(src);
    // Line 3: `(wrap (add 1 2)  (add 3 4))` — the gap between siblings
    // (the second of the two spaces) is column 36.
    let node = locate_node(&h, &body, FILE, 3, 36).expect("expected a located node");
    let target = definition_target(&h, node, &def_locs).expect("the enclosing call resolves");
    assert_eq!((target.line, target.col), (2, 1), "expected wrap's defun, not add's");
}

#[test]
fn locate_inside_a_sibling_still_finds_it() {
    let src = "(defun add ((x int) (y int)) int (+ x y))\n(defun main () int (+ (add 1 2)  (add 3 4)))\n";
    let (h, body, def_locs, _docs) = program(src);
    // Column 35 is inside the second `(add 3 4)` call.
    let node = locate_node(&h, &body, FILE, 2, 35).expect("expected a located node");
    let target = definition_target(&h, node, &def_locs).expect("expected a resolvable reference");
    assert_eq!((target.line, target.col), (1, 1)); // `add`'s defun
}

#[test]
fn locate_past_every_span_falls_back_to_the_closest_preceding_node() {
    // A cursor way past the last form's end is contained by nothing; the
    // fallback (pre-span behavior) still resolves to the closest preceding
    // node rather than returning nothing — completion_locals depends on
    // locate_node finding *something* here.
    let src = "(defun main () int (+ 1 2))\n";
    let (h, body, _, _) = program(src);
    assert!(locate_node(&h, &body, FILE, 3, 1).is_some());
}
