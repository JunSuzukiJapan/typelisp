//! `Native` payloads: Rust data a box holds for a subsystem above this crate.

use std::any::Any;
use std::sync::Arc;

use crate::value::{BoxId, BoxedObj, Native, StructPayload, TypeId, Value};
use super::Heap;

impl Heap {
    /// Stores `data` in a fresh box of the type `type_key`, returning its
    /// `Value::Boxed`. The box owns `data` and drops it when it is collected.
    pub fn alloc_native<T: Any + Send>(&mut self, type_key: TypeId, data: T) -> Value {
        let native = Native(Arc::new(parking_lot::Mutex::new(Box::new(data))));
        self.alloc_boxed(BoxedObj::Struct { type_key, payload: StructPayload::Native(native) })
    }

    /// Runs `f` on the `T` the box `id` holds. `None` when `id` is not a
    /// native box or holds something other than a `T`.
    ///
    /// The heap's box table is not locked while `f` runs, so `f` may use the
    /// heap; it must not ask for the same box again.
    pub fn native_with<T: Any + Send, R>(&self, id: BoxId, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let native = match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Native(Native(data)), .. }) => Arc::clone(data),
            _ => return None,
        };
        let mut guard = native.lock();
        guard.downcast_mut::<T>().map(f)
    }
}
