//! What the checker's registry holds, written into a dump's unit
//! (`docs/dev/checker-rebuild-plan.md` 8.3): what the unit defined, and the
//! generic bodies it checked, so the prelude's are checked once, when its
//! dump is made, and never by a session.
//!
//! A tree's values live on the heap that checked it. They are written as
//! [`OwnedForm`]s beside the trees, in the order [`Typed::each_value`]
//! visits them, and put back into the trees in the same order when the dump
//! is read: a field of a tree holding a value is written as nothing
//! ([`slot`]).

use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::owned_form::{owned_to_value, value_to_owned, OwnedForm};
use crate::types::{Path, Type};
use crate::{Error, Heap, Value};
use typelisp_mem::SymRef;

use super::generic::Base;
use super::registry::{BlanketImpl, Definition, MethodDef, Registry, Template, TraitImpl, TypeEntry, ValueDef};
use super::tree::Typed;
use typelisp_mem::symbols::NsId;

/// A value field of a tree, written as nothing and read back as `()`,
/// which [`RebuiltBodies::restore`] then replaces.
pub(crate) mod slot {
    use crate::Value;

    pub(crate) fn serialize<S: serde::Serializer>(_: &Value, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_unit()
    }

    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Value, D::Error> {
        <() as serde::Deserialize>::deserialize(d)?;
        Ok(Value::Empty)
    }

    /// An optional value field: whether there is one is written.
    pub(crate) mod opt {
        use crate::Value;

        pub(crate) fn serialize<S: serde::Serializer>(v: &Option<Value>, s: S) -> Result<S::Ok, S::Error> {
            serde::Serialize::serialize(&v.map(|_| ()), s)
        }

        pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
            let present: Option<()> = serde::Deserialize::deserialize(d)?;
            Ok(present.map(|()| Value::Empty))
        }
    }
}

/// A list of names, each with something, the names written as symbols.
pub(crate) mod sym_pairs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::owned_form::OwnedSym;
    use typelisp_mem::SymRef;

    pub(crate) fn serialize<T: Serialize, S: Serializer>(pairs: &[(SymRef, T)], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(pairs.iter().map(|(name, t)| (OwnedSym::of(*name), t)))
    }

    pub(crate) fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(d: D) -> Result<Vec<(SymRef, T)>, D::Error> {
        let pairs: Vec<(OwnedSym, T)> = Deserialize::deserialize(d)?;
        Ok(pairs.into_iter().map(|(name, t)| (name.symbol(), t)).collect())
    }
}

/// A list of names, written as symbols.
pub(crate) mod syms {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::owned_form::OwnedSym;
    use typelisp_mem::SymRef;

    pub(crate) fn serialize<S: Serializer>(names: &[SymRef], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(names.iter().map(|name| OwnedSym::of(*name)))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<SymRef>, D::Error> {
        let names: Vec<OwnedSym> = Deserialize::deserialize(d)?;
        Ok(names.into_iter().map(|name| name.symbol()).collect())
    }
}

/// A symbol in a key of the registry, written as its [`crate::owned_form::OwnedSym`].
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) struct Sym(#[serde(with = "crate::owned_form::sym_serde")] pub(crate) SymRef);

/// A module in a key of the registry, written as its path's segments.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) struct Ns(#[serde(with = "ns_serde")] pub(crate) NsId);

mod ns_serde {
    use typelisp_mem::symbols::{ns_of, ns_path, NsId};

    pub(crate) fn serialize<S: serde::Serializer>(ns: &NsId, s: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&ns_path(*ns), s)
    }

    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<NsId, D::Error> {
        let segments: Vec<String> = serde::Deserialize::deserialize(d)?;
        Ok(ns_of(&segments))
    }
}

/// `entries` in the order of their keys as written, so the same registry is
/// written the same way every time.
pub(crate) fn sorted<K: Serialize, V>(mut entries: Vec<(K, V)>) -> Vec<(K, V)> {
    entries.sort_by_cached_key(|(k, _)| bincode::serialize(k).expect("a registry key serializes"));
    entries
}

/// The entries of a registry a unit wrote, each with what it holds
/// (`None`: removed), table by table ([`Registry::changed_since`]).
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct RegistryDelta {
    pub(crate) values: Vec<(Path, Option<ValueDef>)>,
    pub(crate) types: Vec<(Path, Option<TypeEntry>)>,
    pub(crate) methods: Vec<((Path, Sym), Option<MethodDef>)>,
    pub(crate) inst_methods: Vec<((Path, Sym), Option<Vec<(Vec<Type>, MethodDef)>>)>,
    pub(crate) method_names: Vec<(Sym, Option<()>)>,
    pub(crate) imports: Vec<((Ns, Sym), Option<Path>)>,
    pub(crate) reexports: Vec<((Ns, Sym), Option<()>)>,
    pub(crate) ctors: Vec<((Ns, Sym), Option<(Path, usize)>)>,
    pub(crate) static_uses: Vec<((Ns, Sym), Option<(Path, Sym)>)>,
    pub(crate) module_aliases: Vec<((Ns, Sym), Option<Ns>)>,
    pub(crate) modules: Vec<(Ns, Option<()>)>,
    pub(crate) trait_impls: Vec<((Path, Path), Option<Vec<TraitImpl>>)>,
    pub(crate) blanket_impls: Vec<(Path, Option<BlanketImpl>)>,
    pub(crate) blanket_owners: Vec<(Path, Option<Path>)>,
    pub(crate) docs: Vec<(Definition, Option<String>)>,
    pub(crate) predeclared: Vec<(Path, Option<()>)>,
    pub(crate) throw_tags: Vec<(Sym, Option<Type>)>,
    pub(crate) callbacks_lifted: u64,
    pub(crate) local_macros: u64,
}

/// What a unit wrote of the checker's registry: its entries and
/// its generic bodies.
#[derive(Default, Serialize, Deserialize)]
pub struct RebuiltUnit {
    registry: RegistryDelta,
    bodies: RebuiltBodies,
}

impl RebuiltUnit {
    /// What `reg` was written since the generation `since`, its values read
    /// out of `heap`.
    pub(crate) fn of(heap: &Heap, reg: &Registry, since: u64) -> Result<RebuiltUnit, Error> {
        Ok(RebuiltUnit { registry: reg.changed_since(since), bodies: RebuiltBodies::of(heap, reg, since)? })
    }

    /// The unit, its bodies' values allocated on `heap` and rooted for good.
    pub(crate) fn restore(self, heap: &mut Heap) -> Result<Unit, Error> {
        Ok(Unit { registry: self.registry, bodies: self.bodies.restore(heap)? })
    }
}

/// A unit a dump wrote, read back.
#[derive(Debug, Default)]
pub(crate) struct Unit {
    registry: RegistryDelta,
    bodies: Bodies,
}

impl Unit {
    /// Into `reg`, uncommitted.
    pub(crate) fn apply(self, reg: &mut Registry) {
        reg.apply(self.registry);
        self.bodies.apply(reg);
    }

    /// The modules, the types and the methods the unit wrote, for the
    /// session's names.
    pub(crate) fn written(&self) -> (Vec<NsId>, Vec<Path>, Vec<(Path, SymRef)>) {
        let modules = self.registry.modules.iter().filter(|(_, e)| e.is_some()).map(|(n, _)| n.0).collect();
        let types = self.registry.types.iter().filter(|(_, e)| e.is_some()).map(|(p, _)| *p).collect();
        let methods = self.registry.methods.iter().filter(|(_, d)| d.is_some()).map(|((p, m), _)| (*p, m.0)).collect();
        (modules, types, methods)
    }
}

/// A method an `impl` took from its trait's default body (`entry` `None`:
/// forgotten).
#[derive(Serialize, Deserialize)]
struct Defaulted {
    owner: Path,
    #[serde(with = "crate::owned_form::sym_serde")]
    method: SymRef,
    entry: Option<(Path, Vec<Type>)>,
}

/// The generic bodies a registry holds, and the methods its `impl`s took
/// from a trait's default body, written down.
#[derive(Default, Serialize, Deserialize)]
struct RebuiltBodies {
    /// Each body, or `None` for one forgotten.
    templates: Vec<(Base, Option<Rc<Template>>)>,
    defaulted: Vec<Defaulted>,
    /// The trees' values, in order.
    values: Vec<OwnedForm>,
}

impl RebuiltBodies {
    /// What `reg` was written since the generation `since`, its values read
    /// out of `heap`. In an order of their names, so the same registry is
    /// written the same way every time.
    fn of(heap: &Heap, reg: &Registry, since: u64) -> Result<RebuiltBodies, Error> {
        let mut templates = reg.templates_since(since);
        templates.sort_by_key(|(base, _)| spelling(base));
        let mut defaulted: Vec<Defaulted> =
            reg.defaulted_since(since).into_iter().map(|((owner, method), entry)| Defaulted { owner, method, entry }).collect();
        defaulted.sort_by_key(|d| (d.owner.to_string(), d.method.name().to_string()));
        let mut values = Vec::new();
        let mut failed = None;
        for template in templates.iter().filter_map(|(_, t)| t.as_ref()) {
            for t in &template.body {
                t.each_value(&mut |v| match value_to_owned(heap, v) {
                    Ok(f) => values.push(f),
                    Err(e) => failed = failed.take().or(Some(e)),
                });
            }
        }
        match failed {
            Some(e) => Err(e),
            None => Ok(RebuiltBodies { templates, defaulted, values }),
        }
    }

    /// The bodies, each value allocated on `heap` and rooted for good, as
    /// a body checked in this session is.
    fn restore(self, heap: &mut Heap) -> Result<Bodies, Error> {
        let mut values = self.values.into_iter();
        let mut failed = None;
        let mut templates = Vec::with_capacity(self.templates.len());
        for (base, template) in self.templates {
            let template = template.map(|template| {
                let mut template = Rc::try_unwrap(template).unwrap_or_else(|_| unreachable!("a template just read is held once"));
                for t in &mut template.body {
                    fill(t, heap, &mut values, &mut failed);
                }
                Rc::new(template)
            });
            templates.push((base, template));
        }
        if let Some(e) = failed {
            return Err(e);
        }
        if values.next().is_some() {
            return Err(Error::TypeError("the dump's rebuilt bodies hold more values than their trees".into()));
        }
        let defaulted = self.defaulted.into_iter().map(|d| ((d.owner, d.method), d.entry)).collect();
        Ok(Bodies { templates, defaulted })
    }
}

/// Bodies a dump wrote, read back.
#[derive(Debug, Default)]
struct Bodies {
    templates: Vec<(Base, Option<Rc<Template>>)>,
    defaulted: Vec<((Path, SymRef), Option<(Path, Vec<Type>)>)>,
}

impl Bodies {
    /// Into `reg`, uncommitted.
    fn apply(self, reg: &mut Registry) {
        for (base, template) in self.templates {
            reg.put_template(base, template);
        }
        for (key, defaulted) in self.defaulted {
            reg.put_defaulted(key, defaulted);
        }
    }
}

/// `t`'s values, taken from `values` in order.
fn fill(t: &mut Typed, heap: &mut Heap, values: &mut impl Iterator<Item = OwnedForm>, failed: &mut Option<Error>) {
    t.values_mut(&mut |slot: &mut Value| {
        let made = match values.next() {
            Some(f) => owned_to_value(heap, &f),
            None => Err(Error::TypeError("the dump's rebuilt bodies hold fewer values than their trees".into())),
        };
        match made {
            Ok(v) => {
                heap.push_permanent_root(v);
                *slot = v;
            }
            Err(e) => *failed = failed.take().or(Some(e)),
        }
    });
}

/// `base` spelled out, to order by.
fn spelling(base: &Base) -> (String, String) {
    match base {
        Base::Fn(path) => (path.to_string(), String::new()),
        Base::Method(owner, method) => (owner.to_string(), method.name().to_string()),
    }
}
