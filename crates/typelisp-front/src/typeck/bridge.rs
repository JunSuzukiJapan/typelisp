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

/// How the current checker writes `Self` in a trait's signatures.
const SELF: &str = "Self";

use crate::check::registry::{AdtDef, AdtKind, FnSig, OptKeyParam, TraitBound, Variant};
use crate::check::analyze::MacroShape;
use crate::check::registry::TraitDef as OldTraitDef;
use crate::check::Registry as OldRegistry;
use crate::types::{ParamOwner, Path, Type, TypeParam};
use typelisp_mem::symbols::intern;

use super::registry::{Field, FnDef, MethodDef, Registry, TraitDef, TraitImpl, TypeDef, TypeEntry, ValueDef, VarDef};

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
            let path = Path::of_symbol(ns, sym);
            let var = m.vars.get_sym(sym).expect("a key of the table");
            let ty = convert(&var.ty, &HashMap::new(), &|| format!("the global {}", path));
            new.values.insert(path, ValueDef::Var(VarDef { ty, public: var.public, mutable: var.mutable }));
        }
        for sym in m.macros.symbols() {
            let def = m.macros.get_sym(sym).expect("a key of the table");
            new.values.insert(Path::of_symbol(ns, sym), ValueDef::Macro { public: def.public, shape: MacroShape::of(def) });
        }
        for sym in m.types.symbols() {
            let def = m.types.get_sym(sym).expect("a key of the table");
            import_type(old, def, new);
        }
        for sym in m.type_aliases.symbols() {
            let public = m.type_aliases.get_sym(sym).expect("a key of the table").public;
            new.types.insert(Path::of_symbol(ns, sym), TypeEntry::Alias { public });
        }
        for sym in m.traits.symbols() {
            let def = m.traits.get_sym(sym).expect("a key of the table");
            new.types.insert(Path::of_symbol(ns, sym), TypeEntry::Trait(import_trait(def)));
        }
        for b in &m.blanket_impls {
            new.blanket_impls.insert(b.trait_path, ());
        }
        for sym in m.c_structs.symbols() {
            new.types.insert(Path::of_symbol(ns, sym), TypeEntry::CStruct);
        }
        for sym in m.mod_aliases.symbols() {
            new.module_aliases.insert((ns, sym), *m.mod_aliases.get_sym(sym).expect("a key of the table"));
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
    let variants = match def.kind {
        AdtKind::Sum => def
            .variants
            .iter()
            .map(|v| Variant {
                name: v.name,
                fields: v.fields.iter().map(|t| convert(t, &by_name, &|| format!("a variant of {}", path))).collect(),
            })
            .collect(),
        AdtKind::Struct => Vec::new(),
    };
    let has_defaults = old.struct_defaults.contains_key(&path);
    new.types.insert(
        path,
        TypeEntry::Type(TypeDef {
            kind: def.kind,
            params,
            public: def.public,
            builtin: def.builtin,
            stdlib: def.stdlib,
            fields,
            has_defaults,
            variants,
        }),
    );
    for (name, af) in def.assoc.iter() {
        let sig = fn_def(ParamOwner::Method(path, name), &def.params, &af.sig);
        new.define_method(path, name, MethodDef { sig: FnDef { builtin: af.builtin, ..sig }, instance: af.instance, home: af.home, inst_args: None });
    }
    for t in &def.impls {
        add_impl(new, *t, path, TraitImpl { args: None });
    }
    for inst in def.insts.values() {
        let args: Vec<Type> = inst.args.iter().map(|t| convert(t, &HashMap::new(), &|| format!("an instantiation of {}", path))).collect();
        for t in &inst.impls {
            add_impl(new, *t, path, TraitImpl { args: Some(args.clone()) });
        }
        for (name, af) in inst.assoc.iter() {
            let sig = fn_def(ParamOwner::Method(path, name), &[], &af.sig);
            let method = MethodDef { sig: FnDef { builtin: af.builtin, ..sig }, instance: af.instance, home: af.home, inst_args: Some(args.clone()) };
            new.define_inst_method(path, name, args.clone(), method);
        }
    }
}

/// A trait: `Self` and its associated types are its parameters, in that
/// order.
fn import_trait(def: &OldTraitDef) -> TraitDef {
    let path = def.name;
    let names: Vec<String> = std::iter::once(SELF.to_string()).chain(def.assoc_types.iter().cloned()).collect();
    let params = type_params(ParamOwner::Def(path), &names);
    let by_name: HashMap<String, Type> = names.iter().cloned().zip(params.iter().map(|p| Type::Param(*p))).collect();
    let methods = def
        .method_order
        .iter()
        .map(|m| {
            let sig = def.methods.get(*m).expect("`method_order` lists the trait's methods");
            let own = fn_def(ParamOwner::Def(path), &names, sig);
            (*m, own)
        })
        .collect();
    let supertraits = def
        .supertraits
        .iter()
        .map(|b| TraitBound {
            trait_path: b.trait_path,
            assoc: b.assoc.iter().map(|(k, t)| (k.clone(), convert(t, &by_name, &|| format!("a supertrait of {}", path)))).collect(),
        })
        .collect();
    TraitDef { params, methods, defaults: def.defaults.keys().collect(), supertraits, public: def.public }
}

/// Record `imp`, an `impl` of `trait_path` for `ty`.
fn add_impl(new: &mut Registry, trait_path: Path, ty: Path, imp: TraitImpl) {
    let mut all = new.trait_impls.get(&(trait_path, ty)).cloned().unwrap_or_default();
    all.push(imp);
    new.trait_impls.insert((trait_path, ty), all);
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
