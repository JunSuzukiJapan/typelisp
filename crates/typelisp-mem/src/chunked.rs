//! [`ChunkStack`], the growable sequence the runtime's tables are kept in
//! where an entry must never move or a growth must never copy: the heap's
//! slot tables, a task's root stack, its open frames, and the driver's
//! frames.

use crate::prelude::*;

/// Entries in a [`ChunkStack`]'s first chunk; each chunk after it has twice
/// the entries of the one before, up to [`LAST_LEN`], and every chunk from
/// there on has that many. Powers of two, so that which chunk an index is in
/// is the position of its highest bit ([`place`]). The first is small, so
/// that a table with a few entries takes little memory (a board may have
/// only kilobytes of RAM); the last is not large, so that a table with many
/// asks for no block of memory a fragmented heap cannot give.
const FIRST_SHIFT: u32 = 4;
const FIRST_LEN: usize = 1 << FIRST_SHIFT;
const LAST_SHIFT: u32 = 10;
const LAST_LEN: usize = 1 << LAST_SHIFT;
/// How many chunks double, and how many entries they hold together.
const DOUBLING: usize = (LAST_SHIFT - FIRST_SHIFT) as usize + 1;
const DOUBLED: usize = FIRST_LEN * ((1 << DOUBLING) - 1);

/// How many entries chunk `k` has.
fn chunk_len(k: usize) -> usize {
    FIRST_LEN << k.min(DOUBLING - 1)
}

/// The chunk entry `i` is in, and where in it. Of the chunks that double,
/// chunk `k` holds the `FIRST_LEN << k` entries from `FIRST_LEN * (2^k - 1)`
/// on, so `i + FIRST_LEN` has its highest bit at `FIRST_SHIFT + k` and the
/// rest of it is the offset; after them, every chunk is `LAST_LEN` entries.
fn place(i: usize) -> (usize, usize) {
    if i >= DOUBLED {
        let past = i - DOUBLED;
        return (DOUBLING + (past >> LAST_SHIFT), past & (LAST_LEN - 1));
    }
    let j = i + FIRST_LEN;
    let high = usize::BITS - 1 - j.leading_zeros();
    ((high - FIRST_SHIFT) as usize, j - (1 << high))
}

/// A sequence that grows and shrinks at its end, as a `Vec` does, but is
/// kept in chunks ([`chunk_len`]) that are never moved or resized once made.
///
/// So an entry's address is fixed for as long as it is in the sequence, and
/// growing never copies: a `Vec` that grows past its capacity needs its old
/// buffer and one twice as large at the same moment, and leaves the old one
/// behind as a hole the next, larger growth cannot use. On a board, whose
/// allocator moves nothing, that holds a table's growth to well under the
/// memory there is.
///
/// The address stability is also scaffolding for
/// `docs/dev/os-threads-design.md` §3: a `HeapShared` growing one of these
/// under a lock will let readers on other threads dereference an
/// already-issued index without taking that lock, because growth only ever
/// appends a chunk. Nothing here is thread-safe yet.
///
/// Shrinking keeps the chunks, as a `Vec` keeps its capacity.
pub struct ChunkStack<T> {
    /// Each chunk a `Vec` made with its chunk's length as its capacity, and
    /// never pushed past it, so that it never reallocates. The chunks past
    /// the one the last entry is in are empty.
    chunks: Vec<Vec<T>>,
    len: usize,
}

// Written by hand rather than `#[derive(Default)]`: the derive would require
// `T: Default` even though an empty `ChunkStack` never needs one.
impl<T> Default for ChunkStack<T> {
    fn default() -> ChunkStack<T> {
        ChunkStack::new()
    }
}

impl<T> ChunkStack<T> {
    /// An empty sequence, which has allocated nothing.
    pub const fn new() -> ChunkStack<T> {
        ChunkStack { chunks: Vec::new(), len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// How many entries there is room for without another chunk.
    pub fn capacity(&self) -> usize {
        (0..self.chunks.len()).map(chunk_len).sum()
    }

    /// Appends `v`, making a chunk for it if the last is full, and returns
    /// the index it landed at.
    pub fn push(&mut self, v: T) -> usize {
        let i = self.len;
        let (k, offset) = place(i);
        if k == self.chunks.len() {
            self.chunks.push(Vec::with_capacity(chunk_len(k)));
        }
        let chunk = &mut self.chunks[k];
        debug_assert_eq!(chunk.len(), offset);
        chunk.push(v);
        self.len += 1;
        i
    }

    /// Removes the last entry and returns it.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        self.chunks[place(self.len).0].pop()
    }

    /// The last entry.
    pub fn last(&self) -> Option<&T> {
        self.len.checked_sub(1).map(|i| &self[i])
    }

    /// Drops every entry from index `len` on; nothing when there are not
    /// that many.
    pub fn truncate(&mut self, len: usize) {
        if len >= self.len {
            return;
        }
        let (k, offset) = place(len);
        self.chunks[k].truncate(offset);
        for chunk in &mut self.chunks[k + 1..] {
            chunk.clear();
        }
        self.len = len;
    }

    pub fn clear(&mut self) {
        self.truncate(0)
    }

    /// Every entry, first to last.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.chunks.iter().flat_map(|chunk| chunk.iter())
    }
}

impl<T> core::ops::Index<usize> for ChunkStack<T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        assert!(i < self.len, "index {} past the {} entries", i, self.len);
        let (k, offset) = place(i);
        &self.chunks[k][offset]
    }
}

impl<T> core::ops::IndexMut<usize> for ChunkStack<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        assert!(i < self.len, "index {} past the {} entries", i, self.len);
        let (k, offset) = place(i);
        &mut self.chunks[k][offset]
    }
}

impl<'a, T> IntoIterator for &'a ChunkStack<T> {
    type Item = &'a T;
    type IntoIter = core::iter::FlatMap<core::slice::Iter<'a, Vec<T>>, core::slice::Iter<'a, T>, fn(&'a Vec<T>) -> core::slice::Iter<'a, T>>;
    fn into_iter(self) -> Self::IntoIter {
        self.chunks.iter().flat_map(|chunk| chunk.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::{ChunkStack, DOUBLING, FIRST_LEN, LAST_LEN};

    #[test]
    fn push_returns_sequential_indices_and_index_reads_them_back() {
        let mut s: ChunkStack<i64> = ChunkStack::new();
        for i in 0..1000 {
            assert_eq!(s.push(i as i64), i);
        }
        assert_eq!(s.len(), 1000);
        for i in 0..1000 {
            assert_eq!(s[i], i as i64);
        }
        s[3] = -3;
        assert_eq!(s[3], -3);
    }

    /// Every entry is in a chunk of its own place, and the chunks hold the
    /// entries in order without a gap: doubling, then all of the last size.
    #[test]
    fn each_index_has_its_place_and_the_chunks_double() {
        let mut s: ChunkStack<usize> = ChunkStack::new();
        for i in 0..5000 {
            s.push(i);
            assert!(i < s.capacity());
        }
        let lens: Vec<usize> = s.chunks.iter().map(|c| c.capacity()).collect();
        assert_eq!(&lens[..DOUBLING], &(0..DOUBLING).map(|k| FIRST_LEN << k).collect::<Vec<_>>()[..]);
        assert!(lens[DOUBLING..].iter().all(|n| *n == LAST_LEN), "{:?}", lens);
        assert!(s.iter().copied().eq(0..5000));
        assert!((&s).into_iter().copied().eq(0..5000));
    }

    /// An entry's address stays as it was while the sequence grows past
    /// many chunks.
    #[test]
    fn growth_moves_no_entry() {
        let mut s: ChunkStack<u32> = ChunkStack::new();
        s.push(7);
        let first: *const u32 = &s[0];
        for i in 0..5000 {
            s.push(i);
        }
        assert_eq!(first, &s[0] as *const u32);
    }

    /// Popping and truncating across chunk boundaries, then growing again
    /// into the chunks kept.
    #[test]
    fn it_shrinks_and_grows_at_its_end() {
        let mut s: ChunkStack<usize> = ChunkStack::new();
        for i in 0..3000 {
            s.push(i);
        }
        let capacity = s.capacity();
        s.truncate(100);
        assert_eq!(s.len(), 100);
        assert_eq!(s.last(), Some(&99));
        assert!(s.iter().copied().eq(0..100));
        assert_eq!(s.pop(), Some(99));
        for i in 99..3000 {
            s.push(i);
        }
        assert!(s.iter().copied().eq(0..3000));
        assert_eq!(s.capacity(), capacity);
        s.truncate(5000);
        assert_eq!(s.len(), 3000);
        s.clear();
        assert!(s.is_empty());
        assert_eq!(s.pop(), None);
        assert_eq!(s.last(), None);
    }
}
