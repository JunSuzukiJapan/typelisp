//! A box written in source: `#(..)` a fresh `Vector`, `#nA(..)` a fresh
//! `Array`, `#{..}` a fresh tuple, and any other box, which a program put
//! into S-expression data, itself.
//!
//! Each collection is written as the form that builds it, which is checked
//! in its place. A literal's elements are
//! never evaluated: a symbol, a path or a list is quoted. The element type is
//! what is expected (`Option<Sexpr>` where data is), or else what the
//! elements make it.

use crate::check::forms;
use crate::RootScope;
use super::sexpr_value::is_option_of_sexpr;
use crate::typeck::sexpr_value::{is_sexpr_expectation, option_of_sexpr};
use crate::check::forms::{list_from_vec_locs, member_form, name_form};
use crate::mem::BoxId;
use crate::types::{tuple_path, Path, Type};
use crate::{Error, Heap, Loc, TypeId, Value};

use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::tree::{Node, Typed};

/// The box `id` written in source, checked.
pub(crate) fn check_boxed(cx: &Cx, heap: &mut Heap, locals: &Locals, id: BoxId, expected: Option<&Type>, loc: Option<Loc>) -> Result<Typed, Error> {
    let expected = expected.map(|e| cx.resolve(e));
    let expected = expected.as_ref();
    if crate::type_key::heap_type_is_id(heap, id, TypeId::SEXPR_VECTOR) {
        let form = vector(heap, id, expected)?;
        check_expr(cx, heap, locals, form, expected, loc)
    } else if crate::type_key::heap_type_is_id(heap, id, TypeId::SEXPR_ARRAY) {
        let form = array(heap, id, expected)?;
        check_expr(cx, heap, locals, form, expected, loc)
    } else if let Some(arity) = heap.sexpr_tuple_arity(id) {
        // Where data is expected, a tuple of data, which the caller widens.
        let data = expected.is_some_and(is_sexpr_expectation);
        let form = tuple(heap, id, data)?;
        let data_ty = Type::Named(tuple_path(arity), vec![option_of_sexpr(); arity]);
        check_expr(cx, heap, locals, form, if data { Some(&data_ty) } else { expected }, loc)
    } else {
        // An object no reader makes: a program put it into S-expression
        // data, and it evaluates to itself (CLHS 3.1.2.1.3), as data.
        heap.push_root(Value::Boxed(id));
        Ok(Typed { node: Node::Quote { datum: Value::Boxed(id) }, ty: Type::Named(Path::builtin(typelisp_mem::wk::SEXPR), Vec::new()), loc })
    }
}

/// The elements of the box `id`.
fn elements(heap: &Heap, id: BoxId) -> Vec<Value> {
    (0..heap.struct_field_count(id)).map(|i| heap.struct_field(id, i)).collect()
}

/// The expected element type `ty`, when nothing in it is still to be
/// decided: a type written into the form names no hole, so the elements
/// decide one.
fn decided(ty: Option<Type>) -> Option<Type> {
    let mut open = false;
    if let Some(t) = &ty {
        super::unify::walk(t, &mut |t| open |= matches!(t, Type::Infer(_)));
    }
    ty.filter(|_| !open)
}

/// `elem_ty`, or the elements' when it is `None`: refused when there are
/// none, with the spelling that gives it.
fn element_type(elem_ty: Option<Type>, elems: &[Value], written: &str, annotated: &str) -> Result<Option<Type>, Error> {
    if elem_ty.is_none() && elems.is_empty() {
        return Err(Error::TypeError(format!("`{}` with no elements has no element type to infer: say it with `(the {} ...)`", written, annotated)));
    }
    Ok(elem_ty)
}

/// `#(..)`: a vector, each element pushed onto it.
fn vector(heap: &mut Heap, id: BoxId, expected: Option<&Type>) -> Result<Value, Error> {
    let elems = elements(heap, id);
    let elem_ty = element_type(decided(literal_element_type(expected, &Path::builtin(typelisp_mem::wk::VECTOR))), &elems, "#(..)", "Vector<T>")?;
    rooted(heap, |h| vector_literal_form(h, elem_ty.as_ref(), &elems))
}

/// `#nA(..)`: an array of the dimensions and the elements in row-major
/// order, so that one with a zero dimension is built the same way.
fn array(heap: &mut Heap, id: BoxId, expected: Option<&Type>) -> Result<Value, Error> {
    let parts = elements(heap, id);
    let (Value::Boxed(dims_id), Value::Boxed(data_id)) = (parts[0], parts[1]) else {
        return Err(Error::TypeError("an array literal's dimensions and elements are not vectors".into()));
    };
    let dims = elements(heap, dims_id);
    let data = elements(heap, data_id);
    let array = Path::in_ns(crate::types::std_module("array"), "Array");
    let elem_ty = element_type(decided(literal_element_type(expected, &array)), &data, "#nA(..)", "Array<T>")?;
    let dims_form = rooted(heap, |h| vector_literal_form(h, Some(&Type::Int), &dims))?;
    let data_form = rooted(heap, |h| vector_literal_form(h, elem_ty.as_ref(), &data))?;
    let make = name_form(heap, &Path::internal("array-from-row-major"));
    rooted(heap, |h| list_from_vec_locs(h, &[(make, None), (dims_form, None), (data_form, None)]))
}

/// `#{..}`: the tuple type's `new` of the elements, each one quoted when
/// they are `data`.
fn tuple(heap: &mut Heap, id: BoxId, data: bool) -> Result<Value, Error> {
    let elems = elements(heap, id);
    let new = rooted(heap, |h| member_form(h, &tuple_path(elems.len()), "new"))?;
    let mut items: Vec<(Value, Option<Loc>)> = vec![(new, None)];
    for e in elems {
        items.push((rooted(heap, |h| literal_element_form(h, e, data))?, None));
    }
    rooted(heap, |h| list_from_vec_locs(h, &items))
}

/// What `make` makes, rooted until the top-level form is checked.
fn rooted(heap: &mut Heap, make: impl FnOnce(&mut Heap) -> Result<Value, Error>) -> Result<Value, Error> {
    let v = make(heap)?;
    heap.push_root(v);
    Ok(v)
}

/// The variable a vector literal is built in (`typeck::collection_literal`).
/// Spelled with a leading `$` for the reason [`MATCH_SCRUT`] is: no reader
/// produces it, so it cannot capture a name the program wrote — and a
/// literal's elements are constants, so they never refer to it either.
const VECTOR_LITERAL_VAR: &str = "$vector-literal";

/// The element type the context of a `#(..)`/`#nA(..)` literal fixes:
/// `T` of an expected `Vector<T>`/`Array<T>` (`collection` names which), or
/// `Option<Sexpr>` when the literal is expected as S-expression data. `None`
/// when the context says nothing, and the first element decides.
pub(crate) fn literal_element_type(expected: Option<&Type>, collection: &Path) -> Option<Type> {
    match expected {
        Some(Type::Named(p, args)) if p == collection && args.len() == 1 => {
            Some(args[0].clone())
        }
        Some(t) if is_sexpr_expectation(t) => Some(option_of_sexpr()),
        _ => None,
    }
}

/// One element of a literal collection, as the form that evaluates to it. A
/// literal's contents are never evaluated, so a symbol, a path or a list is
/// quoted; numbers, strings, characters, booleans and nested literals
/// evaluate to themselves already. In a data vector (`data`) every element
/// is quoted, so that each one is S-expression data.
pub(crate) fn literal_element_form(heap: &mut Heap, e: Value, data: bool) -> Result<Value, Error> {
    if data || matches!(e, Value::Symbol(_) | Value::Path(_) | Value::Cons(_)) {
        let quote = heap.intern_symbol("quote");
        forms::list_from_vec_locs(heap, &[(quote, None), (e, None)])
    } else {
        Ok(e)
    }
}

/// `(let (($vector-literal (the Vector<T> (Vector::new))))
///    (push $vector-literal E)... $vector-literal)` for the elements `elems`.
/// With no `elem_ty`, the vector is `(Vector::new)` and the first element
/// decides `T`.
pub(crate) fn vector_literal_form(heap: &mut Heap, elem_ty: Option<&Type>, elems: &[Value]) -> Result<Value, Error> {
    let data = elem_ty.is_some_and(is_option_of_sexpr);
    let mut s = RootScope::new(heap);
    let var = s.intern_symbol(VECTOR_LITERAL_VAR);
    let new = forms::member_form(&mut s, &Path::builtin(typelisp_mem::wk::VECTOR), "new")?;
    s.push_root(new);
    let new_call = forms::list_from_vec_locs(&mut s, &[(new, None)])?;
    s.push_root(new_call);
    let init = match elem_ty {
        Some(elem_ty) => {
            let vec_ty = forms::type_to_form(&mut s, &Type::Named(Path::builtin(typelisp_mem::wk::VECTOR), vec![elem_ty.clone()]))?;
            s.push_root(vec_ty);
            let the = s.intern_symbol("the");
            forms::list_from_vec_locs(&mut s, &[(the, None), (vec_ty, None), (new_call, None)])?
        }
        None => new_call,
    };
    s.push_root(init);
    let binding = forms::list_from_vec_locs(&mut s, &[(var, None), (init, None)])?;
    s.push_root(binding);
    let bindings = forms::list_from_vec_locs(&mut s, &[(binding, None)])?;
    s.push_root(bindings);
    let let_sym = s.intern_symbol("let");
    let push = Value::Symbol(s.names().read(typelisp_mem::NsId::STD, "push"));
    let mut items: Vec<(Value, Option<Loc>)> = vec![(let_sym, None), (bindings, None)];
    for &e in elems {
        let ef = literal_element_form(&mut s, e, data)?;
        s.push_root(ef);
        let call = forms::list_from_vec_locs(&mut s, &[(push, None), (var, None), (ef, None)])?;
        s.push_root(call);
        items.push((call, None));
    }
    items.push((var, None));
    forms::list_from_vec_locs(&mut s, &items)
}
