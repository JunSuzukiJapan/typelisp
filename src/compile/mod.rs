//! Shared core for the LLVM-based `compile` (JIT) and `compile-file` (AOT)
//! paths. The compiler body itself (AST -> LLVM IR) is typelisp code loaded
//! the same way `prelude.rs` loads the standard library; this module only
//! provides the pieces that have to live in Rust: the LLVM context/locking,
//! and (in later phases) the AST bridge and runtime shims.

/// Every runtime function a compiled body can call, by name.
///
/// `Interp::compile_function` declares all of these into its module before it
/// compiles anything, and the island's `get-function` aborts the process on a
/// name that is not there. Exposed so anything else that drives
/// `compile-function` directly — a test standing in for the driver, say — can
/// install the same declarations, rather than rediscovering the set one abort
/// at a time. The list is the driver's own, so it cannot drift from it.
pub fn runtime_function_names() -> Vec<&'static str> {
    crate::compile::externs::rt_extern_functions().iter().map(|(name, _)| *name).collect()
}

/// `(name, address)` for every runtime function `module` declares — the part
/// of [`jit_engine`]'s `externals` that the runtime supplies. A module that
/// declares the whole set ([`runtime_function_names`]) gets the whole set
/// back; one that declares three gets three.
pub fn runtime_externals(module: &Module<'static>) -> Vec<(String, usize)> {
    crate::compile::externs::rt_extern_functions()
        .iter()
        .filter(|(name, _)| module.get_function(name).is_some())
        .map(|(name, addr)| (name.to_string(), *addr))
        .collect()
}

/// The JIT-compiled code of `module`, linked against `externals` — **the only
/// way this crate JIT-compiles anything**.
///
/// `externals` is `(name, address)` for everything `module` calls that it does
/// not define. Nothing else is looked for: a name the code refers to and no
/// external supplies — whether `module` declares it or code generation added
/// the call itself — is an error naming that symbol, on every OS (see
/// [`orc`]). Every function `module` defines is resolved here, so a failure to
/// link is reported now rather than on first call.
///
/// Must be called with [`COMPILE_LOCK`] held, like everything that touches
/// the shared context.
pub fn jit_engine(module: &Module<'static>, externals: &[(String, usize)]) -> Result<orc::JitCode, String> {
    orc::JitCode::new(module, externals)
}

pub mod abi_signature;
pub mod aot;
pub mod bootstrap;
pub mod core_bridge;
pub mod driver;
pub mod dump;
pub mod externs;
pub mod ffi;
pub mod llvm_builtins;
pub mod orc;

/// Registers this crate's `llvm-*` builtin implementations with the
/// interpreter, for the current thread.
///
/// The interpreter reaches them through a hook rather than by naming them
/// (see [`llvm_builtins`]'s doc comment for why the dependency points this
/// way), so *something* has to connect the two. Ordinarily that is
/// [`crate::compiler::load_aot`], since loading the island is what makes the
/// `llvm-*` builtins reachable in the first place — every driver, both
/// bootstrappers and `compile-file` come through it.
///
/// Called directly only by code that drives the builders without an island:
/// the tests that build a module by hand. An `Interp` with neither is not
/// broken, it simply has no backend, and says so.
pub fn install_llvm_backend() {
    crate::eval::interp::set_backend(crate::eval::interp::Backend {
        llvm_builtin: llvm_builtins::eval_llvm_builtin_method,
        handle_is_live: |h| llvm_builtins::llvm_handle_get(h).is_some(),
        compile_function: driver::compile_function,
        disassemble_function: driver::disassemble_function,
        compile_file: aot::compile_file,
        dump_image: dump::dump_image,
        define_ffi: ffi::define_ffi,
        callback_entry: ffi::callback_entry,
    });
}
pub mod prelude_bootstrap;
pub mod symbols;

/// One precompiled bitcode artifact, ready to install into an `Interp` —
/// everything `Interp::install_compiled_library` needs to know about it.
///
/// A struct rather than four positional parameters because the artifacts that
/// go through it (the compiler island, the prelude, a session's own compiled
/// definitions) differ in every field.
pub struct CompiledLibrary<'a> {
    /// Human-readable name for error messages ("compiler island", "prelude").
    pub label: &'a str,
    pub bitcode: &'a [u8],
    /// Every definition whose native body this artifact carries.
    pub items: &'a [symbols::CompiledItem],
    /// Which ABI those bodies answer to (`typelisp_abi::BODY_ABI_*`), straight
    /// off the dump that carried them.
    ///
    /// The artifact is the only thing that knows. Bitcode does not say, and
    /// calling a body under the wrong ABI is a wrong answer rather than an
    /// error anyone would see — so this rides along from the file to the call.
    pub body_abi: u8,
    /// Which tag layout those bodies were emitted under
    /// (`typelisp_mem::tagged::LAYOUT_*`), off the same dump. Must equal the
    /// runtime's `LAYOUT`; the install refuses otherwise.
    pub body_layout: u8,
}

/// The shared Rust-only runtime library (`typelisp-rt`, a separate crate —
/// see its doc comment for why) re-exported under its old in-crate path so
/// every existing `crate::compile::runtime::...` reference elsewhere in this
/// crate keeps working unchanged.
/// Which ABI the code this process compiles comes out under.
///
/// It is the *committed island's* answer, not `SOURCE`'s: the island doing the
/// emitting in this process is the one loaded from `compiler_island.typld`,
/// and across an ABI change that dump is a generation behind the source it was
/// built from. So this tracks [`crate::compiler::ISLAND_DUMP_EMITS_ABI`] and
/// moves when the artifact does.
///
/// Everything the running island produces answers to it: the closure
/// constructor `build-make-closure` names, and the ABI recorded on every
/// JIT-compiled function.
pub const EMITTED_BODY_ABI: u8 = crate::compiler::ISLAND_DUMP_EMITS_ABI;

/// Which tag layout the code this process compiles comes out under — the
/// committed island's answer, tracking
/// [`crate::compiler::ISLAND_DUMP_EMITS_LAYOUT`] exactly as
/// [`EMITTED_BODY_ABI`] tracks the ABI.
///
/// When this differs from `typelisp_mem::tagged::LAYOUT` the process is the
/// middle of a layout changeover: what it emits cannot run here. Only the
/// island bootstrap may run in that state (it emits without running), and
/// `driver::compile_scc` / `aot::compile_file` refuse rather than JIT or link
/// a body the runtime cannot decode.
pub const EMITTED_LAYOUT: u8 = crate::compiler::ISLAND_DUMP_EMITS_LAYOUT;

pub use typelisp_rt as runtime;

/// The coroutine-ABI driver, which lives in `typelisp-rt` rather than here:
/// it needs a `Heap` and a function pointer, and nothing from LLVM. Re-exported
/// under the backend path the tests and the island's callers already use.
pub use typelisp_rt::coroutine;

/// Every *user*-defined `defun`/`defmethod`'s own LLVM symbol name (JIT and
/// AOT alike) is this prefix followed by its typelisp name — never the bare
/// name unprefixed. Without it, a user function whose name happens to
/// collide with a libc symbol the LLVM backend itself calls into (`fmod`,
/// the lowering target of the `frem` instruction `f64`'s `mod` compiles to —
/// see `core_bridge::translate_assoc`'s doc comment) would resolve to that
/// libc symbol instead of the user's own compiled body, an infinite-
/// recursion trap discovered compiling a test function literally named
/// `fmod`. `rt_*` runtime shims (`typelisp_rt::rt_car` and friends) are
/// untouched by this — they're never looked up through this prefix, only
/// ever by their own hardcoded name in `compiler.rs`'s SOURCE and
/// `Interp::rt_extern_functions` — so a user function named e.g. `rt_cons`
/// is prefixed like any other and can't collide with the real `rt_cons`
/// either.
///
/// The single source of truth for this name is `core_bridge`'s own
/// `user_symbol_name`/`user_method_symbol_name` — every call/reference site
/// (a `(call ...)`/`(assoc ...)` node's embedded name string,
/// `Interp::compile_scc`'s `declare_external_function`/`externals`
/// wiring, `compile::aot`'s per-`defun` `internal_name`) goes through one of
/// those two, so this prefix only needs to be applied once per definition
/// site — never at a second, easy-to-desync spot.
pub const USER_SYMBOL_PREFIX: &str = "tl_";

use std::sync::{Arc, Mutex, OnceLock};

use inkwell::context::Context;
use inkwell::module::Module;


/// Serializes every LLVM-Context-touching operation. LLVM's C API is not
/// safe to call concurrently against one `Context` from multiple threads
/// (its type-uniquing tables aren't synchronized), so this is held for the
/// duration of any compile even though nothing here spawns compiler threads
/// today — cheap insurance against a known class of non-deterministic crash.
///
/// **Destruction counts as a Context-touching operation**, and it is the one
/// that used to escape: `~Module` walks its functions and unregisters every
/// value name from the Context's own tables. Nothing *calls* it, so it slipped
/// past a rule written for calls — an interpreter going out of scope on one
/// thread would tear a module down while another thread was building or
/// parsing IR, and the crash landed in `llvm::Value::destroyValueName` with no
/// hint that a drop was involved. A module is dropped where its builder holds
/// this lock.
pub static COMPILE_LOCK: Mutex<()> = Mutex::new(());

/// JIT-compiled code whose last owner is gone, waiting until no compiled
/// chain can be inside it.
///
/// A plain `Mutex` of its own, never held together with anything but itself:
/// retiring takes only this one, and draining takes it *inside*
/// `COMPILE_LOCK`, so the two can't be acquired in opposite orders.
static RETIRED_CODE: Mutex<Vec<Arc<orc::JitCode>>> = Mutex::new(Vec::new());

/// Hands a share of `code` over, to be dropped by [`destroy_retired_llvm`]
/// rather than where its owner happens to go out of scope.
///
/// Dropping the last share frees the code and unregisters its unwind
/// information, and that must not happen while anything is still in it: a
/// task parked mid-call in a function since redefined, one a worker thread is
/// stepping, or a panic unwinding through it (unregistering mid-unwind hangs
/// the *next* unwind — see `tests/compiled_unwind_test.rs`). A `CompiledFn`
/// dies wherever the `FnDef` holding it does, which knows none of that.
///
/// The cost is that retired code lives until someone compiles again with no
/// compiled chain standing — the list is drained at the head of every JIT
/// construction, and a process that never compiles again is about to exit
/// anyway. A program that always has a task parked mid-call keeps every body
/// it ever retired.
pub(crate) fn retire_llvm(code: Arc<orc::JitCode>) {
    RETIRED_CODE.lock().unwrap_or_else(|e| e.into_inner()).push(code);
}

/// Drops everything [`retire_llvm`] has collected — unless a compiled chain is
/// standing anywhere in the process, in which case everything waits for a
/// later compilation.
///
/// Which code a chain's frames point into is not recorded, so while any chain
/// stands (`typelisp_rt::coroutine::live_chains`) nothing retired is dropped.
/// Called with [`COMPILE_LOCK`] held, at the head of every JIT construction.
/// The list is emptied first and dropped afterwards, so its `Mutex` is not
/// held while the code is cleared.
fn destroy_retired_llvm() {
    if typelisp_rt::coroutine::live_chains() > 0 {
        return;
    }
    let retired = std::mem::take(&mut *RETIRED_CODE.lock().unwrap_or_else(|e| e.into_inner()));
    drop(retired);
}

/// `Context` holds a raw `LLVMContextRef`, so it isn't `Sync` and can't sit
/// in a `static` directly. Wrapping it asserts that's fine: every access goes
/// through [`llvm_context`], and every LLVM-Context-touching call site is
/// expected to hold [`COMPILE_LOCK`] first, so two threads never actually
/// touch the inner `Context` concurrently.
struct ContextCell(Context);
unsafe impl Sync for ContextCell {}

static LLVM_CONTEXT: OnceLock<ContextCell> = OnceLock::new();

/// The single process-wide LLVM `Context`. Modules/builders/values created
/// from it borrow it for the life of the process (no per-compile teardown),
/// which is what lets the interpreter's LLVM handle registry hold `'static`
/// inkwell types instead of threading a lifetime through every value that
/// could carry one.
/// Verifies `module`, naming the functions that failed.
///
/// `Module::verify`'s own message is the verifier's complaint with no
/// indication of *where* — five "Instruction does not dominate all uses" over
/// a module holding hundreds of functions says nothing about which one to
/// read. Each function is asked separately first, so the names come back with
/// it.
pub(crate) fn verify_module_naming_functions(module: &inkwell::module::Module<'static>, label: &str) -> Result<(), String> {
    let Err(e) = module.verify() else { return Ok(()) };
    let mut bad = Vec::new();
    let mut f = module.get_first_function();
    while let Some(func) = f {
        // Declarations have nothing to verify, and asking anyway is a crash in
        // some LLVM builds.
        if func.count_basic_blocks() > 0 && !func.verify(false) {
            bad.push(func.get_name().to_string_lossy().into_owned());
        }
        f = func.get_next_function();
    }
    if bad.is_empty() {
        return Err(format!("{} failed verification: {}", label, e));
    }
    Err(format!("{} failed verification in {}: {}", label, bad.join(", "), e))
}

pub fn llvm_context() -> &'static Context {
    &LLVM_CONTEXT.get_or_init(|| ContextCell(Context::create())).0
}

/// The fixed C ABI every JIT-compiled function uses, regardless of its
/// typelisp-level arity: an `i64` argument array (and its length) in, one
/// `i64` out — see `registry::llvm_module_def`'s doc comment for why
/// `compiler.rs`'s `compile-function` always builds LLVM functions under
/// this exact signature.
///
/// `extern "C-unwind"`, not `extern "C"`: `typelisp_rt::rt_panic` unwinds, and
/// an `extern "C"` pointer would promise Rust that the callee cannot — a
/// promise broken the moment a `(panic ...)` fires.
pub type CompiledSignature = unsafe extern "C-unwind" fn(*const i64, u32) -> i64;

/// A JIT-compiled function: its entry address, and a share of the code that
/// address is in.
///
/// The share matters twice over: it keeps the code itself, and the unwind
/// information the system unwinder walks when a compiled `(panic ...)`
/// unwinds. Every function of one module holds a share of the same
/// [`orc::JitCode`], so a whole SCC's code lives as long as any member of it.
///
/// The share is given up through [`retire_llvm`] rather than dropped in place
/// — see there.
pub struct CompiledFn {
    /// Held for its lifetime, never called through. `Option` only so [`Drop`]
    /// can move it out; it is `Some` for the whole life of a `CompiledFn`.
    code: Option<Arc<orc::JitCode>>,
    /// This function's own JIT-resolved address — see [`Self::address`].
    addr: usize,
    /// Which ABI that address answers to, carried from the artifact or set by
    /// whatever just compiled it. Nothing about the address itself says.
    body_abi: u8,
}

impl Drop for CompiledFn {
    fn drop(&mut self) {
        if let Some(code) = self.code.take() {
            retire_llvm(code);
        }
    }
}

impl CompiledFn {
    /// JIT-compiles `module` and resolves `fn_name`, one of the functions it
    /// defines. `externals` is `(name, address)` for everything else the
    /// module calls: the `rt_*` shims it declares, and every *other*
    /// already-`compile`d top-level function `fn_name`'s body calls, each
    /// forward-declared, with no body, in `module` under that same name — see
    /// [`crate::eval::interp::Interp::compile_function`]'s doc comment for why
    /// that declaration has to exist *before* the typelisp compiler body ever
    /// runs (`compile-call`'s `get-function` needs to find *something* by that
    /// name). Must be called with [`COMPILE_LOCK`] held — which is also what
    /// makes this the place to drop whatever [`retire_llvm`] has been handed
    /// since the last compile.
    ///
    /// `body_abi` is asked for rather than assumed: nothing about the address
    /// says which ABI it answers to.
    pub fn new(
        module: &Module<'static>,
        fn_name: &str,
        externals: &[(String, usize)],
        body_abi: u8,
    ) -> Result<CompiledFn, String> {
        Ok(Self::new_multi(module, &[fn_name.to_string()], externals, body_abi)?
            .pop()
            .expect("one name asked for, one function back"))
    }

    /// Like [`Self::new`], for every name in `fn_names` — the SCC compile
    /// path, where a mutually recursive group of top-level functions is
    /// emitted into one module (every member forward-declared before any body
    /// is translated — see [`crate::eval::interp::Interp::compile_scc`]'s doc
    /// comment) and linked once. A call from one member to another is resolved
    /// inside the module, the same way self-recursion is; `externals` is only
    /// ever the set of targets *outside* this group.
    pub fn new_multi(
        module: &Module<'static>,
        fn_names: &[String],
        externals: &[(String, usize)],
        body_abi: u8,
    ) -> Result<Vec<CompiledFn>, String> {
        destroy_retired_llvm();
        let code = Arc::new(jit_engine(module, externals)?);
        fn_names
            .iter()
            .map(|fn_name| {
                let addr = code.address(fn_name)?;
                Ok(CompiledFn { code: Some(code.clone()), addr, body_abi })
            })
            .collect()
    }

    /// Runs this function. A compiled `(panic ...)` unwinds out of here
    /// rather than returning, so callers that need to recover must wrap the
    /// call in [`catch_compiled_panic`].
    ///
    pub fn call(&self, args: &[i64]) -> i64 {
        <Self as crate::eval::interp::CompiledBody>::call(self, args)
    }

    /// This function's own JIT-resolved address — what a *later* `compile`d
    /// function's body that calls this one is linked against (see
    /// [`Self::new`]'s `externals` parameter).
    pub fn address(&self) -> usize {
        self.addr
    }
}

/// The evaluator's view of this: an address it can call through, and an
/// owner whose `Drop` retires the code behind it.
///
/// The trait lives in the front end (`eval::interp`) and names only the
/// address, so nothing there has to mention the JIT — see its doc comment.
impl crate::eval::interp::CompiledBody for CompiledFn {
    fn address(&self) -> usize {
        self.addr
    }

    fn body_abi(&self) -> u8 {
        self.body_abi
    }
}


#[cfg(test)]
mod tests {
    use inkwell::AddressSpace;

    use super::{llvm_context, CompiledFn, COMPILE_LOCK};
    use crate::compile::runtime::rt_ping;

    /// Stage 0's JIT-side half of the proof that a `#[no_mangle]` Rust
    /// function from [`crate::compile::runtime`] is callable through the
    /// exact same `externals` list that calling another JIT-compiled typelisp
    /// function by name goes through — `rt_ping`'s real address is just another `usize`
    /// to map, indistinguishable to this machinery from a previously-JIT'd
    /// function's address. See `aot_output_can_call_an_rt_extern_function`
    /// (`aot.rs`) for the AOT-side counterpart.
    #[test]
    fn jit_can_call_an_rt_extern_function() {
        let ctx = llvm_context();
        let _guard = COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("jit_ping_test");

        let ptr_ty = ctx.ptr_type(AddressSpace::default());
        let fn_ty = ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        let rt_ping_decl = module.add_function("rt_ping", fn_ty, None);
        let caller = module.add_function("jit_ping_test", fn_ty, None);

        let builder = ctx.create_builder();
        let entry = ctx.append_basic_block(caller, "entry");
        builder.position_at_end(entry);
        let one_slot = builder.build_alloca(ctx.i64_type(), "one_slot").unwrap();
        builder.build_store(one_slot, ctx.i64_type().const_int(41, false)).unwrap();
        let argc_one = ctx.i32_type().const_int(1, false);
        let call = builder.build_call(rt_ping_decl, &[one_slot.into(), argc_one.into()], "rt_ping_result").unwrap();
        let result = match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            inkwell::values::ValueKind::Instruction(_) => panic!("rt_ping call produced no value"),
        };
        builder.build_return(Some(&result)).unwrap();
        module.verify().expect("module failed verification");

        let externals = vec![("rt_ping".to_string(), rt_ping as *const () as usize)];
        let compiled = CompiledFn::new(&module, "jit_ping_test", &externals, typelisp_abi::BODY_ABI_CLASSIC)
            .expect("CompiledFn::new failed");
        assert_eq!(compiled.call(&[]), 42);
    }

    /// Stage 1's JIT-side proof: registering a `Heap` via
    /// `runtime::set_active_heap` (exactly what `Interp::eval`'s
    /// compiled-call dispatch now does before every call) makes it visible,
    /// through nothing but `rt_heap_live_count`'s real address, to code that
    /// was JIT-compiled with no awareness of which `Heap` it'd end up
    /// running against.
    #[test]
    fn jit_compiled_code_sees_the_heap_registered_by_set_active_heap() {
        use crate::compile::runtime::{rt_heap_live_count, set_active_heap};
        use crate::{Heap, Value};

        let ctx = llvm_context();
        let _guard = COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("jit_heap_test");

        let ptr_ty = ctx.ptr_type(AddressSpace::default());
        let fn_ty = ctx.i64_type().fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        let rt_heap_live_count_decl = module.add_function("rt_heap_live_count", fn_ty, None);
        let caller = module.add_function("jit_heap_test", fn_ty, None);

        let builder = ctx.create_builder();
        let entry = ctx.append_basic_block(caller, "entry");
        builder.position_at_end(entry);
        let null_args = ptr_ty.const_null();
        let argc_zero = ctx.i32_type().const_int(0, false);
        let call = builder
            .build_call(rt_heap_live_count_decl, &[null_args.into(), argc_zero.into()], "live_count")
            .unwrap();
        let result = match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            inkwell::values::ValueKind::Instruction(_) => panic!("rt_heap_live_count call produced no value"),
        };
        builder.build_return(Some(&result)).unwrap();
        module.verify().expect("module failed verification");

        let mut heap = Heap::with_capacity(8);
        heap.cons(Value::Int(1), Value::Empty).expect("cons failed");
        heap.cons(Value::Int(2), Value::Empty).expect("cons failed");
        set_active_heap(&mut heap as *mut Heap);

        let externals = vec![("rt_heap_live_count".to_string(), rt_heap_live_count as *const () as usize)];
        let compiled = CompiledFn::new(&module, "jit_heap_test", &externals, typelisp_abi::BODY_ABI_CLASSIC)
            .expect("CompiledFn::new failed");
        assert_eq!(compiled.call(&[]), 2);
    }

    /// Stage 3's JIT-side proof: `rt_cons`/`rt_car`/`rt_cdr` are callable
    /// from JIT-compiled code through nothing but their real addresses
    /// (the same `externals` mechanism as every test above), and a cons
    /// cell built through `rt_cons` survives a round trip back out through
    /// `rt_car`/`rt_cdr` correctly. `1`/`2`'s tagged form (`n << 3`, tag
    /// `000` = fixnum) is hardcoded here rather than calling into
    /// `runtime`'s own (private, by design) `encode`/`decode` — this is
    /// exactly the bit-twiddling `compiler.rs`'s own future
    /// `compile-construct`/tagging helpers will do via `build-shl`/...,
    /// just written directly against inkwell for this lower-level test.
    #[test]
    fn jit_compiled_code_round_trips_a_cons_through_rt_cons_rt_car_rt_cdr() {
        use crate::compile::runtime::{rt_car, rt_cdr, rt_cons};
        use crate::Heap;

        let ctx = llvm_context();
        let _guard = COMPILE_LOCK.lock().unwrap();
        let module = ctx.create_module("jit_cons_test");

        let ptr_ty = ctx.ptr_type(AddressSpace::default());
        let i64_ty = ctx.i64_type();
        let fn_ty = i64_ty.fn_type(&[ptr_ty.into(), ctx.i32_type().into()], false);
        let rt_cons_decl = module.add_function("rt_cons", fn_ty, None);
        let rt_car_decl = module.add_function("rt_car", fn_ty, None);
        let rt_cdr_decl = module.add_function("rt_cdr", fn_ty, None);
        let caller = module.add_function("jit_cons_test", fn_ty, None);

        let builder = ctx.create_builder();
        let entry = ctx.append_basic_block(caller, "entry");
        builder.position_at_end(entry);

        let tagged_one = i64_ty.const_int(1 << 3, false);
        let tagged_two = i64_ty.const_int(2 << 3, false);
        let cons_args = builder.build_alloca(i64_ty.array_type(2), "cons_args").unwrap();
        let argc_zero32 = ctx.i32_type().const_int(0, false);
        let slot0 = unsafe { builder.build_gep(i64_ty, cons_args, &[argc_zero32], "slot0").unwrap() };
        builder.build_store(slot0, tagged_one).unwrap();
        let slot1 = unsafe { builder.build_gep(i64_ty, cons_args, &[ctx.i32_type().const_int(1, false)], "slot1").unwrap() };
        builder.build_store(slot1, tagged_two).unwrap();
        let argc_two = ctx.i32_type().const_int(2, false);
        let pair = builder.build_call(rt_cons_decl, &[cons_args.into(), argc_two.into()], "pair").unwrap();
        let pair = match pair.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            inkwell::values::ValueKind::Instruction(_) => panic!("rt_cons call produced no value"),
        };

        let one_slot = builder.build_alloca(i64_ty, "one_slot").unwrap();
        builder.build_store(one_slot, pair).unwrap();
        let argc_one = ctx.i32_type().const_int(1, false);
        let car_call = builder.build_call(rt_car_decl, &[one_slot.into(), argc_one.into()], "car_result").unwrap();
        let car_result = match car_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            inkwell::values::ValueKind::Instruction(_) => panic!("rt_car call produced no value"),
        };
        let cdr_call = builder.build_call(rt_cdr_decl, &[one_slot.into(), argc_one.into()], "cdr_result").unwrap();
        let cdr_result = match cdr_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            inkwell::values::ValueKind::Instruction(_) => panic!("rt_cdr call produced no value"),
        };

        let three = i64_ty.const_int(3, false);
        let car_untagged = builder.build_right_shift(car_result, three, true, "car_untagged").unwrap();
        let cdr_untagged = builder.build_right_shift(cdr_result, three, true, "cdr_untagged").unwrap();
        let thousand = i64_ty.const_int(1000, false);
        let combined =
            builder.build_int_add(builder.build_int_mul(car_untagged, thousand, "car_scaled").unwrap(), cdr_untagged, "combined").unwrap();
        builder.build_return(Some(&combined)).unwrap();
        module.verify().expect("module failed verification");

        let mut heap = Heap::with_capacity(8);
        crate::compile::runtime::set_active_heap(&mut heap as *mut Heap);

        let externals = vec![
            ("rt_cons".to_string(), rt_cons as *const () as usize),
            ("rt_car".to_string(), rt_car as *const () as usize),
            ("rt_cdr".to_string(), rt_cdr as *const () as usize),
        ];
        let compiled = CompiledFn::new(&module, "jit_cons_test", &externals, typelisp_abi::BODY_ABI_CLASSIC)
            .expect("CompiledFn::new failed");
        assert_eq!(compiled.call(&[]), 1002);
        // The real `Heap` (not just the tagged `i64`s) actually grew by one
        // cons cell — proof `rt_cons` went through `Heap::cons`, not some
        // shortcut that happened to produce the right bit pattern.
        assert_eq!(heap.live_count(), 1);
    }
}
