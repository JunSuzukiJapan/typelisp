//! Compiled frames: the per-task arena their words live in, and the heap's
//! operations on them.

use crate::chunked::ChunkStack;
use crate::prelude::*;
use crate::symbols::SymRef;
use crate::sync::atomic::{AtomicUsize, Ordering};
use crate::tagged::{raw_value, value_words, Word, VALUE_WORDS};
use crate::value::{BoxId, BoxedObj, TypeId, Value};
use super::Heap;

/// Where a compiled frame's parts are: its words and its mask — for reading
/// and writing them with no lookup at all.
///
/// A frame is named by the address of its words (what compiled code is handed
/// on resuming, and what the driver keeps), and its length and mask sit in the
/// header just below them ([`FrameArena`]), so this is rebuilt from that
/// address alone ([`FrameRaw::from_data`]).
///
/// Valid for as long as the frame is: a frame never moves (the arena's chunks
/// are never relocated). Only the activation running a frame touches it, and a
/// collection reads it only while every thread is stopped. Using one after its
/// frame was closed is the caller's bug.
#[derive(Clone, Copy, Debug)]
pub struct FrameRaw {
    words: *mut Word,
    len: usize,
    mask: *const u64,
}

// SAFETY: the pointers are into a root stack's arena, which every thread may
// use; see the type's documentation for who may use them when.
unsafe impl Send for FrameRaw {}
unsafe impl Sync for FrameRaw {}

impl FrameRaw {
    /// The frame whose words start at `data`.
    ///
    /// # Safety
    ///
    /// `data` must be the address of a live frame's words, as
    /// [`Heap::open_frame`] handed it out.
    pub unsafe fn from_data(data: *mut Word) -> FrameRaw {
        FrameRaw {
            words: data,
            len: *data.sub(FRAME_HEADER - FRAME_HEADER_LEN) as usize,
            mask: *data.sub(FRAME_HEADER - FRAME_HEADER_MASK) as usize as *const u64,
        }
    }

    /// The address of the word array — the frame's name.
    pub fn data(self) -> *mut Word {
        self.words
    }

    pub fn len(self) -> usize {
        self.len
    }

    pub fn is_empty(self) -> bool {
        self.len == 0
    }

    /// # Safety
    ///
    /// The frame must still be live.
    pub unsafe fn word(self, idx: usize) -> Word {
        assert!(idx < self.len, "frame slot {} out of range", idx);
        *self.words.add(idx)
    }

    /// # Safety
    ///
    /// The frame must still be live.
    pub unsafe fn set_word(self, idx: usize, w: Word) {
        assert!(idx < self.len, "frame slot {} out of range", idx);
        *self.words.add(idx) = w;
    }

    /// The 64 bits starting at slot `idx`: a value as wide as any is, in as
    /// many words as that takes, the low one first. The words above a
    /// narrower value are whatever was there.
    ///
    /// # Safety
    ///
    /// The frame must still be live.
    pub unsafe fn value(self, idx: usize) -> i64 {
        assert!(idx + VALUE_WORDS <= self.len, "frame slot {} out of range", idx);
        raw_value(core::slice::from_raw_parts(self.words.add(idx), VALUE_WORDS))
    }

    /// Writes all 64 bits [`value`](Self::value) reads.
    ///
    /// # Safety
    ///
    /// The frame must still be live.
    pub unsafe fn set_value(self, idx: usize, v: i64) {
        assert!(idx + VALUE_WORDS <= self.len, "frame slot {} out of range", idx);
        for (i, w) in value_words(v, i64::BITS).enumerate() {
            *self.words.add(idx + i) = w;
        }
    }

    /// Whether slot `idx` holds a tagged word the collector traces.
    ///
    /// # Safety
    ///
    /// The frame must still be live.
    pub unsafe fn is_rooted(self, idx: usize) -> bool {
        assert!(idx < self.len, "frame slot {} out of range", idx);
        !self.mask.is_null() && *self.mask.add(idx / 64) & (1u64 << (idx % 64)) != 0
    }
}

/// The words in front of each frame's own, in a [`FrameArena`]: the root
/// stack depth the frame was opened at, its length, and its mask table.
const FRAME_HEADER: usize = 3;
const FRAME_HEADER_DEPTH: usize = 0;
const FRAME_HEADER_LEN: usize = 1;
const FRAME_HEADER_MASK: usize = 2;

/// How many words a root stack's first frame chunk holds; each later one
/// doubles, up to [`frame_chunk_max`].
const FRAME_CHUNK_MIN: usize = 256;
const FRAME_CHUNK_MAX: usize = 1 << 16;

/// The most words a frame chunk is made with (a frame larger than that gets
/// a chunk of its own size): [`FRAME_CHUNK_MAX`], or, where the memory there
/// is is known (a board's heap), as many as fit an eighth of it, as the
/// heap's first allocations are sized (`Heap::fitting_cells`).
fn frame_chunk_max() -> usize {
    let room = crate::memory::capacity();
    if room == usize::MAX {
        return FRAME_CHUNK_MAX;
    }
    (room / 8 / size_of::<Word>()).clamp(FRAME_CHUNK_MIN, FRAME_CHUNK_MAX)
}

/// The most bytes the calls in progress of one task may take (`typl -c
/// --frame-memory`): its frames' words and the tables that keep them
/// ([`frame_cost`]). Unbounded until [`set_frame_memory`] gives a bound.
static FRAME_MEMORY: AtomicUsize = AtomicUsize::new(usize::MAX);

/// What the driver keeps of each frame, in bytes, beside what the heap does
/// ([`set_frame_memory`]).
static DRIVER_ENTRY: AtomicUsize = AtomicUsize::new(0);

/// Bounds the calls in progress of each task at `bytes` from now on; the
/// driver keeps `driver_entry` bytes of each frame of its own. Before any
/// frame is opened.
pub fn set_frame_memory(bytes: usize, driver_entry: usize) {
    FRAME_MEMORY.store(bytes, Ordering::Relaxed);
    DRIVER_ENTRY.store(driver_entry, Ordering::Relaxed);
}

/// The bound [`set_frame_memory`] gave.
pub fn frame_memory() -> usize {
    FRAME_MEMORY.load(Ordering::Relaxed)
}

/// What a frame of `nslots` words takes: its words and header in a chunk,
/// the placeholder root its prologue pushes, its address in the open
/// frames, and the driver's entry for it. A direct function's frame, which
/// no driver keeps, is counted the same.
fn frame_cost(nslots: usize) -> usize {
    (FRAME_HEADER + nslots) * size_of::<Word>() + size_of::<Value>() + size_of::<*mut Word>() + DRIVER_ENTRY.load(Ordering::Relaxed)
}

/// A frame that would take a task's calls in progress past
/// [`frame_memory`].
#[derive(Debug)]
pub struct FrameMemoryExceeded {
    /// What the calls in progress take without it.
    pub in_use: usize,
    /// What the frame would add.
    pub cost: usize,
}

/// The compiled frames of one root stack — one task's — in the order they were
/// opened.
///
/// **A stack, because a task's frames are.** A frame is opened by a prologue
/// and closed when its activation is over (a return, or an unwind leaving
/// it), and the frames of one task nest: the driver pushes on a call and pops
/// on a return, and a nested driver on a machine frame finishes before the
/// activation that started it goes on. So opening is a bump and closing a
/// pop, where a box in the shared table cost a lock, a slot and an allocation
/// each way.
///
/// **Per root stack, because a task moves.** A task can be put down on one
/// worker thread and picked up on another, and its frames go with it — as its
/// root stack does. The collector walks every root stack, so it walks these
/// with them ([`Heap::trace_frames`]).
///
/// **Closed by depth.** Each frame records the root stack depth it was opened
/// at, where its prologue pushed a placeholder root. Truncating the root
/// stack to a depth at or below it — the driver leaving the frame, or anyone
/// abandoning everything above a point they recorded — closes it too, so a
/// chain abandoned with its roots cannot leave frames behind.
///
/// Chunks are never moved or resized once made, which is what keeps a frame's
/// address its name for as long as it is open.
#[derive(Default)]
pub(super) struct FrameArena {
    chunks: Vec<Box<[Word]>>,
    /// How much of each chunk is in use, from its start.
    used: Vec<usize>,
    /// The chunk the next frame goes in, or the last one when it is full.
    cur: usize,
    /// The open frames' word addresses, innermost last.
    pub(super) open: ChunkStack<*mut Word>,
    /// What the open frames take ([`frame_cost`]).
    bytes: usize,
}

// SAFETY: `open` points into `chunks`, which the arena owns; the root stack it
// belongs to is used by one thread at a time, as the rest of it is.
unsafe impl Send for FrameArena {}

impl FrameArena {
    /// A new frame of `nslots` zeroed words with mask table `mask`, opened at
    /// root stack depth `depth`.
    fn open(&mut self, nslots: usize, mask: *const u64, depth: usize) -> FrameRaw {
        self.bytes += frame_cost(nslots);
        let need = FRAME_HEADER + nslots;
        if self.chunks.is_empty() || self.used[self.cur] + need > self.chunks[self.cur].len() {
            if !self.chunks.is_empty() && self.used[self.cur] > 0 {
                self.cur += 1;
            }
            if self.cur < self.chunks.len() && self.chunks[self.cur].len() < need {
                self.chunks.truncate(self.cur);
                self.used.truncate(self.cur);
            }
            if self.cur == self.chunks.len() {
                let size = self.chunks.last().map_or(FRAME_CHUNK_MIN, |c| (c.len() * 2).min(frame_chunk_max()));
                self.chunks.push(vec![0; size.max(need)].into_boxed_slice());
                self.used.push(0);
            }
        }
        let at = self.used[self.cur];
        self.used[self.cur] = at + need;
        let block = &mut self.chunks[self.cur][at..at + need];
        block[FRAME_HEADER_DEPTH] = depth as Word;
        block[FRAME_HEADER_LEN] = nslots as Word;
        block[FRAME_HEADER_MASK] = mask as usize as Word;
        block[FRAME_HEADER..].fill(0);
        let data = block[FRAME_HEADER..].as_mut_ptr();
        self.open.push(data);
        FrameRaw { words: data, len: nslots, mask }
    }

    /// Closes every frame opened at depth `depth` or deeper, innermost first.
    pub(super) fn close_from(&mut self, depth: usize) {
        while let Some(&data) = self.open.last() {
            // SAFETY: an open frame's header is in its chunk.
            let opened_at = unsafe { *data.sub(FRAME_HEADER - FRAME_HEADER_DEPTH) } as usize;
            if opened_at < depth {
                break;
            }
            self.open.pop();
            let len = unsafe { *data.sub(FRAME_HEADER - FRAME_HEADER_LEN) } as usize;
            self.used[self.cur] -= FRAME_HEADER + len;
            self.bytes -= frame_cost(len);
            if self.used[self.cur] == 0 && self.cur > 0 {
                self.cur -= 1;
            }
        }
    }
}

impl Heap {
    // ---- compiled frames --------------------------------------------------

    /// Opens a compiled function's activation record on the running task's
    /// root stack: `nslots` words, all zero, whose mask is the function's
    /// table — `mask`, `nslots.div_ceil(64)` words the compiler emitted beside
    /// its code, or null for a function that roots no slot. See
    /// [`FrameArena`].
    ///
    /// Pushes a placeholder root first and records the frame at its depth, so
    /// that the frame is closed exactly when that root goes:
    /// [`truncate_roots`](Self::truncate_roots) to the depth it was opened at
    /// closes it, and nothing the activation does to its own roots above can.
    /// The placeholder is a fixnum, which the collector follows nowhere.
    ///
    /// Refuses a frame that would take the task's calls in progress past
    /// [`frame_memory`], opening nothing.
    ///
    /// Zero, not `Value::Empty`'s encoding: an unmasked slot is a raw word and
    /// zero is a fine raw word, while a masked slot reads as a fixnum 0 until
    /// the prologue writes it — so a collection between opening and the first
    /// store traces nothing bogus. Never itself triggers a collection.
    ///
    /// # Safety
    ///
    /// A non-null `mask` must hold `nslots.div_ceil(64)` words and outlive the
    /// frame.
    pub unsafe fn open_frame(&mut self, nslots: usize, mask: *const u64) -> Result<FrameRaw, FrameMemoryExceeded> {
        self.debug_assert_not_native("open_frame");
        let (in_use, cost) = ((*self.current).frames.bytes, frame_cost(nslots));
        if in_use + cost > frame_memory() {
            return Err(FrameMemoryExceeded { in_use, cost });
        }
        let depth = self.root_count();
        self.roots_mut().push(Value::Int(0));
        Ok((*self.current).frames.open(nslots, mask, depth))
    }

    /// Closes the frame whose words start at `data`, and every frame and root
    /// above it: truncates the root stack to the depth it was opened at.
    ///
    /// # Safety
    ///
    /// `data` must be the address of a frame [`open_frame`](Self::open_frame)
    /// opened on the running task's root stack and not yet closed.
    pub unsafe fn close_frame(&mut self, data: *mut Word) {
        let depth = *data.sub(FRAME_HEADER - FRAME_HEADER_DEPTH) as usize;
        self.truncate_roots(depth);
    }

    /// How many frames are open on the running task's root stack.
    pub fn open_frame_count(&self) -> usize {
        unsafe { &*self.current }.frames.open.len()
    }

    /// Store an *interpreted* closure — parameter list, return
    /// representation, body, and captured environment, all core forms and all
    /// `Value` — returning its `Value::Boxed`. See [`BoxedObj::Closure`].
    pub fn alloc_closure(&mut self, params: Value, ret: Value, body: Value, env: Value) -> Value {
        self.alloc_boxed(BoxedObj::Closure { params, ret, body, env })
    }

    /// True if `id` holds a [`BoxedObj::Closure`].
    pub fn is_closure(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Closure { .. }))
    }

    /// An interpreted closure's parts: `(params, body, env)`. Panics if `id`
    /// does not hold one — the same internal-invariant-trap convention as
    /// [`f64_value`](Self::f64_value).
    pub fn closure_parts(&self, id: BoxId) -> (Value, Value, Value) {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Closure { params, body, env, .. }) => (*params, *body, *env),
            _ => panic!("BoxId does not hold a Closure"),
        }
    }

    /// An interpreted closure's declared return representation — the core
    /// form, not a decoded `Repr` (this crate has no notion of one). Only a
    /// boundary crossing needs it; see [`BoxedObj::Closure`]. Panics like
    /// [`closure_parts`](Self::closure_parts).
    pub fn closure_ret(&self, id: BoxId) -> Value {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Closure { ret, .. }) => *ret,
            _ => panic!("BoxId does not hold a Closure"),
        }
    }

    // An interpreted closure is immutable, deliberately: `labels` needs a
    // group of siblings that can all see each other, and it gets that by
    // putting empty *cells* in a frame first and filling them in after each
    // closure is built. The closures capture the finished environment from
    // the start, so there is nothing to tie back — and so no setter here.

    /// Store a built-in used as a function value, returning its
    /// `Value::Boxed` — `recv_type` is the receiver type for a built-in
    /// *method* (`i32::+`) and `None` for a free built-in (`eval`). See
    /// [`BoxedObj::Builtin`].
    ///
    /// `name` is the builtin's own symbol, so the box holds only permanent
    /// references and allocating one never triggers a collection (like
    /// [`alloc_cell`](Self::alloc_cell)).
    pub fn alloc_builtin_fn(&mut self, recv_type: Option<SymRef>, name: SymRef, ret_type: Option<TypeId>) -> Value {
        self.alloc_boxed(BoxedObj::Builtin { recv_type, name, ret_type })
    }

    /// True if `id` holds a `BoxedObj::Builtin`.
    pub fn is_builtin_fn(&self, id: BoxId) -> bool {
        matches!(self.shared.boxes.read_recursive().slots[id.0 as usize], Some(BoxedObj::Builtin { .. }))
    }

    /// A built-in function value's receiver type — `None` for a free
    /// built-in. Panics if `id` doesn't hold a `BoxedObj::Builtin`, the same
    /// internal-invariant-trap convention as
    /// [`f64_value`](Self::f64_value).
    pub fn builtin_fn_recv(&self, id: BoxId) -> Option<SymRef> {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Builtin { recv_type, .. }) => *recv_type,
            _ => panic!("BoxId does not hold a Builtin"),
        }
    }

    /// A built-in function value's symbol. Panics like
    /// [`builtin_fn_recv`](Self::builtin_fn_recv).
    pub fn builtin_fn_name(&self, id: BoxId) -> SymRef {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Builtin { name, .. }) => *name,
            _ => panic!("BoxId does not hold a Builtin"),
        }
    }

    /// The runtime identity of what a built-in function value returns — see
    /// [`BoxedObj::Builtin`]'s `ret_type`.
    pub fn builtin_fn_ret_type(&self, id: BoxId) -> Option<TypeId> {
        match &self.shared.boxes.read_recursive().slots[id.0 as usize] {
            Some(BoxedObj::Builtin { ret_type, .. }) => *ret_type,
            _ => panic!("BoxId does not hold a Builtin"),
        }
    }
}
