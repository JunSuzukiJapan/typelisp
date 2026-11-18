//! `Slab`, the chunked vector the heap's slot tables are built on.

/// Entries per [`Slab`] chunk — a power of two so the chunk/offset split is a
/// shift and a mask rather than a division.
const SLAB_CHUNK_SHIFT: u32 = 10;
const SLAB_CHUNK_LEN: usize = 1 << SLAB_CHUNK_SHIFT;
const SLAB_CHUNK_MASK: usize = SLAB_CHUNK_LEN - 1;

/// A growable slot store, indexed like `Vec<Option<T>>` (which this replaces
/// as the type of `str_slots`/`box_slots`/`permanent_roots`) but never
/// relocating an existing entry: storage is chunks of `SLAB_CHUNK_LEN` slots,
/// appended as the slab grows, so a slot's address is fixed for the slab's
/// whole life once written. A plain `Vec<Option<T>>` cannot promise that —
/// every `push` past capacity moves every existing element to a new buffer.
///
/// That stability is scaffolding for `docs/dev/os-threads-design.md` §3: a
/// `HeapShared` growing one of these under a lock will let readers on other
/// threads dereference an already-issued index without taking that lock at
/// all, because growth only ever appends a new chunk and never disturbs one
/// already handed out. Nothing here is thread-safe yet — this type only
/// gives the storage the *shape* that later phases make safe to share; nobody
/// but the single owning `Heap` touches a `Slab` today.
pub(super) struct Slab<T> {
    chunks: Vec<Box<[Option<T>]>>,
    len: usize,
}

// Written by hand rather than `#[derive(Default)]`: the derive would require
// `T: Default` even though an empty `Slab` never needs one (there is nothing
// in either field to default-construct an element of).
impl<T> Default for Slab<T> {
    fn default() -> Slab<T> {
        Slab::new()
    }
}

impl<T> Slab<T> {
    pub(super) fn new() -> Slab<T> {
        Slab { chunks: Vec::new(), len: 0 }
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }

    /// Appends `v` as a new slot, growing by one chunk if the current one is
    /// full, and returns the index it landed at.
    pub(super) fn push(&mut self, v: Option<T>) -> usize {
        let i = self.len;
        let chunk_idx = i >> SLAB_CHUNK_SHIFT;
        if chunk_idx == self.chunks.len() {
            let mut chunk = Vec::with_capacity(SLAB_CHUNK_LEN);
            chunk.resize_with(SLAB_CHUNK_LEN, || None);
            self.chunks.push(chunk.into_boxed_slice());
        }
        self.chunks[chunk_idx][i & SLAB_CHUNK_MASK] = v;
        self.len += 1;
        i
    }

    /// Every occupied slot's index and value, in ascending order — the sweep
    /// and audit loops' iteration shape (`Vec<Option<T>>::iter().enumerate()`
    /// used to give this directly; `Slab` doesn't implement `Iterator`
    /// because a chunked store has no single contiguous slice to hand out).
    pub(super) fn iter(&self) -> impl Iterator<Item = &Option<T>> {
        (0..self.len).map(move |i| &self[i])
    }
}

impl<T> std::ops::Index<usize> for Slab<T> {
    type Output = Option<T>;
    fn index(&self, i: usize) -> &Option<T> {
        &self.chunks[i >> SLAB_CHUNK_SHIFT][i & SLAB_CHUNK_MASK]
    }
}

impl<T> std::ops::IndexMut<usize> for Slab<T> {
    fn index_mut(&mut self, i: usize) -> &mut Option<T> {
        &mut self.chunks[i >> SLAB_CHUNK_SHIFT][i & SLAB_CHUNK_MASK]
    }
}

#[cfg(test)]
mod slab_tests {
    use super::Slab;

    // `SLAB_CHUNK_LEN` is 1024 — these counts are chosen to cross a chunk
    // boundary (so growth mid-slab is exercised, not just a single chunk).
    const PAST_ONE_CHUNK: usize = super::SLAB_CHUNK_LEN + 5;

    #[test]
    fn push_returns_sequential_indices_and_index_reads_them_back() {
        let mut s: Slab<i64> = Slab::new();
        for i in 0..PAST_ONE_CHUNK {
            let idx = s.push(Some(i as i64));
            assert_eq!(idx, i);
        }
        assert_eq!(s.len(), PAST_ONE_CHUNK);
        for i in 0..PAST_ONE_CHUNK {
            assert_eq!(s[i], Some(i as i64));
        }
    }

    #[test]
    fn none_entries_round_trip_like_an_ordinary_option_vec() {
        let mut s: Slab<i64> = Slab::new();
        s.push(Some(1));
        s.push(None);
        s.push(Some(3));
        assert_eq!(s[0], Some(1));
        assert_eq!(s[1], None);
        assert_eq!(s[2], Some(3));
    }

    #[test]
    fn index_mut_overwrites_a_slot_in_place() {
        let mut s: Slab<i64> = Slab::new();
        s.push(Some(1));
        s[0] = Some(2);
        assert_eq!(s[0], Some(2));
        s[0] = None;
        assert_eq!(s[0], None);
    }

    #[test]
    fn growth_past_a_chunk_boundary_does_not_move_earlier_entries() {
        // The whole point of `Slab` over `Vec<Option<T>>`: an address taken
        // before growth stays valid after it. Cons cells and `Box<RootStack>`
        // get this from a raw pointer directly; a `Slab<i64>` slot has no
        // address to hand out through the public API, so this test stands in
        // for that guarantee by checking that entries written before the
        // chunk boundary still read back correctly *after* pushing well past
        // it (a `Vec<Option<T>>` would have reallocated and copied them, so
        // this alone would not distinguish the two — the real proof is that
        // `push` never invalidates any `&Option<T>` obtained via `index`
        // before the growing call, which the borrow checker enforces at
        // compile time: `Slab::push` takes `&mut self`, so no such borrow can
        // be alive across it in safe code).
        let mut s: Slab<i64> = Slab::new();
        for i in 0..super::SLAB_CHUNK_LEN {
            s.push(Some(i as i64));
        }
        for i in 0..10 {
            s.push(Some(1000 + i));
        }
        for i in 0..super::SLAB_CHUNK_LEN {
            assert_eq!(s[i], Some(i as i64));
        }
    }

    #[test]
    fn iter_visits_every_slot_in_order() {
        let mut s: Slab<i64> = Slab::new();
        s.push(Some(10));
        s.push(None);
        s.push(Some(30));
        let collected: Vec<Option<i64>> = s.iter().copied().collect();
        assert_eq!(collected, vec![Some(10), None, Some(30)]);
    }
}
