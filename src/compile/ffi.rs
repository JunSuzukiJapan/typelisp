//! The C FFI: what `(defffi ...)` compiles to.
//!
//! # The problem this solves
//!
//! A compiled call site names its callee to the driver, so every callee
//! answers the coroutine ABI (`llvm_builtins::coroutine_fn_type`): it finds
//! its arguments where the driver left them and leaves its answer in its
//! frame. A C function is not written that way. `strlen` takes a pointer and
//! answers with a `size_t`; `sqrt` takes and answers with a `double`.
//!
//! So each declaration gets a **thunk**: a coroutine-ABI function whose body
//! takes the argument words, converts them to the C types declared for them,
//! calls the real function under its real signature, and converts the answer
//! back. One function, emitted here, in LLVM IR.
//!
//! # Why that makes the rest of the compiler unaware of the FFI
//!
//! The thunk is named by [`ConcreteNames::symbol`] — the
//! same rule a `defun`'s compiled body is named by. Two things follow, and
//! they are the whole reason for this design:
//!
//! - **The interpreter needs no wiring.** A thunk is a `CompiledBody` hung on
//!   the declaration's `FnDef`, and `Interp::enter` already prefers a compiled
//!   body over walking forms.
//! - **The self-hosted compiler needs no changes.** A compiled call site asks
//!   `symbols::callee_symbol_name` for the callee's symbol and emits a call to
//!   it; for an FFI declaration that is the thunk's own name, under the
//!   ABI the island always calls. The island never learns that C is
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
use typelisp_front::ffi_callback::CallbackSig;
use typelisp_front::Type;
use typelisp_mem::{ConcreteNames, TypeId, TypeShape};

use crate::compile::carriers::{self, Carrier};
use crate::compile::llvm_builtins::{pending_words, store_words};
use crate::compile::{llvm_context, CompiledFn, COMPILE_LOCK};

/// A type as the C side of the boundary sees it.
///
/// A second vocabulary beside `Repr`, and the reason is `Repr::RawWord`: it
/// folds `ptr`, `c-long` and `c-ulong` into one, because how a value crosses
/// the compiled boundary is the same for all of them (an untagged word). The
/// thunk needs the other answer — C's `long` is as wide as the target says,
/// and a pointer is a pointer to the C function.
///
/// **This enum is the one list of what the FFI can spell.** The checker
/// deliberately does not keep a second one: it parses the declared types like
/// any other annotation and lets [`Self::from_type`] be the judge, because the
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
    /// An opaque pointer. Crosses as a raw word in both directions —
    /// see [`typelisp_front::check::repr::Repr::RawWord`] for why it may not
    /// be stored anywhere that tags what it holds.
    Ptr,
}

impl CType {
    /// The C type a declared typelisp type means, or `None` if it means none.
    fn from_type(ty: TypeId) -> Option<CType> {
        let scalar = match typelisp_mem::type_shape(ty) {
            // A callback parameter: what crosses is the address of the entry
            // C calls (`typelisp_front::ffi_callback`), so to the thunk it is
            // a pointer. Whether the callback's own types can be spelled is
            // [`callback_ctypes`]'s question.
            TypeShape::Fn(..) => return Some(CType::Ptr),
            // A typed pointer is an address like any other; which C object it
            // points at is checked by the runtime, not the C signature.
            TypeShape::Ptr(_) => return Some(CType::Ptr),
            TypeShape::Unit => return Some(CType::Void),
            // The primitives are `std::primitive`'s types, which take no arguments.
            TypeShape::Named(ns, name, args) if ns == typelisp_mem::StdModule::PRIMITIVE.ns() && args.is_empty() => {
                typelisp_front::types::scalar_of_symbol(name)?
            }
            _ => return None,
        };
        // Covers the six widths *and* `c-long`/`c-ulong`, which that function
        // answers the target's C `long` width for.
        if let Some((bits, signed)) = typelisp_front::types::int_width_signed(&scalar) {
            return Some(CType::Int { bits, signed });
        }
        match scalar {
            Type::F32 => Some(CType::F32),
            Type::F64 => Some(CType::F64),
            Type::Bool => Some(CType::Bool),
            Type::Str => Some(CType::Str),
            Type::Ptr => Some(CType::Ptr),
            _ => None,
        }
    }

    /// What a value of this type is, as an LLVM type — `None` for `Void`,
    /// which is not a value.
    fn llvm(self) -> Option<BasicTypeEnum<'static>> {
        let ctx = llvm_context();
        Some(match self {
            CType::Int { bits, .. } => crate::compile::carriers::int(bits).into(),
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
    let ret = CType::from_type(decl.ret).ok_or_else(|| unspellable(decl.ret, "a return type", decl))?;

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

    // ---- the thunk ----
    let name = ConcreteNames::symbol(&decl.path.to_string());
    if module.get_function(&name).is_some() {
        return Err(format!("internal error: `{}` is already defined in this module", name));
    }
    // A coroutine-ABI function like any other compiled one, with no `pc`
    // dispatch and no resume block: it makes its frame, runs to completion and
    // answers in the frame's value slot. The frame is still required — the
    // driver takes the frame `rt_frame_open` publishes on the way back, and
    // asks its handler slot about an unwind.
    let thunk = module.add_function(&name, crate::compile::llvm_builtins::coroutine_fn_type(), None);
    let entry = ctx.append_basic_block(thunk, "entry");
    builder.position_at_end(entry);

    let word_ty = carriers::word();
    // The driver protocol's own slots, and nothing else: no locals, so no slot
    // is rooted and there is no mask.
    let value_words = carriers::value_words();
    let frame = call_shim(
        &builder,
        module,
        "rt_frame_open",
        &[word_ty.const_int(typelisp_abi::frame_reserved_slots(value_words) as u64, false), word_ty.const_zero()],
    )?;

    let mut call_args: Vec<BasicMetadataValueEnum<'static>> = Vec::with_capacity(params.len());
    // The C strings made for this call, to be freed once it has returned.
    let mut owned_cstrings: Vec<IntValue<'static>> = Vec::new();
    let mut arg_words: Vec<Vec<IntValue<'static>>> = Vec::with_capacity(params.len());
    let mut at = 0;
    for c in &params {
        // The words the caller passed. Their shape is the *declared* one —
        // `encode_crossing_args` was driven by the same signature this thunk
        // was built from. All of them are read before anything
        // else runs.
        let n = c.llvm().map_or(1, carriers::words_of);
        let words = pending_words(&builder, module, "rt_pending_arg", at, n).map_err(|e| format!("ffi: {}", e))?;
        at += n;
        arg_words.push(words);
    }
    for (c, words) in params.iter().zip(arg_words) {
        if *c == CType::Str {
            let made = call_shim(&builder, module, "rt_ffi_cstring_new", &words)?;
            owned_cstrings.push(made);
            let p = builder
                .build_int_to_ptr(made, ctx.ptr_type(inkwell::AddressSpace::default()), "cstr_ptr")
                .map_err(|e| format!("ffi: failed to make a pointer from a C string: {}", e))?;
            call_args.push(p.into());
        } else {
            call_args.push(words_to_c(&builder, &words, *c)?.into());
        }
    }

    // Native for exactly the C call: everything it is given is a C value
    // by now, and nothing it answers is a Lisp one yet — see
    // `typelisp_rt::rt_ffi_enter_native`.
    call_shim(&builder, module, "rt_ffi_enter_native", &[])?;
    let call = builder
        .build_call(c_fn, &call_args, "ffi_call")
        .map_err(|e| format!("ffi: failed to build the call to `{}`: {}", decl.c_symbol, e))?;
    call_shim(&builder, module, "rt_ffi_leave_native", &[])?;
    let out = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) if ret == CType::Str => {
            // Copied *before* the arguments are freed: a C function that
            // answers with a pointer into one of them (`strchr`, `strstr`) is
            // ordinary, and freeing first would leave this reading freed
            // memory. The cost is that a `raise` in here — a null result, or
            // bytes that are not UTF-8 — leaks the argument strings, on a
            // path that is ending the call anyway.
            let raw = builder
                .build_ptr_to_int(v.into_pointer_value(), word_ty, "ret_cstr")
                .map_err(|e| format!("ffi: failed to take the address of a string result: {}", e))?;
            vec![call_shim(&builder, module, "rt_ffi_string_from_cstr", &[raw])?]
        }
        inkwell::values::ValueKind::Basic(v) => c_to_words(&builder, v, ret)?,
        // A `void` C function still has to answer with a word. `Repr::Unit` crosses as 0, so this is the
        // same placeholder every other unit-valued call produces.
        inkwell::values::ValueKind::Instruction(_) => vec![word_ty.const_zero()],
    };
    for made in owned_cstrings {
        call_shim(&builder, module, "rt_ffi_cstring_free", &[made])?;
    }
    let frame = builder
        .build_int_to_ptr(frame, ctx.ptr_type(inkwell::AddressSpace::default()), "ffi_frame")
        .map_err(|e| format!("ffi: failed to take the frame's address: {}", e))?;
    store_words(&builder, frame, typelisp_abi::FRAME_VALUE_SLOT, &out, "ffi_value_slot").map_err(|e| format!("ffi: failed to store the result: {}", e))?;
    builder
        .build_return(Some(&word_ty.const_int(typelisp_abi::STATUS_RETURN as u64, false)))
        .map_err(|e| format!("ffi: failed to build the return: {}", e))?;
    Ok(thunk)
}

/// Calls the runtime function `name` with `args` and answers its word,
/// leaving the current function if it raised.
fn call_shim(
    builder: &inkwell::builder::Builder<'static>,
    module: &Module<'static>,
    name: &str,
    args: &[IntValue<'static>],
) -> Result<IntValue<'static>, String> {
    let args: Vec<BasicValueEnum<'static>> = args.iter().map(|a| (*a).into()).collect();
    let (f, v) = crate::compile::runtime_abi::call_word(builder, module, name, &args).map_err(|e| format!("ffi: {}", e))?;
    crate::compile::raise_check::after_call(builder, f).map_err(|e| format!("ffi: {}", e))?;
    Ok(v)
}

/// Every declared parameter as a [`CType`], or an error naming the first one
/// that is not spellable in C.
fn ctypes(types: &[TypeId], decl: &FfiDecl) -> Result<Vec<CType>, String> {
    // A callback parameter's own types are checked here, at the declaration,
    // rather than when an entry is first made for it.
    for t in types {
        if let TypeShape::Fn(params, _, ret) = typelisp_mem::type_shape(*t) {
            callback_ctypes(&params, ret).map_err(|e| format!("defffi: `{}`: {}", decl.path, e))?;
        }
    }
    types
        .iter()
        .map(|t| match CType::from_type(*t) {
            Some(CType::Void) => Err(format!(
                "defffi: `{}` declares a `()` parameter, which is not a value C can be passed",
                decl.path
            )),
            Some(c) => Ok(c),
            None => Err(unspellable(*t, "a parameter type", decl)),
        })
        .collect()
}

fn unspellable(ty: TypeId, where_: &str, decl: &FfiDecl) -> String {
    format!(
        "defffi: `{}` declares `{}` as {}, which the FFI cannot spell in C. \
         It can spell the integer widths (i8/i16/i32/u8/u16/u32), c-long, c-ulong, f32, f64, \
         bool, string, ptr, and `()`.",
        decl.path, typelisp_print::shared::name_view().type_name(ty), where_
    )
}

/// The words an argument or an answer crosses in, as the C type the
/// declaration named.
fn words_to_c(
    builder: &inkwell::builder::Builder<'static>,
    words: &[IntValue<'static>],
    c: CType,
) -> Result<BasicValueEnum<'static>, String> {
    let ctx = llvm_context();
    let [word, ..] = words else { return Err("internal error: an argument in no words".to_string()) };
    let carrier = match c {
        // A `ptr` is already the whole word; it only has to become a pointer.
        CType::Ptr => {
            return Ok(builder
                .build_int_to_ptr(*word, ctx.ptr_type(inkwell::AddressSpace::default()), "arg_ptr")
                .map_err(|e| format!("ffi: failed to make a pointer argument: {}", e))?
                .into())
        }
        CType::Int { bits, signed } => Carrier::Int { bits, signed },
        CType::Bool => Carrier::Bool,
        CType::F32 => Carrier::F32,
        CType::F64 => Carrier::F64,
        // Both of these are handled in `emit_thunk`, beside the runtime shims
        // they need and this converter cannot reach.
        CType::Str | CType::Void => return Err(format!("internal error: {:?} reached the scalar argument converter", c)),
    };
    carrier.from_words(builder, words).map_err(|e| format!("ffi: failed to convert an argument: {}", e))
}

/// What the C function answered with, as the words it crosses in.
fn c_to_words(
    builder: &inkwell::builder::Builder<'static>,
    v: BasicValueEnum<'static>,
    c: CType,
) -> Result<Vec<IntValue<'static>>, String> {
    let word_ty = carriers::word();
    let v = match c {
        // A `c-long` narrower than a word is the whole word here.
        CType::Int { bits, signed: true } if bits < word_ty.get_bit_width() => builder
            .build_int_s_extend(v.into_int_value(), word_ty, "ret_sext")
            .map_err(|e| format!("ffi: failed to widen a signed result: {}", e))?
            .into(),
        CType::Str | CType::Void => return Err(format!("internal error: {:?} reached the scalar result converter", c)),
        _ => v,
    };
    carriers::to_words(builder, v).map_err(|e| format!("ffi: failed to convert a result: {}", e))
}

fn add_attribute(f: FunctionValue<'static>, loc: inkwell::attributes::AttributeLoc, name: &str) {
    let ctx = llvm_context();
    let kind = inkwell::attributes::Attribute::get_named_enum_kind_id(name);
    f.add_attribute(loc, ctx.create_enum_attribute(kind, 0));
}

/// Libraries opened so far, by the name the declaration wrote.
///
/// Here rather than in `typelisp_rt_posix::os`, which is a safe wrapper over one C
/// call per function and has no business holding process-wide state. Never
/// emptied: see [`typelisp_rt_posix::os::dl_open`] for why nothing is ever closed.
static OPEN_LIBS: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

thread_local! {
    /// Whether the C functions `defffi` names are the compiled program's
    /// target's and not this process's: set while a program for another
    /// machine compiles on this thread ([`CFunctionsOn::target`]).
    static C_FUNCTIONS_ON_TARGET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// While it lives, `defffi` on this thread does not look its C function up
/// in this process: the function is the target's, which only the
/// executable's link finds. The interpreter's thunk then calls [`not_here`] instead.
pub(crate) struct CFunctionsOn {
    before: bool,
}

impl CFunctionsOn {
    /// `on_target`: the C functions are the target's.
    pub(crate) fn target(on_target: bool) -> CFunctionsOn {
        CFunctionsOn { before: C_FUNCTIONS_ON_TARGET.with(|c| c.replace(on_target)) }
    }
}

impl Drop for CFunctionsOn {
    fn drop(&mut self) {
        C_FUNCTIONS_ON_TARGET.with(|c| c.set(self.before));
    }
}

/// The names of the C functions [`not_here`] stands in for, for its message:
/// it is one function for all of them, so it cannot tell which was called.
static NOT_HERE_NAMES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// What the interpreter's thunk calls in place of the target's C function
/// while the target's program compiles: running it here, at compile time
/// (from a macro, say), cannot be done, and nothing it could answer would be
/// the target's. Ends `typl` with the reason; called with whatever the C
/// signature passes, which it reads none of.
extern "C" fn not_here() {
    let names = NOT_HERE_NAMES.lock().unwrap_or_else(|e| e.into_inner()).join("`, `");
    eprintln!(
        "error: one of the target's C functions (`{}`) was called while typl compiled the program for the \
         target; they run only on the target",
        names
    );
    std::process::exit(1)
}

/// The address `decl` names, or an error saying what was looked for and where.
fn resolve(decl: &FfiDecl) -> Result<usize, String> {
    if C_FUNCTIONS_ON_TARGET.with(|c| c.get()) {
        NOT_HERE_NAMES.lock().unwrap_or_else(|e| e.into_inner()).push(decl.c_symbol.clone());
        return Ok(not_here as *const () as usize);
    }
    let handle = match &decl.library {
        None => 0,
        Some(lib) => open_library(lib)?,
    };
    typelisp_rt_posix::os::dl_sym(handle, &decl.c_symbol).ok_or_else(|| match &decl.library {
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
        if let Some(h) = typelisp_rt_posix::os::dl_open(c) {
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

/// A thunk, and the JIT-compiled code it lives in.
struct FfiThunk {
    /// Held for its lifetime — dropping it would free the code at `addr`.
    /// `CompiledFn` holds a share of its code for exactly this reason; see its
    /// doc comment. Also the one place that says which ABI the
    /// resolved entry answers to, which [`FfiThunk::body_abi`] reads back.
    code: CompiledFn,
    addr: usize,
}

impl CompiledBody for FfiThunk {
    fn address(&self) -> usize {
        self.addr
    }

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
    // A declaration at a builtin's own path would be miscompiled rather than
    // rejected: `symbols::callee_symbol_name` maps that path to the builtin's
    // `rt_*` shim, so a compiled call site would reach the shim instead of
    // this thunk. Cheap to say so here, where the path and the shim table are
    // both in reach.
    let last = decl.path.last_segment();
    if crate::compile::externs::is_rt_builtin(&decl.path) {
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
    externals.extend(crate::compile::runtime_externals(&module));
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

/// A callback's C parameter and return types — or why C cannot call a
/// function with them.
fn callback_ctypes(params: &[TypeId], ret: TypeId) -> Result<(Vec<CType>, CType), String> {
    let mut out = Vec::with_capacity(params.len());
    for p in params {
        match CType::from_type(*p) {
            Some(CType::Void) => return Err("a callback parameter cannot be `()`".to_string()),
            Some(c) => out.push(c),
            None => {
                return Err(format!(
                    "a callback takes `{}`, which the FFI cannot spell in C. It can spell the \
                     integer widths (i8/i16/i32/u8/u16/u32), c-long, c-ulong, f32, f64, bool, \
                     string and ptr.",
                    typelisp_print::shared::name_view().type_name(*p)
                ))
            }
        }
    }
    let ret = match CType::from_type(ret) {
        // Who would free it? C did not make it and cannot know how.
        Some(CType::Str) => {
            return Err(
                "a callback cannot return `string` — C would be handed memory nobody frees. \
                 Return a `ptr` to memory C owns instead."
                    .to_string(),
            )
        }
        Some(c) => c,
        None => return Err(format!("a callback returns `{}`, which the FFI cannot spell in C", typelisp_print::shared::name_view().type_name(ret))),
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
    pointees: &[Option<TypeId>],
    ret: CType,
) -> Result<FunctionValue<'static>, String> {
    let ctx = llvm_context();
    let word_ty = carriers::word();
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
    let zero = word_ty.const_zero();

    call_shim(&builder, module, "rt_ffi_callback_enter", &[])?;

    // `[target, n, kind0, word0, aux0, ...]` — see `rt_ffi_callback_invoke`.
    let mut triples: Vec<IntValue<'static>> = Vec::with_capacity(3 * params.len());
    for (i, c) in params.iter().enumerate() {
        let v = entry.get_nth_param(i as u32).expect("the entry has one parameter per declared type");
        let (kind, words, aux) = match (c, pointees.get(i).copied().flatten()) {
            // Copied into a typelisp string by the runtime, where a bad one
            // can fail inside the callback rather than here.
            (CType::Str, _) => (
                1,
                vec![builder
                    .build_ptr_to_int(v.into_pointer_value(), word_ty, "cb_cstr")
                    .map_err(|e| format!("ffi: failed to take a string argument's address: {}", e))?],
                zero,
            ),
            // A typed pointer, checked by the runtime against the memory
            // `unsafe`s allocated — `aux` is the pointee type, read out of
            // the module's type table.
            (_, Some(pointee)) => {
                let pointee = crate::compile::type_table::load(&builder, module, pointee)?;
                (2, c_to_words(&builder, v, *c)?, pointee)
            }
            _ => (0, c_to_words(&builder, v, *c)?, zero),
        };
        for word in words {
            triples.extend([word_ty.const_int(kind, false), word, aux]);
        }
    }
    let mut given = vec![target, word_ty.const_int(triples.len() as u64 / 3, false)];
    given.extend(triples);
    let len = given.len();
    let words = builder
        .build_alloca(word_ty.array_type(len as u32), "cb_args")
        .map_err(|e| format!("ffi: failed to reserve the callback argument array: {}", e))?;
    store_words(&builder, words, 0, &given, "cb_arg_slot").map_err(|e| format!("ffi: failed to store the callback arguments: {}", e))?;
    let (_, value) = crate::compile::runtime_abi::call(
        &builder,
        module,
        "rt_ffi_callback_invoke",
        &[words.into(), ctx.i32_type().const_int(len as u64, false).into()],
    )
    .map_err(|e| format!("ffi: failed to call the callback: {}", e))?;
    // Converted before the native section is re-entered, though it touches
    // no heap: every step after `rt_ffi_callback_leave` is C's.
    let out = match ret {
        CType::Void => None,
        _ => {
            let n = ret.llvm().map_or(1, carriers::words_of);
            let words = carriers::to_words(&builder, value).map_err(|e| format!("ffi: failed to split the callback's answer: {}", e))?;
            Some(words_to_c(&builder, &words[..n], ret)?)
        }
    };
    call_shim(&builder, module, "rt_ffi_callback_leave", &[])?;
    match out {
        Some(v) => builder.build_return(Some(&v)),
        None => builder.build_return(None),
    }
    .map_err(|e| format!("ffi: failed to build the callback's return: {}", e))?;
    Ok(entry)
}

/// [`emit_callback_entry`] for the callback `sig`, into an executable's
/// module: the body is the module's own compiled function.
pub(crate) fn emit_callback_entry_in_module(
    module: &Module<'static>,
    name: &str,
    sig: &CallbackSig,
) -> Result<FunctionValue<'static>, String> {
    let (params, ret) = sig.signature();
    let (ctypes, cret) = callback_ctypes(&params, ret)?;
    let symbol = ConcreteNames::symbol(&sig.function.to_string());
    let body = module
        .get_function(&symbol)
        .ok_or_else(|| format!("compile-file: `{}` is passed to C as a callback but was not compiled", sig.function))?;
    let target = body.as_global_value().as_pointer_value().const_to_int(crate::compile::carriers::word());
    emit_callback_entry(module, name, target, &ctypes, &pointees(&params), cret)
}

/// Each parameter's pointee type, for the ones that are typed pointers.
fn pointees(params: &[TypeId]) -> Vec<Option<TypeId>> {
    params
        .iter()
        .map(|p| match typelisp_mem::type_shape(*p) {
            TypeShape::Ptr(pointee) => Some(pointee),
            _ => None,
        })
        .collect()
}

thread_local! {
    /// The code of every entry made on this thread. Never dropped: C may keep
    /// an entry's address for as long as the process lives. (The body an
    /// entry runs is kept alive by the interpreter's own table.)
    static SESSION_ENTRIES: std::cell::RefCell<Vec<CompiledFn>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// `Backend::callback_entry`: compile the function `sig` names if it is not
/// already, emit its entry and JIT it.
pub fn callback_entry(
    interp: &Interp,
    heap: &mut typelisp_mem::Heap,
    sig: CallbackSig,
) -> Result<(usize, Rc<dyn CompiledBody>), String> {
    let (params, ret) = sig.signature();
    let (ctypes, cret) = callback_ctypes(&params, ret)?;
    let path = sig.function;
    let compiled = |interp: &Interp| interp.root.borrow().get_fn(&path).and_then(|f| f.compiled.borrow().clone());
    if compiled(interp).is_none() {
        let target = crate::CompileTarget::Fn(path);
        crate::compile::driver::compile_function(interp, heap, &target)
            .map_err(|e| format!("ffi: `{}` is passed to C as a callback and cannot be compiled: {}", path, e))?;
    }
    let body = compiled(interp).ok_or_else(|| format!("ffi: compiling `{}` left it without a body", path))?;
    if body.body_abi() != typelisp_abi::BODY_ABI_COROUTINE {
        return Err(format!("internal error: `{}` is compiled under the classic ABI", path));
    }

    let _guard = COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let module = llvm_context().create_module("ffi_callback");
    let target = crate::compile::carriers::word().const_int(body.address() as u64, false);
    let entry = emit_callback_entry(&module, "ffi_callback_entry", target, &ctypes, &pointees(&params), cret)?;
    let name = entry.get_name().to_str().map_err(|e| format!("ffi: entry name: {}", e))?.to_string();
    module
        .verify()
        .map_err(|e| format!("ffi: the callback entry for `{}` is not valid IR: {}", path, e.to_string()))?;
    let externals = crate::compile::runtime_externals(&module);
    let code = CompiledFn::new(&module, &name, &externals, typelisp_abi::BODY_ABI_CLASSIC)
        .map_err(|e| format!("ffi: failed to JIT the callback entry for `{}`: {}", path, e))?;
    let addr = code.address();
    SESSION_ENTRIES.with(|v| v.borrow_mut().push(code));
    Ok((addr, body))
}
