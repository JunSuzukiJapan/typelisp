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
fn encode(v: Value) -> i64 {
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

/// The inverse of [`encode`].
fn decode(tagged: i64) -> Value {
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

#[cfg(test)]
mod tests {
    use typelisp_mem::{Heap, PathId, StrId, SymId, Value};

    use super::{decode, encode, rt_car, rt_cdr, rt_cons, rt_heap_init, rt_heap_live_count, rt_ping, rt_set_car, rt_set_cdr, set_active_heap};

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

        heap.cons(Value::Int(1), Value::Empty).expect("cons failed");
        assert_eq!(unsafe { rt_heap_live_count(std::ptr::null(), 0) }, 1);
    }

    #[test]
    fn rt_heap_init_registers_a_freshly_created_heap() {
        let args = [4i64];
        assert_eq!(unsafe { rt_heap_init(args.as_ptr(), 1) }, 0);
        assert_eq!(unsafe { rt_heap_live_count(std::ptr::null(), 0) }, 0);
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
}
