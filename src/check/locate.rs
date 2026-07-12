//! Cursor-position lookups over a checked document, for the LSP's hover and
//! goto-definition (`src/bin/lsp.rs`).
//!
//! `Typed::loc` (`ast.rs`) is a *point*, not a span: for a list-form node it's
//! the position of the opening parenthesis (`Heap::cons_loc`); for a bare atom
//! checked as an *element of some enclosing list* (e.g. an argument, a `let`
//! binding value, a function body form) it's the position the reader recorded
//! for that specific occurrence (`Heap::elem_locs`, threaded through the
//! checker as `arg_locs`/`Checker::check_at`) — this is what lets a `Var`
//! (local variable) reference be found in its own right. An atom with *no*
//! enclosing list at all (vanishingly rare — the entry point only ever checks
//! whole top-level forms) still has no location. A full span (as opposed to
//! just a start point) would need the reader rebuilt to track every token's
//! end, not just its start — out of scope here.
//!
//! Even with only start points, the smallest node containing the cursor can
//! still be found exactly: sibling forms never overlap in source order, so
//! among every node whose start position is `<= cursor`, the one with the
//! *greatest* start position is the innermost one that contains it (ties —
//! a wrapper node sharing its first child's exact position — broken toward
//! the deeper node). [`locate_node`] is that search.

use crate::{DefLocs, Expr, Loc, Registry, TopLevel, Typed};

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

/// What a [`CompletionCandidate`] names — mirrors the tables a
/// [`crate::Namespace`] keeps, plus `Module` for a child module name (useful
/// for completing a `module::` prefix).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Function,
    Method,
    Type,
    Variable,
    Macro,
    Trait,
    Module,
}

/// One name reachable for completion, with enough to render an LSP
/// `CompletionItem` (`src/bin/lsp.rs::handle_completion`): its bare name
/// (already lowercase — the reader case-folds every symbol), what kind of
/// thing it is, and a short human-readable detail string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub name: String,
    pub kind: CompletionKind,
    pub detail: String,
}

/// Every bare name visible for completion while checking module
/// `module_path`: that module's own definitions, plus the root namespace's
/// *public* ones (root's own, if `module_path` is already empty) — the same
/// two-namespace search order `Checker::resolve_fn`/`resolve_global`/etc. use
/// at name-resolution time, just enumerated instead of looked up by one name.
/// Best-effort like the rest of this module: a `use`-imported bare name
/// (`Namespace::aliases`/`mod_aliases`/`static_uses`) is only offered when
/// `module_path` is the name's own module — a root-level `use` alias isn't
/// re-checked for visibility here the way `resolve_ctor` checks the owning
/// type's `public` flag, so it's simply left out of the cross-module case
/// rather than risking a false positive.
pub fn completion_candidates(reg: &Registry, module_path: &[String]) -> Vec<CompletionCandidate> {
    let mut out = Vec::new();
    if let Some(ns) = reg.root.module(module_path) {
        push_namespace(ns, &mut out, false);
    }
    if !module_path.is_empty() {
        push_namespace(&reg.root, &mut out, true);
    }
    out
}

/// Push every name in `ns` into `out`. `public_only` gates the four tables
/// that carry a `public` flag (fns/macros/types/vars/traits); constructor and
/// `use`-alias tables are only ever pushed when `public_only` is false (see
/// [`completion_candidates`]'s doc comment), and child module names are
/// always offered regardless (a module itself has no visibility flag today).
fn push_namespace(ns: &crate::Namespace, out: &mut Vec<CompletionCandidate>, public_only: bool) {
    for (name, sig) in &ns.fns {
        if !public_only || sig.public {
            out.push(CompletionCandidate {
                name: name.clone(),
                kind: CompletionKind::Function,
                detail: format!("({:?}) -> {:?}", sig.params, sig.ret),
            });
        }
    }
    for (name, def) in &ns.macros {
        if !public_only || def.public {
            out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Macro, detail: "macro".to_string() });
        }
    }
    for (name, def) in &ns.types {
        if !public_only || def.public {
            out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Type, detail: "type".to_string() });
        }
    }
    for (name, vi) in &ns.vars {
        if !public_only || vi.public {
            out.push(CompletionCandidate {
                name: name.clone(),
                kind: CompletionKind::Variable,
                detail: format!("{:?}", vi.ty),
            });
        }
    }
    for (name, def) in &ns.traits {
        if !public_only || def.public {
            out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Trait, detail: "trait".to_string() });
        }
    }
    for name in ns.modules.keys() {
        out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Module, detail: "module".to_string() });
    }
    if !public_only {
        for name in ns.ctors.keys() {
            out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Function, detail: "constructor".to_string() });
        }
        for name in ns.aliases.keys() {
            out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Function, detail: "use".to_string() });
        }
        for name in ns.mod_aliases.keys() {
            out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Module, detail: "use".to_string() });
        }
        for name in ns.static_uses.keys() {
            out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Method, detail: "use".to_string() });
        }
    }
}
