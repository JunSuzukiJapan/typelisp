//! Checking a `match` arm's pattern against the type of the value it
//! matches.
//!
//! `_` matches anything. A name matches anything and binds it, except that
//! a name that is a variant of the type being matched is that variant. A
//! list `(variant pattern...)` is a variant of the type being matched, its
//! fields matched by the patterns; the variant is named bare, since the
//! type is the one being matched.

use crate::check::analyze::{is_option_of_sexpr, is_sexpr_expectation, sexpr_variant_for_literal, BIGNUM_IS_GONE, NIL_IS_GONE};
use crate::sexpr_variant::{self, is_ctor_name};
use crate::check::registry::AdtKind;
use crate::types::{path_is_builtin, Path, Type};
use crate::{wk, Error, Heap, Value};
use typelisp_mem::symbols::{intern, member_symbol, NsId};
use typelisp_mem::SymRef;

use super::bounds::bound_method;
use super::expr::{check_expr, Cx};
use super::locals::Locals;
use super::registry::TypeDef;
use super::resolve::{ctor_named, find_named, receiver_method};
use super::subst::Subst;
use super::tree::Pat;

/// The names a pattern binds, with their types, in the order written.
pub(crate) type Binds = Vec<(SymRef, Type)>;

/// An arm's whole pattern, matching a value of type `expected`.
///
/// A bare name here is always a variant: `_` is the one pattern that
/// matches every value, so a misspelled variant is an error rather than an
/// arm that binds a variable, matches everything and leaves the arms after
/// it unreachable.
pub(crate) fn check_arm_pattern(cx: &Cx, heap: &mut Heap, locals: &Locals, expected: &Type, v: Value, binds: &mut Binds) -> Result<Pat, Error> {
    if let Value::Symbol(id) = v {
        if !id.is(wk::UNDERSCORE) && !id.is_keyword() {
            return match bare_variant(cx, heap, locals, expected, id, binds)? {
                Some(pat) => Ok(pat),
                None => Err(Error::TypeError(format!(
                    "pattern: `{}` is not a variant of `{}` — a bare name in a `match` arm is a variant; only `_` matches every value",
                    heap.symbol_spelling(id),
                    cx.resolve(expected)
                ))),
            };
        }
    }
    check_pattern(cx, heap, locals, expected, v, binds)
}

/// A pattern inside an arm's, matching a value of type `expected`.
fn check_pattern(cx: &Cx, heap: &mut Heap, locals: &Locals, expected: &Type, v: Value, binds: &mut Binds) -> Result<Pat, Error> {
    match v {
        Value::Symbol(id) if id.is(wk::UNDERSCORE) => Ok(Pat::Wild),
        // A keyword names itself: as a pattern it is that symbol.
        Value::Symbol(id) if id.is_keyword() => symbol_pattern(cx, heap, locals, expected, id),
        Value::Symbol(id) => {
            if let Some(pat) = bare_variant(cx, heap, locals, expected, id, binds)? {
                return Ok(pat);
            }
            if binds.iter().any(|(n, _)| *n == id) {
                return Err(Error::TypeError(format!("pattern: `{}` is bound twice", heap.symbol_spelling(id))));
            }
            binds.push((id, expected.clone()));
            Ok(Pat::Bind(id))
        }
        Value::Cons(_) => {
            let parts = heap.list_to_vec(v)?;
            let head = match parts[0] {
                // `(:= expr)`: equal to the value of `expr`.
                Value::Symbol(h) if h.is(wk::COLON_EQUALS) => {
                    let [_, expr] = parts[..] else { return Err(Error::TypeError("pattern: (:= expr)".into())) };
                    return value_pattern(cx, heap, locals, expected, expr);
                }
                // `'sym`: that symbol.
                Value::Symbol(h) if h.is(wk::QUOTE) && matches!(parts[..], [_, Value::Symbol(_)]) => {
                    let Value::Symbol(sym) = parts[1] else { unreachable!("matched above") };
                    return symbol_pattern(cx, heap, locals, expected, sym);
                }
                Value::Symbol(h) if h.is_keyword() || h.is(wk::QUOTE) || h.is(wk::THE) => {
                    unimplemented!("typeck: the pattern `({} ...)`", heap.symbol_spelling(h))
                }
                Value::Symbol(h) => h,
                _ => return Err(Error::TypeError("pattern: constructor must be a symbol".into())),
            };
            let ty = cx.resolve(expected);
            if is_sexpr_expectation(&ty) && !option_variant(cx, &ty, head) {
                if !is_ctor_name(head.name()) && names_type(cx, head) {
                    unimplemented!("typeck: a downcast pattern")
                }
                // An `Option<Sexpr>` is matched by `Sexpr`'s variants as if
                // its `some` were peeled, beside `none` in the same arms.
                if is_option_of_sexpr(&ty) && is_ctor_name(head.name()) {
                    return check_pattern(cx, heap, locals, &sexpr_ty(), v, binds);
                }
            }
            let (owner, targs, def) = sum_type(cx, expected, || format!("({} ...)", heap.symbol_spelling(head)))?;
            let Some(variant) = def.variant_index(member_symbol(head.name())) else {
                return Err(Error::TypeError(format!("`{}` is not a constructor of `{}`", heap.symbol_spelling(head), owner)));
            };
            if path_is_builtin(&owner, wk::SEXPR) {
                match variant {
                    sexpr_variant::NIL => return Err(Error::TypeError(NIL_IS_GONE.into())),
                    sexpr_variant::RETIRED_BIGNUM => return Err(Error::TypeError(BIGNUM_IS_GONE.into())),
                    _ => {}
                }
            }
            let def = def.clone();
            variant_pattern(cx, heap, locals, owner, targs, &def, variant, &parts[1..], binds)
        }
        // The immediate literals of the scrutinee's own type are matched by
        // their word.
        Value::Int(n) if cx.with_default(expected).is_integer() => {
            cx.decide(expected);
            Ok(Pat::Lit { tag: wk::INT_ANY_WIDTH, value: Value::Int(n) })
        }
        Value::Bool(_) if cx.resolve(expected) == Type::Bool => Ok(Pat::Lit { tag: wk::BOOL, value: v }),
        Value::Char(_) if cx.resolve(expected) == Type::Char => Ok(Pat::Lit { tag: wk::CHAR, value: v }),
        Value::Boxed(id) if heap.sexpr_tuple_arity(id).is_some() => tuple_pattern(cx, heap, locals, expected, id, binds),
        // Against a `Sexpr`, comparing would be `eq`: identity, which a
        // separately built string or boxed number never has.
        Value::Str(_) | Value::Boxed(_) if is_sexpr_expectation(&cx.resolve(expected)) => Err(Error::TypeError(format!(
            "pattern: `{}` against a `Sexpr` scrutinee would compare by identity — `Eq` on `sexpr` is `eq`, so this arm could never match a separately built value. Write `({} {})` to destructure it and compare by value instead",
            crate::check::core::print(heap, v),
            sexpr_variant_for_literal(heap, v),
            crate::check::core::print(heap, v)
        ))),
        Value::Int(_) | Value::Bool(_) | Value::Char(_) | Value::Str(_) | Value::Boxed(_) => value_pattern(cx, heap, locals, expected, v),
        other => Err(Error::TypeError(format!("pattern does not match `{}`: `{:?}`", cx.resolve(expected), other))),
    }
}

/// The variant `id` names of the sum type `expected` is, as a pattern — or
/// `None` when `expected` is not a sum type or has no such variant.
fn bare_variant(cx: &Cx, heap: &mut Heap, locals: &Locals, expected: &Type, id: SymRef, binds: &mut Binds) -> Result<Option<Pat>, Error> {
    let ty = cx.resolve(expected);
    let Type::Named(owner, targs) = &ty else { return Ok(None) };
    if is_sexpr_expectation(&ty) && !option_variant(cx, &ty, id) {
        // Against a `Sexpr`, a type's or a variant's name is a downcast.
        if names_type(cx, id) {
            unimplemented!("typeck: a downcast pattern")
        }
        return Ok(None);
    }
    let Some(def) = cx.reg.type_def(owner).filter(|d| d.kind == AdtKind::Sum) else { return Ok(None) };
    let Some(variant) = def.variant_index(member_symbol(id.name())) else { return Ok(None) };
    let def = def.clone();
    variant_pattern(cx, heap, locals, *owner, targs.clone(), &def, variant, &[], binds).map(Some)
}

/// Whether `id` names a type, or a variant a `use` brought in: against a
/// `Sexpr`, a pattern headed by one is a downcast.
fn names_type(cx: &Cx, id: SymRef) -> bool {
    find_named(cx.reg, cx.ns, id, |ns, key| cx.reg.types.get(&Path::of_symbol(ns, key))).is_some() || ctor_named(cx.reg, cx.ns, id).is_some()
}

/// Whether `id` is a variant of `ty` when it is `Option<Sexpr>`: `some` and
/// `none` are its own, not downcasts.
fn option_variant(cx: &Cx, ty: &Type, id: SymRef) -> bool {
    is_option_of_sexpr(ty) && cx.reg.type_def(&Path::builtin(wk::OPTION)).is_some_and(|d| d.variant_index(member_symbol(id.name())).is_some())
}

/// `Sexpr`.
fn sexpr_ty() -> Type {
    Type::Named(Path::builtin(wk::SEXPR), Vec::new())
}

/// The sum type a pattern `what` (for the message) matches a value of, with
/// its type arguments and definition.
fn sum_type<'r>(cx: &Cx<'r>, expected: &Type, what: impl Fn() -> String) -> Result<(Path, Vec<Type>, &'r TypeDef), Error> {
    let ty = cx.resolve(expected);
    if let Type::Named(p, args) = &ty {
        if let Some(def) = cx.reg.type_def(p) {
            return match def.kind {
                AdtKind::Sum => Ok((*p, args.clone(), def)),
                AdtKind::Struct => unimplemented!("typeck: a struct pattern"),
            };
        }
    }
    if let Type::Dyn(..) = ty {
        unimplemented!("typeck: matching a trait object")
    }
    Err(Error::TypeError(format!("pattern `{}` does not match a value of type `{}`", what(), ty)))
}

/// The variant `variant` of `owner<targs>`, its fields matched by `subs`.
#[allow(clippy::too_many_arguments)]
fn variant_pattern(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    owner: Path,
    targs: Vec<Type>,
    def: &TypeDef,
    variant: usize,
    subs: &[Value],
    binds: &mut Binds,
) -> Result<Pat, Error> {
    fields_pattern(cx, heap, locals, owner, targs, def, variant, false, subs, binds)
}

/// [`variant_pattern`], `downcast` saying whether it matches a `Sexpr`
/// holding such a value.
#[allow(clippy::too_many_arguments)]
fn fields_pattern(
    cx: &Cx,
    heap: &mut Heap,
    locals: &Locals,
    owner: Path,
    targs: Vec<Type>,
    def: &TypeDef,
    variant: usize,
    downcast: bool,
    subs: &[Value],
    binds: &mut Binds,
) -> Result<Pat, Error> {
    // A struct's one shape is its fields.
    let (name, fields): (String, Vec<Type>) = match def.kind {
        AdtKind::Struct => (owner.to_string(), def.fields.iter().map(|f| f.ty.clone()).collect()),
        AdtKind::Sum => (def.variants[variant].name.to_string(), def.variants[variant].fields.clone()),
    };
    if subs.len() != fields.len() {
        return Err(Error::TypeError(format!("pattern `{}`: expected {} field(s), got {}", name, fields.len(), subs.len())));
    }
    let subst = Subst::positional(&def.params, &targs);
    let field_tys: Vec<Type> = fields.iter().map(|f| subst.apply(f)).collect();
    let mut checked = Vec::with_capacity(subs.len());
    for (s, t) in subs.iter().zip(&field_tys) {
        checked.push(check_pattern(cx, heap, locals, t, *s, binds)?);
    }
    let subs = checked;
    Ok(Pat::Ctor { owner, targs, variant, downcast, field_tys, subs })
}

/// `#{p0 p1 ...}`: a tuple's elements, each matched by its own pattern.
/// Against a `Sexpr` it matches a `#{..}` datum of that arity, whose
/// elements are `Option<Sexpr>`s.
fn tuple_pattern(cx: &Cx, heap: &mut Heap, locals: &Locals, expected: &Type, id: crate::mem::BoxId, binds: &mut Binds) -> Result<Pat, Error> {
    let n = heap.struct_field_count(id);
    let path = crate::types::tuple_path(n);
    let (targs, downcast) = match cx.resolve(expected) {
        Type::Named(p, args) if p == path => (args, false),
        t if is_sexpr_expectation(&t) => (vec![crate::check::analyze::option_of_sexpr(); n], true),
        t => {
            return Err(Error::TypeError(format!(
                "pattern: `{}` is a tuple of {} element(s), which does not match `{}`",
                crate::check::core::print(heap, Value::Boxed(id)),
                n,
                t
            )))
        }
    };
    let subs: Vec<Value> = (0..n).map(|i| heap.struct_field(id, i)).collect();
    let def = cx.reg.type_def(&path).expect("every tuple arity is defined").clone();
    fields_pattern(cx, heap, locals, path, targs, &def, 0, downcast, &subs, binds)
}

/// The pattern `'sym`: equal to the symbol `sym`, which is made with
/// `intern` from its name and its module, so it is the same symbol however
/// the arm is run.
fn symbol_pattern(cx: &Cx, heap: &mut Heap, locals: &Locals, expected: &Type, sym: SymRef) -> Result<Pat, Error> {
    let home = typelisp_mem::symbols::ns_path(sym.home()).join("::");
    let intern = Value::Symbol(heap.names().read(NsId::STD, "intern"));
    let name = heap.alloc_string(sym.name().to_string());
    heap.push_root(name);
    let home = heap.alloc_string(home);
    heap.push_root(home);
    let expr = crate::check::core::list(heap, &[intern, name, home]);
    heap.pop_root();
    heap.pop_root();
    value_pattern(cx, heap, locals, expected, expr?)
}

/// The pattern that matches a value `equals` to `expr`'s: a test
/// `(equals $match-scrut expr)`, checked where the arm is, with
/// `$match-scrut` the value matched. So a type's own `Eq` compares it, and
/// a type nothing can compare is refused here.
fn value_pattern(cx: &Cx, heap: &mut Heap, locals: &Locals, expected: &Type, expr: Value) -> Result<Pat, Error> {
    let ty = cx.with_default(expected);
    let equals = member_symbol("equals");
    let comparable = match &ty {
        Type::Param(p) => bound_method(cx.reg, cx.scope, *p, equals).is_some(),
        Type::Dyn(t, _) => super::dyn_trait::has_method(cx.reg, t, equals),
        _ => receiver_method(cx.reg, cx.ns, &ty, equals).is_some(),
    };
    if !comparable {
        return Err(Error::TypeError(format!(
            "pattern: `{}` cannot be compared by value — its type `{}` does not implement `Eq`",
            crate::check::core::print(heap, expr),
            ty
        )));
    }
    let scrut = intern(MATCH_SCRUT);
    heap.push_root(expr);
    let call = Value::Symbol(heap.names().read(NsId::STD, "equals"));
    let form = crate::check::core::list(heap, &[call, Value::Symbol(scrut), expr]);
    heap.pop_root();
    let form = form?;
    heap.push_root(form);
    let guard_locals = locals.bind(vec![(scrut, expected.clone())]);
    let test = guard_locals.and_then(|l| check_expr(cx, heap, &l, form, Some(&Type::Bool), heap.cons_loc(expr)));
    heap.pop_root();
    Ok(Pat::Guard { name: scrut, test: Box::new(test?) })
}

/// The local a value pattern's test names the value matched by: no reader
/// can write a name that starts with `$`.
const MATCH_SCRUT: &str = "$match-scrut";
