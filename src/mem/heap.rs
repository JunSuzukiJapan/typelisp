//! A manually-managed cons-cell heap with a precise mark-sweep collector.
//!
//! # Design
//!
//! * **Fixed arena.** `with_capacity(n)` allocates `n` cells once. The buffer
//!   is never reallocated, so the raw `*mut Cell` inside every [`Value::Cons`]
//!   stays valid for the heap's lifetime. The arena is a single owned
//!   allocation, freed in `Drop` — the backing memory cannot leak.
//! * **Free list.** Unused cells are chained through `Cell::next_free`.
//!   Allocation pops the head; if the list is empty it runs a GC and retries.
//!   The arena never grows: if a GC frees nothing, allocation returns
//!   [`Error::HeapExhausted`] (size the arena up front instead).
//! * **Mark-sweep.** `gc()` marks every cell reachable from the root set
//!   (iteratively — no native recursion, so deep/long structures are safe),
//!   then rebuilds the free list from all unmarked cells. Cycles are reclaimed
//!   (unlike reference counting).
//!
//! All `unsafe` is confined here; the public API is safe.

use std::ptr;

use crate::Error;
use super::value::{Cell, ConsRef, Value};

pub struct Heap {
    base: *mut Cell, // start of the arena; owns the allocation
    cap: usize,
    free: *mut Cell, // head of the free list (null when empty)
    free_count: usize,
    roots: Vec<Value>,
}

impl Heap {
    /// Create a heap with `capacity` cons cells pre-allocated.
    pub fn with_capacity(capacity: usize) -> Heap {
        let mut v: Vec<Cell> = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            v.push(Cell::blank());
        }
        let boxed: Box<[Cell]> = v.into_boxed_slice();
        let base: *mut Cell = Box::into_raw(boxed) as *mut Cell;

        // Link every cell onto the free list: 0 -> 1 -> ... -> n-1 -> null.
        unsafe {
            for i in 0..capacity {
                let p = base.add(i);
                (*p).next_free = if i + 1 < capacity { base.add(i + 1) } else { ptr::null_mut() };
            }
        }

        Heap {
            base,
            cap: capacity,
            free: if capacity > 0 { base } else { ptr::null_mut() },
            free_count: capacity,
            roots: Vec::new(),
        }
    }

    // ---- statistics -------------------------------------------------------

    /// Total number of cells in the arena.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Cells currently on the free list (available to allocate).
    pub fn free_count(&self) -> usize {
        self.free_count
    }

    /// Cells currently in use (`capacity - free_count`).
    pub fn live_count(&self) -> usize {
        self.cap - self.free_count
    }

    // ---- roots ------------------------------------------------------------

    /// Register `v` as a GC root (keeps it and everything it reaches alive).
    pub fn push_root(&mut self, v: Value) {
        self.roots.push(v);
    }

    /// Remove the most recently pushed root.
    pub fn pop_root(&mut self) -> Option<Value> {
        self.roots.pop()
    }

    /// Number of registered roots.
    pub fn root_count(&self) -> usize {
        self.roots.len()
    }

    // ---- allocation -------------------------------------------------------

    /// Allocate a cons cell `(car . cdr)`. Runs a GC if the free list is empty;
    /// returns [`Error::HeapExhausted`] if even then no cell is available.
    pub fn cons(&mut self, car: Value, cdr: Value) -> Result<Value, Error> {
        if self.free.is_null() {
            self.gc();
            if self.free.is_null() {
                return Err(Error::HeapExhausted);
            }
        }
        let p = self.free;
        unsafe {
            self.free = (*p).next_free;
            (*p).car = car;
            (*p).cdr = cdr;
            (*p).mark = false;
            (*p).next_free = ptr::null_mut();
        }
        self.free_count -= 1;
        Ok(Value::Cons(ConsRef(p)))
    }

    // ---- accessors --------------------------------------------------------

    /// `(car v)`. `car` of `nil` is `nil`; of a non-cons it is an error.
    pub fn car(&self, v: Value) -> Result<Value, Error> {
        match v {
            Value::Nil => Ok(Value::Nil),
            Value::Cons(c) => Ok(unsafe { (*c.0).car }),
            _ => Err(Error::NotACons),
        }
    }

    /// `(cdr v)`. `cdr` of `nil` is `nil`; of a non-cons it is an error.
    pub fn cdr(&self, v: Value) -> Result<Value, Error> {
        match v {
            Value::Nil => Ok(Value::Nil),
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

    // ---- collection -------------------------------------------------------

    /// Run a mark-sweep collection. Returns the number of cells reclaimed
    /// (cells that were live garbage before this call).
    pub fn gc(&mut self) -> usize {
        // MARK: iterative DFS from the roots (no native recursion).
        let mut stack: Vec<*mut Cell> = Vec::new();
        for &r in &self.roots {
            if let Value::Cons(c) = r {
                stack.push(c.0);
            }
        }
        while let Some(p) = stack.pop() {
            unsafe {
                if (*p).mark {
                    continue;
                }
                (*p).mark = true;
                if let Value::Cons(c) = (*p).car {
                    if !(*c.0).mark {
                        stack.push(c.0);
                    }
                }
                if let Value::Cons(c) = (*p).cdr {
                    if !(*c.0).mark {
                        stack.push(c.0);
                    }
                }
            }
        }

        // SWEEP: rebuild the free list over the whole arena. Unmarked cells
        // (including ones already free) go back on the list; marked cells are
        // kept and their mark bit cleared for the next cycle.
        let old_free = self.free_count;
        let mut new_free: *mut Cell = ptr::null_mut();
        let mut new_free_count = 0usize;
        for i in (0..self.cap).rev() {
            unsafe {
                let p = self.base.add(i);
                if (*p).mark {
                    (*p).mark = false;
                } else {
                    (*p).car = Value::Nil;
                    (*p).cdr = Value::Nil;
                    (*p).next_free = new_free;
                    new_free = p;
                    new_free_count += 1;
                }
            }
        }
        self.free = new_free;
        self.free_count = new_free_count;
        new_free_count - old_free
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        if !self.base.is_null() {
            // Reconstitute the owning Box and drop it, freeing the arena.
            unsafe {
                let raw = ptr::slice_from_raw_parts_mut(self.base, self.cap);
                drop(Box::from_raw(raw));
            }
        }
    }
}
