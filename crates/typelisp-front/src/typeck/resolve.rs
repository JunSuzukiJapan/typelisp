//! Name resolution: what a name written in a module means
//! (`docs/dev/name-resolution.md`, `docs/dev/explicit-symbols-plan.md` 3.3).
//!
//! A module sees what it defines and what it brings in. No enclosing module
//! is consulted.

use crate::typeck::sexpr_value::is_option_of_sexpr;
use crate::types::{in_std, nominal_path, prelude_ns, Path, Type};
use typelisp_mem::symbols::NsId;
use typelisp_mem::SymRef;

use super::registry::{BlanketImpl, MethodDef, Registry, TraitDef, TypeEntry, ValueDef};
use crate::Error;

/// What the bare name `sym`, read in the module `here`, names as `lookup`
/// finds it among one module's definitions (`lookup` is given the module and
/// the symbol to look up there), with the definition's path.
///
/// In order: the module's own definition; what a `use` there brought in
/// under the name; what the name means in the module the symbol belongs to,
/// its definition or its import there (a name a macro's expansion carried in,
/// or one read as the definition `std::prelude` re-exports); in a module of
/// the standard library, what `std` imports under it; what `std::prelude`
/// re-exports under it. An import of something `lookup` does not find leaves
/// the search going.
pub(crate) fn find_named<T>(reg: &Registry, here: NsId, sym: SymRef, lookup: impl Fn(NsId, SymRef) -> Option<T>) -> Option<(Path, T)> {
    let at = |ns: NsId, key: SymRef| lookup(ns, key).map(|v| (Path::of_symbol(ns, key), v));
    let through = |ns: NsId, key: SymRef| reg.imports.get(&(ns, key)).and_then(|target| at(target.ns(), target.name_symbol()));
    if let Some(found) = at(here, sym).or_else(|| through(here, sym)) {
        return Some(found);
    }
    // A member of the shared vocabulary belongs to every module alike, so
    // where it was made says nothing about which definition it names.
    if sym.home() != here && !sym.is_shared_vocabulary() {
        if let Some(found) = at(sym.home(), sym).or_else(|| through(sym.home(), sym)) {
            return Some(found);
        }
    }
    if in_std(here) {
        if let Some(found) = through(NsId::STD, sym) {
            return Some(found);
        }
    }
    let prelude = prelude_ns();
    reg.reexports.get(&(prelude, sym))?;
    through(prelude, sym)
}

/// What the path `p` names: `p` itself, or, when `p`'s module re-exports
/// the name (`(pub use ...)`), what the re-export names, followed to the
/// definition.
pub(crate) fn reexported(reg: &Registry, p: Path) -> Path {
    let key = (p.ns(), p.name_symbol());
    match (reg.reexports.get(&key), reg.imports.get(&key)) {
        (Some(()), Some(target)) => reexported(reg, *target),
        _ => p,
    }
}

/// The trait the bare name `sym` names in `here`.
pub(crate) fn trait_named(reg: &Registry, here: NsId, sym: SymRef) -> Option<(Path, &TraitDef)> {
    let (path, entry) = find_named(reg, here, sym, |ns, key| reg.types.get(&Path::of_symbol(ns, key)))?;
    match entry {
        TypeEntry::Trait(def) => Some((path, def)),
        _ => None,
    }
}

/// The function, global variable or macro the bare name `sym` names in `here`.
pub(crate) fn value_named<'r>(reg: &'r Registry, here: NsId, sym: SymRef) -> Option<(Path, &'r ValueDef)> {
    find_named(reg, here, sym, |ns, key| reg.values.get(&Path::of_symbol(ns, key)))
}

/// The variant a `use` of its type brought in under the bare name `sym`.
pub(crate) fn ctor_named(reg: &Registry, here: NsId, sym: SymRef) -> Option<(Path, usize)> {
    find_named(reg, here, sym, |ns, key| reg.ctors.get(&(ns, key)).copied()).map(|(_, c)| c)
}

/// The static method a `use` of its type brought in under the bare name `sym`.
pub(crate) fn static_use_named(reg: &Registry, here: NsId, sym: SymRef) -> Option<(Path, SymRef)> {
    find_named(reg, here, sym, |ns, key| reg.static_uses.get(&(ns, key)).copied()).map(|(_, s)| s)
}

/// The instance method `name` (a member symbol) a receiver of type `ty`
/// answers to, seen from `here`, with the type it belongs to.
///
/// A method without `pub` is seen as [`sees`] says, from the module it was
/// written in.
///
/// An `Option<Sexpr>` is the type an S-expression has, so it answers to
/// `Sexpr`'s instance methods too, whose signatures take it; what `Sexpr`
/// does not have is `Option`'s.
pub(crate) fn receiver_method<'r>(reg: &'r Registry, here: NsId, ty: &Type, name: SymRef) -> Result<Option<(Path, &'r MethodDef)>, Error> {
    if is_option_of_sexpr(ty) {
        let sexpr = Path::builtin(typelisp_mem::wk::SEXPR);
        if let Some(m) = reg.method(&sexpr, name).filter(|m| m.instance) {
            return Ok(Some((sexpr, m)));
        }
    }
    let Some(owner) = nominal_path(ty) else { return Ok(None) };
    let args = match ty {
        Type::Named(_, args) => args.as_slice(),
        _ => &[],
    };
    if reg.has_inst_method(&owner, name) && args.iter().any(|a| {
        let mut open = false;
        super::unify::walk(a, &mut |t| open |= matches!(t, Type::Infer(_)));
        open
    }) {
        return Err(Error::TypeError(format!(
            "`{}` is a method of some instantiations of `{}`, and the receiver's type `{}` does not say which yet — say it, e.g. with `(the {}<...> ...)`",
            name.name(),
            typelisp_print::shared::name_view().definition(owner.ns(), owner.name_symbol()),
            ty,
            typelisp_print::shared::name_view().definition(owner.ns(), owner.name_symbol())
        )));
    }
    let Some(m) = reg.method_at(&owner, name, args) else {
        return blanket_method(reg, ty, name);
    };
    let home = m.home.map_or(owner.ns(), |h| h.ns());
    Ok((m.instance && (m.sig.public || sees(here, home))).then_some((owner, m)))
}

/// The method `name` the blanket `impl` covering `ty` gives it: under the
/// blanket `impl`'s owner, to be specialized at `ty`. Two that cover `ty`
/// both give it one, and which is meant is not said (Rust's "multiple
/// applicable items").
fn blanket_method<'r>(reg: &'r Registry, ty: &Type, name: SymRef) -> Result<Option<(Path, &'r MethodDef)>, Error> {
    let covering: Vec<(Path, &BlanketImpl)> = reg
        .blankets_with_method(name)
        .into_iter()
        .filter(|(trait_path, _)| super::bounds::blanket_covers(reg, ty, &crate::typeck::registry::TraitBound { trait_path: *trait_path, assoc: Default::default() }))
        .collect();
    match covering.as_slice() {
        [] => Ok(None),
        [(_, blanket)] => Ok(reg.method(&blanket.owner, name).filter(|m| m.instance).map(|m| (blanket.owner, m))),
        [(a, _), (b, _), ..] => {
            let view = typelisp_print::shared::name_view();
            let (mut a, mut b) = (view.definition(a.ns(), a.name_symbol()), view.definition(b.ns(), b.name_symbol()));
            if b < a {
                std::mem::swap(&mut a, &mut b);
            }
            Err(Error::TypeError(format!(
                "`{}` on a `{}`: the blanket `impl`s of `{}` and `{}` both give it one, and nothing says which is meant",
                name.name(),
                ty,
                a,
                b
            )))
        }
    }
}

/// Whether what the module `home` defines without `pub` is visible from
/// `here`: only from that module itself, and within the standard library
/// from any of its modules. An enclosing or an enclosed module is another
/// module.
pub(crate) fn sees(here: NsId, home: NsId) -> bool {
    home == here || (in_std(here) && in_std(home))
}
