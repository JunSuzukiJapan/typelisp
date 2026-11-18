//! Equality of `bool` and `Sexpr` values.

use crate::{Heap, Value};
use crate::eval::value::EvalError;

pub(crate) fn expect_bool(v: &Value) -> Result<bool, EvalError> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(EvalError::Internal(format!("expected a Bool, got {:?}", other))),
    }
}

pub(super) fn bool_eq(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(expect_bool(&args[0])? == expect_bool(&args[1])?))
}


/// `eq` on `Sexpr`: compares the underlying `mem::Value` directly (see
/// `registry::sexpr_assoc`'s doc comment for why this matches CL's `eq`
/// semantics — cons identity, scalar/symbol value equality).
pub(super) fn sexpr_eq(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(typelisp_rt::equality::eq_val(heap, args[0], args[1])))
}

/// `eql` on `Sexpr`: CL's `eql` is `eq` plus "two numbers of the same type
/// and value are equivalent even when they aren't the same object" — the
/// one case that can actually diverge from `eq`'s plain `Value` equality
/// now that `Sexpr::f64` is heap-boxed (`Value::Boxed`, see `BoxedObj`):
/// `eq`'s `==` compares two boxed floats by `BoxId` identity (correctly not
/// `eq` for separately-allocated equal floats, the same way two separately
/// built `Str`s aren't `eq` — see `registry::sexpr_assoc`'s doc comment),
/// but they must still be `eql`. Every other `Sexpr` variant is either
/// immediate (`Int`/`Char`/`Bool`/`Sym`, already value-equal under `eq`) or
/// `eq`-as-identity by design (`Cons`/`Str`) — CL's own `eql` agrees `eq` is
/// already correct for those, so this only special-cases `Boxed`.
///
/// `Value::Boxed` now holds more than floats (structs, hash tables, binding
/// cells, closures) — only a *float* box gets content comparison here; every
/// other boxed kind is an aggregate/identity object for which CL's `eql` is
/// `eq` anyway, so they fall through to the identity comparison below.
pub(super) fn sexpr_eql(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(typelisp_rt::equality::eql_val(heap, args[0], args[1])))
}

/// CL's `equal` on `Sexpr`: `eql` on every atom but `Cons` (structural
/// recursion) and `Str` (case-sensitive content). The comparison itself is
/// [`typelisp_rt::equality::equal_val`], which the compiled `rt_sexpr_equal`
/// shim also calls — one implementation, so `(equal x y)` cannot answer
/// differently depending on whether the caller happened to be compiled.
pub(super) fn sexpr_equal(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(typelisp_rt::equality::equal_val(heap, args[0], args[1])))
}

/// CL's `equalp` on `Sexpr` — [`sexpr_equal`]'s case-folding, cross-type-
/// numeric, struct/enum-recursive sibling. Same shared implementation.
pub(super) fn sexpr_equalp(heap: &Heap, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Bool(typelisp_rt::equality::equalp_val(heap, args[0], args[1])))
}
