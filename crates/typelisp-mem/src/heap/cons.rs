//! Cons-cell allocation and access.

use crate::prelude::*;
use core::ptr;
use core::sync::atomic::Ordering;
use crate::Error;
use crate::value::{ConsRef, LocId, Value};
use super::{HEADROOM_DIVISOR, Heap};

impl Heap {
    // ---- allocation -------------------------------------------------------

    /// Allow the arena to grow by appending chunks until it holds `max_cells`,
    /// instead of reporting [`Error::HeapExhausted`] the moment a GC frees
    /// nothing. Growth never moves an existing cell (see the module doc
    /// comment), so live [`Value::Cons`] pointers stay valid across it.
    ///
    /// A limit at or below the current capacity disables growth, so
    /// `set_growth_limit(0)` restores the fixed-arena default. The ceiling is
    /// the point: it is what still turns a runaway leak into a loud error
    /// rather than letting the process be OOM-killed.
    pub fn set_growth_limit(&mut self, max_cells: usize) {
        self.shared.arena.lock().growth_limit = max_cells;
    }

    /// The current growth ceiling in cells (`0` when the arena is fixed).
    pub fn growth_limit(&self) -> usize {
        self.shared.arena.lock().growth_limit
    }

    /// Collect before *every* cons allocation. Enormously slow — this is a
    /// test/debug mode, never a production setting.
    ///
    /// It exists because a missing GC root is otherwise close to undebuggable:
    /// the allocation that frees an unrooted intermediate is rarely the one
    /// that later reads the corrupted value, so the symptom appears far from
    /// the cause and only under whatever allocation pattern happens to empty
    /// the free list. Under stress mode the two coincide — the very next
    /// `cons` after a value goes unrooted is the one that collects it — so a
    /// test that would fail once in a hundred runs fails every run.
    pub fn set_gc_stress(&mut self, on: bool) {
        self.shared.gc_stress.store(on, Ordering::Relaxed);
    }

    /// While `on`, a `cons` that finds no free cell grows the arena rather
    /// than collect, and fails with [`Error::HeapExhausted`] when it cannot
    /// grow. For code that runs where a collection is not safe: an interrupt
    /// handler, entered between any two instructions of a program that may
    /// hold a value in a register only, out of sight of the roots. Answers
    /// whether it was on before.
    pub fn inhibit_collection(&self, on: bool) -> bool {
        self.shared.collect_inhibited.swap(on, Ordering::SeqCst)
    }

    /// Whether [`Heap::set_gc_stress`] is currently on.
    pub fn gc_stress(&self) -> bool {
        self.shared.gc_stress.load(Ordering::Relaxed)
    }

    /// Append one chunk, doubling the arena but never passing `growth_limit`.
    /// Returns false when growth is disabled or the ceiling is already reached,
    /// which is what makes the caller report [`Error::HeapExhausted`].
    ///
    /// Takes the whole lock for its whole body — nothing it calls (only the
    /// self-contained, `self`-free [`Heap::alloc_chunk`]) locks the arena
    /// again, so this never nests a `Mutex::lock` inside another one on the
    /// same mutex (which would deadlock: `std::sync::Mutex` is not
    /// reentrant). [`Heap::cons`]'s doc comment explains why *its* locking is
    /// split into several short critical sections instead of one like this.
    fn grow(&mut self) -> bool {
        let mut arena = self.shared.arena.lock();
        if arena.growth_limit <= arena.cap {
            return false;
        }
        // Doubling keeps the amortized cost of growth constant. `MIN_CHUNK`
        // stops a heap created with a tiny (or zero) capacity from growing one
        // cell at a time.
        const MIN_CHUNK: usize = 1024;
        let want = arena.cap.max(MIN_CHUNK);
        let len = want.min(arena.growth_limit - arena.cap);
        if len == 0 {
            return false;
        }
        let (chunk, free) = Self::alloc_chunk(len, arena.free);
        arena.chunks.push(chunk);
        arena.cap += len;
        arena.free = free;
        arena.free_count += len;
        true
    }

    /// Allocate a cons cell `(car . cdr)`. Runs a GC if the free list is empty,
    /// then grows the arena if [`Heap::set_growth_limit`] permits; returns
    /// [`Error::HeapExhausted`] if even then no cell is available.
    ///
    /// Taking a cell is one critical section — look at the free list and
    /// pop it under the same lock — because another thread's `cons` can
    /// empty the list between two separate ones. Collecting and growing
    /// happen outside it: [`Heap::gc`] and [`Heap::grow`] take the arena
    /// lock themselves, and `std::sync::Mutex` does not reenter.
    pub fn cons(&mut self, car: Value, cdr: Value) -> Result<Value, Error> {
        self.debug_assert_not_native("cons");
        // Every cons is already a point where a collection may run (the
        // free list may be empty), so every caller already has what it
        // keeps rooted — which is exactly what makes it a safe place to let
        // *another* thread's collection run too.
        self.safepoint();
        let inhibited = self.shared.collect_inhibited.load(Ordering::SeqCst);
        if self.gc_stress() && !inhibited {
            // Deliberately collect before *every* allocation so a caller that
            // failed to root an intermediate value is caught here, at the
            // allocation that invalidates it, instead of surfacing later as
            // corruption far from the missing root. Test-only — see
            // `set_gc_stress`.
            self.gc();
        }
        loop {
            if let Some(v) = self.pop_free(car, cdr) {
                return Ok(v);
            }
            if inhibited {
                if self.grow() {
                    continue;
                }
                return Err(Error::HeapExhausted);
            }
            let reclaimed = self.gc();
            // A collection that hands back one cell is not a collection that
            // helped: with a live set just under capacity the *next* cons
            // empties the free list again, so nearly every allocation pays for
            // a full mark of the whole live set. (Measured before this check:
            // loading the prelude into a `1 << 16` heap took ~108s against 1.2s
            // in a `1 << 18` one — the same work, ~90x the time.) So grow
            // whenever the collection failed to restore a working margin, not
            // only when it freed nothing at all.
            let (free_count, cap) = {
                let arena = self.shared.arena.lock();
                (arena.free_count, arena.cap)
            };
            if free_count.saturating_mul(HEADROOM_DIVISOR) < cap {
                self.grow();
            }
            // Exhausted means what it always has: a collection freed nothing
            // and growth is not permitted. A collection that freed something
            // is worth another try even if other threads took those cells
            // first — they cannot keep doing so without garbage running out.
            if reclaimed == 0 && self.shared.arena.lock().free.is_null() && !self.grow() {
                return Err(Error::HeapExhausted);
            }
        }
    }

    /// Take a cell off the free list and fill it, or `None` if the list is
    /// empty — see [`Heap::cons`].
    fn pop_free(&mut self, car: Value, cdr: Value) -> Option<Value> {
        let mut arena = self.shared.arena.lock();
        let p = arena.free;
        if p.is_null() {
            return None;
        }
        unsafe {
            arena.free = (*p).next_free;
            (*p).car = car;
            (*p).cdr = cdr;
            (*p).mark = false;
            (*p).next_free = ptr::null_mut();
            // A recycled cell must not inherit the position of whatever form
            // used to live at this address. Blanking here is what replaced both
            // the reader table's per-batch bulk clear and the sweep's
            // entry-by-entry removal — see `Cell`'s doc comment.
            (*p).car_loc = LocId::NONE;
            (*p).self_loc = LocId::NONE;
        }
        arena.free_count -= 1;
        Some(Value::Cons(ConsRef(p)))
    }

    // ---- accessors --------------------------------------------------------

    /// `(car c)`. Errors unless `c` is a cons (the empty list has no car).
    pub fn car(&self, v: Value) -> Result<Value, Error> {
        match v {
            Value::Cons(c) => Ok(unsafe { (*c.0).car }),
            _ => Err(Error::NotACons),
        }
    }

    /// `(cdr c)`. Errors unless `c` is a cons.
    pub fn cdr(&self, v: Value) -> Result<Value, Error> {
        match v {
            Value::Cons(c) => Ok(unsafe { (*c.0).cdr }),
            _ => Err(Error::NotACons),
        }
    }

    /// `(rplaca c val)`. Errors unless `c` is a cons.
    pub fn set_car(&mut self, c: Value, val: Value) -> Result<(), Error> {
        match c {
            Value::Cons(cell) => {
                unsafe { (*cell.0).car = val };
                Ok(())
            }
            _ => Err(Error::NotACons),
        }
    }

    /// `(rplacd c val)`. Errors unless `c` is a cons.
    pub fn set_cdr(&mut self, c: Value, val: Value) -> Result<(), Error> {
        match c {
            Value::Cons(cell) => {
                unsafe { (*cell.0).cdr = val };
                Ok(())
            }
            _ => Err(Error::NotACons),
        }
    }

    /// Collect the elements of a proper list into a vector. Errors on an
    /// improper list (a non-`Empty`, non-cons tail).
    pub fn list_to_vec(&self, v: Value) -> Result<Vec<Value>, Error> {
        let mut out = Vec::new();
        let mut cur = v;
        loop {
            match cur {
                Value::Empty => return Ok(out),
                Value::Cons(c) => {
                    out.push(unsafe { (*c.0).car });
                    cur = unsafe { (*c.0).cdr };
                }
                _ => return Err(Error::ImproperList),
            }
        }
    }

    /// Like [`Heap::list_to_vec`], but pairs each element with the source
    /// location the reader recorded for it (the spine cell's own `car_loc`) —
    /// `None` for an element from a list with no location (e.g. a
    /// macro-expanded or otherwise synthesized one). The checker uses this so a
    /// bare atom in argument position can carry its own `Loc`, which the LSP's
    /// hover/goto-definition need.
    pub fn list_to_vec_locs(&self, v: Value) -> Result<Vec<(Value, Option<crate::errors::Loc>)>, Error> {
        let mut out = Vec::new();
        let mut cur = v;
        loop {
            match cur {
                Value::Empty => return Ok(out),
                Value::Cons(c) => {
                    let loc = self.loc_of(unsafe { (*c.0).car_loc });
                    out.push((unsafe { (*c.0).car }, loc));
                    cur = unsafe { (*c.0).cdr };
                }
                _ => return Err(Error::ImproperList),
            }
        }
    }
}
