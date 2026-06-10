//! Object header and shape descriptors.

use std::mem::size_of;

/// Allocation alignment (satisfies f64/pointer alignment and leaves the low 3
/// bits of the TypeInfo pointer free for GC flags).
pub const ALIGN: usize = 16;

const MARK_BIT: usize = 0b001;
const TI_MASK: usize = !0b111usize;

/// Per-object header. `gc_alloc` returns a pointer to the payload that follows
/// this header; the collector recovers the header via `header(payload)`.
#[repr(C)]
pub struct ObjHeader {
    /// Pointer to the `TypeInfo`, with GC flags stolen from the low bits.
    ti_and_flags: usize,
    /// Variable element count: String = byte length, Vec/RawBuffer = element
    /// count. Unused (0) for Struct/Closure.
    pub len: usize,
}

impl ObjHeader {
    pub fn ti(&self) -> *const TypeInfo {
        (self.ti_and_flags & TI_MASK) as *const TypeInfo
    }
    pub fn is_marked(&self) -> bool {
        self.ti_and_flags & MARK_BIT != 0
    }
    pub fn set_mark(&mut self) {
        self.ti_and_flags |= MARK_BIT;
    }
    pub fn clear_mark(&mut self) {
        self.ti_and_flags &= !MARK_BIT;
    }
    pub fn set_ti(&mut self, ti: *const TypeInfo) {
        debug_assert!((ti as usize) & !TI_MASK == 0, "TypeInfo must be 8-aligned");
        let flags = self.ti_and_flags & !TI_MASK;
        self.ti_and_flags = (ti as usize) | flags;
    }
}

/// The kind of a managed object's shape.
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GcKind {
    Struct = 0,
    StringKind = 1,
    Vec = 2,
    Closure = 3,
    RawBuffer = 4,
}

/// Shape descriptor for a managed type. One `'static` instance per shape.
#[repr(C)]
pub struct TypeInfo {
    pub kind: GcKind,
    /// Struct/Closure: payload size. Vec: size of the control block. String: 0.
    pub fixed_size: usize,
    /// Byte offsets (relative to payload start) of GC-pointer fields.
    pub ptr_offsets: &'static [usize],
    /// Vec/RawBuffer only: element descriptor.
    pub elem: Option<&'static ElemInfo>,
}

unsafe impl Sync for TypeInfo {}

#[repr(C)]
pub struct ElemInfo {
    pub elem_size: usize,
    /// Whether elements are GC pointers (Vec<String> => true; Vec<i32> => false).
    pub elem_is_ptr: bool,
}

unsafe impl Sync for ElemInfo {}

/// Header size in bytes.
pub const HEADER_SIZE: usize = size_of::<ObjHeader>();

pub fn align_up(n: usize, align: usize) -> usize {
    (n + align - 1) & !(align - 1)
}

/// Recover the header from a payload pointer.
///
/// # Safety
/// `payload` must point at the payload of a live managed object.
pub unsafe fn header<'a>(payload: *mut u8) -> &'a mut ObjHeader {
    &mut *(payload.sub(HEADER_SIZE) as *mut ObjHeader)
}

/// The base allocation pointer for a payload (where dealloc must happen).
pub fn base_of(payload: *mut u8) -> *mut u8 {
    unsafe { payload.sub(HEADER_SIZE) }
}
