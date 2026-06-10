//! Runtime entry points exposed to compiled code (and used by GC tests).
//!
//! Single-threaded JIT: the heap and shadow stack live in thread-locals. Code
//! generation binds these symbols via `ExecutionEngine::add_global_mapping`.

use std::cell::RefCell;

use crate::gc::heap::Heap;
use crate::gc::shadow;
use crate::gc::types::{header, TypeInfo};

thread_local! {
    static HEAP: RefCell<Heap> = RefCell::new(Heap::new());
}

/// Reset the heap (start of a run).
#[no_mangle]
pub extern "C" fn gc_init() {
    HEAP.with(|h| h.borrow_mut().shutdown());
    shadow::clear();
}

/// Free everything and clear roots (end of a run).
#[no_mangle]
pub extern "C" fn gc_shutdown() {
    HEAP.with(|h| h.borrow_mut().shutdown());
    shadow::clear();
}

/// Force a collection.
#[no_mangle]
pub extern "C" fn gc_collect() {
    HEAP.with(|h| h.borrow_mut().collect());
}

/// Allocate a managed object: `payload_size` payload bytes described by `ti`.
/// Returns a pointer to the zeroed payload.
#[no_mangle]
pub extern "C" fn gc_alloc(payload_size: usize, ti: *const TypeInfo) -> *mut u8 {
    HEAP.with(|h| h.borrow_mut().alloc(payload_size, ti))
}

/// Set the variable element count in an object's header (String byte length,
/// Vec/RawBuffer element count).
#[no_mangle]
pub extern "C" fn gc_set_len(payload: *mut u8, len: usize) {
    unsafe { header(payload).len = len };
}

/// Read the element count from an object's header.
#[no_mangle]
pub extern "C" fn gc_len(payload: *mut u8) -> usize {
    unsafe { header(payload).len }
}

// ---- shadow stack (thin extern wrappers) -----------------------------------

#[no_mangle]
pub extern "C" fn gc_push_frame(frame: *mut shadow::ShadowFrame) {
    shadow::push_frame(frame);
}

#[no_mangle]
pub extern "C" fn gc_pop_frame() {
    shadow::pop_frame();
}

#[no_mangle]
pub extern "C" fn gc_stack_top_addr() -> *mut *mut shadow::ShadowFrame {
    shadow::top_addr()
}

#[no_mangle]
pub extern "C" fn gc_register_global(slot: *mut *mut u8) {
    shadow::register_global(slot);
}

// ---- test/inspection helpers ----------------------------------------------

/// Bytes currently allocated (test hook).
pub fn gc_bytes_allocated() -> usize {
    HEAP.with(|h| h.borrow().bytes_allocated())
}

/// Number of live tracked objects (test hook).
pub fn gc_live_count() -> usize {
    HEAP.with(|h| h.borrow().live_count())
}
