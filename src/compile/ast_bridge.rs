//! Bridges the checker's typed AST ([`Typed`]/[`Expr`]) into a plain `Sexpr`
//! the (typelisp-hosted) compiler body can pattern-match on directly — the
//! same "give the compiler ordinary data, not a Rust API" approach
//! `prelude.rs` already uses for the standard library.
//!
//! Each node becomes a tagged list `(tag . fields...)`, e.g. `Expr::Int(42)`
//! -> `(int 42)`. Only the shapes a current compiler phase actually consumes
//! are translated for real; anything else becomes `(unsupported "<Variant>")`
//! so the compiler body can `panic` with a clear message instead of the
//! bridge silently doing the wrong thing — add a real translation here only
//! once a phase needs to compile that node.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use super::freevars::{labels_free_vars, lambda_free_vars};
use crate::{Error, Expr, Heap, LabelDef, Path, Type, Typed, Value};

/// A process-wide counter for synthesizing unique LLVM symbol names for
/// anonymous functions — every `lambda`/`Expr::FnRef`-forwarding-wrapper/
/// immediately-invoked-lambda gets one (labels/closures Stage 4), since none
/// of those have a name from user source the way a `labels` def or `defun`
/// does. Process-wide (not per-translation-pass or per-outer-function) so
/// two lambdas compiled into the same shared AOT module — even from two
/// *different* `defun`s — can never collide, with no mangling scheme to get
/// right.
static LAMBDA_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_lambda_name(prefix: &str) -> String {
    let n = LAMBDA_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}${}", prefix, n)
}

/// Conses a proper list from `items` (in order), rooting as it goes — the
/// same push/pop discipline `crate::eval::interp::alloc_quoted` uses for
/// `Expr::Quote`, just generalized to N items instead of a fixed two
/// (`car`/`cdr`). The building block both `tagged` (below) and any
/// untagged sub-list (e.g. a `labels` def's parameter-name list) use.
fn list_of(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    let mut acc = Value::Empty;
    for item in items.iter().rev() {
        heap.push_root(*item);
        heap.push_root(acc);
        let next = heap.cons(*item, acc);
        heap.pop_root();
        heap.pop_root();
        acc = next?;
    }
    Ok(acc)
}

/// Conses `tag` onto a list built from `items` — see [`list_of`].
fn tagged(heap: &mut Heap, tag: &str, items: &[Value]) -> Result<Value, Error> {
    let list = list_of(heap, items)?;
    let tag_sym = heap.intern_symbol(tag);
    heap.push_root(tag_sym);
    heap.push_root(list);
    let result = heap.cons(tag_sym, list);
    heap.pop_root();
    heap.pop_root();
    result
}

/// Builds a `Sexpr` list of `(name . is-fn)` pairs from typed
/// parameter/captured names — every such list needs this (the automatic
/// `ClosureBox` retain/release insertion work, a follow-up to
/// labels/closures Stage 4, needs to know per bound name whether it's safe
/// to call `build-closure-retain`/`build-closure-release` on it; see
/// `compiler.rs`'s `bind-params`/`bind-captures`).
fn tagged_sym_list(heap: &mut Heap, names: &[(String, Type)]) -> Result<Value, Error> {
    let mut acc = Value::Empty;
    for (n, ty) in names.iter().rev() {
        let sym = heap.intern_symbol(n);
        let is_fn = Value::Bool(matches!(ty, Type::Fn(..)));
        heap.push_root(sym);
        let pair = heap.cons(sym, is_fn);
        heap.pop_root();
        let pair = pair?;
        heap.push_root(pair);
        heap.push_root(acc);
        let next = heap.cons(pair, acc);
        heap.pop_root();
        heap.pop_root();
        acc = next?;
    }
    Ok(acc)
}

/// `(unsupported "<Variant>")` — see this module's doc comment.
fn unsupported(heap: &mut Heap, variant: &str) -> Result<Value, Error> {
    let name = heap.alloc_string(variant.to_string());
    tagged(heap, "unsupported", &[name])
}

/// Translates each of `items` in order, rooting every translated `Value` as
/// it goes (so an earlier sibling survives a later sibling's own `heap.cons`
/// calls — the same concern `tagged` has for its own `items`, just one level
/// up). On success every value in the returned `Vec` is left rooted; the
/// caller must pop exactly that many roots once it's done embedding them in
/// whatever it builds next (see `Expr::Assoc`'s arm below). On error, pops
/// everything pushed so far before propagating. `direct` is threaded through
/// unchanged — see [`ast_to_sexpr_scoped`]'s doc comment.
fn ast_list_to_sexpr(heap: &mut Heap, items: &[Typed], direct: &HashSet<String>) -> Result<Vec<Value>, Error> {
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        match ast_to_sexpr_scoped(heap, item, direct) {
            Ok(v) => {
                heap.push_root(v);
                values.push(v);
            }
            Err(e) => {
                for _ in 0..values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        }
    }
    Ok(values)
}

/// The call-argument counterpart of [`ast_list_to_sexpr`]: each translated
/// form is wrapped as `(is-fn . form)` rather than left bare — `apply`/
/// `apply-indirect`/`call`'s argument lists need this (unlike `Expr::Assoc`'s,
/// which stays untagged via [`ast_list_to_sexpr`]: arithmetic operands are
/// always `i64`, never `Fn`-typed, so `compile-assoc` never needs the tag).
/// `compile-call-args` reads `is-fn` to decide whether a given argument's
/// value needs the automatic `ClosureBox` retain/release treatment at all.
fn tagged_ast_list_to_sexpr(heap: &mut Heap, items: &[Typed], direct: &HashSet<String>) -> Result<Vec<Value>, Error> {
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        let form = match ast_to_sexpr_scoped(heap, item, direct) {
            Ok(v) => v,
            Err(e) => {
                for _ in 0..values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        };
        let is_fn = Value::Bool(matches!(item.ty, Type::Fn(..)));
        heap.push_root(form);
        let pair = heap.cons(is_fn, form);
        heap.pop_root();
        let pair = match pair {
            Ok(p) => p,
            Err(e) => {
                for _ in 0..values.len() {
                    heap.pop_root();
                }
                return Err(e);
            }
        };
        heap.push_root(pair);
        values.push(pair);
    }
    Ok(values)
}

/// Translates one typed AST node. See the module doc comment for the tagged
/// shape and which variants are real vs. `unsupported` placeholders today.
pub fn ast_to_sexpr(heap: &mut Heap, typed: &Typed) -> Result<Value, Error> {
    ast_to_sexpr_scoped(heap, typed, &HashSet::new())
}

/// `direct` is the set of names that resolve to a direct call rather than
/// (in a later phase) a boxed/indirect one: currently in-scope `labels`
/// siblings, including the def being compiled itself (self-recursion) —
/// see [`translate_apply`]. Empty at the top level (a `defun`'s own body
/// has no such names; they only come into existence inside a `labels`
/// form, see [`translate_labels`]).
fn ast_to_sexpr_scoped(heap: &mut Heap, typed: &Typed, direct: &HashSet<String>) -> Result<Value, Error> {
    match &typed.expr {
        Expr::Int(n) => tagged(heap, "int", &[Value::Int(*n)]),
        Expr::Float(f) => tagged(heap, "float", &[Value::Float(*f)]),
        Expr::Bool(b) => tagged(heap, "bool", &[Value::Bool(*b)]),
        Expr::Char(c) => tagged(heap, "char", &[Value::Char(*c)]),
        Expr::Str(s) => {
            let v = heap.alloc_string(s.clone());
            tagged(heap, "str", &[v])
        }
        Expr::Unit => tagged(heap, "unit", &[]),
        // Always `(var name)`, even when `name` is a currently in-scope
        // `labels` sibling/self (`direct.contains(name)`) — that only
        // matters to [`translate_apply`], which intercepts the *callee*
        // position of an `Expr::Apply` before a bare `Expr::Var` node for
        // that name would ever reach here. A sibling *referenced as a
        // value* rather than called (e.g. a nested `lambda` capturing it, or
        // a `labels` block's trailing body returning it bare) still becomes
        // this same plain `(var name)` — deliberately: the "is this an
        // ordinary value or a sibling that needs boxing into a `ClosureBox`"
        // judgment is made entirely at compile time instead, by
        // `compiler.rs`'s `resolve-value` (a follow-up to labels/closures
        // Stage 4) — see that function's doc comment for why doing it there
        // (where the relevant `fn-env` is still in scope) rather than here
        // avoids colliding with `compile-lambda`'s own, separate decision to
        // always give an escaping `lambda`'s body a *fresh* `fn-env`.
        Expr::Var(name) => {
            let v = heap.alloc_string(name.clone());
            let is_fn = Value::Bool(matches!(typed.ty, Type::Fn(..)));
            tagged(heap, "var", &[v, is_fn])
        }
        // `(assoc type-name method instance arg...)` — a fixed 3-field
        // header (both strings, then the receiver flag) followed by the
        // translated argument list. `compile_value` only matters about
        // `Expr::Var`'s `name`/`Expr::Assoc`'s `type_name`/`method` as
        // plain text, so they're translated as `Str`s like `Expr::Str`
        // (rather than e.g. interned symbols) — there's no reason for the
        // compiler body to treat them differently from any other string.
        Expr::Assoc { type_name, method, instance, args } => {
            let type_name_v = heap.alloc_string(type_name.to_string());
            heap.push_root(type_name_v);
            let method_v = heap.alloc_string(method.clone());
            heap.push_root(method_v);
            let arg_values = match ast_list_to_sexpr(heap, args, direct) {
                Ok(v) => v,
                Err(e) => {
                    heap.pop_root(); // method_v
                    heap.pop_root(); // type_name_v
                    return Err(e);
                }
            };
            let mut items = vec![type_name_v, method_v, Value::Bool(*instance)];
            items.extend(arg_values.iter().copied());
            let result = tagged(heap, "assoc", &items);
            for _ in 0..arg_values.len() {
                heap.pop_root();
            }
            heap.pop_root(); // method_v
            heap.pop_root(); // type_name_v
            result
        }
        // Everything below needs either multi-child rooting (a child's
        // translated `Value` must stay rooted while its siblings are
        // translated — `Expr::If`'s three children, `Expr::Let`'s bindings
        // plus body, ...) or a node shape this phase doesn't compile yet
        // (`Construct`/`FieldGet`/`FieldSet`/`Match`/`Loop`/...). Both are
        // deliberately deferred to the phase that first needs them, rather
        // than building out unrooted multi-child plumbing nothing exercises
        // yet — see the module doc comment.
        Expr::Global(_) => unsupported(heap, "Global"),
        Expr::FnRef(path) => translate_fnref(heap, path, &typed.ty),
        Expr::MethodRef { .. } => unsupported(heap, "MethodRef"),
        Expr::If(..) => unsupported(heap, "If"),
        Expr::Let(..) => unsupported(heap, "Let"),
        Expr::Call(path, args) => translate_call(heap, path, args, direct),
        Expr::Lambda { params, body } => translate_lambda(heap, params, body),
        Expr::Labels { defs, body } => translate_labels(heap, defs, body, direct),
        Expr::Apply(callee, args) => translate_apply(heap, callee, args, direct),
        Expr::Construct { .. } => unsupported(heap, "Construct"),
        Expr::FieldGet(..) => unsupported(heap, "FieldGet"),
        Expr::FieldSet(..) => unsupported(heap, "FieldSet"),
        Expr::Match(..) => unsupported(heap, "Match"),
        Expr::Set(..) => unsupported(heap, "Set"),
        Expr::SetGlobal(..) => unsupported(heap, "SetGlobal"),
        Expr::Loop(_) => unsupported(heap, "Loop"),
        Expr::Break => unsupported(heap, "Break"),
        Expr::Return(_) => unsupported(heap, "Return"),
        Expr::Panic(_) => unsupported(heap, "Panic"),
        Expr::Quote(_) => unsupported(heap, "Quote"),
    }
}

/// `Expr::Labels { defs, body }` -> `(labels (captured-sym...) ((name
/// (param-sym...) single-body-form)...) single-trailing-body-form)`.
///
/// `captured` (labels/closures Stage 2: outer-scope capture) is the whole
/// block's free-variable list (`freevars::labels_free_vars`) — empty when
/// no def references anything outside its own parameters/siblings, exactly
/// reproducing Stage 1's no-capture shape (just with an extra empty list
/// field). Every sibling shares this *one* list rather than each getting its
/// own narrower one — see `labels_free_vars`'s doc comment for why a shared
/// environment, not a per-sibling one, is what lets sibling-to-sibling calls
/// work without a second, transitive analysis pass.
///
/// A def's body may otherwise reference its own parameters, any sibling/self
/// name (resolved as a direct call when it's the callee of an `Apply`, see
/// [`translate_apply`], or boxed into a `ClosureBox` on demand when
/// referenced bare as a value, see `compiler.rs`'s `resolve-value`), or any
/// of `captured`'s names — only a genuinely nonexistent name is rejected at
/// compile time (`resolve-value`'s "unbound variable" panic).
/// Every def's body and the trailing body must be a single expression — the
/// same restriction `Interp::add_compiled_function` already applies to a
/// `defun`'s own body, just extended uniformly to `labels` rather than
/// lifted here.
fn translate_labels(heap: &mut Heap, defs: &[LabelDef], body: &[Typed], direct: &HashSet<String>) -> Result<Value, Error> {
    let mut siblings = direct.clone();
    for (name, _, _) in defs {
        siblings.insert(name.clone());
    }

    let captured_names = labels_free_vars(defs, direct);
    let captured_list = tagged_sym_list(heap, &captured_names)?;
    heap.push_root(captured_list);

    let mut def_values = Vec::with_capacity(defs.len());
    for (name, params, fbody) in defs {
        if fbody.len() != 1 {
            for _ in 0..def_values.len() {
                heap.pop_root();
            }
            heap.pop_root(); // captured_list
            return Err(Error::TypeError(format!(
                "compile: labels function \"{}\" has a multi-expression body, not yet supported",
                name
            )));
        }
        match translate_labels_def(heap, name, params, &fbody[0], &siblings) {
            Ok(v) => {
                heap.push_root(v);
                def_values.push(v);
            }
            Err(e) => {
                for _ in 0..def_values.len() {
                    heap.pop_root();
                }
                heap.pop_root(); // captured_list
                return Err(e);
            }
        }
    }
    let defs_list = match list_of(heap, &def_values) {
        Ok(v) => v,
        Err(e) => {
            for _ in 0..def_values.len() {
                heap.pop_root();
            }
            heap.pop_root(); // captured_list
            return Err(e);
        }
    };
    for _ in 0..def_values.len() {
        heap.pop_root();
    }
    heap.push_root(defs_list);

    if body.len() != 1 {
        heap.pop_root(); // defs_list
        heap.pop_root(); // captured_list
        return Err(Error::TypeError("compile: labels body has a multi-expression body, not yet supported".into()));
    }
    let body_v = match ast_to_sexpr_scoped(heap, &body[0], &siblings) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // defs_list
            heap.pop_root(); // captured_list
            return Err(e);
        }
    };
    heap.push_root(body_v);
    let result = tagged(heap, "labels", &[captured_list, defs_list, body_v]);
    heap.pop_root(); // body_v
    heap.pop_root(); // defs_list
    heap.pop_root(); // captured_list
    result
}

/// One `labels` def: `(name-str (param-sym...) single-body-form)` — an
/// untagged 3-element list (its fixed position within `labels`'s own
/// already-tagged shape makes a separate tag unnecessary).
fn translate_labels_def(heap: &mut Heap, name: &str, params: &[(String, crate::Type)], body: &Typed, siblings: &HashSet<String>) -> Result<Value, Error> {
    let name_v = heap.alloc_string(name.to_string());
    heap.push_root(name_v);
    let param_list = match tagged_sym_list(heap, params) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            return Err(e);
        }
    };
    heap.push_root(param_list);
    let body_v = match ast_to_sexpr_scoped(heap, body, siblings) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            heap.pop_root();
            return Err(e);
        }
    };
    heap.push_root(body_v);
    let result = list_of(heap, &[name_v, param_list, body_v]);
    heap.pop_root(); // body_v
    heap.pop_root(); // param_list
    heap.pop_root(); // name_v
    result
}

/// `Expr::Apply(callee, args)` dispatches on `callee`'s shape (labels/closures
/// Stage 4 extends this from the Stage 1 direct-only version):
/// - a `Var` naming a currently in-scope `labels` sibling/self ->
///   [`translate_direct_apply`] (unchanged since Stage 1).
/// - a `lambda` literal being called immediately (an IIFE) ->
///   [`translate_immediate_lambda_call`] — never escapes, so no `ClosureBox`.
/// - anything else (a captured closure variable, a higher-order function's
///   own parameter, ...) -> [`translate_indirect_apply`] — `callee`'s value
///   is evaluated and called through at runtime, since nothing here can
///   know ahead of time which `ClosureBox` it'll be.
fn translate_apply(heap: &mut Heap, callee: &Typed, args: &[Typed], direct: &HashSet<String>) -> Result<Value, Error> {
    match &callee.expr {
        Expr::Var(n) if direct.contains(n) => translate_direct_apply(heap, n, args, direct),
        Expr::Lambda { params, body } => translate_immediate_lambda_call(heap, params, body, args, direct),
        _ => translate_indirect_apply(heap, callee, args, direct),
    }
}

/// `(apply name arg...)` — a direct call to a `labels` sibling/self
/// (Stage 1 scope, unchanged).
fn translate_direct_apply(heap: &mut Heap, name: &str, args: &[Typed], direct: &HashSet<String>) -> Result<Value, Error> {
    let name_v = heap.alloc_string(name.to_string());
    heap.push_root(name_v);
    let arg_values = match tagged_ast_list_to_sexpr(heap, args, direct) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            return Err(e);
        }
    };
    let mut items = vec![name_v];
    items.extend(arg_values.iter().copied());
    let result = tagged(heap, "apply", &items);
    for _ in 0..arg_values.len() {
        heap.pop_root();
    }
    heap.pop_root(); // name_v
    result
}

/// `((lambda (params) body) args...)` — an immediately-invoked `lambda`
/// literal as the callee of its own `Apply` (labels/closures Stage 4): never
/// escapes, so it needs no `ClosureBox` at all. Translated as if it were a
/// single-def, non-recursive `labels` block instead (`(labels ()
/// ((name (params) body)) (name args...))`), reusing [`translate_labels`]'s
/// entire mechanism — declare-then-compile, captured-list sharing, direct-
/// call dispatch into the *outer* scope's own siblings, all for free —
/// rather than teaching `compiler.rs` a second way to build essentially the
/// same kind of LLVM function. Passing the *outer* `direct` set through
/// (rather than an empty one, unlike [`translate_lambda`]'s escaping case)
/// is deliberate and safe specifically because this lambda never escapes:
/// it's compiled within the very same `compile-labels`-style scope its call
/// site already has, so it can call outer `labels` siblings directly just
/// like another sibling could (the synthesized `labels` block is, after
/// all, nested at exactly the point the source `Apply` was). `name` is a
/// fresh, process-wide-unique local name ([`fresh_lambda_name`]) —
/// `translate_labels_def`/`compile-labels`'s own `outer_fn_name$inner_name`
/// mangling still guarantees no collision with anything else in the same
/// module even though this name never came from user source.
fn translate_immediate_lambda_call(
    heap: &mut Heap,
    params: &[(String, Type)],
    lambda_body: &[Typed],
    args: &[Typed],
    direct: &HashSet<String>,
) -> Result<Value, Error> {
    let name = fresh_lambda_name("__lambda");
    let dummy_ty = Type::Unit;
    let def: LabelDef = (name.clone(), params.to_vec(), lambda_body.to_vec());
    let call = Typed {
        expr: Expr::Apply(Box::new(Typed { expr: Expr::Var(name), ty: dummy_ty.clone() }), args.to_vec()),
        ty: dummy_ty,
    };
    translate_labels(heap, &[def], std::slice::from_ref(&call), direct)
}

/// `(apply-indirect callee-form arg...)` — the general indirect-dispatch
/// case (labels/closures Stage 4): `callee` is translated and (at
/// `compiler.rs`'s `compile-apply-indirect`) evaluated like any other value,
/// then called through `build-closure-apply` at runtime. A deliberately
/// distinct tag from `(apply name arg...)` — *not* the plan's originally
/// sketched `(apply (direct name) arg...)`/`(apply (indirect callee) arg...)`
/// unification — so Stage 1-3's already-shipped `(apply name arg...)` shape
/// (and `compiler.rs`'s/tests' existing assumptions about it) needs no
/// change at all.
fn translate_indirect_apply(heap: &mut Heap, callee: &Typed, args: &[Typed], direct: &HashSet<String>) -> Result<Value, Error> {
    let callee_v = ast_to_sexpr_scoped(heap, callee, direct)?;
    heap.push_root(callee_v);
    let arg_values = match tagged_ast_list_to_sexpr(heap, args, direct) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            return Err(e);
        }
    };
    let mut items = vec![callee_v];
    items.extend(arg_values.iter().copied());
    let result = tagged(heap, "apply-indirect", &items);
    for _ in 0..arg_values.len() {
        heap.pop_root();
    }
    heap.pop_root(); // callee_v
    result
}

/// Builds `(lambda name-str (captured-sym...) (param-sym...)
/// single-body-form)` — the tagged shape both a real escaping `Expr::Lambda`
/// ([`translate_lambda`]) and a synthesized `Expr::FnRef` forwarding wrapper
/// ([`translate_fnref`]) produce. `name` is the caller's responsibility to
/// make unique ([`fresh_lambda_name`]) — both source forms are anonymous at
/// the typelisp level, unlike a `labels` def. `body` must already be rooted
/// by the caller (the same convention [`tagged`]'s own `items` slice
/// elements rely on) and remains the caller's to pop afterward; this
/// function only roots/pops what it itself allocates.
fn build_lambda_tag(heap: &mut Heap, name: &str, captured: &[(String, Type)], params: &[(String, Type)], body: Value) -> Result<Value, Error> {
    let name_v = heap.alloc_string(name.to_string());
    heap.push_root(name_v);
    let captured_list = match tagged_sym_list(heap, captured) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // name_v
            return Err(e);
        }
    };
    heap.push_root(captured_list);
    let param_list = match tagged_sym_list(heap, params) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root(); // captured_list
            heap.pop_root(); // name_v
            return Err(e);
        }
    };
    heap.push_root(param_list);
    let result = tagged(heap, "lambda", &[name_v, captured_list, param_list, body]);
    heap.pop_root(); // param_list
    heap.pop_root(); // captured_list
    heap.pop_root(); // name_v
    result
}

/// `Expr::Lambda { params, body }` -> `(lambda name (captured...)
/// (param-sym...) single-body-form)` (labels/closures Stage 4) — a `lambda`
/// value that *escapes*: reached anywhere a `Expr::Lambda` is its own typed
/// node rather than the literal callee of its own enclosing `Apply`
/// ([`translate_immediate_lambda_call`] handles that case instead, via a
/// completely different, boxing-free translation). `compiler.rs`'s
/// `compile-lambda` (the consumer of this tag) therefore never has to decide
/// whether to box the result — every `lambda` tag it ever sees is, by
/// construction, the escaping case.
///
/// The lambda's own body is translated with an *empty* `direct` set,
/// matching [`lambda_free_vars`]'s own treatment of enclosing `labels`
/// siblings: a nested `lambda` never gets direct-call access to them (only
/// a `labels` def's own siblings do), so a sibling name referenced here
/// becomes an ordinary capture attempt instead — which now resolves
/// correctly (a follow-up to labels/closures Stage 4): `compiler.rs`'s
/// `compile-lambda` builds *this* lambda's own `ClosureBox` env array in the
/// *outer* scope (where the real `fn-env` is still available), so
/// `compile-env-args`/`resolve-value` box the sibling there, and the boxed
/// value flows into this lambda's own `env` via the ordinary `bind-captures`
/// path — no special-casing needed inside the lambda's own body at all.
fn translate_lambda(heap: &mut Heap, params: &[(String, Type)], body: &[Typed]) -> Result<Value, Error> {
    if body.len() != 1 {
        return Err(Error::TypeError("compile: lambda has a multi-expression body, not yet supported".into()));
    }
    let captured_names = lambda_free_vars(params, body);
    let body_v = ast_to_sexpr_scoped(heap, &body[0], &HashSet::new())?;
    heap.push_root(body_v);
    let result = build_lambda_tag(heap, &fresh_lambda_name("lambda"), &captured_names, params, body_v);
    heap.pop_root(); // body_v
    result
}

/// `Expr::FnRef(path)` -> a non-capturing `lambda` tag that just forwards
/// every argument straight through to the named top-level `defun`
/// (labels/closures Stage 4): `(lambda fnref$N () (arg0 arg1 ...) (call
/// path-local-name (var arg0) (var arg1) ...))`. Lets a top-level function
/// used as a first-class value (e.g. passed where a `(fn (i64) i64)` is
/// expected) reach the exact same `ClosureBox` machinery a real `lambda`
/// value does, rather than inventing a second runtime representation for
/// "function reference" values — `compile-lambda` builds this wrapper's
/// `ClosureBox` exactly like any other, `compile-call` (already built for
/// labels/closures Stage 3) handles the forwarding call inside it. Param
/// names are synthesized positionally from `ty`'s arity (the only place that
/// arity is available — an `Expr::FnRef` carries no parameter names of its
/// own, just a `Path`) — a variadic target's `&rest` parameter isn't
/// forwarded (out of scope; this only synthesizes `ty`'s fixed parameters).
fn translate_fnref(heap: &mut Heap, path: &Path, ty: &Type) -> Result<Value, Error> {
    let params: Vec<(String, Type)> = match ty {
        Type::Fn(params, ..) => params.iter().enumerate().map(|(i, t)| (format!("arg{}", i), t.clone())).collect(),
        _ => return unsupported(heap, "FnRef"),
    };

    let target_v = heap.alloc_string(path.local().to_string());
    heap.push_root(target_v);
    let mut var_values = Vec::with_capacity(params.len());
    for (n, t) in &params {
        let is_fn = matches!(t, Type::Fn(..));
        let s = heap.alloc_string(n.clone());
        heap.push_root(s);
        let v = match tagged(heap, "var", &[s, Value::Bool(is_fn)]) {
            Ok(v) => v,
            Err(e) => {
                heap.pop_root(); // s
                for _ in 0..var_values.len() {
                    heap.pop_root();
                }
                heap.pop_root(); // target_v
                return Err(e);
            }
        };
        heap.pop_root(); // s
        // wrap as `(is-fn . form)` — the same tagged shape every other
        // `apply`/`apply-indirect`/`call` argument list uses (see
        // `tagged_ast_list_to_sexpr`), built by hand here since these `var`
        // forms are synthesized from `ty`'s arity rather than translated
        // from real `Typed` argument nodes.
        heap.push_root(v);
        let pair = heap.cons(Value::Bool(is_fn), v);
        heap.pop_root(); // v
        let pair = match pair {
            Ok(p) => p,
            Err(e) => {
                for _ in 0..var_values.len() {
                    heap.pop_root();
                }
                heap.pop_root(); // target_v
                return Err(e);
            }
        };
        heap.push_root(pair);
        var_values.push(pair);
    }
    let mut call_items = vec![target_v];
    call_items.extend(var_values.iter().copied());
    let call_body = match tagged(heap, "call", &call_items) {
        Ok(v) => v,
        Err(e) => {
            for _ in 0..var_values.len() {
                heap.pop_root();
            }
            heap.pop_root(); // target_v
            return Err(e);
        }
    };
    for _ in 0..var_values.len() {
        heap.pop_root();
    }
    heap.pop_root(); // target_v
    heap.push_root(call_body);
    let result = build_lambda_tag(heap, &fresh_lambda_name("fnref"), &[], &params, call_body);
    heap.pop_root(); // call_body
    result
}

/// `Expr::Call(path, args)` -> `(call name arg...)` (labels/closures Stage
/// 3) — same shape as [`translate_apply`]'s output, but for a call to a
/// *top-level* `defun` rather than a `labels` sibling/self. Always
/// resolvable: `Checker::resolve_fn` only ever lets a `defun` call a name
/// that's already fully registered (itself included, for self-recursion —
/// see `Checker::check_defun`'s doc comment), so unlike [`translate_apply`]
/// there's no indirect/boxed case to fall back to `unsupported` for. Only
/// `path`'s local (final) segment is kept — `compiler.rs`'s `compile-call`
/// looks the callee up by that same plain name via `get-function` against
/// the destination module, matching how every compiled top-level function
/// is itself declared under its local name (`Interp::add_compiled_function`).
fn translate_call(heap: &mut Heap, path: &Path, args: &[Typed], direct: &HashSet<String>) -> Result<Value, Error> {
    let name_v = heap.alloc_string(path.local().to_string());
    heap.push_root(name_v);
    let arg_values = match tagged_ast_list_to_sexpr(heap, args, direct) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            return Err(e);
        }
    };
    let mut items = vec![name_v];
    items.extend(arg_values.iter().copied());
    let result = tagged(heap, "call", &items);
    for _ in 0..arg_values.len() {
        heap.pop_root();
    }
    heap.pop_root(); // name_v
    result
}

/// Collects every distinct top-level [`Path`] an `Expr::Call` reachable from
/// `typed` refers to, in first-encounter order. Mirrors exactly the node
/// shapes [`ast_to_sexpr_scoped`] actually recurses into — anything that
/// becomes `unsupported` there has no calls worth collecting, since
/// translating that node would fail before ever reaching them anyway.
///
/// JIT-only (labels/closures Stage 3): `Interp::compile_function` calls this
/// *before* running the typelisp compiler body, to learn which other
/// already-`compile`d top-level functions this body's `(call ...)`s need
/// forward-declared (no body) in its own throwaway module and wired to their
/// real address via `add_global_mapping` — `compile-call`'s `get-function`
/// would otherwise panic, not knowing about anything outside the module it
/// was handed. `compile::aot::compile_file` needs none of this: its one
/// shared module already has every earlier `defun`'s real body in it by the
/// time a later one's call needs to find it.
pub fn collect_call_targets(typed: &Typed) -> Vec<Path> {
    let mut out = Vec::new();
    collect_calls(typed, &mut out);
    out
}

fn collect_calls(typed: &Typed, out: &mut Vec<Path>) {
    match &typed.expr {
        Expr::Call(path, args) => {
            if !out.contains(path) {
                out.push(path.clone());
            }
            for a in args {
                collect_calls(a, out);
            }
        }
        // `translate_fnref` turns this into a forwarding `(call
        // path-local-name ...)` wrapper, so `path` needs the same
        // pre-declaration treatment as a real `Expr::Call` would.
        Expr::FnRef(path) => {
            if !out.contains(path) {
                out.push(path.clone());
            }
        }
        Expr::Assoc { args, .. } => {
            for a in args {
                collect_calls(a, out);
            }
        }
        Expr::Apply(callee, args) => {
            collect_calls(callee, out);
            for a in args {
                collect_calls(a, out);
            }
        }
        // Covers both an escaping `lambda` value's own body and an
        // immediately-invoked lambda literal's body (the latter reached via
        // the `Apply` arm above recursing into its `callee`, which is this
        // same `Expr::Lambda` node) — either way, a `Call` inside it still
        // needs the same JIT-only pre-declaration treatment.
        Expr::Lambda { body, .. } => {
            for e in body {
                collect_calls(e, out);
            }
        }
        Expr::Labels { defs, body } => {
            for (_, _, def_body) in defs {
                for e in def_body {
                    collect_calls(e, out);
                }
            }
            for e in body {
                collect_calls(e, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Type;

    fn typed(expr: Expr, ty: Type) -> Typed {
        Typed { expr, ty }
    }

    /// Unpacks a tagged-list `Value` into (tag name, field values), asserting
    /// it's a proper list (every `cdr` until the final `Empty` is itself a
    /// `Cons`, matching `tagged`'s own construction).
    fn untag(heap: &Heap, v: Value) -> (String, Vec<Value>) {
        let sym = heap.car(v).expect("tagged list has a car");
        let tag = match sym {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            other => panic!("expected a tag symbol, got {:?}", other),
        };
        let mut fields = Vec::new();
        let mut rest = heap.cdr(v).expect("tagged list has a cdr");
        while !rest.is_empty() {
            fields.push(heap.car(rest).expect("list field has a car"));
            rest = heap.cdr(rest).expect("list field has a cdr");
        }
        (tag, fields)
    }

    /// Unwraps a tagged call-argument pair `(is-fn . form)` — see
    /// `tagged_ast_list_to_sexpr`.
    fn untag_arg(heap: &Heap, pair: Value) -> (bool, Value) {
        let is_fn = match heap.car(pair).expect("arg pair has a car") {
            Value::Bool(b) => b,
            other => panic!("expected a Bool is-fn tag, got {:?}", other),
        };
        let form = heap.cdr(pair).expect("arg pair has a cdr");
        (is_fn, form)
    }

    /// Unwraps a tagged name pair `(name . is-fn)` — see `tagged_sym_list`.
    fn untag_name(heap: &Heap, pair: Value) -> (String, bool) {
        let name = match heap.car(pair).expect("name pair has a car") {
            Value::Symbol(id) => heap.symbol_name(id).to_string(),
            other => panic!("expected a Sym, got {:?}", other),
        };
        let is_fn = match heap.cdr(pair).expect("name pair has a cdr") {
            Value::Bool(b) => b,
            other => panic!("expected a Bool is-fn tag, got {:?}", other),
        };
        (name, is_fn)
    }

    #[test]
    fn translates_an_int_literal() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Int(42), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "int");
        assert_eq!(fields, vec![Value::Int(42)]);
    }

    #[test]
    fn translates_a_bool_literal() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Bool(true), Type::Bool)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "bool");
        assert_eq!(fields, vec![Value::Bool(true)]);
    }

    #[test]
    fn translates_a_str_literal() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Str("hi".to_string()), Type::Str)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "str");
        match fields[0] {
            Value::Str(id) => assert_eq!(heap.string(id), "hi"),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    #[test]
    fn translates_a_var_reference() {
        let mut heap = Heap::with_capacity(1 << 10);
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Var("a".to_string()), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "var");
        match fields[0] {
            Value::Str(id) => assert_eq!(heap.string(id), "a"),
            other => panic!("expected a Str, got {:?}", other),
        }
        assert_eq!(fields[1], Value::Bool(false), "an I64-typed var is never Fn-typed");
    }

    #[test]
    fn translates_an_instance_method_call() {
        let mut heap = Heap::with_capacity(1 << 10);
        let a = typed(Expr::Var("a".to_string()), Type::I64);
        let b = typed(Expr::Var("b".to_string()), Type::I64);
        let assoc = Expr::Assoc {
            type_name: crate::Path::root("i64"),
            method: "+".to_string(),
            instance: true,
            args: vec![a, b],
        };
        let v = ast_to_sexpr(&mut heap, &typed(assoc, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "assoc");
        match fields[0] {
            Value::Str(id) => assert_eq!(heap.string(id), "i64"),
            other => panic!("expected a Str, got {:?}", other),
        }
        match fields[1] {
            Value::Str(id) => assert_eq!(heap.string(id), "+"),
            other => panic!("expected a Str, got {:?}", other),
        }
        assert_eq!(fields[2], Value::Bool(true));
        let (arg0_tag, arg0_fields) = untag(&heap, fields[3]);
        assert_eq!(arg0_tag, "var");
        match arg0_fields[0] {
            Value::Str(id) => assert_eq!(heap.string(id), "a"),
            other => panic!("expected a Str, got {:?}", other),
        }
        let (arg1_tag, _) = untag(&heap, fields[4]);
        assert_eq!(arg1_tag, "var");
    }

    #[test]
    fn an_unimplemented_node_becomes_an_explicit_unsupported_tag() {
        let mut heap = Heap::with_capacity(1 << 10);
        let cond = Box::new(typed(Expr::Bool(true), Type::Bool));
        let then = Box::new(typed(Expr::Int(1), Type::I64));
        let els = Box::new(typed(Expr::Int(2), Type::I64));
        let v = ast_to_sexpr(&mut heap, &typed(Expr::If(cond, then, els), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "unsupported");
        match fields[0] {
            Value::Str(id) => assert_eq!(heap.string(id), "If"),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    fn fn_ty() -> Type {
        Type::Fn(vec![Type::I64], None, Box::new(Type::I64))
    }

    fn expect_str(heap: &Heap, v: Value) -> String {
        match v {
            Value::Str(id) => heap.string(id).to_string(),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    /// Walks a proper `Sexpr` list into a `Vec` of its elements (`car`/`cdr`
    /// until `Empty`), the same loop shape several tests below repeat.
    fn list_elems(heap: &Heap, mut list: Value) -> Vec<Value> {
        let mut elems = Vec::new();
        while !list.is_empty() {
            elems.push(heap.car(list).unwrap());
            list = heap.cdr(list).unwrap();
        }
        elems
    }

    /// `Expr::Labels { defs: [(f, .. (g x)), (g, .. x)], body: (f 5) }` —
    /// `f` calls its sibling `g` directly, and the trailing body calls `f`
    /// directly too. Confirms both the `(labels (captured...) ((name
    /// (params) body)...) trailing-body)` shape (an empty captured list,
    /// since neither def references anything outside its own
    /// parameters/siblings — labels/closures Stage 1 scope) and that
    /// sibling/self calls become `(apply name arg...)`, not `(unsupported
    /// "Apply")`.
    #[test]
    fn translates_a_labels_form_with_a_sibling_call() {
        let mut heap = Heap::with_capacity(1 << 10);
        let g_body = vec![typed(Expr::Var("x".to_string()), Type::I64)];
        let f_body = vec![typed(
            Expr::Apply(
                Box::new(typed(Expr::Var("g".to_string()), fn_ty())),
                vec![typed(Expr::Var("x".to_string()), Type::I64)],
            ),
            Type::I64,
        )];
        let defs = vec![
            ("f".to_string(), vec![("x".to_string(), Type::I64)], f_body),
            ("g".to_string(), vec![("x".to_string(), Type::I64)], g_body),
        ];
        let body = vec![typed(
            Expr::Apply(Box::new(typed(Expr::Var("f".to_string()), fn_ty())), vec![typed(Expr::Int(5), Type::I64)]),
            Type::I64,
        )];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Labels { defs, body }, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "labels");
        assert!(fields[0].is_empty(), "expected an empty captured list, got {:?}", fields[0]);

        let defs_seen = list_elems(&heap, fields[1]);
        assert_eq!(defs_seen.len(), 2);

        let f_def = defs_seen[0];
        assert_eq!(expect_str(&heap, heap.car(f_def).unwrap()), "f");
        let f_rest = heap.cdr(f_def).unwrap();
        let f_params = heap.car(f_rest).unwrap();
        let (param0_name, _) = untag_name(&heap, heap.car(f_params).unwrap());
        assert_eq!(param0_name, "x");
        let f_body_v = heap.car(heap.cdr(f_rest).unwrap()).unwrap();
        let (f_body_tag, f_body_fields) = untag(&heap, f_body_v);
        assert_eq!(f_body_tag, "apply");
        assert_eq!(expect_str(&heap, f_body_fields[0]), "g");

        let (body_tag, body_fields) = untag(&heap, fields[2]);
        assert_eq!(body_tag, "apply");
        assert_eq!(expect_str(&heap, body_fields[0]), "f");
    }

    /// labels/closures Stage 2: `go`'s body references `offset`, an outer
    /// (enclosing-`defun`) name that's neither its own parameter nor a
    /// sibling — confirms the captured list is non-empty and carries that
    /// name, as a `Sym` (matching the parameter list's own convention).
    #[test]
    fn translates_a_labels_form_that_captures_an_outer_scope_name() {
        let mut heap = Heap::with_capacity(1 << 10);
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
        let body = vec![typed(
            Expr::Apply(Box::new(typed(Expr::Var("go".to_string()), fn_ty())), vec![typed(Expr::Int(1), Type::I64)]),
            Type::I64,
        )];
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Labels { defs, body }, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "labels");

        let captured = list_elems(&heap, fields[0]);
        assert_eq!(captured.len(), 1);
        let (name, is_fn) = untag_name(&heap, captured[0]);
        assert_eq!(name, "offset");
        assert!(!is_fn);
    }

    /// A call to a function value that *isn't* a currently in-scope `labels`
    /// sibling/self (here: no enclosing `labels` at all) has no direct-call
    /// target to resolve to — labels/closures Stage 4 makes this a real
    /// `apply-indirect` translation (`callee`'s value, evaluated and called
    /// through `build-closure-apply` at runtime) rather than `unsupported`.
    #[test]
    fn an_apply_to_a_name_outside_the_current_labels_scope_is_indirect() {
        let mut heap = Heap::with_capacity(1 << 10);
        let callee = typed(Expr::Var("not-a-sibling".to_string()), fn_ty());
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Apply(Box::new(callee), vec![typed(Expr::Int(1), Type::I64)]), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "apply-indirect");
        let (callee_tag, callee_fields) = untag(&heap, fields[0]);
        assert_eq!(callee_tag, "var");
        assert_eq!(expect_str(&heap, callee_fields[0]), "not-a-sibling");
        let (is_fn, arg_form) = untag_arg(&heap, fields[1]);
        assert!(!is_fn);
        let (arg_tag, _) = untag(&heap, arg_form);
        assert_eq!(arg_tag, "int");
    }

    /// labels/closures Stage 3: a top-level `Expr::Call` becomes `(call name
    /// arg...)` — always a real translation, never `unsupported`, since
    /// `Checker::resolve_fn` guarantees the callee is already a registered
    /// `defun` (see `translate_call`'s doc comment).
    #[test]
    fn translates_a_call_to_another_top_level_function() {
        let mut heap = Heap::with_capacity(1 << 10);
        let call = Expr::Call(crate::Path::root("square"), vec![typed(Expr::Var("a".to_string()), Type::I64)]);
        let v = ast_to_sexpr(&mut heap, &typed(call, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "call");
        assert_eq!(expect_str(&heap, fields[0]), "square");
        let (is_fn, arg_form) = untag_arg(&heap, fields[1]);
        assert!(!is_fn);
        let (arg_tag, arg_fields) = untag(&heap, arg_form);
        assert_eq!(arg_tag, "var");
        assert_eq!(expect_str(&heap, arg_fields[0]), "a");
    }

    /// Only the local segment of a qualified `Path` survives translation —
    /// `compiler.rs`'s `compile-call` looks callees up by plain name, the
    /// same way every compiled top-level function is itself declared (see
    /// `translate_call`'s doc comment).
    #[test]
    fn translates_a_call_keeping_only_the_paths_local_segment() {
        let mut heap = Heap::with_capacity(1 << 10);
        let call = Expr::Call(crate::Path::of(&["geo", "distance"]), vec![]);
        let v = ast_to_sexpr(&mut heap, &typed(call, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "call");
        assert_eq!(expect_str(&heap, fields[0]), "distance");
    }

    #[test]
    fn collect_call_targets_is_empty_for_a_body_with_no_calls() {
        let v = typed(Expr::Int(1), Type::I64);
        assert!(collect_call_targets(&v).is_empty());
    }

    #[test]
    fn collect_call_targets_finds_a_direct_top_level_call() {
        let v = typed(Expr::Call(crate::Path::root("g"), vec![typed(Expr::Int(1), Type::I64)]), Type::I64);
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("g")]);
    }

    /// `collect_call_targets` recurses into a `labels` block's def bodies and
    /// trailing body too — a top-level `Expr::Call` can appear nested inside
    /// either (e.g. a `labels` sibling itself calling out to another
    /// top-level `defun`), and `Interp::compile_function` needs every one of
    /// them pre-declared, not just calls sitting directly in the outer body.
    #[test]
    fn collect_call_targets_finds_a_call_nested_inside_a_labels_def_body() {
        let inner = typed(Expr::Call(crate::Path::root("helper"), vec![typed(Expr::Int(1), Type::I64)]), Type::I64);
        let defs = vec![("f".to_string(), vec![], vec![inner])];
        let body = vec![typed(
            Expr::Apply(Box::new(typed(Expr::Var("f".to_string()), fn_ty())), vec![]),
            Type::I64,
        )];
        let v = typed(Expr::Labels { defs, body }, Type::I64);
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("helper")]);
    }

    #[test]
    fn collect_call_targets_deduplicates_repeated_calls_to_the_same_target() {
        let v = typed(
            Expr::Assoc {
                type_name: crate::Path::root("i64"),
                method: "+".to_string(),
                instance: true,
                args: vec![
                    typed(Expr::Call(crate::Path::root("g"), vec![]), Type::I64),
                    typed(Expr::Call(crate::Path::root("g"), vec![]), Type::I64),
                ],
            },
            Type::I64,
        );
        assert_eq!(collect_call_targets(&v), vec![crate::Path::root("g")]);
    }

    /// labels/closures Stage 4: a standalone (escaping) `Expr::Lambda` ->
    /// `(lambda name (captured...) (param-sym...) single-body-form)`, with
    /// an empty captured list when the body only references its own
    /// parameter.
    #[test]
    fn translates_an_escaping_lambda() {
        let mut heap = Heap::with_capacity(1 << 10);
        let lambda = Expr::Lambda {
            params: vec![("y".to_string(), Type::I64)],
            body: vec![typed(Expr::Var("y".to_string()), Type::I64)],
        };
        let v = ast_to_sexpr(&mut heap, &typed(lambda, fn_ty())).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "lambda");
        assert!(!expect_str(&heap, fields[0]).is_empty(), "expected a non-empty synthesized name");
        assert!(fields[1].is_empty(), "expected an empty captured list, got {:?}", fields[1]);
        let params = list_elems(&heap, fields[2]);
        assert_eq!(params.len(), 1);
        let (param0_name, is_fn) = untag_name(&heap, params[0]);
        assert_eq!(param0_name, "y");
        assert!(!is_fn);
        let (body_tag, body_fields) = untag(&heap, fields[3]);
        assert_eq!(body_tag, "var");
        assert_eq!(expect_str(&heap, body_fields[0]), "y");
    }

    /// Two separately-translated lambdas get distinct synthesized names —
    /// `fresh_lambda_name`'s whole reason for existing (no two anonymous
    /// functions can collide once compiled into the same shared module).
    #[test]
    fn two_escaping_lambdas_get_distinct_synthesized_names() {
        let mut heap = Heap::with_capacity(1 << 10);
        let make = || Expr::Lambda { params: vec![], body: vec![typed(Expr::Int(1), Type::I64)] };
        let v1 = ast_to_sexpr(&mut heap, &typed(make(), fn_ty())).unwrap();
        let v2 = ast_to_sexpr(&mut heap, &typed(make(), fn_ty())).unwrap();
        let (_, f1) = untag(&heap, v1);
        let (_, f2) = untag(&heap, v2);
        assert_ne!(expect_str(&heap, f1[0]), expect_str(&heap, f2[0]));
    }

    /// labels/closures Stage 4: a `lambda` value that captures an outer name
    /// has it in the captured list, the same as a `labels` block would.
    #[test]
    fn translates_a_lambda_capturing_an_outer_name() {
        let mut heap = Heap::with_capacity(1 << 10);
        let lambda = Expr::Lambda {
            params: vec![("y".to_string(), Type::I64)],
            body: vec![typed(
                Expr::Assoc {
                    type_name: crate::Path::root("i64"),
                    method: "+".to_string(),
                    instance: true,
                    args: vec![typed(Expr::Var("y".to_string()), Type::I64), typed(Expr::Var("x".to_string()), Type::I64)],
                },
                Type::I64,
            )],
        };
        let v = ast_to_sexpr(&mut heap, &typed(lambda, fn_ty())).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "lambda");
        let captured = list_elems(&heap, fields[1]);
        assert_eq!(captured.len(), 1);
        let (name, is_fn) = untag_name(&heap, captured[0]);
        assert_eq!(name, "x");
        assert!(!is_fn);
    }

    /// labels/closures Stage 4: `((lambda (params) body) args...)` becomes a
    /// single-def, non-recursive `labels` block — not a `lambda` tag at
    /// all — proving the immediate-call path never boxes the function.
    #[test]
    fn translates_an_immediately_invoked_lambda_as_a_single_def_labels_block() {
        let mut heap = Heap::with_capacity(1 << 10);
        let lambda = typed(
            Expr::Lambda { params: vec![("y".to_string(), Type::I64)], body: vec![typed(Expr::Var("y".to_string()), Type::I64)] },
            fn_ty(),
        );
        let apply = Expr::Apply(Box::new(lambda), vec![typed(Expr::Int(5), Type::I64)]);
        let v = ast_to_sexpr(&mut heap, &typed(apply, Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "labels");
        assert!(fields[0].is_empty(), "expected an empty captured list, got {:?}", fields[0]);
        let defs_seen = list_elems(&heap, fields[1]);
        assert_eq!(defs_seen.len(), 1);
        let (body_tag, body_fields) = untag(&heap, fields[2]);
        assert_eq!(body_tag, "apply");
        let def_name = expect_str(&heap, heap.car(defs_seen[0]).unwrap());
        assert_eq!(expect_str(&heap, body_fields[0]), def_name);
    }

    /// labels/closures Stage 4: `Expr::FnRef(path)` becomes a non-capturing
    /// `lambda` that forwards positionally-synthesized arguments straight
    /// through to a `(call path-local-name ...)`.
    #[test]
    fn translates_an_fnref_as_a_forwarding_lambda() {
        let mut heap = Heap::with_capacity(1 << 10);
        let ty = Type::Fn(vec![Type::I64, Type::I64], None, Box::new(Type::I64));
        let v = ast_to_sexpr(&mut heap, &typed(Expr::FnRef(crate::Path::root("add2")), ty)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "lambda");
        assert!(fields[1].is_empty(), "expected an empty captured list, got {:?}", fields[1]);
        let params = list_elems(&heap, fields[2]);
        assert_eq!(params.len(), 2);
        let (body_tag, body_fields) = untag(&heap, fields[3]);
        assert_eq!(body_tag, "call");
        assert_eq!(expect_str(&heap, body_fields[0]), "add2");
        assert_eq!(body_fields.len() - 1, 2, "expected one forwarded arg per parameter");
        for (i, param) in params.iter().enumerate() {
            let (param_name, _) = untag_name(&heap, *param);
            let (_, arg_form) = untag_arg(&heap, body_fields[1 + i]);
            let (arg_tag, arg_fields) = untag(&heap, arg_form);
            assert_eq!(arg_tag, "var");
            assert_eq!(expect_str(&heap, arg_fields[0]), param_name);
        }
    }
}
