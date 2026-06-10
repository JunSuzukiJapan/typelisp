//! Non-moving mark-sweep heap.
//!
//! M9 uses a simple `malloc`-backed allocator with an object registry: each
//! object is one `alloc_zeroed`, tracked for sweeping. (Size-class free lists
//! are a later optimization; correctness comes first.) The collector is precise
//! via the shadow stack and never moves objects.

use std::alloc::{alloc_zeroed, dealloc, Layout};

use crate::gc::shadow;
use crate::gc::types::*;

struct Record {
    base: *mut u8,
    layout: Layout,
}

pub struct Heap {
    objects: Vec<Record>,
    bytes_allocated: usize,
    threshold: usize,
}

const MIN_THRESHOLD: usize = 1 << 20; // 1 MiB

impl Heap {
    pub fn new() -> Heap {
        Heap { objects: Vec::new(), bytes_allocated: 0, threshold: MIN_THRESHOLD }
    }

    pub fn bytes_allocated(&self) -> usize {
        self.bytes_allocated
    }
    pub fn live_count(&self) -> usize {
        self.objects.len()
    }

    /// Allocate a managed object with `payload_size` payload bytes described by
    /// `ti`. Returns a pointer to the (zeroed) payload. May trigger a collection.
    pub fn alloc(&mut self, payload_size: usize, ti: *const TypeInfo) -> *mut u8 {
        let total = align_up(HEADER_SIZE + payload_size, ALIGN);
        if self.bytes_allocated + total > self.threshold {
            self.collect();
        }
        let layout = Layout::from_size_align(total, ALIGN).expect("bad layout");
        let base = unsafe { alloc_zeroed(layout) };
        assert!(!base.is_null(), "out of memory");
        unsafe {
            let h = &mut *(base as *mut ObjHeader);
            h.set_ti(ti);
            h.len = 0;
        }
        self.objects.push(Record { base, layout });
        self.bytes_allocated += total;
        unsafe { base.add(HEADER_SIZE) }
    }

    /// Run a full stop-the-world collection.
    pub fn collect(&mut self) {
        // Mark.
        let mut work: Vec<*mut u8> = Vec::new();
        shadow::for_each_root(|root| mark(root, &mut work));
        drain(&mut work);

        // Sweep.
        let mut freed = 0usize;
        self.objects.retain(|rec| unsafe {
            let h = &mut *(rec.base as *mut ObjHeader);
            if h.is_marked() {
                h.clear_mark();
                true
            } else {
                freed += rec.layout.size();
                dealloc(rec.base, rec.layout);
                false
            }
        });
        self.bytes_allocated -= freed;
        self.threshold = std::cmp::max(MIN_THRESHOLD, self.bytes_allocated * 2);
    }

    /// Free every tracked object (teardown). Pointers become invalid.
    pub fn shutdown(&mut self) {
        for rec in self.objects.drain(..) {
            unsafe { dealloc(rec.base, rec.layout) };
        }
        self.bytes_allocated = 0;
        self.threshold = MIN_THRESHOLD;
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Mark a single object grey (push to the work list) if not already marked.
fn mark(payload: *mut u8, work: &mut Vec<*mut u8>) {
    unsafe {
        let h = header(payload);
        if h.is_marked() {
            return;
        }
        h.set_mark();
        work.push(payload);
    }
}

/// Trace all reachable objects from the work list.
fn drain(work: &mut Vec<*mut u8>) {
    while let Some(p) = work.pop() {
        unsafe {
            let h = header(p);
            let ti = &*h.ti();
            match ti.kind {
                GcKind::Struct | GcKind::Closure | GcKind::Vec => {
                    for &off in ti.ptr_offsets {
                        let child = *(p.add(off) as *const *mut u8);
                        if !child.is_null() {
                            mark(child, work);
                        }
                    }
                }
                GcKind::StringKind => { /* no pointers */ }
                GcKind::RawBuffer => {
                    if let Some(ei) = ti.elem {
                        if ei.elem_is_ptr {
                            for i in 0..h.len {
                                let slot = p.add(i * ei.elem_size) as *const *mut u8;
                                let e = *slot;
                                if !e.is_null() {
                                    mark(e, work);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
