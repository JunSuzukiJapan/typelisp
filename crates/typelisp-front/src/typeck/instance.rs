//! A struct's values: `make-instance`, and reading a field (`p::x`).

use crate::types::{Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::member_symbol;
use typelisp_mem::SymRef;

use super::expr::{check_expr, Cx};
use super::widen::expect;
use super::locals::Locals;
use super::registry::{TypeDef, TypeEntry, ValueDef};
use super::resolve::{find_named, sees, value_named};
use super::subst::Subst;
use super::tree::{Node, Typed};
use super::type_syntax::parse_type;

const SHAPE: &str = "make-instance: (make-instance 'Type :slot value...)";

/// `(make-instance 'Type :slot value...)`: a new struct, every slot given.
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
    let written = match heap.list_to_vec(*quoted).ok().as_deref() {
        Some([Value::Symbol(q), name]) if q.is(wk::QUOTE) => *name,
        _ => return Err(Error::TypeError(SHAPE.into())),
    };
    let (path, def, targs) = instance_type(cx, heap, written)?;
    let tname = path.to_string();
    if def.kind != crate::check::registry::AdtKind::Struct || def.builtin {
        return Err(Error::TypeError(format!("make-instance: `{}` is not a struct type", tname)));
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
                "make-instance {}: slot `{}` is not `pub`, so it is not visible outside `{}`'s module",
                tname,
                slot.name(),
                tname
            )));
        }
        given[i] = Some((*value, value_loc.clone()));
        order.push(i);
        rest = &tail[1..];
    }
    if let Some(i) = given.iter().position(Option::is_none) {
        if def.has_defaults {
            unimplemented!("typeck: `make-instance` of `{}`, whose slots have defaults", tname)
        }
        return Err(Error::TypeError(format!("make-instance {}: slot `{}` is not given and has no default", tname, def.fields[i].name)));
    }

    // What is expected decides the type arguments it can before the values
    // are checked, so a value is checked against its field's type there.
    let ty = Type::Named(path, targs.clone());
    expect(cx, &ty, expected)?;
    let field_tys: Vec<Type> = def.fields.iter().map(|f| subst.apply(&f.ty)).collect();
    let mut checked: Vec<Option<Typed>> = (0..def.fields.len()).map(|_| None).collect();
    for &i in &order {
        let (value, value_loc) = given[i].clone().expect("every slot is given");
        checked[i] = Some(check_expr(cx, heap, locals, value, Some(&field_tys[i]), value_loc)?);
    }
    for (p, t) in def.params.iter().zip(&targs) {
        cx.must_decide(t.clone(), format!("cannot infer type argument `{}` for `{}`", p.name.name(), tname), loc.clone());
    }

    // The values are evaluated in the order written; when that is not the
    // slots' order, each is held in a temporary first.
    let in_order = order.windows(2).all(|w| w[0] < w[1]);
    let mut binds = Vec::new();
    let args: Vec<Typed> = if in_order {
        checked.into_iter().map(|c| c.expect("every slot is checked")).collect()
    } else {
        let mut temps: Vec<Option<Typed>> = (0..def.fields.len()).map(|_| None).collect();
        for &i in &order {
            let value = checked[i].take().expect("every slot is checked");
            let temp = cx.temp();
            temps[i] = Some(Typed { node: Node::Local { name: temp }, ty: value.ty.clone(), loc: None });
            binds.push((temp, value));
        }
        temps.into_iter().map(|t| t.expect("every slot has a temporary")).collect()
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
fn instance_type(cx: &Cx, heap: &Heap, written: Value) -> Result<(Path, TypeDef, Vec<Type>), Error> {
    if let Value::Symbol(id) = written {
        let found = find_named(cx.reg, cx.ns, id, |ns, key| match cx.reg.types.get(&Path::of_symbol(ns, key)) {
            Some(TypeEntry::Type(def)) => Some(def.clone()),
            _ => None,
        });
        if let Some((path, def)) = found {
            let targs = def.params.iter().map(|_| cx.holes.borrow_mut().fresh()).collect();
            return Ok((path, def, targs));
        }
    }
    // A path to the type, its arguments left to be inferred too.
    if let Value::Path(_) = written {
        let path = crate::types::written_path(heap, cx.ns, written)?;
        if let Some(TypeEntry::Type(def)) = cx.reg.types.get(&path) {
            if !def.public && !super::resolve::sees(cx.ns, path.ns()) {
                return Err(Error::TypeError(format!("make-instance: `{}` is not `pub`, so it is not visible outside its module", path)));
            }
            let targs = def.params.iter().map(|_| cx.holes.borrow_mut().fresh()).collect();
            return Ok((path, def.clone(), targs));
        }
    }
    let ty = parse_type(heap, &cx.types(), written)?;
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
/// local of a struct type: that field.
pub(crate) fn check_field(cx: &Cx, heap: &mut Heap, locals: &Locals, recv: SymRef, member: SymRef, loc: Option<Loc>) -> Result<Typed, Error> {
    let Some(recv_ty) = locals.lookup(recv) else {
        if let Some((_, ValueDef::Var(_))) = value_named(cx.reg, cx.ns, recv) {
            unimplemented!("typeck: `{}::{}`, a member of the global `{}`", recv.name(), member.name(), recv.name())
        }
        return Err(Error::TypeError(format!(
            "unresolved path: {}::{} (`{}` is not a variable, and no module or type `{}` is visible here)",
            recv.name(),
            member.name(),
            recv.name(),
            recv.name()
        )));
    };
    let recv_ty = cx.resolve(recv_ty);
    let obj = Typed { node: Node::Local { name: recv }, ty: recv_ty.clone(), loc: loc.clone() };
    // A field of the C struct a typed pointer points at.
    if matches!(&recv_ty, Type::PtrTo(inner) if matches!(**inner, Type::CStruct(_))) {
        let read = super::c_memory::field_get(cx, heap, obj, member).expect("a typed pointer to a C struct");
        return read.map(|mut t| {
            t.loc = loc;
            t
        });
    }
    let field = match &recv_ty {
        Type::Named(path, args) => cx.reg.type_def(path).and_then(|def| {
            let name = member_symbol(member.name());
            let i = def.fields.iter().position(|f| f.name == name)?;
            let f = &def.fields[i];
            (f.public || sees(cx.ns, path.ns())).then(|| (i, Subst::positional(&def.params, args).apply(&f.ty)))
        }),
        _ => None,
    };
    let Some((index, ty)) = field else {
        unimplemented!("typeck: `{}::{}` on a `{}`, which is no field of it", recv.name(), member.name(), recv_ty)
    };
    Ok(Typed { node: Node::FieldGet { obj: Box::new(obj), index }, ty, loc })
}
