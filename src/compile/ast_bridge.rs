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

use crate::{Error, Expr, Heap, LabelDef, Typed, Value};

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

/// Builds a `Sexpr` symbol list from plain parameter names — the same
/// shape `Interp::add_compiled_function` builds for a `defun`'s own
/// parameter list (consumed by `compiler.rs`'s `bind-params`), just
/// reusable here for a `labels` def's parameters too.
fn sym_list(heap: &mut Heap, names: &[String]) -> Result<Value, Error> {
    let mut acc = Value::Empty;
    for n in names.iter().rev() {
        let sym = heap.intern_symbol(n);
        heap.push_root(sym);
        heap.push_root(acc);
        let next = heap.cons(sym, acc);
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
        Expr::Var(name) => {
            let v = heap.alloc_string(name.clone());
            tagged(heap, "var", &[v])
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
        Expr::FnRef(_) => unsupported(heap, "FnRef"),
        Expr::MethodRef { .. } => unsupported(heap, "MethodRef"),
        Expr::If(..) => unsupported(heap, "If"),
        Expr::Let(..) => unsupported(heap, "Let"),
        Expr::Call(..) => unsupported(heap, "Call"),
        Expr::Lambda { .. } => unsupported(heap, "Lambda"),
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

/// `Expr::Labels { defs, body }` -> `(labels ((name (param-sym...)
/// single-body-form)...) single-trailing-body-form)`.
///
/// Stage 1 scope (labels compilation work — see `compiler.rs`'s doc
/// comment): no outer-scope capture. A def's body may reference its own
/// parameters and any sibling/self name (resolved as a direct call, see
/// [`translate_apply`]) — anything else is a name the compiler body itself
/// will reject at compile time (`compile-var`'s "unbound variable" panic),
/// not checked here; real free-variable analysis (`src/compile/freevars.rs`)
/// is a later phase. Every def's body and the trailing body must be a
/// single expression — the same restriction `Interp::add_compiled_function`
/// already applies to a `defun`'s own body, just extended uniformly to
/// `labels` rather than lifted here.
fn translate_labels(heap: &mut Heap, defs: &[LabelDef], body: &[Typed], direct: &HashSet<String>) -> Result<Value, Error> {
    let mut siblings = direct.clone();
    for (name, _, _) in defs {
        siblings.insert(name.clone());
    }

    let mut def_values = Vec::with_capacity(defs.len());
    for (name, params, fbody) in defs {
        if fbody.len() != 1 {
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
            return Err(e);
        }
    };
    for _ in 0..def_values.len() {
        heap.pop_root();
    }
    heap.push_root(defs_list);

    if body.len() != 1 {
        heap.pop_root();
        return Err(Error::TypeError("compile: labels body has a multi-expression body, not yet supported".into()));
    }
    let body_v = match ast_to_sexpr_scoped(heap, &body[0], &siblings) {
        Ok(v) => v,
        Err(e) => {
            heap.pop_root();
            return Err(e);
        }
    };
    heap.push_root(body_v);
    let result = tagged(heap, "labels", &[defs_list, body_v]);
    heap.pop_root(); // body_v
    heap.pop_root(); // defs_list
    result
}

/// One `labels` def: `(name-str (param-sym...) single-body-form)` — an
/// untagged 3-element list (its fixed position within `labels`'s own
/// already-tagged shape makes a separate tag unnecessary).
fn translate_labels_def(heap: &mut Heap, name: &str, params: &[(String, crate::Type)], body: &Typed, siblings: &HashSet<String>) -> Result<Value, Error> {
    let name_v = heap.alloc_string(name.to_string());
    heap.push_root(name_v);
    let param_names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
    let param_list = match sym_list(heap, &param_names) {
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

/// `Expr::Apply(callee, args)` -> `(apply name arg...)` when `callee` is a
/// `Var` naming a currently in-scope `labels` sibling/self (a direct call).
/// Anything else is a callee that would need indirect/boxed dispatch — a
/// `ClosureBox`/runtime-shim mechanism a later phase builds — so it falls
/// back to `unsupported("Apply")` rather than guessing at a shape nothing
/// consumes yet.
fn translate_apply(heap: &mut Heap, callee: &Typed, args: &[Typed], direct: &HashSet<String>) -> Result<Value, Error> {
    let name = match &callee.expr {
        Expr::Var(n) if direct.contains(n) => n.clone(),
        _ => return unsupported(heap, "Apply"),
    };
    let name_v = heap.alloc_string(name);
    heap.push_root(name_v);
    let arg_values = match ast_list_to_sexpr(heap, args, direct) {
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

    /// `Expr::Labels { defs: [(f, .. (g x)), (g, .. x)], body: (f 5) }` —
    /// `f` calls its sibling `g` directly, and the trailing body calls `f`
    /// directly too. Confirms both the `(labels ((name (params) body)...)
    /// trailing-body)` shape and that sibling/self calls become `(apply
    /// name arg...)`, not `(unsupported "Apply")`.
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

        let mut def_list = fields[0];
        let mut defs_seen = Vec::new();
        while !def_list.is_empty() {
            defs_seen.push(heap.car(def_list).unwrap());
            def_list = heap.cdr(def_list).unwrap();
        }
        assert_eq!(defs_seen.len(), 2);

        let f_def = defs_seen[0];
        assert_eq!(expect_str(&heap, heap.car(f_def).unwrap()), "f");
        let f_rest = heap.cdr(f_def).unwrap();
        let f_params = heap.car(f_rest).unwrap();
        match heap.car(f_params).unwrap() {
            Value::Symbol(id) => assert_eq!(heap.symbol_name(id), "x"),
            other => panic!("expected a Sym, got {:?}", other),
        }
        let f_body_v = heap.car(heap.cdr(f_rest).unwrap()).unwrap();
        let (f_body_tag, f_body_fields) = untag(&heap, f_body_v);
        assert_eq!(f_body_tag, "apply");
        assert_eq!(expect_str(&heap, f_body_fields[0]), "g");

        let (body_tag, body_fields) = untag(&heap, fields[1]);
        assert_eq!(body_tag, "apply");
        assert_eq!(expect_str(&heap, body_fields[0]), "f");
    }

    /// A call to a function value that *isn't* a currently in-scope
    /// `labels` sibling/self (here: no enclosing `labels` at all) has no
    /// direct-call target to resolve to — indirect/boxed dispatch is a
    /// later phase's work, so this stays an explicit `unsupported`, not a
    /// best-effort guess.
    #[test]
    fn an_apply_to_a_name_outside_the_current_labels_scope_is_unsupported() {
        let mut heap = Heap::with_capacity(1 << 10);
        let callee = typed(Expr::Var("not-a-sibling".to_string()), fn_ty());
        let v = ast_to_sexpr(&mut heap, &typed(Expr::Apply(Box::new(callee), vec![typed(Expr::Int(1), Type::I64)]), Type::I64)).unwrap();
        let (tag, fields) = untag(&heap, v);
        assert_eq!(tag, "unsupported");
        assert_eq!(expect_str(&heap, fields[0]), "Apply");
    }
}
