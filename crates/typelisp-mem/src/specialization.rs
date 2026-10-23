//! The definitions monomorphization makes of a generic one: one for each
//! list of type arguments, named by a symbol that no table holds
//! ([`crate::symbols::unlisted`]).
//!
//! A specialization is identified by what it is of and at which types, never
//! by its name. The name is only what it is shown as, and what its LLVM
//! symbol is made from: nothing reads it back. So the table is the one way
//! from a generic and its type arguments to the specialization's symbol
//! ([`specialization`]), and back ([`specialized`]).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::symbols::{unlisted, NsId, SymRef};
use crate::TypeId;

/// A generic definition: what a specialization is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Generic {
    /// The function the symbol names in the module.
    Fn(NsId, SymRef),
    /// The method named by the last symbol of the type the middle one names
    /// in the module.
    Method(NsId, SymRef, SymRef),
}

impl Generic {
    /// The module the specializations belong to: the function's, or the
    /// type's.
    fn home(&self) -> NsId {
        match self {
            Generic::Fn(ns, _) | Generic::Method(ns, _, _) => *ns,
        }
    }

    /// The name the generic is written with.
    fn written(&self) -> &'static str {
        match self {
            Generic::Fn(_, name) | Generic::Method(_, _, name) => name.name(),
        }
    }
}

#[derive(Default)]
struct Table {
    made: HashMap<(Generic, Vec<TypeId>), SymRef>,
    of: HashMap<SymRef, (Generic, Vec<TypeId>)>,
}

fn table() -> std::sync::MutexGuard<'static, Table> {
    static TABLE: OnceLock<Mutex<Table>> = OnceLock::new();
    TABLE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// The specialization of `generic` at `args`: the same symbol every time it
/// is asked for. Shown as the generic's name and the arguments' types,
/// `unwrap <int>`.
pub fn specialization(generic: Generic, args: &[TypeId]) -> SymRef {
    let key = (generic, args.to_vec());
    if let Some(&sym) = table().made.get(&key) {
        return sym;
    }
    // Spelled before the table is locked again: a type's name is read under
    // the type table's lock, not this one's.
    let shown = format!(
        "{} <{}>",
        generic.written(),
        args.iter().map(|a| crate::type_name(*a).to_string()).collect::<Vec<_>>().join(",")
    );
    let mut t = table();
    if let Some(&sym) = t.made.get(&key) {
        return sym;
    }
    let sym = unlisted(generic.home(), &shown);
    t.of.insert(sym, key.clone());
    t.made.insert(key, sym);
    sym
}

/// What `sym` is the specialization of, and at which types — `None` for a
/// symbol [`specialization`] did not make.
pub fn specialized(sym: SymRef) -> Option<(Generic, Vec<TypeId>)> {
    table().of.get(&sym).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::{child_ns, find_exact, intern_exact};
    use crate::TypeShape;

    #[test]
    fn a_specialization_is_one_symbol_no_table_holds() {
        let m = child_ns(NsId::ROOT, "specialization-test");
        let f = intern_exact(m, "pick");
        let int = crate::intern_type(TypeShape::named(NsId::STD, "int", &[]));
        let s = specialization(Generic::Fn(m, f), &[int]);
        assert_eq!(specialization(Generic::Fn(m, f), &[int]), s);
        assert_eq!(s.name(), "pick <std::int>");
        assert_eq!(s.home(), m);
        assert_eq!(find_exact(m, "pick <std::int>"), None);
        assert_eq!(specialized(s), Some((Generic::Fn(m, f), vec![int])));
        assert_eq!(specialized(f), None);
    }
}
