//! A struct's values: `make-instance`, and reading a field (`p::x`).

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

use super::expr::{check_expr, Cx};
use super::widen::expect;
use super::locals::Locals;
use super::registry::{TypeDef, TypeEntry, ValueDef};
use super::method::check_method_call;
use super::resolve::{find_named, sees, value_named};
use super::subst::Subst;
use super::tree::{Node, Typed};
use super::type_syntax::parse_type;

const SHAPE: &str = "make-instance: (make-instance 'Type :slot value...)";

/// `(make-instance 'Type :slot value...)`: a new struct, every slot given
/// or given its default.
///
/// The values are evaluated in the order they are written. `'Pair` leaves
/// a generic struct's type arguments to be inferred from the values and the
/// expected type; `'Pair<int,string>` names them.
pub(crate) fn check_make_instance(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    args: &[(Value, Option<Loc>)],
    expected: Option<&Type>,
    loc: Option<Loc>,
) -> Result<Typed, Error> {
    let Some((quoted, _)) = args.first() else { return Err(Error::TypeError(SHAPE.into())) };
    let (written, written_at) = match heap.list_to_vec_locs(*quoted).ok().as_deref() {
        Some([(Value::Symbol(q), _), (name, at)]) if q.is(wk::QUOTE) => (*name, at.clone()),
        _ => return Err(Error::TypeError(SHAPE.into())),
    };
    let (path, def, targs) = instance_type(cx, heap, written, written_at.as_ref())?;
    let tname = path.to_string();
    if def.kind != crate::typeck::registry::AdtKind::Struct || def.builtin {
        return Err(Error::TypeError(format!("make-instance: `{}` is not a struct type", crate::check::core::print(heap, written))));
    }
    let subst = Subst::positional(&def.params, &targs);

    let mut given: Vec<Option<(Value, Option<Loc>)>> = vec![None; def.fields.len()];
    let mut order = Vec::new();
    let mut rest = &args[1..];
    while let [(slot, _), tail @ ..] = rest {
        let Value::Symbol(slot) = slot else { return Err(slot_shape(&tname, &def)) };
        if !slot.is_keyword() {
            return Err(slot_shape(&tname, &def));
        }
        let Some((value, value_loc)) = tail.first() else {
            return Err(Error::TypeError(format!("make-instance {}: `:{}` has no value", tname, slot.name())));
        };
        let member = member_symbol(slot.name());
        let Some(i) = def.fields.iter().position(|f| f.name == member) else {
            return Err(Error::TypeError(format!(
                "make-instance {}: no slot `{}` (its slots are {})",
                tname,
                slot.name(),
                def.fields.iter().map(|f| format!("`{}`", f.name)).collect::<Vec<_>>().join(", ")
            )));
        };
        if given[i].is_some() {
            return Err(Error::TypeError(format!("make-instance {}: slot `{}` is given twice", tname, slot.name())));
        }
        if !def.fields[i].public && !sees(cx.ns, path.ns()) {
            return Err(Error::TypeError(format!(
                "make-instance {}: slot `{}` is not `pub`, so it can be given only inside module `{}`",
                tname,
                slot.name(),
                typelisp_mem::symbols::ns_path(path.ns()).join("::")
            )));
        }
        given[i] = Some((*value, value_loc.clone()));
        order.push(i);
        rest = &tail[1..];
    }
    if let Some(i) = given.iter().enumerate().position(|(i, g)| g.is_none() && !def.fields[i].default) {
        return Err(Error::TypeError(format!("make-instance {}: slot `{}` is not given and has no default", tname, def.fields[i].name)));
    }

    // What is expected decides the type arguments it can before the values
    // are checked, so a value is checked against its field's type there.
    let ty = Type::Named(path, targs.clone());
    expect(cx, &ty, expected)?;
    let field_tys: Vec<Type> = def.fields.iter().map(|f| subst.apply(&f.ty)).collect();
    let mut checked: Vec<Option<Typed>> = (0..def.fields.len()).map(|_| None).collect();
    for &i in &order {
        let (value, value_loc) = given[i].clone().expect("a slot written is given");
        checked[i] = Some(check_expr(cx, heap, locals, value, Some(&field_tys[i]), value_loc)?);
    }
    for (p, t) in def.params.iter().zip(&targs) {
        cx.must_decide(t.clone(), format!("cannot infer type argument `{}` for `{}`", p.name.name(), tname), loc.clone());
    }

    // The values are evaluated in the order written; when that is not the
    // slots' order, each is held in a temporary first.
    let in_order = order.windows(2).all(|w| w[0] < w[1]);
    let mut binds = Vec::new();
    // A slot not given is its default, from the method that gives it.
    let default = |i: usize| {
        let node = Node::MethodCall { owner: path, method: super::defstruct::default_method(def.fields[i].name), instance: false, targs: targs.clone(), args: Vec::new() };
        Typed { node, ty: field_tys[i].clone(), loc: None }
    };
    let args: Vec<Typed> = if in_order {
        checked.into_iter().enumerate().map(|(i, c)| c.unwrap_or_else(|| default(i))).collect()
    } else {
        let mut temps: Vec<Option<Typed>> = (0..def.fields.len()).map(|_| None).collect();
        for &i in &order {
            let value = checked[i].take().expect("every slot is checked");
            let temp = cx.temp();
            temps[i] = Some(Typed { node: Node::Local { name: temp }, ty: value.ty.clone(), loc: None });
            binds.push((temp, value));
        }
        temps.into_iter().enumerate().map(|(i, t)| t.unwrap_or_else(|| default(i))).collect()
    };
    let construct = Typed {
        node: Node::Construct { owner: path, targs, variant: 0, mutable: true, field_tys, args },
        ty: ty.clone(),
        loc: loc.clone(),
    };
    if binds.is_empty() {
        return Ok(construct);
    }
    Ok(Typed { node: Node::Let { binds, body: vec![construct] }, ty, loc })
}

/// The struct `make-instance` names: its path, definition and type
/// arguments, each a fresh hole when the name is written bare.
fn instance_type(cx: &Cx, heap: &Heap, written: Value, at: Option<&Loc>) -> Result<(Path, TypeDef, Vec<Type>), Error> {
    if let Value::Symbol(id) = written {
        let found = find_named(cx.reg, cx.ns, id, |ns, key| match cx.reg.types.get(&Path::of_symbol(ns, key)) {
            Some(TypeEntry::Type(def)) => Some(def.clone()),
            _ => None,
        });
        if let Some((path, def)) = found {
            if let Some(entry) = cx.reg.types.get(&path) {
                super::type_syntax::note_use(&cx.types(), path, entry, at);
            }
            let targs = def.params.iter().map(|_| cx.holes.borrow_mut().fresh()).collect();
            return Ok((path, def, targs));
        }
    }
    // A path to the type, its arguments left to be inferred too.
    if let Value::Path(_) = written {
        let path = crate::types::written_path(heap, cx.ns, written)?;
        if let Some(entry @ TypeEntry::Type(def)) = cx.reg.types.get(&path) {
            if !def.public && !super::resolve::sees(cx.ns, path.ns()) {
                return Err(Error::TypeError(format!("make-instance: `{}` is not `pub`, so it is not visible outside its module", path)));
            }
            super::type_syntax::note_use(&cx.types(), path, entry, at);
            let targs = def.params.iter().map(|_| cx.holes.borrow_mut().fresh()).collect();
            return Ok((path, def.clone(), targs));
        }
    }
    let ty = parse_type(heap, &cx.types(), written, at)?;
    let Type::Named(path, targs) = &ty else {
        return Err(Error::TypeError(format!("make-instance: `{}` is not a struct type", ty)));
    };
    let def = cx.reg.type_def(path).expect("a named type is a defined one").clone();
    Ok((*path, def, targs.clone()))
}

fn slot_shape(tname: &str, def: &TypeDef) -> Error {
    Error::TypeError(format!(
        "make-instance {}: the arguments after the type are pairs of a slot keyword and a value, such as `:{} v`",
        tname,
        def.fields.first().map(|f| f.name.name()).unwrap_or("slot"),
    ))
}

/// `recv::member` (read as `(%value-member recv member)`), where `recv` is a
/// local or a global variable: the field `member` of its value, the element
/// `member` (a number) of a tuple, or the call `(member recv)` of an
/// instance method of its type that takes nothing else.
pub(crate) fn check_field(cx: &Cx, heap: &mut Heap, locals: &Locals, recv: SymRef, member: SymRef, loc: Option<Loc>) -> Result<Typed, Error> {
    let unresolved = || Error::TypeError(format!("unresolved path: {}::{}", recv.name(), member.name()));
    let obj = match locals.lookup(recv)? {
        Some(ty) => Typed { node: Node::Local { name: recv }, ty: cx.resolve(ty), loc: loc.clone() },
        None => match value_named(cx.reg, cx.ns, recv) {
            Some((path, ValueDef::Var(var))) => Typed { node: Node::Global { path }, ty: var.ty.clone(), loc: loc.clone() },
            _ => {
                return Err(Error::TypeError(format!(
                    "unresolved path: {}::{} (`{}` is not a variable, and no module or type `{}` is visible here)",
                    recv.name(),
                    member.name(),
                    recv.name(),
                    recv.name()
                )))
            }
        },
    };
    let recv_ty = obj.ty.clone();
    // A field of the C struct a typed pointer points at.
    if matches!(&recv_ty, Type::PtrTo(inner) if matches!(**inner, Type::CStruct(_))) {
        let read = super::c_memory::field_get(cx, heap, obj, member).expect("a typed pointer to a C struct");
        return read.map(|mut t| {
            t.loc = loc;
            t
        });
    }
    let Type::Named(path, args) = &recv_ty else { return Err(unresolved()) };
    // A tuple's element `t::1` is its field of that number.
    let name = match crate::typeck::instance::tuple_element_field(&cx.holes.borrow().shown(&recv_ty), member.name())? {
        Some(field) => member_symbol(&field),
        None => member_symbol(member.name()),
    };
    let field = cx.reg.type_def(path).and_then(|def| {
        let i = def.fields.iter().position(|f| f.name == name)?;
        let f = &def.fields[i];
        (f.public || sees(cx.ns, path.ns())).then(|| (i, Subst::positional(&def.params, args).apply(&f.ty)))
    });
    if let Some((index, ty)) = field {
        return Ok(Typed { node: Node::FieldGet { obj: Box::new(obj), index }, ty, loc });
    }
    match cx.receiver_method(&recv_ty, name) {
        Some((owner, def)) if def.instance => check_method_call(cx, heap, locals, owner, name, def, Some(obj), &[], None, loc),
        _ if cx.reg.has_inst_method(path, name) => {
            unimplemented!("typeck: `{}::{}`, a method `{}` has for one instantiation", recv.name(), member.name(), path)
        }
        _ => Err(unresolved()),
    }
}

/// `t::0` on a tuple: the field that holds element `0`, or an error naming
/// the tuple's arity when there is no such element. `None` when `ty` is not
/// a tuple or `name` is not an index, so the name is looked up as written.
pub(crate) fn tuple_element_field(ty: &Type, name: &str) -> Result<Option<String>, Error> {
    let Type::Named(path, _) = ty else { return Ok(None) };
    let Some(arity) = crate::types::tuple_arity(path) else { return Ok(None) };
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    match name.parse::<usize>() {
        Ok(i) if i < arity => Ok(Some(crate::types::tuple_field(i))),
        _ => Err(Error::TypeError(format!(
            "`{}` has {} element(s), numbered 0 to {}; there is no element `{}`",
            ty,
            arity,
            arity - 1,
            name
        ))),
    }
}
