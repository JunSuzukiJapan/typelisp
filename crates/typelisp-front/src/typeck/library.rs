//! The standard library's definitions, made what every module reads.
//!
//! `std` imports every definition of the library's modules, and
//! `std::prelude` re-exports what [`prelude_offers`] offers, with the
//! library's types its functions and variables mention: a function a module
//! may call bare has a type the module may write. Each is keyed by the name
//! read in that module, as a `(use PATH)` written there would key it. Done
//! after every form the
//! library defines something in, as the current checker does
//! (`crate::check::Registry::reexport_std_in_prelude`).

use crate::check::registry::{prelude_offers, PRELUDE_MODULES};
use crate::types::{prelude_ns, std_module, Path, Type};
use typelisp_mem::symbols::{intern_in, Import, NsId, SessionNames};

use super::registry::{Registry, ValueDef};

/// Bring the library's definitions into `std` and `std::prelude`.
pub(crate) fn reexport(reg: &mut Registry) {
    let library = |m: NsId| m == NsId::STD || PRELUDE_MODULES.iter().any(|seg| std_module(seg) == m);
    let mut defined: Vec<Path> = reg.values.iter().map(|(p, _)| *p).chain(reg.types.iter().map(|(p, _)| *p)).filter(|p| library(p.ns())).collect();
    defined.extend(reg.ctors.iter().filter(|((m, _), _)| library(*m)).map(|((m, s), _)| Path::of_symbol(*m, *s)));
    let mut offered: Vec<Path> = defined.iter().copied().filter(prelude_offers).collect();
    let mut mentioned = Vec::new();
    for path in &offered {
        match reg.values.get(path) {
            Some(ValueDef::Fn(def)) => {
                for t in def.params.iter().chain(def.rest.as_ref()).chain(std::iter::once(&def.ret)) {
                    mentioned_types(t, &mut mentioned);
                }
            }
            Some(ValueDef::Var(var)) => mentioned_types(&var.ty, &mut mentioned),
            _ => {}
        }
    }
    offered.extend(mentioned.into_iter().filter(|p| defined.contains(p)));
    let prelude = prelude_ns();
    for path in offered {
        let key = (prelude, intern_in(prelude, path.name_symbol().name()));
        if reg.imports.get(&key) != Some(&path) {
            reg.imports.insert(key, path);
        }
        if reg.reexports.get(&key).is_none() {
            reg.reexports.insert(key, ());
        }
    }
    for path in defined.into_iter().filter(|p| p.ns() != NsId::STD) {
        let key = (NsId::STD, intern_in(NsId::STD, path.name_symbol().name()));
        if reg.imports.get(&key) != Some(&path) {
            reg.imports.insert(key, path);
        }
    }
}

/// Tell the session's names what [`reexport`] wrote since the last commit:
/// each import as what it brings in, and each re-export as offered to every
/// module.
pub(crate) fn publish(reg: &Registry, names: &mut SessionNames) {
    for key in reg.imports.written() {
        let Some(target) = reg.imports.get(key) else { continue };
        let what = match reg.type_def(target) {
            Some(_) => Import::Type(target.name_symbol()),
            None => Import::Item(target.name_symbol()),
        };
        names.import_name(key.0, key.1, what);
    }
    for key in reg.reexports.written() {
        if let Some(target) = reg.imports.get(key) {
            names.export(target.name_symbol());
        }
    }
}

/// The named types and traits `t` mentions, its arguments' included.
fn mentioned_types(t: &Type, out: &mut Vec<Path>) {
    match t {
        Type::Named(p, args) | Type::Dyn(p, args) => {
            out.push(*p);
            for a in args {
                mentioned_types(a, out);
            }
        }
        Type::Fn(params, rest, ret) => {
            for p in params.iter().chain(rest.as_deref()).chain(std::iter::once(&**ret)) {
                mentioned_types(p, out);
            }
        }
        Type::PtrTo(inner) => mentioned_types(inner, out),
        _ => {}
    }
}
