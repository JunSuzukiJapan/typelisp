//! What a name resolved to, as the checker hands it to its consumers: a
//! definition with a body of its own ([`CompileTarget`]).

use crate::{Path, SymRef};
/// A definition with a body of its own, named the way the checker resolved
/// it: a free function (or a macro, whose expander is one) by its path, or a
/// method by its type's path and its own name.
///
/// The target of a `(compile name)` / `(compile type::method)` form —
/// `Checker::resolve_callable` builds it out of the one name resolution — and
/// also how the compile pipeline names a node of its call graph and an entry
/// of a precompiled library. Nothing reads a definition back out of a spelled
/// name: a function `b` of module `a` and a method `b` of type `a` are two
/// different values of this type, where their names as text would be easy to
/// confuse.
///
/// Also what a dump records for each body its bitcode carries.
///
/// Ordered by the method's name as it is shown, so a list of these sorts the
/// same way in every run.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CompileTarget {
    Fn(Path),
    Method(Path, #[serde(with = "crate::owned_form::sym_serde")] SymRef),
}

impl PartialOrd for CompileTarget {
    fn partial_cmp(&self, other: &CompileTarget) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CompileTarget {
    fn cmp(&self, other: &CompileTarget) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (CompileTarget::Fn(a), CompileTarget::Fn(b)) => a.cmp(b),
            (CompileTarget::Fn(_), CompileTarget::Method(..)) => Ordering::Less,
            (CompileTarget::Method(..), CompileTarget::Fn(_)) => Ordering::Greater,
            (CompileTarget::Method(a, m), CompileTarget::Method(b, n)) => a.cmp(b).then_with(|| m.name().cmp(n.name())),
        }
    }
}
