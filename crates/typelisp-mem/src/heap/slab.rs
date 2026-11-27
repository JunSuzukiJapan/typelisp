//! `Slab`, the chunked vector the heap's slot tables are built on.

use crate::prelude::*;
/// Entries in a [`Slab`]'s first chunk; each chunk after it has twice the
/// entries of the one before, up to [`SLAB_LAST_LEN`], and every chunk from
/// there on has that many. Powers of two, so that which chunk an index is in
/// is the position of its highest bit ([`Slab::place`]). The first is small,
/// so that a heap with a few boxes takes little memory for them (a board may
/// have only kilobytes of RAM); the last is not large, so that a heap with
/// many asks for no block of memory a fragmented heap cannot give.
const SLAB_FIRST_SHIFT: u32 = 4;
const SLAB_FIRST_LEN: usize = 1 << SLAB_FIRST_SHIFT;
const SLAB_LAST_SHIFT: u32 = 10;
const SLAB_LAST_LEN: usize = 1 << SLAB_LAST_SHIFT;
/// How many chunks double, and how many slots they hold together.
const SLAB_DOUBLING: usize = (SLAB_LAST_SHIFT - SLAB_FIRST_SHIFT) as usize + 1;
const SLAB_DOUBLED: usize = SLAB_FIRST_LEN * ((1 << SLAB_DOUBLING) - 1);

/// A growable slot store, indexed like `Vec<Option<T>>` (which this replaces
/// as the type of `str_slots`/`box_slots`/`permanent_roots`) but never
/// relocating an existing entry: storage is chunks, `SLAB_FIRST_LEN` slots
/// and then twice the last one's up to `SLAB_LAST_LEN`, appended as the slab
/// grows, so a slot's address is fixed for the slab's
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

    /// How many slots there is room for without another chunk.
    pub(super) fn capacity(&self) -> usize {
        (0..self.chunks.len()).map(Self::chunk_len).sum()
    }

    /// How many slots chunk `k` has.
    fn chunk_len(k: usize) -> usize {
        SLAB_FIRST_LEN << k.min(SLAB_DOUBLING - 1)
    }

    /// The chunk slot `i` is in, and where in it. Of the chunks that double,
    /// chunk `k` holds the `SLAB_FIRST_LEN << k` slots from
    /// `SLAB_FIRST_LEN * (2^k - 1)` on, so `i + SLAB_FIRST_LEN` has its
    /// highest bit at `SLAB_FIRST_SHIFT + k` and the rest of it is the
    /// offset; after them, every chunk is `SLAB_LAST_LEN` slots.
    fn place(i: usize) -> (usize, usize) {
        if i >= SLAB_DOUBLED {
            let past = i - SLAB_DOUBLED;
            return (SLAB_DOUBLING + (past >> SLAB_LAST_SHIFT), past & (SLAB_LAST_LEN - 1));
        }
        let j = i + SLAB_FIRST_LEN;
        let high = usize::BITS - 1 - j.leading_zeros();
        ((high - SLAB_FIRST_SHIFT) as usize, j - (1 << high))
    }

    /// Appends `v` as a new slot, growing by one chunk if the current one is
    /// full, and returns the index it landed at.
    pub(super) fn push(&mut self, v: Option<T>) -> usize {
        let i = self.len;
        let (chunk_idx, offset) = Slab::<T>::place(i);
        if chunk_idx == self.chunks.len() {
            let len = Self::chunk_len(chunk_idx);
            let mut chunk = Vec::with_capacity(len);
            chunk.resize_with(len, || None);
            self.chunks.push(chunk.into_boxed_slice());
        }
        self.chunks[chunk_idx][offset] = v;
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

impl<T> core::ops::Index<usize> for Slab<T> {
    type Output = Option<T>;
    fn index(&self, i: usize) -> &Option<T> {
        let (chunk, offset) = Slab::<T>::place(i);
        &self.chunks[chunk][offset]
    }
}

impl<T> core::ops::IndexMut<usize> for Slab<T> {
    fn index_mut(&mut self, i: usize) -> &mut Option<T> {
        let (chunk, offset) = Slab::<T>::place(i);
        &mut self.chunks[chunk][offset]
    }
}

#[cfg(test)]
mod slab_tests {
    use super::Slab;

    // Past several chunks (16, 32, 64, ... slots), so that growth mid-slab
    // is exercised, not just a single chunk.
    const PAST_ONE_CHUNK: usize = 1000;

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
        for i in 0..super::SLAB_FIRST_LEN {
            s.push(Some(i as i64));
        }
        for i in 0..10 {
            s.push(Some(1000 + i));
        }
        for i in 0..super::SLAB_FIRST_LEN {
            assert_eq!(s[i], Some(i as i64));
        }
    }

    /// Every slot is in a chunk of its own place, and the chunks hold the
    /// slots in order without a gap: doubling, then all of the last size.
    #[test]
    fn each_index_has_its_place_and_the_chunks_double() {
        let mut s: Slab<usize> = Slab::new();
        for i in 0..5000 {
            s.push(Some(i));
            assert!(i < s.capacity());
        }
        let lens: Vec<usize> = s.chunks.iter().map(|c| c.len()).collect();
        assert_eq!(&lens[..super::SLAB_DOUBLING], &(0..super::SLAB_DOUBLING).map(|k| super::SLAB_FIRST_LEN << k).collect::<Vec<_>>()[..]);
        assert!(lens[super::SLAB_DOUBLING..].iter().all(|n| *n == super::SLAB_LAST_LEN), "{:?}", lens);
        let mut expected = 0;
        for chunk in &s.chunks {
            for slot in chunk.iter().take_while(|v| v.is_some()) {
                assert_eq!(*slot, Some(expected));
                expected += 1;
            }
        }
        assert_eq!(expected, 5000);
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
