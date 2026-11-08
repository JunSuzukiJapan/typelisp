//! `quote`: `(quote datum)`, or `'datum`, the datum as it was read.
//!
//! A quoted symbol is a `Symbol`: the quote is its only literal syntax, as
//! `42` is an integer's. Any other datum is S-expression data,
//! `Option<Sexpr>`, since `'()` is one and the empty list is `none`.

use crate::check::analyze::option_of_sexpr;
use crate::types::Type;
use crate::{Error, Heap, Loc, Value};

use super::tree::{Node, Typed};

/// `(quote datum)` — `args` without the `quote`.
pub(crate) fn check_quote(heap: &mut Heap, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(datum, _)] = args else {
        return Err(Error::TypeError("quote: (quote datum)".into()));
    };
    let ty = match datum {
        Value::Symbol(_) => Type::Symbol,
        _ => option_of_sexpr(),
    };
    heap.push_root(*datum);
    Ok(Typed { node: Node::Quote { datum: *datum }, ty, loc })
}
