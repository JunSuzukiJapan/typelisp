//! `(setf place value)`: assign `value` to `place`.
//!
//! A place is a variable, a field `var::field`, or a call `(accessor recv
//! key...)`. A field's place assigns through the struct's `set-field`
//! method, and a call's through the receiver's type's `set-accessor` method
//! (`set` too, for `get`): what a place means is decided by the receiver's
//! type, as a method is, so two types may give the same accessor unrelated
//! setters. A variable's assignment is the value assigned; a setter's is
//! what the setter returns.

use crate::check::bound_name;
use crate::types::{path_spelling, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::method::check_method_call;
use super::registry::ValueDef;
use super::resolve::value_named;
use super::tree::{Node, Typed};

/// `(setf place value)` — `args` without the `setf`.
pub(crate) fn check_setf(cx: &Cx, heap: &mut Heap, locals: &Locals, args: &[(Value, Option<Loc>)], loc: Option<Loc>) -> Result<Typed, Error> {
    let [(place, _), (value, value_loc)] = args else {
        return Err(Error::TypeError("setf: (setf place value)".into()));
    };
    let value = (*value, value_loc.clone());
    match *place {
        Value::Path(id) => Err(Error::TypeError(format!("setf: `{}` is not a place", path_spelling(heap, id)))),
        Value::Cons(_) => {
            let items = heap.list_to_vec_locs(*place).map_err(|_| Error::TypeError("setf: place must be a proper list".into()))?;
            match items.split_first() {
                Some(((Value::Symbol(head), _), [(Value::Symbol(recv), _), (Value::Symbol(field), _)])) if head.is(wk::VALUE_MEMBER) => {
                    set_field(cx, heap, locals, *recv, *field, value, loc)
                }
                Some(((Value::Symbol(head), _), rest)) => set_call_place(cx, heap, locals, *head, rest, value, loc),
                Some(_) => Err(Error::TypeError("setf: place's head must be a function name".into())),
                None => Err(Error::TypeError("setf: empty place".into())),
            }
        }
        // A symbol macro's place is its form's.
        Value::Symbol(name) if locals.symbol_macro(name).is_some() => {
            let form = locals.symbol_macro(name).expect("checked above");
            check_setf(cx, heap, locals, &[(form, heap.cons_loc(form)), value], loc)
        }
        Value::Symbol(name) => set_variable(cx, heap, locals, name, value, loc),
        _ => Err(Error::TypeError("setf: target must be a variable, `var::field`, or a `(get recv key...)` call form".into())),
    }
}

/// `(setf name value)`: a local, or a global variable not a constant.
fn set_variable(cx: &Cx, heap: &mut Heap, locals: &Locals, name: SymRef, (value, value_loc): (Value, Option<Loc>), loc: Option<Loc>) -> Result<Typed, Error> {
    let spelling = bound_name(name, "setf")?;
    if let Some(ty) = locals.lookup(name)? {
        let ty = ty.clone();
        let value = check_expr(cx, heap, locals, value, Some(&ty), value_loc)?;
        return Ok(Typed { node: Node::Set { name, value: Box::new(value) }, ty, loc });
    }
    match value_named(cx.reg, cx.ns, name) {
        Some((path, ValueDef::Var(var))) => {
            let value = check_expr(cx, heap, locals, value, Some(&var.ty), value_loc)?;
            if !var.mutable {
                return Err(Error::TypeError(format!("setf: cannot assign to constant `{}`", spelling)));
            }
            Ok(Typed { node: Node::SetGlobal { path, value: Box::new(value) }, ty: var.ty.clone(), loc })
        }
        _ => Err(Error::TypeError(format!("setf: unbound variable: {}", spelling))),
    }
}

/// `(setf recv::field value)`: the struct's `set-field`, `recv` being a
/// local or a global variable.
fn set_field(cx: &Cx, heap: &mut Heap, locals: &Locals, recv: SymRef, field: SymRef, value: (Value, Option<Loc>), loc: Option<Loc>) -> Result<Typed, Error> {
    let obj = match locals.lookup(recv)? {
        Some(ty) => Typed { node: Node::Local { name: recv }, ty: cx.resolve(ty), loc: None },
        None => match value_named(cx.reg, cx.ns, recv) {
            Some((path, ValueDef::Var(var))) => Typed { node: Node::Global { path }, ty: var.ty.clone(), loc: None },
            _ => return Err(Error::TypeError(format!("setf: unbound variable: {}", recv.name()))),
        },
    };
    let owner = match &obj.ty {
        Type::PtrTo(inner) if matches!(**inner, Type::CStruct(_)) => {
            let set = super::c_memory::field_set(cx, heap, locals, obj, field, value).expect("a typed pointer to a C struct");
            return set.map(|mut t| {
                t.loc = loc;
                t
            });
        }
        Type::Named(p, _) => *p,
        _ => return Err(Error::TypeError(format!("setf: `{}` has no field `{}`", recv.name(), field.name()))),
    };
    // A tuple's element `t::1` is its field of that number.
    let field = match crate::check::analyze::tuple_element_field(&cx.holes.borrow().shown(&obj.ty), field.name())? {
        Some(name) => name,
        None => field.name().to_string(),
    };
    let setter = member_symbol(&format!("set-{}", field));
    let Some((owner, def)) = cx.receiver_method(&obj.ty, setter) else {
        return Err(Error::TypeError(format!("setf: `{}` has no field `{}`", owner, field)));
    };
    check_method_call(cx, heap, locals, owner, setter, def, Some(obj), std::slice::from_ref(&value), None, loc)
}

/// `(setf (accessor recv key...) value)`: the receiver's type's
/// `set-accessor`, or `set` for `get`, called with the keys and the value.
fn set_call_place(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    accessor: SymRef,
    rest: &[(Value, Option<Loc>)],
    value: (Value, Option<Loc>),
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let spelling = heap.symbol_spelling(accessor).to_string();
    if spelling == "c-deref" {
        return super::c_memory::set_c_deref(cx, heap, locals, rest, value, loc);
    }
    if spelling == "aref" {
        let Some((array, subs)) = rest.split_first() else {
            return Err(Error::TypeError("setf: `(aref ...)` place needs an array argument".into()));
        };
        return super::aref::check_aref(cx, heap, locals, array, subs, Some(&value), None, loc);
    }
    let Some(((recv, recv_loc), keys)) = rest.split_first() else {
        return Err(Error::TypeError(format!("setf: `({} ...)` place needs a receiver argument", spelling)));
    };
    let recv = check_expr(cx, heap, locals, *recv, None, recv_loc.clone())?;
    // A number literal no type decided yet is read as its default, as a
    // call's receiver is.
    let Type::Named(owner, _) = cx.with_default(&recv.ty) else {
        return Err(Error::TypeError(format!("setf: `{}` has no settable accessor `{}`", cx.with_default(&recv.ty), spelling)));
    };
    let mut candidates = vec![member_symbol(&format!("set-{}", spelling))];
    if spelling == "get" {
        candidates.push(member_symbol("set"));
    }
    let Some((setter, (owner, def))) = candidates.iter().find_map(|name| cx.receiver_method(&recv.ty, *name).map(|found| (*name, found))) else {
        let tried = candidates.iter().map(|s| format!("`{}`", s)).collect::<Vec<_>>().join(" or ");
        return Err(Error::TypeError(format!("setf: `{}` has no setter for `{}` (looked for {})", owner, spelling, tried)));
    };
    let args: Vec<(Value, Option<Loc>)> = keys.iter().cloned().chain([value]).collect();
    check_method_call(cx, heap, locals, owner, setter, def, Some(recv), &args, None, loc)
}
