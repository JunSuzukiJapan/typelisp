//! Free-variable analysis for `labels` capture support (Stage 2 of the
//! labels/closures compilation work — see `compiler.rs`'s doc comment for
//! the overall design).
//!
//! [`labels_free_vars`] computes *one* captured-name list for an entire
//! `labels` block: the union, over every def, of names its body references
//! that are neither that def's own parameter nor any sibling's name
//! (siblings always resolve to a direct call, never a captured value — see
//! `ast_bridge::translate_apply`). Every sibling shares this single list as
//! its environment, rather than each computing its own narrower one. That
//! sharing is what makes sibling-to-sibling calls work without a second,
//! transitive analysis pass: if sibling `A` calls sibling `B` and only `B`'s
//! body mentions an outer variable, `A` still received that variable in its
//! own (shared) environment, so it has a value on hand to forward — no need
//! to ask "which captures does the function I'm calling need" at every call
//! site.
//!
//! Each collected name carries its [`Type`] alongside it (the automatic
//! `ClosureBox` retain/release insertion work needs to know, per captured
//! name, whether it's `Fn`-typed — see `compiler.rs`'s module doc comment —
//! and this is the one place that type is still on hand, since it comes
//! straight from the referencing `Var` node's own `Typed::ty`).

use std::collections::HashSet;

use crate::{Expr, LabelDef, Pattern, Type, Typed};

/// The free variables of a single `lambda`'s body (labels/closures Stage
/// 4) — [`labels_free_vars`]'s one-function counterpart, for a `lambda`
/// that has no siblings of its own at all (unlike a `labels` def). Walked
/// with an *empty* siblings set — matching [`walk`]'s own existing
/// nested-`Lambda` arm exactly (a `lambda` never gets direct-call access to
/// whatever `labels` block encloses it, so any name from there that its body
/// references becomes an ordinary free variable/capture attempt here too,
/// not a resolved direct call). When that name turns out to be a sibling
/// *function* rather than an ordinary value, `compiler.rs`'s `compile-lambda`
/// still resolves it correctly (a follow-up to Stage 4): it builds *this*
/// lambda's `ClosureBox` env array in the outer scope, where the real
/// `fn-env` this sibling lives in is still available, so `resolve-value`
/// boxes it there before this lambda's own `bind-captures` ever runs.
pub fn lambda_free_vars(params: &[(String, Type)], body: &[Typed]) -> Vec<(String, Type)> {
    let bound: HashSet<String> = params.iter().map(|(n, _)| n.clone()).collect();
    let mut seen = HashSet::new();
    let mut order = Vec::new();
    walk_body(body, &bound, &HashSet::new(), &mut seen, &mut order);
    order
}

/// The free variables of a whole `labels` block, deduplicated, in
/// first-occurrence order (defs in the given order, then within each def's
/// body) — that order becomes the shared captured environment's slot order.
///
/// `outer_direct` is whatever names an *enclosing* `labels` block already
/// resolves to a direct call (`ast_bridge::ast_to_sexpr_scoped`'s `direct`
/// set) — a nested `labels` referencing one of those names is calling it,
/// not capturing a value, so it must be excluded here exactly like this
/// block's own sibling names, or this analysis would manufacture a bogus
/// captured slot for a name `compile-apply` resolves through `fn-env`
/// instead of `env`.
///
/// `outer_captured` is the *closest enclosing* `labels` block's own shared
/// captured-list (empty at the top level, or when this block isn't nested
/// inside another) — unconditionally copied into the front of this block's
/// own result, regardless of whether any def here actually references one
/// of those names directly. This is what makes a nested `labels` block's
/// captured-list a strict prefix-superset of every block enclosing it,
/// however deep the nesting: calling an enclosing block's own sibling (which
/// `compiler.rs`'s `compile-apply` resolves via `fn-env`, now correctly
/// shared down through nesting — see that module's doc comment) still needs
/// that sibling's *own* captured values forwarded along, and the only way
/// this def's compiled body can have them on hand to forward is if they're
/// already in its own captured list too. Padding the list with names this
/// block's own defs never reference is the trade-off that buys this — same
/// idea as `compile-apply`'s own "every sibling in one block shares the same
/// captured list" sharing, just extended across nesting levels instead of
/// across siblings within one level.
pub fn labels_free_vars(defs: &[LabelDef], outer_direct: &HashSet<String>, outer_captured: &[(String, Type)]) -> Vec<(String, Type)> {
    let mut siblings: HashSet<String> = outer_direct.clone();
    for (name, _, _) in defs {
        siblings.insert(name.clone());
    }
    let mut seen = HashSet::new();
    let mut order = Vec::new();
    for (name, ty) in outer_captured {
        if seen.insert(name.clone()) {
            order.push((name.clone(), ty.clone()));
        }
    }
    for (_, params, body) in defs {
        let bound: HashSet<String> = params.iter().map(|(n, _)| n.clone()).collect();
        walk_body(body, &bound, &siblings, &mut seen, &mut order);
    }
    order
}

fn walk_body(body: &[Typed], bound: &HashSet<String>, siblings: &HashSet<String>, seen: &mut HashSet<String>, order: &mut Vec<(String, Type)>) {
    for typed in body {
        walk(typed, bound, siblings, seen, order);
    }
}

fn note(name: &str, ty: &Type, bound: &HashSet<String>, siblings: &HashSet<String>, seen: &mut HashSet<String>, order: &mut Vec<(String, Type)>) {
    if bound.contains(name) || siblings.contains(name) {
        return;
    }
    if seen.insert(name.to_string()) {
        order.push((name.to_string(), ty.clone()));
    }
}

fn walk(typed: &Typed, bound: &HashSet<String>, siblings: &HashSet<String>, seen: &mut HashSet<String>, order: &mut Vec<(String, Type)>) {
    match &typed.expr {
        Expr::Int(_) | Expr::Float(_) | Expr::Bignum(_) | Expr::Ratio(_) | Expr::Bool(_) | Expr::Char(_) | Expr::Str(_) | Expr::SymLit(_) | Expr::Unit => {}
        Expr::Var(name) => note(name, &typed.ty, bound, siblings, seen, order),
        Expr::Global(_) | Expr::FnRef(_) | Expr::MethodRef { .. } => {}
        Expr::If(cond, then, els) => {
            // Walks a right-leaning `if`/`else-if` chain iteratively rather
            // than recursing once per link — the same reason
            // `Interp::eval`'s own `Expr::If` arm does (see that arm's doc
            // comment): a chain the size of `compiler.rs`'s `compile-value`
            // dispatcher (~40 tags) is deep enough to overflow the stack on
            // a naive recursive walk, and unlike `eval` (which only ever
            // needs to descend into whichever single branch the condition
            // selects), a free-variable walk must visit *every* branch —
            // but only the `els` side ever chains to another `Expr::If`, so
            // iterating that one link keeps the other two (`cond`/`then`,
            // ordinary recursive calls) at their natural, shallow depth.
            let mut cond = cond;
            let mut then = then;
            let mut els = els;
            loop {
                walk(cond, bound, siblings, seen, order);
                walk(then, bound, siblings, seen, order);
                match &els.expr {
                    Expr::If(c2, t2, e2) => {
                        cond = c2;
                        then = t2;
                        els = e2;
                    }
                    _ => {
                        walk(els, bound, siblings, seen, order);
                        break;
                    }
                }
            }
        }
        Expr::Let(bindings, body) => {
            for (_, value) in bindings {
                walk(value, bound, siblings, seen, order);
            }
            let mut inner = bound.clone();
            for (name, _) in bindings {
                inner.insert(name.clone());
            }
            walk_body(body, &inner, siblings, seen, order);
        }
        Expr::Call(_, args) => {
            for a in args {
                walk(a, bound, siblings, seen, order);
            }
        }
        Expr::Lambda { params, body } => {
            let mut inner = bound.clone();
            for (name, _) in params {
                inner.insert(name.clone());
            }
            // A nested `lambda` no longer treats `siblings` as
            // direct-callable names (only a `labels` def's own siblings
            // get that treatment — see `ast_bridge::translate_apply`), so
            // any sibling name it references becomes an ordinary free
            // variable here, propagating outward like any other capture.
            walk_body(body, &inner, &HashSet::new(), seen, order);
        }
        Expr::Labels { defs, body } => {
            let mut inner_siblings = siblings.clone();
            for (name, _, _) in defs {
                inner_siblings.insert(name.clone());
            }
            for (_, params, fbody) in defs {
                let mut inner_bound = bound.clone();
                for (name, _) in params {
                    inner_bound.insert(name.clone());
                }
                walk_body(fbody, &inner_bound, &inner_siblings, seen, order);
            }
            walk_body(body, bound, &inner_siblings, seen, order);
        }
        Expr::Apply(callee, args) => {
            walk(callee, bound, siblings, seen, order);
            for a in args {
                walk(a, bound, siblings, seen, order);
            }
        }
        Expr::Assoc { args, .. } | Expr::TraitCall { args, .. } | Expr::DynCall { args, .. } => {
            for a in args {
                walk(a, bound, siblings, seen, order);
            }
        }
        Expr::DynBox { value, .. } => walk(value, bound, siblings, seen, order),
        Expr::DynValue(inner) => walk(inner, bound, siblings, seen, order),
        Expr::Construct { args, .. } => {
            for a in args {
                walk(a, bound, siblings, seen, order);
            }
        }
        Expr::FieldGet(inner, _) => walk(inner, bound, siblings, seen, order),
        Expr::FieldSet(inner, _, value) => {
            walk(inner, bound, siblings, seen, order);
            walk(value, bound, siblings, seen, order);
        }
        Expr::Match(scrutinee, arms) => {
            walk(scrutinee, bound, siblings, seen, order);
            for arm in arms {
                let mut inner = bound.clone();
                collect_pattern_bindings(&arm.pat, &mut inner);
                walk_body(&arm.body, &inner, siblings, seen, order);
            }
        }
        // `typed.ty` here is the *target variable's* type, not `Set`'s own
        // expression type — `Checker::check_setf` builds the `Expr::Set` node
        // with `ty` taken from `env.get(&name)` (see `translate_set`'s doc
        // comment in `ast_bridge.rs`), so recording it as `name`'s type here
        // is exactly right. That matters because `ast_bridge.rs` does
        // translate `Expr::Set` for real, and `tagged_sym_list` feeds each
        // captured name's recorded type through `binding_kind`/
        // `struct_field_kind` to pick its closure-env slot's kind tag — a
        // name captured *only* on a `set`'s left-hand side still gets the
        // correct kind from this.
        Expr::Set(name, value) => {
            note(name, &typed.ty, bound, siblings, seen, order);
            walk(value, bound, siblings, seen, order);
        }
        Expr::SetGlobal(_, value) => walk(value, bound, siblings, seen, order),
        Expr::Loop(body) => walk_body(body, bound, siblings, seen, order),
        Expr::Break => {}
        Expr::Return(value) => {
            if let Some(v) = value {
                walk(v, bound, siblings, seen, order);
            }
        }
        Expr::Panic(msg) => walk(msg, bound, siblings, seen, order),
        Expr::Quote(_) => {}
        Expr::CompileFn(_) => {}
    }
}

/// Names bound in the *enclosing* function-like scope (a `defun`/`lambda`/
/// `labels`-def body under translation) that some directly-or-transitively
/// nested `lambda`/`labels` captures — closure-representation unification,
/// Stage 4. `ast_bridge` intersects this against its own params/`let`-bound
/// names to decide which specific bindings must be promoted to a shared
/// `BoxedObj::Cell` at bind time (`compiler.rs`'s `bind-params`/
/// `bind-let-values`) rather than living in an ordinary stack slot — the
/// promotion a mutable *captured* binding needs so a `setf` inside the
/// capturing closure is visible everywhere else that shares it (CL
/// semantics, matching the interpreter's own heap-cell captures).
///
/// Walks `body` without descending into a found nested `Lambda`/`Labels`'s
/// own body: [`lambda_free_vars`]/[`labels_free_vars`] already resolve
/// arbitrarily deep beneath it (a name two closures deep still surfaces
/// here, since neither of those functions' own walks stop at a nested
/// closure boundary either — they only stop at *their own* params). Called
/// with an empty `outer_direct`/`outer_captured` for the `Labels` case
/// deliberately: the extra names that imprecision can add are never among
/// the enclosing scope's own bound names anyway (a `labels` sibling name
/// lives in a separate `fn-env` namespace, and a name from even further out
/// can't collide with one this body itself binds), so the caller's later
/// intersection against its own bound-name set silently discards them.
///
/// Deliberately over-approximates rather than being scope-precise about
/// shadowing (a name that's both an outer binding *and* an unrelated,
/// shadowing inner one can get flagged from either occurrence) — Stage 4's
/// explicit "correctness first" scope: an unnecessary cell costs a little
/// codegen complexity, never correctness, and precise shadow-tracking is
/// deferred to a later optimization pass (only a captured-and-`setf`
/// binding truly needs one at all; Stage 4 cell-boxes every captured
/// binding unconditionally).
pub fn names_captured_by_nested(body: &[Typed]) -> HashSet<String> {
    let mut out = HashSet::new();
    for t in body {
        collect_nested_captures(t, &mut out);
    }
    out
}

fn collect_nested_captures(typed: &Typed, out: &mut HashSet<String>) {
    match &typed.expr {
        Expr::Lambda { params, body } => {
            for (n, _) in lambda_free_vars(params, body) {
                out.insert(n);
            }
        }
        Expr::Labels { defs, body } => {
            for (n, _) in labels_free_vars(defs, &HashSet::new(), &[]) {
                out.insert(n);
            }
            for t in body {
                collect_nested_captures(t, out);
            }
        }
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Bignum(_)
        | Expr::Ratio(_)
        | Expr::Bool(_)
        | Expr::Char(_)
        | Expr::Str(_)
        | Expr::SymLit(_)
        | Expr::Unit
        | Expr::Var(_)
        | Expr::Global(_)
        | Expr::FnRef(_)
        | Expr::MethodRef { .. }
        | Expr::Break
        | Expr::Quote(_)
        | Expr::CompileFn(_) => {}
        Expr::If(c, t2, e) => {
            collect_nested_captures(c, out);
            collect_nested_captures(t2, out);
            collect_nested_captures(e, out);
        }
        Expr::Let(bindings, body) => {
            for (_, v) in bindings {
                collect_nested_captures(v, out);
            }
            for t in body {
                collect_nested_captures(t, out);
            }
        }
        Expr::Call(_, args) => {
            for a in args {
                collect_nested_captures(a, out);
            }
        }
        Expr::Apply(callee, args) => {
            collect_nested_captures(callee, out);
            for a in args {
                collect_nested_captures(a, out);
            }
        }
        Expr::Assoc { args, .. }
        | Expr::TraitCall { args, .. }
        | Expr::DynCall { args, .. }
        | Expr::Construct { args, .. } => {
            for a in args {
                collect_nested_captures(a, out);
            }
        }
        Expr::DynBox { value, .. } => collect_nested_captures(value, out),
        Expr::DynValue(inner) => collect_nested_captures(inner, out),
        Expr::FieldGet(inner, _) => collect_nested_captures(inner, out),
        Expr::FieldSet(inner, _, value) => {
            collect_nested_captures(inner, out);
            collect_nested_captures(value, out);
        }
        Expr::Match(scrutinee, arms) => {
            collect_nested_captures(scrutinee, out);
            for arm in arms {
                for t in &arm.body {
                    collect_nested_captures(t, out);
                }
            }
        }
        Expr::Set(_, value) => collect_nested_captures(value, out),
        Expr::SetGlobal(_, value) => collect_nested_captures(value, out),
        Expr::Loop(body) => {
            for t in body {
                collect_nested_captures(t, out);
            }
        }
        Expr::Return(value) => {
            if let Some(v) = value {
                collect_nested_captures(v, out);
            }
        }
        Expr::Panic(msg) => collect_nested_captures(msg, out),
    }
}

fn collect_pattern_bindings(pat: &Pattern, bound: &mut HashSet<String>) {
    match pat {
        Pattern::Wildcard | Pattern::Int(_) | Pattern::Bool(_) | Pattern::Char(_) => {}
        Pattern::Bind(name, _) => {
            bound.insert(name.clone());
        }
        Pattern::Ctor { args, .. } => {
            for a in args {
                collect_pattern_bindings(a, bound);
            }
        }
        Pattern::TypeTest(_, inner) => collect_pattern_bindings(inner, bound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(expr: Expr, ty: Type) -> Typed {
        Typed { loc: None, expr, ty }
    }

    /// `go`'s body references `offset` — neither its own parameter `k` nor
    /// a sibling name (it's the only def in this block) — so it's the
    /// block's one free variable.
    #[test]
    fn a_var_referencing_an_outer_scope_name_is_a_free_variable() {
        let go_body = vec![typed(
            Expr::Assoc {
                type_name: crate::Path::root("i64"),
                method: "+".to_string(),
                instance: true,
                args: vec![typed(Expr::Var("k".to_string()), Type::I64), typed(Expr::Var("offset".to_string()), Type::I64)],
                home: vec![],
            },
            Type::I64,
        )];
        let defs = vec![("go".to_string(), vec![("k".to_string(), Type::I64)], go_body)];
        assert_eq!(labels_free_vars(&defs, &HashSet::new(), &[]), vec![("offset".to_string(), Type::I64)]);
    }

    /// `f`'s own parameter `x` and its sibling `g` are both in scope, so
    /// neither counts as a free variable — only `shared` (from the
    /// enclosing scope) does, and it's collected once even though both
    /// `f` and `g` reference it.
    #[test]
    fn own_params_and_sibling_names_are_excluded_but_a_shared_outer_name_is_collected_once() {
        let f_body = vec![typed(
            Expr::Apply(
                Box::new(typed(Expr::Var("g".to_string()), Type::Fn(vec![Type::I64], None, Box::new(Type::I64)))),
                vec![typed(Expr::Var("x".to_string()), Type::I64)],
            ),
            Type::I64,
        )];
        let g_body = vec![typed(
            Expr::Assoc {
                type_name: crate::Path::root("i64"),
                method: "+".to_string(),
                instance: true,
                args: vec![typed(Expr::Var("x".to_string()), Type::I64), typed(Expr::Var("shared".to_string()), Type::I64)],
                home: vec![],
            },
            Type::I64,
        )];
        let defs = vec![
            ("f".to_string(), vec![("x".to_string(), Type::I64)], f_body),
            ("g".to_string(), vec![("x".to_string(), Type::I64)], g_body),
        ];
        assert_eq!(labels_free_vars(&defs, &HashSet::new(), &[]), vec![("shared".to_string(), Type::I64)]);
    }

    /// No def references anything outside its own parameters/siblings ->
    /// an empty list, matching Stage 1's no-capture behavior exactly.
    #[test]
    fn a_labels_block_with_no_outer_references_has_no_free_variables() {
        let g_body = vec![typed(Expr::Var("n".to_string()), Type::I64)];
        let f_body = vec![typed(
            Expr::Apply(
                Box::new(typed(Expr::Var("g".to_string()), Type::Fn(vec![Type::I64], None, Box::new(Type::I64)))),
                vec![typed(Expr::Var("x".to_string()), Type::I64)],
            ),
            Type::I64,
        )];
        let defs = vec![
            ("f".to_string(), vec![("x".to_string(), Type::I64)], f_body),
            ("g".to_string(), vec![("n".to_string(), Type::I64)], g_body),
        ];
        assert_eq!(labels_free_vars(&defs, &HashSet::new(), &[]), Vec::new());
    }

    /// `lambda_free_vars`: a single lambda's body referencing a name that's
    /// neither its own parameter is a free variable, same as `labels_free_vars`
    /// for a non-recursive, sibling-less def.
    #[test]
    fn a_lambda_referencing_an_outer_name_has_one_free_variable() {
        let body = vec![typed(
            Expr::Assoc {
                type_name: crate::Path::root("i64"),
                method: "+".to_string(),
                instance: true,
                args: vec![typed(Expr::Var("y".to_string()), Type::I64), typed(Expr::Var("x".to_string()), Type::I64)],
                home: vec![],
            },
            Type::I64,
        )];
        assert_eq!(lambda_free_vars(&[("y".to_string(), Type::I64)], &body), vec![("x".to_string(), Type::I64)]);
    }

    /// A lambda's own parameter is never a free variable.
    #[test]
    fn a_lambda_referencing_only_its_own_parameter_has_no_free_variables() {
        let body = vec![typed(Expr::Var("y".to_string()), Type::I64)];
        assert_eq!(lambda_free_vars(&[("y".to_string(), Type::I64)], &body), Vec::new());
    }

    /// A captured name whose static type is `Fn` is recorded with that type
    /// — the piece of information this whole module exists to carry now,
    /// for the automatic `ClosureBox` retain/release insertion work.
    #[test]
    fn a_captured_closure_typed_name_carries_its_fn_type() {
        let fn_ty = Type::Fn(vec![Type::I64], None, Box::new(Type::I64));
        let body = vec![typed(
            Expr::Apply(Box::new(typed(Expr::Var("f".to_string()), fn_ty.clone())), vec![typed(Expr::Int(1), Type::I64)]),
            Type::I64,
        )];
        assert_eq!(lambda_free_vars(&[], &body), vec![("f".to_string(), fn_ty)]);
    }

    /// `names_captured_by_nested`: an outer `n` referenced inside a nested
    /// `lambda` is flagged — the exact case that needs `n`'s own `defun`
    /// param slot cell-boxed (closure-representation unification, Stage 4).
    #[test]
    fn a_name_referenced_inside_a_nested_lambda_is_captured_by_nested() {
        let inner = vec![typed(
            Expr::Assoc {
                type_name: crate::Path::root("i64"),
                method: "+".to_string(),
                instance: true,
                args: vec![typed(Expr::Var("x".to_string()), Type::I64), typed(Expr::Var("n".to_string()), Type::I64)],
                home: vec![],
            },
            Type::I64,
        )];
        let body = vec![typed(
            Expr::Lambda { params: vec![("x".to_string(), Type::I64)], body: inner },
            Type::Fn(vec![Type::I64], None, Box::new(Type::I64)),
        )];
        let mut expected = HashSet::new();
        expected.insert("n".to_string());
        assert_eq!(names_captured_by_nested(&body), expected);
    }

    /// A name only ever referenced *outside* any nested closure is not
    /// flagged — an ordinary local stays an ordinary stack slot.
    #[test]
    fn a_name_never_referenced_inside_a_nested_closure_is_not_captured_by_nested() {
        let body = vec![typed(Expr::Var("n".to_string()), Type::I64)];
        assert_eq!(names_captured_by_nested(&body), HashSet::new());
    }

    /// Doubly-nested: `n` is referenced only inside a `lambda` nested inside
    /// *another* `lambda` — both `lambda_free_vars` calls (the inner one via
    /// `walk`'s own recursion into `Expr::Lambda`, transparently) surface it
    /// as free relative to the outermost nested lambda, so it's still found
    /// here without any special doubly-nested handling of its own.
    #[test]
    fn a_name_captured_by_a_doubly_nested_lambda_is_still_found() {
        let innermost = vec![typed(Expr::Var("n".to_string()), Type::I64)];
        let middle = vec![typed(
            Expr::Lambda { params: vec![("y".to_string(), Type::I64)], body: innermost },
            Type::Fn(vec![Type::I64], None, Box::new(Type::I64)),
        )];
        let body = vec![typed(
            Expr::Lambda { params: vec![("x".to_string(), Type::I64)], body: middle },
            Type::Fn(vec![Type::I64], None, Box::new(Type::I64)),
        )];
        let mut expected = HashSet::new();
        expected.insert("n".to_string());
        assert_eq!(names_captured_by_nested(&body), expected);
    }

    /// A `labels` block nested inside the body is treated the same as a
    /// `lambda` — its own free variables (via `labels_free_vars`) surface as
    /// captured-by-nested, and walking continues into its own trailing body
    /// afterward (unlike a `lambda`, whose entire body is opaque to this
    /// walk once `lambda_free_vars` has been called on it, a `labels`
    /// block's trailing body is *not* part of any def's own closure — only
    /// each def's own body is).
    #[test]
    fn a_labels_block_nested_in_the_body_contributes_its_own_free_variables() {
        let go_body = vec![typed(Expr::Var("n".to_string()), Type::I64)];
        let defs = vec![("go".to_string(), vec![], go_body)];
        let trailing = vec![typed(Expr::Var("m".to_string()), Type::I64)];
        let body = vec![typed(Expr::Labels { defs, body: trailing }, Type::I64)];
        let mut expected = HashSet::new();
        expected.insert("n".to_string());
        assert_eq!(names_captured_by_nested(&body), expected);
    }
}
