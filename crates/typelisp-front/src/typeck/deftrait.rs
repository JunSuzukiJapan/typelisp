//! `deftrait`: a trait, the methods a type implementing it has.
//!
//! `(deftrait Name (Super...) (:type Assoc)... (defmethod name (self param...) Ret)...)`.
//! The list after the name is the supertraits, `()` for none. `Self` and
//! each `(:type Assoc)` are the trait's type parameters, `Self` first
//! (`docs/dev/checker-rebuild-plan.md` 7.4); a method's signature is written
//! in them. An `impl` of the trait for a type gives the bodies.

use crate::check::bound_name;
use crate::check::registry::TraitBound;
use crate::types::{ParamOwner, Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::{member_symbol, well_known_symbol};
use typelisp_mem::SymRef;

use super::define::{Definer, Kind};
use super::defun::parse_header;
use super::impl_block::method_params;
use super::registry::{FnDef, Registry, TraitDef, TypeEntry};
use super::resolve::trait_named;
use super::scope::TypeScope;
use super::type_syntax::{parse_type, TypeCx};

const SHAPE: &str = "deftrait: (deftrait Name (Super...) (:type AssocName)... (defmethod method (params...) ret)...) \
                     — the supertrait list is required; write `()` for none";

/// `(deftrait ...)` — `parts` without the `deftrait` — and its core form,
/// unrooted. The caller undoes the definition if checking fails.
pub(crate) fn check_deftrait(reg: &mut Registry, d: &mut Definer, heap: &mut Heap, parts: &[(Value, Option<Loc>)]) -> Result<Value, Error> {
    let [(name, _), (supers, _), items @ ..] = parts else {
        return Err(Error::TypeError(SHAPE.into()));
    };
    let Value::Symbol(name) = *name else {
        return Err(Error::TypeError("deftrait: name must be a symbol".into()));
    };
    let spelling = bound_name(name, "deftrait")?;
    if matches!(items.first(), Some((Value::Str(_), _))) {
        unimplemented!("typeck: a `deftrait` with a documentation string")
    }
    d.ownable(heap, name, "deftrait")?;
    d.type_name(reg, Kind::Trait, name)?;
    let ns = d.site.ns;
    let path = Path::of_symbol(ns, name);
    // Read before the trait is registered, so a trait cannot list itself.
    let supertraits = supertraits(heap, reg, ns, &spelling, *supers)?;
    let mut names = vec![well_known_symbol(wk::SELF_TYPE)];
    let mut methods: Vec<(SymRef, Value, Value)> = Vec::new();
    for (item, _) in items {
        match item_of(heap, *item)? {
            Item::Assoc(a) => names.push(a),
            Item::Method(name, params, ret) => {
                let m = member_symbol(name.name());
                if methods.iter().any(|(n, _, _)| member_symbol(n.name()) == m) {
                    return Err(Error::TypeError(format!("deftrait: duplicate method `{}`", m)));
                }
                methods.push((name, params, ret));
            }
        }
    }
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Def(path), &names).map_err(|twice| {
        Error::TypeError(format!("deftrait {}: `{}` is declared twice", spelling, twice.0.name()))
    })?;
    let params = scope.params().to_vec();
    // Registered before the signatures are read, so a signature can name the
    // trait (`:dyn Name`).
    let mut def = TraitDef { params: params.clone(), methods: Vec::new(), defaults: Vec::new(), supertraits, public: false };
    reg.types.insert(path, TypeEntry::Trait(def.clone()));
    let tcx = TypeCx { reg, ns, scope: &scope, self_type: None };
    let self_ty = Type::Param(params[0]);
    for (name, written, ret) in methods {
        let (_, ps) = method_params(heap, &tcx, name, written, &self_ty)?;
        let ret = parse_type(heap, &tcx, ret)?;
        let sig = FnDef { public: true, ..FnDef::plain(params.clone(), ps.into_iter().map(|(_, t)| t).collect(), ret) };
        def.methods.push((member_symbol(name.name()), sig));
    }
    reg.types.insert(path, TypeEntry::Trait(def));
    crate::check::forms::module_form(heap, &path, &[])
}

/// One item of a `deftrait`.
enum Item {
    /// `(:type Name)`.
    Assoc(SymRef),
    /// `(defmethod name (params...) Ret)`: the name, the parameters and the
    /// result type, as written.
    Method(SymRef, Value, Value),
}

fn item_of(heap: &Heap, item: Value) -> Result<Item, Error> {
    let elems = heap.list_to_vec(item).map_err(|_| Error::TypeError("deftrait: item must start with a symbol".into()))?;
    let elems = match elems.as_slice() {
        [Value::Symbol(k), rest @ ..] if k.is(wk::KW_TYPE) => {
            return match rest {
                [Value::Symbol(a)] => Ok(Item::Assoc(*a)),
                [_] => Err(Error::TypeError("deftrait: associated type name must be a symbol".into())),
                _ => Err(Error::TypeError("deftrait: (:type AssocName)".into())),
            };
        }
        [Value::Symbol(d), rest @ ..] if d.is(wk::DEFMETHOD) => rest,
        [Value::Symbol(p), Value::Symbol(d), rest @ ..] if p.is(wk::PUB) && d.is(wk::DEFMETHOD) => rest,
        _ => {
            return Err(Error::TypeError(
                "deftrait: a method is written (defmethod name (params...) ReturnType body...), or (pub defmethod ...) to make it public".into(),
            ))
        }
    };
    let [header, params, ret, rest @ ..] = elems else {
        return Err(Error::TypeError(
            "deftrait: method must be (defmethod name (params...) ret) or (defmethod name (params...) ret body...)".into(),
        ));
    };
    let (name, own) = parse_header(heap, *header)?;
    if !own.is_empty() {
        unimplemented!("typeck: a trait method with type parameters of its own")
    }
    if !rest.is_empty() {
        unimplemented!("typeck: a trait method with a default body, a `:where` clause or a documentation string")
    }
    Ok(Item::Method(name, *params, *ret))
}

/// The supertrait list `v` of the trait `name`.
fn supertraits(heap: &Heap, reg: &Registry, ns: typelisp_mem::symbols::NsId, name: &str, v: Value) -> Result<Vec<TraitBound>, Error> {
    let elems = heap
        .list_to_vec(v)
        .map_err(|_| Error::TypeError(format!("deftrait {}: the supertrait list must be a list — write `()` for none", name)))?;
    let mut out: Vec<TraitBound> = Vec::new();
    for elem in elems {
        let Value::Symbol(sym) = elem else {
            match elem {
                Value::Cons(_) => unimplemented!("typeck: a supertrait whose associated types are pinned"),
                _ => return Err(Error::TypeError(format!("deftrait {}: supertrait name must be a symbol", name))),
            }
        };
        let Some((trait_path, sdef)) = trait_named(reg, ns, sym) else {
            return Err(Error::TypeError(format!(
                "deftrait {}: unknown supertrait `{}` — the form is (deftrait Name (Super...) items...), \
                 so the list right after the name is the supertrait list (write `()` for none)",
                name,
                heap.symbol_spelling(sym)
            )));
        };
        if let Some(a) = sdef.params.get(1) {
            return Err(Error::TypeError(format!(
                "deftrait {}: supertrait `{}` has an associated type `{}`, which must be pinned here — \
                 write `({} ({} <type>))`",
                name,
                trait_path,
                a.name.name(),
                trait_path,
                a.name.name()
            )));
        }
        if out.iter().any(|b| b.trait_path == trait_path) {
            return Err(Error::TypeError(format!("deftrait {}: `{}` is listed as a supertrait twice", name, trait_path)));
        }
        out.push(TraitBound { trait_path, assoc: Default::default() });
    }
    Ok(out)
}
