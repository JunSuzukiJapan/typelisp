//! The LLVM handle registry, and `rt_llvm_call`, the one shim a compiled
//! island reaches the `llvm-*` builtins through.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue};
use typelisp_mem::{wk, SymRef, Value};
use typelisp_mem::symbols::member_symbol;
use crate::eval::interp::{expect_str, expect_struct_box, scope_clone_frames_heap, scope_pop_frame_heap, scope_push_frame_heap, scope_set_raw, EvalError};
use crate::types::{Path, Type};
use super::eval_llvm_builtin_method;

// ---- LLVM handle registry + `rt_llvm_call` (interp-closure removal ------
// ---- Stage 1) -----------------------------------------------------------
//
// Compiled code represents every Rust-native LLVM value (an `llvm-*
// LlvmModule`/`LlvmBuilder`/`LlvmFunction`/`LlvmBasicBlock`/`LlvmValue`)
// object, and a native-repr `Scope<V>`) as an *LLVM handle*: an index into this
// thread-local registry, carried as a plain untraced `i64`
// (`Repr::Handle` — kind `1`, the integer kind). That is
// what makes the self-hosted compiler island's own `defun`s compilable:
// their `llvm-*` method calls lower to a single generic shim,
// [`rt_llvm_call`], which decodes handles back to those objects, dispatches
// into the very same [`eval_llvm_builtin_method`]/native-scope builtins the
// interpreter uses, and encodes the result.
//
// Registry entries are only ever *appended* during a compile session; the
// mark/release pair below lets the outermost compiled-island entry point
// drop everything it accumulated once the session ends (the `Rc`s inside
// the registered objects keep shared structures like a module or a
// scope's frames alive exactly as long as some other owner still needs
// them). Thread-local for the same reason `set_active_heap` is: `cargo
// test` workers each drive their own independent `Heap`/LLVM session.

/// A Rust-native object that has no representation in the value heap, held
/// here so a typelisp value can refer to it by an opaque integer handle.
///
/// These are the compiler's own working objects — an LLVM module, builder,
/// function, block, value, and a `Scope` whose `V` is one of those. They
/// cannot live on the GC heap (an inkwell handle is not a `Value` and has no
/// traceable shape), and they used to be carried as `RtValue::Llvm*`/`Scope`
/// variants instead: a second, Rust-side value universe running alongside the
/// heap one, which is what forced every binding to be routed by static type
/// (`SlotKind`) rather than just being a `Value`.
///
/// Keeping the object here and handing out an `i64` collapses that: at the
/// language level an `llvm-value` is an integer like any other, and the same
/// representation already crosses to compiled code
/// (`Repr::Handle`'s integer kind), so interpreted and
/// compiled tiers now agree by construction rather than by conversion. Type
/// safety is unaffected — `LLVM_HANDLE_TYPES` keeps these distinct from `i32`
/// statically, which is where it was always enforced.
///
/// `typelisp_rt::stream` represents OS streams the same way.
#[derive(Clone)]
pub(crate) enum NativeHandle {
    Module(Rc<RefCell<Module<'static>>>),
    Builder(Rc<RefCell<Builder<'static>>>),
    Function(FunctionValue<'static>),
    BasicBlock(BasicBlock<'static>),
    Value(BasicValueEnum<'static>),
}

/// The registry itself, wrapped only so its teardown can be given the same
/// treatment its explicit releases get.
///
/// A `Module`/`Builder` share sitting here is often the *last* one — the
/// module a compile was driven with is registered before
/// `run_compile_function`'s own mark, so it outlives the release bracket and
/// stays until the thread ends. Destroying it runs `~Module`, which rewrites
/// the shared LLVM Context's value-name tables, so it has to happen under
/// `COMPILE_LOCK` like every other Context-touching operation (see that
/// static's doc comment) — including at thread exit, which is where this
/// `Drop` comes in. A thread normally exits outside the lock and takes it
/// here; a process LLVM ends with `exit` from inside a compile runs this with
/// the lock already held, and then the take nests instead of waiting for
/// itself forever.
///
/// The shares are not handed to `compile::retire_llvm` the way an engine is:
/// these are `Rc`s, and every share of a given module lives on this thread
/// (the registry is thread-local, and so is the compile that made it), so
/// moving one to a list another thread drains would put a non-atomic refcount
/// under two threads. Locking in place keeps the counting single-threaded and
/// still puts the destructor inside the lock.
struct LlvmHandles(Vec<NativeHandle>);

impl Drop for LlvmHandles {
    fn drop(&mut self) {
        let _guard = crate::compile::COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        self.0.clear();
    }
}

// A handle is an index into this vector, which is what lets one cross to
// compiled code as a plain integer with no ownership attached. It grows for
// the life of the thread except where `llvm_handles_mark`/`_release` bracket a
// compile (`driver::run_compile_function`), which is the one place a bounded
// batch of handles is known to be finished with.
thread_local! {
    static LLVM_HANDLES: RefCell<LlvmHandles> = const { RefCell::new(LlvmHandles(Vec::new())) };
}

/// Registers `h` and returns its handle — the `i64` both the interpreter and
/// compiled code carry in place of the Rust-native object.
pub(crate) fn llvm_handle_register(h: NativeHandle) -> i64 {
    LLVM_HANDLES.with(|t| {
        let mut t = t.borrow_mut();
        t.0.push(h);
        (t.0.len() - 1) as i64
    })
}

/// The object behind handle `h`, or `None` for a never-issued (or already
/// released) handle.
pub(crate) fn llvm_handle_get(h: i64) -> Option<NativeHandle> {
    if h < 0 {
        return None;
    }
    LLVM_HANDLES.with(|t| t.borrow().0.get(h as usize).cloned())
}

/// Register `h` as the interpreter-level value standing for it.
fn handle_value(h: NativeHandle) -> Value {
    Value::Int(llvm_handle_register(h))
}

pub(super) fn llvm_module_value(m: Module<'static>) -> Value {
    handle_value(NativeHandle::Module(Rc::new(RefCell::new(m))))
}

pub(crate) fn llvm_module_value_rc(m: Rc<RefCell<Module<'static>>>) -> Value {
    handle_value(NativeHandle::Module(m))
}

pub(super) fn llvm_builder_value(b: Builder<'static>) -> Value {
    handle_value(NativeHandle::Builder(Rc::new(RefCell::new(b))))
}

pub(super) fn llvm_function_value(f: FunctionValue<'static>) -> Value {
    handle_value(NativeHandle::Function(f))
}

pub(super) fn llvm_block_value(b: BasicBlock<'static>) -> Value {
    handle_value(NativeHandle::BasicBlock(b))
}

pub(super) fn llvm_value_value(v: BasicValueEnum<'static>) -> Value {
    handle_value(NativeHandle::Value(v))
}

/// The LLVM module `v` is a handle for, or `None` if it is not one.
///
/// An `llvm-module` is an opaque integer at the value level, so a caller
/// outside this module (a test inspecting generated IR, say) needs this to get
/// at the object behind it.
pub fn llvm_module_of(v: &Value) -> Option<Rc<RefCell<Module<'static>>>> {
    match llvm_handle_get(match v {
        Value::Int(h) => *h,
        _ => return None,
    })? {
        NativeHandle::Module(m) => Some(m),
        _ => None,
    }
}

/// The LLVM value `v` is a handle for — [`llvm_module_of`]'s counterpart.
pub fn llvm_value_of(v: &Value) -> Option<BasicValueEnum<'static>> {
    match llvm_handle_get(match v {
        Value::Int(h) => *h,
        _ => return None,
    })? {
        NativeHandle::Value(x) => Some(x),
        _ => None,
    }
}

/// The handle integer standing for `v` at the compiled boundary.
///
/// An LLVM object already *is* its handle, so this is the identity on it; only
/// a native scope still has to be registered on the way out.
fn handle_of(v: &Value) -> Option<i64> {
    match v {
        Value::Int(h) => Some(*h),
        _ => None,
    }
}

/// The interpreter value standing for handle integer `raw`, the inverse of
/// [`handle_of`].
fn value_of_handle(raw: i64) -> Option<Value> {
    llvm_handle_get(raw).map(|_| Value::Int(raw))
}

/// The current registry length — pass to [`llvm_handles_release`] to drop
/// every handle issued after this point.
///
/// [`crate::compile::driver::run_compile_function`] brackets each compile with a pair, so the
/// transient handles the native compiler registers while emitting one
/// function's IR do not accumulate across many compiles. Everything else
/// issued outside such a bracket lives as long as the thread.
pub(crate) fn llvm_handles_mark() -> usize {
    LLVM_HANDLES.with(|t| t.borrow().0.len())
}

/// Drops every handle issued since the matching [`llvm_handles_mark`].
///
/// The handles come out of the registry first and are destroyed afterwards,
/// under `COMPILE_LOCK`: a released `Builder`/`Module` share is often the last
/// one, and its destructor touches the shared LLVM Context — see
/// [`LlvmHandles`]. The take nests inside the whole-compile guard the caller
/// (`driver::run_compile_function`) runs under; the registry's own `RefCell`
/// borrow ends before the lock is taken, so a destructor that reached back
/// into the registry could not deadlock on that either.
pub(crate) fn llvm_handles_release(mark: usize) {
    let released: Vec<NativeHandle> = LLVM_HANDLES.with(|t| t.borrow_mut().0.split_off(mark));
    let _guard = crate::compile::COMPILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    drop(released);
}

/// The handle integer `v` carries, for the `expect_llvm_*` accessors.
pub(super) fn expect_handle(v: &Value, want: &str) -> Result<NativeHandle, EvalError> {
    let h = match v {
        Value::Int(n) => *n,
        other => return Err(EvalError::Internal(format!("expected an {}, got {:?}", want, other))),
    };
    llvm_handle_get(h).ok_or_else(|| EvalError::Internal(format!("expected an {}, got dangling handle {}", want, h)))
}

/// How `rt_llvm_call` decodes one raw argument word, per the op table.
#[derive(Clone, Copy, Debug)]
enum LlvmArgK {
    /// A registry handle — decode via [`llvm_handle_get`].
    Handle,
    /// A tagged heap `Value::Boxed` at a `StructPayload::Frames` scope.
    ///
    /// Compiled code treats a scope word as opaque: it only ever receives one
    /// from `rt_llvm_call` and hands it straight back, so what the word *is*
    /// is settled entirely here, and the committed island bitcode is
    /// indifferent to the change from registry handle to tagged box.
    Scope,
    /// A tagged heap `Value::Str`.
    Str,
    /// A raw untagged integer.
    Int,
    /// A raw 0/1 word.
    Bool,
}

/// How `rt_llvm_call` encodes the builtin's result.
#[derive(Clone, Copy, Debug)]
enum LlvmRetK {
    /// Register the value, return its handle.
    Handle,
    /// A scope box, tagged — see [`LlvmArgK::Scope`]. Registered as a session
    /// root on the way out: the committed island binds scope words at the
    /// untraced integer kind, so nothing else keeps the box alive while the
    /// compile session runs.
    Scope,
    /// Return `0` (`compile-unit`'s convention).
    Unit,
    /// Raw 0/1.
    Bool,
    /// A small `int`, raw — the way an `int` argument arrives
    /// ([`LlvmArgK::Int`]); the caller's side tags it.
    Int,
    /// A freshly-allocated tagged heap `Value::Str`.
    Str,
    /// `Option<handle>` as a heap `BoxedObj::Enum` (`Some` payload =
    /// `Value::Int(handle)`, decoded through the ordinary integer field
    /// kind by a compiled `match`) — the native scope `get`'s shape.
    OptHandle,
}

/// One dispatchable builtin: `(type_key, method)` plus its marshaling
/// shape, keyed in [`llvm_op_table`] by [`compile::symbols::llvm_op_id`].
struct LlvmOp {
    /// The receiver type's vocabulary symbol: `wk::LLVM_MODULE` ...,
    /// `wk::SCOPE` for the native scope.
    ty: u32,
    /// The receiver type as the op id spells it.
    type_key: &'static str,
    method: SymRef,
    args: Vec<LlvmArgK>,
    ret: LlvmRetK,
}

fn llvm_arg_kind(ty: &Type) -> LlvmArgK {
    if crate::check::repr::is_llvm_handle_ty(ty) {
        LlvmArgK::Handle
    } else if ty.is_int_family() {
        // An `int` parameter arrives raw too: the bridge untags it at the
        // call site (`core_bridge::native_arg`), a bignum having been
        // refused there, so this side never sees a tagged word for one.
        LlvmArgK::Int
    } else {
        match ty {
            Type::Str => LlvmArgK::Str,
            Type::Bool => LlvmArgK::Bool,
            other => panic!("llvm_op_table: parameter type {:?} has no rt_llvm_call marshaling", other),
        }
    }
}

fn llvm_ret_kind(ty: &Type) -> LlvmRetK {
    if crate::check::repr::is_llvm_handle_ty(ty) {
        LlvmRetK::Handle
    } else {
        match ty {
            Type::Unit => LlvmRetK::Unit,
            Type::Bool => LlvmRetK::Bool,
            Type::Int => LlvmRetK::Int,
            Type::Str => LlvmRetK::Str,
            other => panic!("llvm_op_table: return type {:?} has no rt_llvm_call marshaling", other),
        }
    }
}

/// The `rt_llvm_call` dispatch table, keyed by [`compile::symbols::llvm_op_id`]'s
/// stable hash. The `llvm-*` entries are *derived* from the same built-in
/// definitions (`typelisp_front::builtins`) the checker types these methods
/// with — table and signatures cannot drift apart. The six `native-scope`
/// entries are written out by hand because `Scope<V>`'s signatures are generic
/// over `V` (here always an LLVM handle — see
/// `core_bridge::llvm_assoc_key`, which only routes a `Scope<V>` with an
/// LLVM-handle `V` to this table in the first place).
fn llvm_op_table() -> &'static HashMap<i64, LlvmOp> {
    static TABLE: std::sync::OnceLock<HashMap<i64, LlvmOp>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t: HashMap<i64, LlvmOp> = HashMap::new();
        let insert = |t: &mut HashMap<i64, LlvmOp>, ty: u32, type_key: &'static str, method: SymRef, args: Vec<LlvmArgK>, ret: LlvmRetK| {
            let id = crate::compile::symbols::llvm_op_id(type_key, method.name());
            if t.insert(id, LlvmOp { ty, type_key, method, args, ret }).is_some() {
                panic!("llvm_op_table: op id collision on {}", id);
            }
        };
        let builtins = typelisp_front::builtins::Builtins::new();
        for (ty, type_key) in [(wk::LLVM_MODULE, "llvm-module"), (wk::LLVM_FUNCTION, "llvm-function"), (wk::LLVM_BUILDER, "llvm-builder")] {
            for (method, m) in builtins.methods(&crate::Path::builtin(ty)) {
                let args = m.sig.params.iter().map(llvm_arg_kind).collect();
                let ret = llvm_ret_kind(&m.sig.ret);
                insert(&mut t, ty, type_key, method, args, ret);
            }
        }
        use LlvmArgK::{Handle as H, Str as S};
        // The scope receiver is a tagged box word now, not a registry handle
        // (`LlvmArgK::Scope`). Only the op *ids* are baked into the committed
        // island bitcode — this table, and so the marshaling shape, is Rust
        // side and free to change with it.
        const SC: LlvmArgK = LlvmArgK::Scope;
        insert(&mut t, wk::SCOPE, "native-scope", member_symbol("new"), vec![], LlvmRetK::Scope);
        insert(&mut t, wk::SCOPE, "native-scope", member_symbol("clone-frames"), vec![SC], LlvmRetK::Scope);
        insert(&mut t, wk::SCOPE, "native-scope", member_symbol("push-frame"), vec![SC], LlvmRetK::Unit);
        insert(&mut t, wk::SCOPE, "native-scope", member_symbol("pop-frame"), vec![SC], LlvmRetK::Unit);
        insert(&mut t, wk::SCOPE, "native-scope", member_symbol("get"), vec![SC, S], LlvmRetK::OptHandle);
        insert(&mut t, wk::SCOPE, "native-scope", member_symbol("set"), vec![SC, S, H], LlvmRetK::Unit);
        t
    })
}

/// The `rt_*` family's abort-on-invariant-break convention
/// (`typelisp-rt`'s `fatal`), for the main-crate shims.
fn rt_llvm_fatal(msg: &str) -> ! {
    eprintln!("typelisp runtime error: {}", msg);
    std::process::abort();
}

/// `(rt-llvm-call opid arg...)` for compiled code — the generic dispatch
/// shim behind every compiled `llvm-*`/native-`Scope<V>` builtin method
/// call (`compiler.rs`'s `compile-llvm-op`; interp-closure removal Stage
/// 1). `args[0]` is the [`compile::symbols::llvm_op_id`] hash embedded at
/// translate time; the rest are marshaled per the matching
/// [`llvm_op_table`] entry and dispatched into the *exact same*
/// [`eval_llvm_builtin_method`]/native-scope builtins the interpreter
/// itself uses — one implementation, two callers, no drift.
///
/// Argument values are fully materialized into Rust-side objects
/// *before* anything here can allocate on the GC heap, so callers only
/// need their usual kind-driven rooting (a tagged `Str` argument crossing
/// in stays valid until then because nothing between the caller's own
/// allocation and this decode allocates).
///
/// # Safety
///
/// `args` must point to `argc` valid `i64`s; a `Heap` must already be
/// registered on this thread (`set_active_heap`). Errors abort via
/// [`rt_llvm_fatal`], mirroring `typelisp-rt`'s `fatal`.
pub(crate) unsafe extern "C" fn rt_llvm_call(args: *const i64, argc: u32) -> i64 {
    let argv = std::slice::from_raw_parts(args, argc as usize);
    let Some((&opid, raw_args)) = argv.split_first() else {
        rt_llvm_fatal("rt_llvm_call: missing op id");
    };
    let Some(op) = llvm_op_table().get(&opid) else {
        rt_llvm_fatal(&format!("rt_llvm_call: unknown op id {}", opid));
    };
    if raw_args.len() != op.args.len() {
        rt_llvm_fatal(&format!(
            "rt_llvm_call: {}::{} expects {} arguments, got {}",
            op.type_key,
            op.method,
            op.args.len(),
            raw_args.len()
        ));
    }
    let heap = crate::compile::runtime::active_heap();
    let mut vals: Vec<Value> = Vec::with_capacity(raw_args.len());
    for (raw, k) in raw_args.iter().zip(&op.args) {
        vals.push(match k {
            LlvmArgK::Handle => match value_of_handle(*raw) {
                Some(v) => v,
                None => rt_llvm_fatal(&format!(
                    "rt_llvm_call: {}::{}: dangling llvm handle {} (registry holds {}, args so far {:?})",
                    op.type_key,
                    op.method,
                    raw,
                    llvm_handles_mark(),
                    raw_args
                )),
            },
            LlvmArgK::Str => match crate::compile::runtime::decode(*raw) {
                v @ Value::Str(_) => v,
                other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a Str argument, got {:?}", op.type_key, op.method, other)),
            },
            LlvmArgK::Scope => match crate::compile::runtime::decode(*raw) {
                v @ Value::Boxed(_) => v,
                other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a scope box, got {:?}", op.type_key, op.method, other)),
            },
            LlvmArgK::Int => Value::Int(*raw),
            LlvmArgK::Bool => Value::Bool(*raw != 0),
        });
    }
    // The scope `get` returns `Option<V>` — encoded specially, so it's
    // handled before the generic single-value result path.
    //
    // The stored element word goes into the `Some` payload verbatim: for the
    // island's `Scope<llvm-value>`/`Scope<llvm-function>` that is the same
    // `Value::Int(handle)` this produced before scopes moved to the heap, so
    // the option box the committed bitcode unwraps is bit-identical.
    if op.ty == wk::SCOPE && op.method.is(wk::M_GET) {
        let found = match (expect_struct_box(&vals[0]), expect_str(heap, &vals[1])) {
            (Ok(id), Ok(name)) => heap.scope_get(id, &name),
            (Err(e), _) | (_, Err(e)) => rt_llvm_fatal(&format!("rt_llvm_call: native-scope::get: {:?}", e)),
        };
        // `Option<llvm-value>`, interned through the one producer: a type's
        // runtime identity includes its instantiation, and `native-scope::get`
        // is the one builtin here that returns a generic box.
        let ty = typelisp_front::type_key::type_id(
            &crate::types::Type::Named(
                crate::Path::builtin(typelisp_mem::wk::OPTION),
                vec![crate::types::Type::Named(crate::Path::builtin(typelisp_mem::wk::LLVM_VALUE), vec![])],
            ),
        );
        // A niche (`Repr::Niche`): the handle's fixnum word for `Some`, the
        // empty-list immediate for `None` — what `encode` makes of the
        // `Value` `alloc_option` answers, and what the island's `pat-some`
        // reads back.
        let option = heap.alloc_option(ty, found);
        return crate::compile::runtime::encode(option);
    }
    let result: Result<Value, EvalError> = if op.ty == wk::SCOPE {
        match op.method.well_known() {
            wk::M_NEW => Ok(heap.alloc_scope()),
            wk::M_CLONE_FRAMES => scope_clone_frames_heap(heap, &vals),
            wk::M_PUSH_FRAME => scope_push_frame_heap(heap, &vals),
            wk::M_POP_FRAME => scope_pop_frame_heap(heap, &vals),
            // The element type is irrelevant here: the value arrives already
            // in its stored form, so `set` stores the word as given.
            wk::SET => scope_set_raw(heap, &vals),
            _ => rt_llvm_fatal(&format!("rt_llvm_call: unknown native-scope method {}", op.method)),
        }
    } else {
        match eval_llvm_builtin_method(heap, &Path::builtin(op.ty), op.method, &vals) {
            Some(r) => r,
            None => rt_llvm_fatal(&format!("rt_llvm_call: {} has no builtin method {}", op.type_key, op.method)),
        }
    };
    let v = match result {
        Ok(v) => v,
        Err(e) => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: {:?}", op.type_key, op.method, e)),
    };
    match op.ret {
        LlvmRetK::Handle => match handle_of(&v) {
            Some(h) => h,
            None => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a handle result, got {:?}", op.type_key, op.method, v)),
        },
        LlvmRetK::Scope => {
            // Root for the compile session: the committed island holds this
            // word in an untraced local, so nothing else would keep the box
            // alive across the next collection.
            heap.push_session_root(v);
            crate::compile::runtime::encode(v)
        }
        LlvmRetK::Unit => 0,
        LlvmRetK::Bool => match v {
            Value::Bool(b) => i64::from(b),
            other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a Bool result, got {:?}", op.type_key, op.method, other)),
        },
        LlvmRetK::Int => match v {
            Value::Int(n) => n,
            other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected an int result, got {:?}", op.type_key, op.method, other)),
        },
        LlvmRetK::Str => match v {
            // Already a heap string; encoding is the tagged word, no copy.
            sv @ Value::Str(_) => crate::compile::runtime::encode(sv),
            other => rt_llvm_fatal(&format!("rt_llvm_call: {}::{}: expected a Str result, got {:?}", op.type_key, op.method, other)),
        },
        LlvmRetK::OptHandle => rt_llvm_fatal("rt_llvm_call: OptHandle result outside native-scope::get"),
    }
}
