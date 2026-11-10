//! `(aref array sub...)` and `(setf (aref array sub...) value)`: an
//! `Array<T>`'s cell, its subscripts collected into the `Vector<int>` its
//! `get` and `set` take.
//!
//! `(aref a i j)` is
//!
//! ```text
//! (let* ((idx (Vector::new))) (progn (push idx i) (push idx j) (get a idx)))
//! ```
//!
//! with `a` the array, checked first, bound to a temporary, so the array is
//! evaluated before its subscripts and each subscript once, in order. The
//! write ends in `(set a idx value)` instead. `aref` names nothing but this:
//! a `Vector<T>`, `string` or `HashTable<K,V>` is indexed with `get` and
//! `set` directly.

use crate::check::forms::{list_from_vec_locs, member_form, wrap_let_star};
use crate::types::{path_is_builtin, Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::{well_known_symbol, NsId};

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// `(aref array sub...)` read (`value` `None`) or written with `value`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_aref(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    array: &(Value, Option<Loc>),
    subs: &[(Value, Option<Loc>)],
    value: Option<&(Value, Option<Loc>)>,
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let array = check_expr(cx, heap, locals, array.0, None, array.1.clone())?;
    let ty = cx.resolve(&array.ty);
    if !matches!(&ty, Type::Named(p, _) if path_is_builtin(p, typelisp_mem::wk::ARRAY)) {
        return Err(Error::TypeError(format!(
            "aref: `{}` is not an `Array<T>`. `aref` collects its subscripts into the \
             `Vector<i32>` an array is indexed by; a `Vector<T>`, `string` or `HashTable<K,V>` \
             takes its key directly, as `(get x k)` / `(setf (get x k) v)`.",
            cx.holes.borrow().shown(&ty)
        )));
    }
    let a = cx.temp();
    let idx = Value::Symbol(cx.temp());
    let roots = heap.root_count();
    let form = expansion(heap, Value::Symbol(a), idx, subs, value);
    let checked = form.and_then(|form| locals.bind(vec![(a, array.ty.clone())]).and_then(|l| check_expr(cx, heap, &l, form, expected, loc.clone())));
    heap.truncate_roots(roots);
    let checked = checked?;
    let ty = checked.ty.clone();
    Ok(Typed { node: Node::Let { binds: vec![(a, array)], body: vec![checked] }, ty, loc })
}

/// The form [`check_aref`] checks, `a` naming the array and `idx` the
/// subscripts' vector; each piece stays rooted until the caller truncates.
fn expansion(heap: &mut Heap, a: Value, idx: Value, subs: &[(Value, Option<Loc>)], value: Option<&(Value, Option<Loc>)>) -> Result<Value, Error> {
    let std = |heap: &Heap, name: &str| (Value::Symbol(heap.names().read(NsId::STD, name)), None);
    let new = rooted(heap, |h| member_form(h, &Path::builtin(typelisp_mem::wk::VECTOR), "new"))?;
    let empty = rooted(heap, |h| list_from_vec_locs(h, &[(new, None)]))?;
    let mut body = vec![(Value::Symbol(well_known_symbol(wk::PROGN)), None)];
    for sub in subs {
        let push = std(heap, "push");
        body.push((rooted(heap, |h| list_from_vec_locs(h, &[push, (idx, None), sub.clone()]))?, None));
    }
    let last = match value {
        None => {
            let get = std(heap, "get");
            rooted(heap, |h| list_from_vec_locs(h, &[get, (a, None), (idx, None)]))?
        }
        Some(v) => {
            let set = std(heap, "set");
            rooted(heap, |h| list_from_vec_locs(h, &[set, (a, None), (idx, None), v.clone()]))?
        }
    };
    body.push((last, None));
    let body = rooted(heap, |h| list_from_vec_locs(h, &body))?;
    rooted(heap, |h| wrap_let_star(h, vec![(idx, empty)], body))
}

/// What `make` makes, rooted until [`check_aref`] truncates.
fn rooted(heap: &mut Heap, make: impl FnOnce(&mut Heap) -> Result<Value, Error>) -> Result<Value, Error> {
    let v = make(heap)?;
    heap.push_root(v);
    Ok(v)
}
