//! Bridges the checker's typed AST (`check::ast::Expr`/`Typed`) into typelisp
//! data (the `AstExpr` builtin ADT, see `check::registry::ast_expr_def`) so
//! the typelisp-written compiler (`compile::compiler_source`) can pattern-match
//! over a function's body with the language's own `match`.
//!
//! Phase 1/2 ([docs/TODO.md](../../docs/TODO.md)「ステップ5」) only bridges the
//! narrow `i64`/`bool`-scalar subset needed to compile a function like
//! `(defun max2 ((a i64) (b i64)) i64 (if (< a b) b a))` or `(defun gt ((a
//! i64) (b i64)) bool (> a b))`: integer/bool literals, parameter references,
//! `if`, and `i64`'s binary arithmetic/comparison instance methods. Anything
//! else returns `None` — `compile` then refuses the function outright rather
//! than miscompiling it (see `Interp::builtin_ast_body` in `src/eval/interp.rs`).

use crate::{Expr, Path, RtValue, Type, Typed};

/// Variant indices of `AstExpr`'s constructors — must match
/// `check::registry::ast_expr_def`'s `variants` order exactly.
const A_INT: usize = 0;
const A_BOOL: usize = 1;
const A_VAR: usize = 2;
const A_IF: usize = 3;
const A_BINOP: usize = 4;

/// `i64`'s binary arithmetic/comparison instance methods (`registry::int_assoc`)
/// — the only `Expr::Assoc` shape Phase 1 bridges.
const I64_BINOPS: [&str; 9] = ["+", "-", "*", "<", "<=", ">", ">=", "=", "/="];

fn data(variant: usize, fields: Vec<RtValue>) -> RtValue {
    RtValue::Data { type_name: Path::root("astexpr"), variant, fields }
}

/// Converts one checked expression into an `AstExpr` value, or `None` if it
/// uses a construct outside Phase 1/2's scope.
pub(crate) fn typed_to_ast(t: &Typed) -> Option<RtValue> {
    match &t.expr {
        Expr::Int(n) => Some(data(A_INT, vec![RtValue::Int(*n)])),
        Expr::Bool(b) => Some(data(A_BOOL, vec![RtValue::Bool(*b)])),
        // A `Var`'s own `Expr` doesn't carry its type — Phase 1/2b only ever
        // bind `i64`/`bool` parameters (no `let`-bound locals are bridged
        // yet), so checking the wrapping `Typed.ty` here is how a stray
        // unsupported-type variable (impossible today, but not by
        // construction) would be caught rather than silently mistyped
        // downstream.
        Expr::Var(name) if matches!(t.ty, Type::I64 | Type::Bool) => Some(data(A_VAR, vec![RtValue::Str(name.clone())])),
        Expr::If(c, then, els) => {
            let c = typed_to_ast(c)?;
            let then = typed_to_ast(then)?;
            let els = typed_to_ast(els)?;
            Some(data(A_IF, vec![c, then, els]))
        }
        Expr::Assoc { type_name, method, instance: true, args }
            if *type_name == Path::root("i64") && args.len() == 2 && I64_BINOPS.contains(&method.as_str()) =>
        {
            let lhs = typed_to_ast(&args[0])?;
            let rhs = typed_to_ast(&args[1])?;
            Some(data(A_BINOP, vec![RtValue::Str(method.clone()), lhs, rhs]))
        }
        _ => None,
    }
}
