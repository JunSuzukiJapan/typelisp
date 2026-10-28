//! A type's or a trait's members, keyed by their member symbols.
//!
//! A method is named by the symbol [`typelisp_mem::symbols::member_symbol`]
//! makes — one per spelling, in the module every type's members share — so a
//! table of members compares symbols, never strings. The table is ordered by
//! the members' names, so that what it serializes to (a dump, the ABI
//! signature) is the same in every run; the symbols' addresses differ run to
//! run.

use std::collections::BTreeMap;

use typelisp_mem::SymRef;

use crate::owned_form::OwnedSym;

/// A member symbol ordered by its name — [`Members`]'s key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ByName(SymRef);

impl Ord for ByName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.0 == other.0 {
            return std::cmp::Ordering::Equal;
        }
        self.0.name().cmp(other.0.name())
    }
}

impl PartialOrd for ByName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Members keyed by their member symbols, in the order of their names.
#[derive(Clone, Debug)]
pub struct Members<T> {
    map: BTreeMap<ByName, T>,
}

impl<T> Default for Members<T> {
    fn default() -> Self {
        Members { map: BTreeMap::new() }
    }
}

impl<T> Members<T> {
    pub fn new() -> Self {
        Members::default()
    }

    pub fn get(&self, member: SymRef) -> Option<&T> {
        self.map.get(&ByName(member))
    }

    pub fn get_mut(&mut self, member: SymRef) -> Option<&mut T> {
        self.map.get_mut(&ByName(member))
    }

    pub fn contains_key(&self, member: SymRef) -> bool {
        self.map.contains_key(&ByName(member))
    }

    pub fn insert(&mut self, member: SymRef, value: T) -> Option<T> {
        self.map.insert(ByName(member), value)
    }

    pub fn remove(&mut self, member: SymRef) -> Option<T> {
        self.map.remove(&ByName(member))
    }

    pub fn entry_or_insert_with(&mut self, member: SymRef, make: impl FnOnce() -> T) -> &mut T {
        self.map.entry(ByName(member)).or_insert_with(make)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (SymRef, &T)> {
        self.map.iter().map(|(k, v)| (k.0, v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (SymRef, &mut T)> {
        self.map.iter_mut().map(|(k, v)| (k.0, v))
    }

    pub fn keys(&self) -> impl Iterator<Item = SymRef> + '_ {
        self.map.keys().map(|k| k.0)
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.map.values()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.map.values_mut()
    }

    pub fn retain(&mut self, mut keep: impl FnMut(SymRef, &mut T) -> bool) {
        self.map.retain(|k, v| keep(k.0, v))
    }
}

impl<T> FromIterator<(SymRef, T)> for Members<T> {
    fn from_iter<I: IntoIterator<Item = (SymRef, T)>>(iter: I) -> Self {
        Members { map: iter.into_iter().map(|(k, v)| (ByName(k), v)).collect() }
    }
}

impl<T> Extend<(SymRef, T)> for Members<T> {
    fn extend<I: IntoIterator<Item = (SymRef, T)>>(&mut self, iter: I) {
        self.map.extend(iter.into_iter().map(|(k, v)| (ByName(k), v)))
    }
}

impl<T> IntoIterator for Members<T> {
    type Item = (SymRef, T);
    type IntoIter = std::iter::Map<std::collections::btree_map::IntoIter<ByName, T>, fn((ByName, T)) -> (SymRef, T)>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.into_iter().map(|(k, v)| (k.0, v))
    }
}

impl<'a, T> IntoIterator for &'a Members<T> {
    type Item = (SymRef, &'a T);
    type IntoIter = std::iter::Map<std::collections::btree_map::Iter<'a, ByName, T>, fn((&'a ByName, &'a T)) -> (SymRef, &'a T)>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.iter().map(|(k, v)| (k.0, v))
    }
}

/// Written out as `(member, value)` pairs, each member as its
/// [`OwnedSym`] — the one form that means the same thing in another process.
impl<T: serde::Serialize> serde::Serialize for Members<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let pairs: Vec<(OwnedSym, &T)> = self.map.iter().map(|(k, v)| (OwnedSym::of(k.0), v)).collect();
        pairs.serialize(s)
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Members<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Members<T>, D::Error> {
        let pairs: Vec<(OwnedSym, T)> = serde::Deserialize::deserialize(d)?;
        Ok(pairs.into_iter().map(|(k, v)| (k.symbol(), v)).collect())
    }
}

/// A list of member symbols written as their [`OwnedSym`]s. For
/// `#[serde(with = "member_list_serde")]`.
pub mod member_list_serde {
    use super::{OwnedSym, SymRef};

    pub fn serialize<S: serde::Serializer>(syms: &[SymRef], s: S) -> Result<S::Ok, S::Error> {
        let owned: Vec<OwnedSym> = syms.iter().map(|m| OwnedSym::of(*m)).collect();
        serde::Serialize::serialize(&owned, s)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<SymRef>, D::Error> {
        let owned: Vec<OwnedSym> = serde::Deserialize::deserialize(d)?;
        Ok(owned.iter().map(OwnedSym::symbol).collect())
    }
}

/// A table keyed by `(type or trait, member symbol)` written as
/// `((path, member), value)` pairs, each member as its [`OwnedSym`]. For
/// `#[serde(with = "method_table_serde")]`.
pub mod method_table_serde {
    use std::collections::HashMap;

    use super::{OwnedSym, SymRef};
    use crate::Path;

    pub fn serialize<S: serde::Serializer, V: serde::Serialize>(table: &HashMap<(Path, SymRef), V>, s: S) -> Result<S::Ok, S::Error> {
        let mut pairs: Vec<((Path, OwnedSym), &V)> = table.iter().map(|((p, m), v)| ((*p, OwnedSym::of(*m)), v)).collect();
        // The same bytes every run: a `HashMap`'s order is not.
        pairs.sort_by(|((pa, ma), _), ((pb, mb), _)| (pa.to_string(), format!("{:?}", ma)).cmp(&(pb.to_string(), format!("{:?}", mb))));
        serde::Serialize::serialize(&pairs, s)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>, V: serde::Deserialize<'de>>(d: D) -> Result<HashMap<(Path, SymRef), V>, D::Error> {
        let pairs: Vec<((Path, OwnedSym), V)> = serde::Deserialize::deserialize(d)?;
        Ok(pairs.into_iter().map(|((p, m), v)| ((p, m.symbol()), v)).collect())
    }
}
