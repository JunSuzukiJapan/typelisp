//! The C FFI: what `(defffi ...)` compiles to.
//!
//! # The problem this solves
//!
//! Everything compiled in this workspace shares one signature —
//! `i64 f(const i64 *args, u32 argc)`, [`compiled_fn_type`] — and every
//! `rt_*` shim is written to match it. A C function is not written to match
//! it. `strlen` takes a pointer and answers with a `size_t`; `sqrt` takes and
//! answers with a `double` (in a floating-point register, which the shared
//! signature has no way to name at all).
//!
//! So each declaration gets a **thunk**: a function *in* the shared signature
//! whose body unpacks `args`, converts each word to the C type declared for
//! it, calls the real function under its real signature, and converts the
//! answer back. One function, emitted here, in LLVM IR.
//!
//! # Why that makes the rest of the compiler unaware of the FFI
//!
//! The thunk is named by [`crate::compile::symbols::user_symbol_name`] — the
//! same rule a `defun`'s compiled body is named by. Two things follow, and
//! they are the whole reason for this design:
//!
//! - **The interpreter needs no wiring.** A thunk is a `CompiledBody` hung on
//!   the declaration's `FnDef`, and `Interp::enter` already prefers a compiled
//!   body over walking forms.
//! - **The self-hosted compiler needs no changes.** A compiled call site asks
//!   `symbols::callee_symbol_name` for the callee's symbol and emits a call to
//!   it; for an FFI declaration that is the thunk's own name, in the shared
//!   signature the island always emits. The island never learns that C is
//!   involved, so `src/compiler.rs` — and the artifact built from it — is
//!   untouched by any of this.
//!
//! # What is deliberately not here
//!
//! Variadic C functions (`printf`) and struct-by-value arguments. Both need
//! the platform's argument-classification rules — where AArch64 Darwin puts a
//! variadic argument is not where it puts a fixed one — and a thunk built
//! from a fixed signature does not follow them. The type vocabulary below is
//! what closes that off: neither can be spelled, so neither can be declared.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;

use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue};

use typelisp_front::eval::interp::{CompiledBody, FfiDecl, Interp};

use crate::compile::llvm_builtins::compiled_fn_type;
use crate::compile::{llvm_context, CompiledFn, COMPILE_LOCK};

/// A type as the C side of the boundary sees it.
///
/// A second vocabulary beside `Repr`, and the reason is [`Repr::Narrow`]: it
/// folds all six integer widths into one, because how a value crosses the
/// compiled boundary is the same for all of them (a sign-extended machine
/// word). The thunk needs the other answer — `i8` really is one byte to the C
/// function, and getting there is a `trunc`.
///
/// **This enum is the one list of what the FFI can spell.** The checker
/// deliberately does not keep a second one: it parses the declared types like
/// any other annotation and lets [`Self::from_key`] be the judge, because the
/// judge should be whatever actually has to emit the conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CType {
    /// A C integer of this width. `signed` picks `sext` over `zext` when the
    /// value comes back, and is read from the type's own name rather than
    /// decided here — see `types::int_width_signed`.
    Int { bits: u32, signed: bool },
    F32,
    F64,
    Bool,
    Void,
    /// `const char *`. Not a word the boundary can hand over as-is: a typelisp
    /// `string` is a Rust `String` in the heap's string table, neither
    /// NUL-terminated nor guaranteed free of NULs. It is copied into a C
    /// string for one call and copied back out of one — see
    /// `typelisp_rt::rt_ffi_cstring_new` and its two siblings.
    Str,
    /// An opaque pointer. Crosses as a raw 64-bit word in both directions —
    /// see [`typelisp_front::check::repr::Repr::RawWord`] for why it may not
    /// be stored anywhere that tags what it holds.
    Ptr,
}

impl CType {
    /// The C type a declared typelisp type means, or `None` if it means none.
    ///
    /// The spellings are `type_key::type_key_of_type`'s, which is what the
    /// `defffi` node carries — one producer, so there is nothing to keep in
    /// agreement.
    fn from_key(key: &str) -> Option<CType> {
        // A callback parameter: what crosses is the address of the entry C
        // calls (`typelisp_front::ffi_callback`), so to the thunk it is a
        // pointer. Whether the callback's own types can be spelled is
        // [`callback_ctypes`]'s question.
        if typelisp_front::ffi_callback::split_fn_key(key).is_some() {
            return Some(CType::Ptr);
        }
        // Covers the six widths *and* `c-long`/`c-ulong`, which that function
        // answers 64 for — the one place the LP64 assumption is written down.
        if let Some((bits, signed)) = typelisp_front::types::int_width_signed(key) {
            return Some(CType::Int { bits, signed });
        }
        match key {
            "f32" => Some(CType::F32),
            "f64" => Some(CType::F64),
            "bool" => Some(CType::Bool),
            "()" | "unit" => Some(CType::Void),
            "string" => Some(CType::Str),
            "ptr" => Some(CType::Ptr),
            _ => None,
        }
    }

    /// This width as an LLVM integer type. Only the four the declarable
    /// widths use, so there is no custom-width case to get wrong.
    fn int_type(bits: u32) -> inkwell::types::IntType<'static> {
        let ctx = llvm_context();
        match bits {
            8 => ctx.i8_type(),
            16 => ctx.i16_type(),
            32 => ctx.i32_type(),
            _ => ctx.i64_type(),
        }
    }

    /// What a value of this type is, as an LLVM type — `None` for `Void`,
    /// which is not a value.
    fn llvm(self) -> Option<BasicTypeEnum<'static>> {
        let ctx = llvm_context();
        Some(match self {
            CType::Int { bits, .. } => CType::int_type(bits).into(),
            CType::F32 => ctx.f32_type().into(),
            CType::F64 => ctx.f64_type().into(),
            CType::Bool => ctx.bool_type().into(),
            CType::Str | CType::Ptr => ctx.ptr_type(inkwell::AddressSpace::default()).into(),
            CType::Void => return None,
        })
    }

    /// The attribute the platform ABI requires at a call site for this type,
    /// if any.
    ///
    /// Not decoration. AArch64 on Darwin requires the *caller* to extend an
    /// argument narrower than a register, and to expect a narrow return
    /// already extended; without these, a `char`-taking function reads the
    /// bits that happened to be above it. The development platform for this
    /// workspace is arm64 macOS, so an omission here is not a portability
    /// footnote but the first thing that breaks.
    fn extension_attribute(self) -> Option<&'static str> {
        match self {
            CType::Int { bits, signed } if bits < 32 => Some(if signed { "signext" } else { "zeroext" }),
            // `_Bool` is one byte in memory and passed extended.
            CType::Bool => Some("zeroext"),
            _ => None,
        }
    }
}

/// Emit `decl`'s thunk into `module` and answer with it.
///
/// The C function is declared here too, by its own name and its own
/// signature; who resolves that name to an address is the caller's business
/// (the JIT maps it eagerly, an AOT build leaves it to the linker).
pub(crate) fn emit_thunk(module: &Module<'static>, decl: &FfiDecl) -> Result<FunctionValue<'static>, String> {
    let ctx = llvm_context();
    let builder = ctx.create_builder();

    let params = ctypes(&decl.params, decl)?;
    let ret = CType::from_key(&decl.ret)
        .ok_or_else(|| unspellable(&decl.ret, "a return type", decl))?;

    // ---- the C function, under its real signature ----
    let arg_tys: Vec<BasicMetadataTypeEnum<'static>> =
        params.iter().map(|c| c.llvm().expect("a parameter is never void").into()).collect();
    let c_fn_ty = match ret.llvm() {
        Some(t) => t.fn_type(&arg_tys, false),
        None => ctx.void_type().fn_type(&arg_tys, false),
    };
    // Reusing an existing declaration is right when two `defffi`s name the
    // same C function, and wrong when the name is already taken by something
    // declared under a different signature. An AOT module has every `rt_*`
    // shim declared under the shared ABI before any thunk is emitted, so
    // `(defffi (my-car "rt_car") (sexpr) sexpr)` would otherwise call one
    // through a signature it does not have.
    let c_fn = match module.get_function(&decl.c_symbol) {
        Some(f) if f.get_type() == c_fn_ty => f,
        Some(_) => {
            return Err(format!(
                "defffi: `{}` is already declared in this module under a different signature, so \
                 the declared one cannot be the one that gets called. (A runtime shim's name is \
                 the usual way to reach this.)",
                decl.c_symbol
            ))
        }
        None => module.add_function(&decl.c_symbol, c_fn_ty, None),
    };
    for (i, c) in params.iter().enumerate() {
        if let Some(attr) = c.extension_attribute() {
            add_attribute(c_fn, inkwell::attributes::AttributeLoc::Param(i as u32), attr);
        }
    }
    if let Some(attr) = ret.extension_attribute() {
        add_attribute(c_fn, inkwell::attributes::AttributeLoc::Return, attr);
    }

    // ---- the thunk, under the shared signature ----
    let name = crate::compile::symbols::user_symbol_name(&decl.path.to_string());
    if module.get_function(&name).is_some() {
        return Err(format!("internal error: `{}` is already defined in this module", name));
    }
    // The marshalling body keeps the shared `(args, argc)` signature -- it
    // reads its arguments out of that array and returns a value, which is all
    // a call into C ever needs. What changed in C2d is who calls *it*: a
    // compiled Lisp caller now names its callee to the driver, so the symbol
    // the *Lisp* name resolves to has to answer the coroutine ABI. So the
    // body moves to a name of its own and `name` becomes a coroutine entry
    // that copies the pending arguments into an array and calls it
    // (`emit_coroutine_entry` below). One boundary, in one place, instead of
    // teaching every call site which of its callees is a `defffi`.
    let body_name = format!("{}$ffi", name);
    let thunk = module.add_function(&body_name, compiled_fn_type(), None);
    let entry = ctx.append_basic_block(thunk, "entry");
    builder.position_at_end(entry);

    let i64_ty = ctx.i64_type();
    let args_ptr = thunk.get_nth_param(0).expect("the shared signature has two parameters").into_pointer_value();

    // One scratch slot, reused: every shim call below takes exactly one word
    // and consumes it before the next is stored. In the entry block because
    // that is where LLVM wants an `alloca`.
    let scratch = builder
        .build_alloca(i64_ty, "ffi_scratch")
        .map_err(|e| format!("ffi: failed to reserve the shim argument slot: {}", e))?;
    let one = ctx.i32_type().const_int(1, false);
    let shim = |name: &str| match module.get_function(name) {
        Some(f) => f,
        None => module.add_function(name, compiled_fn_type(), None),
    };

    let mut call_args: Vec<BasicMetadataValueEnum<'static>> = Vec::with_capacity(params.len());
    // The C strings made for this call, to be freed once it has returned.
    let mut owned_cstrings: Vec<IntValue<'static>> = Vec::new();
    for (i, c) in params.iter().enumerate() {
        // `args[i]`, the word the caller put there. Its shape is the
        // *declared* one — `encode_crossing_args` was driven by the same
        // signature this thunk was built from.
        let slot = unsafe {
            builder
                .build_gep(i64_ty, args_ptr, &[i64_ty.const_int(i as u64, false)], "arg_slot")
                .map_err(|e| format!("ffi: failed to index the argument array: {}", e))?
        };
        let word = builder
            .build_load(i64_ty, slot, "arg_word")
            .map_err(|e| format!("ffi: failed to load an argument: {}", e))?
            .into_int_value();
        if *c == CType::Str {
            let made = call_shim(&builder, shim("rt_ffi_cstring_new"), scratch, one, word, "cstr")?;
            owned_cstrings.push(made);
            let p = builder
                .build_int_to_ptr(made, ctx.ptr_type(inkwell::AddressSpace::default()), "cstr_ptr")
                .map_err(|e| format!("ffi: failed to make a pointer from a C string: {}", e))?;
            call_args.push(p.into());
        } else {
            call_args.push(word_to_c(&builder, word, *c)?.into());
        }
    }

    // Native for exactly the C call: everything it is given is a C value
    // by now, and nothing it answers is a Lisp one yet — see
    // `typelisp_rt::rt_ffi_enter_native`.
    let zero = i64_ty.const_zero();
    call_shim(&builder, shim("rt_ffi_enter_native"), scratch, one, zero, "native")?;
    let call = builder
        .build_call(c_fn, &call_args, "ffi_call")
        .map_err(|e| format!("ffi: failed to build the call to `{}`: {}", decl.c_symbol, e))?;
    call_shim(&builder, shim("rt_ffi_leave_native"), scratch, one, zero, "running")?;
    let out = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) if ret == CType::Str => {
            // Copied *before* the arguments are freed: a C function that
            // answers with a pointer into one of them (`strchr`, `strstr`) is
            // ordinary, and freeing first would leave this reading freed
            // memory. The cost is that a `raise` in here — a null result, or
            // bytes that are not UTF-8 — leaks the argument strings, on a
            // path that is ending the call anyway.
            let raw = builder
                .build_ptr_to_int(v.into_pointer_value(), i64_ty, "ret_cstr")
                .map_err(|e| format!("ffi: failed to take the address of a string result: {}", e))?;
            call_shim(&builder, shim("rt_ffi_string_from_cstr"), scratch, one, raw, "ret_str")?
        }
        inkwell::values::ValueKind::Basic(v) => c_to_word(&builder, v, ret)?,
        // A `void` C function still has to answer with a word, because the
        // shared signature says so. `Repr::Unit` crosses as 0, so this is the
        // same placeholder every other unit-valued call produces.
        inkwell::values::ValueKind::Instruction(_) => i64_ty.const_zero(),
    };
    for made in owned_cstrings {
        call_shim(&builder, shim("rt_ffi_cstring_free"), scratch, one, made, "freed")?;
    }
    builder.build_return(Some(&out)).map_err(|e| format!("ffi: failed to build the return: {}", e))?;

    emit_coroutine_entry(module, &name, thunk, params.len())
}

/// A coroutine-ABI entry point for a classic body: `i64 f(i64 frame)` that
/// makes a frame, copies the pending arguments into an array, calls `body`
/// under the shared `(args, argc)` signature, leaves its answer in the
/// frame's value slot and returns `STATUS_RETURN`.
///
/// There is no `pc` dispatch and no resume block, because there is nothing to
/// resume: the body runs to completion by construction. `rt_frame_entered` is
/// still required — the driver takes the frame it publishes on the way back,
/// and a caller that never published one is a protocol break, not a shortcut.
fn emit_coroutine_entry(
    module: &Module<'static>,
    name: &str,
    body: FunctionValue<'static>,
    argc: usize,
) -> Result<FunctionValue<'static>, String> {
    let ctx = llvm_context();
    let i64_ty = ctx.i64_type();
    let entry_fn = module.add_function(name, crate::compile::llvm_builtins::coroutine_fn_type(), None);
    let builder = ctx.create_builder();
    builder.position_at_end(ctx.append_basic_block(entry_fn, "entry"));

    let shim = |n: &str| match module.get_function(n) {
        Some(f) => f,
        None => module.add_function(n, compiled_fn_type(), None),
    };
    let scratch = builder
        .build_alloca(i64_ty, "ffi_entry_scratch")
        .map_err(|e| format!("ffi: failed to reserve the entry scratch slot: {}", e))?;
    let one = ctx.i32_type().const_int(1, false);

    // The driver protocol's own slots, and nothing else: this entry has no
    // locals, but a frame the driver cannot read the handler slot of is a
    // frame it cannot ask about an unwind. It asked with one slot here until
    // C4 reserved the second, and the two FFI tests that *raise* are what
    // said so.
    let frame = call_shim(
        &builder,
        shim("rt_frame_new"),
        scratch,
        one,
        i64_ty.const_int(typelisp_abi::FRAME_RESERVED_SLOTS as u64, false),
        "ffi_frame",
    )?;
    call_shim(&builder, shim("rt_frame_entered"), scratch, one, frame, "ffi_entered")?;

    let args = builder
        .build_alloca(i64_ty.array_type(argc.max(1) as u32), "ffi_entry_args")
        .map_err(|e| format!("ffi: failed to reserve the entry argument array: {}", e))?;
    for i in 0..argc {
        let word = call_shim(
            &builder,
            shim("rt_pending_arg"),
            scratch,
            one,
            i64_ty.const_int(i as u64, false),
            "ffi_pending",
        )?;
        let slot = unsafe {
            builder
                .build_gep(i64_ty, args, &[i64_ty.const_int(i as u64, false)], "ffi_entry_arg_ptr")
                .map_err(|e| format!("ffi: failed to index the entry argument array: {}", e))?
        };
        builder.build_store(slot, word).map_err(|e| format!("ffi: failed to store an entry argument: {}", e))?;
    }

    let out = builder
        .build_call(body, &[args.into(), ctx.i32_type().const_int(argc as u64, false).into()], "ffi_body")
        .map_err(|e| format!("ffi: failed to call the marshalling body: {}", e))?;
    let out = match out.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
        inkwell::values::ValueKind::Instruction(_) => {
            return Err("internal error: the marshalling body produced no value".to_string())
        }
    };

    let data = call_shim(&builder, shim("rt_frame_data"), scratch, one, frame, "ffi_frame_data")?;
    let data = builder
        .build_int_to_ptr(data, ctx.ptr_type(inkwell::AddressSpace::default()), "ffi_frame_ptr")
        .map_err(|e| format!("ffi: failed to take the frame's data pointer: {}", e))?;
    let value_slot = unsafe {
        builder
            .build_gep(i64_ty, data, &[i64_ty.const_zero()], "ffi_value_slot")
            .map_err(|e| format!("ffi: failed to index the frame's value slot: {}", e))?
    };
    builder.build_store(value_slot, out).map_err(|e| format!("ffi: failed to store the result: {}", e))?;
    builder
        .build_return(Some(&i64_ty.const_zero()))
        .map_err(|e| format!("ffi: failed to build the entry return: {}", e))?;
    Ok(entry_fn)
}

/// Call a one-word `rt_*` shim: store `word` in `scratch` and call under the
/// shared signature, which is what every shim is written to.
fn call_shim(
    builder: &inkwell::builder::Builder<'static>,
    f: FunctionValue<'static>,
    scratch: inkwell::values::PointerValue<'static>,
    one: IntValue<'static>,
    word: IntValue<'static>,
    name: &str,
) -> Result<IntValue<'static>, String> {
    builder.build_store(scratch, word).map_err(|e| format!("ffi: failed to store a shim argument: {}", e))?;
    let call = builder
        .build_call(f, &[scratch.into(), one.into()], name)
        .map_err(|e| format!("ffi: failed to call a runtime shim: {}", e))?;
    match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(v.into_int_value()),
        inkwell::values::ValueKind::Instruction(_) => {
            Err("internal error: a runtime shim produced no value".to_string())
        }
    }
}

/// Every declared parameter as a [`CType`], or an error naming the first one
/// that is not spellable in C.
fn ctypes(keys: &[String], decl: &FfiDecl) -> Result<Vec<CType>, String> {
    // A callback parameter's own types are checked here, at the declaration,
    // rather than when an entry is first made for it.
    for k in keys {
        if let Some((params, ret)) = typelisp_front::ffi_callback::split_fn_key(k) {
            callback_ctypes(&params, &ret).map_err(|e| format!("defffi: `{}`: {}", decl.path, e))?;
        }
    }
    keys.iter()
        .map(|k| match CType::from_key(k) {
            Some(CType::Void) => Err(format!(
                "defffi: `{}` declares a `()` parameter, which is not a value C can be passed",
                decl.path
            )),
            Some(c) => Ok(c),
            None => Err(unspellable(k, "a parameter type", decl)),
        })
        .collect()
}

fn unspellable(key: &str, where_: &str, decl: &FfiDecl) -> String {
    format!(
        "defffi: `{}` declares `{}` as {}, which the FFI cannot spell in C. \
         It can spell the integer widths (i8/i16/i32/u8/u16/u32), c-long, c-ulong, f32, f64, \
         bool, string, ptr, and `()`.",
        decl.path, key, where_
    )
}

/// The i64 the shared signature carries, as the C type the declaration named.
fn word_to_c(
    builder: &inkwell::builder::Builder<'static>,
    word: IntValue<'static>,
    c: CType,
) -> Result<BasicValueEnum<'static>, String> {
    let ctx = llvm_context();
    Ok(match c {
        // A `ptr` is already the whole word; it only has to become a pointer.
        CType::Ptr => builder
            .build_int_to_ptr(word, ctx.ptr_type(inkwell::AddressSpace::default()), "arg_ptr")
            .map_err(|e| format!("ffi: failed to make a pointer argument: {}", e))?
            .into(),
        CType::Int { bits: 64, .. } => word.into(),
        CType::Int { bits, .. } => builder
            .build_int_truncate(word, CType::int_type(bits), "arg_narrow")
            .map_err(|e| format!("ffi: failed to narrow an integer argument: {}", e))?
            .into(),
        CType::Bool => builder
            .build_int_truncate(word, ctx.bool_type(), "arg_bool")
            .map_err(|e| format!("ffi: failed to narrow a bool argument: {}", e))?
            .into(),
        // A float crosses as the bits of its `f64` — that is what
        // `encode_crossing_value` puts in the word, for `f32` too.
        CType::F64 => builder
            .build_bit_cast(word, ctx.f64_type(), "arg_f64")
            .map_err(|e| format!("ffi: failed to reinterpret an f64 argument: {}", e))?
            .into(),
        CType::F32 => {
            let wide = builder
                .build_bit_cast(word, ctx.f64_type(), "arg_f32_wide")
                .map_err(|e| format!("ffi: failed to reinterpret an f32 argument: {}", e))?
                .into_float_value();
            builder
                .build_float_trunc(wide, ctx.f32_type(), "arg_f32")
                .map_err(|e| format!("ffi: failed to narrow an f32 argument: {}", e))?
                .into()
        }
        // Both of these are handled in `emit_thunk`, beside the runtime shims
        // they need and this converter cannot reach.
        CType::Str | CType::Void => {
            Err(format!("internal error: {:?} reached the scalar argument converter", c))?
        }
    })
}

/// What the C function answered with, as the i64 the shared signature returns.
fn c_to_word(
    builder: &inkwell::builder::Builder<'static>,
    v: BasicValueEnum<'static>,
    c: CType,
) -> Result<IntValue<'static>, String> {
    let ctx = llvm_context();
    let i64_ty = ctx.i64_type();
    Ok(match c {
        CType::Ptr => builder
            .build_ptr_to_int(v.into_pointer_value(), i64_ty, "ret_ptr")
            .map_err(|e| format!("ffi: failed to take the address of a pointer result: {}", e))?,
        CType::Int { bits, signed } => {
            let n = v.into_int_value();
            if bits == 64 {
                n
            } else if signed {
                builder
                    .build_int_s_extend(n, i64_ty, "ret_sext")
                    .map_err(|e| format!("ffi: failed to widen a signed result: {}", e))?
            } else {
                builder
                    .build_int_z_extend(n, i64_ty, "ret_zext")
                    .map_err(|e| format!("ffi: failed to widen an unsigned result: {}", e))?
            }
        }
        // `false`/`true` cross as 0/1, so the one bit is zero-extended.
        CType::Bool => builder
            .build_int_z_extend(v.into_int_value(), i64_ty, "ret_bool")
            .map_err(|e| format!("ffi: failed to widen a bool result: {}", e))?,
        CType::F64 => builder
            .build_bit_cast(v.into_float_value(), i64_ty, "ret_f64")
            .map_err(|e| format!("ffi: failed to reinterpret an f64 result: {}", e))?
            .into_int_value(),
        // Widened to `f64` before its bits are taken, because that is the
        // shape the boundary carries an `f32` in — see `Repr::F32`.
        CType::F32 => {
            let wide = builder
                .build_float_ext(v.into_float_value(), ctx.f64_type(), "ret_f32_wide")
                .map_err(|e| format!("ffi: failed to widen an f32 result: {}", e))?;
            builder
                .build_bit_cast(wide, i64_ty, "ret_f32")
                .map_err(|e| format!("ffi: failed to reinterpret an f32 result: {}", e))?
                .into_int_value()
        }
        CType::Str | CType::Void => {
            Err(format!("internal error: {:?} reached the scalar result converter", c))?
        }
    })
}

fn add_attribute(f: FunctionValue<'static>, loc: inkwell::attributes::AttributeLoc, name: &str) {
    let ctx = llvm_context();
    let kind = inkwell::attributes::Attribute::get_named_enum_kind_id(name);
    f.add_attribute(loc, ctx.create_enum_attribute(kind, 0));
}

/// Libraries opened so far, by the name the declaration wrote.
///
/// Here rather than in `typelisp_rt::os`, which is a safe wrapper over one C
/// call per function and has no business holding process-wide state. Never
/// emptied: see [`typelisp_rt::os::dl_open`] for why nothing is ever closed.
static OPEN_LIBS: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

/// The address `decl` names, or an error saying what was looked for and where.
fn resolve(decl: &FfiDecl) -> Result<usize, String> {
    let handle = match &decl.library {
        None => 0,
        Some(lib) => open_library(lib)?,
    };
    typelisp_rt::os::dl_sym(handle, &decl.c_symbol).ok_or_else(|| match &decl.library {
        Some(lib) => format!(
            "defffi: `{}` has no symbol `{}` — the library opened, so the name is what is wrong",
            lib, decl.c_symbol
        ),
        None => format!(
            "defffi: no symbol `{}` in this process. Nothing already linked defines it, so it \
             needs a `:library \"name\"` saying where to find it.",
            decl.c_symbol
        ),
    })
}

/// Open `name`, trying the platform's spellings of it in turn.
fn open_library(name: &str) -> Result<usize, String> {
    let mut guard = OPEN_LIBS.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(h) = cache.get(name) {
        return Ok(*h);
    }
    let candidates = library_candidates(name);
    for c in &candidates {
        if let Some(h) = typelisp_rt::os::dl_open(c) {
            cache.insert(name.to_string(), h);
            return Ok(h);
        }
    }
    Err(format!(
        "defffi: could not open library `{}` — tried {}",
        name,
        candidates.iter().map(|c| format!("`{}`", c)).collect::<Vec<_>>().join(", ")
    ))
}

/// The filenames `name` might be, most conventional first.
///
/// A name with a `/` in it is a path and is used as written; anything else is
/// a short name, decorated the way the platform decorates one.
fn library_candidates(name: &str) -> Vec<String> {
    if name.contains('/') {
        return vec![name.to_string()];
    }
    let ext = if cfg!(target_os = "macos") { "dylib" } else { "so" };
    vec![format!("lib{}.{}", name, ext), format!("{}.{}", name, ext), name.to_string()]
}

/// A thunk, and the engine holding the code it lives in.
struct FfiThunk {
    /// Held for its lifetime — dropping it would free the code at `addr`.
    /// `CompiledFn` holds an `ExecutionEngine` share for exactly this reason;
    /// see its doc comment. Also the one place that says which ABI the
    /// resolved entry answers to, which [`FfiThunk::body_abi`] reads back.
    code: CompiledFn,
    addr: usize,
}

impl CompiledBody for FfiThunk {
    fn address(&self) -> usize {
        self.addr
    }

    /// The address is the coroutine entry `emit_thunk` puts under the Lisp
    /// name, not the marshalling body behind it — so an interpreted caller
    /// has to drive it like any other compiled body. Reporting classic here
    /// made `CompiledBody::call` hand it `(args, argc)`, which the entry
    /// answers by reading arguments that were never put in `call_state`
    /// ("rt_pending_arg: argument 0 was not passed").
    ///
    /// Deferred to the `CompiledFn` that resolved that entry, so the answer
    /// is stated once, where the symbol was looked up.
    fn body_abi(&self) -> u8 {
        self.code.body_abi()
    }
}

/// `(defffi ...)`: resolve the symbol, emit the thunk, JIT it.
///
/// The `Interp` is unused today and taken anyway, so this matches the shape of
/// every other [`typelisp_front::eval::interp::Backend`] hook.
pub fn define_ffi(_interp: &Interp, decl: &FfiDecl) -> Result<Rc<dyn CompiledBody>, String> {
    // A declaration whose *last segment* is a builtin's name would be
    // miscompiled rather than rejected: `symbols::callee_symbol_name` maps
    // that name to the builtin's `rt_*` shim, so a compiled call site would
    // reach the shim instead of this thunk — and in a module other than the
    // root, the checker's own redefinition check never sees a clash to report.
    // Cheap to say so here, where the name and the shim table are both in
    // reach.
    let last = decl.path.last_segment();
    if crate::compile::externs::is_rt_builtin_name(last) {
        return Err(format!(
            "defffi: `{}` is the name of a builtin, and a compiled call to it would reach the \
             builtin rather than the C function. Declare it under another name, with the C symbol \
             written out: (defffi (my-{} \"{}\") ...)",
            last, last, decl.c_symbol
        ));
    }

    // Before any IR is built: a name that cannot be resolved should be
    // reported as the missing symbol it is, not as a link failure later.
    let addr = resolve(decl)?;

    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let module = llvm_context().create_module(&format!("ffi_{}", decl.c_symbol));
    let thunk = emit_thunk(&module, decl)?;
    let name = thunk.get_name().to_str().map_err(|e| format!("ffi: thunk name: {}", e))?.to_string();
    module
        .verify()
        .map_err(|e| format!("ffi: the thunk for `{}` is not valid IR: {}", decl.c_symbol, e.to_string()))?;

    // The C function, plus every `rt_*` shim the thunk actually declared (a
    // string conversion, or nothing at all). Filtered to what is in the module
    // because `CompiledFn::new` requires a forward declaration for each name
    // it is asked to map — the same filter `driver`'s bitcode path uses.
    let mut externals: Vec<(String, usize)> = vec![(decl.c_symbol.clone(), addr)];
    externals.extend(
        crate::compile::externs::rt_extern_functions()
            .iter()
            .filter(|(n, _)| module.get_function(n).is_some())
            .map(|(n, a)| (n.to_string(), *a)),
    );
    // `name` is the coroutine entry `emit_thunk` put under the Lisp name, not
    // the classic marshalling body behind it — so that is the ABI recorded
    // here, and `FfiThunk` reads it back rather than restating it.
    let code = CompiledFn::new(&module, &name, &externals, typelisp_abi::BODY_ABI_COROUTINE)
        .map_err(|e| format!("ffi: failed to JIT the thunk for `{}`: {}", decl.c_symbol, e))?;
    let addr = code.address();
    Ok(Rc::new(FfiThunk { code, addr }))
}

// ---- C calling back: the entries ------------------------------------------
//
// A `defffi` parameter declared `(fn (T...) R)` hands C the address of an
// **entry**: a function with that C signature which runs a typelisp function.
// The mirror image of a thunk. A thunk turns words into C values, calls C and
// turns the answer back; an entry turns C values into words, runs the
// function (`typelisp_rt::ffi_callback::rt_ffi_callback_invoke`) and turns
// its answer into the C value it returns. Both conversions are the ones the
// thunk already has, run in the other direction.

/// A callback's C parameter and return types, from their type keys — or why
/// C cannot call a function with them.
fn callback_ctypes(params: &[String], ret: &str) -> Result<(Vec<CType>, CType), String> {
    let mut out = Vec::with_capacity(params.len());
    for p in params {
        match CType::from_key(p) {
            Some(CType::Void) => return Err("a callback parameter cannot be `()`".to_string()),
            Some(c) => out.push(c),
            None => {
                return Err(format!(
                    "a callback takes `{}`, which the FFI cannot spell in C. It can spell the \
                     integer widths (i8/i16/i32/u8/u16/u32), c-long, c-ulong, f32, f64, bool, \
                     string and ptr.",
                    p
                ))
            }
        }
    }
    let ret = match CType::from_key(ret) {
        // Who would free it? C did not make it and cannot know how.
        Some(CType::Str) => {
            return Err(
                "a callback cannot return `string` — C would be handed memory nobody frees. \
                 Return a `ptr` to memory C owns instead."
                    .to_string(),
            )
        }
        Some(c) => c,
        None => return Err(format!("a callback returns `{}`, which the FFI cannot spell in C", ret)),
    };
    Ok((out, ret))
}

/// Emit an entry named `name` into `module`: a function with the C signature
/// `params`/`ret` that runs the coroutine-ABI body at `target` — a constant
/// address (JIT) or a function of the same module (AOT).
fn emit_callback_entry(
    module: &Module<'static>,
    name: &str,
    target: IntValue<'static>,
    params: &[CType],
    ret: CType,
) -> Result<FunctionValue<'static>, String> {
    let ctx = llvm_context();
    let i64_ty = ctx.i64_type();
    let builder = ctx.create_builder();
    let arg_tys: Vec<BasicMetadataTypeEnum<'static>> =
        params.iter().map(|c| c.llvm().expect("a callback parameter is never void").into()).collect();
    let fn_ty = match ret.llvm() {
        Some(t) => t.fn_type(&arg_tys, false),
        None => ctx.void_type().fn_type(&arg_tys, false),
    };
    let entry = module.add_function(name, fn_ty, None);
    // The same extensions a call site asks for, from the callee's side: C
    // extends a narrow argument before the call and expects a narrow answer
    // already extended.
    for (i, c) in params.iter().enumerate() {
        if let Some(attr) = c.extension_attribute() {
            add_attribute(entry, inkwell::attributes::AttributeLoc::Param(i as u32), attr);
        }
    }
    if let Some(attr) = ret.extension_attribute() {
        add_attribute(entry, inkwell::attributes::AttributeLoc::Return, attr);
    }
    builder.position_at_end(ctx.append_basic_block(entry, "entry"));
    let shim = |n: &str| match module.get_function(n) {
        Some(f) => f,
        None => module.add_function(n, compiled_fn_type(), None),
    };
    let scratch = builder
        .build_alloca(i64_ty, "cb_scratch")
        .map_err(|e| format!("ffi: failed to reserve the callback scratch slot: {}", e))?;
    let one = ctx.i32_type().const_int(1, false);
    let zero = i64_ty.const_zero();

    call_shim(&builder, shim("rt_ffi_callback_enter"), scratch, one, zero, "cb_enter")?;

    // `[target, n, kind0, word0, ...]` — see `rt_ffi_callback_invoke`.
    let len = 2 + 2 * params.len();
    let words = builder
        .build_alloca(i64_ty.array_type(len as u32), "cb_args")
        .map_err(|e| format!("ffi: failed to reserve the callback argument array: {}", e))?;
    let store = |i: usize, w: IntValue<'static>| -> Result<(), String> {
        let slot = unsafe {
            builder
                .build_gep(i64_ty, words, &[i64_ty.const_int(i as u64, false)], "cb_arg_slot")
                .map_err(|e| format!("ffi: failed to index the callback argument array: {}", e))?
        };
        builder.build_store(slot, w).map_err(|e| format!("ffi: failed to store a callback argument: {}", e))?;
        Ok(())
    };
    store(0, target)?;
    store(1, i64_ty.const_int(params.len() as u64, false))?;
    for (i, c) in params.iter().enumerate() {
        let v = entry.get_nth_param(i as u32).expect("the entry has one parameter per declared type");
        let (kind, word) = match c {
            // Copied into a typelisp string by the runtime, where a bad one
            // can fail inside the callback rather than here.
            CType::Str => (
                1,
                builder
                    .build_ptr_to_int(v.into_pointer_value(), i64_ty, "cb_cstr")
                    .map_err(|e| format!("ffi: failed to take a string argument's address: {}", e))?,
            ),
            _ => (0, c_to_word(&builder, v, *c)?),
        };
        store(2 + 2 * i, i64_ty.const_int(kind, false))?;
        store(3 + 2 * i, word)?;
    }
    let call = builder
        .build_call(
            shim("rt_ffi_callback_invoke"),
            &[words.into(), ctx.i32_type().const_int(len as u64, false).into()],
            "cb_value",
        )
        .map_err(|e| format!("ffi: failed to call the callback: {}", e))?;
    let value = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
        inkwell::values::ValueKind::Instruction(_) => {
            return Err("internal error: the callback runner produced no value".to_string())
        }
    };
    // Converted before the native section is re-entered, though it touches
    // no heap: every step after `rt_ffi_callback_leave` is C's.
    let out = match ret {
        CType::Void => None,
        _ => Some(word_to_c(&builder, value, ret)?),
    };
    call_shim(&builder, shim("rt_ffi_callback_leave"), scratch, one, zero, "cb_leave")?;
    match out {
        Some(v) => builder.build_return(Some(&v)),
        None => builder.build_return(None),
    }
    .map_err(|e| format!("ffi: failed to build the callback's return: {}", e))?;
    Ok(entry)
}

/// The C types a callback key declares.
fn callback_sig_ctypes(sig: &typelisp_front::ffi_callback::CallbackSig) -> Result<(Vec<CType>, CType), String> {
    callback_ctypes(&sig.params, &sig.ret)
}

/// [`emit_callback_entry`] for the callback `sig` names, into an executable's
/// module: the body is the module's own compiled function.
pub(crate) fn emit_callback_entry_in_module(
    module: &Module<'static>,
    name: &str,
    sig: &typelisp_front::ffi_callback::CallbackSig,
) -> Result<FunctionValue<'static>, String> {
    let (params, ret) = callback_sig_ctypes(sig)?;
    let symbol = crate::compile::symbols::user_symbol_name(&sig.path);
    let body = module
        .get_function(&symbol)
        .ok_or_else(|| format!("compile-file: `{}` is passed to C as a callback but was not compiled", sig.path))?;
    let target = body.as_global_value().as_pointer_value().const_to_int(llvm_context().i64_type());
    emit_callback_entry(module, name, target, &params, ret)
}

thread_local! {
    /// The code of every entry made on this thread. Never dropped: C may keep
    /// an entry's address for as long as the process lives. (The body an
    /// entry runs is kept alive by the interpreter's own table.)
    static SESSION_ENTRIES: std::cell::RefCell<Vec<CompiledFn>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// `Backend::callback_entry`: compile the function `key` names if it is not
/// already, emit its entry and JIT it.
pub fn callback_entry(
    interp: &Interp,
    heap: &mut typelisp_mem::Heap,
    key: &str,
) -> Result<(usize, Rc<dyn CompiledBody>), String> {
    let sig = typelisp_front::ffi_callback::CallbackSig::parse(key)?;
    let (params, ret) = callback_sig_ctypes(&sig)?;
    let path = typelisp_front::dump::parse_path(&sig.path);
    let compiled = |interp: &Interp| interp.root.borrow().get_fn(&path).and_then(|f| f.compiled.borrow().clone());
    if compiled(interp).is_none() {
        let target = crate::CompileTarget::Fn(typelisp_front::check::resolved::Ref::synthetic(path.clone()));
        crate::compile::driver::compile_function(interp, heap, &target)
            .map_err(|e| format!("ffi: `{}` is passed to C as a callback and cannot be compiled: {}", path, e))?;
    }
    let body = compiled(interp).ok_or_else(|| format!("ffi: compiling `{}` left it without a body", path))?;
    if body.body_abi() != typelisp_abi::BODY_ABI_COROUTINE {
        return Err(format!("internal error: `{}` is compiled under the classic ABI", path));
    }

    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let module = llvm_context().create_module("ffi_callback");
    let target = llvm_context().i64_type().const_int(body.address() as u64, false);
    let entry = emit_callback_entry(&module, "ffi_callback_entry", target, &params, ret)?;
    let name = entry.get_name().to_str().map_err(|e| format!("ffi: entry name: {}", e))?.to_string();
    module
        .verify()
        .map_err(|e| format!("ffi: the callback entry for `{}` is not valid IR: {}", path, e.to_string()))?;
    let externals: Vec<(String, usize)> = crate::compile::externs::rt_extern_functions()
        .iter()
        .filter(|(n, _)| module.get_function(n).is_some())
        .map(|(n, a)| (n.to_string(), *a))
        .collect();
    let code = CompiledFn::new(&module, &name, &externals, typelisp_abi::BODY_ABI_CLASSIC)
        .map_err(|e| format!("ffi: failed to JIT the callback entry for `{}`: {}", path, e))?;
    let addr = code.address();
    SESSION_ENTRIES.with(|v| v.borrow_mut().push(code));
    Ok((addr, body))
}
