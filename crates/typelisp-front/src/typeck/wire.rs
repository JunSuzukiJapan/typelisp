//! The generic bodies the rebuilt checker checked, written into a dump
//! (`docs/dev/checker-rebuild-plan.md` 8.3), so the prelude's are checked
//! once, when its dump is made, and never by a session.
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
use super::registry::{Registry, Template};
use super::tree::Typed;

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

/// A method an `impl` took from its trait's default body.
#[derive(Serialize, Deserialize)]
struct Defaulted {
    owner: Path,
    #[serde(with = "crate::owned_form::sym_serde")]
    method: SymRef,
    trait_path: Path,
    args: Vec<Type>,
}

/// The generic bodies a registry holds, and the methods its `impl`s took
/// from a trait's default body, written down.
#[derive(Default, Serialize, Deserialize)]
pub struct RebuiltBodies {
    templates: Vec<(Base, Rc<Template>)>,
    defaulted: Vec<Defaulted>,
    /// The trees' values, in order.
    values: Vec<OwnedForm>,
}

impl RebuiltBodies {
    /// What `reg` holds, its values read out of `heap`. In an order of their
    /// names, so the same registry is written the same way every time.
    pub(crate) fn of(heap: &Heap, reg: &Registry) -> Result<RebuiltBodies, Error> {
        let mut templates: Vec<(Base, Rc<Template>)> = reg.templates().map(|(b, t)| (*b, t.clone())).collect();
        templates.sort_by_key(|(base, _)| spelling(base));
        let mut defaulted: Vec<Defaulted> = reg
            .all_defaulted()
            .map(|((owner, method), (trait_path, args))| Defaulted { owner: *owner, method: *method, trait_path: *trait_path, args: args.clone() })
            .collect();
        defaulted.sort_by_key(|d| (d.owner.to_string(), d.method.name().to_string()));
        let mut values = Vec::new();
        let mut failed = None;
        for (_, template) in &templates {
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
    pub(crate) fn restore(self, heap: &mut Heap) -> Result<Bodies, Error> {
        let mut values = self.values.into_iter();
        let mut failed = None;
        let mut templates = Vec::with_capacity(self.templates.len());
        for (base, template) in self.templates {
            let mut template = Rc::try_unwrap(template).unwrap_or_else(|_| unreachable!("a template just read is held once"));
            for t in &mut template.body {
                fill(t, heap, &mut values, &mut failed);
            }
            templates.push((base, Rc::new(template)));
        }
        if let Some(e) = failed {
            return Err(e);
        }
        if values.next().is_some() {
            return Err(Error::TypeError("the dump's rebuilt bodies hold more values than their trees".into()));
        }
        let defaulted = self.defaulted.into_iter().map(|d| ((d.owner, d.method), (d.trait_path, d.args))).collect();
        Ok(Bodies { templates, defaulted })
    }
}

/// Bodies a dump wrote, read back.
#[derive(Debug)]
pub(crate) struct Bodies {
    templates: Vec<(Base, Rc<Template>)>,
    defaulted: Vec<((Path, SymRef), (Path, Vec<Type>))>,
}

impl Bodies {
    /// Into `reg`, where it has none of its own.
    pub(crate) fn adopt(self, reg: &mut Registry) {
        for (base, template) in self.templates {
            reg.adopt_template(base, template);
        }
        for (key, defaulted) in self.defaulted {
            reg.adopt_defaulted(key, defaulted);
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
