//! Name resolution: what a name written in a module means
//! (`docs/dev/name-resolution.md`, `docs/dev/explicit-symbols-plan.md` 3.3).
//!
//! A module sees what it defines and what it brings in. No enclosing module
//! is consulted.

use crate::types::{in_std, nominal_path, prelude_ns, Path, Type};
use typelisp_mem::symbols::NsId;
use typelisp_mem::SymRef;

use super::registry::{MethodDef, Registry, ValueDef};

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
pub(crate) fn receiver_method<'r>(reg: &'r Registry, here: NsId, ty: &Type, name: SymRef) -> Option<(Path, &'r MethodDef)> {
    let owner = nominal_path(ty)?;
    let args = match ty {
        Type::Named(_, args) => args.as_slice(),
        _ => &[],
    };
    let m = reg.method_at(&owner, name, args)?;
    let home = m.home.map_or(owner.ns(), |h| h.ns());
    (m.instance && (m.sig.public || sees(here, home))).then_some((owner, m))
}

/// Whether what the module `home` defines without `pub` is visible from
/// `here`: only from that module itself, and within the standard library
/// from any of its modules. An enclosing or an enclosed module is another
/// module.
pub(crate) fn sees(here: NsId, home: NsId) -> bool {
    home == here || (in_std(here) && in_std(home))
}
