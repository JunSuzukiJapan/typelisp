extern crate typelisp;

use std::ptr::null_mut;
use typelisp::gc::*;

// Static shape descriptors used by the tests (these stand in for what code
// generation will emit per monomorphized type).

// A struct with two GC-pointer fields at payload offsets 0 and 8.
static PAIR_PTRS: [usize; 2] = [0, 8];
static PAIR_TI: TypeInfo = TypeInfo {
    kind: GcKind::Struct,
    fixed_size: 16,
    ptr_offsets: &PAIR_PTRS,
    elem: None,
};

// A leaf struct with no pointer fields (one i64).
static LEAF_TI: TypeInfo = TypeInfo {
    kind: GcKind::Struct,
    fixed_size: 8,
    ptr_offsets: &[],
    elem: None,
};

// A string-like object (bytes inline, no pointers).
static STRING_TI: TypeInfo = TypeInfo {
    kind: GcKind::StringKind,
    fixed_size: 0,
    ptr_offsets: &[],
    elem: None,
};

// Vec control block { data_ptr @0, cap @8 }; data_ptr is a GC child.
static VEC_PTRS: [usize; 1] = [0];
static VEC_TI: TypeInfo = TypeInfo {
    kind: GcKind::Vec,
    fixed_size: 16,
    ptr_offsets: &VEC_PTRS,
    elem: None,
};

// Element buffer of pointers (e.g. Vec<String>).
static ELEM_PTR: ElemInfo = ElemInfo { elem_size: 8, elem_is_ptr: true };
static RAWBUF_PTR_TI: TypeInfo = TypeInfo {
    kind: GcKind::RawBuffer,
    fixed_size: 0,
    ptr_offsets: &[],
    elem: Some(&ELEM_PTR),
};

unsafe fn store_ptr(payload: *mut u8, offset: usize, value: *mut u8) {
    *(payload.add(offset) as *mut *mut u8) = value;
}

fn fresh() {
    gc_init();
}

#[test]
fn alloc_alignment_and_header() {
    fresh();
    let p = gc_alloc(8, &LEAF_TI);
    assert_eq!(p as usize % 16, 0, "payload must be 16-aligned");
    // payload is zeroed
    let v = unsafe { *(p as *const i64) };
    assert_eq!(v, 0);
    assert_eq!(gc_live_count(), 1);
}

#[test]
fn garbage_is_reclaimed() {
    fresh();
    // allocate with no roots -> all collected
    let _ = gc_alloc(8, &LEAF_TI);
    let _ = gc_alloc(8, &LEAF_TI);
    assert_eq!(gc_live_count(), 2);
    gc_collect();
    assert_eq!(gc_live_count(), 0);
    assert_eq!(gc_bytes_allocated(), 0);
}

#[test]
fn rooted_object_survives() {
    fresh();
    let obj = gc_alloc(8, &LEAF_TI);
    // build a shadow frame holding &obj
    let mut slot: *mut u8 = obj;
    let slots: [*mut *mut u8; 1] = [&mut slot as *mut *mut u8];
    let mut frame = ShadowFrame { prev: null_mut(), count: 1, slots: slots.as_ptr() };
    gc_push_frame(&mut frame);

    // garbage alongside it
    let _ = gc_alloc(8, &LEAF_TI);
    gc_collect();
    assert_eq!(gc_live_count(), 1, "the rooted object survives, garbage is freed");

    gc_pop_frame();
    gc_collect();
    assert_eq!(gc_live_count(), 0);
}

#[test]
fn transitive_marking() {
    fresh();
    // pair -> (leaf_a, leaf_b)
    let leaf_a = gc_alloc(8, &LEAF_TI);
    let leaf_b = gc_alloc(8, &LEAF_TI);
    let pair = gc_alloc(16, &PAIR_TI);
    unsafe {
        store_ptr(pair, 0, leaf_a);
        store_ptr(pair, 8, leaf_b);
    }

    let mut slot: *mut u8 = pair;
    let slots: [*mut *mut u8; 1] = [&mut slot as *mut *mut u8];
    let mut frame = ShadowFrame { prev: null_mut(), count: 1, slots: slots.as_ptr() };
    gc_push_frame(&mut frame);

    gc_collect();
    assert_eq!(gc_live_count(), 3, "pair and both leaves survive");

    // drop one child reference; it should be collected
    unsafe { store_ptr(pair, 8, null_mut()) };
    gc_collect();
    assert_eq!(gc_live_count(), 2, "pair + one leaf survive");

    gc_pop_frame();
}

#[test]
fn string_has_no_children() {
    fresh();
    let s = gc_alloc(5, &STRING_TI);
    gc_set_len(s, 5);
    unsafe {
        let bytes = std::slice::from_raw_parts_mut(s, 5);
        bytes.copy_from_slice(b"hello");
    }
    let mut slot: *mut u8 = s;
    let slots: [*mut *mut u8; 1] = [&mut slot as *mut *mut u8];
    let mut frame = ShadowFrame { prev: null_mut(), count: 1, slots: slots.as_ptr() };
    gc_push_frame(&mut frame);

    gc_collect();
    assert_eq!(gc_live_count(), 1);
    unsafe {
        let bytes = std::slice::from_raw_parts(s, 5);
        assert_eq!(bytes, b"hello");
    }
    gc_pop_frame();
}

#[test]
fn vec_of_pointers_traces_elements() {
    fresh();
    // two string elements
    let s0 = gc_alloc(1, &STRING_TI);
    gc_set_len(s0, 1);
    let s1 = gc_alloc(1, &STRING_TI);
    gc_set_len(s1, 1);

    // element buffer holding [s0, s1]
    let buf = gc_alloc(2 * 8, &RAWBUF_PTR_TI);
    gc_set_len(buf, 2);
    unsafe {
        store_ptr(buf, 0, s0);
        store_ptr(buf, 8, s1);
    }
    // vec control block { data_ptr = buf, cap = 2 }
    let vec = gc_alloc(16, &VEC_TI);
    gc_set_len(vec, 2);
    unsafe {
        store_ptr(vec, 0, buf);
        *(vec.add(8) as *mut usize) = 2;
    }

    let mut slot: *mut u8 = vec;
    let slots: [*mut *mut u8; 1] = [&mut slot as *mut *mut u8];
    let mut frame = ShadowFrame { prev: null_mut(), count: 1, slots: slots.as_ptr() };
    gc_push_frame(&mut frame);

    gc_collect();
    // vec + buffer + 2 strings all reachable
    assert_eq!(gc_live_count(), 4, "vec -> buffer -> elements all survive");

    gc_pop_frame();
    gc_collect();
    assert_eq!(gc_live_count(), 0);
}

#[test]
fn global_roots_survive() {
    fresh();
    let obj = gc_alloc(8, &LEAF_TI);
    let mut slot: *mut u8 = obj;
    gc_register_global(&mut slot as *mut *mut u8);
    let _garbage = gc_alloc(8, &LEAF_TI);
    gc_collect();
    assert_eq!(gc_live_count(), 1, "global-rooted object survives");
}
