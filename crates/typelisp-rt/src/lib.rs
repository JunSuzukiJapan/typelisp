//! The shared Rust-only runtime library compiled code calls into directly —
//! the destination for builtins that genuinely can't be written in typelisp
//! (direct cons-heap access, raw GC-heap bookkeeping; see the `typelisp`
//! crate's `docs/TODO.md`, "Sexpr表現 + Match/Construct/共有Rustライブラリ
//! 実装計画" section, for the full plan this crate is Stage 0/1 of).
//!
//! A separate crate, depending on nothing but `typelisp-mem` (no
//! `inkwell`/LLVM) — Stage 1 found that linking the *whole* `typelisp` crate
//! (which embeds all of LLVM) into an AOT-compiled executable just to reach
//! these few functions drags in LLVM's entire system-library footprint
//! (`libc++`, zlib, libffi, terminfo, ...) for no benefit. This crate's
//! `staticlib` artifact stays small and dependency-free, which is all
//! `compile-file`'s linker step needs.
//!
//! Every function here is declared under the exact same ABI as a compiled
//! typelisp function (`unsafe extern "C" fn(*const i64, u32) -> i64`,
//! `typelisp::compile::CompiledSignature`) — so the existing cross-function-
//! call wiring (JIT: `add_global_mapping`'s `externals` list in
//! `typelisp::compile::CompiledFn::new`; AOT: ordinary linker symbol
//! resolution against this crate's `staticlib`, see `typelisp::compile::aot`'s
//! `write_executable`) treats a call to one of these exactly like a call to
//! another already-compiled typelisp function — no new call mechanism is
//! needed, only `#[no_mangle]` so the linker sees a plain, unmangled C
//! symbol name.
//!
//! `rt_ping` is Stage 0's deliberately trivial placeholder: it exists only
//! to prove that *some* Rust function defined in this crate is callable
//! from both a JIT-compiled function (via `add_global_mapping`) and an
//! AOT-linked native executable (via the system linker) before any real
//! heap/Sexpr machinery is built on top.

/// Returns `args[0] + 1` if `argc >= 1`, otherwise `0`. No real runtime
/// behavior depends on this — see the module doc comment.
///
/// # Safety
///
/// If `argc >= 1`, `args` must be non-null and point to at least one valid,
/// readable `i64` — exactly what every compiled-function call site already
/// guarantees for its own `args` array.
#[no_mangle]
pub unsafe extern "C" fn rt_ping(args: *const i64, argc: u32) -> i64 {
    if argc >= 1 {
        *args + 1
    } else {
        0
    }
}

// ---- Stage 1: the active `Heap` ---------------------------------------

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use typelisp_mem::Heap;

// The `Heap` every other `rt_*` function in this crate (from Stage 3
// onward) implicitly operates on — see the module doc comment's framing of
// this as CL/Scheme-style "the heap is one implicit, process-wide thing",
// not something compiled code ever holds or passes a handle to itself.
//
// `thread_local!`, not a plain `static`: `cargo test` runs many tests
// concurrently on different OS threads within *one process*, and each test
// that exercises `compile`/`compile-file` owns its own independent `Heap`.
// A single global pointer would let two tests' compiled code race on each
// other's heap. Each real OS thread only ever drives one `Interp`/`Heap`
// at a time (compiling and running compiled code is synchronous, never
// handed off mid-call to another thread), so a thread-local slot gives the
// same "implicit, no handle passed around" ergonomics with no cross-test
// hazard. A single-threaded AOT-compiled executable (its own OS process)
// just has the one thread's slot — behaves identically to a plain global
// there.
thread_local! {
    static ACTIVE_HEAP: Cell<*mut Heap> = const { Cell::new(std::ptr::null_mut()) };
}

/// Registers `heap` as this thread's active `Heap` for any `rt_*` call that
/// follows. The JIT path (`typelisp::eval::Interp::eval`'s compiled-call
/// dispatch) calls this with its own `heap: &mut Heap` immediately before
/// every call into compiled code — cheap enough (one pointer store) to just
/// always do, rather than trying to detect whether the callee might
/// transitively touch the heap. The AOT path has no Rust caller to do this
/// for it, so [`rt_heap_init`] does the equivalent at process startup
/// instead (see its doc comment).
///
/// `pub`, not `pub(crate)`: the `typelisp` crate is a different crate from
/// this one and has to call this directly — there is no "visible to one
/// specific dependent crate" visibility in Rust narrower than plain `pub`.
pub fn set_active_heap(heap: *mut Heap) {
    ACTIVE_HEAP.with(|cell| cell.set(heap));
}

/// Dereferences the current thread's active `Heap`.
///
/// # Safety
///
/// Some prior call on this thread — [`set_active_heap`] (JIT) or
/// [`rt_heap_init`] (AOT) — must have registered a still-valid `Heap`
/// pointer, and no other reference to that same `Heap` may be live (this
/// produces an exclusive `&mut`). Every `rt_*` function below calls this
/// exactly once per invocation and lets the borrow end with the call, so
/// two `rt_*` calls never alias each other's borrow.
unsafe fn active_heap() -> &'static mut Heap {
    let ptr = ACTIVE_HEAP.with(|cell| cell.get());
    debug_assert!(!ptr.is_null(), "rt_* function called with no active Heap registered on this thread");
    &mut *ptr
}

/// [`active_heap`] for `rt_*`-shaped shims living in the *main* crate
/// (`rt_llvm_call`, interp-closure removal Stage 1) — those can't be
/// defined here because they dispatch into `inkwell`-backed builtins this
/// crate deliberately doesn't depend on.
///
/// # Safety
///
/// Identical contract to [`active_heap`]: a still-valid `Heap` must be
/// registered on this thread and no other reference to it may be live for
/// the duration of the returned borrow.
pub unsafe fn shim_active_heap() -> &'static mut Heap {
    active_heap()
}

/// The cons-cell arena size an AOT-compiled executable allocates for itself
/// at startup (see [`rt_heap_init`]) when its `main` doesn't otherwise say
/// — matches the capacity `compile-file` itself already uses for the
/// *compiler's own* (unrelated) `Heap` in `typelisp::compile::aot::compile_file`.
const DEFAULT_AOT_HEAP_CAPACITY: usize = 1 << 16;

/// AOT's counterpart to the JIT path's [`set_active_heap`]: an AOT-compiled
/// executable is its own standalone process with no embedding Rust caller
/// to register a `Heap` for it, so it has to create and register one for
/// itself. `typelisp::compile::aot::build_main_wrapper` emits a call to this
/// as the very first thing the generated `main` does, before calling the
/// file's own `tl_main`.
///
/// Reads the desired capacity from `args[0]` if `argc >= 1`, otherwise uses
/// [`DEFAULT_AOT_HEAP_CAPACITY`]. The allocated `Heap` is deliberately
/// never freed — it must outlive every other compiled function in the
/// process, i.e. live until the OS reclaims everything at process exit, the
/// same trade-off [`rt_push_permanent_sexpr_root`] already accepts for values
/// that outlive their last explicit owner.
///
/// # Safety
///
/// Same as [`rt_ping`]: if `argc >= 1`, `args` must point to at least one
/// valid, readable `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_heap_init(args: *const i64, argc: u32) -> i64 {
    let capacity = if argc >= 1 { *args as usize } else { DEFAULT_AOT_HEAP_CAPACITY };
    let heap = Box::new(Heap::with_capacity(capacity));
    set_active_heap(Box::into_raw(heap));
    0
}

/// Diagnostic-only: the active `Heap`'s live cons-cell count
/// (`Heap::live_count`). Stage 1's proof that the registered `Heap` is
/// genuinely reachable and reflects real state, not a placeholder for any
/// actual language feature — Stage 3 replaces the need for this with real
/// `rt-cons`/`rt-car`/... functions.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread (see [`active_heap`]).
#[no_mangle]
pub unsafe extern "C" fn rt_heap_live_count(_args: *const i64, _argc: u32) -> i64 {
    active_heap().live_count() as i64
}

// ---- Stage 2/3: the tagged `Sexpr` representation ----------------------

use typelisp_mem::{BoxId, ConsRef, PathId, StrId, SymId, Value};

const TAG_BITS: i64 = 3;
const TAG_MASK: i64 = 0b111;

// Stage 2's tag table (`docs/TODO.md`): 8 tags in the low 3 bits. `Nil`/
// `Bool` share one "immediate constant" tag (`TAG_IMMEDIATE`) since `Value`
// has 9 variants but only 8 tag slots — see that doc for the full rationale
// (why this needs no more than 3 bits, the alignment argument for `Cons`
// pointers, etc.). `TAG_BOXED` (formerly `TAG_FLOAT`, reclaimed by the
// `Sexpr`/`RtValue` unification plan — see `BoxedObj`'s doc comment in
// `typelisp-mem`): `Value::Float` used to claim this tag directly and was
// never actually representable in compiled code (`encode`/`decode` both
// `fatal()`ed on it); an `f64` doesn't fit losslessly in the remaining bits
// alongside a tag anyway, so this tag now means "payload is a `BoxId` into
// the heap's boxed-object store" instead of trying to pack an immediate
// float — `Float` is that store's first occupant, with more (structs,
// closures, `HashTable<K,V>`, `Scope<V>`) planned to follow.
const TAG_FIXNUM: i64 = 0b000;
const TAG_CONS: i64 = 0b001;
const TAG_SYMBOL: i64 = 0b010;
const TAG_STR: i64 = 0b011;
const TAG_CHAR: i64 = 0b100;
const TAG_PATH: i64 = 0b101;
const TAG_IMMEDIATE: i64 = 0b110;
const TAG_BOXED: i64 = 0b111;

const IMMEDIATE_NIL: i64 = 0;
const IMMEDIATE_FALSE: i64 = 1;
const IMMEDIATE_TRUE: i64 = 2;

/// Prints `msg` to stderr and aborts the process — the only safe way to
/// fail out of an `rt_*` function. A bare Rust `panic!` would try to unwind
/// back through whatever JIT-compiled or AOT-linked native code called in
/// (no Rust landing pads there), which is undefined behavior across an
/// `extern "C"` boundary; aborting is the documented-safe alternative.
/// Every error case below is a contract violation by `compiler.rs` itself
/// (an internal compiler bug), never a normal/recoverable runtime
/// condition — there is no `Result`-like channel back to compiled code to
/// report it through instead.
fn fatal(msg: &str) -> ! {
    eprintln!("typelisp runtime error: {}", msg);
    std::process::abort();
}

/// Encodes a `Value` into the tagged `i64` representation compiled code
/// uses for a `Sexpr`. `Value::Boxed` needs no allocation here — unlike a
/// hypothetical unboxed `Float` payload, a `BoxId` is already just a small
/// integer index, exactly like `Symbol`/`Str`/`Path`; the caller must have
/// already allocated the box (via e.g. `Heap::alloc_float`) the same way a
/// `Value::Cons` must already be a live heap cell before reaching this
/// function.
///
/// `pub`, not `pub(crate)`: Stage 5's `Expr::Call` dispatch
/// (`typelisp::eval::Interp::eval`) needs this same encoding from the
/// `typelisp` crate, to marshal a `Sexpr`-typed argument/return value
/// across the typelisp-call-syntax boundary into a compiled function — the
/// same tagging scheme every `rt_*` function below already uses, so
/// re-deriving it on the other side of the crate boundary would just be
/// duplicated, easy-to-desync logic.
pub fn encode(v: Value) -> i64 {
    match v {
        Value::Int(n) => (n << TAG_BITS) | TAG_FIXNUM,
        Value::Cons(c) => (c.addr() as i64) | TAG_CONS,
        Value::Symbol(id) => ((id.as_u32() as i64) << TAG_BITS) | TAG_SYMBOL,
        Value::Str(id) => ((id.as_u32() as i64) << TAG_BITS) | TAG_STR,
        Value::Char(c) => ((c as i64) << TAG_BITS) | TAG_CHAR,
        Value::Path(id) => ((id.as_u32() as i64) << TAG_BITS) | TAG_PATH,
        Value::Empty => (IMMEDIATE_NIL << TAG_BITS) | TAG_IMMEDIATE,
        Value::Bool(false) => (IMMEDIATE_FALSE << TAG_BITS) | TAG_IMMEDIATE,
        Value::Bool(true) => (IMMEDIATE_TRUE << TAG_BITS) | TAG_IMMEDIATE,
        Value::Boxed(id) => ((id.as_u32() as i64) << TAG_BITS) | TAG_BOXED,
    }
}

/// The inverse of [`encode`]. `pub` for the same cross-crate reason — see
/// [`encode`]'s doc comment.
pub fn decode(tagged: i64) -> Value {
    match tagged & TAG_MASK {
        TAG_FIXNUM => Value::Int(tagged >> TAG_BITS),
        TAG_CONS => Value::Cons(unsafe { ConsRef::from_addr((tagged & !TAG_MASK) as usize) }),
        TAG_SYMBOL => Value::Symbol(SymId::from_u32((tagged >> TAG_BITS) as u32)),
        TAG_STR => Value::Str(StrId::from_u32((tagged >> TAG_BITS) as u32)),
        TAG_CHAR => {
            let scalar = (tagged >> TAG_BITS) as u32;
            Value::Char(char::from_u32(scalar).unwrap_or_else(|| fatal("decode: invalid char scalar value")))
        }
        TAG_PATH => Value::Path(PathId::from_u32((tagged >> TAG_BITS) as u32)),
        TAG_IMMEDIATE => match tagged >> TAG_BITS {
            IMMEDIATE_NIL => Value::Empty,
            IMMEDIATE_FALSE => Value::Bool(false),
            IMMEDIATE_TRUE => Value::Bool(true),
            other => fatal(&format!("decode: unknown immediate tag payload {}", other)),
        },
        TAG_BOXED => Value::Boxed(BoxId::from_u32((tagged >> TAG_BITS) as u32)),
        _ => unreachable!("a 3-bit mask is always one of the 8 arms above"),
    }
}

/// `(cons car cdr)` for compiled code.
///
/// Like the interpreter's own `cons` builtin, this may run a GC
/// (`Heap::cons` does so internally when its free list is empty) — but
/// unlike the interpreter, nothing here pushes any *other* live `Sexpr`
/// value the calling compiled frame still holds onto [`active_heap`]'s
/// root set first. That's Stage 4's job (`docs/TODO.md`); until it lands, a
/// GC triggered by this call could reclaim a cons cell a caller still
/// needs.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_cons(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_cons: expected 2 arguments");
    }
    let car = decode(*args);
    let cdr = decode(*args.add(1));
    match active_heap().cons(car, cdr) {
        Ok(v) => encode(v),
        Err(_) => fatal("rt_cons: heap exhausted, no cons cell could be reclaimed"),
    }
}

/// `(car c)` for compiled code. Fatal (not a recoverable error) if `c`
/// isn't a cons — the checker is responsible for guaranteeing that never
/// happens, same as the interpreter's own `car` builtin treats it as an
/// internal-error-class `Panic`.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_car(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_car: expected 1 argument");
    }
    match active_heap().car(decode(*args)) {
        Ok(v) => encode(v),
        Err(_) => fatal("rt_car: argument is not a cons"),
    }
}

/// `(cdr c)` for compiled code — see [`rt_car`]'s doc comment.
///
/// # Safety
///
/// Same as [`rt_car`].
#[no_mangle]
pub unsafe extern "C" fn rt_cdr(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_cdr: expected 1 argument");
    }
    match active_heap().cdr(decode(*args)) {
        Ok(v) => encode(v),
        Err(_) => fatal("rt_cdr: argument is not a cons"),
    }
}

/// `(rplaca c val)` for compiled code. Returns the compiled representation
/// of `Unit` (the literal `0` `compile-unit` already uses — see
/// `compiler.rs`), not a re-encoded `Sexpr` value: `set-car`'s return type
/// is the language-level `Unit`, unrelated to `Sexpr`'s own tag space.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_set_car(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_set_car: expected 2 arguments");
    }
    let c = decode(*args);
    let val = decode(*args.add(1));
    match active_heap().set_car(c, val) {
        Ok(()) => 0,
        Err(_) => fatal("rt_set_car: first argument is not a cons"),
    }
}

/// `(rplacd c val)` for compiled code — see [`rt_set_car`]'s doc comment.
///
/// # Safety
///
/// Same as [`rt_set_car`].
#[no_mangle]
pub unsafe extern "C" fn rt_set_cdr(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_set_cdr: expected 2 arguments");
    }
    let c = decode(*args);
    let val = decode(*args.add(1));
    match active_heap().set_cdr(c, val) {
        Ok(()) => 0,
        Err(_) => fatal("rt_set_cdr: first argument is not a cons"),
    }
}

// ---- sexpr-* island accessors (closure unification Stage 8) -------------
//
// The rest of the `sexpr-*` island layer beyond `car`/`cdr`/`cons`: the tag
// predicates (`sexpr-consp`/`sexpr-null`/`sexpr-atom`/`sexpr-symp`) and the
// typed payload extractors (`sexpr-int`/`sexpr-bool`/`sexpr-char`/
// `sexpr-str`/`sexpr-sym-name`; `sexpr-float` reuses [`rt_float_value`],
// whose bits-in-`i64` result is exactly the compiled `f64` convention).
// Before Stage 8 these builtins had no compiled lowering at all, so any
// user function walking a `Sexpr` list (every `&rest` consumer, above all)
// was uncompilable. Each mirrors its interpreter builtin
// (`Interp::eval_builtin`'s matching arm) exactly, including the
// panic-on-tag-mismatch contract of the extractors. `compiler.rs`'s
// `compile-call` rename table maps the `sexpr-*` names here.

/// `(sexpr-consp x)`: `1` if the tagged value is a cons cell, else `0` —
/// the compiled `bool` convention.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_consp(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_consp: expected 1 argument");
    }
    i64::from(matches!(decode(*args), Value::Cons(_)))
}

/// `(sexpr-null x)`: `1` if the tagged value is the empty list `()`.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C" fn rt_null(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_null: expected 1 argument");
    }
    i64::from(matches!(decode(*args), Value::Empty))
}

/// `(sexpr-atom x)`: `1` if the tagged value is *not* a cons cell — the
/// exact negation of [`rt_consp`], mirroring the interpreter's `!v.is_cons()`.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C" fn rt_atom(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_atom: expected 1 argument");
    }
    i64::from(!matches!(decode(*args), Value::Cons(_)))
}

/// `(sexpr-symp x)`: `1` if the tagged value is an interned symbol.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C" fn rt_symp(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_symp: expected 1 argument");
    }
    i64::from(matches!(decode(*args), Value::Symbol(_)))
}

/// `(sexpr-int x)`: the raw `i64` payload of an `Int` node. Fatal on any
/// other tag — the same panic contract the interpreter's `sexpr-int` has.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C" fn rt_sexpr_int(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_int: expected 1 argument");
    }
    match decode(*args) {
        Value::Int(n) => n,
        _ => fatal("sexpr-int: expected an Int Sexpr node"),
    }
}

/// `(sexpr-bool x)`: the `Bool` node's payload as compiled `0`/`1`.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C" fn rt_sexpr_bool(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_bool: expected 1 argument");
    }
    match decode(*args) {
        Value::Bool(b) => i64::from(b),
        _ => fatal("sexpr-bool: expected a Bool Sexpr node"),
    }
}

/// `(sexpr-char x)`: the `Char` node's Unicode scalar value as a raw `i64`
/// — the compiled `char` convention (`compile-char`'s own widened payload).
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C" fn rt_sexpr_char(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_char: expected 1 argument");
    }
    match decode(*args) {
        Value::Char(c) => c as i64,
        _ => fatal("sexpr-char: expected a Char Sexpr node"),
    }
}

/// `(sexpr-str x)`: the `Str` node itself, unchanged — a compiled `str`
/// value *is* the tagged `Value::Str` word (`Type::Str`'s passthrough kind),
/// so the extraction is an identity plus the tag check the interpreter's
/// arm also performs.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C" fn rt_sexpr_str(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_str: expected 1 argument");
    }
    match decode(*args) {
        Value::Str(_) => *args,
        _ => fatal("sexpr-str: expected a Str Sexpr node"),
    }
}

/// `(sexpr-sym-name x)`: a fresh `Str` holding the symbol's name — the
/// fused `(Sym v)`-bind + `symbol->string` read the interpreter's
/// `sexpr-sym-name` performs. Allocates (like [`rt_str_append`]); a `Heap`
/// must be registered.
///
/// # Safety
///
/// Same as [`rt_consp`], plus a registered `Heap` (this allocates).
#[no_mangle]
pub unsafe extern "C" fn rt_sym_name(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sym_name: expected 1 argument");
    }
    match decode(*args) {
        Value::Symbol(id) => {
            let heap = active_heap();
            let name = heap.symbol_name(id).to_string();
            encode(heap.alloc_string(name))
        }
        _ => fatal("sexpr-sym-name: expected a Sym Sexpr node"),
    }
}

// ---- Sexpr/RtValue unification, Stage 0: boxed objects (Float) ---------
//
// `Value::Boxed` (`TAG_BOXED`) is the tagged representation for the heap's
// general boxed-object store (`BoxedObj`, in `typelisp-mem`) — the
// mechanism the `Sexpr`/`RtValue` unification plan uses to bring
// struct/closure/`HashTable<K,V>`/`Scope<V>` values into `Sexpr` uniformly,
// instead of leaving them in a separate `Rc`-managed `RtValue` universe.
// `Float` is the first, deliberately trivial occupant (an `f64` doesn't fit
// losslessly alongside a 3-bit tag, so `Sexpr::Float` was never actually
// representable in compiled code before this). Unlike `rt_cons`'s `car`/
// `cdr` (already-tagged `Sexpr` values), `rt_float_new`'s argument is a raw
// `f64` bit pattern (`f64::to_bits`), not a tagged `Sexpr` — there is no
// existing `Sexpr` value to decode, this constructs a brand new one, the
// same way `compile-construct-sexpr`'s other constructors take their raw
// field payloads (an `Int` literal's bits, a `Char`'s scalar value, ...)
// rather than already-tagged `Sexpr`s.

/// Allocates a boxed `Sexpr` float from a raw `f64` bit pattern.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// (the `f64`'s `to_bits()` reinterpreted as `i64`); a `Heap` must already
/// be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_float_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_float_new: expected 1 argument");
    }
    let f = f64::from_bits(*args as u64);
    encode(active_heap().alloc_float(f))
}

/// Reads the `f64` bit pattern out of a boxed `Sexpr` float — `args[0]` is a
/// tagged `Sexpr` value (as [`rt_car`] etc. take), unlike `rt_float_new`'s
/// raw payload. Fatal if it isn't actually a boxed float — the checker is
/// responsible for guaranteeing that never happens, same convention as
/// [`rt_car`] on a non-cons.
///
/// # Safety
///
/// Same as [`rt_float_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_float_value(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_float_value: expected 1 argument");
    }
    match decode(*args) {
        // Positively `is_float`, not "any box": every other `BoxedObj` kind
        // would make `float_value` *panic*, and a panic out of this
        // `nounwind` shim aborts the process instead of reporting anything —
        // so the doc'd fatal has to be an explicit test.
        Value::Boxed(id) if active_heap().is_float(id) => {
            active_heap().float_value(id).to_bits() as i64
        }
        _ => fatal("rt_float_value: argument is not a boxed float"),
    }
}

/// Discriminates the numeric boxed `Sexpr` kinds that share `TAG_BOXED`'s
/// one tag: `1` for a boxed float, `2` for a bignum, `3` for a ratio, `0`
/// for everything else — *including* non-boxed values, so it's total over
/// every tagged word and `compile-sexpr-tag-test` can call it without a
/// prior tag check (a `kind == 1` result already implies `TAG_BOXED`).
/// `args[0]` is a tagged `Sexpr` value, like [`rt_float_value`]'s.
///
/// # Safety
///
/// Same as [`rt_float_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_box_kind(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_box_kind: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) => {
            let heap = active_heap();
            if heap.is_float(id) {
                1
            } else if heap.is_bignum(id) {
                2
            } else if heap.is_ratio(id) {
                3
            } else {
                0
            }
        }
        _ => 0,
    }
}

// ---- bignum/ratio compiled representation -------------------------------
//
// `bignum`/`ratio` (`num_bigint::BigInt`/`num_rational::BigRational`) are
// `Sign`+`Vec<u32>`-shaped Rust structs with no fixed-width FFI-safe layout —
// unlike `f64`, there is no "native register" form to fall back to, so a
// `Type::Bignum`/`Type::Ratio` value's compiled representation is *always* a
// tagged `TAG_BOXED` `i64` pointing at a `BoxedObj::Bignum`/`Ratio`, exactly
// the same convention `Type::Str` already uses (never bare bits the way
// `f64` is). `rt_box_kind` above already discriminates them (`2`/`3`); the
// functions below are the construction/arithmetic/conversion primitives that
// were still missing (`docs/dev/TODO.md`'s "bignum/ratioにcompiled表現を与える"
// follow-up).
//
// None of these need extra GC-root bookkeeping beyond what the caller
// already does for any tagged argument it passes in: allocating a new
// `BoxedObj` (`box_slots`) never triggers the cons-heap's mark-sweep (that
// only fires on cons-arena exhaustion), the same assumption `rt_struct_new`/
// `rt_float_new` already rely on above.
//
// Division by zero is checked explicitly (`Sign::NoSign`) before calling
// into `BigInt`/`BigRational`'s `Div`/`Rem` operators, which otherwise raise
// a genuine Rust `panic!` — unwinding that across this `extern "C"` boundary
// is undefined behavior (see `fatal`'s doc comment), unlike an `i64` divide
// by zero, which traps at the machine level instead.

use num_bigint::{BigInt, BigUint, Sign};
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{FromPrimitive, ToPrimitive};

/// Decodes `args[idx]` as a boxed bignum and clones its `BigInt` out —
/// shared preamble for every `rt_bignum_*`/`rt_ratio_from_bignums` function
/// below.
///
/// # Safety
///
/// `args` must be valid for at least `idx + 1` `i64`s, the one at `idx`
/// decoding to a `Value::Boxed` bignum; a `Heap` must already be registered
/// on this thread.
unsafe fn bignum_arg(args: *const i64, idx: isize, who: &str) -> BigInt {
    match decode(*args.offset(idx)) {
        Value::Boxed(id) => active_heap().bignum_value(id).clone(),
        _ => fatal(&format!("{who}: argument is not a boxed bignum")),
    }
}

/// The two-bignum-operand preamble every `rt_bignum_*` binop/comparison
/// shares.
///
/// # Safety
///
/// Same as [`bignum_arg`], for both `args[0]` and `args[1]`.
unsafe fn bignum_pair(args: *const i64, argc: u32, who: &str) -> (BigInt, BigInt) {
    if argc < 2 {
        fatal(&format!("{who}: expected 2 arguments"));
    }
    (bignum_arg(args, 0, who), bignum_arg(args, 1, who))
}

/// Decodes `args[idx]` as a boxed ratio and clones its `BigRational` out —
/// the `ratio` counterpart of [`bignum_arg`].
///
/// # Safety
///
/// Same as [`bignum_arg`].
unsafe fn ratio_arg(args: *const i64, idx: isize, who: &str) -> BigRational {
    match decode(*args.offset(idx)) {
        Value::Boxed(id) => active_heap().ratio_value(id).clone(),
        _ => fatal(&format!("{who}: argument is not a boxed ratio")),
    }
}

/// The two-ratio-operand preamble every `rt_ratio_*` binop/comparison
/// shares — the `ratio` counterpart of [`bignum_pair`].
///
/// # Safety
///
/// Same as [`bignum_pair`].
unsafe fn ratio_pair(args: *const i64, argc: u32, who: &str) -> (BigRational, BigRational) {
    if argc < 2 {
        fatal(&format!("{who}: expected 2 arguments"));
    }
    (ratio_arg(args, 0, who), ratio_arg(args, 1, who))
}

/// Allocates a boxed bignum from a compile-time-embedded literal:
/// `args[0]` is the sign (`-1`/`0`/`1`), `args[1..]` are the magnitude's
/// base-2^32 digits, least-significant first (`BigInt::to_u32_digits`'s own
/// shape, reversed at construction time by `compiler.rs`'s
/// `compile-bignum-literal`) — the same "argc-many raw payload scalars"
/// convention [`rt_str_new`] uses for a literal's characters.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least `argc` valid
/// `i64`s; a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_bignum_new: expected at least 1 argument (the sign)");
    }
    let sign = match *args {
        -1 => Sign::Minus,
        0 => Sign::NoSign,
        1 => Sign::Plus,
        other => fatal(&format!("rt_bignum_new: invalid sign {other}")),
    };
    let mut digits = Vec::with_capacity(argc as usize - 1);
    for i in 1..argc as isize {
        digits.push(*args.offset(i) as u32);
    }
    let n = BigInt::from_biguint(sign, BigUint::new(digits));
    encode(active_heap().alloc_bignum(n))
}

/// Builds a boxed ratio from two already-boxed bignums (`args[0]`=numerator,
/// `args[1]`=denominator) — `BigRational::new` reduces to lowest terms and
/// normalizes the sign, the same invariant every other `ratio` construction
/// path (the reader, `bignum->ratio`, ...) already relies on. Used both for
/// a ratio literal (`compiler.rs`'s `compile-ratio-literal`, over two nested
/// `compile-bignum-literal` calls) and `bignum->ratio`.
///
/// # Safety
///
/// `argc` must be `>= 2`, both decoding to boxed bignums; a `Heap` must
/// already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_from_bignums(args: *const i64, argc: u32) -> i64 {
    let (numer, denom) = bignum_pair(args, argc, "rt_ratio_from_bignums");
    if denom.sign() == Sign::NoSign {
        fatal("rt_ratio_from_bignums: zero denominator");
    }
    encode(active_heap().alloc_ratio(BigRational::new(numer, denom)))
}

/// `bignum::+` for compiled code.
///
/// # Safety
///
/// Same as [`bignum_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_add(args: *const i64, argc: u32) -> i64 {
    let (a, b) = bignum_pair(args, argc, "rt_bignum_add");
    encode(active_heap().alloc_bignum(a + b))
}

/// `bignum::-` for compiled code.
///
/// # Safety
///
/// Same as [`bignum_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_sub(args: *const i64, argc: u32) -> i64 {
    let (a, b) = bignum_pair(args, argc, "rt_bignum_sub");
    encode(active_heap().alloc_bignum(a - b))
}

/// `bignum::*` for compiled code.
///
/// # Safety
///
/// Same as [`bignum_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_mul(args: *const i64, argc: u32) -> i64 {
    let (a, b) = bignum_pair(args, argc, "rt_bignum_mul");
    encode(active_heap().alloc_bignum(a * b))
}

/// `bignum::/` for compiled code — truncates toward zero (CL `truncate`),
/// like `int_assoc`'s own `/`. Fatal on a zero divisor (see this section's
/// module doc comment for why a bare Rust panic isn't safe here).
///
/// # Safety
///
/// Same as [`bignum_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_div(args: *const i64, argc: u32) -> i64 {
    let (a, b) = bignum_pair(args, argc, "rt_bignum_div");
    if b.sign() == Sign::NoSign {
        fatal("rt_bignum_div: division by zero");
    }
    encode(active_heap().alloc_bignum(a / b))
}

/// `bignum::mod` for compiled code — floored remainder (CL `mod`; result
/// takes the sign of the divisor), matching the interpreter's
/// `eval_bignum_builtin` and `int_assoc`'s own floored `mod`. Fatal on a zero
/// divisor.
///
/// # Safety
///
/// Same as [`bignum_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_mod(args: *const i64, argc: u32) -> i64 {
    let (a, b) = bignum_pair(args, argc, "rt_bignum_mod");
    if b.sign() == Sign::NoSign {
        fatal("rt_bignum_mod: division by zero");
    }
    encode(active_heap().alloc_bignum(a.mod_floor(&b)))
}

/// Three-way comparison (`-1`/`0`/`1`) for compiled code — the single
/// primitive `compiler.rs`'s `compile-assoc` derives all six bignum
/// comparison operators from (`<`/`<=`/`>`/`>=`/`=`/`/=`, plus the
/// `eq`/`eql`/`equal`/`equalp` aliases for `=`), the same "one primitive,
/// several derived comparisons" shape [`rt_str_lt`] establishes for strings.
///
/// # Safety
///
/// Same as [`bignum_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_cmp(args: *const i64, argc: u32) -> i64 {
    let (a, b) = bignum_pair(args, argc, "rt_bignum_cmp");
    match a.cmp(&b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// `bignum->int` for compiled code — narrowing to `i32`, fatal if the value
/// doesn't fit (same precedent as `int_assoc`'s `int->char`).
///
/// # Safety
///
/// `argc` must be `>= 1`, `args[0]` a boxed bignum; a `Heap` must be
/// registered.
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_to_int(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_bignum_to_int: expected 1 argument");
    }
    let n = bignum_arg(args, 0, "rt_bignum_to_int");
    match n.to_i32() {
        Some(v) => v as i64,
        None => fatal("rt_bignum_to_int: value does not fit in i32"),
    }
}

/// `(rt-bignum-fits-i32 n)` — raw `0`/`1`: does `n` fit in an `i32`?
/// `try-bignum->int`'s (`Option<i32>`) first half — `compiler.rs`'s
/// `compile-assoc` only calls [`rt_bignum_to_int_raw`] after this confirms
/// `1`, the same two-call `Option`-building shape `rt_hashtable_contains`/
/// `_get_raw` already establish.
///
/// # Safety
///
/// Same as [`rt_bignum_to_int`].
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_fits_i32(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_bignum_fits_i32: expected 1 argument");
    }
    let n = bignum_arg(args, 0, "rt_bignum_fits_i32");
    i64::from(n.to_i32().is_some())
}

/// `try-bignum->int`'s second half — only ever called after
/// [`rt_bignum_fits_i32`] confirmed `1`; fatal if it turns out not to fit
/// (an internal-invariant trap, the same convention
/// [`rt_hashtable_get_raw`] uses for its own "caller already checked" case).
///
/// # Safety
///
/// Same as [`rt_bignum_to_int`].
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_to_int_raw(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_bignum_to_int_raw: expected 1 argument");
    }
    let n = bignum_arg(args, 0, "rt_bignum_to_int_raw");
    match n.to_i32() {
        Some(v) => v as i64,
        None => fatal("rt_bignum_to_int_raw: called without a prior rt_bignum_fits_i32 check"),
    }
}

/// `bignum->float` for compiled code — widening, rounded (`BigInt::to_f64`
/// saturates to +/-infinity rather than failing, so this is total).
///
/// # Safety
///
/// Same as [`rt_bignum_to_int`].
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_to_float(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_bignum_to_float: expected 1 argument");
    }
    let n = bignum_arg(args, 0, "rt_bignum_to_float");
    let f = match n.to_f64() {
        Some(f) => f,
        None => fatal("rt_bignum_to_float: conversion failed"),
    };
    f.to_bits() as i64
}

/// `bignum->ratio` for compiled code — widening and always exact
/// (`BigRational::from_integer`, denominator `1`).
///
/// # Safety
///
/// Same as [`rt_bignum_to_int`].
#[no_mangle]
pub unsafe extern "C" fn rt_bignum_to_ratio(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_bignum_to_ratio: expected 1 argument");
    }
    let n = bignum_arg(args, 0, "rt_bignum_to_ratio");
    encode(active_heap().alloc_ratio(BigRational::from_integer(n)))
}

/// `int->bignum` for compiled code — always exact widening. `args[0]` is
/// already a plain (untagged) `i64`, the native fixed-width integer's own
/// compiled representation (whatever its original width, already widened to
/// `i64` by the same native int-widening every other cross-width int
/// operation here uses).
///
/// # Safety
///
/// `argc` must be `>= 1`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_int_to_bignum(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_int_to_bignum: expected 1 argument");
    }
    encode(active_heap().alloc_bignum(BigInt::from(*args)))
}

/// `int->ratio` for compiled code — always exact widening, denominator `1`.
/// A direct primitive rather than routing through [`rt_int_to_bignum`] +
/// [`rt_ratio_from_bignums`], since a native-int receiver's compiled
/// representation (a plain `i64`) never needs the intermediate bignum boxed
/// at all here.
///
/// # Safety
///
/// Same as [`rt_int_to_bignum`].
#[no_mangle]
pub unsafe extern "C" fn rt_int_to_ratio(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_int_to_ratio: expected 1 argument");
    }
    encode(active_heap().alloc_ratio(BigRational::from_integer(BigInt::from(*args))))
}

/// `float->bignum` for compiled code — truncating toward zero
/// (`BigInt::from_f64`, the multi-precision analogue of `f64 as i64`, exact
/// for any finite value unlike a saturating native cast). Fatal on a
/// non-finite input (NaN/infinity), matching the interpreter's own
/// `float_to_bignum`.
///
/// # Safety
///
/// `argc` must be `>= 1`, `args[0]` an `f64`'s raw bit pattern; a `Heap` must
/// be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_float_to_bignum(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_float_to_bignum: expected 1 argument");
    }
    let f = f64::from_bits(*args as u64);
    match BigInt::from_f64(f) {
        Some(n) => encode(active_heap().alloc_bignum(n)),
        None => fatal("rt_float_to_bignum: value is not finite"),
    }
}

/// `float->ratio` for compiled code — widening and exact: every finite
/// `f64` is itself an exact dyadic rational (`BigRational::from_float`, CL's
/// `rational` rather than the lossy-round-trip `rationalize`). Fatal on a
/// non-finite input.
///
/// # Safety
///
/// Same as [`rt_float_to_bignum`].
#[no_mangle]
pub unsafe extern "C" fn rt_float_to_ratio(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_float_to_ratio: expected 1 argument");
    }
    let f = f64::from_bits(*args as u64);
    match BigRational::from_float(f) {
        Some(r) => encode(active_heap().alloc_ratio(r)),
        None => fatal("rt_float_to_ratio: value is not finite"),
    }
}

/// `ratio::+` for compiled code.
///
/// # Safety
///
/// Same as [`ratio_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_add(args: *const i64, argc: u32) -> i64 {
    let (a, b) = ratio_pair(args, argc, "rt_ratio_add");
    encode(active_heap().alloc_ratio(a + b))
}

/// `ratio::-` for compiled code.
///
/// # Safety
///
/// Same as [`ratio_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_sub(args: *const i64, argc: u32) -> i64 {
    let (a, b) = ratio_pair(args, argc, "rt_ratio_sub");
    encode(active_heap().alloc_ratio(a - b))
}

/// `ratio::*` for compiled code.
///
/// # Safety
///
/// Same as [`ratio_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_mul(args: *const i64, argc: u32) -> i64 {
    let (a, b) = ratio_pair(args, argc, "rt_ratio_mul");
    encode(active_heap().alloc_ratio(a * b))
}

/// `ratio::/` for compiled code. Fatal on a zero divisor (checked via the
/// divisor's numerator sign — a reduced ratio is zero iff its numerator is,
/// its denominator always being a positive nonzero invariant).
///
/// # Safety
///
/// Same as [`ratio_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_div(args: *const i64, argc: u32) -> i64 {
    let (a, b) = ratio_pair(args, argc, "rt_ratio_div");
    if b.numer().sign() == Sign::NoSign {
        fatal("rt_ratio_div: division by zero");
    }
    encode(active_heap().alloc_ratio(a / b))
}

/// Three-way comparison for compiled code — the `ratio` counterpart of
/// [`rt_bignum_cmp`], same derivation shape for all six comparisons +
/// `eq`/`eql`/`equal`/`equalp`.
///
/// # Safety
///
/// Same as [`ratio_pair`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_cmp(args: *const i64, argc: u32) -> i64 {
    let (a, b) = ratio_pair(args, argc, "rt_ratio_cmp");
    match a.cmp(&b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// `ratio->bignum` for compiled code — truncates toward zero
/// (`Ratio::to_integer`).
///
/// # Safety
///
/// `argc` must be `>= 1`, `args[0]` a boxed ratio; a `Heap` must be
/// registered.
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_to_bignum(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ratio_to_bignum: expected 1 argument");
    }
    let r = ratio_arg(args, 0, "rt_ratio_to_bignum");
    encode(active_heap().alloc_bignum(r.to_integer()))
}

/// `ratio->float` for compiled code — widening, rounded.
///
/// # Safety
///
/// Same as [`rt_ratio_to_bignum`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_to_float(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ratio_to_float: expected 1 argument");
    }
    let r = ratio_arg(args, 0, "rt_ratio_to_float");
    let f = match r.to_f64() {
        Some(f) => f,
        None => fatal("rt_ratio_to_float: conversion failed"),
    };
    f.to_bits() as i64
}

/// `numerator` for compiled code — a fresh boxed bignum copy of the
/// (already-reduced) numerator; `ratio` itself is never mutated.
///
/// # Safety
///
/// Same as [`rt_ratio_to_bignum`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_numerator(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ratio_numerator: expected 1 argument");
    }
    let r = ratio_arg(args, 0, "rt_ratio_numerator");
    encode(active_heap().alloc_bignum(r.numer().clone()))
}

/// `denominator` for compiled code — the `numerator` counterpart.
///
/// # Safety
///
/// Same as [`rt_ratio_to_bignum`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_denominator(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ratio_denominator: expected 1 argument");
    }
    let r = ratio_arg(args, 0, "rt_ratio_denominator");
    encode(active_heap().alloc_bignum(r.denom().clone()))
}

// ---- Sexpr/RtValue unification, Stage 1: boxed objects (Struct) --------
//
// `BoxedObj::Struct` (`typelisp-mem`) is the shared runtime shape behind a
// `defstruct` instance, `Vector<T>`, and `cons-cell<K,V>` alike — see its
// doc comment. This mem/rt-layer plumbing is deliberately unwired from
// `compiler.rs`/the interpreter for now (that's Stage 2/3 of the
// unification plan, per `docs/TODO.md`): these three functions exist so the
// representation itself can be exercised and tested in isolation first.

/// `(rt-struct-new type-name field0 field1 ...)` for compiled code —
/// allocates a boxed `Sexpr` struct. `args[0]` is a tagged `Sexpr` `Str`
/// (the struct's type name, read once here rather than kept as a live
/// `Sexpr` reference — `Heap::alloc_struct` copies it out into an owned
/// `String`, like every other `BoxedObj` payload); `args[1..argc]` are the
/// field values, already-tagged `Sexpr`s copied into the new struct's field
/// vector unchanged (this function doesn't interpret them, same as
/// [`rt_cons`] doesn't interpret its `car`/`cdr`).
///
/// # Safety
///
/// `argc` must be `>= 1`, `args` must point to at least `argc` valid `i64`s,
/// and `args[0]` must decode to a `Value::Str`; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_struct_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_struct_new: expected at least 1 argument (the type name)");
    }
    let type_name = match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        _ => fatal("rt_struct_new: first argument is not a Str"),
    };
    let mut fields = Vec::with_capacity(argc as usize - 1);
    for i in 1..argc as isize {
        fields.push(decode(*args.offset(i)));
    }
    encode(active_heap().alloc_struct(type_name, fields))
}

/// `(rt-struct-field-get s idx)` for compiled code — the `idx`-th field of
/// boxed struct `args[0]` (a tagged `Sexpr`), where `args[1]` is a *raw*
/// (untagged) `i64` index, matching [`rt_str_ref`]'s convention for its own
/// raw index argument. Fatal if `args[0]` isn't a boxed struct or `idx` is
/// out of range — the checker (a `defstruct` field access) or the builtin
/// method itself (`Vector<T>`'s own bounds check) is responsible for
/// guaranteeing that never happens once this is wired up, the same
/// convention every other `rt_*` bounds violation here follows.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// the first decoding to a `Value::Boxed` struct; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_struct_field_get(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_struct_field_get: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_field_get: first argument is not a boxed Sexpr"),
    };
    let idx = *args.add(1);
    if idx < 0 {
        fatal("rt_struct_field_get: negative field index");
    }
    encode(active_heap().struct_field(id, idx as usize))
}

// ---- enum values (Option/Result/defenum) — the heap-unified shape ------
//
// `BoxedObj::Enum` (`typelisp-mem`) is the shared runtime shape behind
// every enum value: `Option<T>`/`Result<T,E>`/a user `defenum` instance —
// see its doc comment for what it replaces (the interpreter's Rust-side
// `RtValue::Data` and compiled code's raw leaked `malloc` box). These three
// mirror `rt_struct_new`/`rt_struct_field_get`'s shapes; a separate family
// rather than `rt_struct_*` reuse so no caller ever has to know about a
// variant-tag slot offset, and so `rt_struct_field_get`'s "is a Struct"
// invariant stays intact.

/// `(rt-data-new type-name variant field0 field1 ...)` for compiled code —
/// allocates a boxed enum value. `args[0]` is a tagged `Sexpr` `Str` (the
/// enum's type name, copied out like [`rt_struct_new`]'s), `args[1]` is the
/// *raw* (untagged) variant index, `args[2..argc]` are the variant's field
/// values, already-tagged `Sexpr`s stored unchanged.
///
/// # Safety
///
/// `argc` must be `>= 2`, `args` must point to at least `argc` valid
/// `i64`s, and `args[0]` must decode to a `Value::Str`; a `Heap` must
/// already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_data_new(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_data_new: expected at least 2 arguments (type name, variant)");
    }
    let type_name = match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        _ => fatal("rt_data_new: first argument is not a Str"),
    };
    let variant = *args.add(1);
    if variant < 0 {
        fatal("rt_data_new: negative variant index");
    }
    let mut fields = Vec::with_capacity(argc as usize - 2);
    for i in 2..argc as isize {
        fields.push(decode(*args.offset(i)));
    }
    encode(active_heap().alloc_enum(type_name, variant as usize, fields))
}

/// `(rt-data-variant v)` for compiled code — the variant index of boxed
/// enum value `args[0]` (a tagged `Sexpr`), returned as a *raw* (untagged)
/// `i64`, ready for a `match` arm's tag comparison. Fatal if `args[0]`
/// isn't a boxed enum — the checker's type discipline is what guarantees
/// only enum-typed scrutinees reach a tag test. Allocates nothing, so
/// triggers no GC.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// decoding to a `Value::Boxed` enum; a `Heap` must already be registered
/// on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_data_variant(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_data_variant: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_enum(id) => id,
        _ => fatal("rt_data_variant: argument is not a boxed enum value"),
    };
    active_heap().enum_variant(id) as i64
}

/// `(rt-data-field v idx)` for compiled code — the `idx`-th field of boxed
/// enum value `args[0]` (a tagged `Sexpr`), where `args[1]` is a *raw*
/// (untagged) index like [`rt_struct_field_get`]'s. The result is the
/// stored tagged `Sexpr`; per-kind untagging is the caller's job
/// (`compile-sexpr-field`), exactly as for a struct field. Fatal if
/// `args[0]` isn't a boxed enum or `idx` is out of range. No `set`
/// counterpart exists: enum values are immutable.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// the first decoding to a `Value::Boxed` enum; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_data_field(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_data_field: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_enum(id) => id,
        _ => fatal("rt_data_field: first argument is not a boxed enum value"),
    };
    let idx = *args.add(1);
    if idx < 0 {
        fatal("rt_data_field: negative field index");
    }
    encode(active_heap().enum_field(id, idx as usize))
}

/// `(rt-sexpr-instance-test v type-name variant)` for compiled code's
/// Sexpr-downcast pattern guard (`(point x y)`/`(color::red)`/`(the T p)`
/// against a `Sexpr` scrutinee — `compiler.rs`'s `compile-sexpr-instance-
/// test`, `core_bridge::translate_pattern`'s `downcast`/`pat-typetest`
/// encoding). Tests whether tagged `Sexpr` value `args[0]` is a boxed
/// struct or enum whose own `type_name` matches the `Str` `args[1]`, and —
/// for an enum — whose variant also matches the *raw* `args[2]` (`-1` skips
/// the variant check, for a struct downcast or a `(the T p)` whole-enum
/// bind, where any variant of `T` matches).
///
/// Unlike every other `rt_*` shim here, a mismatch is never fatal: a
/// heterogeneous `Sexpr`'s runtime shape is only discoverable here — that's
/// the entire reason this function exists, rather than the checker ruling
/// out the mismatch the way it does for every ordinary (non-downcast)
/// pattern. Returns a raw (untagged) `i64` boolean, matching
/// [`rt_str_lt`]/every other `rt_*` predicate's convention (never a tagged
/// `Sexpr::Bool`) — `compile-sexpr-instance-test` wraps it in its own
/// `build-icmp-eq` against `1`, exactly like [`rt_box_kind`]'s callers do.
///
/// # Safety
///
/// `argc` must be `>= 3`, `args` must point to at least 3 valid `i64`s, and
/// `args[1]` must decode to a `Value::Str`; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_sexpr_instance_test(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_sexpr_instance_test: expected 3 arguments");
    }
    let v = decode(*args);
    let type_name = match decode(*args.add(1)) {
        Value::Str(id) => active_heap().string(id).to_string(),
        _ => fatal("rt_sexpr_instance_test: second argument is not a Str"),
    };
    let variant = *args.add(2);
    let id = match v {
        Value::Boxed(id) => id,
        _ => return 0,
    };
    let heap = active_heap();
    let matches = if heap.is_struct(id) {
        heap.struct_type_name(id) == type_name
    } else if heap.is_enum(id) {
        heap.enum_type_name(id) == type_name && (variant < 0 || heap.enum_variant(id) as i64 == variant)
    } else {
        false
    };
    matches as i64
}

// ---- compiled closures & binding cells — the heap-unified function value --
//
// `BoxedObj::CompiledClosure` (`typelisp-mem`) is the GC-heap flip of
// compiled code's raw `malloc`'d, reference-counted `ClosureBox` — see its
// doc comment for the env/`sexpr_mask` slot convention these four expose.
// `BoxedObj::Cell` (the interpreter's own shared binding cell) gets its
// first compiled-code face here too (`rt_cell_*`): a capture that must obey
// the language's shared-cell `setf` semantics is a cell *reference* in the
// closure env, and both worlds mutate the very same heap object.

/// `(rt-closure-new fn-ptr sexpr-mask slot0 slot1 ...)` for compiled code —
/// allocates a boxed compiled closure. `args[0]` is the *raw* native entry
/// point (a `compiled_fn_type_with_env`-ABI function pointer), `args[1]` is
/// the *raw* capture mask (bit `i` set ⇒ `args[2+i]` is a tagged `Sexpr`,
/// stored decoded; clear ⇒ raw native bits, stored verbatim behind an
/// immediate `Value::Int` — see `BoxedObj::CompiledClosure`), and
/// `args[2..argc]` are the captured slots. At most 64 slots — the same
/// limit the replaced `ClosureBox` fn-mask had. Allocation never triggers a
/// collection (the box store grows on demand), so the slot values may be
/// un-rooted at the moment of the call.
///
/// # Safety
///
/// `argc` must be `>= 2`, `args` must point to at least `argc` valid
/// `i64`s, masked slots must hold valid tagged values, and `args[0]` must
/// be a valid function pointer (opaque here — only stored); a `Heap` must
/// already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_closure_new(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_closure_new: expected at least 2 arguments (fn ptr, sexpr mask)");
    }
    let env_len = argc as usize - 2;
    if env_len > 64 {
        fatal("rt_closure_new: more than 64 captured slots");
    }
    let fn_ptr = *args as usize;
    let sexpr_mask = *args.add(1) as u64;
    let mut env = Vec::with_capacity(env_len);
    for i in 0..env_len {
        let raw = *args.add(2 + i);
        if sexpr_mask & (1 << i) != 0 {
            env.push(decode(raw));
        } else {
            env.push(Value::Int(raw));
        }
    }
    encode(active_heap().alloc_compiled_closure(fn_ptr, env, sexpr_mask))
}

/// `(rt-closure-fnptr clo)` for compiled code — the *raw* native entry
/// point of boxed compiled closure `args[0]` (a tagged `Sexpr`), ready for
/// an indirect call. Fatal if `args[0]` isn't a boxed compiled closure —
/// the checker's type discipline (`Type::Fn`) is what guarantees only
/// function values reach an apply site. Allocates nothing, so triggers no
/// GC.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// decoding to a `Value::Boxed` compiled closure; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_closure_fnptr(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_closure_fnptr: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_compiled_closure(id) => id,
        _ => fatal("rt_closure_fnptr: argument is not a boxed compiled closure"),
    };
    active_heap().compiled_closure_fnptr(id) as i64
}

/// `(rt-closure-env-len clo)` for compiled code — the number of captured
/// slots boxed compiled closure `args[0]` carries, as a *raw* `i64` (the
/// `env_len` argument an indirect `compiled_fn_type_with_env` call passes
/// on). Fatal like [`rt_closure_fnptr`]. Allocates nothing.
///
/// # Safety
///
/// Same contract as [`rt_closure_fnptr`].
#[no_mangle]
pub unsafe extern "C" fn rt_closure_env_len(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_closure_env_len: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_compiled_closure(id) => id,
        _ => fatal("rt_closure_env_len: argument is not a boxed compiled closure"),
    };
    active_heap().compiled_closure_env_len(id) as i64
}

/// `(rt-closure-env-get clo idx)` for compiled code — the `idx`-th captured
/// slot of boxed compiled closure `args[0]`, where `args[1]` is a *raw*
/// index. Hands back the exact `i64` word the slot was created with: a
/// masked slot re-encodes its stored tagged value, an unmasked slot unwraps
/// the raw native bits it smuggled through `Value::Int` (see
/// [`rt_closure_new`]). Fatal if `args[0]` isn't a boxed compiled closure
/// or `idx` is out of range. Allocates nothing.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// the first decoding to a `Value::Boxed` compiled closure; a `Heap` must
/// already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_closure_env_get(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_closure_env_get: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_compiled_closure(id) => id,
        _ => fatal("rt_closure_env_get: first argument is not a boxed compiled closure"),
    };
    let idx = *args.add(1);
    if idx < 0 {
        fatal("rt_closure_env_get: negative slot index");
    }
    let heap = active_heap();
    let v = heap.compiled_closure_env_get(id, idx as usize);
    if heap.compiled_closure_mask(id) & (1 << idx) != 0 {
        encode(v)
    } else {
        match v {
            Value::Int(raw) => raw,
            other => fatal(&format!("rt_closure_env_get: unmasked slot holds a non-raw value {:?}", other)),
        }
    }
}

/// How [`rt_apply_any`] re-enters the tree-walking interpreter: the closure
/// as a tagged word, then the very same `(args_ptr, argc)` the compiled
/// callee would have received.
///
/// A function pointer rather than a direct call because this crate depends
/// on `typelisp-mem` alone — the interpreter lives in the `typelisp` crate,
/// which depends on *this* one, so the edge can only run at runtime.
/// `typelisp::eval::Interp` installs it ([`set_apply_interpreted`]) at the
/// same moment it registers the active heap; an AOT-compiled executable has
/// no interpreter to install and leaves it unset.
pub type ApplyInterpretedFn = unsafe extern "C" fn(closure: i64, args: *const i64, argc: u32) -> i64;

thread_local! {
    static APPLY_INTERPRETED: Cell<Option<ApplyInterpretedFn>> = const { Cell::new(None) };
}

/// Installs (or, with `None`, clears) this thread's interpreter re-entry
/// hook — see [`ApplyInterpretedFn`]. Thread-local for exactly the reason
/// [`set_active_heap`] is: one `Interp` per thread, many per process under
/// `cargo test`.
pub fn set_apply_interpreted(f: Option<ApplyInterpretedFn>) {
    APPLY_INTERPRETED.with(|cell| cell.set(f));
}

/// `(rt-apply-any closure args-ptr argc)` — compiled code's one way to call
/// a function *value*, whatever kind of function it turns out to hold.
/// `args[0]` is the callee as a tagged word, `args[1]` the address of the
/// caller's own argument array, and `args[2]` how many arguments it holds;
/// the result is the callee's return word. `compiler.rs`'s
/// `compile-apply-indirect` reaches this through the `build-closure-apply`
/// builtin, which emits the single call.
///
/// Compiled code cannot know which kind it has: a `Type::Fn` value is a
/// `BoxedObj::CompiledClosure` when its `lambda` was JIT/AOT compiled and a
/// `BoxedObj::Closure` when it was not, and since the JIT became an
/// optimisation rather than a requirement (the cons-cell interpreter's
/// Phase 2) both reach the same apply site. What `build-closure-apply` used
/// to emit — `rt_closure_fnptr` + an `rt_closure_env_get` copy loop + an
/// indirect call — handled only the first and `fatal`'d on the second,
/// aborting the process from a place no `Result` could catch. The dispatch
/// belongs where the value is, so it is here.
///
/// The compiled case is the old emitted sequence, done in Rust: read the
/// entry point, re-encode the captured environment (the exact per-slot rule
/// [`rt_closure_env_get`] applies), and call through the one fixed
/// `compiled_fn_type_with_env` ABI every closure-boxed function shares. The
/// interpreted case hands off to [`set_apply_interpreted`]'s hook, which
/// decodes the argument words by the closure's own declared parameter
/// representations, evaluates the body, and encodes the result back.
///
/// # Safety
///
/// `args` must point to 3 valid `i64`s, `args[1]` to `args[2]` valid `i64`s
/// that stay valid for the call, and a `Heap` must be registered on this
/// thread. Every heap-backed word in the callee's argument array must
/// already be rooted by the caller (`compile-call-args`' `push-sexpr-root`
/// for a `kind = 2` argument does this): an interpreted body allocates, and
/// an unrooted argument would be collected under it.
#[no_mangle]
pub unsafe extern "C" fn rt_apply_any(args: *const i64, argc: u32) -> i64 {
    if argc != 3 {
        fatal("rt_apply_any: expected 3 arguments (closure, argument array, argument count)");
    }
    let closure = *args;
    let callee_args = *args.add(1) as usize as *const i64;
    let callee_argc = *args.add(2) as u32;
    let id = match decode(closure) {
        Value::Boxed(id) => id,
        other => fatal(&format!("rt_apply_any: callee is not a function value: {:?}", other)),
    };
    let heap = active_heap();
    if !heap.is_compiled_closure(id) {
        return match APPLY_INTERPRETED.with(|cell| cell.get()) {
            Some(hook) => hook(closure, callee_args, callee_argc),
            // AOT: there is no interpreter in the process at all. Nothing
            // *should* produce an interpreted closure there either — an
            // AOT-compiled program is compiled through — so this is an
            // internal-invariant break rather than a limitation to work
            // around.
            None => fatal("rt_apply_any: the callee is not a compiled closure and no interpreter is registered on this thread"),
        };
    }
    let env_len = heap.compiled_closure_env_len(id);
    let mask = heap.compiled_closure_mask(id);
    let env: Vec<i64> = (0..env_len)
        .map(|i| {
            let v = heap.compiled_closure_env_get(id, i);
            if mask & (1 << i) != 0 {
                encode(v)
            } else {
                match v {
                    Value::Int(raw) => raw,
                    other => fatal(&format!("rt_apply_any: unmasked closure slot {} holds a non-raw value {:?}", i, other)),
                }
            }
        })
        .collect();
    let fn_ptr = heap.compiled_closure_fnptr(id);
    // SAFETY: `rt_closure_new` is the only producer of a
    // `BoxedObj::CompiledClosure`, and `build-make-closure` only ever hands
    // it an LLVM function compiled under `compiled_fn_type_with_env`'s exact
    // signature.
    let f: unsafe extern "C" fn(*const i64, u32, *const i64, u32) -> i64 = std::mem::transmute(fn_ptr);
    f(callee_args, callee_argc, env.as_ptr(), env.len() as u32)
}

/// `(rt-cell-new v)` for compiled code — allocates a shared binding cell
/// (`BoxedObj::Cell`) holding tagged `Sexpr` `args[0]`, returning the
/// cell's own tagged reference. Unlike the interpreter's `Heap::alloc_cell`
/// there is no owning-`Rc` liveness registration — the cell lives by
/// ordinary reachability (a root, a closure env slot, ...) like any other
/// box. Allocation never triggers a collection, so `args[0]` may be
/// un-rooted at the moment of the call — but the caller must make the
/// returned cell reachable before anything can trigger one.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// holding a valid tagged value; a `Heap` must already be registered on
/// this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_cell_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_cell_new: expected 1 argument");
    }
    let v = decode(*args);
    encode(active_heap().alloc_cell_unregistered(v))
}

/// `(rt-cell-get c)` for compiled code — the current contents of binding
/// cell `args[0]` (a tagged `Sexpr` reference to a `BoxedObj::Cell`), as a
/// tagged `Sexpr`; per-kind untagging is the caller's job
/// (`compile-sexpr-field` conventions), exactly as for a struct field.
/// Fatal if `args[0]` isn't a boxed cell. Allocates nothing.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// decoding to a `Value::Boxed` cell; a `Heap` must already be registered
/// on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_cell_get(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_cell_get: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_cell(id) => id,
        _ => fatal("rt_cell_get: argument is not a boxed cell"),
    };
    encode(active_heap().cell_get(id))
}

/// `(rt-cell-set! c v)` for compiled code — overwrites binding cell
/// `args[0]`'s contents in place with tagged `Sexpr` `args[1]`; `setf`'s
/// primitive for a shared (captured) binding, visible to every holder of
/// the cell — interpreter bindings included, since both worlds share the
/// very same `BoxedObj::Cell`. Returns the compiled representation of
/// `Unit` (`0`), the same convention [`rt_struct_field_set`] uses. Fatal if
/// `args[0]` isn't a boxed cell. Allocates nothing.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// the first decoding to a `Value::Boxed` cell, the second holding a valid
/// tagged value; a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_cell_set(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_cell_set: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) if active_heap().is_cell(id) => id,
        _ => fatal("rt_cell_set: first argument is not a boxed cell"),
    };
    let v = decode(*args.add(1));
    active_heap().cell_set(id, v);
    0
}

/// `(rt-struct-field-set! s idx val)` for compiled code — overwrites the
/// `idx`-th field of boxed struct `args[0]` in place with `args[2]` (a
/// tagged `Sexpr`); `args[1]` is a raw index, like [`rt_struct_field_get`].
/// Returns the compiled representation of `Unit` (`0`), the same convention
/// [`rt_set_car`]/[`rt_set_cdr`] use for their own in-place mutation.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s,
/// the first decoding to a `Value::Boxed` struct; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_struct_field_set(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_struct_field_set: expected 3 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_field_set: first argument is not a boxed Sexpr"),
    };
    let idx = *args.add(1);
    if idx < 0 {
        fatal("rt_struct_field_set: negative field index");
    }
    let val = decode(*args.add(2));
    active_heap().struct_set_field(id, idx as usize, val);
    0
}

/// `(rt-struct-field-count s)` for compiled code — the number of fields boxed
/// struct `args[0]` (a tagged `Sexpr`) holds, returned as a *raw* (untagged)
/// `i64` (like `rt_str_length`'s own raw count result, and the raw index
/// [`rt_struct_field_get`] consumes). `Vector<T>::len`'s primitive: a growable
/// boxed struct's field count is its element count. Fatal if `args[0]` isn't a
/// boxed struct. Allocates nothing, so triggers no GC.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// decoding to a `Value::Boxed` struct; a `Heap` must already be registered
/// on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_struct_field_count(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_struct_field_count: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_field_count: first argument is not a boxed Sexpr"),
    };
    active_heap().struct_field_count(id) as i64
}

/// `(rt-struct-push-field! s val)` for compiled code — appends `args[1]` (a
/// tagged `Sexpr`) as a new field of boxed struct `args[0]`, growing its field
/// count by one. `Vector<T>::push`'s primitive. Returns the compiled
/// representation of `Unit` (`0`), the same convention [`rt_struct_field_set`]
/// uses for its own in-place mutation. Fatal if `args[0]` isn't a boxed
/// struct. Only pushes an already-decoded `Value` onto the struct's own field
/// `Vec` (a Rust `Vec::push`, not a cons-heap allocation), so triggers no GC —
/// the caller need not keep `val` rooted across this call.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s, the
/// first decoding to a `Value::Boxed` struct; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_struct_push_field(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_struct_push_field: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_push_field: first argument is not a boxed Sexpr"),
    };
    let val = decode(*args.add(1));
    active_heap().struct_push_field(id, val);
    0
}

/// `(rt-struct-pop-field! s)` for compiled code — removes and returns the
/// last field of boxed struct `args[0]` (a tagged `Sexpr`), shrinking its
/// field count by one. `Vector<T>::pop`'s primitive, the inverse of
/// [`rt_struct_push_field`]. Only ever called after the compiled caller's
/// own `rt_struct_field_count` check confirmed at least one field present
/// (`compiler.rs`'s `compile-vector-op` "pop" branch) — mirrors
/// [`rt_hashtable_remove_raw`]'s "caller already checked
/// `rt_hashtable_contains`" precondition, since `pop`, like `remove`,
/// returns `Option<T>` (`None` on empty is a legitimate outcome the checked
/// branch builds directly, never by calling this). Fatal if `args[0]` isn't
/// a boxed struct, or if it turns out empty anyway (an internal-invariant
/// trap, the same convention [`rt_hashtable_get_raw`]'s "key not present"
/// fatal follows).
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// decoding to a `Value::Boxed` struct; a `Heap` must already be registered
/// on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_struct_pop_field(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_struct_pop_field: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_pop_field: first argument is not a boxed Sexpr"),
    };
    match active_heap().struct_pop_field(id) {
        Some(v) => encode(v),
        None => fatal("rt_struct_pop_field: vector is empty (caller must check rt_struct_field_count first)"),
    }
}

// ---- Iter-compile plan, Stage C: `HashTable<K,V>` primitives ------------
//
// A `HashTable<K,V>` is a `BoxedObj::Struct` with a `StructPayload::Map`
// payload (unlike a `Vector<T>`/`defstruct`'s `StructPayload::Fields`); the
// mem layer owns the key hashing/interning (`Heap::hashtable_set` takes the
// already-*tagged* key/value `Value`s and computes the `MemHashKey` itself),
// so these are thin adapters, the same shape as the `rt_struct_*` family
// above. The `Option`-returning lookups (`get`/`remove`) are deliberately
// *not* here: a compiled `Option` is a `malloc`'d sum-ADT box, and bridging a
// runtime map lookup into that representation is a separate problem from
// iteration (which needs only build/populate/enumerate) — see
// `docs/dev/iter-compile-plan.md`.

/// `(rt-hashtable-new)` for compiled code — an empty `HashTable<K,V>`
/// (`HashTable::new`). Ignores its arguments (the method takes none).
///
/// # Safety
///
/// A `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_new(_args: *const i64, _argc: u32) -> i64 {
    encode(active_heap().alloc_hashtable())
}

/// `(rt-hashtable-set ht key val)` for compiled code — inserts/overwrites,
/// with `args[1]`/`args[2]` already tagged `Sexpr`s (the mem layer hashes the
/// key). Returns the compiled `Unit` (`0`), like `HashTable<K,V>::set`. Only
/// stores already-decoded `Value`s into the map (no cons-heap allocation), so
/// triggers no GC.
///
/// # Safety
///
/// `argc >= 3`, `args` valid for 3 `i64`s, `args[0]` a boxed `HashTable`; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_set(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_hashtable_set: expected 3 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_hashtable_set: first argument is not a boxed HashTable"),
    };
    let key = decode(*args.add(1));
    let val = decode(*args.add(2));
    active_heap().hashtable_set(id, key, val);
    0
}

/// `(rt-hashtable-count ht)` for compiled code — the entry count as a raw
/// `i64` (`HashTable<K,V>::count`, like `rt_struct_field_count`'s raw result).
///
/// # Safety
///
/// `argc >= 1`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_count(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_hashtable_count: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_hashtable_count: first argument is not a boxed HashTable"),
    };
    active_heap().hashtable_count(id) as i64
}

/// `(rt-hashtable-clear ht)` for compiled code — empties the map in place,
/// returning `Unit` (`0`). Allocates nothing, so triggers no GC.
///
/// # Safety
///
/// `argc >= 1`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_clear(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_hashtable_clear: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_hashtable_clear: first argument is not a boxed HashTable"),
    };
    active_heap().hashtable_clear(id);
    0
}

/// The `BoxId` of `args[0]` decoded as a boxed struct — shared preamble of the
/// three `Vector`-building `HashTable` enumerators below.
///
/// # Safety
///
/// `argc >= 1`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
unsafe fn hashtable_arg(args: *const i64, argc: u32, who: &str) -> BoxId {
    if argc < 1 {
        fatal(&format!("{who}: expected 1 argument"));
    }
    match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal(&format!("{who}: first argument is not a boxed HashTable")),
    }
}

/// `(rt-hashtable-keys ht)` — a fresh `Vector<K>` of the map's keys, in the
/// map's own iteration order. The keys are the map's already-tagged `Value`s,
/// still reachable through the (caller-rooted) map box, so building the vector
/// in a single `alloc_struct` needs no intermediate rooting.
///
/// # Safety
///
/// See [`hashtable_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_keys(args: *const i64, argc: u32) -> i64 {
    let id = hashtable_arg(args, argc, "rt_hashtable_keys");
    let fields: Vec<Value> = active_heap().hashtable_pairs(id).into_iter().map(|(k, _)| k).collect();
    encode(active_heap().alloc_struct("vector".to_string(), fields))
}

/// `(rt-hashtable-values ht)` — a fresh `Vector<V>` of the map's values; see
/// [`rt_hashtable_keys`].
///
/// # Safety
///
/// See [`hashtable_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_values(args: *const i64, argc: u32) -> i64 {
    let id = hashtable_arg(args, argc, "rt_hashtable_values");
    let fields: Vec<Value> = active_heap().hashtable_pairs(id).into_iter().map(|(_, v)| v).collect();
    encode(active_heap().alloc_struct("vector".to_string(), fields))
}

/// `(rt-hashtable-entries ht)` — a fresh `Vector<cons-cell<K,V>>`, each entry
/// a `cons-cell` boxing the pair (matching `Checker::check_construct`'s own
/// `cons-cell` layout and the interpreter's `hashtable_entries`). Unlike
/// `keys`/`values`, each `cons-cell` is a *new* allocation, so each is rooted
/// as it is built — otherwise a later `alloc_struct`'s GC could reclaim an
/// earlier, not-yet-referenced cell. (The pair's `Value`s themselves stay
/// reachable through the map box while this runs, like `keys`/`values`.)
///
/// # Safety
///
/// See [`hashtable_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_entries(args: *const i64, argc: u32) -> i64 {
    let id = hashtable_arg(args, argc, "rt_hashtable_entries");
    let pairs = active_heap().hashtable_pairs(id);
    let mut fields = Vec::with_capacity(pairs.len());
    let mut rooted = 0usize;
    for (k, v) in pairs {
        let cell = active_heap().alloc_struct("cons-cell".to_string(), vec![k, v]);
        active_heap().push_root(cell);
        rooted += 1;
        fields.push(cell);
    }
    let vec = active_heap().alloc_struct("vector".to_string(), fields);
    for _ in 0..rooted {
        active_heap().pop_root();
    }
    encode(vec)
}


/// `(rt-hashtable-contains ht key)` for compiled code — a raw `i64` 0/1: does
/// `key` (already tagged) exist in the map? `HashTable<K,V>::get`/`remove`'s
/// primitive (compiled): since a `mem::Value`'s tag space is fully used by
/// real values, there is no free bit pattern to signal "absent" from a
/// value-returning call alone, so `get`/`remove` are compiled as *two* calls
/// — check here first, then [`rt_hashtable_get_raw`]/
/// [`rt_hashtable_remove_raw`] only if this returned `1`. Safe (no race) in
/// single-threaded compiled code: nothing between the two calls can remove
/// the entry this one just confirmed present.
///
/// # Safety
///
/// `argc >= 2`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_contains(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_hashtable_contains: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_hashtable_contains: first argument is not a boxed HashTable"),
    };
    let key = decode(*args.add(1));
    if active_heap().hashtable_get(id, key).is_some() {
        1
    } else {
        0
    }
}

/// `(rt-hashtable-get-raw ht key)` — the tagged `Sexpr` value at `key`, for
/// compiled code. Only ever called after [`rt_hashtable_contains`] confirmed
/// `key` present; fatal if it turns out absent (an internal-invariant trap,
/// the same convention every other `rt_*` bounds/shape violation here uses —
/// the compiled caller's own `if` guard is responsible for never letting that
/// happen). The caller (`compiler.rs`'s `compile-hashtable-op`) still decodes
/// the raw tagged result per `V`'s `struct_field_kind` before storing it into
/// a freshly built `Option<V>` box (`compile-sexpr-field`, the same decode a
/// `BoxedObj::Struct` field read already uses).
///
/// # Safety
///
/// `argc >= 2`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_get_raw(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_hashtable_get_raw: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_hashtable_get_raw: first argument is not a boxed HashTable"),
    };
    let key = decode(*args.add(1));
    match active_heap().hashtable_get(id, key) {
        Some(v) => encode(v),
        None => fatal("rt_hashtable_get_raw: key not present (caller must check rt_hashtable_contains first)"),
    }
}

/// `(rt-hashtable-remove-raw ht key)` — like [`rt_hashtable_get_raw`], but
/// also deletes the entry (`HashTable<K,V>::remove`'s primitive). Same
/// "caller already checked [`rt_hashtable_contains`]" precondition.
///
/// # Safety
///
/// `argc >= 2`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_remove_raw(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_hashtable_remove_raw: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_hashtable_remove_raw: first argument is not a boxed HashTable"),
    };
    let key = decode(*args.add(1));
    match active_heap().hashtable_remove(id, key) {
        Some(v) => encode(v),
        None => fatal("rt_hashtable_remove_raw: key not present (caller must check rt_hashtable_contains first)"),
    }
}


// ---- Stage 4: GC root safety -------------------------------------------

/// Registers a `Sexpr`-typed value as a GC root for as long as it's live in
/// a compiled frame — closing the gap [`rt_cons`]'s doc comment calls out:
/// without this, a GC that `rt_cons` (or any other allocating `rt_*` call)
/// triggers internally could reclaim a cons cell some *other* live value in
/// the calling frame still points to, since nothing makes that value
/// visible to `Heap::gc`'s root walk otherwise. `compiler.rs`'s future
/// rooting pass (paralleling its existing `ClosureBox` retain/release
/// insertion) is expected to wrap every `Sexpr`-typed local's lexical scope
/// in a push here / [`rt_pop_sexpr_root`] there, the same way retain/release
/// already wrap a `Fn`-typed one's.
///
/// Returns its argument unchanged (like `build-closure-retain`), so a
/// caller can chain it directly around the value it's rooting rather than
/// needing a separate statement.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_push_sexpr_root(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_push_sexpr_root: expected 1 argument");
    }
    let tagged = *args;
    active_heap().push_root(decode(tagged));
    tagged
}

/// The inverse of [`rt_push_sexpr_root`] — pops the most recently pushed
/// root and returns it (decoded back to its tagged form), for the matching
/// end of whatever lexical scope pushed it. Fatal if nothing is on the root
/// stack to pop: every call site is expected to be paired 1:1 with an
/// earlier [`rt_push_sexpr_root`], so an empty stack here is a
/// `compiler.rs`-side bug, not a recoverable condition.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_pop_sexpr_root(_args: *const i64, _argc: u32) -> i64 {
    match active_heap().pop_root() {
        Some(v) => encode(v),
        None => fatal("rt_pop_sexpr_root: root stack was empty"),
    }
}

/// Registers a `Sexpr`-typed value as a *permanent* GC root — for a value
/// stored into a general-ADT box's field (`compiler.rs`'s
/// `compile-construct-box-fields`), not a call-stack-scoped local.
/// [`rt_push_sexpr_root`]'s root lives on `Heap`'s ordinary `roots` stack,
/// which every caller above and below relies on strict LIFO pairing for
/// (push on scope entry, pop on scope exit) — a box field has no such scope:
/// it must stay reachable for as long as the box itself does, which can
/// outlive the activation that built it. Pushing it there with no matching
/// pop would desync every *other* `rt_pop_sexpr_root` call still to come in
/// the same thread (the next one would pop this field's root instead of
/// whatever it actually owns). [`typelisp_mem::Heap::push_permanent_root`]
/// exists precisely to take such a value off to one side, in a `Vec`
/// `Heap::gc`'s mark phase walks but no `rt_*` function ever pops from — see
/// that method's doc comment. No `rt_pop_permanent_sexpr_root` exists, by
/// design: this leaks one root slot per call, forever, the same trade-off
/// the box itself already makes (never `build-free`'d — `compiler.rs`'s
/// `compile-construct-box` doc comment).
///
/// Returns its argument unchanged, like [`rt_push_sexpr_root`].
///
/// # Safety
///
/// Same as [`rt_push_sexpr_root`].
#[no_mangle]
pub unsafe extern "C" fn rt_push_permanent_sexpr_root(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_push_permanent_sexpr_root: expected 1 argument");
    }
    let tagged = *args;
    active_heap().push_permanent_root(decode(tagged));
    tagged
}

/// Returns the active `Heap`'s current root count (`Heap::root_count`), as a
/// raw host index — never a tagged `Sexpr`, unlike every other `rt_*`
/// function's `i64` payload. `compiler.rs`'s `retain-bindings`/
/// `bind-let-values` call this *immediately before* their own
/// [`rt_push_sexpr_root`] call for a `kind = 2` (`Sexpr`-typed) binding, to
/// capture the exact stack position that root is about to occupy (`push_root`
/// appends at the end, so "current count" *is* "the new root's index"). That
/// index is then stashed in a second word of the binding's own slot
/// (`compiler.rs`'s `bind-params`/`bind-captures`/`bind-let-values` all now
/// allocate two words per binding rather than one) so a later `setf` to the
/// same binding (`compile-set`) can hand it straight to
/// [`rt_set_sexpr_root`] — see that function's doc comment for why an
/// update-in-place, not a second push, is what a reassignment actually
/// needs.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_root_count(_args: *const i64, _argc: u32) -> i64 {
    active_heap().root_count() as i64
}

/// `setf`'s GC-root counterpart to [`rt_push_sexpr_root`]: overwrites the
/// root at stack position `args[0]` (a raw host index, *not* a tagged
/// `Sexpr` — see [`rt_root_count`]) with `args[1]` (`Heap::set_root`).
///
/// A `let`/parameter/captured binding's GC root is pushed exactly once, at
/// bind time, holding whatever value the binding started with
/// (`rt_push_sexpr_root`, called from `compiler.rs`'s `retain-bindings`/
/// `bind-let-values`). `compile-set` only ever overwrites the binding's own
/// value *slot* in place — it never touches `Heap.roots` — so without this,
/// a `setf` that reassigns a `kind = 2` binding to a freshly built value
/// leaves that new value completely unrooted for the rest of the binding's
/// scope: the existing root stays pinned to the *original* value (itself
/// now harmlessly over-retained, not a correctness problem), while the new
/// one is exposed to the very next GC any unrelated allocation triggers.
/// `crates/typelisp-rt/src/lib.rs`'s own
/// `a_setf_reassigned_sexpr_value_is_corrupted_by_a_gc_triggered_by_other_allocations_without_rt_set_sexpr_root`
/// test demonstrates that corruption directly, the same way
/// [`rt_push_sexpr_root`]'s doc comment points at
/// `an_unrooted_value_is_corrupted_by_a_gc_triggered_by_other_allocations`
/// for the never-rooted-at-all case this complements. A binding keeps the
/// same root-stack slot for its whole lifetime (`rt_root_count`, called
/// once when the root is first pushed, hands back that fixed index), so
/// every subsequent `setf` to the same binding just updates that one slot
/// again — `O(1)` regardless of how many times it's reassigned (e.g. inside
/// a loop), unlike pushing a fresh root per `setf` would be (which has no
/// matching pop count known until runtime).
///
/// Returns `args[1]` unchanged, like [`rt_push_sexpr_root`].
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s; a
/// `Heap` must already be registered on this thread; `args[0]` must be a
/// valid index into that `Heap`'s current root stack (always true in
/// practice — it's always a value `rt_root_count` itself returned earlier in
/// the same dynamic scope, never user input).
#[no_mangle]
pub unsafe extern "C" fn rt_set_sexpr_root(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_set_sexpr_root: expected 2 arguments");
    }
    let idx = *args as usize;
    let tagged = *args.add(1);
    active_heap().set_root(idx, decode(tagged));
    tagged
}

/// `compiler.rs`'s `compile-break`/`compile-return` call this, with
/// `args[0]` set to the root count `compile-loop` read (`rt_root_count`)
/// right before entering the loop's own body, immediately before building
/// the jump to the loop's exit block — discarding, in one call
/// (`Heap::truncate_roots`), every `Sexpr`-typed GC root any number of
/// nested `let`/`match` scopes between the `break`/`return` site and the
/// loop pushed and never got the chance to pop, since a `break`/`return`
/// jumps straight past their own ordinary pop-on-scope-exit code
/// (`compile-let`'s `unroot-let-sexpr-values`, `compile-match`'s
/// `pop-sexpr-root` at its merge block — both skip that pop specifically
/// *because* the block is already terminated by the jump this truncates
/// for). Unlike [`rt_pop_sexpr_root`], which is fatal on an empty stack,
/// this is a no-op if `args[0] >= rt_root_count()` already (a `break`/
/// `return` with no scopes open above the loop itself) — the same
/// "truncating to at-or-past the current length does nothing" behavior
/// [`typelisp_mem::Heap::truncate_roots`] gets from `Vec::truncate`.
///
/// Returns `0` — nothing meaningful to hand back, like [`rt_heap_init`].
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_truncate_sexpr_roots(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_truncate_sexpr_roots: expected 1 argument");
    }
    let len = *args as usize;
    active_heap().truncate_roots(len);
    0
}

// ---- Stage 5: Match -----------------------------------------------------

/// `compiler.rs`'s `compile-match-arms` calls this once every arm's
/// pattern has failed to match — provably unreachable for a well-typed
/// program (the checker's own exhaustiveness check already guarantees one
/// of `nil`/`int`/`float`/`char`/`bool`/`sym`/`str`/`cons` always matches a
/// `Sexpr` scrutinee), so this is a trap for a `compiler.rs`/checker bug,
/// not a normal/recoverable runtime condition — there is nothing
/// meaningful to return.
///
/// # Safety
///
/// None beyond the ordinary compiled-function-ABI contract — unlike every
/// other `rt_*` function here, this one never touches the active `Heap`.
#[no_mangle]
pub unsafe extern "C" fn rt_match_fail(_args: *const i64, _argc: u32) -> i64 {
    fatal("match: no pattern arm matched (the checker should have guaranteed exhaustiveness)")
}

/// `(panic msg)` for compiled code (`core_bridge`'s `panic` arm /
/// `compiler.rs`'s `compile-panic`): prints `"panic: {msg}"` — the same
/// wording `EvalError::Panic`'s `Display` impl uses for an *interpreted*
/// `(panic ...)` — and aborts the process. Unlike the interpreted path
/// (where a user `panic` unwinds as an ordinary, recoverable `Result::Err`
/// the caller can propagate), compiled code has no landing pads to unwind
/// through across the JIT/AOT native-code boundary, so aborting is the only
/// safe option here — the same rule every other unrecoverable compiled-code
/// failure path (e.g. [`rt_match_fail`]) already follows. A deliberate
/// behavioral divergence from the interpreter, not an oversight.
///
/// # Safety
///
/// `argc` must be `>= 1`, `args[0]` must decode to a `Value::Str`; a `Heap`
/// must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_panic(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_panic: expected 1 argument");
    }
    let msg = match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        _ => fatal("rt_panic: argument is not a Str"),
    };
    eprintln!("panic: {}", msg);
    std::process::abort();
}

// ---- Stage 7: Str --------------------------------------------------------

/// `(str c0 c1 ... cN-1)` for compiled code — builds a fresh, heap-allocated
/// string from `argc` *raw* (untagged) Unicode scalar values, one per
/// character, and returns it already tagged the same way `encode`/`decode`
/// represent a `Value::Str` — this tagged form doubles as the compiled
/// representation of a bare `Type::Str` value too (`compiler.rs`'s
/// `compile-sexpr-field`/`compile-construct-sexpr` doc comments explain why:
/// keeping the tag, rather than stripping it the way `int`/`char`/`bool`
/// extraction does, is what lets a `Type::Str` local reuse
/// [`rt_push_sexpr_root`]'s already-generic `decode`-based protection with
/// no new rooting mechanism).
///
/// `compiler.rs`'s `compile-str` is the only caller: a string literal's
/// content is entirely known at compile time, so each character becomes an
/// ordinary `const-i64` operand (`core_bridge::str_form` builds a
/// `(str (int c0) (int c1) ...)` node, reusing `compile-value`'s existing
/// `int` handling for every character rather than needing a new
/// literal-embedding mechanism) — unlike every other allocating `rt_*`
/// function, none of `args` here is itself a tagged `Sexpr` value to
/// `decode`.
///
/// Unlike [`rt_cons`], `Heap::alloc_string` never runs a GC itself (the
/// string arena grows without a fixed capacity) — but the string this
/// returns is exactly as unrooted as a fresh cons cell until some caller
/// roots it or immediately consumes it, for the same reason [`rt_cons`]'s
/// doc comment gives.
///
/// # Safety
///
/// `args` must point to at least `argc` valid, readable `i64`s, each a valid
/// Unicode scalar value; a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_str_new(args: *const i64, argc: u32) -> i64 {
    let mut s = String::with_capacity(argc as usize);
    for i in 0..argc as isize {
        let scalar = *args.offset(i) as u32;
        match char::from_u32(scalar) {
            Some(c) => s.push(c),
            None => fatal("rt_str_new: invalid char scalar value"),
        }
    }
    encode(active_heap().alloc_string(s))
}

/// Interns `args[0]` (a tagged `Value::Str`) as a symbol, returning it as a
/// tagged `Value::Symbol` — the compiled-code half of a quoted symbol
/// literal (`core_bridge::quoted_form`'s `Sym` arm builds the name as an
/// ordinary `(str (int c0) ...)` node, exactly like any other string
/// literal, then wraps the compiled result in a call here rather than
/// needing its own character-embedding mechanism the way `rt_str_new`
/// needed one for `Str`). Interning is idempotent and the result is
/// permanent (`Heap::intern_symbol` — symbols are never collected, unlike a
/// fresh `Str`), so unlike `rt_str_new`/`rt_cons` this needs no GC-root
/// protection at all.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// encoding a `Value::Str`; a `Heap` must already be registered on this
/// thread.
#[no_mangle]
pub unsafe extern "C" fn rt_intern_symbol(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_intern_symbol: expected 1 argument");
    }
    let name = match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        _ => fatal("rt_intern_symbol: argument is not a Str"),
    };
    encode(active_heap().intern_symbol(&name))
}

/// Mints a fresh `gensym` symbol, returning it as a tagged `Value::Symbol` —
/// the compiled-code half of `gensym` (`compiler.rs`'s `compile-call` rewrites
/// the free call `(gensym)` to this shim by name, the same way it rewrites
/// `sexpr-car` -> `rt_car`). Takes no arguments. Delegates to [`Heap::gensym`]
/// so it draws from the *same* per-heap counter the interpreter's `gensym`
/// builtin uses — a program whose macro expansion mixes interpreted and
/// compiled `gensym` calls (a `case` body's own vs. a JIT'd `do` expansion-
/// lambda's) must never mint the same name twice. The result is a permanent
/// interned symbol, so — like [`rt_intern_symbol`] — it needs no GC-root
/// protection.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread. `args`/`argc` are
/// unused (`gensym` is nullary).
#[no_mangle]
pub unsafe extern "C" fn rt_gensym(_args: *const i64, _argc: u32) -> i64 {
    encode(active_heap().gensym())
}

/// Signed integer division `a / b` — the compiled-code half of `i64`/`i32`
/// `/`, which (unlike `+`/`-`/`*`) can't be a bare LLVM instruction because
/// LLVM `sdiv` by zero is undefined behavior. Matches the interpreter's
/// `int_binop`, which panics on a zero divisor; `checked_div` also catches the
/// one other trap case, `i64::MIN / -1` (arithmetic overflow). Operands are
/// raw untagged `i64`s (the int branch of `compile-assoc` computes them the
/// same way `build-add` does), so there is no decode/encode here.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_i64_div(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_i64_div: expected 2 arguments");
    }
    match (*args).checked_div(*args.add(1)) {
        Some(q) => q,
        None => fatal("divide by zero"),
    }
}

/// Floored remainder `a mod b` (CL `mod`) — the compiled-code half of
/// `i64`/`i32` `mod`, matching the interpreter's `eval_int_builtin`. The
/// result takes the sign of the divisor `b` (unlike `srem`/`rem`, which take
/// the sign of the dividend), so `-7 mod 3 = 2`. Same zero-trap handling as
/// [`rt_i64_div`]; `i64::MIN % -1` is mathematically `0` (`checked_rem`
/// returns `None` on that overflow case, treated as the exact `0` here).
/// Raw-`i64` operand convention.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_i64_mod(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_i64_mod: expected 2 arguments");
    }
    let (a, b) = (*args, *args.add(1));
    if b == 0 {
        fatal("mod by zero");
    }
    let r = a.checked_rem(b).unwrap_or(0);
    if r != 0 && (r < 0) != (b < 0) {
        r + b
    } else {
        r
    }
}

/// `(ash integer count)` — the compiled-code half of `i64`/`i32` `ash`,
/// matching `eval_int_builtin`'s. Not a bare LLVM `shl`/`ashr` because both
/// are undefined behavior once the shift amount reaches the operand's bit
/// width, which `count >= 64`/`count <= -64` would hit directly; those are
/// clamped here to their limiting value (`0` left, sign-smeared `-1`/`0`
/// right) instead. `args[0]` is the integer, `args[1]` the count (receiver-
/// first, CL's own argument order).
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_i64_ash(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_i64_ash: expected 2 arguments");
    }
    let (n, count) = (*args, *args.add(1));
    if count >= 0 {
        if count >= 64 {
            0
        } else {
            n.wrapping_shl(count as u32)
        }
    } else if -count >= 64 {
        if n < 0 {
            -1
        } else {
            0
        }
    } else {
        n >> (-count)
    }
}

/// `(logbitp index integer)` — the compiled-code half, matching
/// `eval_int_builtin`'s. Not a bare `lshr`+`and` because a variable shift by
/// `index >= 64` is undefined behavior in LLVM; that case is handled
/// directly (any bit position beyond the width just reads the sign).
/// `args[0]` is the index (receiver-first), `args[1]` the integer.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_i64_logbitp(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_i64_logbitp: expected 2 arguments");
    }
    let (index, n) = (*args, *args.add(1));
    let bit = if index >= 64 { (n < 0) as i64 } else { (n >> index) & 1 };
    bit
}

/// `logcount` (population count of a nonnegative integer, or of the 0-bits
/// of a negative one — CL's own "infinite two's complement" reading) — the
/// compiled-code half, matching `eval_int_builtin`'s. Not a bare
/// `llvm.ctpop.i64` call because the negative case first needs a conditional
/// `lognot`, cheaper to express directly here than as extra IR at every call
/// site.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_i64_logcount(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_i64_logcount: expected 1 argument");
    }
    let n = *args;
    (if n >= 0 { n.count_ones() } else { (!n).count_ones() }) as i64
}

/// `integer-length` (bits needed, excluding sign) — the compiled-code half,
/// matching `eval_int_builtin`'s: `n`'s own bit length when nonnegative,
/// else `!n`'s (CL's negative-integer-length identity).
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_i64_integer_length(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_i64_integer_length: expected 1 argument");
    }
    let n = *args;
    let m = if n >= 0 { n } else { !n };
    (64 - m.leading_zeros()) as i64
}

/// Shared shape for the `f64` transcendental unaries that have no LLVM
/// intrinsic in the LLVM version this project pins (`sin`/`cos`/`exp`/`log`
/// do and go straight to LLVM IR in `compiler.rs` instead — see
/// `llvm_builder_build_float_unary_intrinsic`): decode the raw bit-pattern
/// argument, apply `f`, re-encode. `argc`/pointer validity is the caller's
/// (`compiler.rs`'s `alloca-args`/`store-arg`) responsibility, same as every
/// other `rt_*` shim here.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// (an `f64`'s raw bit pattern).
unsafe fn rt_f64_unary(args: *const i64, argc: u32, name: &str, f: fn(f64) -> f64) -> i64 {
    if argc < 1 {
        fatal(&format!("{}: expected 1 argument", name));
    }
    f(f64::from_bits(*args as u64)).to_bits() as i64
}

#[no_mangle]
pub unsafe extern "C" fn rt_f64_tan(args: *const i64, argc: u32) -> i64 {
    rt_f64_unary(args, argc, "rt_f64_tan", f64::tan)
}
#[no_mangle]
pub unsafe extern "C" fn rt_f64_asin(args: *const i64, argc: u32) -> i64 {
    rt_f64_unary(args, argc, "rt_f64_asin", f64::asin)
}
#[no_mangle]
pub unsafe extern "C" fn rt_f64_acos(args: *const i64, argc: u32) -> i64 {
    rt_f64_unary(args, argc, "rt_f64_acos", f64::acos)
}
#[no_mangle]
pub unsafe extern "C" fn rt_f64_atan(args: *const i64, argc: u32) -> i64 {
    rt_f64_unary(args, argc, "rt_f64_atan", f64::atan)
}
#[no_mangle]
pub unsafe extern "C" fn rt_f64_sinh(args: *const i64, argc: u32) -> i64 {
    rt_f64_unary(args, argc, "rt_f64_sinh", f64::sinh)
}
#[no_mangle]
pub unsafe extern "C" fn rt_f64_cosh(args: *const i64, argc: u32) -> i64 {
    rt_f64_unary(args, argc, "rt_f64_cosh", f64::cosh)
}
#[no_mangle]
pub unsafe extern "C" fn rt_f64_tanh(args: *const i64, argc: u32) -> i64 {
    rt_f64_unary(args, argc, "rt_f64_tanh", f64::tanh)
}
#[no_mangle]
pub unsafe extern "C" fn rt_f64_asinh(args: *const i64, argc: u32) -> i64 {
    rt_f64_unary(args, argc, "rt_f64_asinh", f64::asinh)
}
#[no_mangle]
pub unsafe extern "C" fn rt_f64_acosh(args: *const i64, argc: u32) -> i64 {
    rt_f64_unary(args, argc, "rt_f64_acosh", f64::acosh)
}
#[no_mangle]
pub unsafe extern "C" fn rt_f64_atanh(args: *const i64, argc: u32) -> i64 {
    rt_f64_unary(args, argc, "rt_f64_atanh", f64::atanh)
}

/// Interns `args[0..argc]` (each a tagged `Value::Symbol`, one per `::`
/// segment, in order — `core_bridge::quoted_form`'s `Path` arm builds
/// each segment as its own `(str ...)` literal, so the compiled IR calls
/// [`rt_intern_symbol`] once per segment before collecting the results
/// here) as a single `::`-path, returning it as a tagged `Value::Path`.
/// Same permanence/no-GC-root-needed reasoning as `rt_intern_symbol`
/// (`Heap::intern_path` — a path references only permanent symbols and is
/// itself permanent).
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least `argc` valid
/// `i64`s, each encoding a `Value::Symbol`; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_intern_path(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_intern_path: expected at least 1 argument");
    }
    let mut segs = Vec::with_capacity(argc as usize);
    for i in 0..argc as isize {
        match decode(*args.offset(i)) {
            Value::Symbol(id) => segs.push(id),
            _ => fatal("rt_intern_path: argument is not a Symbol"),
        }
    }
    encode(active_heap().intern_path(&segs))
}

/// Converts a tagged `Value::Path` into a fresh `Sexpr` list of its segments
/// as `Value::Symbol`s — the compiled-code half of the `Sexpr` `path`
/// variant's field extraction (`compiler.rs`'s `compile-sexpr-field` variant
/// `10`), mirroring `typelisp::eval::interp`'s `match_sexpr_ctor` `SEXPR_PATH`
/// arm exactly. Every segment is a permanent interned `Value::Symbol` (never
/// GC-collected), so only the freshly built `Cons` chain linking them needs
/// root protection while under construction, the same discipline
/// [`rt_cons`] itself would need for a multi-cell build.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// encoding a `Value::Path`; a `Heap` must already be registered on this
/// thread.
#[no_mangle]
pub unsafe extern "C" fn rt_path_to_list(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_path_to_list: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Path(id) => id,
        _ => fatal("rt_path_to_list: argument is not a Path"),
    };
    let heap = active_heap();
    let segs = heap.path_segments(id).to_vec();
    let mut acc = Value::Empty;
    for seg in segs.into_iter().rev() {
        heap.push_root(acc);
        let result = heap.cons(Value::Symbol(seg), acc);
        heap.pop_root();
        acc = match result {
            Ok(v) => v,
            Err(_) => fatal("rt_path_to_list: heap exhausted, no cons cell could be reclaimed"),
        };
    }
    encode(acc)
}

/// The inverse of [`rt_path_to_list`]: walks a tagged `Sexpr` list of
/// `Value::Symbol`s and interns it as a `Value::Path` — the compiled-code
/// half of the `Sexpr` `path` variant's construction (`compiler.rs`'s
/// `compile-construct-sexpr` variant `10`). Unlike [`rt_intern_path`] (whose
/// segments are a compile-time-known-arity variadic argument list, for a
/// literal quoted path), this one's segment count is only known at runtime —
/// the list itself is a single already-compiled value, not `N` separate
/// compiled sub-expressions.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// encoding a proper `Sexpr` list of `Value::Symbol`s; a `Heap` must already
/// be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_list_to_path(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_list_to_path: expected 1 argument");
    }
    let heap = active_heap();
    let mut segs = Vec::new();
    let mut cur = decode(*args);
    loop {
        match cur {
            Value::Empty => break,
            Value::Cons(_) => {
                match heap.car(cur) {
                    Ok(Value::Symbol(id)) => segs.push(id),
                    _ => fatal("rt_list_to_path: list element is not a Symbol"),
                }
                cur = match heap.cdr(cur) {
                    Ok(v) => v,
                    Err(_) => fatal("rt_list_to_path: argument is not a proper list"),
                };
            }
            _ => fatal("rt_list_to_path: argument is not a proper list"),
        }
    }
    if segs.is_empty() {
        fatal("rt_list_to_path: path must have at least one segment");
    }
    encode(heap.intern_path(&segs))
}

/// `str::length` for compiled code — the character count (not byte length)
/// of `args[0]`, matching the interpreter's own `string_length`. Returns a
/// bare `i64` (a `Type::I64` result is never itself Sexpr-tagged).
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`
/// encoding a `Value::Str`; a `Heap` must already be registered on this
/// thread.
#[no_mangle]
pub unsafe extern "C" fn rt_str_length(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_str_length: expected 1 argument");
    }
    let id = match decode(*args) {
        Value::Str(id) => id,
        _ => fatal("rt_str_length: argument is not a Str"),
    };
    active_heap().string(id).chars().count() as i64
}

/// `str::ref` for compiled code — the `i`-th Unicode scalar value of
/// `args[0]`'s content (`args[1]`, a bare `i64` index), matching the
/// interpreter's own `string_ref`. Returns a bare (untagged) scalar, the
/// same `Type::Char` representation `compile-sexpr-field`'s own `char`
/// extraction produces. Fatal on an out-of-range index — the type system
/// can't express the bound, the same `car`/`cdr`-on-non-`Cons` precedent
/// every other `rt_*` bounds violation here follows (the interpreter's own
/// `string_ref` instead raises a catchable `Panic`, but there is no such
/// channel across the compiled-code ABI boundary — see [`fatal`]).
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// the first encoding a `Value::Str`; a `Heap` must already be registered on
/// this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_str_ref(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_str_ref: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Str(id) => id,
        _ => fatal("rt_str_ref: first argument is not a Str"),
    };
    let idx = *args.add(1);
    let found = if idx >= 0 { active_heap().string(id).chars().nth(idx as usize) } else { None };
    match found {
        Some(c) => c as i64,
        None => fatal("rt_str_ref: index out of range"),
    }
}

/// `str::eq` for compiled code — content equality, not `Sexpr`'s own `eq`
/// (two separately-heap-allocated `Str`s never satisfy that even with equal
/// content — see `registry::sexpr_assoc`'s doc comment). Returns a bare
/// `0`/`1`, the same `Type::Bool` convention every comparison builtin here
/// already uses (`build-icmp-*`'s zero-extended result).
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// both encoding a `Value::Str`; a `Heap` must already be registered on this
/// thread.
#[no_mangle]
pub unsafe extern "C" fn rt_str_eq(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_str_eq: expected 2 arguments");
    }
    let a = match decode(*args) {
        Value::Str(id) => id,
        _ => fatal("rt_str_eq: first argument is not a Str"),
    };
    let b = match decode(*args.add(1)) {
        Value::Str(id) => id,
        _ => fatal("rt_str_eq: second argument is not a Str"),
    };
    let heap = active_heap();
    i64::from(heap.string(a) == heap.string(b))
}

/// `str::lt` for compiled code — lexicographic (code-point order) content
/// comparison, matching the interpreter's own `string_lt` (Rust `&str`'s
/// `<`). Returns a bare `0`/`1` like [`rt_str_eq`]. Backs the compiled form
/// of the prelude's `impl Ord string` (`less` delegates to `lt`).
///
/// # Safety
///
/// Same as [`rt_str_eq`].
#[no_mangle]
pub unsafe extern "C" fn rt_str_lt(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_str_lt: expected 2 arguments");
    }
    let a = match decode(*args) {
        Value::Str(id) => id,
        _ => fatal("rt_str_lt: first argument is not a Str"),
    };
    let b = match decode(*args.add(1)) {
        Value::Str(id) => id,
        _ => fatal("rt_str_lt: second argument is not a Str"),
    };
    let heap = active_heap();
    i64::from(heap.string(a) < heap.string(b))
}

/// `str::equalp` for compiled code — ASCII case-insensitive content equality
/// (`&str::eq_ignore_ascii_case`), matching the interpreter's own
/// `string_content_eqp`. Returns a bare `0`/`1` like [`rt_str_eq`]. Unlike
/// `eq`/`equal` (plain content comparison, `rt_str_eq`), `equalp` has no
/// single-instruction lowering — the compiler can't fold case in place — so
/// it needs this dedicated runtime helper.
///
/// # Safety
///
/// Same as [`rt_str_eq`].
#[no_mangle]
pub unsafe extern "C" fn rt_str_equalp(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_str_equalp: expected 2 arguments");
    }
    let a = match decode(*args) {
        Value::Str(id) => id,
        _ => fatal("rt_str_equalp: first argument is not a Str"),
    };
    let b = match decode(*args.add(1)) {
        Value::Str(id) => id,
        _ => fatal("rt_str_equalp: second argument is not a Str"),
    };
    let heap = active_heap();
    i64::from(heap.string(a).eq_ignore_ascii_case(heap.string(b)))
}

/// `char::equalp` for compiled code — ASCII case-insensitive equality
/// (`char::eq_ignore_ascii_case`), matching the interpreter's own `char_eqp`.
/// Unlike a compiled `char`'s `eq`/`eql`/`equal`/`<`/... (raw `i64` code-point
/// `icmp`s, lowered in place by `compile-assoc`), `equalp` folds case and so
/// has no single-instruction form. `args[0]`/`args[1]` are the *raw* (untagged)
/// `i64` Unicode scalar values a compiled `char` is represented as — not
/// tagged `Sexpr`s — matching `compile-assoc`'s char branch, which compiles
/// its operands to bare code points. Returns a bare `0`/`1`.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
/// No `Heap` is needed (both operands are immediates).
#[no_mangle]
pub unsafe extern "C" fn rt_char_equalp(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_char_equalp: expected 2 arguments");
    }
    let a = *args;
    let b = *args.add(1);
    match (char::from_u32(a as u32), char::from_u32(b as u32)) {
        (Some(x), Some(y)) => i64::from(x.eq_ignore_ascii_case(&y)),
        // A compiled `char` always holds a valid scalar value; a mismatch here
        // would be an internal invariant break, but two non-`char` bit
        // patterns can still only be "equalp" if bit-identical.
        _ => i64::from(a == b),
    }
}

/// `str::append` for compiled code — concatenates the content of
/// `args[0]`/`args[1]` into a freshly allocated string, matching the
/// interpreter's own `string_append`. Returns the tagged form, exactly like
/// [`rt_str_new`] (and just as unrooted until a caller protects it).
///
/// # Safety
///
/// Same as [`rt_str_eq`].
#[no_mangle]
pub unsafe extern "C" fn rt_str_append(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_str_append: expected 2 arguments");
    }
    let a = match decode(*args) {
        Value::Str(id) => id,
        _ => fatal("rt_str_append: first argument is not a Str"),
    };
    let b = match decode(*args.add(1)) {
        Value::Str(id) => id,
        _ => fatal("rt_str_append: second argument is not a Str"),
    };
    let heap = active_heap();
    let s = format!("{}{}", heap.string(a), heap.string(b));
    encode(heap.alloc_string(s))
}

// ---- Global variables (compile `Global`/`SetGlobal` support) ------------
//
// A `defvar`/`defconstant` a compiled function references needs storage
// compiled code can reach — the interpreter's own `Interp.globals` is a
// plain Rust `HashMap` on the other side of the JIT/AOT boundary, invisible
// here. Each such global is instead backed by a permanent GC root
// (`Heap::push_permanent_root`, the same mechanism `rt_push_permanent_sexpr_root`
// already uses for boxed-struct fields — "lives for the process, no matching
// pop" is exactly a global's actual lifetime). `rt_global_new`/`rt_global_get`/
// `rt_global_set` are the read/write surface; see `rt_global_new`'s doc
// comment for why a global's *id* (what `compiler.rs`'s generated code
// addresses it by) and its raw permanent-root position aren't the same
// number in general.

thread_local! {
    /// Global id -> the `Heap::permanent_root` position it landed at, in
    /// `rt_global_new` call order — see that function's doc comment.
    /// `thread_local!` for the same cross-test-isolation reason
    /// `ACTIVE_HEAP` is (module doc comment); unlike `ACTIVE_HEAP`, nothing
    /// re-points this automatically on every call, so [`reset_global_table`]
    /// must be called whenever a fresh `Heap` begins its lifetime.
    static GLOBAL_INDEX: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// Clears this thread's global-id table. Must be called whenever a fresh
/// `Heap`/`Interp` pair begins its lifetime (`typelisp::eval::Interp::new`)
/// — otherwise a stale entry left over from an earlier `Heap` on a reused
/// `cargo test` worker thread would resolve to a permanent-root position
/// that has nothing to do with the *current* `Heap`'s (empty, freshly
/// created) `permanent_roots`. AOT's standalone executable never needs to
/// call this itself: [`rt_heap_init`] runs exactly once, before any
/// `rt_global_new` call, in a process that only ever has the one `Heap`.
pub fn reset_global_table() {
    GLOBAL_INDEX.with(|t| t.borrow_mut().clear());
}

/// Allocates a new global-variable slot holding `v` and returns its id — a
/// small, sequential index that later [`rt_global_get`]/[`rt_global_set`]
/// calls address it by. The Rust-side (non-FFI) counterpart of
/// [`rt_global_new`], for the interpreter itself to promote an
/// already-`defvar`d global to a compiled-global slot before generating IR
/// (`typelisp::eval::Interp::add_compiled_function`) — avoids that caller
/// round-tripping through the `extern "C"` tagged-`i64` ABI just to reach
/// this same bookkeeping, the same reason [`encode`]/[`decode`] are already
/// plain `pub fn`s rather than FFI-only.
///
/// The id is *not* the slot's `Heap::permanent_root` position: unrelated
/// code ([`rt_push_permanent_sexpr_root`], called by `compiler.rs`'s
/// `compile-construct-box-fields` for an ordinary boxed-struct field) also
/// pushes permanent roots, so a global's raw heap-root position isn't
/// predictable at the point `compiler.rs` generates a reference to it. What
/// *is* predictable: only this function ever appends to `GLOBAL_INDEX`, and
/// its callers (this crate's [`rt_global_new`], and `add_compiled_function`)
/// control the order those calls happen in — so `GLOBAL_INDEX`'s own length
/// at the moment of a given call is exactly the id the compiler already
/// expects for it.
pub fn global_new(heap: &mut Heap, v: Value) -> usize {
    let perm_idx = heap.permanent_root_count();
    heap.push_permanent_root(v);
    GLOBAL_INDEX.with(|t| {
        let mut t = t.borrow_mut();
        t.push(perm_idx);
        t.len() - 1
    })
}

/// `args[0]` (its initial tagged value) — see [`global_new`] for the id this
/// assigns.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_global_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_global_new: expected 1 argument");
    }
    let v = decode(*args);
    global_new(active_heap(), v) as i64
}

/// [`global_new`]'s id -> `Heap::permanent_root` position lookup, `pub` for
/// the same cross-crate reason [`encode`]/[`decode`] are: `typelisp::eval::
/// interp`'s `Expr::Global` (an *interpreted* read of a promoted global,
/// bypassing the `rt_global_get`/`encode` FFI round trip entirely since it
/// already holds the live `Heap`) needs the exact same id -> position
/// resolution [`rt_global_get`] does below — reading `Heap::permanent_root(id)`
/// directly would only coincidentally work were `id` and the position always
/// equal, true only when nothing else has pushed a permanent root between
/// two [`global_new`] calls (see [`global_new`]'s own doc comment for a case
/// where that's no longer true: an `Option`/`Result`-typed global whose
/// heap-referencing field also needs its own protecting permanent root).
/// `None` if `id` was never returned by [`global_new`] on this thread since
/// the last [`reset_global_table`] — callers across the FFI boundary
/// (`rt_global_get`/`rt_global_set`) turn that into their own `fatal` abort
/// rather than let a panic try to unwind through compiled native code; a
/// plain Rust-to-Rust caller may still choose to panic on it.
pub fn global_perm_idx(id: usize) -> Option<usize> {
    GLOBAL_INDEX.with(|t| t.borrow().get(id).copied())
}

/// Reads global `id`'s (an [`rt_global_new`] return value) current value.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread; `args` must point to
/// at least 1 valid `i64`; `id` must be a value `rt_global_new` actually
/// returned on this thread since the last [`reset_global_table`].
#[no_mangle]
pub unsafe extern "C" fn rt_global_get(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_global_get: expected 1 argument (the global id)");
    }
    let id = *args as usize;
    let perm_idx = global_perm_idx(id).unwrap_or_else(|| fatal("rt_global_get: unknown global id"));
    encode(active_heap().permanent_root(perm_idx))
}

/// Overwrites global `id`'s value (`args[1]`, tagged) in place
/// (`Heap::set_permanent_root`); returns the new value unchanged, like
/// [`rt_push_sexpr_root`].
///
/// # Safety
///
/// Same as [`rt_global_get`], plus `args` must point to at least 2 valid
/// `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_global_set(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_global_set: expected 2 arguments (the global id and its new value)");
    }
    let id = *args as usize;
    let tagged = *args.add(1);
    let perm_idx = global_perm_idx(id).unwrap_or_else(|| fatal("rt_global_set: unknown global id"));
    active_heap().set_permanent_root(perm_idx, decode(tagged));
    tagged
}

// ===========================================================================
// Trait objects and vtables (TODO T4)
// ===========================================================================
//
// A trait object is a `BoxedObj::Dyn` fat box: a vtable id plus the concrete
// value. The vtable itself lives *outside* the heap, here — one table per
// (concrete type, trait) pair, holding raw native function pointers in the
// trait's declared method order (`TraitDef::method_order`). A call site knows
// its slot statically, so dispatch is `rt_dyn_vtable` -> `rt_vtable_slot` ->
// an indirect call through the ordinary `compiled_fn_type` ABI, with no
// lookup by name or type at run time. Ids are assigned by the interpreter
// (`Interp::vtable_id_for`), which keeps its own parallel table of
// `(type, method)` identities for tree-walking calls.

thread_local! {
    /// vtable id -> slot -> native entry point (`compiled_fn_type`:
    /// `i64 f(i64* args, i32 argc)`), or 0 for a slot never filled in.
    /// `thread_local!` for the same cross-test-isolation reason
    /// [`GLOBAL_INDEX`] is, and reset by the same `Interp::new` hook.
    static VTABLES: RefCell<Vec<Vec<usize>>> = const { RefCell::new(Vec::new()) };

    /// `(vtable id, trait id)` -> the vtable id of the *same concrete type*
    /// for that trait — the supertrait upcast table ([`rt_dyn_upcast`]).
    /// Filled in by the interpreter from `Expr::DynBox::supers`
    /// ([`upcast_define`]) and by AOT startup ([`rt_upcast_set`]), and reset
    /// alongside [`VTABLES`], whose id space it is keyed by.
    static UPCASTS: RefCell<HashMap<(u32, u32), u32>> = RefCell::new(HashMap::new());
}

/// Clears this thread's vtable and upcast tables — the trait-object
/// counterpart of [`reset_global_table`], called from `Interp::new` for the
/// same reason: vtable ids are per-`Interp`, so a stale table from an earlier
/// one on a reused `cargo test` worker thread would answer with function
/// pointers into a JIT module that has since been dropped.
pub fn reset_vtable_table() {
    VTABLES.with(|t| t.borrow_mut().clear());
    UPCASTS.with(|t| t.borrow_mut().clear());
}

/// Records that a trait object dispatching through vtable `from` becomes one
/// dispatching through vtable `to` when upcast to the trait interned as
/// `trait_id` — the compiled tier's copy of `Interp::dyn_upcasts`, published
/// wherever the interpreter interns a boxing site's supertrait tables.
pub fn upcast_define(from: u32, trait_id: u32, to: u32) {
    UPCASTS.with(|t| {
        t.borrow_mut().insert((from, trait_id), to);
    });
}

/// Installs (or replaces) vtable `id`'s slots. Called from the interpreter
/// once a compilation unit's function addresses are final
/// (`Interp::compile_scc`) and from AOT startup; a slot holding 0 means "not
/// compiled", which [`rt_vtable_slot`] refuses to call.
pub fn vtable_define(id: u32, slots: Vec<usize>) {
    VTABLES.with(|t| {
        let mut t = t.borrow_mut();
        if t.len() <= id as usize {
            t.resize(id as usize + 1, Vec::new());
        }
        t[id as usize] = slots;
    });
}

/// Boxes `args[1]` (a tagged value) as a trait object dispatching through
/// vtable `args[0]`.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_dyn_new(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_dyn_new: expected 2 arguments (the vtable id and the value)");
    }
    let vtable_id = *args as u32;
    let value = decode(*args.add(1));
    encode(active_heap().alloc_dyn(vtable_id, value))
}

/// The vtable id of the trait object `args[0]`, as a raw `i64`.
///
/// # Safety
///
/// `argc` must be `>= 1`, `args[0]` must be a tagged `BoxedObj::Dyn`, and a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_dyn_vtable(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_dyn_vtable: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) if active_heap().is_dyn(id) => active_heap().dyn_vtable_id(id) as i64,
        _ => fatal("rt_dyn_vtable: not a trait object"),
    }
}

/// The concrete value inside the trait object `args[0]`, tagged.
///
/// # Safety
///
/// Same as [`rt_dyn_vtable`].
#[no_mangle]
pub unsafe extern "C" fn rt_dyn_value(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_dyn_value: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) if active_heap().is_dyn(id) => encode(active_heap().dyn_value(id)),
        _ => fatal("rt_dyn_value: not a trait object"),
    }
}

/// Slot `args[1]` of vtable `args[0]`: the raw native entry point a `:dyn`
/// call site then calls indirectly. An unfilled slot aborts rather than
/// jumping to null — reaching one means a method that a boxing site pulled
/// into the call graph was not compiled, which is a compiler bug, not
/// anything user code can provoke.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_vtable_slot(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_vtable_slot: expected 2 arguments (the vtable id and the slot)");
    }
    let id = *args as usize;
    let slot = *args.add(1) as usize;
    let ptr = VTABLES.with(|t| t.borrow().get(id).and_then(|s| s.get(slot).copied()).unwrap_or(0));
    if ptr == 0 {
        fatal("rt_vtable_slot: vtable slot is empty — a dyn dispatch target was not compiled");
    }
    ptr as i64
}

/// Upcasts the trait object `args[1]` to the supertrait interned as trait id
/// `args[0]`, returning a fat box over the same concrete value dispatching
/// through that supertrait's own vtable (`Expr::DynUpcast`). Emitted only
/// where the layouts do *not* share a prefix — the leftmost-spine case is a
/// checker-level retype that reaches no code at all.
///
/// A missing entry aborts rather than mis-dispatching: the mapping is
/// registered wherever a box for this concrete type is created
/// (`Expr::DynBox::supers`), and the checker only admits the conversion when
/// the target is in the source's supertrait closure, so reaching one means
/// the box came from somewhere that skipped registration — a compiler bug.
///
/// The source box stays rooted by the caller (a `kind = 2` argument), which
/// is what keeps the concrete value alive across the allocation below.
///
/// # Safety
///
/// `argc` must be `>= 2`, `args[1]` must be a tagged `BoxedObj::Dyn`, and a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_dyn_upcast(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_dyn_upcast: expected 2 arguments (the target trait id and the trait object)");
    }
    let trait_id = *args as u32;
    let value = decode(*args.add(1));
    let Value::Boxed(id) = value else {
        fatal("rt_dyn_upcast: not a trait object");
    };
    if !active_heap().is_dyn(id) {
        fatal("rt_dyn_upcast: not a trait object");
    }
    let from = active_heap().dyn_vtable_id(id);
    let to = UPCASTS.with(|t| t.borrow().get(&(from, trait_id)).copied());
    let Some(to) = to else {
        fatal("rt_dyn_upcast: no supertrait vtable registered for this trait object");
    };
    if to == from {
        return encode(value);
    }
    let inner = active_heap().dyn_value(id);
    encode(active_heap().alloc_dyn(to, inner))
}

/// Registers the upcast `(vtable args[0], trait id args[1]) -> vtable
/// args[2]`. Used by AOT startup for the same reason [`rt_vtable_set`] is —
/// the tables have to exist before `main` runs any user code, and there is
/// no interpreter around to fill them; the JIT calls [`upcast_define`]
/// Rust-side instead.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_upcast_set(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_upcast_set: expected 3 arguments (the source vtable id, the trait id and the target vtable id)");
    }
    upcast_define(*args as u32, *args.add(1) as u32, *args.add(2) as u32);
    0
}

/// Sets slot `args[1]` of vtable `args[0]` to the native entry point
/// `args[2]`, extending the table as needed. Used by AOT startup, where the
/// pointers are `ptrtoint` constants the linker resolves; the JIT fills
/// tables through [`vtable_define`] instead.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_vtable_set(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_vtable_set: expected 3 arguments (the vtable id, the slot and the function pointer)");
    }
    let id = *args as usize;
    let slot = *args.add(1) as usize;
    let ptr = *args.add(2) as usize;
    VTABLES.with(|t| {
        let mut t = t.borrow_mut();
        if t.len() <= id {
            t.resize(id + 1, Vec::new());
        }
        let slots = &mut t[id];
        if slots.len() <= slot {
            slots.resize(slot + 1, 0);
        }
        slots[slot] = ptr;
    });
    0
}

#[cfg(test)]
mod tests {
    use typelisp_mem::{Heap, PathId, StrId, SymId, Value};

    use super::{
        active_heap, decode, encode, reset_global_table, rt_car, rt_cdr, rt_cons, rt_global_get, rt_global_new, rt_global_set,
        rt_heap_init, rt_heap_live_count, rt_ping, rt_pop_sexpr_root, rt_push_permanent_sexpr_root, rt_push_sexpr_root, rt_root_count,
        rt_char_equalp, rt_set_car, rt_set_cdr, rt_set_sexpr_root, rt_str_append, rt_str_eq, rt_str_equalp, rt_str_length, rt_str_lt,
        rt_str_new, rt_str_ref, rt_hashtable_contains, rt_hashtable_count, rt_hashtable_entries, rt_hashtable_get_raw,
        rt_hashtable_keys, rt_hashtable_new, rt_hashtable_remove_raw, rt_hashtable_set, rt_data_field, rt_data_new, rt_data_variant,
        rt_cell_get, rt_cell_new, rt_cell_set, rt_closure_env_get, rt_closure_env_len, rt_closure_fnptr, rt_closure_new,
        rt_struct_field_count, rt_struct_field_get, rt_struct_field_set, rt_struct_new, rt_struct_pop_field, rt_struct_push_field, set_active_heap,
    };

    #[test]
    fn rt_ping_adds_one_to_its_first_argument() {
        let args = [41i64];
        assert_eq!(unsafe { rt_ping(args.as_ptr(), 1) }, 42);
    }

    #[test]
    fn rt_ping_returns_zero_with_no_arguments() {
        assert_eq!(unsafe { rt_ping(std::ptr::null(), 0) }, 0);
    }

    #[test]
    fn rt_heap_live_count_reflects_the_registered_heaps_real_state() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);
        assert_eq!(unsafe { rt_heap_live_count(std::ptr::null(), 0) }, 0);

        // Goes through `rt_cons`, not `heap.cons(...)` directly: once `heap`
        // has been registered as the active `Heap` via a raw pointer, every
        // further mutation must go through that same raw-pointer-derived
        // access path (the `rt_*` functions, via `active_heap()`) rather
        // than the original `&mut heap` binding — under Stacked Borrows,
        // reborrowing `heap` directly (as `heap.cons(...)` implicitly does)
        // invalidates the raw pointer `set_active_heap` was given, which
        // `rt_heap_live_count`'s own `active_heap()` call below then trips.
        let cons_args = [encode(Value::Int(1)), encode(Value::Empty)];
        unsafe { rt_cons(cons_args.as_ptr(), 2) };
        assert_eq!(unsafe { rt_heap_live_count(std::ptr::null(), 0) }, 1);
    }

    #[test]
    fn rt_heap_init_registers_a_freshly_created_heap() {
        let args = [4i64];
        assert_eq!(unsafe { rt_heap_init(args.as_ptr(), 1) }, 0);
        assert_eq!(unsafe { rt_heap_live_count(std::ptr::null(), 0) }, 0);

        // `rt_heap_init` deliberately never frees the `Heap` it allocates
        // (see its own doc comment — it must outlive every compiled function
        // in an AOT-linked process, i.e. until the OS reclaims it at exit).
        // That's the right trade-off in production, but this test's process
        // doesn't exit here, so reclaim it explicitly to keep the test
        // itself leak-free under Miri's leak checker rather than suppressing
        // that checker (which would also hide a *real* leak introduced
        // elsewhere in this same test in the future).
        unsafe {
            let heap_ptr = active_heap() as *mut Heap;
            drop(Box::from_raw(heap_ptr));
        }
        set_active_heap(std::ptr::null_mut());
    }

    #[test]
    fn encode_decode_round_trips_every_immediate_variant() {
        for n in [0i64, 1, -1, 42, -42, i64::MIN >> 3, i64::MAX >> 3] {
            assert_eq!(decode(encode(Value::Int(n))), Value::Int(n), "Int({})", n);
        }
        for c in ['a', 'Z', '0', '\u{10FFFF}', '\0'] {
            assert_eq!(decode(encode(Value::Char(c))), Value::Char(c), "Char({:?})", c);
        }
        assert_eq!(decode(encode(Value::Bool(true))), Value::Bool(true));
        assert_eq!(decode(encode(Value::Bool(false))), Value::Bool(false));
        assert_eq!(decode(encode(Value::Empty)), Value::Empty);
        assert_eq!(decode(encode(Value::Symbol(SymId::from_u32(7)))), Value::Symbol(SymId::from_u32(7)));
        assert_eq!(decode(encode(Value::Str(StrId::from_u32(9)))), Value::Str(StrId::from_u32(9)));
        assert_eq!(decode(encode(Value::Path(PathId::from_u32(3)))), Value::Path(PathId::from_u32(3)));
    }

    #[test]
    fn encode_decode_round_trips_a_cons() {
        let mut heap = Heap::with_capacity(8);
        let pair = heap.cons(Value::Int(1), Value::Int(2)).expect("cons failed");
        assert_eq!(decode(encode(pair)), pair);
    }

    #[test]
    fn rt_cons_rt_car_rt_cdr_round_trip_through_a_real_heap() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let car_in = encode(Value::Int(1));
        let cdr_in = encode(Value::Int(2));
        let cons_args = [car_in, cdr_in];
        let pair = unsafe { rt_cons(cons_args.as_ptr(), 2) };

        let one_arg = [pair];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(1));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(2));
    }

    #[test]
    fn rt_set_car_and_rt_set_cdr_mutate_in_place() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let cons_args = [encode(Value::Int(1)), encode(Value::Int(2))];
        let pair = unsafe { rt_cons(cons_args.as_ptr(), 2) };

        let set_car_args = [pair, encode(Value::Int(99))];
        assert_eq!(unsafe { rt_set_car(set_car_args.as_ptr(), 2) }, 0, "set-car returns Unit (0)");
        let set_cdr_args = [pair, encode(Value::Empty)];
        assert_eq!(unsafe { rt_set_cdr(set_cdr_args.as_ptr(), 2) }, 0, "set-cdr returns Unit (0)");

        let one_arg = [pair];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(99));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Empty);
    }

    /// Forces a real `Heap::gc()` (capacity 4, more than 4 conses follow)
    /// while a `precious` cons cell is rooted via [`rt_push_sexpr_root`] —
    /// proving the root genuinely keeps it (and what it points to) alive
    /// across a collection triggered by *other*, unrelated allocations, not
    /// just that the API doesn't crash.
    #[test]
    fn rt_push_sexpr_root_protects_a_value_across_a_gc_triggered_by_other_allocations() {
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        let precious_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let precious = unsafe { rt_cons(precious_args.as_ptr(), 2) };
        let pushed = unsafe { rt_push_sexpr_root(&precious as *const i64, 1) };
        assert_eq!(pushed, precious, "rt_push_sexpr_root returns its argument unchanged");

        // Exhausts the remaining 3 free cells and forces at least one GC;
        // none of these throwaway conses are rooted, so each is garbage by
        // the time the next one runs.
        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let popped = unsafe { rt_pop_sexpr_root(std::ptr::null(), 0) };
        assert_eq!(popped, precious, "the popped root is the same value that was pushed");

        let one_arg = [precious];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(111));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(222));
    }

    /// The negative case `rt_push_sexpr_root`'s test above guards against:
    /// without rooting it, the *same* cons cell gets reclaimed by the GC the
    /// later allocations trigger and reused for one of them — so reading
    /// through the original (now-dangling, from the language's perspective)
    /// tagged value no longer shows what it was consed with. Confirms the
    /// problem [`rt_cons`]'s doc comment describes is real, not
    /// hypothetical, and that the fixed-arena sweep
    /// (`Heap::gc`, lowest-index-first free-list rebuild) makes this
    /// deterministic rather than a flaky one-in-a-while corruption.
    #[test]
    fn an_unrooted_value_is_corrupted_by_a_gc_triggered_by_other_allocations() {
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        let unrooted_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let unrooted = unsafe { rt_cons(unrooted_args.as_ptr(), 2) };
        // Deliberately not pushed as a root.

        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let one_arg = [unrooted];
        let car_after = decode(unsafe { rt_car(one_arg.as_ptr(), 1) });
        let cdr_after = decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) });
        assert_ne!((car_after, cdr_after), (Value::Int(111), Value::Int(222)));
    }

    /// [`rt_push_permanent_sexpr_root`]'s own version of
    /// [`rt_push_sexpr_root_protects_a_value_across_a_gc_triggered_by_other_allocations`]
    /// — protects a value across a forced `gc()` with *no* matching pop at
    /// all (unlike the ordinary root, which the other test still pops at
    /// the end).
    #[test]
    fn rt_push_permanent_sexpr_root_protects_a_value_across_a_gc_with_no_matching_pop() {
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        let precious_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let precious = unsafe { rt_cons(precious_args.as_ptr(), 2) };
        let pushed = unsafe { rt_push_permanent_sexpr_root(&precious as *const i64, 1) };
        assert_eq!(pushed, precious, "rt_push_permanent_sexpr_root returns its argument unchanged");

        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let one_arg = [precious];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(111));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(222));
    }

    /// A permanent root pushed in between an ordinary push/pop pair must
    /// not desync that pair's LIFO accounting — the reason
    /// [`rt_push_permanent_sexpr_root`] feeds a separate `Vec`
    /// (`Heap::push_permanent_root`) rather than the same stack
    /// [`rt_push_sexpr_root`]/[`rt_pop_sexpr_root`] share.
    #[test]
    fn rt_push_permanent_sexpr_root_does_not_desync_an_interleaved_ordinary_root_pop() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let ordinary_args = [encode(Value::Int(1)), encode(Value::Int(1))];
        let ordinary = unsafe { rt_cons(ordinary_args.as_ptr(), 2) };
        unsafe { rt_push_sexpr_root(&ordinary as *const i64, 1) };

        let permanent_args = [encode(Value::Int(2)), encode(Value::Int(2))];
        let permanent = unsafe { rt_cons(permanent_args.as_ptr(), 2) };
        unsafe { rt_push_permanent_sexpr_root(&permanent as *const i64, 1) };

        let popped = unsafe { rt_pop_sexpr_root(std::ptr::null(), 0) };
        assert_eq!(popped, ordinary, "the ordinary root pops, unaffected by the permanent push");
    }

    /// The problem [`rt_set_sexpr_root`] exists to fix, reproduced directly:
    /// a `kind = 2` binding's GC root is pushed once, at bind time, holding
    /// its *original* value (`bind-let-values`'s `rt_push_sexpr_root` call).
    /// `compile-set` (pre-fix) only overwrote the binding's own value slot —
    /// it never touched that root — so a `setf` reassigning the binding to a
    /// freshly built value left the new value with no root at all, exposed
    /// to the very next GC any unrelated allocation triggers. This is
    /// exactly [`an_unrooted_value_is_corrupted_by_a_gc_triggered_by_other_allocations`]'s
    /// scenario, just reached via a stale *existing* root rather than no
    /// root ever having been pushed.
    #[test]
    fn a_setf_reassigned_sexpr_value_is_corrupted_by_a_gc_triggered_by_other_allocations_without_rt_set_sexpr_root() {
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        // `(let ((s (Cons (Int 1) (Int 1)))) ...)` — bind-let-values pushes
        // a root for the *original* value.
        let original_args = [encode(Value::Int(1)), encode(Value::Int(1))];
        let original = unsafe { rt_cons(original_args.as_ptr(), 2) };
        unsafe { rt_push_sexpr_root(&original as *const i64, 1) };

        // `(setf s (Cons (Int 111) (Int 222)))` — overwrites s's own slot
        // with this new value, but (without rt_set_sexpr_root) no root is
        // ever pushed or updated for it.
        let reassigned_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let reassigned = unsafe { rt_cons(reassigned_args.as_ptr(), 2) };

        // Many unrelated allocations, forcing real gc() calls under a tiny
        // heap — same technique as the un-rooted-value test above.
        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let one_arg = [reassigned];
        let car_after = decode(unsafe { rt_car(one_arg.as_ptr(), 1) });
        let cdr_after = decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) });
        assert_ne!(
            (car_after, cdr_after),
            (Value::Int(111), Value::Int(222)),
            "a setf-reassigned value with no updated root is expected to be corrupted -- \
             this test documents the bug rt_set_sexpr_root fixes, see the test below"
        );
    }

    /// [`rt_set_sexpr_root`]'s own fix for the test above: capture the
    /// binding's root-stack index via [`rt_root_count`] right before the
    /// initial [`rt_push_sexpr_root`] (mirroring `compiler.rs`'s
    /// `retain-bindings`/`bind-let-values`), then hand that same index to
    /// `rt_set_sexpr_root` on every `setf` instead of pushing a second root
    /// — the reassigned value now survives the identical GC pressure the
    /// test above shows corrupts it without this fix.
    #[test]
    fn rt_set_sexpr_root_protects_a_setf_reassigned_value_across_a_gc_triggered_by_other_allocations() {
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        let idx = unsafe { rt_root_count(std::ptr::null(), 0) };
        let original_args = [encode(Value::Int(1)), encode(Value::Int(1))];
        let original = unsafe { rt_cons(original_args.as_ptr(), 2) };
        unsafe { rt_push_sexpr_root(&original as *const i64, 1) };

        let reassigned_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let reassigned = unsafe { rt_cons(reassigned_args.as_ptr(), 2) };
        let set_args = [idx, reassigned];
        let returned = unsafe { rt_set_sexpr_root(set_args.as_ptr(), 2) };
        assert_eq!(returned, reassigned, "rt_set_sexpr_root returns its new-value argument unchanged");

        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let one_arg = [reassigned];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(111));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(222));

        let popped = unsafe { rt_pop_sexpr_root(std::ptr::null(), 0) };
        assert_eq!(popped, reassigned, "popping the binding's root now yields the reassigned value, not the original");
    }

    /// Builds a tagged `Value::Str` via [`rt_str_new`] itself, rather than a
    /// direct `heap.alloc_string(...)` call — every test below needs
    /// pre-existing string content to exercise `rt_str_length`/`rt_str_ref`/
    /// `rt_str_eq`/`rt_str_append` against, and (per the existing
    /// `rt_heap_live_count_reflects_the_registered_heaps_real_state` test's
    /// own comment) once a `Heap` is registered via [`set_active_heap`], every
    /// further mutation must go through that same raw-pointer-derived access
    /// path (here, another `rt_*` call) rather than reborrowing the original
    /// `&mut heap` binding directly, which Stacked Borrows treats as
    /// invalidating the raw pointer.
    fn make_str(s: &str) -> i64 {
        let args: Vec<i64> = s.chars().map(|c| c as i64).collect();
        unsafe { rt_str_new(args.as_ptr(), args.len() as u32) }
    }

    #[test]
    fn rt_str_new_builds_a_string_from_raw_char_scalars() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = make_str("hi");
        match decode(tagged) {
            Value::Str(id) => assert_eq!(unsafe { active_heap() }.string(id), "hi"),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    #[test]
    fn rt_str_new_with_no_characters_builds_an_empty_string() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = unsafe { rt_str_new(std::ptr::null(), 0) };
        match decode(tagged) {
            Value::Str(id) => assert_eq!(unsafe { active_heap() }.string(id), ""),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    #[test]
    fn rt_str_length_counts_unicode_scalar_values_not_bytes() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = make_str("héllo");
        assert_eq!(unsafe { rt_str_length([tagged].as_ptr(), 1) }, 5);
    }

    #[test]
    fn rt_str_ref_returns_the_nth_char_as_a_bare_scalar() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = make_str("hi");
        let args = [tagged, 1];
        assert_eq!(unsafe { rt_str_ref(args.as_ptr(), 2) }, 'i' as i64);
    }

    #[test]
    fn rt_str_eq_compares_content_not_identity() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let a = make_str("hi");
        let b = make_str("hi");
        let c = make_str("bye");
        assert_eq!(unsafe { rt_str_eq([a, b].as_ptr(), 2) }, 1, "separately allocated, equal content");
        assert_eq!(unsafe { rt_str_eq([a, c].as_ptr(), 2) }, 0);
    }

    #[test]
    fn rt_str_equalp_is_ascii_case_insensitive() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let a = make_str("Hello");
        let b = make_str("hello");
        let c = make_str("HELLO");
        let d = make_str("world");
        assert_eq!(unsafe { rt_str_equalp([a, b].as_ptr(), 2) }, 1, "differ only in case");
        assert_eq!(unsafe { rt_str_equalp([a, c].as_ptr(), 2) }, 1);
        assert_eq!(unsafe { rt_str_equalp([a, d].as_ptr(), 2) }, 0);
        assert_eq!(unsafe { rt_str_eq([a, b].as_ptr(), 2) }, 0, "plain `eq` still distinguishes case");
    }

    #[test]
    fn rt_char_equalp_is_ascii_case_insensitive() {
        // `char` args are raw code points, not tagged Sexprs — no active heap
        // is needed.
        assert_eq!(unsafe { rt_char_equalp(['A' as i64, 'a' as i64].as_ptr(), 2) }, 1, "A equalp a");
        assert_eq!(unsafe { rt_char_equalp(['A' as i64, 'A' as i64].as_ptr(), 2) }, 1);
        assert_eq!(unsafe { rt_char_equalp(['A' as i64, 'b' as i64].as_ptr(), 2) }, 0);
        assert_eq!(unsafe { rt_char_equalp(['1' as i64, '1' as i64].as_ptr(), 2) }, 1, "non-letters compare by value");
    }

    #[test]
    fn rt_str_lt_orders_lexicographically_by_content() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let a = make_str("abc");
        let b = make_str("abd");
        let a2 = make_str("abc");
        assert_eq!(unsafe { rt_str_lt([a, b].as_ptr(), 2) }, 1);
        assert_eq!(unsafe { rt_str_lt([b, a].as_ptr(), 2) }, 0);
        assert_eq!(unsafe { rt_str_lt([a, a2].as_ptr(), 2) }, 0, "equal content is not strictly less");
    }

    #[test]
    fn rt_str_append_concatenates_into_a_fresh_string() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let a = make_str("foo");
        let b = make_str("bar");
        let tagged = unsafe { rt_str_append([a, b].as_ptr(), 2) };
        match decode(tagged) {
            Value::Str(id) => assert_eq!(unsafe { active_heap() }.string(id), "foobar"),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    #[test]
    fn rt_struct_new_builds_a_boxed_struct_with_its_fields() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let type_name = make_str("point");
        let x = encode(Value::Int(1));
        let y = encode(Value::Int(2));
        let args = [type_name, x, y];
        let tagged = unsafe { rt_struct_new(args.as_ptr(), 3) };

        match decode(tagged) {
            Value::Boxed(id) => {
                let h = unsafe { active_heap() };
                assert_eq!(h.struct_type_name(id), "point");
                assert_eq!(h.struct_field_count(id), 2);
            }
            other => panic!("expected a boxed struct, got {:?}", other),
        }

        let get_x = [tagged, 0];
        let get_y = [tagged, 1];
        assert_eq!(decode(unsafe { rt_struct_field_get(get_x.as_ptr(), 2) }), Value::Int(1));
        assert_eq!(decode(unsafe { rt_struct_field_get(get_y.as_ptr(), 2) }), Value::Int(2));
    }

    #[test]
    fn rt_struct_new_with_no_fields_builds_an_empty_struct() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let type_name = make_str("unit-struct");
        let args = [type_name];
        let tagged = unsafe { rt_struct_new(args.as_ptr(), 1) };

        match decode(tagged) {
            Value::Boxed(id) => assert_eq!(unsafe { active_heap() }.struct_field_count(id), 0),
            other => panic!("expected a boxed struct, got {:?}", other),
        }
    }

    #[test]
    fn rt_data_new_builds_a_boxed_enum_with_its_variant_and_fields() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let type_name = make_str("option");
        let payload = encode(Value::Int(42));
        // args = [type-name, raw variant index, field0]
        let args = [type_name, 0, payload];
        let tagged = unsafe { rt_data_new(args.as_ptr(), 3) };

        match decode(tagged) {
            Value::Boxed(id) => {
                let h = unsafe { active_heap() };
                assert!(h.is_enum(id));
                assert!(!h.is_struct(id), "an enum box must not read as a struct");
                assert_eq!(h.enum_type_name(id), "option");
                assert_eq!(h.enum_variant(id), 0);
                assert_eq!(h.enum_field_count(id), 1);
            }
            other => panic!("expected a boxed enum, got {:?}", other),
        }

        assert_eq!(unsafe { rt_data_variant([tagged].as_ptr(), 1) }, 0);
        assert_eq!(decode(unsafe { rt_data_field([tagged, 0].as_ptr(), 2) }), Value::Int(42));
    }

    #[test]
    fn rt_data_new_with_no_fields_builds_a_nullary_variant() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let type_name = make_str("option");
        let args = [type_name, 1];
        let tagged = unsafe { rt_data_new(args.as_ptr(), 2) };

        match decode(tagged) {
            Value::Boxed(id) => {
                let h = unsafe { active_heap() };
                assert_eq!(h.enum_variant(id), 1);
                assert_eq!(h.enum_field_count(id), 0);
            }
            other => panic!("expected a boxed enum, got {:?}", other),
        }
        assert_eq!(unsafe { rt_data_variant([tagged].as_ptr(), 1) }, 1);
    }

    /// A rooted enum box keeps its heap-referencing field alive across a
    /// collection — the mark-phase fan-out `push_boxed_nested`'s `Enum` arm
    /// provides, mirroring what a struct's fields already get.
    #[test]
    fn a_rooted_enum_box_keeps_its_cons_field_alive_across_gc() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let cell = heap.cons(Value::Int(7), Value::Empty).expect("cons");
        let type_name = make_str("option");
        let args = [type_name, 0, encode(cell)];
        let tagged = unsafe { rt_data_new(args.as_ptr(), 3) };
        let boxed = decode(tagged);

        heap.push_root(boxed);
        heap.gc();

        let id = match boxed {
            Value::Boxed(id) => id,
            other => panic!("expected a boxed enum, got {:?}", other),
        };
        match unsafe { active_heap() }.enum_field(id, 0) {
            Value::Cons(_) => {
                assert_eq!(heap.car(unsafe { active_heap() }.enum_field(id, 0)).expect("car"), Value::Int(7));
            }
            other => panic!("expected the cons field to survive, got {:?}", other),
        }
    }

    #[test]
    fn rt_closure_new_round_trips_fnptr_mask_and_slots() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        // slot 0 (masked): a tagged int; slot 1 (unmasked): raw float bits,
        // stored and handed back verbatim.
        let tagged_int = encode(Value::Int(7));
        let raw_bits = (1.5f64).to_bits() as i64;
        let fn_ptr = 0x1000i64; // opaque at this layer — only stored
        let args = [fn_ptr, 0b01, tagged_int, raw_bits];
        let clo = unsafe { rt_closure_new(args.as_ptr(), 4) };

        match decode(clo) {
            Value::Boxed(id) => {
                let h = unsafe { active_heap() };
                assert!(h.is_compiled_closure(id));
                assert_eq!(h.compiled_closure_mask(id), 0b01);
            }
            other => panic!("expected a boxed compiled closure, got {:?}", other),
        }
        assert_eq!(unsafe { rt_closure_fnptr([clo].as_ptr(), 1) }, fn_ptr);
        assert_eq!(unsafe { rt_closure_env_len([clo].as_ptr(), 1) }, 2);
        assert_eq!(unsafe { rt_closure_env_get([clo, 0].as_ptr(), 2) }, tagged_int);
        assert_eq!(unsafe { rt_closure_env_get([clo, 1].as_ptr(), 2) }, raw_bits);
    }

    /// A rooted compiled closure keeps its captured cell — and through it
    /// the cell's contents — alive across a collection: the mark-phase
    /// fan-out `push_boxed_nested`'s `CompiledClosure` arm. (Its interpreted
    /// twin `BoxedObj::Closure` has an arm of its own, tracing `params`/
    /// `ret`/`body`/`env`; `tests/mem_test.rs` covers that one.)
    #[test]
    fn a_rooted_compiled_closure_keeps_its_captured_cell_alive_across_gc() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let cons = heap.cons(Value::Int(7), Value::Empty).expect("cons");
        let cell = unsafe { rt_cell_new([encode(cons)].as_ptr(), 1) };
        let clo = unsafe { rt_closure_new([0x1000, 0b1, cell].as_ptr(), 3) };
        let boxed = decode(clo);

        heap.push_root(boxed);
        heap.gc();

        let got_cell = unsafe { rt_closure_env_get([clo, 0].as_ptr(), 2) };
        assert_eq!(got_cell, cell, "the captured cell slot survives verbatim");
        let contents = unsafe { rt_cell_get([got_cell].as_ptr(), 1) };
        assert_eq!(heap.car(decode(contents)).expect("car"), Value::Int(7));
    }

    #[test]
    fn rt_cell_set_mutates_the_shared_cell_both_worlds_see() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let cell = unsafe { rt_cell_new([encode(Value::Int(1))].as_ptr(), 1) };
        assert_eq!(unsafe { rt_cell_get([cell].as_ptr(), 1) }, encode(Value::Int(1)));
        assert_eq!(unsafe { rt_cell_set([cell, encode(Value::Int(2))].as_ptr(), 2) }, 0, "cell-set returns Unit (0)");
        assert_eq!(unsafe { rt_cell_get([cell].as_ptr(), 1) }, encode(Value::Int(2)));

        // The very same heap object is what the interpreter's own cell API
        // sees — a compiled `setf` and an interp `setf` mutate one binding.
        let id = match decode(cell) {
            Value::Boxed(id) => id,
            other => panic!("expected a boxed cell, got {:?}", other),
        };
        let h = unsafe { active_heap() };
        assert!(h.is_cell(id));
        assert_eq!(h.cell_get(id), Value::Int(2));
        h.cell_set(id, Value::Int(3));
        assert_eq!(unsafe { rt_cell_get([cell].as_ptr(), 1) }, encode(Value::Int(3)));
    }

    /// An unreachable compiled closure is swept like any other box. (Neither
    /// closure box needs a swept-token report to an interpreter side table
    /// any more: `CompiledClosure` carries its env inline, and the
    /// interpreted `BoxedObj::Closure` carries its own code as heap data. The
    /// whole dead-token mechanism went with the side table.)
    #[test]
    fn an_unreachable_compiled_closure_is_swept() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let clo = unsafe { rt_closure_new([0x1000, 0].as_ptr(), 2) };
        let id = match decode(clo) {
            Value::Boxed(id) => id,
            other => panic!("expected a boxed compiled closure, got {:?}", other),
        };
        heap.gc(); // nothing roots it

        assert!(!heap.is_compiled_closure(id), "the unrooted closure box was swept");
    }

    #[test]
    fn rt_struct_field_set_mutates_in_place() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let type_name = make_str("point");
        let args = [type_name, encode(Value::Int(1)), encode(Value::Int(2))];
        let tagged = unsafe { rt_struct_new(args.as_ptr(), 3) };

        let set_args = [tagged, 0, encode(Value::Int(99))];
        assert_eq!(unsafe { rt_struct_field_set(set_args.as_ptr(), 3) }, 0, "field-set returns Unit (0)");

        let get_x = [tagged, 0];
        let get_y = [tagged, 1];
        assert_eq!(decode(unsafe { rt_struct_field_get(get_x.as_ptr(), 2) }), Value::Int(99));
        assert_eq!(decode(unsafe { rt_struct_field_get(get_y.as_ptr(), 2) }), Value::Int(2), "the other field is untouched");
    }

    #[test]
    fn rt_struct_field_count_returns_the_raw_field_count() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let empty = unsafe { rt_struct_new([make_str("vector")].as_ptr(), 1) };
        assert_eq!(unsafe { rt_struct_field_count([empty].as_ptr(), 1) }, 0, "empty vector has length 0");

        let three = [make_str("vector"), encode(Value::Int(10)), encode(Value::Int(20)), encode(Value::Int(30))];
        let tagged = unsafe { rt_struct_new(three.as_ptr(), 4) };
        assert_eq!(unsafe { rt_struct_field_count([tagged].as_ptr(), 1) }, 3, "raw count, not a tagged Sexpr Int");
    }

    #[test]
    fn rt_hashtable_new_set_count_and_entries_round_trip() {
        let mut heap = Heap::with_capacity(64);
        set_active_heap(&mut heap as *mut Heap);

        let ht = unsafe { rt_hashtable_new(std::ptr::null(), 0) };
        assert_eq!(unsafe { rt_hashtable_count([ht].as_ptr(), 1) }, 0, "a fresh map is empty");

        // Two int->int entries (keys/values as raw tagged `Sexpr` ints).
        assert_eq!(unsafe { rt_hashtable_set([ht, encode(Value::Int(1)), encode(Value::Int(10))].as_ptr(), 3) }, 0);
        assert_eq!(unsafe { rt_hashtable_set([ht, encode(Value::Int(2)), encode(Value::Int(20))].as_ptr(), 3) }, 0);
        // Overwriting an existing key doesn't grow the count.
        assert_eq!(unsafe { rt_hashtable_set([ht, encode(Value::Int(1)), encode(Value::Int(99))].as_ptr(), 3) }, 0);
        assert_eq!(unsafe { rt_hashtable_count([ht].as_ptr(), 1) }, 2, "two distinct keys");

        // `keys` builds a `Vector` whose field count matches the entry count.
        let keys = unsafe { rt_hashtable_keys([ht].as_ptr(), 1) };
        match decode(keys) {
            Value::Boxed(id) => assert_eq!(unsafe { active_heap() }.struct_field_count(id), 2),
            other => panic!("expected a boxed vector, got {:?}", other),
        }

        // `entries` builds a `Vector` of `cons-cell`s, one per entry.
        let entries = unsafe { rt_hashtable_entries([ht].as_ptr(), 1) };
        match decode(entries) {
            Value::Boxed(id) => {
                let h = unsafe { active_heap() };
                assert_eq!(h.struct_field_count(id), 2, "one cons-cell per entry");
                // Each element is itself a boxed cons-cell (2 fields: car, cdr).
                match h.struct_field(id, 0) {
                    Value::Boxed(cell) => assert_eq!(h.struct_field_count(cell), 2, "a cons-cell has car and cdr"),
                    other => panic!("expected a boxed cons-cell, got {:?}", other),
                }
            }
            other => panic!("expected a boxed vector, got {:?}", other),
        }
    }

    #[test]
    fn rt_hashtable_contains_get_raw_and_remove_raw_round_trip() {
        let mut heap = Heap::with_capacity(64);
        set_active_heap(&mut heap as *mut Heap);

        let ht = unsafe { rt_hashtable_new(std::ptr::null(), 0) };
        let key = encode(Value::Int(7));
        let val = encode(Value::Int(70));

        assert_eq!(unsafe { rt_hashtable_contains([ht, key].as_ptr(), 2) }, 0, "not present yet");

        unsafe { rt_hashtable_set([ht, key, val].as_ptr(), 3) };
        assert_eq!(unsafe { rt_hashtable_contains([ht, key].as_ptr(), 2) }, 1, "present after set");
        assert_eq!(decode(unsafe { rt_hashtable_get_raw([ht, key].as_ptr(), 2) }), Value::Int(70));

        assert_eq!(decode(unsafe { rt_hashtable_remove_raw([ht, key].as_ptr(), 2) }), Value::Int(70), "remove returns the removed value");
        assert_eq!(unsafe { rt_hashtable_contains([ht, key].as_ptr(), 2) }, 0, "gone after remove");
    }

    #[test]
    fn rt_struct_push_field_grows_the_struct_and_appends_at_the_end() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = unsafe { rt_struct_new([make_str("vector")].as_ptr(), 1) };
        assert_eq!(unsafe { rt_struct_push_field([tagged, encode(Value::Int(7))].as_ptr(), 2) }, 0, "push returns Unit (0)");
        assert_eq!(unsafe { rt_struct_push_field([tagged, encode(Value::Int(8))].as_ptr(), 2) }, 0);

        assert_eq!(unsafe { rt_struct_field_count([tagged].as_ptr(), 1) }, 2, "grew from 0 to 2");
        assert_eq!(decode(unsafe { rt_struct_field_get([tagged, 0].as_ptr(), 2) }), Value::Int(7));
        assert_eq!(decode(unsafe { rt_struct_field_get([tagged, 1].as_ptr(), 2) }), Value::Int(8), "second push lands at index 1");
    }

    #[test]
    fn rt_struct_pop_field_shrinks_the_struct_and_returns_the_last_element() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = unsafe { rt_struct_new([make_str("vector")].as_ptr(), 1) };
        unsafe { rt_struct_push_field([tagged, encode(Value::Int(7))].as_ptr(), 2) };
        unsafe { rt_struct_push_field([tagged, encode(Value::Int(8))].as_ptr(), 2) };

        assert_eq!(decode(unsafe { rt_struct_pop_field([tagged].as_ptr(), 1) }), Value::Int(8), "pop returns the last-pushed element");
        assert_eq!(unsafe { rt_struct_field_count([tagged].as_ptr(), 1) }, 1, "shrank from 2 to 1");
        assert_eq!(decode(unsafe { rt_struct_pop_field([tagged].as_ptr(), 1) }), Value::Int(7));
        assert_eq!(unsafe { rt_struct_field_count([tagged].as_ptr(), 1) }, 0, "empty after popping both elements");
        // Popping past empty aborts the process (panicking across an `extern
        // "C"` boundary can't unwind — the same "abort, don't unwind"
        // convention every other `rt_*` bounds violation follows, see
        // `compiler.rs`'s `compile-vector-op` doc comment), so it isn't
        // exercised here with `#[should_panic]`; [`Heap::struct_pop_field`]'s
        // own doc comment covers the emptiness check.
    }

    /// A struct field that itself holds a cons must survive a GC the same
    /// way a cons reachable through *another* cons's `car`/`cdr` does — the
    /// direct test that `Heap::gc`'s mark phase actually traces into a
    /// `BoxedObj::Struct`'s fields (`push_boxed_nested`), not just marks the
    /// box slot itself and stops.
    #[test]
    fn gc_traces_into_a_rooted_structs_fields() {
        let mut heap = Heap::with_capacity(16);
        set_active_heap(&mut heap as *mut Heap);

        let inner_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let inner_cons = unsafe { rt_cons(inner_args.as_ptr(), 2) };

        let type_name = make_str("wrapper");
        let struct_args = [type_name, inner_cons];
        let boxed = unsafe { rt_struct_new(struct_args.as_ptr(), 2) };
        unsafe { rt_push_sexpr_root(&boxed as *const i64, 1) };

        // Exhaust the remaining free cells with unrelated garbage to force a
        // real GC.
        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let get_field = [boxed, 0];
        let field = unsafe { rt_struct_field_get(get_field.as_ptr(), 2) };
        let one_arg = [field];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(111));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(222));
    }

    /// The negative-case counterpart to the test above — an *unrooted*
    /// struct's field-held cons is not protected from a GC triggered by
    /// other allocations, confirming the previous test's positive result
    /// isn't a coincidence of a GC never actually running.
    #[test]
    fn gc_reclaims_an_unrooted_structs_fields() {
        let mut heap = Heap::with_capacity(16);
        set_active_heap(&mut heap as *mut Heap);

        let inner_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let inner_cons = unsafe { rt_cons(inner_args.as_ptr(), 2) };

        let type_name = make_str("wrapper");
        let struct_args = [type_name, inner_cons];
        let _boxed = unsafe { rt_struct_new(struct_args.as_ptr(), 2) };
        // Deliberately not rooted.

        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }
        unsafe { active_heap() }.gc();

        // The struct itself is unreachable, so its box slot was recycled by
        // the sweep above — and with it, the only path that kept the
        // field's cons cell (already exercised as garbage by the unrelated
        // allocations, but this pins down the box side specifically).
        assert_eq!(unsafe { active_heap() }.box_count(), 0, "the unrooted struct was reclaimed");
    }

    /// Round-trips a global variable through `rt_global_new`/`rt_global_get`
    /// — the id `rt_global_new` returns is what later `rt_global_get`/
    /// `rt_global_set` calls address it by.
    #[test]
    fn rt_global_new_and_rt_global_get_round_trip() {
        reset_global_table();
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let initial = [encode(Value::Int(42))];
        let id = unsafe { rt_global_new(initial.as_ptr(), 1) };
        assert_eq!(id, 0, "the first global on a freshly reset table gets id 0");

        let id_args = [id];
        assert_eq!(decode(unsafe { rt_global_get(id_args.as_ptr(), 1) }), Value::Int(42));
    }

    /// `rt_global_set` overwrites a global's value in place; a later
    /// `rt_global_get` sees the new value.
    #[test]
    fn rt_global_set_overwrites_in_place() {
        reset_global_table();
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let initial = [encode(Value::Int(1))];
        let id = unsafe { rt_global_new(initial.as_ptr(), 1) };

        let set_args = [id, encode(Value::Int(99))];
        let returned = unsafe { rt_global_set(set_args.as_ptr(), 2) };
        assert_eq!(decode(returned), Value::Int(99), "rt_global_set returns its new value unchanged");

        let id_args = [id];
        assert_eq!(decode(unsafe { rt_global_get(id_args.as_ptr(), 1) }), Value::Int(99));
    }

    /// A global's id (`rt_global_new`'s return value) is a small sequential
    /// counter driven purely by `rt_global_new` call order — an unrelated
    /// permanent-root push (`rt_push_permanent_sexpr_root`, what ordinary
    /// boxed-struct field construction uses) interleaved between two
    /// globals must not perturb that numbering, even though both features
    /// share the same underlying `Heap::permanent_roots` storage.
    #[test]
    fn rt_global_ids_are_sequential_regardless_of_interleaved_permanent_roots() {
        reset_global_table();
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let first = [encode(Value::Int(10))];
        let first_id = unsafe { rt_global_new(first.as_ptr(), 1) };

        let unrelated = [encode(Value::Int(-1))];
        unsafe { rt_push_permanent_sexpr_root(unrelated.as_ptr(), 1) };

        let second = [encode(Value::Int(20))];
        let second_id = unsafe { rt_global_new(second.as_ptr(), 1) };

        assert_eq!((first_id, second_id), (0, 1));
        let first_args = [first_id];
        let second_args = [second_id];
        assert_eq!(decode(unsafe { rt_global_get(first_args.as_ptr(), 1) }), Value::Int(10));
        assert_eq!(decode(unsafe { rt_global_get(second_args.as_ptr(), 1) }), Value::Int(20));
    }

    /// A global survives a forced `gc()` triggered by unrelated allocations
    /// with no matching pop at all — the same permanent-root protection
    /// [`rt_push_permanent_sexpr_root_protects_a_value_across_a_gc_with_no_matching_pop`]
    /// proves for its own direct use of `Heap::push_permanent_root`.
    #[test]
    fn rt_global_protects_its_value_across_a_gc_triggered_by_other_allocations() {
        reset_global_table();
        let mut heap = Heap::with_capacity(4);
        set_active_heap(&mut heap as *mut Heap);

        let precious_args = [encode(Value::Int(111)), encode(Value::Int(222))];
        let precious = unsafe { rt_cons(precious_args.as_ptr(), 2) };
        let initial = [precious];
        let id = unsafe { rt_global_new(initial.as_ptr(), 1) };

        for i in 0..20 {
            let args = [encode(Value::Int(i)), encode(Value::Int(i))];
            unsafe { rt_cons(args.as_ptr(), 2) };
        }

        let id_args = [id];
        let surviving = unsafe { rt_global_get(id_args.as_ptr(), 1) };
        let one_arg = [surviving];
        assert_eq!(decode(unsafe { rt_car(one_arg.as_ptr(), 1) }), Value::Int(111));
        assert_eq!(decode(unsafe { rt_cdr(one_arg.as_ptr(), 1) }), Value::Int(222));
    }
}
