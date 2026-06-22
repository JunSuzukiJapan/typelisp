//! Bridges the checker's typed AST (`check::ast::Expr`/`Typed`) into typelisp
//! data (the `AstExpr` builtin ADT, see `check::registry::ast_expr_def`) so
//! the typelisp-written compiler (`compile::compiler_source`) can pattern-match
//! over a function's body with the language's own `match`.
//!
//! Phase 1/2/3 ([docs/TODO.md](../../docs/TODO.md)「ステップ5」) only bridges
//! the narrow `i64`/`bool`/`f64`/`char`/`Sexpr`-scalar subset needed to
//! compile a function like `(defun max2 ((a i64) (b i64)) i64 (if (< a b) b
//! a))` or `(defun gt ((a i64) (b i64)) bool (> a b))`: integer/bool/float/
//! char literals, the `Nil` `Sexpr` literal, parameter references, `if`,
//! `let`, calls to other compiled functions, `i64`'s/`f64`'s binary
//! arithmetic/comparison instance methods, `char`'s `eq`/`lt`, and
//! `cons`/`car`/`cdr`/`null` (Phase 3 — a `Sexpr` value is only bridged at
//! all if it's `Nil` or `Cons`; see `crate::eval::interp::value_to_ptr`'s
//! doc comment for what happens if a *runtime* value falls outside that).
//! Anything else returns `None` — `compile` then refuses the function
//! outright rather than miscompiling it (see `Interp::builtin_ast_body` in
//! `src/eval/interp.rs`).

use std::cell::RefCell;
use std::rc::Rc;

use crate::{Expr, Interp, Path, QuotedSexpr, RtValue, Type, Typed};

/// Variant indices of `AstExpr`'s constructors — must match
/// `check::registry::ast_expr_def`'s `variants` order exactly. `A_FLOAT`/
/// `A_LET`/`A_CALL`/`A_CHAR`/`A_NIL`/`A_CONS`/`A_CAR`/`A_CDR`/`A_NULLP` are
/// appended last (Phase 2c/2e/2f/2d/3) rather than grouped with related
/// variants — see `ast_expr_def`'s doc comment on why these indices are
/// append-only.
const A_INT: usize = 0;
const A_BOOL: usize = 1;
const A_VAR: usize = 2;
const A_IF: usize = 3;
const A_BINOP: usize = 4;
const A_FLOAT: usize = 5;
const A_LET: usize = 6;
const A_CALL: usize = 7;
const A_CHAR: usize = 8;
const A_NIL: usize = 9;
const A_CONS: usize = 10;
const A_CAR: usize = 11;
const A_CDR: usize = 12;
const A_NULLP: usize = 13;

/// `i64`'s and `f64`'s binary arithmetic/comparison instance methods
/// (`registry::int_assoc`/`float_assoc`) — the only `Expr::Assoc` shapes
/// Phase 1/2c bridge. Both types register the same operator strings, so one
/// list covers either receiver type.
const BINOPS: [&str; 9] = ["+", "-", "*", "<", "<=", ">", ">=", "=", "/="];

/// The scalar types bridged at all (Phase 1/2b/2c/2d/3) — shared by every
/// guard that needs to tell "a value this narrow scope can carry" from
/// "something `compile` must refuse" (`Var`'s/`Call`'s wrapping `Typed.ty`).
fn is_bridgeable_scalar_type(t: &Type) -> bool {
    matches!(t, Type::I64 | Type::Bool | Type::F64 | Type::Char) || is_sexpr_value_type(t)
}

/// Whether `t` is the `Sexpr` type itself — `compile`'s `Sexpr` support
/// (Phase 3) only ever sees this one type (no type parameters: `Sexpr` is
/// not generic), unlike e.g. `Vector<T>`.
fn is_sexpr_value_type(t: &Type) -> bool {
    matches!(t, Type::Named(p, params) if params.is_empty() && *p == Path::root("sexpr"))
}

fn data(variant: usize, fields: Vec<RtValue>) -> RtValue {
    RtValue::Data { type_name: Path::root("astexpr"), variant, fields }
}

fn rt_vector(items: Vec<RtValue>) -> RtValue {
    RtValue::Vector(Rc::new(RefCell::new(items)))
}

/// Converts one checked expression into an `AstExpr` value, or `None` if it
/// uses a construct outside Phase 1/2's scope. Takes `interp` (Phase 2f)
/// solely to check `Expr::Call`'s callee against `Interp.compiled` — every
/// other case is a pure tree conversion with no interpreter state involved.
/// `group` (Phase 2g, self/mutual recursion) is the set of function names
/// currently being compiled *together* in the same `LlvmModule` as `t`'s own
/// function — `compile`'s singleton `[name]` for an ordinary single-function
/// compile (making a self-recursive call bridgeable), or `compile-group`'s
/// full list for several functions compiled together so they can call each
/// other before any of them is registered in `Interp.compiled`. A callee in
/// `group` hasn't finished compiling yet (no JIT'd address exists), but its
/// `LlvmFunction` declaration already exists in the shared module by the time
/// any group member's body is built (`compile-group` declares all of them
/// up front) — see `crate::compile::compiler_source`'s doc comment.
pub(crate) fn typed_to_ast(t: &Typed, interp: &Interp, group: &[Path]) -> Option<RtValue> {
    match &t.expr {
        Expr::Int(n) => Some(data(A_INT, vec![RtValue::Int(*n)])),
        Expr::Bool(b) => Some(data(A_BOOL, vec![RtValue::Bool(*b)])),
        Expr::Float(n) => Some(data(A_FLOAT, vec![RtValue::Float(*n)])),
        Expr::Char(c) => Some(data(A_CHAR, vec![RtValue::Char(*c)])),
        // `Sexpr`'s `Nil` literal (Phase 3) — `()`/`(quote ())`. Every other
        // `QuotedSexpr` shape (`Int`/`Float`/`Char`/`Bool`/`Sym`/`Str`/`Cons`)
        // stays unbridged: a non-empty quoted literal would need `cons`-ing
        // a whole structure at compile time, which `compile-value` has no
        // primitive for (only `cons`/`car`/`cdr` at *runtime*, see `A_CONS`
        // below).
        Expr::Quote(QuotedSexpr::Nil) => Some(data(A_NIL, vec![])),
        // A `Var`'s own `Expr` doesn't carry its type — Phase 1/2 only ever
        // bind `i64`/`bool`/`f64`/`char` parameters *or* `let`-bound locals
        // (Phase 2e) of those same types, so checking the wrapping `Typed.ty`
        // here is how a stray unsupported-type variable (impossible today,
        // not by construction) would be caught rather than silently
        // mistyped downstream.
        Expr::Var(name) if is_bridgeable_scalar_type(&t.ty) => {
            Some(data(A_VAR, vec![RtValue::Str(name.clone())]))
        }
        Expr::If(c, then, els) => {
            let c = typed_to_ast(c, interp, group)?;
            let then = typed_to_ast(then, interp, group)?;
            let els = typed_to_ast(els, interp, group)?;
            Some(data(A_IF, vec![c, then, els]))
        }
        Expr::Assoc { type_name, method, instance: true, args }
            if (*type_name == Path::root("i64") || *type_name == Path::root("f64"))
                && args.len() == 2
                && BINOPS.contains(&method.as_str()) =>
        {
            let lhs = typed_to_ast(&args[0], interp, group)?;
            let rhs = typed_to_ast(&args[1], interp, group)?;
            Some(data(A_BINOP, vec![RtValue::Str(method.clone()), lhs, rhs]))
        }
        // `char`'s `eq`/`lt` (`registry::char_assoc`, Phase 2d) — `char` is
        // stored as a plain `i32`-wide `IntValue` throughout IR construction,
        // so the same `icmp`-lowering `build-op` already does for `i64`'s
        // `=`/`<` works unchanged; only the *method name* differs from the
        // operator string `build-op` expects, so it's translated here rather
        // than teaching `build-op` a second name for the same operation.
        Expr::Assoc { type_name, method, instance: true, args }
            if *type_name == Path::root("char") && args.len() == 2 && (method == "eq" || method == "lt") =>
        {
            let op = if method == "eq" { "=" } else { "<" };
            let lhs = typed_to_ast(&args[0], interp, group)?;
            let rhs = typed_to_ast(&args[1], interp, group)?;
            Some(data(A_BINOP, vec![RtValue::Str(op.to_string()), lhs, rhs]))
        }
        // `let` (CL parallel-binding semantics — every `val` is checked
        // against the *outer* scope, never an earlier sibling binding;
        // `let*` is already desugared to nested single-binding `let`s by
        // the checker, so it needs no separate case here). Bridging fails
        // (`None`) if *any* binding's value or body form does — e.g. a
        // `string`-typed binding — refusing the whole function rather than
        // silently dropping part of it.
        Expr::Let(bindings, body) => {
            let mut names = Vec::with_capacity(bindings.len());
            let mut values = Vec::with_capacity(bindings.len());
            for (name, val) in bindings {
                names.push(RtValue::Str(name.clone()));
                values.push(typed_to_ast(val, interp, group)?);
            }
            let body = body.iter().map(|f| typed_to_ast(f, interp, group)).collect::<Option<Vec<_>>>()?;
            Some(data(A_LET, vec![rt_vector(names), rt_vector(values), rt_vector(body)]))
        }
        // `cons`/`car`/`cdr` (Phase 3): always `Interp::eval_builtin`
        // (Rust-implemented) free functions, never entries in
        // `Interp.compiled`, so they need their own name-based bridging
        // ahead of the general `ACall` case below rather than its
        // `interp.is_compiled` guard.
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "cons" && call_args.len() == 2 => {
            let lhs = typed_to_ast(&call_args[0], interp, group)?;
            let rhs = typed_to_ast(&call_args[1], interp, group)?;
            Some(data(A_CONS, vec![lhs, rhs]))
        }
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "car" && call_args.len() == 1 => {
            let v = typed_to_ast(&call_args[0], interp, group)?;
            Some(data(A_CAR, vec![v]))
        }
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "cdr" && call_args.len() == 1 => {
            let v = typed_to_ast(&call_args[0], interp, group)?;
            Some(data(A_CDR, vec![v]))
        }
        // `null` (Phase 3): an ordinary typelisp `defun` in `prelude.rs`
        // (`(match s ((Nil) true) (_ false)))`), bridged directly onto the
        // `Nil`-pointer-check primitive instead of requiring this bridge to
        // understand `match` at all — `consp`/`atom` (also prelude `defun`s,
        // built from `null`/`not`) stay unbridged for the same reason and
        // are a follow-up step.
        Expr::Call(path, call_args) if path.is_simple() && path.local() == "null" && call_args.len() == 1 => {
            let v = typed_to_ast(&call_args[0], interp, group)?;
            Some(data(A_NULLP, vec![v]))
        }
        // A call to another `defun` (Phase 2f, direct calls between compiled
        // functions; Phase 2g, self/mutual recursion): only bridged if `path`
        // is a simple top-level name (the only shape `compile`'s other
        // introspection builtins — `ast-params`/`param-is-bool`/`ret-is-bool`/
        // etc — support) *and* it's either already compiled (`Interp.compiled`,
        // Phase 2f — its `LlvmFunction` lives in some *other*, already-JIT'd
        // module, resolved later by `add_global_mapping`) *or* a member of
        // `group` (Phase 2g — its `LlvmFunction` is a bare declaration already
        // present in *this same* module by construction, resolved by an
        // ordinary intra-module `call` with no JIT/global-mapping step
        // needed at all, regardless of whether that group member's own body
        // has been built yet — see `crate::compile::compiler_source`'s doc
        // comment on `compile-group`). Calling an uncompiled, non-group
        // function is refused by failing the bridge here (`None`), the same
        // way an unsupported-type binding refuses the whole function,
        // **not** by emitting an `ACall` that `compile-value` would have to
        // reject at IR-build time. The reverse direction (compiled code
        // calling back into uncompiled/tree-walked code) needs a trampoline
        // and stays unsupported — see [docs/TODO.md](../../docs/TODO.md)
        // 「ステップ5・Phase 2f」.
        Expr::Call(path, call_args)
            if path.is_simple()
                && (interp.is_compiled(path) || group.contains(path))
                && is_bridgeable_scalar_type(&t.ty) =>
        {
            let args = call_args.iter().map(|a| typed_to_ast(a, interp, group)).collect::<Option<Vec<_>>>()?;
            Some(data(A_CALL, vec![RtValue::Str(path.local().to_string()), rt_vector(args)]))
        }
        _ => None,
    }
}
