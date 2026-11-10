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
use crate::check::registry::PRELUDE_MODULES;
use crate::types::{in_std, std_module, Path, Type};
use crate::{Error, Heap};
use typelisp_mem::symbols::NsId;
use typelisp_mem::SymRef;

use super::registry::{Registry, TypeEntry, ValueDef};
use super::use_form::Site;

/// What a definition defines, as its messages call it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Function,
    Variable,
    Type,
    Trait,
    Alias,
    CStruct,
    Method,
    Macro,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Function => "function",
            Kind::Variable => "variable",
            Kind::Type => "type",
            Kind::Trait => "trait",
            Kind::Alias => "type alias",
            Kind::CStruct => "C struct",
            Kind::Method => "method",
            Kind::Macro => "macro",
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
    /// The definition was written `(pub ...)`: visible outside its module.
    pub(crate) public: bool,
}

impl Definer<'_> {
    /// The symbol `name`, written by the definition form `who`, defines here:
    /// the module's own symbol of its spelling, which is `name` itself unless
    /// a macro of another module wrote it. Refused for a reserved word, and
    /// for a name the standard library offers every module unless it was
    /// shadowed.
    pub(crate) fn ownable(&self, heap: &Heap, name: SymRef, who: &str) -> Result<SymRef, Error> {
        if name.home() == typelisp_mem::NsId::SYSTEM {
            return Err(Error::TypeError(format!("{}: `{}` is a reserved word, which no module can define", who, name.name())));
        }
        if in_std(self.site.ns) || !heap.names().offers(name) {
            return Ok(typelisp_mem::symbols::intern_in(self.site.ns, name.name()));
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
            None => return self.redefine(reg, kind, name, None),
            Some(ValueDef::Fn(def)) => (Kind::Function, def.builtin),
            Some(ValueDef::Var(_)) => (Kind::Variable, false),
            Some(ValueDef::Macro { .. }) => (Kind::Macro, false),
        };
        if held != kind {
            return Err(clash(kind, name, held.word()));
        }
        self.redefine(reg, kind, name, Some(builtin))
    }

    /// Whether the type or the trait `name`, as `kind` says, may be
    /// defined: types and traits share one name space with type aliases and
    /// C structs, so nothing else of the name is here, and a definition of
    /// the same kind already here may be replaced.
    pub(crate) fn type_name(&mut self, reg: &Registry, kind: Kind, name: SymRef) -> Result<(), Error> {
        let clash = |other: &str| {
            Error::TypeError(format!(
                "cannot define {} `{}`: a {} of that name already exists here — types and traits share one name space (write them under different modules if both are needed)",
                kind.word(),
                name.name(),
                other
            ))
        };
        let existing = match (reg.types.get(&Path::of_symbol(self.site.ns, name)), kind) {
            (None, _) => None,
            (Some(TypeEntry::Type(def)), Kind::Type) => Some(def.builtin),
            (Some(TypeEntry::Trait(_)), Kind::Trait) => Some(false),
            (Some(TypeEntry::Alias { .. }), Kind::Alias) => Some(false),
            (Some(TypeEntry::CStruct(_)), Kind::CStruct) => Some(false),
            (Some(TypeEntry::Type(_)), _) => return Err(clash("type")),
            (Some(TypeEntry::Trait(_)), _) => return Err(clash("trait")),
            (Some(TypeEntry::Alias { .. }), _) => {
                return Err(Error::TypeError(format!(
                    "cannot define {} `{}`: a `deftype` alias of that name already exists here",
                    kind.word(),
                    name.name()
                )))
            }
            (Some(TypeEntry::CStruct(_)), _) => {
                return Err(Error::TypeError(format!(
                    "cannot define {} `{}`: a `def-c-struct` of that name already exists here",
                    kind.word(),
                    name.name()
                )))
            }
        };
        self.redefine(reg, kind, name, existing)
    }

    /// Whether the method `method` of `owner` may be defined again, when it
    /// is already.
    pub(crate) fn method(&mut self, reg: &Registry, owner: &Path, method: SymRef) -> Result<(), Error> {
        match reg.method(owner, method) {
            None => Ok(()),
            Some(m) => self.outcome(Kind::Method, method, m.sig.builtin),
        }
    }

    /// Whether the method `method` of `owner` may be defined for the
    /// instantiation `args`, when the type already has one for it or for
    /// every instantiation.
    pub(crate) fn inst_method(&mut self, reg: &Registry, owner: &Path, method: SymRef, args: &[Type]) -> Result<(), Error> {
        match reg.method_at(owner, method, args) {
            None => Ok(()),
            Some(m) => self.outcome(Kind::Method, method, m.sig.builtin),
        }
    }

    /// The redefinition policy's answer for `name`, a `kind`, already
    /// defined here (`existing` says whether as a built-in) or not.
    fn redefine(&mut self, reg: &Registry, kind: Kind, name: SymRef, existing: Option<bool>) -> Result<(), Error> {
        if let Some(other) = std_elsewhere(reg, self.site.ns, name) {
            return Err(Error::TypeError(format!(
                "cannot define {} `{}` here: the standard library already defines `{}`",
                kind.word(),
                name.name(),
                other
            )));
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

/// Another module of the standard library that defines `name`, when `ns`
/// is one of its modules: a name has one definition across the modules
/// `std::prelude` offers, and a type name one across all of them.
fn std_elsewhere(reg: &Registry, ns: NsId, name: SymRef) -> Option<Path> {
    if !in_std(ns) {
        return None;
    }
    let library = |m: NsId| m == NsId::STD || PRELUDE_MODULES.iter().any(|seg| std_module(seg) == m);
    let elsewhere = |p: &Path, within: &dyn Fn(NsId) -> bool| p.ns() != ns && within(p.ns()) && p.name_symbol().name() == name.name();
    if library(ns) {
        let value = reg.values.iter().map(|(p, _)| *p).find(|p| elsewhere(p, &library));
        let ty = || reg.types.iter().map(|(p, _)| *p).find(|p| elsewhere(p, &library));
        let ctor = || reg.ctors.iter().find(|((m, s), _)| *m != ns && library(*m) && s.name() == name.name()).map(|((m, s), _)| Path::of_symbol(*m, *s));
        if let Some(p) = value.or_else(ty).or_else(ctor) {
            return Some(p);
        }
    }
    reg.types
        .iter()
        .find(|(p, entry)| elsewhere(p, &in_std) && !matches!(entry, TypeEntry::Alias { .. }))
        .map(|(p, _)| *p)
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
