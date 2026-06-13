//! A manually-managed value heap with a precise mark-sweep collector.
//!
//! # Design
//!
//! * **Fixed cons arena.** `with_capacity(n)` allocates `n` cons cells once. The
//!   buffer is never reallocated, so the raw `*mut Cell` inside every
//!   [`Value::Cons`] stays valid for the heap's lifetime. It is a single owned
//!   allocation, freed in `Drop` — the backing memory cannot leak.
//! * **Free list.** Unused cells are chained through `Cell::next_free`.
//!   Allocation pops the head; if the list is empty it runs a GC and retries.
//!   The arena never grows: a GC that frees nothing makes allocation return
//!   [`Error::HeapExhausted`] (size the arena up front instead).
//! * **Symbols** are interned (name -> [`SymId`]) and never collected — they are
//!   few and live for the heap's lifetime, like CL symbols in a package.
//! * **Strings** live in a slot store and ARE collected: the mark phase marks
//!   every reachable [`StrId`], the sweep frees unmarked slots (recycling
//!   indices). So unreachable strings do not leak.
//! * **Mark-sweep.** `gc()` marks everything reachable from the root set
//!   (iteratively — no native recursion), then rebuilds the cons free list and
//!   sweeps strings. Cycles are reclaimed (unlike reference counting).
//!
//! All `unsafe` is confined here; the public API is safe.

use std::collections::HashMap;
use std::ptr;

use crate::Error;
use super::value::{Cell, ConsRef, StrId, SymId, Value};

pub struct Heap {
    base: *mut Cell, // start of the cons arena; owns the allocation
    cap: usize,
    free: *mut Cell, // head of the free list (null when empty)
    free_count: usize,
    roots: Vec<Value>,

    // interned symbols (permanent)
    sym_names: Vec<String>,
    sym_ids: HashMap<String, u32>,

    // GC-managed string store
    str_slots: Vec<Option<String>>,
    str_free: Vec<u32>,
    str_marks: Vec<bool>,
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
            sym_names: Vec::new(),
            sym_ids: HashMap::new(),
            str_slots: Vec::new(),
            str_free: Vec::new(),
            str_marks: Vec::new(),
        }
    }

    // ---- statistics -------------------------------------------------------

    /// Total number of cons cells in the arena.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Cons cells currently on the free list (available to allocate).
    pub fn free_count(&self) -> usize {
        self.free_count
    }

    /// Cons cells currently in use (`capacity - free_count`).
    pub fn live_count(&self) -> usize {
        self.cap - self.free_count
    }

    /// Distinct interned symbols.
    pub fn symbol_count(&self) -> usize {
        self.sym_names.len()
    }

    /// Strings currently allocated (occupied slots).
    pub fn string_count(&self) -> usize {
        self.str_slots.iter().filter(|s| s.is_some()).count()
    }

    // ---- roots ------------------------------------------------------------

    /// Register `v` as a GC root.
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

    // ---- symbols ----------------------------------------------------------

    /// Intern a symbol by name, returning its `Value::Symbol`.
    ///
    /// typelisp symbols are **case-insensitive**, so names are folded to a
    /// canonical lowercase form: `Foo`, `FOO`, `foo` all intern to the same
    /// [`SymId`], and [`symbol_name`](Self::symbol_name) returns the canonical
    /// (lowercase) spelling.
    pub fn intern_symbol(&mut self, name: &str) -> Value {
        let key = name.to_lowercase();
        if let Some(&id) = self.sym_ids.get(&key) {
            return Value::Symbol(SymId(id));
        }
        let id = self.sym_names.len() as u32;
        self.sym_names.push(key.clone());
        self.sym_ids.insert(key, id);
        Value::Symbol(SymId(id))
    }

    /// The name of an interned symbol.
    pub fn symbol_name(&self, id: SymId) -> &str {
        &self.sym_names[id.0 as usize]
    }

    // ---- strings ----------------------------------------------------------

    /// Store a string, returning its `Value::Str`. Strings are GC-collected.
    pub fn alloc_string(&mut self, s: String) -> Value {
        if let Some(idx) = self.str_free.pop() {
            self.str_slots[idx as usize] = Some(s);
            self.str_marks[idx as usize] = false;
            Value::Str(StrId(idx))
        } else {
            let idx = self.str_slots.len() as u32;
            self.str_slots.push(Some(s));
            self.str_marks.push(false);
            Value::Str(StrId(idx))
        }
    }

    /// The contents of a stored string.
    pub fn string(&self, id: StrId) -> &str {
        self.str_slots[id.0 as usize].as_deref().expect("dangling StrId")
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

    // ---- collection -------------------------------------------------------

    /// Run a mark-sweep collection. Returns the number of cons cells reclaimed.
    pub fn gc(&mut self) -> usize {
        // reset string marks
        for m in self.str_marks.iter_mut() {
            *m = false;
        }

        // MARK: iterative DFS from the roots (no native recursion).
        let mut stack: Vec<*mut Cell> = Vec::new();
        for i in 0..self.roots.len() {
            match self.roots[i] {
                Value::Cons(c) => stack.push(c.0),
                Value::Str(s) => self.str_marks[s.0 as usize] = true,
                _ => {}
            }
        }
        while let Some(p) = stack.pop() {
            unsafe {
                if (*p).mark {
                    continue;
                }
                (*p).mark = true;
                for v in [(*p).car, (*p).cdr] {
                    match v {
                        Value::Cons(c) if !(*c.0).mark => stack.push(c.0),
                        Value::Str(s) => self.str_marks[s.0 as usize] = true,
                        _ => {}
                    }
                }
            }
        }

        // SWEEP cons: rebuild the free list over the whole arena.
        let old_free = self.free_count;
        let mut new_free: *mut Cell = ptr::null_mut();
        let mut new_free_count = 0usize;
        for i in (0..self.cap).rev() {
            unsafe {
                let p = self.base.add(i);
                if (*p).mark {
                    (*p).mark = false;
                } else {
                    (*p).car = Value::Empty;
                    (*p).cdr = Value::Empty;
                    (*p).next_free = new_free;
                    new_free = p;
                    new_free_count += 1;
                }
            }
        }
        self.free = new_free;
        self.free_count = new_free_count;

        // SWEEP strings: free unmarked occupied slots, recycling indices.
        for i in 0..self.str_slots.len() {
            if self.str_slots[i].is_some() && !self.str_marks[i] {
                self.str_slots[i] = None;
                self.str_free.push(i as u32);
            }
        }

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
