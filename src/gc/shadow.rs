//! Shadow stack and global roots.
//!
//! Compiled code links a `ShadowFrame` on function entry and unlinks it on
//! exit; each frame holds *pointers to* the function's GC-pointer locals (so
//! the collector reads their current values). Top-level `defvar`/`defconstant`
//! managed bindings register their slots as globals.

use std::cell::{Cell, RefCell};
use std::ptr::null_mut;

/// A shadow-stack frame. Layout is part of the codegen ABI.
#[repr(C)]
pub struct ShadowFrame {
    pub prev: *mut ShadowFrame,
    pub count: usize,
    /// `count` slots follow inline; each is a `*mut *mut u8` (pointer to the
    /// local's storage). Represented here as a pointer to the first slot.
    pub slots: *const *mut *mut u8,
}

thread_local! {
    static TOP: Cell<*mut ShadowFrame> = Cell::new(null_mut());
    static GLOBALS: RefCell<Vec<*mut *mut u8>> = RefCell::new(Vec::new());
}

pub fn push_frame(frame: *mut ShadowFrame) {
    TOP.with(|t| {
        unsafe { (*frame).prev = t.get() };
        t.set(frame);
    });
}

pub fn pop_frame() {
    TOP.with(|t| {
        let cur = t.get();
        if !cur.is_null() {
            t.set(unsafe { (*cur).prev });
        }
    });
}

pub fn top_addr() -> *mut *mut ShadowFrame {
    TOP.with(|t| t.as_ptr())
}

pub fn register_global(slot: *mut *mut u8) {
    GLOBALS.with(|g| g.borrow_mut().push(slot));
}

pub fn clear() {
    TOP.with(|t| t.set(null_mut()));
    GLOBALS.with(|g| g.borrow_mut().clear());
}

/// Call `f` with every non-null root pointer (from the shadow stack + globals).
pub fn for_each_root<F: FnMut(*mut u8)>(mut f: F) {
    TOP.with(|t| {
        let mut frame = t.get();
        while !frame.is_null() {
            unsafe {
                let count = (*frame).count;
                let slots = (*frame).slots;
                for i in 0..count {
                    let slot = *slots.add(i); // *mut *mut u8
                    if !slot.is_null() {
                        let root = *slot;
                        if !root.is_null() {
                            f(root);
                        }
                    }
                }
                frame = (*frame).prev;
            }
        }
    });
    GLOBALS.with(|g| {
        for &slot in g.borrow().iter() {
            unsafe {
                if !slot.is_null() {
                    let root = *slot;
                    if !root.is_null() {
                        f(root);
                    }
                }
            }
        }
    });
}
