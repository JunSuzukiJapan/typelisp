//! The C FFI's string conversions and native-section markers.

use typelisp_abi::{active_heap, decode, encode, fatal, raise};
use typelisp_mem::Value;
use crate::ffi_callback;

// ---- the C FFI's string conversions --------------------------------------
//
// A typelisp `string` is a Rust `String` in the heap's string table: not
// NUL-terminated, and free to contain a NUL of its own. A C `const char *` is
// the opposite on both counts. So a string argument is *copied* into a C
// string for the duration of one call and freed after it, and a string result
// is copied back out.
//
// These live here rather than in `os`: that module is the one place that calls
// the C library, and none of this does — `CString` is Rust's, and the
// allocation is Rust's own.

/// `args[0]` (a tagged `Value::Str`) as a freshly allocated C string, returned
/// as its address.
///
/// The caller owns it and must hand it to [`rt_ffi_cstring_free`]; the FFI
/// thunk emits both, around the one call in between.
///
/// # Safety
///
/// `args` must point to at least one readable `i64`, and a `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_ffi_cstring_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ffi_cstring_new: expected 1 argument");
    }
    let s = match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        other => fatal(&format!("rt_ffi_cstring_new: argument is not a string, got {:?}", other)),
    };
    // A NUL inside the string is a real value here and cannot be one there:
    // C would read the prefix and call it the whole string. Raised, not
    // aborted — the program handed over a string it is allowed to hold.
    match std::ffi::CString::new(s) {
        Ok(c) => c.into_raw() as i64,
        Err(_) => raise(
            "ffi: a string argument contains a NUL character, which cannot be passed to C — \
             C would see only the part before it"
                .to_string(),
        ).into_word(),
    }
}

/// Marks the start of a C call: from here until [`rt_ffi_leave_native`]
/// another thread's collection need not wait for this one, however long the
/// C function blocks. The FFI thunk emits the pair around the one call, after
/// the arguments have become C values and before the answer becomes a Lisp
/// one — the C function is handed no heap value, so it has none to touch.
///
/// # Safety
///
/// A `Heap` must be registered on this thread and be running (not already
/// native).
#[no_mangle]
pub unsafe extern "C" fn rt_ffi_enter_native(_args: *const i64, _argc: u32) -> i64 {
    active_heap().enter_native();
    0
}

/// Ends what [`rt_ffi_enter_native`] began, waiting out a collection in
/// progress first.
///
/// # Safety
///
/// A `Heap` must be registered on this thread, inside `rt_ffi_enter_native`.
///
/// Also where a callback's failure kept while C was running is raised again
/// — the first point after the C call where typelisp runs again (see
/// [`ffi_callback`]).
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_ffi_leave_native(_args: *const i64, _argc: u32) -> i64 {
    active_heap().leave_native();
    match ffi_callback::resume_pending() {
        Ok(()) => 0,
        Err(raised) => raised.into_word(),
    }
}

/// Frees what [`rt_ffi_cstring_new`] returned.
///
/// # Safety
///
/// `args[0]` must be an address that call returned and that has not been freed.
#[no_mangle]
pub unsafe extern "C" fn rt_ffi_cstring_free(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ffi_cstring_free: expected 1 argument");
    }
    let p = *args as usize;
    if p != 0 {
        drop(std::ffi::CString::from_raw(p as *mut std::ffi::c_char));
    }
    0
}

/// The C string at `args[0]`, copied onto the heap as a tagged `Value::Str`.
///
/// **Copies, and does not free.** What C handed back is C's — it may be a
/// pointer into a static table (`getenv`), and freeing it would be wrong far
/// more often than right. A function that returns memory the caller must free
/// should be declared as returning `ptr`, so the freeing is written down.
///
/// # Safety
///
/// `args[0]` must be null or a pointer to a NUL-terminated string that stays
/// valid for the duration of this call. A `Heap` must be registered on this
/// thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_ffi_string_from_cstr(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ffi_string_from_cstr: expected 1 argument");
    }
    let p = *args as usize;
    if p == 0 {
        // `string` has no value for "there wasn't one". Saying so beats
        // inventing an empty string, and the fix is in the declaration.
        return raise(
            "ffi: the C function returned a null pointer where a `string` was declared. \
             Declare the result as `ptr` if it can be null."
                .to_string(),
        ).into_word();
    }
    let bytes = std::ffi::CStr::from_ptr(p as *const std::ffi::c_char).to_bytes();
    match std::str::from_utf8(bytes) {
        Ok(s) => {
            let s = s.to_string();
            encode(active_heap().alloc_string(s))
        }
        Err(_) => raise(
            "ffi: the C function returned bytes that are not valid UTF-8, and a typelisp \
             `string` is text. Declare the result as `ptr` to handle the bytes yourself."
                .to_string(),
        ).into_word(),
    }
}
