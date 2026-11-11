//! What an editor asks of a checked program (the LSP): the bare names
//! completion offers in a module, where each definition and each local
//! variable a reference names was written, the type of each expression,
//! and the definitions' documentation.
//!
//! The names offered are what [`super::resolve::find_named`] reaches from
//! the module: its own definitions, its child modules and what its `use`s
//! brought in, then the definitions `std::prelude` re-exports, which every
//! module reads without a `use`. An ancestor's definitions are not visible
//! without a `use`, so none are offered.

use std::collections::HashSet;

use crate::check::{CompletionCandidate, CompletionKind, DefLocs, Docs};
use crate::types::{prelude_ns, Path};
use typelisp_mem::symbols::{ns_children, NsId};

use super::expr::Notes;
use super::registry::{Definition, Registry, TypeEntry, ValueDef};

/// Every bare name visible for completion in `here`, ordered by name.
pub(crate) fn candidates(reg: &Registry, here: NsId) -> Vec<CompletionCandidate> {
    let mut out = Vec::new();
    let prelude = prelude_ns();
    let offered: HashSet<Path> = reg
        .reexports
        .iter()
        .filter(|((ns, _), _)| *ns == prelude)
        .map(|(key, _)| *reg.imports.get(key).expect("a re-export is one of the module's imports"))
        .collect();
    // This module's own definitions, and the ones the prelude offers from
    // other modules.
    push_definitions(reg, |p| p.ns() == here || offered.contains(p), &mut out);
    for (name, _) in ns_children(here) {
        out.push(candidate(name, CompletionKind::Module, "module".to_string()));
    }
    for ((_, sym), _) in reg.imports.iter().filter(|((ns, _), _)| *ns == here) {
        out.push(candidate(sym.name(), CompletionKind::Function, "use".to_string()));
    }
    for ((_, sym), _) in reg.module_aliases.iter().filter(|((ns, _), _)| *ns == here) {
        out.push(candidate(sym.name(), CompletionKind::Module, "use".to_string()));
    }
    for ((_, sym), _) in reg.static_uses.iter().filter(|((ns, _), _)| *ns == here) {
        out.push(candidate(sym.name(), CompletionKind::Method, "use".to_string()));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Push the names of the definitions whose path `keep` accepts into `out`.
fn push_definitions(reg: &Registry, keep: impl Fn(&Path) -> bool, out: &mut Vec<CompletionCandidate>) {
    for (path, def) in reg.values.iter().filter(|(p, _)| keep(p)) {
        let name = path.name_symbol().name();
        out.push(match def {
            ValueDef::Fn(def) => candidate(name, CompletionKind::Function, format!("({:?}) -> {:?}", def.params, def.ret)),
            ValueDef::Var(def) => candidate(name, CompletionKind::Variable, format!("{:?}", def.ty)),
            ValueDef::Macro { .. } => candidate(name, CompletionKind::Macro, "macro".to_string()),
        });
    }
    for (path, entry) in reg.types.iter().filter(|(p, _)| keep(p)) {
        let name = path.name_symbol().name();
        out.push(match entry {
            TypeEntry::Trait(_) => candidate(name, CompletionKind::Trait, "trait".to_string()),
            TypeEntry::Type(_) | TypeEntry::Alias { .. } | TypeEntry::CStruct(_) => candidate(name, CompletionKind::Type, "type".to_string()),
        });
    }
    for ((_, sym), _) in reg.ctors.iter().filter(|((ns, sym), _)| keep(&Path::of_symbol(*ns, *sym))) {
        out.push(candidate(sym.name(), CompletionKind::Function, "constructor".to_string()));
    }
}

fn candidate(name: &str, kind: CompletionKind, detail: String) -> CompletionCandidate {
    CompletionCandidate { name: name.to_string(), kind, detail }
}

/// Where each definition `reg` holds was written, and what `notes` say of
/// each local variable reference and expression.
pub(crate) fn def_locs(reg: &Registry, notes: &Notes) -> DefLocs {
    let mut out = DefLocs::default();
    for (def, loc) in reg.locs.iter() {
        let loc = loc.clone();
        match *def {
            Definition::Fn(p) => out.fns.insert(p, loc),
            Definition::Macro(p) => out.macros.insert(p, loc),
            Definition::Var(p) => out.vars.insert(p, loc),
            Definition::Type(p) => out.types.insert(p, loc),
            Definition::Trait(p) => out.traits.insert(p, loc),
            Definition::Method(p, m) => out.methods.insert((p, m), loc),
            // A reference names a trait's method only through a receiver,
            // whose type's method is the one it reaches.
            Definition::TraitMethod(..) => None,
        };
    }
    out.local_refs = notes.local_refs.borrow().clone();
    out.node_types = notes.node_types.borrow().clone();
    out
}

/// The documentation of each definition `reg` holds.
pub(crate) fn docs(reg: &Registry) -> Docs {
    let mut out = Docs::default();
    for (def, text) in reg.docs.iter() {
        let text = text.clone();
        match *def {
            Definition::Fn(p) => out.fns.insert(p, text),
            Definition::Macro(p) => out.macros.insert(p, text),
            Definition::Var(p) => out.vars.insert(p, text),
            Definition::Type(p) => out.types.insert(p, text),
            Definition::Trait(p) => out.traits.insert(p, text),
            Definition::Method(p, m) => out.methods.insert((p, m), text),
            Definition::TraitMethod(p, m) => out.trait_methods.insert((p, m), text),
        };
    }
    out
}
