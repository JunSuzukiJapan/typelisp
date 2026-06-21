//! Bridges the checker's typed AST (`check::ast::Expr`/`Typed`) into typelisp
//! data (the `AstExpr` builtin ADT, see `check::registry::ast_expr_def`) so
//! the typelisp-written compiler (`compile::compiler_source`) can pattern-match
//! over a function's body with the language's own `match`.
//!
//! Phase 1/2 ([docs/TODO.md](../../docs/TODO.md)「ステップ5」) only bridges the
//! narrow `i64`/`bool`/`f64`-scalar subset needed to compile a function like
//! `(defun max2 ((a i64) (b i64)) i64 (if (< a b) b a))` or `(defun gt ((a
//! i64) (b i64)) bool (> a b))`: integer/bool/float literals, parameter
//! references, `if`, `let`, and `i64`'s/`f64`'s binary arithmetic/comparison
//! instance methods. Anything else returns `None` — `compile` then refuses
//! the function outright rather than miscompiling it (see
//! `Interp::builtin_ast_body` in `src/eval/interp.rs`).

use std::cell::RefCell;
use std::rc::Rc;

use crate::{Expr, Path, RtValue, Type, Typed};

/// Variant indices of `AstExpr`'s constructors — must match
/// `check::registry::ast_expr_def`'s `variants` order exactly. `A_FLOAT`/
/// `A_LET` are appended last (Phase 2c/2e) rather than grouped with related
/// variants — see `ast_expr_def`'s doc comment on why these indices are
/// append-only.
const A_INT: usize = 0;
const A_BOOL: usize = 1;
const A_VAR: usize = 2;
const A_IF: usize = 3;
const A_BINOP: usize = 4;
const A_FLOAT: usize = 5;
const A_LET: usize = 6;

/// `i64`'s and `f64`'s binary arithmetic/comparison instance methods
/// (`registry::int_assoc`/`float_assoc`) — the only `Expr::Assoc` shapes
/// Phase 1/2c bridge. Both types register the same operator strings, so one
/// list covers either receiver type.
const BINOPS: [&str; 9] = ["+", "-", "*", "<", "<=", ">", ">=", "=", "/="];

fn data(variant: usize, fields: Vec<RtValue>) -> RtValue {
    RtValue::Data { type_name: Path::root("astexpr"), variant, fields }
}

fn rt_vector(items: Vec<RtValue>) -> RtValue {
    RtValue::Vector(Rc::new(RefCell::new(items)))
}

/// Converts one checked expression into an `AstExpr` value, or `None` if it
/// uses a construct outside Phase 1/2's scope.
pub(crate) fn typed_to_ast(t: &Typed) -> Option<RtValue> {
    match &t.expr {
        Expr::Int(n) => Some(data(A_INT, vec![RtValue::Int(*n)])),
        Expr::Bool(b) => Some(data(A_BOOL, vec![RtValue::Bool(*b)])),
        Expr::Float(n) => Some(data(A_FLOAT, vec![RtValue::Float(*n)])),
        // A `Var`'s own `Expr` doesn't carry its type — Phase 1/2 only ever
        // bind `i64`/`bool`/`f64` parameters *or* `let`-bound locals (Phase
        // 2e) of those same types, so checking the wrapping `Typed.ty` here
        // is how a stray unsupported-type variable (impossible today, but
        // not by construction) would be caught rather than silently
        // mistyped downstream.
        Expr::Var(name) if matches!(t.ty, Type::I64 | Type::Bool | Type::F64) => {
            Some(data(A_VAR, vec![RtValue::Str(name.clone())]))
        }
        Expr::If(c, then, els) => {
            let c = typed_to_ast(c)?;
            let then = typed_to_ast(then)?;
            let els = typed_to_ast(els)?;
            Some(data(A_IF, vec![c, then, els]))
        }
        Expr::Assoc { type_name, method, instance: true, args }
            if (*type_name == Path::root("i64") || *type_name == Path::root("f64"))
                && args.len() == 2
                && BINOPS.contains(&method.as_str()) =>
        {
            let lhs = typed_to_ast(&args[0])?;
            let rhs = typed_to_ast(&args[1])?;
            Some(data(A_BINOP, vec![RtValue::Str(method.clone()), lhs, rhs]))
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
                values.push(typed_to_ast(val)?);
            }
            let body = body.iter().map(typed_to_ast).collect::<Option<Vec<_>>>()?;
            Some(data(A_LET, vec![rt_vector(names), rt_vector(values), rt_vector(body)]))
        }
        _ => None,
    }
}
