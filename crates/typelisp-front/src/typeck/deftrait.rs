//! `deftrait`: a trait, the methods a type implementing it has.
//!
//! `(deftrait Name (Super...) (:type Assoc)... (defmethod name (self param...) Ret)...)`.
//! The list after the name is the supertraits, `()` for none. `Self` and
//! each `(:type Assoc)` are the trait's type parameters, `Self` first
//! (`docs/dev/checker-rebuild-plan.md` 7.4); a method's signature is written
//! in them. An `impl` of the trait for a type gives the bodies.

use crate::check::bound_name;
use crate::typeck::registry::TraitBound;
use crate::types::{ParamOwner, Path, Type};
use crate::{wk, Error, Heap, Loc, Value};
use typelisp_mem::symbols::{member_symbol, well_known_symbol};
use typelisp_mem::SymRef;

use super::define::{Definer, Kind};
use super::expr::Cx;
use super::generic::Base;
use super::block::check_named_body;
use super::locals::Locals;
use super::defun::{parse_header, split_doc};
use super::impl_block::method_params;
use super::registry::{Definition, FnDef, Registry, Template, TraitDef};
use super::resolve::trait_named;
use super::scope::TypeScope;
use super::type_syntax::{parse_type, TypeCx};

const SHAPE: &str = "deftrait: (deftrait Name (Super...) (:type AssocName)... (defmethod method (params...) ret)...) \
                     — the supertrait list is required; write `()` for none";

/// `(deftrait ...)` — `parts` without the `deftrait` — and its core form,
/// unrooted. The caller undoes the definition if checking fails.
pub(crate) fn check_deftrait(reg: &mut Registry, d: &mut Definer, heap: &mut Heap, parts: &[(Value, Option<Loc>)]) -> Result<Value, Error> {
    let [(name, name_at), (supers, _), items @ ..] = parts else {
        return Err(Error::TypeError(SHAPE.into()));
    };
    let Value::Symbol(name) = *name else {
        return Err(Error::TypeError("deftrait: name must be a symbol".into()));
    };
    let spelling = bound_name(name, "deftrait")?;
    let (doc, items) = split_doc(heap, items);
    let name = d.ownable(heap, name, "deftrait")?;
    d.type_name(reg, Kind::Trait, name)?;
    let ns = d.site.ns;
    let path = Path::of_symbol(ns, name);
    d.record(reg, Definition::Trait(path), doc);
    let mut names = vec![well_known_symbol(wk::SELF_TYPE)];
    let mut methods: Vec<Written> = Vec::new();
    for (item, _) in items {
        match item_of(heap, *item)? {
            Item::Assoc(a) => names.push(a),
            Item::Method(written) => {
                let m = member_symbol(written.name.name());
                if methods.iter().any(|w| member_symbol(w.name.name()) == m) {
                    return Err(Error::TypeError(format!("deftrait: duplicate method `{}`", m)));
                }
                methods.push(written);
            }
        }
    }
    let root = TypeScope::empty();
    let scope = root.declare(ParamOwner::Def(path), &names).map_err(|twice| {
        Error::TypeError(format!("deftrait {}: `{}` is declared twice", spelling, twice.0.name()))
    })?;
    let params = scope.params().to_vec();
    // Read before the trait is registered, so a trait cannot list itself. A
    // pin may name the trait's own associated types.
    let supertraits = supertraits(heap, &TypeCx { reg, ns, scope: &scope, self_type: None, notes: d.notes }, &spelling, *supers)?;
    // Registered before the signatures are read, so a signature can name the
    // trait (`:dyn Name`).
    let mut def = TraitDef { params: params.clone(), methods: Vec::new(), defaults: Vec::new(), supertraits, public: d.public };
    reg.define_trait(path, def.clone());
    super::type_syntax::note_definition(d.notes, path, &super::registry::TypeEntry::Trait(def.clone()), name_at.as_ref());
    let self_ty = Type::Param(params[0]);
    let mut bodies = Vec::new();
    let mut bodyless = Vec::new();
    for w in &methods {
        let m = member_symbol(w.name.name());
        // A method's own type parameters follow the trait's.
        let own = scope.declare(ParamOwner::Method(path, m), &w.own).map_err(|twice| {
            Error::TypeError(format!("deftrait {}: method `{}` declares `{}` twice", spelling, m, twice.0.name()))
        })?;
        if let Some(clash) = w.own.iter().find(|o| names.iter().any(|n| n.name() == o.name())) {
            return Err(Error::TypeError(format!("deftrait {}: method `{}` declares `{}`, which is already the trait's", spelling, m, clash.name())));
        }
        let tcx = TypeCx { reg, ns, scope: &own, self_type: None, notes: d.notes };
        let (_, ps) = method_params(heap, &tcx, w.name, w.params, &self_ty)?;
        let ret = parse_type(heap, &tcx, w.ret, w.ret_at.as_ref())?;
        let type_params = params.iter().chain(own.params()).copied().collect();
        let sig = FnDef { public: true, ..FnDef::plain(type_params, ps.iter().map(|(_, t, _)| t.clone()).collect(), ret.clone()) };
        def.methods.push((m, sig));
        if w.body.is_empty() {
            bodyless.push(m);
        } else {
            def.defaults.push(m);
            bodies.push((m, w.name, &w.own, ps, ret, &w.body));
        }
    }
    // The vtable a trait object of it is called through: a method may not
    // be inherited twice, or declared again.
    let own: Vec<SymRef> = def.methods.iter().map(|(m, _)| *m).collect();
    super::dyn_trait::linearize(reg, &spelling, path, &def.supertraits, &own)?;
    reg.define_trait(path, def);
    for w in &methods {
        if let Some(doc) = &w.doc {
            reg.docs.insert(Definition::TraitMethod(path, member_symbol(w.name.name())), doc.clone());
        }
    }
    for m in bodyless {
        reg.set_template(Base::Method(path, m), None);
    }
    // A default body is checked once, for every type implementing the
    // trait: `Self` is a type parameter that has the trait, its associated
    // types the trait's other parameters.
    let assoc = params[1..].iter().map(|a| (a.name.name().to_string(), Type::Param(*a))).collect();
    let bounded = scope.bound(vec![(params[0], TraitBound { trait_path: path, assoc })]);
    for (m, name, own, ps, ret, body) in bodies {
        let own = bounded.declare(ParamOwner::Method(path, m), own).expect("declared once above");
        let mut body = {
            let cx = Cx::new(reg, d.expander, ns, &own, Some(&self_ty), d.notes);
            let locals_root = Locals::empty();
            let locals = locals_root.bind(ps.clone())?;
            let mut body = check_named_body(&cx, heap, &locals, name, body, &ret)?;
            cx.settle(heap, &mut body)?;
            body
        };
        for t in &body {
            t.each_value(&mut |v| heap.push_permanent_root(v));
        }
        reg.set_template(Base::Method(path, m), Some(Template { params: ps.into_iter().map(|(n, t, _)| (n, t)).collect(), ret, body: std::mem::take(&mut body) }));
    }
    crate::check::forms::module_form(heap, &path, &[])
}

/// A method of a `deftrait` as written: its name, its parameters, its
/// result type and its default body, empty when it has none.
struct Written {
    name: SymRef,
    /// The type parameters the method declares of its own (`name<U...>`).
    own: Vec<SymRef>,
    params: Value,
    ret: Value,
    ret_at: Option<Loc>,
    /// Only a method with a default body has one: in a signature alone, a
    /// string at the end is the result type's place.
    doc: Option<String>,
    body: Vec<(Value, Option<Loc>)>,
}

/// One item of a `deftrait`.
enum Item {
    /// `(:type Name)`.
    Assoc(SymRef),
    /// `(defmethod name (params...) Ret body...)`.
    Method(Written),
}

fn item_of(heap: &Heap, item: Value) -> Result<Item, Error> {
    let elems = heap.list_to_vec_locs(item).map_err(|_| Error::TypeError("deftrait: item must start with a symbol".into()))?;
    let elems = match elems.as_slice() {
        [(Value::Symbol(k), _), rest @ ..] if k.is(wk::KW_TYPE) => {
            return match rest {
                [(Value::Symbol(a), _)] => Ok(Item::Assoc(*a)),
                [_] => Err(Error::TypeError("deftrait: associated type name must be a symbol".into())),
                _ => Err(Error::TypeError("deftrait: (:type AssocName)".into())),
            };
        }
        [(Value::Symbol(d), _), rest @ ..] if d.is(wk::DEFMETHOD) => rest,
        [(Value::Symbol(p), _), (Value::Symbol(d), _), rest @ ..] if p.is(wk::PUB) && d.is(wk::DEFMETHOD) => rest,
        _ => {
            return Err(Error::TypeError(
                "deftrait: a method is written (defmethod name (params...) ReturnType body...), or (pub defmethod ...) to make it public".into(),
            ))
        }
    };
    let [(header, _), (params, _), (ret, ret_at), body @ ..] = elems else {
        return Err(Error::TypeError(
            "deftrait: method must be (defmethod name (params...) ret) or (defmethod name (params...) ret body...)".into(),
        ));
    };
    let (name, own) = parse_header(heap, *header)?;
    let (doc, body) = split_doc(heap, body);
    if let Some((first, _)) = body.first() {
        if crate::typeck::bounds::is_where_clause(heap, *first)? {
            unimplemented!("typeck: a trait method with a `(:where ...)` clause")
        }
    }
    Ok(Item::Method(Written { name, own, params: *params, ret: *ret, ret_at: ret_at.clone(), doc, body: body.to_vec() }))
}

/// The supertrait list `v` of the trait `name`.
fn supertraits(heap: &Heap, tcx: &TypeCx, name: &str, v: Value) -> Result<Vec<TraitBound>, Error> {
    let (reg, ns) = (tcx.reg, tcx.ns);
    let elems = heap
        .list_to_vec_locs(v)
        .map_err(|_| Error::TypeError(format!("deftrait {}: the supertrait list must be a list — write `()` for none", name)))?;
    let mut out: Vec<TraitBound> = Vec::new();
    for (elem, elem_at) in elems {
        // A bare name, or `(Trait (Assoc Type)...)`.
        let (head, head_at, pins) = match elem {
            Value::Cons(_) => {
                let parts = heap.list_to_vec_locs(elem)?;
                (parts[0].0, parts[0].1.clone(), parts[1..].iter().map(|(p, _)| *p).collect())
            }
            _ => (elem, elem_at, Vec::new()),
        };
        let Value::Symbol(sym) = head else {
            return Err(Error::TypeError(format!("deftrait {}: supertrait name must be a symbol", name)));
        };
        let Some((trait_path, sdef)) = trait_named(reg, ns, sym) else {
            return Err(Error::TypeError(format!(
                "deftrait {}: unknown supertrait `{}` — the form is (deftrait Name (Super...) items...), \
                 so the list right after the name is the supertrait list (write `()` for none)",
                name,
                heap.symbol_spelling(sym)
            )));
        };
        if let Some(entry) = reg.types.get(&trait_path) {
            super::type_syntax::note_use(tcx, trait_path, entry, head_at.as_ref());
        }
        let assoc = super::bounds::parse_pins(heap, tcx, "deftrait", trait_path, sdef, &pins)?;
        if let Some(a) = sdef.params[1..].iter().find(|a| !assoc.contains_key(a.name.name())) {
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
        out.push(TraitBound { trait_path, assoc });
    }
    Ok(out)
}
