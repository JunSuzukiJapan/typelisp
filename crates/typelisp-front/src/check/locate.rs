//! Cursor-position lookups over a checked document, for the LSP's hover and
//! goto-definition (`src/bin/lsp.rs`).
//!
//! A located node is a core form, and its span is the cell's own `self_loc`
//! (`Heap::cons_loc`): for a list read from source that covers the opening
//! through closing parenthesis, and for a node the checker built it is the span
//! of the form it was lowered from. A bare atom checked as an *element of some
//! enclosing list* (an argument, a `let` binding value, a body form) is lowered
//! to a node of its own — `(var x)`, `(int 1)` — which carries that
//! occurrence's own span, and that is what lets a local-variable reference be
//! found in its own right.
//!
//! [`locate_node`] prefers true containment: among every node whose span
//! contains the cursor (`Loc::contains`), the one with the greatest start
//! position is the innermost (contained nodes form a nesting chain; ties — a
//! wrapper node sharing its first child's exact position — are broken toward
//! the deeper node). A node with a *degenerate* span (end unknown — e.g. one
//! synthesized by macro expansion) can never contain a cursor, so when nothing
//! contains it the search falls back to the greatest start position `<=`
//! cursor. That keeps positions past a form's end resolving to *something*
//! rather than nothing — [`completion_locals`] depends on that.
//!
//! # Why this module got smaller
//!
//! Over the `Typed` tree, descending needed a 44-line table naming every
//! `Expr` variant's children, and the top level needed a second one per
//! `TopLevel` variant. A core form's children are just its fields, so the
//! descent is uniform and both tables are gone. The one thing a walk must
//! still know is where data stops being program: [`is_opaque`].

use typelisp_mem::{Heap, Value};

use crate::check::core;
use crate::{DefLocs, Docs, Loc, Path, Registry, TopLevelForm};

/// Tags whose fields are *not* sub-expressions, so a walk must not descend
/// into them.
///
/// `quote` is the only place user data appears in a lowered program, and a
/// quoted `(call ...)` datum is data — descending would report a hover for a
/// node that does not exist. The pattern tags are the other case: a
/// `(pat-lit (int 1))`'s literal is part of the pattern, not an expression
/// evaluated at that position, and a pattern binds rather than refers.
fn is_opaque(tag: &str) -> bool {
    matches!(tag, "quote" | "pat-wild" | "pat-bind" | "pat-lit" | "pat-ctor" | "pat-typetest")
}

/// The innermost candidates seen so far during [`locate_node`]'s walk: one
/// among nodes whose span truly contains the cursor, one among nodes that
/// merely start at or before it. Each entry is (start position, depth, node).
#[derive(Default)]
struct Nearest {
    contained: Option<((u32, u32), usize, Value)>,
    preceding: Option<((u32, u32), usize, Value)>,
}

/// Find the smallest node in `body` whose span contains `(line, col)` in
/// `file`, falling back to the closest-preceding node when nothing does —
/// see the module doc comment. `line`/`col` are 1-based, matching [`Loc`].
pub fn locate_node(heap: &Heap, body: &[TopLevelForm], file: &str, line: u32, col: u32) -> Option<Value> {
    let cursor = (line, col);
    let mut best = Nearest::default();
    for tl in body {
        visit(heap, *tl, file, cursor, 0, &mut best);
    }
    best.contained.or(best.preceding).map(|(_, _, t)| t)
}

/// Consider `form` as a candidate, then descend into its fields.
///
/// Every node is considered, the top-level ones included: a `(defun ...)`'s own
/// span is a legitimate hover target, and its fields are its body — so unlike
/// the `Typed` walk there is nothing to special-case about the top level.
fn visit(heap: &Heap, form: Value, file: &str, cursor: (u32, u32), depth: usize, best: &mut Nearest) {
    if !matches!(form, Value::Cons(_)) {
        return;
    }
    if let Some(loc) = heap.cons_loc(form) {
        if &*loc.file == file {
            let key = (loc.line, loc.col);
            if key <= cursor {
                // "Greatest start (ties broken deeper) wins" — the same rule
                // for both candidate kinds; contained nodes form a nesting
                // chain, so among them this picks the innermost.
                let update = |slot: &mut Option<((u32, u32), usize, Value)>| {
                    let better = match slot {
                        None => true,
                        Some((bk, bd, _)) => key > *bk || (key == *bk && depth > *bd),
                    };
                    if better {
                        *slot = Some((key, depth, form));
                    }
                };
                update(&mut best.preceding);
                if loc.contains(cursor.0, cursor.1) {
                    update(&mut best.contained);
                }
            }
        }
    }
    match core::op(heap, form) {
        Some(tag) if is_opaque(tag) => return,
        Some(_) => {
            let Ok(fields) = core::fields(heap, form) else { return };
            for f in fields {
                visit(heap, f, file, cursor, depth + 1, best);
            }
        }
        // A cons whose head is not a symbol is a *list* of nodes — a `match`
        // arm, a `let` binding, a `labels` definition. Its elements are the
        // nodes, so it is not itself a candidate's parent in the same sense;
        // walking its elements at the same depth keeps the depth measuring
        // expression nesting rather than list structure.
        None => {
            let Ok(items) = heap.list_to_vec(form) else { return };
            for item in items {
                visit(heap, item, file, cursor, depth, best);
            }
        }
    }
}

/// Where `node` refers to, if it's a reference this pass can resolve.
///
/// `global`/`set-global`/`call`/`fnref`/`assoc`/`methodref`/`construct` all
/// already carry a fully-qualified [`Path`] (resolved at check time), so this is
/// a plain [`DefLocs`] lookup — no name resolution needed. A local variable
/// reference (`var`) resolves through `DefLocs::local_refs` instead, keyed by
/// the reference's own position: that resolution already happened once at check
/// time (`Checker::check_at`'s `Env::get_loc` lookup), so this is a lookup too,
/// not a fresh scope search. A `var` with no recorded position of its own falls
/// through to `None`, same as any node this pass doesn't recognize.
pub fn definition_target(heap: &Heap, node: Value, def_locs: &DefLocs) -> Option<Loc> {
    match core::op(heap, node)? {
        // `(global (WRITTEN...) (HOME...) PATH REPR)` and `set-global`'s same
        // leading triple.
        "global" | "set-global" => def_locs.vars.get(&path_at(heap, node, 2)?).cloned(),
        // `(call (WRITTEN...) (HOME...) PATH ...)`, `fnref`'s same triple.
        "call" | "fnref" => def_locs.fns.get(&path_at(heap, node, 2)?).cloned(),
        // `(assoc PATH SYM ...)`, `(methodref PATH SYM ...)`.
        "assoc" | "methodref" => {
            def_locs.methods.get(&(path_at(heap, node, 0)?, sym_at(heap, node, 1)?)).cloned()
        }
        "construct" => def_locs.types.get(&path_at(heap, node, 0)?).cloned(),
        // `(compile-fn ...)` names either a function or a method; which one is
        // told by whether a method name follows the path.
        "compile-fn" => match sym_at(heap, node, 1) {
            Some(m) => def_locs.methods.get(&(path_at(heap, node, 0)?, m)).cloned(),
            None => def_locs.fns.get(&path_at(heap, node, 0)?).cloned(),
        },
        "var" => {
            let l = heap.cons_loc(node)?;
            def_locs.local_refs.get(&(l.line, l.col)).cloned()
        }
        _ => None,
    }
}

/// `node`'s docstring, if it's a reference to a documented definition —
/// [`definition_target`]'s `Docs` counterpart, over the identical set of
/// resolved-`Path`-carrying tags (a `var` local has no docstring of its own, so
/// unlike `definition_target` there's no `local_refs` analog to fall back to).
pub fn doc_for<'a>(heap: &Heap, node: Value, docs: &'a Docs) -> Option<&'a str> {
    let found = match core::op(heap, node)? {
        "global" | "set-global" => docs.vars.get(&path_at(heap, node, 2)?),
        "call" | "fnref" => docs.fns.get(&path_at(heap, node, 2)?),
        "assoc" | "methodref" => docs.methods.get(&(path_at(heap, node, 0)?, sym_at(heap, node, 1)?)),
        "construct" => docs.types.get(&path_at(heap, node, 0)?),
        "compile-fn" => match sym_at(heap, node, 1) {
            Some(m) => docs.methods.get(&(path_at(heap, node, 0)?, m)),
            None => docs.fns.get(&path_at(heap, node, 0)?),
        },
        _ => None,
    };
    found.map(|s| s.as_str())
}

/// Hover text for `node`: its checked type, plus its docstring (if any) —
/// `doc_for`'s result appended below a blank line, the conventional LSP hover
/// shape (signature/type first, prose after).
///
/// The type comes from `DefLocs::node_types`, keyed by the node's own position,
/// rather than from the node: a lowered form does not carry its type (see
/// `check::core::Checked`), so the checker records it where both are in hand.
/// A node with no recorded type — one the checker synthesized without a source
/// position — shows its docstring alone, or nothing.
pub fn hover_text(heap: &Heap, node: Value, def_locs: &DefLocs, docs: &Docs) -> String {
    let ty = heap
        .cons_loc(node)
        .and_then(|l| def_locs.node_types.get(&(l.line, l.col)))
        .map(|t| format!("{:?}", t));
    match (ty, doc_for(heap, node, docs)) {
        (Some(ty), Some(doc)) => format!("{}\n\n{}", ty, doc),
        (Some(ty), None) => ty,
        (None, Some(doc)) => doc.to_string(),
        (None, None) => String::new(),
    }
}

/// Every local (`let`/`lambda`/`labels` binding, or a `defun`/`defmethod`/
/// `defmacro`'s own parameter) in lexical scope at `(line, col)` in `file` —
/// for the LSP's completion (`src/bin/lsp.rs`) to offer alongside
/// [`completion_candidates`]'s `Registry`-derived names.
///
/// First finds the cursor's smallest enclosing node with [`locate_node`], then
/// re-walks `body` from the top, pushing each binding name it encounters as it
/// descends into that binding's scope — stopping (by cell identity, `body`
/// being the same forms `locate_node` searched) at the target node and
/// returning whatever names are on the stack at that point. A binding is only
/// pushed once its own scope is entered: a `let` binding's *value* is checked
/// in the outer environment (CL `let`, not `let*`) and correctly does not see
/// the name being bound, matching `Checker::check_let`'s own order. A `match`
/// arm's pattern-bound names are pushed only for that arm's own body, matching
/// `Checker::check_match`'s per-arm `extended_with_locs`.
pub fn completion_locals(heap: &Heap, body: &[TopLevelForm], file: &str, line: u32, col: u32) -> Vec<String> {
    let Some(target) = locate_node(heap, body, file, line, col) else {
        return Vec::new();
    };
    let mut scope: Vec<String> = Vec::new();
    for tl in body {
        if scope_walk(heap, *tl, target, &mut scope) {
            return scope;
        }
    }
    Vec::new()
}

/// Descend toward `target`, keeping `scope` holding exactly the names in scope
/// at the current point. Returns `true` once `target` is reached, leaving
/// `scope` as it was there; on `false` every name this level pushed is popped,
/// so a sibling's search starts clean.
fn scope_walk(heap: &Heap, form: Value, target: Value, scope: &mut Vec<String>) -> bool {
    if form == target {
        return true;
    }
    if !matches!(form, Value::Cons(_)) {
        return false;
    }
    let Some(tag) = core::op(heap, form).map(str::to_string) else {
        // A list of nodes rather than a node: `match` arms, `let` bindings.
        let Ok(items) = heap.list_to_vec(form) else { return false };
        return items.into_iter().any(|item| scope_walk(heap, item, target, scope));
    };
    if is_opaque(&tag) {
        return false;
    }
    let mark = scope.len();
    let found = match tag.as_str() {
        // `(defun PATH ((SYM R)...) RET-R PUBLIC E...)` — the parameters are in
        // scope throughout the body. A `defmethod`'s receiver is simply its
        // first parameter, so it needs no case of its own.
        "defun" => {
            scope.extend(param_names(heap, form, 1));
            fields_from(heap, form, 4).into_iter().any(|f| scope_walk(heap, f, target, scope))
        }
        "defmethod" => {
            scope.extend(param_names(heap, form, 3));
            fields_from(heap, form, 6).into_iter().any(|f| scope_walk(heap, f, target, scope))
        }
        // `(defmacro PATH (SYM...) REST LAMBDA PUBLIC E...)` — the parameters
        // are bare symbols (a macro's are all `sexpr`). The `&optional`/`&key`
        // default forms are checked expressions too, so a cursor can land in
        // one.
        "defmacro" => {
            scope.extend(sym_list_at(heap, form, 1));
            let lambda = core::field(heap, form, 3);
            let in_defaults = lambda
                .map(|l| scope_walk(heap, l, target, scope))
                .unwrap_or(false);
            in_defaults || fields_from(heap, form, 5).into_iter().any(|f| scope_walk(heap, f, target, scope))
        }
        // `(let ((SYM R E)...) E...)`. Each binding's value is checked in the
        // *outer* scope, so this must not see the names being bound here.
        "let" => {
            let binds = core::field(heap, form, 0);
            let in_values = binds
                .map(|b| {
                    heap.list_to_vec(b)
                        .map(|bs| {
                            bs.into_iter().any(|one| {
                                core::field(heap, one, 1)
                                    .map(|v| scope_walk(heap, v, target, scope))
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if in_values {
                return true;
            }
            if let Some(b) = binds {
                if let Ok(bs) = heap.list_to_vec(b) {
                    for one in bs {
                        if let Some(n) = list_head_sym(heap, one) {
                            scope.push(n);
                        }
                    }
                }
            }
            fields_from(heap, form, 1).into_iter().any(|f| scope_walk(heap, f, target, scope))
        }
        // `(lambda ((SYM R)...) RET-R E...)`.
        "lambda" => {
            scope.extend(param_names(heap, form, 0));
            fields_from(heap, form, 2).into_iter().any(|f| scope_walk(heap, f, target, scope))
        }
        // `(labels ((SYM ((SYM R)...) RET-R E...)...) E...)`. Every function's
        // name is visible to every body (its own included) and to the trailing
        // body — see `Checker::check_labels`.
        "labels" => {
            let defs = core::field(heap, form, 0).and_then(|d| heap.list_to_vec(d).ok()).unwrap_or_default();
            for d in &defs {
                if let Some(n) = list_head_sym(heap, *d) {
                    scope.push(n);
                }
            }
            let mut hit = false;
            for d in &defs {
                let pmark = scope.len();
                scope.extend(param_names(heap, *d, 0));
                // A `labels` definition is a bare list, so its body starts
                // after the name, parameters and return representation.
                let body = heap.list_to_vec(*d).map(|v| v.into_iter().skip(3).collect::<Vec<_>>()).unwrap_or_default();
                hit = body.into_iter().any(|f| scope_walk(heap, f, target, scope));
                if hit {
                    break;
                }
                scope.truncate(pmark);
            }
            hit || fields_from(heap, form, 1).into_iter().any(|f| scope_walk(heap, f, target, scope))
        }
        // `(match E R (P E...)...)`. The scrutinee is checked in the outer
        // scope; each arm's pattern-bound names are visible only in that arm's
        // own body.
        "match" => {
            if let Some(scrut) = core::field(heap, form, 0) {
                if scope_walk(heap, scrut, target, scope) {
                    return true;
                }
            }
            let mut hit = false;
            for arm in fields_from(heap, form, 2) {
                let amark = scope.len();
                let parts = heap.list_to_vec(arm).unwrap_or_default();
                if let Some((pat, body)) = parts.split_first() {
                    pattern_bind_names(heap, *pat, scope);
                    hit = body.iter().any(|f| scope_walk(heap, *f, target, scope));
                }
                if hit {
                    break;
                }
                scope.truncate(amark);
            }
            hit
        }
        // Every other node binds nothing; its fields are its sub-expressions.
        _ => {
            let Ok(fields) = core::fields(heap, form) else { return false };
            fields.into_iter().any(|f| scope_walk(heap, f, target, scope))
        }
    };
    if !found {
        scope.truncate(mark);
    }
    found
}

/// Every name a pattern binds, appended in source order. `pat-wild` and
/// `pat-lit` bind nothing; a `pat-ctor`'s field sub-patterns can each bind
/// (nested constructors included).
fn pattern_bind_names(heap: &Heap, pat: Value, scope: &mut Vec<String>) {
    match core::op(heap, pat) {
        Some("pat-bind") => {
            if let Some(n) = sym_at(heap, pat, 0) {
                scope.push(n);
            }
        }
        // `(pat-ctor PATH VARIANT DOWNCAST (REPR...) SUB...)`.
        Some("pat-ctor") => {
            for sub in fields_from(heap, pat, 4) {
                pattern_bind_names(heap, sub, scope);
            }
        }
        // `(pat-typetest PATH SUB)`.
        Some("pat-typetest") => {
            if let Some(sub) = core::field(heap, pat, 1) {
                pattern_bind_names(heap, sub, scope);
            }
        }
        _ => {}
    }
}

/// A node's fields from `skip` onward.
fn fields_from(heap: &Heap, form: Value, skip: usize) -> Vec<Value> {
    core::fields(heap, form).map(|fs| fs.get(skip..).unwrap_or(&[]).to_vec()).unwrap_or_default()
}

/// The names a parameter list `((SYM R)...)` at field `i` binds.
fn param_names(heap: &Heap, form: Value, i: usize) -> Vec<String> {
    let Some(list) = core::field(heap, form, i) else { return Vec::new() };
    heap.list_to_vec(list)
        .map(|ps| ps.into_iter().filter_map(|p| list_head_sym(heap, p)).collect())
        .unwrap_or_default()
}

/// A bare symbol list `(SYM...)` at field `i`.
fn sym_list_at(heap: &Heap, form: Value, i: usize) -> Vec<String> {
    let Some(list) = core::field(heap, form, i) else { return Vec::new() };
    heap.list_to_vec(list)
        .map(|vs| {
            vs.into_iter()
                .filter_map(|v| match v {
                    Value::Symbol(id) => Some(heap.symbol_name(id).to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Field `i` as a path. A single-segment path reads back as a bare symbol (the
/// reader only builds `Value::Path` when it sees `::`).
fn path_at(heap: &Heap, form: Value, i: usize) -> Option<Path> {
    match core::field(heap, form, i)? {
        Value::Path(id) => Some(crate::types::path_from_id(heap, id)),
        Value::Symbol(id) => Some(Path::root(heap.symbol_name(id))),
        _ => None,
    }
}

/// Field `i` of a *node* as a symbol name — `core::field`'s numbering, which
/// counts past the tag.
fn sym_at(heap: &Heap, form: Value, i: usize) -> Option<String> {
    match core::field(heap, form, i)? {
        Value::Symbol(id) => Some(heap.symbol_name(id).to_string()),
        _ => None,
    }
}

/// The first element of a *bare list* as a symbol name.
///
/// Deliberately separate from [`sym_at`]: a bare list has no tag, so its first
/// element is its `car` and `core::field(_, 0)` is its *second*. A `let`
/// binding `(SYM REPR FORM)`, a `labels` definition `(SYM PARAMS RET BODY...)`
/// and a parameter `(SYM REPR)` all name themselves in that first position.
fn list_head_sym(heap: &Heap, list: Value) -> Option<String> {
    match heap.car(list).ok()? {
        Value::Symbol(id) => Some(heap.symbol_name(id).to_string()),
        _ => None,
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
