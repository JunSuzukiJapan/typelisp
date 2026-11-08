//! The current checker's registry copied into the rebuilt one
//! (`docs/dev/checker-rebuild-plan.md` 8.3).
//!
//! While the two checkers stand side by side, the built-in definitions and
//! the prelude (read from its dump) are in the current registry only. The
//! rebuilt checker copies them before it checks its first form, turning each
//! type variable written by name (`Type::Var`) into the position it has
//! (`Type::Param`). This module goes when the switch is made: the dump and
//! the built-in definitions are then written in the rebuilt registry's form.

use std::collections::HashMap;

use crate::check::registry::{AdtDef, AdtKind, FnSig, OptKeyParam, TraitBound};
use crate::check::Registry as OldRegistry;
use crate::types::{ParamOwner, Path, Type, TypeParam};
use typelisp_mem::symbols::intern;

use super::registry::{Field, FnDef, MethodDef, Registry, TypeDef, TypeEntry, ValueDef};

/// Copy everything `old` defines into `new`.
pub(crate) fn import(old: &OldRegistry, new: &mut Registry) {
    for m in old.namespaces() {
        let ns = m.ns;
        for sym in m.fns.symbols() {
            let path = Path::of_symbol(ns, sym);
            let sig = m.fns.get_sym(sym).expect("a key of the table");
            new.values.insert(path, ValueDef::Fn(fn_def(ParamOwner::Def(path), &[], sig)));
        }
        for sym in m.vars.symbols() {
            new.values.insert(Path::of_symbol(ns, sym), ValueDef::Var);
        }
        for sym in m.macros.symbols() {
            new.values.insert(Path::of_symbol(ns, sym), ValueDef::Macro);
        }
        for sym in m.types.symbols() {
            let def = m.types.get_sym(sym).expect("a key of the table");
            import_type(old, def, new);
        }
        for sym in m.type_aliases.symbols() {
            new.types.insert(Path::of_symbol(ns, sym), TypeEntry::Alias);
        }
        for sym in m.traits.symbols() {
            new.types.insert(Path::of_symbol(ns, sym), TypeEntry::Trait);
        }
        for sym in m.c_structs.symbols() {
            new.types.insert(Path::of_symbol(ns, sym), TypeEntry::CStruct);
        }
        for sym in m.aliases.symbols() {
            new.imports.insert((ns, sym), *m.aliases.get_sym(sym).expect("a key of the table"));
        }
        for sym in m.reexports.symbols() {
            new.reexports.insert((ns, sym), ());
        }
        for sym in m.ctors.symbols() {
            new.ctors.insert((ns, sym), *m.ctors.get_sym(sym).expect("a key of the table"));
        }
        for sym in m.static_uses.symbols() {
            new.static_uses.insert((ns, sym), *m.static_uses.get_sym(sym).expect("a key of the table"));
        }
    }
}

/// A type and its methods.
fn import_type(old: &OldRegistry, def: &AdtDef, new: &mut Registry) {
    let path = def.name;
    let params = type_params(ParamOwner::Def(path), &def.params);
    let by_name: HashMap<String, Type> = def.params.iter().cloned().zip(params.iter().map(|p| Type::Param(*p))).collect();
    let fields = match (def.kind, def.variants.as_slice()) {
        (AdtKind::Struct, [v]) => def
            .field_names
            .iter()
            .zip(&v.fields)
            .map(|(name, ty)| Field {
                name: *name,
                ty: convert(ty, &by_name, &|| format!("a field of {}", path)),
                public: def.assoc.get(*name).is_some_and(|af| af.sig.public),
            })
            .collect(),
        _ => Vec::new(),
    };
    let has_defaults = old.struct_defaults.contains_key(&path);
    new.types.insert(
        path,
        TypeEntry::Type(TypeDef { kind: def.kind, params, public: def.public, builtin: def.builtin, fields, has_defaults }),
    );
    for (name, af) in def.assoc.iter() {
        let sig = fn_def(ParamOwner::Method(path, name), &def.params, &af.sig);
        new.methods.insert((path, name), MethodDef { sig: FnDef { builtin: af.builtin, ..sig }, instance: af.instance, home: af.home, inst_args: None });
    }
    for inst in def.insts.values() {
        let args: Vec<Type> = inst.args.iter().map(|t| convert(t, &HashMap::new(), &|| format!("an instantiation of {}", path))).collect();
        for (name, af) in inst.assoc.iter() {
            let sig = fn_def(ParamOwner::Method(path, name), &[], &af.sig);
            let method = MethodDef { sig: FnDef { builtin: af.builtin, ..sig }, instance: af.instance, home: af.home, inst_args: Some(args.clone()) };
            let mut insts = new.inst_methods.get(&(path, name)).cloned().unwrap_or_default();
            insts.push((args.clone(), method));
            new.inst_methods.insert((path, name), insts);
        }
    }
}

/// `owner`'s parameters, named `names` in order.
fn type_params(owner: ParamOwner, names: &[String]) -> Vec<TypeParam> {
    names.iter().enumerate().map(|(i, n)| TypeParam { owner, index: i as u32, name: intern(n) }).collect()
}

/// `sig`, owned by `owner`, whose type parameters are `outer` (a method's
/// type's, written in the names the type declared) followed by its own.
fn fn_def(owner: ParamOwner, outer: &[String], sig: &FnSig) -> FnDef {
    let names: Vec<String> = outer.iter().chain(&sig.type_params).cloned().collect();
    let params = type_params(owner, &names);
    let by_name: HashMap<String, Type> = names.iter().cloned().zip(params.iter().map(|p| Type::Param(*p))).collect();
    let what = || format!("{:?}'s signature", owner);
    let conv = |t: &Type| convert(t, &by_name, &what);
    let bounds = sig
        .bounds
        .iter()
        .map(|(name, bs)| {
            let Some(Type::Param(p)) = by_name.get(name) else {
                panic!("bridge: {} bounds `{}`, which is none of its type parameters", what(), name)
            };
            let bs = bs
                .iter()
                .map(|b| TraitBound { trait_path: b.trait_path, assoc: b.assoc.iter().map(|(k, t)| (k.clone(), conv(t))).collect() })
                .collect();
            (*p, bs)
        })
        .collect();
    let opt = |o: &OptKeyParam| OptKeyParam { decl_ty: conv(&o.decl_ty), ..o.clone() };
    FnDef {
        type_params: params,
        params: sig.params.iter().map(conv).collect(),
        ret: conv(&sig.ret),
        public: sig.public,
        builtin: sig.builtin,
        ffi: sig.ffi,
        rest: sig.rest.as_ref().map(conv),
        optionals: sig.optionals.iter().map(opt).collect(),
        keys: sig.keys.iter().map(opt).collect(),
        bounds,
    }
}

/// `t` with each type variable replaced by its parameter in `by_name`. A
/// variable that is none of them is an internal error: `what` says where.
fn convert(t: &Type, by_name: &HashMap<String, Type>, what: &dyn Fn() -> String) -> Type {
    let each = |ts: &[Type]| ts.iter().map(|t| convert(t, by_name, what)).collect::<Vec<_>>();
    match t {
        Type::Var(name) => match by_name.get(name) {
            Some(p) => p.clone(),
            None => panic!("bridge: {} mentions the type variable `{}`, which it does not declare", what(), name),
        },
        Type::Named(p, args) => Type::Named(*p, each(args)),
        Type::Dyn(p, pins) => Type::Dyn(*p, each(pins)),
        Type::Fn(ps, rest, ret) => {
            Type::Fn(each(ps), rest.as_ref().map(|r| Box::new(convert(r, by_name, what))), Box::new(convert(ret, by_name, what)))
        }
        Type::PtrTo(inner) => Type::PtrTo(Box::new(convert(inner, by_name, what))),
        other => other.clone(),
    }
}
