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

use std::collections::HashSet;

use crate::{Expr, LabelDef, Pattern, Typed};

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
pub fn lambda_free_vars(params: &[(String, crate::Type)], body: &[Typed]) -> Vec<String> {
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
pub fn labels_free_vars(defs: &[LabelDef], outer_direct: &HashSet<String>) -> Vec<String> {
    let mut siblings: HashSet<String> = outer_direct.clone();
    for (name, _, _) in defs {
        siblings.insert(name.clone());
    }
    let mut seen = HashSet::new();
    let mut order = Vec::new();
    for (_, params, body) in defs {
        let bound: HashSet<String> = params.iter().map(|(n, _)| n.clone()).collect();
        walk_body(body, &bound, &siblings, &mut seen, &mut order);
    }
    order
}

fn walk_body(body: &[Typed], bound: &HashSet<String>, siblings: &HashSet<String>, seen: &mut HashSet<String>, order: &mut Vec<String>) {
    for typed in body {
        walk(&typed.expr, bound, siblings, seen, order);
    }
}

fn note(name: &str, bound: &HashSet<String>, siblings: &HashSet<String>, seen: &mut HashSet<String>, order: &mut Vec<String>) {
    if bound.contains(name) || siblings.contains(name) {
        return;
    }
    if seen.insert(name.to_string()) {
        order.push(name.to_string());
    }
}

fn walk(expr: &Expr, bound: &HashSet<String>, siblings: &HashSet<String>, seen: &mut HashSet<String>, order: &mut Vec<String>) {
    match expr {
        Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Char(_) | Expr::Str(_) | Expr::Unit => {}
        Expr::Var(name) => note(name, bound, siblings, seen, order),
        Expr::Global(_) | Expr::FnRef(_) | Expr::MethodRef { .. } => {}
        Expr::If(cond, then, els) => {
            walk(&cond.expr, bound, siblings, seen, order);
            walk(&then.expr, bound, siblings, seen, order);
            walk(&els.expr, bound, siblings, seen, order);
        }
        Expr::Let(bindings, body) => {
            for (_, value) in bindings {
                walk(&value.expr, bound, siblings, seen, order);
            }
            let mut inner = bound.clone();
            for (name, _) in bindings {
                inner.insert(name.clone());
            }
            walk_body(body, &inner, siblings, seen, order);
        }
        Expr::Call(_, args) => {
            for a in args {
                walk(&a.expr, bound, siblings, seen, order);
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
            walk(&callee.expr, bound, siblings, seen, order);
            for a in args {
                walk(&a.expr, bound, siblings, seen, order);
            }
        }
        Expr::Assoc { args, .. } => {
            for a in args {
                walk(&a.expr, bound, siblings, seen, order);
            }
        }
        Expr::Construct { args, .. } => {
            for a in args {
                walk(&a.expr, bound, siblings, seen, order);
            }
        }
        Expr::FieldGet(inner, _) => walk(&inner.expr, bound, siblings, seen, order),
        Expr::FieldSet(inner, _, value) => {
            walk(&inner.expr, bound, siblings, seen, order);
            walk(&value.expr, bound, siblings, seen, order);
        }
        Expr::Match(scrutinee, arms) => {
            walk(&scrutinee.expr, bound, siblings, seen, order);
            for arm in arms {
                let mut inner = bound.clone();
                collect_pattern_bindings(&arm.pat, &mut inner);
                walk_body(&arm.body, &inner, siblings, seen, order);
            }
        }
        Expr::Set(name, value) => {
            note(name, bound, siblings, seen, order);
            walk(&value.expr, bound, siblings, seen, order);
        }
        Expr::SetGlobal(_, value) => walk(&value.expr, bound, siblings, seen, order),
        Expr::Loop(body) => walk_body(body, bound, siblings, seen, order),
        Expr::Break => {}
        Expr::Return(value) => {
            if let Some(v) = value {
                walk(&v.expr, bound, siblings, seen, order);
            }
        }
        Expr::Panic(msg) => walk(&msg.expr, bound, siblings, seen, order),
        Expr::Quote(_) => {}
    }
}

fn collect_pattern_bindings(pat: &Pattern, bound: &mut HashSet<String>) {
    match pat {
        Pattern::Wildcard | Pattern::Int(_) | Pattern::Bool(_) | Pattern::Char(_) => {}
        Pattern::Bind(name) => {
            bound.insert(name.clone());
        }
        Pattern::Ctor { args, .. } => {
            for a in args {
                collect_pattern_bindings(a, bound);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Type;

    fn typed(expr: Expr, ty: Type) -> Typed {
        Typed { expr, ty }
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
            },
            Type::I64,
        )];
        let defs = vec![("go".to_string(), vec![("k".to_string(), Type::I64)], go_body)];
        assert_eq!(labels_free_vars(&defs, &HashSet::new()), vec!["offset".to_string()]);
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
            },
            Type::I64,
        )];
        let defs = vec![
            ("f".to_string(), vec![("x".to_string(), Type::I64)], f_body),
            ("g".to_string(), vec![("x".to_string(), Type::I64)], g_body),
        ];
        assert_eq!(labels_free_vars(&defs, &HashSet::new()), vec!["shared".to_string()]);
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
        assert_eq!(labels_free_vars(&defs, &HashSet::new()), Vec::<String>::new());
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
            },
            Type::I64,
        )];
        assert_eq!(lambda_free_vars(&[("y".to_string(), Type::I64)], &body), vec!["x".to_string()]);
    }

    /// A lambda's own parameter is never a free variable.
    #[test]
    fn a_lambda_referencing_only_its_own_parameter_has_no_free_variables() {
        let body = vec![typed(Expr::Var("y".to_string()), Type::I64)];
        assert_eq!(lambda_free_vars(&[("y".to_string(), Type::I64)], &body), Vec::<String>::new());
    }
}
