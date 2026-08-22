//! What an AOT-linked executable knows about its own program, and how it
//! comes to know it.
//!
//! The renderer needs two facts it cannot read off the heap — an enum's
//! variant *names*, and whether a type has a `print-object` method (see
//! [`crate::PrintEnv`]). The interpreter answers both by asking its scope
//! tree. A standalone executable has no scope tree and no interpreter, so
//! `compile::aot::build_main_wrapper` emits calls to the two registration
//! shims below into the generated `main`, turning those facts into tables
//! before anything can print.
//!
//! # What is deliberately *not* registered
//!
//! The eleven printer control variables (`*print-pretty*`, `*print-circle*`,
//! `*print-base*`, …). `compile-file` compiles one self-contained source
//! file against the compiler island alone — it never loads the prelude,
//! which is where those globals are defined — so an AOT program has none of
//! them, and CL's initial values are the right and only answer. That is what
//! [`crate::runtime::BARE_HOOKS`] already gives, and what the hooks here
//! keep: `opts`/`print_vars` are its two verbatim.
//!
//! # Why the registration shims live here rather than in `typelisp-rt`
//!
//! Same reason [`crate::shim`] does, and it matters more here: these calls
//! are what make a program's `main` reference the printer at all. Emitting
//! them unconditionally would link the directive engine into every
//! executable, which is why `build_main_wrapper` emits them only when the
//! module actually calls a printing shim.

use std::cell::RefCell;
use std::collections::HashMap;

use typelisp_abi::{decode, encode, fatal};
use typelisp_mem::{Heap, Value};

use crate::runtime::{set_print_hooks, PrintHooks, BARE_HOOKS};
use crate::stored_type_key;

thread_local! {
    /// `(type key, variant index) -> variant name`, filled by
    /// [`rt_print_enum_variant`].
    static ENUM_NAMES: RefCell<HashMap<(String, usize), String>> = RefCell::new(HashMap::new());
    /// `type key -> the address of that type's compiled `print-object``,
    /// filled by [`rt_print_object_method`].
    static PRINT_OBJECT: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
    /// The values whose own `print-object` is running right now, innermost
    /// last — the re-entry guard, so `(impl print-object point (… (format
    /// false "~a" self)))` degrades to the built-in `#<point 1 2>` rather
    /// than recursing forever. Keyed on the value, not a depth, so a
    /// genuinely nested structure still prints in full. The interpreter's
    /// `Interp::printing` is the same guard on the other side.
    static PRINTING: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}

/// The hooks an AOT executable prints under: the two tables above, and
/// [`BARE_HOOKS`]' control variables (see the module doc comment).
const AOT_HOOKS: PrintHooks = PrintHooks {
    enum_variant_name: |key, variant| ENUM_NAMES.with(|t| t.borrow().get(&(key.to_string(), variant)).cloned()),
    print_object: aot_print_object,
    opts: BARE_HOOKS.opts,
    print_vars: BARE_HOOKS.print_vars,
};

fn aot_print_object(heap: &mut Heap, v: Value, escape: bool) -> Result<Option<String>, String> {
    let Value::Boxed(id) = v else { return Ok(None) };
    let Some(key) = stored_type_key(heap, id).map(str::to_string) else {
        return Ok(None);
    };
    if PRINTING.with(|p| p.borrow().contains(&v)) {
        return Ok(None);
    }
    let Some(addr) = PRINT_OBJECT.with(|t| t.borrow().get(&key).copied()) else {
        return Ok(None);
    };
    // SAFETY: `addr` came from `rt_print_object_method`, whose only caller is
    // the startup sequence `build_main_wrapper` generates, and the address it
    // passes is a `ptrtoint` of a function the same file compiled under the
    // shared compiled-function ABI. `extern "C-unwind"` because a `(panic
    // ...)` inside the method unwinds out through here.
    let f: unsafe extern "C-unwind" fn(*const i64, u32) -> i64 = unsafe { std::mem::transmute(addr) };
    let args = [encode(v), i64::from(escape)];
    PRINTING.with(|p| p.borrow_mut().push(v));
    let result = unsafe { f(args.as_ptr(), 2) };
    PRINTING.with(|p| {
        p.borrow_mut().pop();
    });
    match decode(result) {
        Value::Str(id) => Ok(Some(heap.string(id).to_string())),
        other => Err(format!("print-object on `{}` returned {:?}, not a string", key, other)),
    }
}

/// Reads a `(pointer, length)` pair of `args` as a `&'static str`.
///
/// # Safety
///
/// The two words must be a pointer to `len` valid UTF-8 bytes that outlive
/// the process — which every caller satisfies, since the only ones are
/// startup calls whose arguments are LLVM global string constants.
unsafe fn static_str(args: *const i64, i: usize, who: &str) -> &'static str {
    let ptr = *args.add(i) as usize as *const u8;
    let len = *args.add(i + 1) as usize;
    match std::str::from_utf8(std::slice::from_raw_parts(ptr, len)) {
        Ok(s) => s,
        Err(_) => fatal(&format!("{}: argument {} is not valid UTF-8", who, i)),
    }
}

/// Registers one enum variant's name: `args` is `[key_ptr, key_len, variant,
/// name_ptr, name_len]`.
///
/// Installs [`AOT_HOOKS`] on first use, so the generated startup sequence is
/// just the registration calls with nothing to remember to do first.
///
/// # Safety
///
/// `args` must point to 5 valid `i64`s in those representations.
#[no_mangle]
pub unsafe extern "C" fn rt_print_enum_variant(args: *const i64, argc: u32) -> i64 {
    if argc < 5 {
        fatal("rt_print_enum_variant: expected 5 arguments");
    }
    let key = static_str(args, 0, "rt_print_enum_variant");
    let variant = *args.add(2) as usize;
    let name = static_str(args, 3, "rt_print_enum_variant");
    install();
    ENUM_NAMES.with(|t| t.borrow_mut().insert((key.to_string(), variant), name.to_string()));
    0
}

/// Registers one type's compiled `print-object`: `args` is `[key_ptr,
/// key_len, fn_addr]`.
///
/// # Safety
///
/// `args` must point to 3 valid `i64`s, the third being the address of a
/// function compiled under the shared compiled-function ABI.
#[no_mangle]
pub unsafe extern "C" fn rt_print_object_method(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_print_object_method: expected 3 arguments");
    }
    let key = static_str(args, 0, "rt_print_object_method");
    let addr = *args.add(2) as usize;
    install();
    PRINT_OBJECT.with(|t| t.borrow_mut().insert(key.to_string(), addr));
    0
}

/// Installs [`AOT_HOOKS`] once. Idempotent, so each registration shim can
/// call it without the startup sequence needing an ordering rule.
fn install() {
    set_print_hooks(Some(AOT_HOOKS));
}
