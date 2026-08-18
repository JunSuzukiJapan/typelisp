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

pub mod aot;
pub mod bootstrap;
pub mod core_bridge;
pub mod core_freevars;
pub mod driver;
pub mod externs;
pub mod llvm_builtins;

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
        compile_file: aot::compile_file,
    });
}
pub mod prelude_bootstrap;
pub mod symbols;

/// One precompiled bitcode artifact, ready to install into an `Interp` —
/// everything `Interp::install_compiled_library` needs to know about it.
///
/// A struct rather than five positional parameters because the two artifacts
/// (the compiler island and the prelude) differ in every field, and the two
/// *modes* each is installed in — a hash-checked runtime load, and a
/// bootstrap load of the previous generation — differ only in
/// [`Self::expected_hash`]. Naming that difference is what keeps a
/// regenerator from accidentally hard-failing on the staleness it creates on
/// purpose.
pub struct CompiledLibrary<'a> {
    /// Human-readable name for error messages ("compiler island", "prelude").
    pub label: &'a str,
    /// What to tell the user to run when the artifact turns out stale.
    pub regen_script: &'a str,
    pub bitcode: &'a [u8],
    /// Every definition whose native body this artifact carries.
    pub items: &'a [symbols::CompiledItem],
    /// `(embedded-hash global name, the hash the live source produces)`, or
    /// `None` to skip the staleness check — see
    /// `Interp::install_compiled_library` for when each is right.
    pub expected_hash: Option<(&'a str, u64)>,
}

/// The shared Rust-only runtime library (`typelisp-rt`, a separate crate —
/// see its doc comment for why) re-exported under its old in-crate path so
/// every existing `crate::compile::runtime::...` reference elsewhere in this
/// crate keeps working unchanged.
pub use typelisp_rt as runtime;

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

use std::sync::{Mutex, OnceLock};

use inkwell::context::Context;
use inkwell::execution_engine::ExecutionEngine;
use inkwell::module::Module;
use inkwell::OptimizationLevel;


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
/// hint that a drop was involved. Objects this crate owns now go through
/// [`retire_llvm`] instead of being dropped where they happen to fall.
pub static COMPILE_LOCK: Mutex<()> = Mutex::new(());

/// LLVM objects waiting to be destroyed with [`COMPILE_LOCK`] held.
///
/// # Safety
///
/// `Send` is asserted because the value crosses threads *without being
/// touched*: retiring moves it into [`RETIRED_LLVM`] and draining moves it
/// back out, and nothing in between reads it. The only operations that ever
/// run on the inner object are its creation and its destructor, and both
/// happen under `COMPILE_LOCK` — creation because every construction site
/// already holds it, destruction because [`destroy_retired_llvm`] is only
/// called from a context that does.
///
/// That covers the reference counts inside, too, which is the subtler half:
/// an `ExecutionEngine` is an `Rc` share, and a non-atomic refcount is
/// exactly the thing a cross-thread move usually breaks. Here every
/// increment is an `ExecutionEngine::clone` in [`CompiledFn::new_multi`]
/// (lock held) and every decrement is a drop inside `destroy_retired_llvm`
/// (lock held); moving a share into or out of the list changes no count.
struct RetiredLlvm {
    objects: Vec<Box<dyn std::any::Any>>,
}

// SAFETY: see the doc comment above.
unsafe impl Send for RetiredLlvm {}

/// Objects retired from any thread, destroyed by whichever thread next takes
/// [`COMPILE_LOCK`] to compile something.
///
/// A plain `Mutex` of its own, never held together with anything but itself:
/// retiring takes only this one, and draining takes it *inside*
/// `COMPILE_LOCK`, so the two can't be acquired in opposite orders.
static RETIRED_LLVM: Mutex<RetiredLlvm> = Mutex::new(RetiredLlvm { objects: Vec::new() });

/// Hands `obj` over to be destroyed later, under [`COMPILE_LOCK`].
///
/// For LLVM objects whose owner goes out of scope somewhere that cannot take
/// the lock — a `CompiledFn` dropped with the interpreter that held it, a
/// module whose builder returned through `?`. Dropping such an object in
/// place is the race this exists to remove, and locking in its `Drop` is not
/// an option: plenty of call sites legitimately *hold* `COMPILE_LOCK` while
/// an LLVM object of theirs goes out of scope (every `compile_test.rs` case
/// that builds a module under the guard, for one), and `Mutex` is not
/// reentrant, so that would trade a rare crash for a reliable hang.
///
/// The cost is that a retired object lives until someone compiles again —
/// bounded, since the list is drained at the head of every JIT construction,
/// and nil in the shape that matters: a process that never compiles again is
/// about to exit anyway.
pub(crate) fn retire_llvm<T: 'static>(obj: T) {
    RETIRED_LLVM.lock().unwrap_or_else(|e| e.into_inner()).objects.push(Box::new(obj));
}

/// Destroys everything [`retire_llvm`] has collected.
///
/// The caller must hold [`COMPILE_LOCK`] — that is the entire point of the
/// detour. The list is emptied first and dropped afterwards, so the inner
/// `Mutex` is not held while destructors run.
fn destroy_retired_llvm() {
    let objects = std::mem::take(&mut RETIRED_LLVM.lock().unwrap_or_else(|e| e.into_inner()).objects);
    drop(objects);
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

/// A JIT-compiled function: its entry address, and a share of the engine
/// that owns the code at that address.
///
/// Reached by address rather than through
/// [`inkwell::execution_engine::JitFunction`], because inkwell's
/// `UnsafeFunctionPointer` is implemented for `unsafe extern "C" fn` only —
/// `ExecutionEngine::get_function` cannot name [`CompiledSignature`]'s
/// `-unwind` ABI at all, so the address comes from `get_function_address` and
/// the engine has to be held directly.
///
/// What the engine keeps alive matters twice over: the code itself, and the
/// `.eh_frame` registration the system unwinder walks when a compiled
/// `(panic ...)` unwinds. Dropping an engine while a panic is still
/// propagating through frames it owns leaves the unwinder unable to complete
/// the *next* unwind — it hangs rather than fails, which
/// `tests/compiled_unwind_test.rs` demonstrates. `ExecutionEngine` is
/// reference-counted, so cloning one share per function is what keeps a whole
/// SCC's engine alive as long as any member of it.
///
/// The share is given up through [`retire_llvm`] rather than dropped in
/// place: the last one to go takes the whole engine — and the modules of IR
/// inside it — down with it, and that teardown has to happen with
/// [`COMPILE_LOCK`] held. Where a `CompiledFn` dies is not something it can
/// choose; it goes wherever the `FnDef` holding it does, which is usually an
/// interpreter being dropped by a thread doing nothing LLVM-related at all.
pub struct CompiledFn {
    /// Held for its lifetime, never called through — see this struct's doc
    /// comment. `Option` only so [`Drop`] can move it out; it is `Some` for
    /// the whole life of a `CompiledFn`.
    engine: Option<ExecutionEngine<'static>>,
    /// This function's own JIT-resolved address — see [`Self::address`].
    addr: usize,
}

impl Drop for CompiledFn {
    fn drop(&mut self) {
        if let Some(engine) = self.engine.take() {
            retire_llvm(engine);
        }
    }
}

impl CompiledFn {
    /// JIT-compiles `fn_name` out of `module`. `externals` (labels/closures
    /// Stage 3) is `(name, address)` for every *other* already-`compile`d
    /// top-level function `fn_name`'s body calls (`call`): each must
    /// already be forward-declared, with no body, in `module` under that
    /// same name — see [`crate::eval::interp::Interp::compile_function`]'s
    /// doc comment for why that declaration has to exist *before* the
    /// typelisp compiler body ever runs (`compile-call`'s `get-function`
    /// needs to find *something* by that name). Wiring each one's real
    /// address via `add_global_mapping` here, before resolving `fn_name`
    /// itself, makes a call through that declaration jump straight to the
    /// real, already-running JIT code instead of an unresolved symbol. Empty
    /// for self-recursion only or no calls at all — the common case, and
    /// every call before Stage 3. Must be called with [`COMPILE_LOCK`]
    /// held — which is also what makes this the place to destroy whatever
    /// [`retire_llvm`] has been handed since the last compile.
    pub fn new(module: &Module<'static>, fn_name: &str, externals: &[(String, usize)]) -> Result<CompiledFn, String> {
        destroy_retired_llvm();
        let engine = module.create_jit_execution_engine(OptimizationLevel::None).map_err(|e| e.to_string())?;
        for (name, addr) in externals {
            let decl = module
                .get_function(name)
                .ok_or_else(|| format!("internal error: no forward declaration for \"{}\" in this module", name))?;
            engine.add_global_mapping(&decl, *addr);
        }
        let addr = engine.get_function_address(fn_name).map_err(|e| e.to_string())?;
        Ok(CompiledFn { engine: Some(engine), addr })
    }

    /// Like [`Self::new`], but resolves every name in `fn_names` out of one
    /// shared JIT execution engine over `module` — labels/closures Stage 5's
    /// SCC compile path: a mutually recursive group of top-level functions
    /// is emitted into one shared module (every member forward-declared
    /// before any body is translated — see
    /// [`crate::eval::interp::Interp::compile_scc`]'s doc comment), so they
    /// must all be JIT'd together out of the same engine rather than one
    /// throwaway engine per function. A call from one member's body to a
    /// sibling still-being-compiled member needs no `add_global_mapping`
    /// entry — LLVM resolves it directly against the sibling's own
    /// already-emitted definition in this same module, the same way ordinary
    /// self-recursion always has; `externals` is only ever the set of
    /// already-`compile`d targets *outside* this group, exactly like
    /// [`Self::new`]'s own `externals`. `fn_name`s from
    /// [`inkwell::execution_engine::ExecutionEngine::get_function`] each
    /// clone their own reference to the shared engine internally (see that
    /// method's doc comment), so every returned [`CompiledFn`] independently
    /// keeps it alive — dropping some of them early is safe. Must be called
    /// with [`COMPILE_LOCK`] held, and drains the retirement list for the
    /// same reason [`Self::new`] does.
    pub fn new_multi(module: &Module<'static>, fn_names: &[String], externals: &[(String, usize)]) -> Result<Vec<CompiledFn>, String> {
        destroy_retired_llvm();
        let engine = module.create_jit_execution_engine(OptimizationLevel::None).map_err(|e| e.to_string())?;
        for (name, addr) in externals {
            let decl = module
                .get_function(name)
                .ok_or_else(|| format!("internal error: no forward declaration for \"{}\" in this module", name))?;
            engine.add_global_mapping(&decl, *addr);
        }
        fn_names
            .iter()
            .map(|fn_name| {
                let addr = engine.get_function_address(fn_name).map_err(|e| e.to_string())?;
                Ok(CompiledFn { engine: Some(engine.clone()), addr })
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

    /// This function's own JIT-resolved address — used to wire
    /// `add_global_mapping` when a *later* `compile`d function's body calls
    /// this one (labels/closures Stage 3, see [`Self::new`]'s `externals`
    /// parameter).
    pub fn address(&self) -> usize {
        self.addr
    }
}

/// The evaluator's view of this: an address it can call through, and an
/// owner whose `Drop` retires the engine behind it.
///
/// The trait lives in the front end (`eval::interp`) and names only the
/// address, so nothing there has to mention `ExecutionEngine` — see its doc
/// comment.
impl crate::eval::interp::CompiledBody for CompiledFn {
    fn address(&self) -> usize {
        self.addr
    }
}


#[cfg(test)]
mod tests {
    use inkwell::AddressSpace;

    use super::{llvm_context, CompiledFn, COMPILE_LOCK};
    use crate::compile::runtime::rt_ping;

    /// Stage 0's JIT-side half of the proof that a `#[no_mangle]` Rust
    /// function from [`crate::compile::runtime`] is callable through the
    /// exact same `externals`/`add_global_mapping` wiring labels/closures
    /// Stage 3 already built for calling another JIT-compiled typelisp
    /// function by name — `rt_ping`'s real address is just another `usize`
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

        let externals = vec![("rt_ping".to_string(), rt_ping as usize)];
        let compiled = CompiledFn::new(&module, "jit_ping_test", &externals).expect("CompiledFn::new failed");
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

        let externals = vec![("rt_heap_live_count".to_string(), rt_heap_live_count as usize)];
        let compiled = CompiledFn::new(&module, "jit_heap_test", &externals).expect("CompiledFn::new failed");
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

        let tagged_one = i64_ty.const_int(1, false).const_shl(i64_ty.const_int(3, false));
        let tagged_two = i64_ty.const_int(2, false).const_shl(i64_ty.const_int(3, false));
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
            ("rt_cons".to_string(), rt_cons as usize),
            ("rt_car".to_string(), rt_car as usize),
            ("rt_cdr".to_string(), rt_cdr as usize),
        ];
        let compiled = CompiledFn::new(&module, "jit_cons_test", &externals).expect("CompiledFn::new failed");
        assert_eq!(compiled.call(&[]), 1002);
        // The real `Heap` (not just the tagged `i64`s) actually grew by one
        // cons cell — proof `rt_cons` went through `Heap::cons`, not some
        // shortcut that happened to produce the right bit pattern.
        assert_eq!(heap.live_count(), 1);
    }
}
