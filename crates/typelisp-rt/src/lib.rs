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

use std::cell::Cell;

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
/// same trade-off `ClosureBox`/`RtValue::HashTable` already accept for
/// values that outlive their last explicit owner.
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

use typelisp_mem::{ConsRef, PathId, StrId, SymId, Value};

const TAG_BITS: i64 = 3;
const TAG_MASK: i64 = 0b111;

// Stage 2's tag table (`docs/TODO.md`): 8 tags in the low 3 bits. `Nil`/
// `Bool` share one "immediate constant" tag (`TAG_IMMEDIATE`) since `Value`
// has 9 variants but only 8 tag slots — see that doc for the full rationale
// (why this needs no more than 3 bits, the alignment argument for `Cons`
// pointers, etc.).
const TAG_FIXNUM: i64 = 0b000;
const TAG_CONS: i64 = 0b001;
const TAG_SYMBOL: i64 = 0b010;
const TAG_STR: i64 = 0b011;
const TAG_CHAR: i64 = 0b100;
const TAG_PATH: i64 = 0b101;
const TAG_IMMEDIATE: i64 = 0b110;
const TAG_FLOAT: i64 = 0b111;

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
/// uses for a `Sexpr`. `Value::Float` isn't representable yet — it needs
/// heap-boxing (a `ClosureBox`-style malloc+refcount allocation), out of
/// scope for Stage 3 (see `docs/TODO.md`); callers that might encounter a
/// `Sexpr` float should not reach this function yet.
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
        Value::Float(_) => fatal("encode: Sexpr Float is not yet representable in compiled code (boxing not implemented)"),
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
        TAG_FLOAT => fatal("decode: Sexpr Float is not yet representable in compiled code (boxing not implemented)"),
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

// ---- Trait-call dispatch -------------------------------------------------

/// `compiler.rs`'s `compile-trait-dispatch` calls this once every candidate
/// implementation's type-id test has failed to match the receiver's own —
/// provably unreachable for a well-typed program (`Checker::check_instance_method`'s
/// call-site check already guarantees every argument's concrete type
/// implements the bound trait), so this is a trap for a `compiler.rs`/checker
/// bug, not a normal/recoverable runtime condition, exactly mirroring
/// [`rt_match_fail`]'s role for an exhausted `match`.
///
/// # Safety
///
/// None beyond the ordinary compiled-function-ABI contract — like
/// [`rt_match_fail`], never touches the active `Heap`.
#[no_mangle]
pub unsafe extern "C" fn rt_trait_call_fail(_args: *const i64, _argc: u32) -> i64 {
    fatal("trait-call: no implementation matched the receiver's type (the checker should have guaranteed one does)")
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
/// ordinary `const-i64` operand (`ast_bridge` translates `Expr::Str` into a
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

#[cfg(test)]
mod tests {
    use typelisp_mem::{Heap, PathId, StrId, SymId, Value};

    use super::{
        active_heap, decode, encode, rt_car, rt_cdr, rt_cons, rt_heap_init, rt_heap_live_count, rt_ping, rt_pop_sexpr_root,
        rt_push_permanent_sexpr_root, rt_push_sexpr_root, rt_root_count, rt_set_car, rt_set_cdr, rt_set_sexpr_root, rt_str_append,
        rt_str_eq, rt_str_length, rt_str_new, rt_str_ref, set_active_heap,
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
}
