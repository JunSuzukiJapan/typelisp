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
//! The printer control variables (`*print-pretty*`, `*print-base*`, …) are
//! prelude globals the executable compiles in like any other, so the startup
//! registers each one's compiled global id ([`rt_print_global`]) and the
//! hooks read the live value from its slot on every printing operation.
//!
//! # Why the registration shims live here rather than in `typelisp-rt`
//!
//! Same reason [`crate::shim`] does, and it matters more here: these calls
//! are what make a program's `main` reference the printer at all. Emitting
//! them unconditionally would link the directive engine into every
//! executable, which is why `build_main_wrapper` emits them only when the
//! module actually calls a printing shim.

use std::cell::RefCell;

use typelisp_abi::{decode, encode, fatal};
use typelisp_mem::{Heap, Value};

use crate::runtime::{set_print_hooks, PrintHooks};
use crate::stored_type_key;

// `(type key, variant index) -> variant name`, filled by
// [`rt_print_enum_variant`] — now `shared::PrintShared::enum_names`
// (`docs/dev/os-threads-design.md` §5/Phase 1d), reached through
// `shared::print_shared()`.

// `FIELD_TEMPLATES`/`PRINT_OBJECT`/`FORMAT_CALL` are
// `shared::PrintShared`'s too, for the same reason: a worker thread
// dispatches through the tables main's startup sequence filled.

thread_local! {
    /// The values whose own `print-object` is running right now, innermost
    /// last — the re-entry guard, so `(impl print-object point (… (format
    /// false "~a" self)))` degrades to the built-in `#<point 1 2>` rather
    /// than recursing forever. Keyed on the value, not a depth, so a
    /// genuinely nested structure still prints in full. The interpreter's
    /// `Interp::printing` is the same guard on the other side.
    static PRINTING: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}

/// The hooks an AOT executable prints under: the tables its startup filled
/// (see the module doc comment).
const AOT_HOOKS: PrintHooks = PrintHooks {
    enum_variant_name: |key, variant| variant_name_in(&crate::shared::print_shared().enum_names.read(), key, variant),
    field_is_niched_option: |key, variant, index| {
        field_is_niched_in(&crate::shared::print_shared().field_templates.read(), key, variant, index)
    },
    field_name: |key, index| {
        let shared = crate::shared::print_shared();
        let names = shared.field_names.read();
        let templates = shared.field_templates.read();
        field_name_in(&names, &templates, key, index)
    },
    print_object: aot_print_object,
    format_call: aot_format_call,
    opts: |heap| crate::runtime::read_opts(heap, &|name| aot_global(heap, name)),
    print_vars: |heap| crate::runtime::read_print_vars(heap, &|name| aot_global(heap, name)),
};

/// `(base type key, variant) -> name`: what [`rt_print_enum_variant`] fills.
pub type VariantNames = std::collections::HashMap<(String, usize), String>;
/// `(base type key, variant or NO_VARIANT, index or EVERY_FIELD) -> template`:
/// what [`rt_print_field_template`] fills.
pub type FieldTemplates = std::collections::HashMap<(String, i64, i64), String>;
/// `(base type key, index) -> field name`: what [`rt_print_field_name`] fills.
pub type FieldNames = std::collections::HashMap<(String, i64), String>;

/// The template registered for field `index` of `key`'s type, or the one
/// registered for every field of it (`Vector<T>`'s elements). Looked up under
/// the key's *base*: the tables hold one entry per type, while the key a
/// value carries names its instantiation (`option<char>`).
fn field_template_in<'t>(templates: &'t FieldTemplates, key: &str, variant: Option<usize>, index: usize) -> Option<&'t String> {
    let base = typelisp_mem::base_type_key(key).to_string();
    let variant = variant.map_or(NO_VARIANT, |v| v as i64);
    templates.get(&(base.clone(), variant, index as i64)).or_else(|| templates.get(&(base, variant, EVERY_FIELD)))
}

/// [`crate::PrintEnv::enum_variant_name`] answered from a registration table
/// — an AOT executable's, or a worker thread's copy of the interpreter's.
pub fn variant_name_in(names: &VariantNames, key: &str, variant: usize) -> Result<String, String> {
    let base = typelisp_mem::base_type_key(key);
    names
        .get(&(base.to_string(), variant))
        .cloned()
        .ok_or_else(|| format!("internal error: no name is registered for variant {} of `{}`", variant, key))
}

/// [`crate::PrintEnv::field_is_niched_option`] from a registration table: the
/// field's template, instantiated by the arguments the value's own key
/// carries. Every field of every registered type has a template, so a
/// missing one is a broken registration.
pub fn field_is_niched_in(templates: &FieldTemplates, key: &str, variant: Option<usize>, index: usize) -> Result<bool, String> {
    let template = field_template_in(templates, key, variant, index)
        .ok_or_else(|| format!("internal error: no type is registered for field {} of `{}`", index, key))?;
    let args = typelisp_mem::type_key_args(key);
    Ok(typelisp_mem::option_prints_wrapped(&typelisp_mem::instantiate_key_template(template, &args)))
}

/// [`crate::PrintEnv::field_name`] from a registration table. The name table
/// holds only named fields, so the template table — which holds every field
/// of every struct — is what tells a positional field (`Ok(None)`) from a type
/// nobody registered (`Err`).
pub fn field_name_in(names: &FieldNames, templates: &FieldTemplates, key: &str, index: usize) -> Result<Option<String>, String> {
    let base = typelisp_mem::base_type_key(key).to_string();
    if let Some(name) = names.get(&(base, index as i64)) {
        return Ok(Some(name.clone()));
    }
    match field_template_in(templates, key, None, index) {
        Some(_) => Ok(None),
        None => Err(format!("internal error: no struct `{}` with a field {} is registered", key, index)),
    }
}

/// A printer control variable's current value, from the compiled global slot
/// [`rt_print_global`] registered for it. The prelude defines every one of
/// them and the startup registers every one, so a missing entry is a broken
/// startup sequence.
fn aot_global(heap: &Heap, name: &str) -> Result<Value, String> {
    let shared = crate::shared::print_shared();
    let id = *shared
        .globals
        .read()
        .get(name)
        .ok_or_else(|| format!("internal error: printer variable {} was not registered at startup", name))?;
    let slot_of = shared
        .global_slot
        .read()
        .ok_or_else(|| "internal error: the runtime did not install its global table before printing".to_string())?;
    let slot = slot_of(id).ok_or_else(|| format!("internal error: {} names compiled global {}, which has no slot", name, id))?;
    Ok(heap.permanent_root(slot))
}

/// Installs how a compiled global id becomes its permanent-root position —
/// what the runtime calls before the program's first initialiser runs.
pub fn set_global_slots(slot_of: fn(usize) -> Option<usize>) {
    *crate::shared::print_shared().global_slot.write() = Some(slot_of);
}

fn aot_print_object(heap: &mut Heap, v: Value, escape: bool) -> Result<Option<String>, String> {
    let Value::Boxed(id) = v else { return Ok(None) };
    if PRINTING.with(|p| p.borrow().contains(&v)) {
        return Ok(None);
    }
    // Looked up by the value's type *name*, not its interned `TypeKeyId`:
    // `rt_print_object_method` fills this table from the executable's static
    // strings before `rt_heap_init` runs (see `build_main_wrapper`), so there
    // is no heap to intern against at registration time. The borrow of the
    // name is kept inside this block so `heap` is free again for the method
    // call below.
    let Some(addr) = ({
        let Some(key) = stored_type_key(heap, id) else { return Ok(None) };
        crate::shared::print_shared().print_object.read().get(&key).copied()
    }) else {
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
        other => {
            // Only the error path needs the name back, so it is read here
            // rather than kept borrowed across the compiled call above.
            let key = stored_type_key(heap, id).unwrap_or_else(|| "<untyped box>".to_string());
            Err(format!("print-object on `{}` returned {:?}, not a string", key, other))
        }
    }
}

/// `~/name/` in an AOT executable, dispatched through `PrintShared::format_call`.
///
/// The table is filled at startup with the methods the checker found by
/// scanning this program's literal control strings, so a name that reaches
/// here either has an entry or was never reachable at all — the error says
/// which, rather than the blanket "not available in a compiled executable"
/// this hook used to be.
///
/// The type key is read off the value the same way the interpreter reads it
/// (`Interp::format_call`): a numeric box names the primitive it is, any
/// other box carries its own key, and each immediate maps to exactly the
/// type it *is* — `Value::Int` included, which is an `i32` and nothing else
/// now that the narrow widths are boxed.
fn aot_format_call(heap: &mut Heap, name: &str, v: Value, colon: bool, at: bool) -> Result<String, String> {
    let keys: Vec<String> = match v {
        Value::Boxed(id) => {
            match heap.primitive_box_type_name(id).map(str::to_string).or_else(|| stored_type_key(heap, id)) {
                Some(k) => vec![k],
                None => return Err(format!("format: ~/{}/ — this value carries no type name to dispatch on", name)),
            }
        }
        Value::Str(_) => vec!["string".to_string()],
        Value::Bool(_) => vec!["bool".to_string()],
        Value::Char(_) => vec!["char".to_string()],
        Value::Symbol(_) => vec!["symbol".to_string()],
        Value::Empty | Value::Cons(_) | Value::Path(_) => vec!["sexpr".to_string()],
        Value::Int(_) => vec!["i32".to_string()],
    };
    let found: Vec<usize> = {
        let shared = crate::shared::print_shared();
        let t = shared.format_call.read();
        keys.iter().filter_map(|k| t.get(&(k.clone(), name.to_string())).copied()).collect()
    };
    let addr = match found.len() {
        1 => found[0],
        0 => {
            return Err(format!(
                "format: ~/{}/ — {} has no method `{}` registered in this executable",
                name,
                keys.iter().map(|k| format!("`{}`", k)).collect::<Vec<_>>().join(" or "),
                name
            ))
        }
        _ => {
            return Err(format!(
                "format: ~/{}/ — an integer argument could be either width and both \
                 `i64` and `i32` define `{}`; there is nothing in the value to choose by",
                name, name
            ))
        }
    };
    // SAFETY: the same contract `aot_print_object` relies on — `addr` came
    // from `rt_format_call_method`, whose only caller is the startup sequence
    // `build_main_wrapper` generates, and the address is a `ptrtoint` of a
    // function this file compiled under the shared compiled-function ABI. The
    // checker verified the signature (`(Self, bool, bool) -> string`) before
    // naming it. `extern "C-unwind"` because a `(panic ...)` inside the
    // method unwinds out through here.
    let f: unsafe extern "C-unwind" fn(*const i64, u32) -> i64 = unsafe { std::mem::transmute(addr) };
    let args = [encode(v), i64::from(colon), i64::from(at)];
    match decode(unsafe { f(args.as_ptr(), 3) }) {
        Value::Str(id) => Ok(heap.string(id).to_string()),
        other => Err(format!("format: ~/{}/ returned {:?}, not a string", name, other)),
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
    crate::shared::print_shared().enum_names.write().insert((key.to_string(), variant), name.to_string());
    0
}

/// The `variant` word of [`rt_print_field_template`] for a struct, which has
/// no variants.
pub const NO_VARIANT: i64 = -1;
/// The `index` word of [`rt_print_field_template`] for a template that
/// applies to every field of the type — a `Vector<T>`'s elements.
pub const EVERY_FIELD: i64 = -1;

/// Registers one field's type key template: `args` is `[key_ptr, key_len,
/// variant, index, template_ptr, template_len]`, with `variant`
/// [`NO_VARIANT`] for a struct and `index` [`EVERY_FIELD`] for a uniform
/// template. See `Interp::field_template_descriptors`.
///
/// # Safety
///
/// `args` must point to 6 valid `i64`s in those representations.
#[no_mangle]
pub unsafe extern "C" fn rt_print_field_template(args: *const i64, argc: u32) -> i64 {
    if argc < 6 {
        fatal("rt_print_field_template: expected 6 arguments");
    }
    let key = static_str(args, 0, "rt_print_field_template");
    let variant = *args.add(2);
    let index = *args.add(3);
    let template = static_str(args, 4, "rt_print_field_template");
    install();
    crate::shared::print_shared().field_templates.write().insert((key.to_string(), variant, index), template.to_string());
    0
}

/// Registers one struct field's name: `args` is `[key_ptr, key_len, index,
/// name_ptr, name_len]`. See `Interp::field_name_descriptors`.
///
/// # Safety
///
/// `args` must point to 5 valid `i64`s in those representations.
#[no_mangle]
pub unsafe extern "C" fn rt_print_field_name(args: *const i64, argc: u32) -> i64 {
    if argc < 5 {
        fatal("rt_print_field_name: expected 5 arguments");
    }
    let key = static_str(args, 0, "rt_print_field_name");
    let index = *args.add(2);
    let name = static_str(args, 3, "rt_print_field_name");
    install();
    crate::shared::print_shared().field_names.write().insert((key.to_string(), index), name.to_string());
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
    crate::shared::print_shared().print_object.write().insert(key.to_string(), addr);
    0
}

/// Registers one method reachable through `~/name/`: `args` is `[key_ptr,
/// key_len, name_ptr, name_len, fn_addr]`.
///
/// One call per `(type, method)` the checker found while scanning this
/// program's literal control strings — see [`aot_format_call`].
///
/// # Safety
///
/// `args` must point to 5 valid `i64`s, the last being the address of a
/// function compiled under the shared compiled-function ABI whose signature
/// is `((self Self) (colon bool) (at bool)) -> string`.
#[no_mangle]
pub unsafe extern "C" fn rt_format_call_method(args: *const i64, argc: u32) -> i64 {
    if argc < 5 {
        fatal("rt_format_call_method: expected 5 arguments");
    }
    let key = static_str(args, 0, "rt_format_call_method");
    let name = static_str(args, 2, "rt_format_call_method");
    let addr = *args.add(4) as usize;
    install();
    crate::shared::print_shared().format_call.write().insert((key.to_string(), name.to_string()), addr);
    0
}

/// Registers one printer control variable's compiled global: `args` is
/// `[name_ptr, name_len, id]`.
///
/// # Safety
///
/// `args` must point to 3 valid `i64`s in those representations.
#[no_mangle]
pub unsafe extern "C" fn rt_print_global(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_print_global: expected 3 arguments");
    }
    let name = static_str(args, 0, "rt_print_global");
    let id = *args.add(2) as usize;
    install();
    crate::shared::print_shared().globals.write().insert(name.to_string(), id);
    0
}

/// Installs [`AOT_HOOKS`] once. Idempotent, so each registration shim can
/// call it without the startup sequence needing an ordering rule.
///
/// Also what an interpreter's worker thread prints with: it has no
/// interpreter to ask, and the tables these hooks read are the ones the
/// interpreter fills for it ([`crate::shared::PrintShared`]).
pub fn install() {
    set_print_hooks(Some(AOT_HOOKS));
}
