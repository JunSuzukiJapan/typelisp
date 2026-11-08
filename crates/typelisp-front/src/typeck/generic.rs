//! Specializations: a generic definition made concrete at the type arguments
//! a call gives it.
//!
//! A generic body is checked once, with its type parameters abstract. What
//! runs is a copy of it per instantiation, lowered with the parameters
//! replaced by the instantiation's types. Each top-level form generates the
//! specializations it needs, so redefining a generic function leaves no copy
//! of the old one behind; the current checker (`crate::check`) works the same
//! way and names them with [`fn_specialization`] too.

use std::collections::HashSet;

use crate::types::{Path, Type};
use crate::Error;
use crate::TypeId;
use typelisp_mem::specialization::{specialization, Generic};
use typelisp_mem::SymRef;

/// Upper bound on the specializations one top-level form generates. A
/// generic function that calls itself at an ever-growing type (`f<T>` calling
/// `f` at `Option<T>`) asks for a new one every time, which the memo cannot
/// stop; hitting this is an error, not a hang.
pub(crate) const SPECIALIZATION_BUDGET: usize = 512;

/// The name of the generic function `base` specialized at `args`.
pub(crate) fn fn_specialization(base: &Path, args: &[Type]) -> Path {
    let ids: Vec<TypeId> = args.iter().map(crate::type_key::type_id).collect();
    Path::of_symbol(base.ns(), specialization(Generic::Fn(base.ns(), base.name_symbol()), &ids))
}

/// The name of `owner`'s method `method` at the type arguments `args`: the
/// owner's, then the method's own. A method written for one instantiation
/// is named this way too, so a call names it the same way whichever `impl`
/// wrote it.
pub(crate) fn method_specialization(owner: &Path, method: SymRef, args: &[Type]) -> SymRef {
    let ids: Vec<TypeId> = args.iter().map(crate::type_key::type_id).collect();
    specialization(Generic::Method(owner.ns(), owner.name_symbol(), method), &ids)
}

/// What is specialized: a function, or a type's method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Base {
    Fn(Path),
    Method(Path, SymRef),
}

/// The name a specialization has: a function's path, or a method's member
/// symbol (of the same type as the generic method).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Name {
    Fn(Path),
    Method(SymRef),
}

/// A specialization one form asked for.
#[derive(Debug)]
pub(crate) struct Request {
    pub(crate) base: Base,
    pub(crate) args: Vec<Type>,
    pub(crate) name: Name,
}

/// The specializations one top-level form asks for, each once.
#[derive(Debug, Default)]
pub(crate) struct Requests {
    asked: HashSet<Name>,
    pending: Vec<Request>,
    taken: usize,
}

impl Requests {
    /// The name the function `base` has specialized at `args`; asked for
    /// the first time, it is queued to be generated.
    pub(crate) fn ask_fn(&mut self, base: &Path, args: Vec<Type>) -> Path {
        let name = fn_specialization(base, &args);
        self.ask(Base::Fn(*base), args, Name::Fn(name));
        name
    }

    /// [`Self::ask_fn`] for `owner`'s method `method`.
    pub(crate) fn ask_method(&mut self, owner: &Path, method: SymRef, args: Vec<Type>) -> SymRef {
        let name = method_specialization(owner, method, &args);
        self.ask(Base::Method(*owner, method), args, Name::Method(name));
        name
    }

    fn ask(&mut self, base: Base, args: Vec<Type>, name: Name) {
        if self.asked.insert(name) {
            self.pending.push(Request { base, args, name });
        }
    }

    /// The next specialization to generate, or `None` when every one asked
    /// for so far has been taken.
    pub(crate) fn next(&mut self) -> Result<Option<Request>, Error> {
        let Some(req) = self.pending.pop() else { return Ok(None) };
        if self.taken >= SPECIALIZATION_BUDGET {
            let last = match (req.base, req.name) {
                (_, Name::Fn(p)) => p.to_string(),
                (Base::Method(owner, _), Name::Method(m)) => format!("{}::{}", owner, m.name()),
                (Base::Fn(_), Name::Method(_)) => unreachable!("a function's specialization is a function"),
            };
            return Err(Error::TypeError(format!(
                "monomorphization did not converge after {} instantiations (a polymorphically \
                 recursive generic function — one that calls itself at an ever-growing type — \
                 cannot be compiled; last requested: {})",
                SPECIALIZATION_BUDGET, last
            )));
        }
        self.taken += 1;
        Ok(Some(req))
    }
}
