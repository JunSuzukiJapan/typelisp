//! `Scope` payloads.

use crate::prelude::*;
use crate::collections::HashMap;
use crate::value::{BoxId, BoxedObj, StructPayload, TypeId, Value};
use super::Heap;

impl Heap {
    // ---- scopes -------------------------------------------------------------

    /// Allocates one fresh, empty scope frame box — see
    /// [`StructPayload`]'s doc comment for why a frame is a box of its own
    /// (frame *sharing* across `scope_clone_frames`) rather than a `HashMap`
    /// stored inline in the scope's payload.
    fn alloc_scope_frame(&mut self) -> BoxId {
        let obj = BoxedObj::Struct { type_key: TypeId::SCOPE_FRAME, payload: StructPayload::Frame(HashMap::new()) };
        match self.alloc_boxed(obj) {
            Value::Boxed(id) => id,
            _ => unreachable!("alloc_boxed always returns Value::Boxed"),
        }
    }

    /// The frame stack of a scope box. Panics if `id` doesn't hold a
    /// `BoxedObj::Struct` with a `StructPayload::Frames` payload — same
    /// internal-invariant-trap convention as
    /// [`hashtable_bucket_key`](Self::hashtable_bucket_key)'s.
    /// Returns an owned copy (rather than a borrow) so callers — `scope_get`
    /// chief among them — can look a name up in each frame's own bindings
    /// (`frame_binding`, another `shared.boxes` reader) while iterating,
    /// without holding two reads of the same `RwLock` open at once
    /// (`parking_lot`'s recursive-read is only safe when no writer is ever
    /// waiting, which will stop holding once real threads share this heap —
    /// see `docs/dev/os-threads-design.md` §2). A scope's frame stack is a
    /// handful of `BoxId`s, so the clone costs nothing worth avoiding.
    fn scope_frames(&self, id: BoxId) -> Vec<BoxId> {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Frames(frames), .. }) => frames.clone(),
            _ => panic!("BoxId does not hold a Scope"),
        }
    }

    /// A binding of `name` in one frame, by the frame's own `BoxId` —
    /// `scope_get`'s per-frame lookup. Panics if `fid` doesn't hold a
    /// `StructPayload::Frame` payload — every `BoxId` on a scope's frame
    /// stack does, by construction.
    fn frame_binding(&self, fid: BoxId, name: &str) -> Option<Value> {
        match &self.shared.boxes.read_recursive().slots[fid.0 as usize] {
            Some(BoxedObj::Struct { payload: StructPayload::Frame(map), .. }) => map.get(name).copied(),
            _ => panic!("BoxId does not hold a Scope frame"),
        }
    }

    /// Stores a fresh `Scope<V>` with one empty frame already pushed — the
    /// frame a function's own parameters/captures bind into, matching
    /// `Scope::new`'s "ready to use immediately" convention (see the
    /// interpreter's `scope_new`) — returning its `Value::Boxed`. Two boxes
    /// are allocated (the scope and its first frame); like every box-store
    /// allocation this never itself triggers a collection.
    pub fn alloc_scope(&mut self) -> Value {
        let frame = self.alloc_scope_frame();
        self.alloc_boxed(BoxedObj::Struct { type_key: TypeId::SCOPE, payload: StructPayload::Frames(vec![frame]) })
    }

    /// `Scope::clone-frames`'s primitive: a brand-new scope box whose stack
    /// holds the *same* frame boxes `id`'s currently does (a `BoxId` copy
    /// per frame, never a copy of a frame's entries) — a later
    /// [`scope_set`](Self::scope_set) into a shared frame is visible through
    /// both scopes, while each scope's stack grows/shrinks independently
    /// ([`scope_push_frame`](Self::scope_push_frame)/
    /// [`scope_pop_frame`](Self::scope_pop_frame) touch only the one scope
    /// they're called on). Panics like [`scope_get`](Self::scope_get).
    pub fn scope_clone_frames(&mut self, id: BoxId) -> Value {
        let frames = self.scope_frames(id);
        self.alloc_boxed(BoxedObj::Struct { type_key: TypeId::SCOPE, payload: StructPayload::Frames(frames) })
    }

    /// Pushes a fresh, empty frame onto a scope's stack. Panics like
    /// [`scope_get`](Self::scope_get).
    pub fn scope_push_frame(&mut self, id: BoxId) {
        let frame = self.alloc_scope_frame();
        match self.shared.boxes.write().slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Frames(frames), .. }) => frames.push(frame),
            _ => panic!("BoxId does not hold a Scope"),
        }
    }

    /// Pops the newest frame off a scope's stack (the frame box itself
    /// becomes garbage once no other scope shares it). A no-op on an empty
    /// stack — the pre-unification `scope_pop_frame`'s `Vec::pop` behavior.
    /// Panics like [`scope_get`](Self::scope_get).
    pub fn scope_pop_frame(&mut self, id: BoxId) {
        match self.shared.boxes.write().slots[id.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Frames(frames), .. }) => {
                frames.pop();
            }
            _ => panic!("BoxId does not hold a Scope"),
        }
    }

    /// `Scope::get`'s primitive: searches from the most-recently-pushed
    /// frame outward, returning the first binding of `name` found, or `None`
    /// if no frame binds it (the search deliberately ends at this scope's
    /// own first frame — an *enclosing function's* scope is a different
    /// value entirely; see `compiler.rs`'s module doc comment). Panics if
    /// `id` doesn't hold a `StructPayload::Frames` payload.
    pub fn scope_get(&self, id: BoxId, name: &str) -> Option<Value> {
        for fid in self.scope_frames(id).into_iter().rev() {
            if let Some(v) = self.frame_binding(fid, name) {
                return Some(v);
            }
        }
        None
    }

    /// `Scope::set`'s primitive: binds `name` to `v` in the newest frame —
    /// always, even if an older (possibly shared, via
    /// [`scope_clone_frames`](Self::scope_clone_frames)) frame already binds
    /// `name`, so a `let`/`labels` def's own bindings never leak into
    /// whatever scope it borrowed frames from. Panics if `id` doesn't hold a
    /// `StructPayload::Frames` payload, or if the stack has no frame to
    /// write into (every frame popped) — the caller-side invariant the
    /// interpreter turns into an `EvalError` at its own boundary, same as
    /// the hash-table key-shape panics.
    pub fn scope_set(&mut self, id: BoxId, name: &str, v: Value) {
        let top = *self.scope_frames(id).last().expect("Scope::set: no frame to write into");
        match self.shared.boxes.write().slots[top.0 as usize].as_mut() {
            Some(BoxedObj::Struct { payload: StructPayload::Frame(map), .. }) => {
                map.insert(name.to_string(), v);
            }
            _ => panic!("BoxId does not hold a Scope frame"),
        }
    }

    /// True if `id` holds a `BoxedObj::Struct` with a
    /// `StructPayload::Frames` payload (a `Scope<V>` value) — the peer of
    /// [`is_struct`](Self::is_struct)/[`is_hashtable`](Self::is_hashtable)
    /// for callers that decode a `Value::Boxed` without knowing its kind.
    /// (Frame boxes get no public predicate: they are internal constituents
    /// of some scope, never handed out as standalone values.)
    pub fn is_scope(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Struct { payload: StructPayload::Frames(_), .. }))
    }

    /// The number of frames currently on a scope's stack — what the
    /// interpreter's `#<scope depth=N>` display shows, and its guard for
    /// turning [`scope_set`](Self::scope_set)'s "no frame to write into"
    /// panic into a catchable evaluation error before calling in (the same
    /// pre-check split as the hash-table key-shape panics). Panics like
    /// [`scope_get`](Self::scope_get).
    pub fn scope_frame_count(&self, id: BoxId) -> usize {
        self.scope_frames(id).len()
    }
}
