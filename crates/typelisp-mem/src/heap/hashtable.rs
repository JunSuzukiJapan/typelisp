//! Hash-table payloads.

use std::collections::HashMap;
use crate::value::{BoxId, BoxedObj, StructPayload, TypeId, Value};
use super::Heap;

impl Heap {
    // ---- hash tables --------------------------------------------------------

    /// Stores an empty hash map, returning its `Value::Boxed` —
    /// `HashTable<K,V>`'s runtime representation (see `BoxedObj`/
    /// `StructPayload::Map`'s doc comments).
    pub fn alloc_hashtable(&mut self, type_key: TypeId, hash: Value, test: Value) -> Value {
        self.alloc_boxed(BoxedObj::Struct { type_key, payload: StructPayload::Map(HashMap::new(), [hash, test]) })
    }

    /// The hash function and the test function the table `id` was made with.
    pub fn hashtable_functions(&self, id: BoxId) -> [Value; 2] {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Map(_, fns), .. }) => *fns,
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// The bucket at `hash`, or `&[]` when there is none. Every read below
    /// goes through here.
    fn bucket(&self, id: BoxId, hash: i64) -> parking_lot::MappedRwLockReadGuard<'_, [(Value, Value)]> {
        parking_lot::RwLockReadGuard::map(self.shared.boxes.read_recursive(), |b| match &b.slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map, _), .. }) => {
                map.get(&hash).map(Vec::as_slice).unwrap_or(&[])
            }
            _ => panic!("BoxId does not hold a HashTable"),
        })
    }

    /// How many entries share `hash`. The caller walks them with
    /// [`hashtable_bucket_key`](Self::hashtable_bucket_key) and compares each
    /// with the key type's own `equals` — this layer has no opinion about
    /// what makes two keys the same.
    pub fn hashtable_bucket_count(&self, id: BoxId, hash: i64) -> usize {
        self.bucket(id, hash).len()
    }

    /// The `i`th key in `hash`'s bucket. Panics on an index the caller did
    /// not get from [`hashtable_bucket_count`](Self::hashtable_bucket_count)
    /// — an internal-invariant trap, like every other index here.
    pub fn hashtable_bucket_key(&self, id: BoxId, hash: i64, i: usize) -> Value {
        match self.bucket(id, hash).get(i) {
            Some((k, _)) => *k,
            None => panic!("HashTable: bucket index {} out of range", i),
        }
    }

    /// The `i`th value in `hash`'s bucket. See
    /// [`hashtable_bucket_key`](Self::hashtable_bucket_key).
    pub fn hashtable_bucket_value(&self, id: BoxId, hash: i64, i: usize) -> Value {
        match self.bucket(id, hash).get(i) {
            Some((_, v)) => *v,
            None => panic!("HashTable: bucket index {} out of range", i),
        }
    }

    /// Writes `key -> val` at position `i` of `hash`'s bucket — replacing the
    /// entry already there, or appending when `i` is the bucket's current
    /// length. The caller decides which by looking first, which is the same
    /// call it had to make anyway to answer "was this key already present".
    pub fn hashtable_bucket_put(&mut self, id: BoxId, hash: i64, i: usize, key: Value, val: Value) {
        match self.shared.boxes.write().slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map, _), .. }) => {
                let bucket = map.entry(hash).or_default();
                if i < bucket.len() {
                    bucket[i] = (key, val);
                } else if i == bucket.len() {
                    bucket.push((key, val));
                } else {
                    panic!("HashTable: bucket index {} out of range (len {})", i, bucket.len());
                }
            }
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// Removes the `i`th entry of `hash`'s bucket, keeping the rest in order.
    pub fn hashtable_bucket_delete(&mut self, id: BoxId, hash: i64, i: usize) {
        match self.shared.boxes.write().slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map, _), .. }) => {
                let Some(bucket) = map.get_mut(&hash) else {
                    panic!("HashTable: no bucket at hash {}", hash);
                };
                if i >= bucket.len() {
                    panic!("HashTable: bucket index {} out of range (len {})", i, bucket.len());
                }
                bucket.remove(i);
                if bucket.is_empty() {
                    map.remove(&hash);
                }
            }
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// The number of entries in a boxed hash map. Panics if `id` doesn't
    /// hold a `BoxedObj::Struct` with a `StructPayload::Map` payload.
    pub fn hashtable_count(&self, id: BoxId) -> usize {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map, _), .. }) => {
                map.values().map(Vec::len).sum()
            }
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// Removes every entry in place. Panics if `id` doesn't hold a
    /// `BoxedObj::Struct` with a `StructPayload::Map` payload.
    pub fn hashtable_clear(&mut self, id: BoxId) {
        match self.shared.boxes.write().slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map, _), .. }) => map.clear(),
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }

    /// Every `(key, value)` pair currently stored, as plain [`Value`]s — the
    /// primitive `HashTable<K,V>::keys`/`values`/`entries` snapshot off of
    /// (see those builtins in `crate::eval::interp`'s `"hashtable"` arm).
    /// Order is whatever the underlying `HashMap` iterates in (unspecified,
    /// like `HashTable<K,V>`'s method surface always has been). Panics if
    /// `id` doesn't hold a `BoxedObj::Struct` with a `StructPayload::Map`
    /// payload.
    pub fn hashtable_pairs(&self, id: BoxId) -> Vec<(Value, Value)> {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Map(map, _), .. }) => {
                map.values().flat_map(|b| b.iter().copied()).collect()
            }
            _ => panic!("BoxId does not hold a HashTable"),
        }
    }
}
