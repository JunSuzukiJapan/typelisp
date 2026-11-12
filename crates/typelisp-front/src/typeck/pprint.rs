//! The pretty printer's forms: `(pprint x)`, `(pprint-fill x)`,
//! `(pprint-linear x)`, `(pprint-tabular x [colinc])`, and
//! `(pprint-logical-block (obj option...) body...)`.
//!
//! The object keeps its own type and is held as a `Sexpr` here, as `print`'s
//! arguments are, so `(pprint 42)` and `(pprint point)` both print. A
//! logical block opens before its body and closes after it; what the body
//! prints is its content.

use crate::typeck::sexpr_value::{option_of_sexpr, sexpr_ty};
use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::SymRef;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// `(pprint x)`, `(pprint-fill x)`, `(pprint-linear x)` or `(pprint-tabular
/// x [colinc])`, `head` saying which — `args` without the head.
pub(crate) fn check_pprint(cx: &Cx, heap: &mut Heap, locals: &Locals, head: SymRef, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let form = head.name();
    let tabular = head.is(wk::PPRINT_TABULAR);
    let (object, colinc) = match args {
        [object] => (object, None),
        [object, colinc] if tabular => (object, Some(colinc)),
        _ if tabular => return Err(Error::TypeError("pprint-tabular: expected an object and an optional column width".into())),
        _ => return Err(Error::TypeError(format!("{}: expected exactly one object to print", form))),
    };
    let object = check_expr(cx, heap, locals, object.0, None, object.1.clone())?;
    let object = held(object);
    // Only `pprint-tabular` reads the column width, whose default is CL's 16.
    let colinc = match colinc {
        Some((c, c_loc)) => check_expr(cx, heap, locals, *c, Some(&Type::I32), c_loc.clone())?,
        None => check_expr(cx, heap, locals, Value::Int(16), Some(&Type::I32), None)?,
    };
    let which = string(cx, heap, locals, form)?;
    Ok(call(wk::F_PPRINT_RT, vec![which, object, colinc], loc))
}

/// `(pprint-logical-block (obj option...) body...)` — `args` without the
/// head. `obj` is the list `pprint-pop` walks, `()` for none; the options
/// are `:prefix`, `:per-line-prefix` and `:suffix`, each a string.
pub(crate) fn check_logical_block(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let Some(((spec, spec_loc), body)) = args.split_first() else {
        return Err(Error::TypeError("pprint-logical-block: (pprint-logical-block (obj :prefix p :suffix s) body...)".into()));
    };
    let spec = heap.list_to_vec_locs(*spec)?;
    let Some(((object, object_loc), options)) = spec.split_first() else {
        return Err(Error::TypeError("pprint-logical-block: the spec needs at least the object to iterate (write `()` for none)".into()));
    };
    let object = check_expr(cx, heap, locals, *object, Some(&option_of_sexpr()), object_loc.clone().or(spec_loc.clone()))?;
    let mut prefix = None;
    let mut suffix = None;
    let mut per_line = false;
    let mut options = options.iter();
    while let Some((key, _)) = options.next() {
        let Value::Symbol(key) = *key else {
            return Err(Error::TypeError("pprint-logical-block: expected a keyword option".into()));
        };
        let Some((value, value_loc)) = options.next() else {
            return Err(Error::TypeError(format!("pprint-logical-block: {} needs a value", key.spelling())));
        };
        let value = check_expr(cx, heap, locals, *value, Some(&Type::Str), value_loc.clone())?;
        match key.well_known() {
            wk::KW_PREFIX => prefix = Some(value),
            wk::KW_PER_LINE_PREFIX => {
                prefix = Some(value);
                per_line = true;
            }
            wk::KW_SUFFIX => suffix = Some(value),
            _ => {
                return Err(Error::TypeError(format!(
                    "pprint-logical-block: unknown option {} (expected :prefix, :per-line-prefix or :suffix)",
                    key.spelling()
                )))
            }
        }
    }
    let prefix = match prefix {
        Some(p) => p,
        None => string(cx, heap, locals, "")?,
    };
    let suffix = match suffix {
        Some(s) => s,
        None => string(cx, heap, locals, "")?,
    };
    let per_line = check_expr(cx, heap, locals, Value::Bool(per_line), Some(&Type::Bool), None)?;
    let mut seq = vec![call(wk::F_PPRINT_BLOCK_START_RT, vec![object, prefix, per_line, suffix], None)];
    for (form, form_loc) in body {
        seq.push(check_expr(cx, heap, locals, *form, None, form_loc.clone())?);
    }
    seq.push(call(wk::F_PPRINT_BLOCK_END_RT, Vec::new(), None));
    Ok(Typed { node: Node::Let { binds: Vec::new(), body: seq }, ty: Type::Unit, loc })
}

/// `value` held as a `Sexpr`.
fn held(value: Typed) -> Typed {
    let loc = value.loc.clone();
    Typed { node: Node::AsSexpr { value: Box::new(value) }, ty: sexpr_ty(), loc }
}

/// The string literal `text`, rooted until the top-level form is checked.
fn string(cx: &Cx, heap: &mut Heap, locals: &Locals, text: &str) -> Result<Typed, Error> {
    let v = heap.alloc_string(text.to_string());
    heap.push_root(v);
    check_expr(cx, heap, locals, v, Some(&Type::Str), None)
}

/// A call of the internal builtin `builtin`, answering `()`.
fn call(builtin: u32, args: Vec<Typed>, loc: Option<Loc>) -> Typed {
    Typed { node: Node::Call { path: Path::internal_builtin(builtin), targs: Vec::new(), args }, ty: Type::Unit, loc }
}
