//! Checking an expression: which kind of form it is, and the module that
//! checks that kind.

use crate::types::Type;
use crate::{Error, Heap, Loc, Value};

use super::literal::literal;
use super::tree::{Node, Typed};

/// `v` checked where `expected` is expected (`None`: anything).
///
/// A value this allocates is rooted on `heap`'s root stack; the top-level
/// form's checking releases it after the tree is lowered.
pub(crate) fn check_expr(heap: &mut Heap, v: Value, expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    if let Some(lit) = literal(heap, v, expected)? {
        if let Some(value) = lit.value {
            heap.push_root(value);
        }
        let typed = Typed { node: Node::Literal { tag: lit.tag, value: lit.value }, ty: lit.ty, loc };
        return expect(typed, expected);
    }
    let what = match v {
        Value::Symbol(id) => format!("the symbol `{}`", heap.symbol_spelling(id)),
        Value::Cons(_) => "a list form".to_string(),
        other => format!("{:?}", other),
    };
    unimplemented!("typeck: checking {} (only literals are checked so far)", what)
}

/// `typed`, if its type is the one `expected`.
fn expect(typed: Typed, expected: Option<&Type>) -> Result<Typed, Error> {
    match expected {
        Some(e) if typed.ty != Type::Never && typed.ty != *e => Err(Error::TypeError(format!(
            "type mismatch: expected `{}`, found `{}`",
            e, typed.ty
        ))),
        _ => Ok(typed),
    }
}
