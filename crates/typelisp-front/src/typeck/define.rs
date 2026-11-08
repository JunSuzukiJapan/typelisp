//! Whether a definition may take its name: the rules every definition
//! form shares.
//!
//! - A reserved word belongs to no module, and a name the standard library
//!   offers every module is taken only after `(shadow name)`.
//! - Functions, global variables and macros share one name space per module,
//!   as types, traits, type aliases and C structs share another: a name has
//!   one meaning in each.
//! - Defining a name again is what the redefinition policy says; a built-in
//!   definition is never replaced.

use crate::check::{MacroExpander, RedefPolicy};
use crate::types::{in_std, Path};
use crate::{Error, Heap};
use typelisp_mem::SymRef;

use super::registry::{Registry, TypeEntry, ValueDef};
use super::use_form::Site;

/// What a definition defines, as its messages call it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Function,
    Variable,
    Type,
    Method,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Function => "function",
            Kind::Variable => "variable",
            Kind::Type => "type",
            Kind::Method => "method",
        }
    }
}

/// Where definitions are checked, where what they warn of goes, and what
/// expands their macro calls.
pub(crate) struct Definer<'w> {
    pub(crate) site: Site,
    pub(crate) warnings: &'w mut Vec<String>,
    /// What expands the macro calls the definitions' bodies make.
    pub(crate) expander: &'w dyn MacroExpander,
}

impl Definer<'_> {
    /// Whether `name`, written by the definition form `who`, may be defined at
    /// all here: not a reserved word, and not a name the standard library
    /// offers every module unless it was shadowed.
    pub(crate) fn ownable(&self, heap: &Heap, name: SymRef, who: &str) -> Result<(), Error> {
        if name.home() == typelisp_mem::NsId::SYSTEM {
            return Err(Error::TypeError(format!("{}: `{}` is a reserved word, which no module can define", who, name.name())));
        }
        if in_std(self.site.ns) || !heap.names().offers(name) {
            return Ok(());
        }
        Err(Error::TypeError(format!(
            "{}: `{}` is a name the standard library offers every module; write `(shadow {})` before defining it here",
            who,
            name.name(),
            name.name()
        )))
    }

    /// Whether the function or the global variable `name` may be defined: its
    /// name space holds nothing else of the name, and a definition of it
    /// already there may be replaced.
    pub(crate) fn value(&mut self, reg: &Registry, kind: Kind, name: SymRef) -> Result<(), Error> {
        let path = Path::of_symbol(self.site.ns, name);
        let (held, builtin) = match reg.values.get(&path) {
            None => return self.redefine(kind, name, None),
            Some(ValueDef::Fn(def)) => (Kind::Function, def.builtin),
            Some(ValueDef::Var(_)) => (Kind::Variable, false),
            Some(ValueDef::Macro { .. }) => {
                return Err(clash(kind, name, "macro"));
            }
        };
        if held != kind {
            return Err(clash(kind, name, held.word()));
        }
        self.redefine(kind, name, Some(builtin))
    }

    /// Whether the type `name` may be defined: no trait, type alias or C
    /// struct of the name is here, and a type of it already here may be
    /// replaced.
    pub(crate) fn type_name(&mut self, reg: &Registry, name: SymRef) -> Result<(), Error> {
        let existing = match reg.types.get(&Path::of_symbol(self.site.ns, name)) {
            None => None,
            Some(TypeEntry::Type(def)) => Some(def.builtin),
            Some(TypeEntry::Trait { .. }) => {
                return Err(Error::TypeError(format!(
                    "cannot define type `{}`: a trait of that name already exists here — types and traits share one name space (write them under different modules if both are needed)",
                    name.name()
                )))
            }
            Some(TypeEntry::Alias { .. }) => {
                return Err(Error::TypeError(format!("cannot define type `{}`: a `deftype` alias of that name already exists here", name.name())))
            }
            Some(TypeEntry::CStruct) => {
                return Err(Error::TypeError(format!("cannot define type `{}`: a `def-c-struct` of that name already exists here", name.name())))
            }
        };
        self.redefine(Kind::Type, name, existing)
    }

    /// Whether the method `method` of `owner` may be defined again, when it
    /// is already.
    pub(crate) fn method(&mut self, reg: &Registry, owner: &Path, method: SymRef) -> Result<(), Error> {
        match reg.method(owner, method) {
            None => Ok(()),
            Some(m) => self.outcome(Kind::Method, method, m.sig.builtin),
        }
    }

    /// The redefinition policy's answer for `name`, a `kind`, already
    /// defined here (`existing` says whether as a built-in) or not.
    fn redefine(&mut self, kind: Kind, name: SymRef, existing: Option<bool>) -> Result<(), Error> {
        if in_std(self.site.ns) {
            unimplemented!("typeck: a definition in the standard library, whose names are one across its modules")
        }
        match existing {
            None => Ok(()),
            Some(builtin) => self.outcome(kind, name, builtin),
        }
    }

    fn outcome(&mut self, kind: Kind, name: SymRef, builtin: bool) -> Result<(), Error> {
        if builtin {
            return Err(Error::TypeError(format!("cannot redefine built-in {} `{}`", kind.word(), name.name())));
        }
        match self.site.policy {
            RedefPolicy::Error => Err(Error::TypeError(format!("{} `{}` is already defined", kind.word(), name.name()))),
            RedefPolicy::Warn => {
                self.warnings.push(format!("warning: redefining {} `{}`", kind.word(), name.name()));
                Ok(())
            }
            RedefPolicy::Silent => Ok(()),
        }
    }
}

/// A `kind` defined under the name of a `held` already here.
fn clash(kind: Kind, name: SymRef, held: &str) -> Error {
    Error::TypeError(format!(
        "cannot define {} `{}`: a {} of that name already exists here — a name has one meaning, \
         whether a function, a variable or a macro",
        kind.word(),
        name.name(),
        held
    ))
}
