//! Cursor-position lookups over a checked document, for the LSP's hover and
//! goto-definition (`src/bin/lsp.rs`).
//!
//! `Typed::loc` (`ast.rs`) is a *point* — the position of the opening
//! parenthesis of the list form a node was read from — not a span, and only
//! list-form nodes have one at all (a bare atom, e.g. a lone `Var`
//! reference, has none: `Value::Symbol`s are interned and reused everywhere,
//! so there is no per-occurrence heap address to hang a location off, unlike
//! `Value::Cons`'s `cons_locs` side table — see `Heap::cons_loc`'s doc
//! comment). A full span (and per-atom positions) would need the reader
//! rebuilt to track every token, not just list heads — out of scope here.
//!
//! Even with only start points, the smallest node containing the cursor can
//! still be found exactly: sibling forms never overlap in source order, so
//! among every node whose start position is `<= cursor`, the one with the
//! *greatest* start position is the innermost one that contains it (ties —
//! a wrapper node sharing its first child's exact position — broken toward
//! the deeper node). [`locate_node`] is that search.

use crate::{DefLocs, Expr, Loc, TopLevel, Typed};

/// Find the smallest node in `body` whose recorded location is at or before
/// `(line, col)` in `file` — see the module doc comment for why "greatest
/// start position `<=` cursor" is exactly "smallest containing node" here.
/// `line`/`col` are 1-based, matching [`Loc`].
pub fn locate_node<'a>(body: &'a [TopLevel], file: &str, line: u32, col: u32) -> Option<&'a Typed> {
    let cursor = (line, col);
    let mut best: Option<((u32, u32), usize, &'a Typed)> = None;
    for tl in body {
        visit_top_level(tl, file, cursor, 0, &mut best);
    }
    best.map(|(_, _, t)| t)
}

fn visit_top_level<'a>(
    tl: &'a TopLevel,
    file: &str,
    cursor: (u32, u32),
    depth: usize,
    best: &mut Option<((u32, u32), usize, &'a Typed)>,
) {
    match tl {
        TopLevel::Defun { body, .. } | TopLevel::Defmethod { body, .. } | TopLevel::Defmacro { body, .. } => {
            for t in body {
                visit_typed(t, file, cursor, depth, best);
            }
        }
        TopLevel::Defvar { value, .. } => visit_typed(value, file, cursor, depth, best),
        TopLevel::Module { body, .. } => {
            for tl in body {
                visit_top_level(tl, file, cursor, depth, best);
            }
        }
        TopLevel::Expr(t) => visit_typed(t, file, cursor, depth, best),
        TopLevel::Use { .. } | TopLevel::Defstruct { .. } | TopLevel::Defenum { .. } => {}
    }
}

fn visit_typed<'a>(
    t: &'a Typed,
    file: &str,
    cursor: (u32, u32),
    depth: usize,
    best: &mut Option<((u32, u32), usize, &'a Typed)>,
) {
    if let Some(loc) = &t.loc {
        if &*loc.file == file {
            let key = (loc.line, loc.col);
            if key <= cursor {
                let better = match best {
                    None => true,
                    Some((bk, bd, _)) => key > *bk || (key == *bk && depth > *bd),
                };
                if better {
                    *best = Some((key, depth, t));
                }
            }
        }
    }
    for child in expr_children(&t.expr) {
        visit_typed(child, file, cursor, depth + 1, best);
    }
}

/// Every immediate `Typed` child of `e`, for [`visit_typed`]'s descent —
/// covers every [`Expr`] variant; the leaf ones (literals, `Var`/`Global`/
/// `FnRef`/`MethodRef` references, `Break`, `Quote`) have none.
fn expr_children(e: &Expr) -> Vec<&Typed> {
    match e {
        Expr::If(a, b, c) => vec![a.as_ref(), b.as_ref(), c.as_ref()],
        Expr::Let(binds, body) => binds.iter().map(|(_, t)| t).chain(body.iter()).collect(),
        Expr::Call(_, args) => args.iter().collect(),
        Expr::Lambda { body, .. } => body.iter().collect(),
        Expr::Labels { defs, body } => defs.iter().flat_map(|(_, _, b)| b.iter()).chain(body.iter()).collect(),
        Expr::Apply(f, args) => std::iter::once(f.as_ref()).chain(args.iter()).collect(),
        Expr::Assoc { args, .. } => args.iter().collect(),
        Expr::TraitCall { args, .. } => args.iter().collect(),
        Expr::Construct { args, .. } => args.iter().collect(),
        Expr::FieldGet(e, _) => vec![e.as_ref()],
        Expr::FieldSet(e, _, v) => vec![e.as_ref(), v.as_ref()],
        Expr::Match(scrutinee, arms) => {
            std::iter::once(scrutinee.as_ref()).chain(arms.iter().flat_map(|a| a.body.iter())).collect()
        }
        Expr::Set(_, e) => vec![e.as_ref()],
        Expr::SetGlobal(_, e) => vec![e.as_ref()],
        Expr::Loop(body) => body.iter().collect(),
        Expr::Return(opt) => opt.iter().map(|b| b.as_ref()).collect(),
        Expr::Panic(e) => vec![e.as_ref()],
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Bignum(_)
        | Expr::Ratio(_)
        | Expr::Bool(_)
        | Expr::Char(_)
        | Expr::Str(_)
        | Expr::Unit
        | Expr::Var(_)
        | Expr::Global(_)
        | Expr::FnRef(_)
        | Expr::MethodRef { .. }
        | Expr::Break
        | Expr::Quote(_) => vec![],
    }
}

/// Where `node` refers to, if it's a reference this pass can resolve.
/// `Global`/`Call`/`FnRef`/`Assoc`/`MethodRef`/`Construct` all already carry
/// a fully-qualified [`crate::Path`] (resolved at check time), so this is a
/// plain [`DefLocs`] lookup — no name resolution needed. A local variable
/// reference (`Expr::Var`) has no binding-site location to resolve to (its
/// binding form, e.g. a `let`, isn't tracked in `DefLocs` — see the module
/// doc comment) and returns `None`, same as any node this pass doesn't
/// recognize as a reference.
pub fn definition_target(node: &Typed, def_locs: &DefLocs) -> Option<Loc> {
    match &node.expr {
        Expr::Global(p) | Expr::SetGlobal(p, _) => def_locs.vars.get(p).cloned(),
        Expr::FnRef(p) | Expr::Call(p, _) => def_locs.fns.get(p).cloned(),
        Expr::MethodRef { type_name, method } | Expr::Assoc { type_name, method, .. } => {
            def_locs.methods.get(&(type_name.clone(), method.clone())).cloned()
        }
        Expr::Construct { type_name, .. } => def_locs.types.get(type_name).cloned(),
        _ => None,
    }
}

/// Hover text for `node`: its checked type. `Type` has no `Display` impl
/// anywhere in this crate — every existing type-mismatch message already
/// formats it with `{:?}` (e.g. `Checker::check`'s "type mismatch: expected
/// {:?}, found {:?}") — so this matches that existing convention rather than
/// introducing a pretty-printer.
pub fn hover_text(node: &Typed) -> String {
    format!("{:?}", node.ty)
}
