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

/// A specialization one form asked for.
#[derive(Debug)]
pub(crate) struct Request {
    pub(crate) base: Path,
    pub(crate) args: Vec<Type>,
    pub(crate) name: Path,
}

/// The specializations one top-level form asks for, each once.
#[derive(Debug, Default)]
pub(crate) struct Requests {
    asked: HashSet<Path>,
    pending: Vec<Request>,
    taken: usize,
}

impl Requests {
    /// The name the specialization of `base` at `args` has; asked for the
    /// first time, it is queued to be generated.
    pub(crate) fn ask(&mut self, base: &Path, args: Vec<Type>) -> Path {
        let name = fn_specialization(base, &args);
        if self.asked.insert(name) {
            self.pending.push(Request { base: *base, args, name });
        }
        name
    }

    /// The next specialization to generate, or `None` when every one asked
    /// for so far has been taken.
    pub(crate) fn next(&mut self) -> Result<Option<Request>, Error> {
        let Some(req) = self.pending.pop() else { return Ok(None) };
        if self.taken >= SPECIALIZATION_BUDGET {
            return Err(Error::TypeError(format!(
                "monomorphization did not converge after {} instantiations (a polymorphically \
                 recursive generic function — one that calls itself at an ever-growing type — \
                 cannot be compiled; last requested: {})",
                SPECIALIZATION_BUDGET, req.name
            )));
        }
        self.taken += 1;
        Ok(Some(req))
    }
}
