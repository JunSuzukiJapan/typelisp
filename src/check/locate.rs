//! Cursor-position lookups over a checked document, for the LSP's hover and
//! goto-definition (`src/bin/lsp.rs`).
//!
//! `Typed::loc` (`ast.rs`) is a source *span*: for a list-form node it covers
//! the opening through closing parenthesis (`Heap::cons_loc`); for a bare atom
//! checked as an *element of some enclosing list* (e.g. an argument, a `let`
//! binding value, a function body form) it's the span the reader recorded
//! for that specific occurrence (`Heap::elem_locs`, threaded through the
//! checker as `arg_locs`/`Checker::check_at`) — this is what lets a `Var`
//! (local variable) reference be found in its own right. A bare atom at top
//! level gets its span from the reader directly
//! (`Reader::read_all_in_spanned` -> `Checker::check_form_at`).
//!
//! [`locate_node`] prefers true containment: among every node whose span
//! contains the cursor (`Loc::contains`), the one with the greatest start
//! position is the innermost (contained nodes form a nesting chain; ties —
//! a wrapper node sharing its first child's exact position — are broken
//! toward the deeper node). A node with a *degenerate* span (end unknown —
//! e.g. one synthesized by macro expansion) can never contain a cursor, so
//! when nothing contains it the search falls back to the pre-span behavior:
//! the greatest start position `<=` cursor. That keeps positions past a
//! form's end (or in macro-synthesized trees) resolving to *something*
//! rather than nothing — `completion_locals` depends on that.

use crate::{CompileTarget, DefLocs, Expr, Loc, Pattern, Registry, TopLevel, Typed};

/// The innermost candidates seen so far during [`locate_node`]'s walk: one
/// among nodes whose span truly contains the cursor, one among nodes that
/// merely start at or before it (the pre-span behavior, kept as a fallback —
/// see the module doc comment). Each entry is (start position, depth, node).
#[derive(Default)]
struct Nearest<'a> {
    contained: Option<((u32, u32), usize, &'a Typed)>,
    preceding: Option<((u32, u32), usize, &'a Typed)>,
}

/// Find the smallest node in `body` whose span contains `(line, col)` in
/// `file`, falling back to the closest-preceding node when nothing does —
/// see the module doc comment. `line`/`col` are 1-based, matching [`Loc`].
pub fn locate_node<'a>(body: &'a [TopLevel], file: &str, line: u32, col: u32) -> Option<&'a Typed> {
    let cursor = (line, col);
    let mut best = Nearest::default();
    for tl in body {
        visit_top_level(tl, file, cursor, 0, &mut best);
    }
    best.contained.or(best.preceding).map(|(_, _, t)| t)
}

fn visit_top_level<'a>(
    tl: &'a TopLevel,
    file: &str,
    cursor: (u32, u32),
    depth: usize,
    best: &mut Nearest<'a>,
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
        // `Load` carries only a path string — nothing typed to locate into.
        TopLevel::Use { .. } | TopLevel::Defstruct { .. } | TopLevel::Defenum { .. } | TopLevel::Load { .. } => {}
    }
}

fn visit_typed<'a>(t: &'a Typed, file: &str, cursor: (u32, u32), depth: usize, best: &mut Nearest<'a>) {
    if let Some(loc) = &t.loc {
        if &*loc.file == file {
            let key = (loc.line, loc.col);
            if key <= cursor {
                // "Greatest start (ties broken deeper) wins" — the same rule
                // for both candidate kinds; contained nodes form a nesting
                // chain, so among them this picks the innermost.
                let update = |slot: &mut Option<((u32, u32), usize, &'a Typed)>| {
                    let better = match slot {
                        None => true,
                        Some((bk, bd, _)) => key > *bk || (key == *bk && depth > *bd),
                    };
                    if better {
                        *slot = Some((key, depth, t));
                    }
                };
                update(&mut best.preceding);
                if loc.contains(cursor.0, cursor.1) {
                    update(&mut best.contained);
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
        | Expr::Quote(_)
        | Expr::CompileFn(_) => vec![],
    }
}

/// Where `node` refers to, if it's a reference this pass can resolve.
/// `Global`/`Call`/`FnRef`/`Assoc`/`MethodRef`/`Construct` all already carry
/// a fully-qualified [`crate::Path`] (resolved at check time), so this is a
/// plain [`DefLocs`] lookup — no name resolution needed. A local variable
/// reference (`Expr::Var`) resolves through `DefLocs::local_refs` instead,
/// keyed by the reference's own position (`node.loc`, populated by
/// `Checker::check_at` for any atom checked as a list element — see
/// `check::locate`'s module doc comment) — that resolution already happened
/// once at check time (`Checker::check_at`'s `Env::get_loc` lookup), so this
/// is a lookup too, not a fresh scope search. A `Var` with no recorded
/// position of its own (rare — only an atom with no enclosing list at all)
/// falls through to `None`, same as any node this pass doesn't recognize as
/// a resolvable reference.
pub fn definition_target(node: &Typed, def_locs: &DefLocs) -> Option<Loc> {
    match &node.expr {
        Expr::Global(r) | Expr::SetGlobal(r, _) => def_locs.vars.get(&r.resolved).cloned(),
        Expr::FnRef(r) | Expr::Call(r, _) => def_locs.fns.get(&r.resolved).cloned(),
        Expr::MethodRef { type_name, method, .. } | Expr::Assoc { type_name, method, .. } => {
            def_locs.methods.get(&(type_name.clone(), method.clone())).cloned()
        }
        Expr::Construct { type_name, .. } => def_locs.types.get(type_name).cloned(),
        Expr::CompileFn(CompileTarget::Fn(r)) => def_locs.fns.get(&r.resolved).cloned(),
        Expr::CompileFn(CompileTarget::Method { type_name, method, .. }) => {
            def_locs.methods.get(&(type_name.clone(), method.clone())).cloned()
        }
        Expr::Var(_) => {
            let l = node.loc.as_ref()?;
            def_locs.local_refs.get(&(l.line, l.col)).cloned()
        }
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

/// Every local (`let`/`let*`/`lambda`/`labels` binding, or a `defun`/
/// `defmethod`/`defmacro`'s own parameter/receiver) in lexical scope at
/// `(line, col)` in `file` — for the LSP's completion (`src/bin/lsp.rs`) to
/// offer alongside [`completion_candidates`]'s `Registry`-derived names.
///
/// First finds the cursor's smallest enclosing node with [`locate_node`],
/// then re-walks `body` from the top, pushing each binding name it
/// encounters as it descends into that binding's scope — stopping (by
/// pointer identity, `body` being the same tree `locate_node` searched) at
/// the target node and returning whatever names are on the stack at that
/// point. A binding is only pushed once its own scope is entered — a `let`
/// binding's *value* expression, for instance, is checked in the *outer*
/// environment (CL `let`, not `let*`) and correctly does not see the name
/// being bound, matching `Checker::check_let`'s own order. A `match` arm's
/// pattern-bound names ([`pattern_bind_names`]) are pushed only for that
/// arm's own body — the scrutinee and sibling arms never see them,
/// matching `Checker::check_match`'s per-arm `extended_with_locs`.
///
/// Residual caveat (now limited to [`locate_node`]'s *fallback* path — a
/// containment hit is exact): when nothing's span contains the cursor
/// (e.g. a macro-synthesized tree with only degenerate spans), the cursor
/// is matched against the *closest preceding* node's start, with no way to
/// tell "inside that node's own span" from "already past it, nothing
/// positioned here yet". A cursor sitting right at a `let`/`lambda`/
/// `labels` scope boundary with *nothing at all* positioned in the new
/// scope can therefore resolve to a node one scope out (e.g. a binding
/// value), missing the boundary's own names. The LSP never hands in such a
/// position anyway: `handle_completion` (`src/bin/lsp.rs`) splices its
/// `(panic "")` placeholder at *exactly* the in-progress identifier's
/// position (and a fresh `(` truncates to `()`, whose `Unit` node keeps the
/// `(`'s own recorded element position), so the checked tree always has a
/// node at — and thus a target in — the scope being completed in. Only a
/// direct caller passing a position in a genuinely empty scope still sees
/// the caveat.
pub fn completion_locals(body: &[TopLevel], file: &str, line: u32, col: u32) -> Vec<String> {
    let Some(target) = locate_node(body, file, line, col) else {
        return Vec::new();
    };
    let target: *const Typed = target;
    let mut scope: Vec<String> = Vec::new();
    for tl in body {
        if scope_top_level(tl, target, &mut scope) {
            return scope;
        }
    }
    Vec::new()
}

/// [`completion_locals`]'s top-level-form half: seeds `scope` with a
/// `defun`/`defmethod`/`defmacro`'s own parameters (and receiver, for a
/// method) before descending into its body via [`scope_typed`]. Returns
/// `true` once `target` is found — `scope` is left holding the accumulated
/// names at that point (truncated back on a `false` return, so a sibling
/// top-level form's search starts clean).
fn scope_top_level(tl: &TopLevel, target: *const Typed, scope: &mut Vec<String>) -> bool {
    let mark = scope.len();
    let found = match tl {
        TopLevel::Defun { params, body, .. } => {
            scope.extend(params.iter().map(|(n, _)| n.clone()));
            body.iter().any(|t| scope_typed(t, target, scope))
        }
        TopLevel::Defmethod { self_name, params, body, .. } => {
            scope.extend(self_name.iter().cloned());
            scope.extend(params.iter().map(|(n, _)| n.clone()));
            body.iter().any(|t| scope_typed(t, target, scope))
        }
        TopLevel::Defmacro { params, body, .. } => {
            scope.extend(params.iter().cloned());
            body.iter().any(|t| scope_typed(t, target, scope))
        }
        TopLevel::Defvar { value, .. } => scope_typed(value, target, scope),
        TopLevel::Module { body, .. } => body.iter().any(|tl| scope_top_level(tl, target, scope)),
        TopLevel::Expr(t) => scope_typed(t, target, scope),
        TopLevel::Use { .. } | TopLevel::Defstruct { .. } | TopLevel::Defenum { .. } | TopLevel::Load { .. } => false,
    };
    if !found {
        scope.truncate(mark);
    }
    found
}

/// [`completion_locals`]'s expression half: `Let`/`Lambda`/`Labels` push
/// their bound names before recursing into the scope those names are
/// visible in; every other node just recurses into its children
/// ([`expr_children`]) with no scope change. Returns `true` once `target`
/// (compared by pointer identity — `node` is a descendant of the same tree
/// [`completion_locals`] called [`locate_node`] on) is found.
///
/// The pointer-identity check happens *inside* each scope-introducing arm,
/// after its names are pushed — not once, up front, before the `match` —
/// because `target` can legitimately resolve to the scope-introducing node
/// itself, not something inside it: `locate_node` returns the *smallest*
/// node with a recorded position at or before the cursor, and when the
/// cursor sits right after a `let`'s bindings list with nothing positioned
/// between there and the next real token (e.g. completing the very first
/// character of what will become the body — before that token exists to
/// have a position of its own), the `Let` node itself is the closest match.
/// An early, unconditional check here would return `true` before this arm's
/// own names were ever pushed, silently dropping them from the offered
/// scope.
fn scope_typed(node: &Typed, target: *const Typed, scope: &mut Vec<String>) -> bool {
    match &node.expr {
        Expr::Match(scrutinee, arms) => {
            // The scrutinee is checked in the outer scope; each arm's
            // pattern-bound names are visible only in that arm's own body
            // (`Checker::check_match`'s per-arm `extended_with_locs`).
            if scope_typed(scrutinee, target, scope) || std::ptr::eq(node, target) {
                return true;
            }
            for arm in arms {
                let mark = scope.len();
                pattern_bind_names(&arm.pat, scope);
                if arm.body.iter().any(|t| scope_typed(t, target, scope)) {
                    return true;
                }
                scope.truncate(mark);
            }
            false
        }
        Expr::Let(binds, body) => {
            // Each binding's value is checked in the *outer* scope (CL
            // `let`) — see `Checker::check_let` — so this must not see the
            // name(s) being bound here.
            if binds.iter().any(|(_, val)| scope_typed(val, target, scope)) {
                return true;
            }
            let mark = scope.len();
            scope.extend(binds.iter().map(|(n, _)| n.clone()));
            let found = std::ptr::eq(node, target) || body.iter().any(|t| scope_typed(t, target, scope));
            if !found {
                scope.truncate(mark);
            }
            found
        }
        Expr::Lambda { params, body } => {
            let mark = scope.len();
            scope.extend(params.iter().map(|(n, _)| n.clone()));
            let found = std::ptr::eq(node, target) || body.iter().any(|t| scope_typed(t, target, scope));
            if !found {
                scope.truncate(mark);
            }
            found
        }
        Expr::Labels { defs, body } => {
            let mark = scope.len();
            // Every function's name is visible to every body (including its
            // own) and to the trailing `body` — see `Checker::check_labels`.
            scope.extend(defs.iter().map(|(name, _, _)| name.clone()));
            if std::ptr::eq(node, target) {
                return true;
            }
            for (_, params, def_body) in defs {
                let pmark = scope.len();
                scope.extend(params.iter().map(|(n, _)| n.clone()));
                let found = def_body.iter().any(|t| scope_typed(t, target, scope));
                scope.truncate(pmark);
                if found {
                    return true;
                }
            }
            let found = body.iter().any(|t| scope_typed(t, target, scope));
            if !found {
                scope.truncate(mark);
            }
            found
        }
        _ => std::ptr::eq(node, target) || expr_children(&node.expr).into_iter().any(|c| scope_typed(c, target, scope)),
    }
}

/// Every name a match pattern binds, appended to `scope` in source order —
/// [`scope_typed`]'s `Match` arm pushes these for the arm body's walk.
/// `Wildcard` and literal patterns bind nothing; a `Ctor`'s field
/// sub-patterns can each bind (nested `Ctor`s included).
fn pattern_bind_names(pat: &Pattern, scope: &mut Vec<String>) {
    match pat {
        Pattern::Bind(name, _) => scope.push(name.clone()),
        Pattern::Ctor { args, .. } => {
            for sub in args {
                pattern_bind_names(sub, scope);
            }
        }
        Pattern::TypeTest(_, inner) => pattern_bind_names(inner, scope),
        Pattern::Wildcard | Pattern::Int(_) | Pattern::Bool(_) | Pattern::Char(_) => {}
    }
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
/// `module_path`: that module's own definitions, plus every ancestor
/// module's (root's included) — the same ancestor-chain walk
/// `Checker::resolve_fn`/`resolve_global`/etc. use at name-resolution time
/// (see `Checker::ns_ancestors`), just enumerated instead of looked up by
/// one name. A name found on this chain is in scope without needing `pub`,
/// exactly like at check time, so every table is offered in full at every
/// level. Best-effort like the rest of this module: a `use`-imported bare
/// name (`Namespace::aliases`/`mod_aliases`/`static_uses`) is only offered
/// from its own defining module, same as before.
pub fn completion_candidates(reg: &Registry, module_path: &[String]) -> Vec<CompletionCandidate> {
    let mut out = Vec::new();
    for k in (0..=module_path.len()).rev() {
        if let Some(ns) = reg.root.module(&module_path[..k]) {
            push_namespace(ns, &mut out);
        }
    }
    out
}

/// Push every name in `ns` into `out`.
fn push_namespace(ns: &crate::Namespace, out: &mut Vec<CompletionCandidate>) {
    for (name, sig) in &ns.fns {
        out.push(CompletionCandidate {
            name: name.clone(),
            kind: CompletionKind::Function,
            detail: format!("({:?}) -> {:?}", sig.params, sig.ret),
        });
    }
    for name in ns.macros.keys() {
        out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Macro, detail: "macro".to_string() });
    }
    for name in ns.types.keys() {
        out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Type, detail: "type".to_string() });
    }
    for (name, vi) in &ns.vars {
        out.push(CompletionCandidate {
            name: name.clone(),
            kind: CompletionKind::Variable,
            detail: format!("{:?}", vi.ty),
        });
    }
    for name in ns.traits.keys() {
        out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Trait, detail: "trait".to_string() });
    }
    for name in ns.modules.keys() {
        out.push(CompletionCandidate { name: name.clone(), kind: CompletionKind::Module, detail: "module".to_string() });
    }
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
