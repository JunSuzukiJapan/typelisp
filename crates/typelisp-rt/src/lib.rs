//! The shared Rust-only runtime library compiled code calls into directly —
//! the destination for builtins that genuinely can't be written in typelisp
//! (direct cons-heap access, raw GC-heap bookkeeping; see
//! `docs/dev/implementation-log.md`, "Sexpr表現 + Match/Construct/共有Rust
//! ライブラリ 実装計画" section, for the full plan this crate is Stage 0/1 of).
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
//! call wiring (JIT: the `externals` list of
//! `typelisp::compile::CompiledFn::new`; AOT: ordinary linker symbol
//! resolution against this crate's `staticlib`, see `typelisp::compile::aot`'s
//! `write_executable`) treats a call to one of these exactly like a call to
//! another already-compiled typelisp function — no new call mechanism is
//! needed, only `#[no_mangle]` so the linker sees a plain, unmangled C
//! symbol name.
//!
//! `rt_ping` is Stage 0's deliberately trivial placeholder: it exists only
//! to prove that *some* Rust function defined in this crate is callable
//! from both a JIT-compiled function (via its `externals`) and an
//! AOT-linked native executable (via the system linker) before any real
//! heap/Sexpr machinery is built on top.

// The printer's shims (`rt_format`/`rt_print`/...) and the reader's
// (`rt_read`) are defined in `typelisp-print` and `typelisp-read`, not here,
// so that a program which never prints does not link the directive engine and
// one which never reads does not link the reader — see those crates' doc
// comments for the measurements.
//
// But `compile-file` links exactly one archive, *this* crate's `staticlib`,
// and rustc leaves an entirely unreferenced dependency out of it (measured: 0
// `typelisp_print` members in `libtypelisp_rt.a` without the line below, 117
// with it). These two re-exports are that reference. They generate no code and
// have no callers on purpose — deleting them as dead would make every
// `println` and every `read` in a compiled program fail to link.
pub use typelisp_print;
pub use typelisp_read;

pub mod coroutine;

pub mod equality;
pub mod c_mem;
pub mod ffi_callback;
pub mod integer;
pub mod net;
pub mod net_builtin;
pub mod x509;
pub mod os;
pub mod readtable;
pub mod sched;
pub mod shared;
pub mod stream;
pub mod stream_builtin;
pub mod sys_builtin;

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

use typelisp_mem::Heap;

// The compiled calling convention — the tagged-word encoding, the active
// `Heap`, and how a shim fails — lives one crate down, in `typelisp-abi`, so
// that `typelisp-print` can define its own shims without depending on this
// crate (which depends on *it*). Re-exported rather than merely imported: the
// `typelisp` crate reaches for several of these as `typelisp_rt::...`, which
// is where they have always been from its point of view.
pub use typelisp_abi::{
    active_heap, decode, encode, fatal, install_quiet_panic_hook, raise, set_active_heap, tagged_arg,
    unwind_interpreted_error, CompiledPanic, CompiledThrow, InterpretedUnwind,
};


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

use typelisp_mem::{BoxId, TypeKeyId, Value};

/// `(cons car cdr)` for compiled code.
///
/// Like the interpreter's own `cons` builtin, this may run a GC
/// (`Heap::cons` does so internally when its free list is empty), and
/// nothing here pushes the calling compiled frame's other live `Sexpr`
/// values onto [`active_heap`]'s root set. That is the *caller's* job now
/// and the island does it: `compile-*` brackets a compiled frame's live
/// values with `rt_push_sexpr_root`/`rt_pop_sexpr_root` (34 call sites in
/// `src/compiler.rs`), and `tests/compiled_binding_gc_test.rs` holds it
/// down. Stage 4 of the representation plan, long landed — this comment
/// used to say the rooting was still missing.
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

/// `(sexpr-car c)` for compiled code. A `c` that is neither a cons nor the
/// empty list raises the panic the interpreter's `sexpr-car` raises: a
/// `Sexpr`'s shape is a run-time fact, so a macro body handed a malformed
/// form gets here, and that is the program's error rather than the
/// runtime's.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_car(args: *const i64, argc: u32) -> i64 {
    cons_half(args, argc, "sexpr-car", Heap::car)
}

/// `(cdr c)` for compiled code — see [`rt_car`]'s doc comment.
///
/// # Safety
///
/// Same as [`rt_car`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_cdr(args: *const i64, argc: u32) -> i64 {
    cons_half(args, argc, "sexpr-cdr", Heap::cdr)
}

/// The body of [`rt_car`] and [`rt_cdr`], which differ only in the half they
/// read.
///
/// The empty list's `car` and `cdr` are the empty list, as in CL. Both sides
/// are `Option<Sexpr>` now, so this is `none` in and `none` out — and it has
/// to agree with the interpreter's `sexpr-car`/`sexpr-cdr`, or the same walk
/// would end differently depending on whether its caller was compiled.
unsafe fn cons_half(
    args: *const i64,
    argc: u32,
    who: &str,
    half: fn(&Heap, Value) -> Result<Value, typelisp_mem::Error>,
) -> i64 {
    if argc < 1 {
        fatal(&format!("{}: expected 1 argument", who));
    }
    let arg = decode(*args);
    if arg.is_empty() {
        return encode(arg);
    }
    match half(active_heap(), arg) {
        Ok(v) => encode(v),
        Err(_) => raise(format!("{}: not a cons", who)),
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
    set_cons_half(args, argc, "rt_set_car", Heap::set_car)
}

/// `(rplacd c val)` for compiled code — see [`rt_set_car`]'s doc comment.
///
/// # Safety
///
/// Same as [`rt_set_car`].
#[no_mangle]
pub unsafe extern "C" fn rt_set_cdr(args: *const i64, argc: u32) -> i64 {
    set_cons_half(args, argc, "rt_set_cdr", Heap::set_cdr)
}

/// The body of [`rt_set_car`] and [`rt_set_cdr`], which differ only in the
/// half they write.
unsafe fn set_cons_half(
    args: *const i64,
    argc: u32,
    who: &str,
    set: fn(&mut Heap, Value, Value) -> Result<(), typelisp_mem::Error>,
) -> i64 {
    if argc < 2 {
        fatal(&format!("{}: expected 2 arguments", who));
    }
    let c = decode(*args);
    let val = decode(*args.add(1));
    match set(active_heap(), c, val) {
        Ok(()) => 0,
        Err(_) => fatal(&format!("{}: first argument is not a cons", who)),
    }
}

// ---- sexpr-* island accessors (closure unification Stage 8) -------------
//
// The rest of the `sexpr-*` island layer beyond `car`/`cdr`/`cons`: the tag
// predicates (`sexpr-consp`/`sexpr-null`/`sexpr-atom`/`sexpr-symp`) and the
// typed payload extractors (`sexpr-i32`/`sexpr-bool`/`sexpr-char`/
// `sexpr-str`/`sexpr-sym-name`; `sexpr-f64` reuses [`rt_f64_value`],
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

/// `(sexpr-int x)`: an `int` node's value — the tagged word itself, a
/// fixnum or a bignum box, once it has been checked to be one (and
/// canonical). Any other node raises the panic the interpreter's
/// `sexpr-int` raises; a non-canonical bignum is the runtime's own fault and
/// stays fatal.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_int(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_int: expected 1 argument");
    }
    let w = *args;
    match decode(w) {
        Value::Int(_) => w,
        Value::Boxed(id) if active_heap().is_bignum(id) => {
            if active_heap().bignum_fits_fixnum(id) {
                fatal("sexpr-int: an int boxed as a bignum that fits a fixnum — not canonical");
            }
            w
        }
        _ => raise("sexpr-int: expected an int Sexpr node".to_string()),
    }
}

/// `(sexpr-i32 x)`: the raw normalized word inside an `i32` node — a
/// `NarrowInt` box like the five narrower widths' since `int` took the bare
/// fixnum word.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_i32(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_i32: expected 1 argument");
    }
    sexpr_narrow_value(*args, 32, true)
}

/// `(sexpr-bool x)`: the `Bool` node's payload as compiled `0`/`1`.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_bool(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_bool: expected 1 argument");
    }
    match decode(*args) {
        Value::Bool(b) => i64::from(b),
        _ => raise("sexpr-bool: expected a Bool Sexpr node".to_string()),
    }
}

/// `(sexpr-char x)`: the `Char` node's Unicode scalar value as a raw `i64`
/// — the compiled `char` convention (`compile-char`'s own widened payload).
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_char(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_char: expected 1 argument");
    }
    match decode(*args) {
        Value::Char(c) => c as i64,
        _ => raise("sexpr-char: expected a Char Sexpr node".to_string()),
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
pub unsafe extern "C-unwind" fn rt_sexpr_str(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_str: expected 1 argument");
    }
    match decode(*args) {
        Value::Str(_) => *args,
        _ => raise("sexpr-str: expected a Str Sexpr node".to_string()),
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
pub unsafe extern "C-unwind" fn rt_sym_name(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sym_name: expected 1 argument");
    }
    match decode(*args) {
        Value::Symbol(id) => {
            let heap = active_heap();
            let name = heap.symbol_name(id).to_string();
            encode(heap.alloc_string(name))
        }
        _ => raise("sexpr-sym-name: expected a Sym Sexpr node".to_string()),
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
// losslessly alongside a 3-bit tag, so `Sexpr::f64` was never actually
// representable in compiled code before this). Unlike `rt_cons`'s `car`/
// `cdr` (already-tagged `Sexpr` values), `rt_f64_new`'s argument is a raw
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
pub unsafe extern "C" fn rt_f64_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_f64_new: expected 1 argument");
    }
    let f = f64::from_bits(*args as u64);
    encode(active_heap().alloc_f64(f))
}

/// [`rt_f64_new`] for an `f32`.
///
/// The argument is still an `f64` bit pattern: a compiled `f32` lives in an
/// `f64` register already rounded to binary32 (`build-fround32`), exactly the
/// way a compiled `u8` lives in an `i64` register already cut to 8 bits. The
/// carrier being wider than the type is not the type being lost — the value
/// *is* the number its type names either way. What changes here is the box,
/// which stores a real `f32` so that every later reader can tell.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_f32_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_f32_new: expected 1 argument");
    }
    let f = f64::from_bits(*args as u64);
    encode(active_heap().alloc_f32(f as f32))
}

/// Reads the `f64` bit pattern out of a boxed `Sexpr` float — `args[0]` is a
/// tagged `Sexpr` value (as [`rt_car`] etc. take), unlike `rt_f64_new`'s
/// raw payload. Fatal if it isn't actually a boxed float — the checker is
/// responsible for guaranteeing that never happens, same convention as
/// [`rt_car`] on a non-cons.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_f64_value(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_f64_value: expected 1 argument");
    }
    match decode(*args) {
        // Positively `is_f64`, not "any box": every other `BoxedObj` kind
        // would make `f64_value` *panic*, and a panic out of this `nounwind`
        // shim aborts the process instead of reporting anything — so the
        // doc'd fatal has to be an explicit test. An `f32` box is refused
        // here too: widening it would be this shim deciding a width its
        // caller already knew.
        Value::Boxed(id) if active_heap().is_f64(id) => active_heap().f64_value(id).to_bits() as i64,
        _ => fatal("rt_f64_value: argument is not a boxed f64"),
    }
}

/// [`rt_f64_value`] for an `f32`, widened into the `f64` bit pattern a
/// compiled `f32` register holds — see [`rt_f32_new`] for why the carrier is
/// wider than the type.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_f32_value(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_f32_value: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) if active_heap().is_f32(id) => {
            f64::from(active_heap().f32_value(id)).to_bits() as i64
        }
        _ => fatal("rt_f32_value: argument is not a boxed f32"),
    }
}

/// `(sexpr-f64 x)`: [`rt_f64_value`] for a program reading a `Sexpr` it
/// does not know the shape of. A node that is not an `f64` raises the
/// interpreter's panic; `rt_f64_value` itself serves typed reads, which the
/// checker guarantees.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_f64(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_f64: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) if active_heap().is_f64(id) => active_heap().f64_value(id).to_bits() as i64,
        _ => raise("sexpr-f64: expected an f64 Sexpr node".to_string()),
    }
}

/// [`rt_sexpr_f64`] for an `f32`, widened as [`rt_f32_value`] widens it.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_f32(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_f32: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) if active_heap().is_f32(id) => f64::from(active_heap().f32_value(id)).to_bits() as i64,
        _ => raise("sexpr-f32: expected an f32 Sexpr node".to_string()),
    }
}

// ---- narrow integers (`i8`/`i16`/`u8`/`u16`/`u32`) ----------------------
//
// The integer half of the float split above, and the same argument: a
// compiled `u8` lives in an `i64` register already cut to 8 bits, so the
// register cannot say which of the five narrow types it is. Inside a
// `Sexpr` — the one place a value has no declared type to be read off — it
// therefore gets a box that says (`BoxedObj::Narrow`). `i32` keeps its bare
// tagged word: that width is what an untagged `Sexpr` integer already means.

/// Allocates a boxed narrow-integer `Sexpr` from a raw register word plus
/// the `wsig` code naming its type — `args[0]` is the value (already
/// normalized by the arithmetic that produced it, and normalized again by
/// `Heap::alloc_narrow` regardless), `args[1]` is `width * 2 + signed`.
///
/// The `wsig` is a *constant* the island emits from the declared type, not
/// something read off the value: the word is the same bit pattern for a `u8`
/// `200` and an `i32` `200`, which is precisely why the box is needed.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_narrow_new(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_narrow_new: expected 2 arguments");
    }
    let (width, signed) = wsig(*args.add(1));
    encode(active_heap().alloc_narrow(width as u8, signed, *args))
}

/// Reads the raw register word out of a boxed narrow-integer `Sexpr` of
/// exactly the type `args[1]`'s `wsig` names — `args[0]` is a tagged `Sexpr`
/// value, as [`rt_f64_value`]'s is.
///
/// Fatal on any other box, the `wsig` included: a `u8` box handed to a
/// `u16` read would be this shim silently widening a type its caller already
/// knew, the same refusal [`rt_f64_value`] makes about an `f32` box.
///
/// # Safety
///
/// Same as [`rt_narrow_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_narrow_value(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_narrow_value: expected 2 arguments");
    }
    let (width, signed) = wsig(*args.add(1));
    narrow_value_of(*args, width as u8, signed, "rt_narrow_value")
}

/// [`rt_narrow_value`]'s body: decode, insist on this exact type, hand back
/// the word. The `sexpr-*` readers use [`sexpr_narrow_value`] instead.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread.
unsafe fn narrow_value_of(v: i64, width: u8, signed: bool, who: &str) -> i64 {
    match decode(v) {
        Value::Boxed(id) => match active_heap().narrow_box(id) {
            Some(n) if n.width == width && n.signed == signed => n.value,
            _ => fatal(&format!("{who}: argument is not a boxed {}{width}", if signed { "i" } else { "u" })),
        },
        _ => fatal(&format!("{who}: argument is not a boxed {}{width}", if signed { "i" } else { "u" })),
    }
}

/// The six fixed-width `sexpr-*` readers' body: [`narrow_value_of`], except
/// that a node of another type raises the interpreter's panic instead of
/// aborting. Which node a `Sexpr` holds is a run-time fact a program can get
/// wrong; a typed read of a narrow box ([`rt_narrow_value`]) has the checker
/// behind it and cannot.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread.
unsafe fn sexpr_narrow_value(v: i64, width: u8, signed: bool) -> i64 {
    if let Value::Boxed(id) = decode(v) {
        if let Some(n) = active_heap().narrow_box(id) {
            if n.width == width && n.signed == signed {
                return n.value;
            }
        }
    }
    let ty = format!("{}{width}", if signed { "i" } else { "u" });
    raise(format!("sexpr-{ty}: expected a {ty} Sexpr node"))
}

/// `(sexpr-i8 x)` and its four siblings: the typed payload extractors for
/// the narrow-integer `Sexpr` variants, alongside `sexpr-i32`/`sexpr-f64`/
/// `sexpr-f32` above. One shim per type rather than one taking a `wsig`,
/// because a builtin accessor is called with exactly its own arguments and
/// the type is in the *name* — `compile-call`'s rename table
/// (`compile::externs`) maps each name to its own symbol.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_i8(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_i8: expected 1 argument");
    }
    sexpr_narrow_value(*args, 8, true)
}

/// [`rt_sexpr_i8`] for `i16`.
///
/// # Safety
///
/// Same as [`rt_sexpr_i8`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_i16(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_i16: expected 1 argument");
    }
    sexpr_narrow_value(*args, 16, true)
}

/// [`rt_sexpr_i8`] for `u8`.
///
/// # Safety
///
/// Same as [`rt_sexpr_i8`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_u8(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_u8: expected 1 argument");
    }
    sexpr_narrow_value(*args, 8, false)
}

/// [`rt_sexpr_i8`] for `u16`.
///
/// # Safety
///
/// Same as [`rt_sexpr_i8`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_u16(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_u16: expected 1 argument");
    }
    sexpr_narrow_value(*args, 16, false)
}

/// [`rt_sexpr_i8`] for `u32`.
///
/// # Safety
///
/// Same as [`rt_sexpr_i8`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_u32(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_sexpr_u32: expected 1 argument");
    }
    sexpr_narrow_value(*args, 32, false)
}

/// Discriminates the numeric boxed `Sexpr` kinds that share `TAG_BOXED`'s
/// one tag: `1` for a boxed `f64`, `4` for a boxed `f32`, `2` for a bignum,
/// `3` for a ratio, `5`/`6`/`7`/`8`/`9` for a boxed `i8`/`i16`/`u8`/`u16`/
/// `u32`, `0` for everything else — *including* non-boxed values, so it's total over
/// every tagged word and `compile-sexpr-tag-test` can call it without a
/// prior tag check (a `kind == 1` result already implies `TAG_BOXED`).
/// `args[0]` is a tagged `Sexpr` value, like [`rt_f64_value`]'s.
///
/// # Safety
///
/// Same as [`rt_f64_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_box_kind(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_box_kind: expected 1 argument");
    }
    match decode(*args) {
        Value::Boxed(id) => {
            let heap = active_heap();
            if heap.is_f64(id) {
                1
            } else if heap.is_f32(id) {
                4
            } else if heap.is_bignum(id) {
                2
            } else if heap.is_ratio(id) {
                3
            } else if let Some(n) = heap.narrow_box(id) {
                // The five narrow integer types get five numbers, not one:
                // this is the test `compile-sexpr-tag-test` uses to decide
                // which `Sexpr` variant a box is, and a `u8` and a `u16` are
                // different variants.
                match (n.width, n.signed) {
                    (8, true) => 5,
                    (16, true) => 6,
                    (8, false) => 7,
                    (16, false) => 8,
                    (32, false) => 9,
                    (32, true) => 10,
                    (w, s) => fatal(&format!("rt_box_kind: a narrow box of an unknown width {}{}", if s { "i" } else { "u" }, w)),
                }
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
// were still missing (`docs/dev/implementation-log.md`'s "`bignum`/`ratio`型の
// compile対応"
// follow-up).
//
// None of these need extra GC-root bookkeeping beyond what the caller
// already does for any tagged argument it passes in: allocating a new
// `BoxedObj` (`box_slots`) never triggers the cons-heap's mark-sweep (that
// only fires on cons-arena exhaustion), the same assumption `rt_struct_new`/
// `rt_f64_new` already rely on above.
//
// Division by zero is checked explicitly (`Sign::NoSign`) before calling
// into `BigInt`/`BigRational`'s `Div`/`Rem` operators, which otherwise raise
// a genuine Rust `panic!` — unwinding that across this `extern "C"` boundary
// is undefined behavior (see `fatal`'s doc comment), unlike an `i64` divide
// by zero, which traps at the machine level instead.

use num_bigint::{BigInt, BigUint, Sign};
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
        // An `int` in either shape: `rt_bignum_new` answers a fixnum for a
        // literal that fits one (a ratio literal's numerator, say).
        Value::Int(n) => BigInt::from(n),
        Value::Boxed(id) => active_heap().bignum_value(id).clone(),
        _ => fatal(&format!("{who}: argument is not an int")),
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
    // Canonical: a literal the checker spelled as `(bignum ..)` is past the
    // fixnum range, but a ratio literal's parts arrive here too and may not
    // be.
    encode(active_heap().int_from_bigint(n))
}

/// Builds a boxed ratio from two already-boxed bignums (`args[0]`=numerator,
/// `args[1]`=denominator) — `BigRational::new` reduces to lowest terms and
/// normalizes the sign, the same invariant every other `ratio` construction
/// path (the reader, `bignum->ratio`, ...) already relies on. Used both for
/// a ratio literal (`compiler.rs`'s `compile-ratio-literal`, over two nested
/// `compile-bignum-literal` calls) and `bignum->ratio`.
///
/// The zero-denominator `fatal` below stays a `fatal` — checked, and it is
/// unreachable from source. The only way to build one is a literal, and the
/// reader rejects `1/0` outright ("ratio literal with zero denominator", as
/// CL's reader does); a value that got here with a zero denominator was
/// mis-built by the compiler, not written by a user.
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

/// `float->int` for compiled code — truncating toward zero into the
/// arbitrary-precision `int` (`BigInt::from_f64`, exact for any finite
/// value, then `Heap::int_from_bigint` for the canonical word). A
/// non-finite input [`raise`]s: there is no integer to truncate to, and the
/// interpreter's `float_to_int` panics the same way.
///
/// # Safety
///
/// `argc` must be `>= 1`, `args[0]` an `f64`'s raw bit pattern; a `Heap` must
/// be registered.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_float_to_int(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_float_to_int: expected 1 argument");
    }
    let f = f64::from_bits(*args as u64);
    match BigInt::from_f64(f) {
        Some(n) => encode(active_heap().int_from_bigint(n)),
        None => raise(format!("float->int: {} is not finite", f)),
    }
}

/// `float->ratio` for compiled code — widening and exact: every finite
/// `f64` is itself an exact dyadic rational (`BigRational::from_float`, CL's
/// `rational` rather than the lossy-round-trip `rationalize`). Fatal on a
/// non-finite input.
///
/// # Safety
///
/// Same as [`rt_float_to_int`].
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

/// `ratio::/` for compiled code. A zero divisor (checked via the divisor's
/// numerator sign — a reduced ratio is zero iff its numerator is, its
/// denominator always being a positive nonzero invariant) [`raise`]s
/// `"divide by zero"`, `eval_ratio_builtin`'s own wording.
///
/// # Safety
///
/// Same as [`ratio_pair`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_ratio_div(args: *const i64, argc: u32) -> i64 {
    let (a, b) = ratio_pair(args, argc, "rt_ratio_div");
    if b.numer().sign() == Sign::NoSign {
        raise("divide by zero".to_string());
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

/// `ratio->int` for compiled code — truncates toward zero
/// (`Ratio::to_integer`).
///
/// # Safety
///
/// `argc` must be `>= 1`, `args[0]` a boxed ratio; a `Heap` must be
/// registered.
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_to_int(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ratio_to_int: expected 1 argument");
    }
    let r = ratio_arg(args, 0, "rt_ratio_to_int");
    encode(active_heap().int_from_bigint(r.to_integer()))
}

/// `ratio->float` for compiled code — widening, rounded.
///
/// # Safety
///
/// Same as [`rt_ratio_to_int`].
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

/// `numerator` for compiled code — the (already-reduced) numerator as an
/// `int`; `ratio` itself is never mutated.
///
/// # Safety
///
/// Same as [`rt_ratio_to_int`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_numerator(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ratio_numerator: expected 1 argument");
    }
    let r = ratio_arg(args, 0, "rt_ratio_numerator");
    encode(active_heap().int_from_bigint(r.numer().clone()))
}

/// `denominator` for compiled code — the `numerator` counterpart.
///
/// # Safety
///
/// Same as [`rt_ratio_to_int`].
#[no_mangle]
pub unsafe extern "C" fn rt_ratio_denominator(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ratio_denominator: expected 1 argument");
    }
    let r = ratio_arg(args, 0, "rt_ratio_denominator");
    encode(active_heap().int_from_bigint(r.denom().clone()))
}

// ---- Sexpr/RtValue unification, Stage 1: boxed objects (Struct) --------
//
// `BoxedObj::Struct` (`typelisp-mem`) is the shared runtime shape behind a
// `defstruct` instance, `Vector<T>`, and `cons-cell<K,V>` alike — see its
// doc comment. This mem/rt-layer plumbing is deliberately unwired from
// `compiler.rs`/the interpreter for now (that's Stage 2/3 of the
// unification plan, per `docs/dev/implementation-log.md`'s "Sexpr/RtValue
// 内部表現統合 実装計画"): these three functions exist so the
// representation itself can be exercised and tested in isolation first.

/// The interned type identity named by a tagged `Sexpr` `Str` argument.
///
/// Compiled code has no `Path`, so it hands these shims the type's *name* —
/// the literal `core_bridge` compiled from `type_key::type_key_of`. This is
/// where that name crosses back into the heap's type-key table, and the one
/// place in this crate where an identity is minted.
///
/// The lookup runs first so the steady state allocates nothing: a type is
/// interned once (by the checker, or by the first value compiled code builds
/// of it) and every later instance finds the id already there.
///
/// # Safety
///
/// `raw` must decode to a `Value::Str`; a `Heap` must be registered on this
/// thread.
unsafe fn type_key_arg(raw: i64, who: &str) -> TypeKeyId {
    if let Some(key) = existing_type_key_arg(raw, who) {
        return key;
    }
    let sid = match decode(raw) {
        Value::Str(id) => id,
        _ => unreachable!("existing_type_key_arg already rejected a non-Str"),
    };
    let heap = active_heap();
    let name = heap.string(sid).to_string();
    heap.intern_type_key(&name)
}

/// The interned identity named by a tagged `Sexpr` `Str` argument, or `None`
/// if this heap has never seen that type — [`type_key_arg`] without the
/// minting.
///
/// What an instance *test* wants: a type no value here has ever had answers
/// the question by itself, and minting an id for it would only grow a
/// permanent table with a name nothing will ever carry.
///
/// # Safety
///
/// Same as [`type_key_arg`].
unsafe fn existing_type_key_arg(raw: i64, who: &str) -> Option<TypeKeyId> {
    let sid = match decode(raw) {
        Value::Str(id) => id,
        _ => fatal(&format!("{}: type name argument is not a Str", who)),
    };
    let heap = active_heap();
    heap.type_key_id(&heap.string(sid))
}

/// `(rt-struct-new type-name field0 field1 ...)` for compiled code —
/// allocates a boxed `Sexpr` struct. `args[0]` is a tagged `Sexpr` `Str`
/// (the struct's type name, interned into a `TypeKeyId` here rather than
/// kept as a live `Sexpr` reference — the box stores the id, not the name);
/// `args[1..argc]` are the
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
    let type_key = type_key_arg(*args, "rt_struct_new");
    let mut fields = Vec::with_capacity(argc as usize - 1);
    for i in 1..argc as isize {
        fields.push(decode(*args.offset(i)));
    }
    encode(active_heap().alloc_struct(type_key, fields))
}

/// The index bound both `rt_struct_field_*` functions check before touching
/// the heap, [`raise`]ing `vector_get`/`vector_set`'s own message when `idx`
/// is outside it.
///
/// The check is here rather than left to the mem layer on purpose: the mem
/// layer's own overrun is a Rust `panic!`, which would cross these
/// functions' FFI boundary as an abort. Catching that panic afterwards is
/// not an option either — the point is to *not* fail, so the bound is tested
/// before the access.
///
/// Both call sites that can actually reach the bound are `Vector<T>`'s
/// `get`/`set` (`compile-vector-op`), whose index is a run-time value; a
/// `defstruct` field access (`compile-field-get`/`compile-field-set`) has a
/// checker-fixed index and never fails here.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread and `id` must name a
/// live boxed struct in it.
unsafe fn checked_field_index(id: BoxId, idx: i64) -> usize {
    if idx < 0 || idx as usize >= active_heap().struct_field_count(id) {
        raise(format!("Vector: index {} out of bounds", idx));
    }
    idx as usize
}

/// `(rt-struct-field-get s idx)` for compiled code — the `idx`-th field of
/// boxed struct `args[0]` (a tagged `Sexpr`), where `args[1]` is a *raw*
/// (untagged) `i64` index, matching [`rt_str_ref`]'s convention for its own
/// raw index argument. Fatal if `args[0]` isn't a boxed struct (a compiler
/// contract violation); an out-of-range `idx` is an ordinary recoverable
/// failure instead — see [`checked_field_index`].
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// the first decoding to a `Value::Boxed` struct; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_struct_field_get(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_struct_field_get: expected 2 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_field_get: first argument is not a boxed Sexpr"),
    };
    let idx = checked_field_index(id, *args.add(1));
    encode(active_heap().struct_field(id, idx))
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
/// enum's type name, interned like [`rt_struct_new`]'s), `args[1]` is the
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
    let type_key = type_key_arg(*args, "rt_data_new");
    let variant = *args.add(1);
    if variant < 0 {
        fatal("rt_data_new: negative variant index");
    }
    let mut fields = Vec::with_capacity(argc as usize - 2);
    for i in 2..argc as isize {
        fields.push(decode(*args.offset(i)));
    }
    encode(active_heap().alloc_enum(type_key, variant as usize, fields))
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
    let Some(type_key) = existing_type_key_arg(*args.add(1), "rt_sexpr_instance_test") else {
        return 0;
    };
    let id = match v {
        Value::Boxed(id) => id,
        _ => return 0,
    };
    let variant = *args.add(2);
    let heap = active_heap();
    let matches = if heap.is_struct(id) {
        heap.struct_type_key(id) == type_key
    } else if heap.is_enum(id) {
        heap.enum_type_key(id) == type_key && (variant < 0 || heap.enum_variant(id) as i64 == variant)
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
    closure_new(args, argc, typelisp_abi::BODY_ABI_CLASSIC, "rt_closure_new")
}

/// [`rt_closure_new`] for a body compiled under the coroutine ABI (Phase C2).
///
/// A separate entry point rather than an extra argument, because the island
/// that compiles the *next* island is the one already committed: it calls
/// `rt_closure_new` with the layout it was built against, and an argument
/// added in the middle would break the bootstrap at the one moment it cannot
/// be repaired from. The name carries the fact instead.
///
/// # Safety
///
/// As [`rt_closure_new`].
#[no_mangle]
pub unsafe extern "C" fn rt_coroutine_closure_new(args: *const i64, argc: u32) -> i64 {
    closure_new(args, argc, typelisp_abi::BODY_ABI_COROUTINE, "rt_coroutine_closure_new")
}

unsafe fn closure_new(args: *const i64, argc: u32, body_abi: u8, what: &str) -> i64 {
    if argc < 2 {
        fatal(&format!("{}: expected at least 2 arguments (fn ptr, sexpr mask)", what));
    }
    let env_len = argc as usize - 2;
    if env_len > 64 {
        fatal(&format!("{}: more than 64 captured slots", what));
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
    encode(active_heap().alloc_compiled_closure(fn_ptr, env, sexpr_mask, body_abi))
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

/// How [`apply_on_this_frame`] re-enters the tree-walking interpreter: the closure
/// as a tagged word, then the very same `(args_ptr, argc)` the compiled
/// callee would have received.
///
/// A function pointer rather than a direct call because this crate depends
/// on `typelisp-mem` alone — the interpreter lives in the `typelisp` crate,
/// which depends on *this* one, so the edge can only run at runtime.
/// `typelisp::eval::Interp` installs it ([`set_apply_interpreted`]) at the
/// same moment it registers the active heap; an AOT-compiled executable has
/// no interpreter to install and leaves it unset.
pub type ApplyInterpretedFn = unsafe extern "C-unwind" fn(closure: i64, args: *const i64, argc: u32) -> i64;

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

/// `(rt-frame-new nslots)` for compiled code — a compiled function's
/// activation record, as a tagged reference to a `BoxedObj::Frame`.
///
/// The argument is a **raw** word, not a tagged one: it is a compile-time
/// constant the island emits with `const-word`, describing the frame's
/// layout rather than being a value in it. There is no mask argument — the
/// frame starts with every slot unmasked and each binding marks its own with
/// `rt_frame_mask_bit`, so the island needs only to have counted the binding
/// sites, not classified them.
///
/// Allocating a frame never itself collects, so the caller may hold unrooted
/// values across this call. **The frame itself must then be rooted**, or the
/// next collection takes it and the pointer `rt_frame_data` handed out dangles.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid `i64`; a `Heap`
/// must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_frame_new: expected 1 argument (slot count)");
    }
    let nslots = *args;
    if nslots < 0 {
        fatal(&format!("rt_frame_new: {} is not a slot count a frame can hold", nslots));
    }
    encode(active_heap().alloc_frame(nslots as usize))
}

/// `(rt-frame-mask-bit f idx)` for compiled code — marks slot `idx` of frame
/// `args[0]` as holding a tagged value, so the collector traces it.
///
/// This stands exactly where the binding's `rt_push_sexpr_root` used to, and
/// costs the same one call — but it is not a push onto a stack that has to be
/// popped in order. The bit says "this slot is a root" for as long as the
/// frame lives, which is the whole point: the frame *is* the root set, so
/// there is no LIFO discipline left to get wrong on an early exit.
///
/// `idx` is a **raw** word (a compile-time slot index); `args[0]` is a tagged
/// reference to the frame.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// the first decoding to a frame; a `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_mask_bit(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_frame_mask_bit: expected 2 arguments (frame, slot index)");
    }
    let idx = *args.add(1);
    if idx < 0 {
        fatal(&format!("rt_frame_mask_bit: {} is not a slot index", idx));
    }
    let heap = active_heap();
    match decode(*args) {
        Value::Boxed(id) if heap.is_frame(id) => {
            heap.set_frame_mask_bit(id, idx as usize);
            0
        }
        other => fatal(&format!("rt_frame_mask_bit: {:?} is not a frame", other)),
    }
}

/// `(rt-frame-data f)` for compiled code — the address of frame `args[0]`'s
/// word array, as an `i64`.
///
/// This is what makes a frame slot usable as a local: the island GEPs into
/// this pointer (`build-slot-ptr`) and the existing `load-raw`/`store-arg`
/// pair does the rest, exactly as it did against an `alloca`.
///
/// **The address is valid only while the frame is live.** Growing the box
/// table moves the box but not the word array's buffer, and the sweep writes
/// its slot back as empty rather than compacting — so the one way to
/// invalidate this is to let the frame be collected.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to 1 valid `i64` holding a
/// tagged reference to a frame; a `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_data(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_frame_data: expected 1 argument");
    }
    let heap = active_heap();
    match decode(*args) {
        Value::Boxed(id) if heap.is_frame(id) => heap.frame_data_ptr(id) as i64,
        other => fatal(&format!("rt_frame_data: {:?} is not a frame", other)),
    }
}

/// `(rt-frame-entered f)` for compiled code — publishes the frame a prologue
/// just allocated, so the driver can reach it on the way back.
///
/// The driver cannot learn this any other way. The frame is made *inside* the
/// callee, after the call has already started, and the ABI's single return
/// value is the status word.
///
/// # Safety
///
/// `argc` must be `>= 1`; `args[0]` is the tagged frame.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_entered(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_frame_entered: expected 1 argument");
    }
    typelisp_abi::call_state::set_current_frame(*args);
    0
}

/// `(rt-pending-argc)` for compiled code — how many arguments the driver left
/// for this entry.
///
/// # Safety
///
/// Takes no arguments; `args`/`argc` are ignored.
#[no_mangle]
pub unsafe extern "C" fn rt_pending_argc(_args: *const i64, _argc: u32) -> i64 {
    typelisp_abi::call_state::pending_argc() as i64
}

/// `(rt-pending-arg i)` for compiled code — one of the arguments the driver
/// left, as the raw word the caller stored.
///
/// `args[0]` is a raw index, not a value in the program.
///
/// # Safety
///
/// `argc` must be `>= 1`.
#[no_mangle]
pub unsafe extern "C" fn rt_pending_arg(args: *const i64, argc: u32) -> i64 {
    use typelisp_abi::call_state::{pending_arg, pending_argc};
    pending_word(args, argc, "rt_pending_arg", "argument", "an argument", pending_arg, pending_argc)
}

/// `(rt-pending-envc)` for compiled code — how many captures the driver left.
///
/// # Safety
///
/// Takes no arguments; `args`/`argc` are ignored.
#[no_mangle]
pub unsafe extern "C" fn rt_pending_envc(_args: *const i64, _argc: u32) -> i64 {
    typelisp_abi::call_state::pending_envc() as i64
}

/// `(rt-pending-env i)` for compiled code — one of the captures the driver
/// left, as the raw word the caller stored.
///
/// The capture counterpart of [`rt_pending_arg`], and separate for the same
/// reason the two lists are: a closure body reads its captures and its
/// parameters by separate indices.
///
/// # Safety
///
/// `argc` must be `>= 1`.
#[no_mangle]
pub unsafe extern "C" fn rt_pending_env(args: *const i64, argc: u32) -> i64 {
    use typelisp_abi::call_state::{pending_env, pending_envc};
    pending_word(args, argc, "rt_pending_env", "capture", "a capture", pending_env, pending_envc)
}

/// The body of [`rt_pending_arg`] and [`rt_pending_env`]: word `args[0]` of
/// one of the driver's two lists, read by `get`, where `count` says how many
/// the driver left. `what` names an entry of that list in the messages, and
/// `a_what` is the same name with its article.
unsafe fn pending_word(
    args: *const i64,
    argc: u32,
    who: &str,
    what: &str,
    a_what: &str,
    get: fn(usize) -> Option<i64>,
    count: fn() -> usize,
) -> i64 {
    if argc < 1 {
        fatal(&format!("{}: expected 1 argument", who));
    }
    let i = *args;
    if i < 0 {
        fatal(&format!("{}: {} is not {} index", who, i, a_what));
    }
    match get(i as usize) {
        Some(w) => w,
        None => fatal(&format!("{}: {} {} was not passed (the driver left {})", who, what, i, count())),
    }
}

/// `(rt-frame-call target arg...)` for compiled code — names the call the
/// driver should make, immediately before the frame returns `STATUS_CALL`.
///
/// `args[0]` is the callee's address (`build-fn-address`); the rest are its
/// arguments, already evaluated and in the callee's declared representations.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to `argc` valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_call(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_frame_call: expected at least 1 argument (the callee)");
    }
    let target = *args as usize;
    let mut passed = Vec::with_capacity(argc as usize - 1);
    for i in 1..argc as usize {
        passed.push(*args.add(i));
    }
    typelisp_abi::call_state::set_pending_call(target, passed, Vec::new());
    0
}

/// `rt_frame_apply(closure, arg...)` — name a function *value* to the driver.
///
/// [`rt_frame_call`] for an `apply`, where the callee is not an address the
/// checker resolved but a value whose nature decides who runs it. The driver
/// resolves it ([`resolve_closure`]): a coroutine body joins this chain, a
/// classic one runs to completion, and an interpreted one goes to whoever
/// owns a continuation stack.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least `argc` valid
/// `i64`s, the first a tagged function value and the rest its arguments in
/// its own declared representations.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_apply(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_frame_apply: expected at least 1 argument (the callee)");
    }
    let closure = *args;
    let mut passed = Vec::with_capacity(argc as usize - 1);
    for i in 1..argc as usize {
        passed.push(*args.add(i));
    }
    typelisp_abi::call_state::set_pending_apply(closure, passed);
    0
}

/// `rt_frame_dyn_call(vtable, slot, arg...)` — name a `:dyn` method to the
/// driver.
///
/// [`rt_frame_apply`] for a callee the vtable names. The arguments start with
/// the *concrete* receiver: the call site unwrapped the trait object before
/// storing it (`compile-dyn-call`), because the method behind the slot
/// expects its own type and knows nothing about the box.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least `argc` valid
/// `i64`s: a vtable id, a slot index, and the method's arguments.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_dyn_call(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_frame_dyn_call: expected at least 2 arguments (vtable id, slot)");
    }
    let vtable = *args as u32;
    let slot = *args.add(1) as u32;
    let mut passed = Vec::with_capacity(argc as usize - 2);
    for i in 2..argc as usize {
        passed.push(*args.add(i));
    }
    typelisp_abi::call_state::set_pending_dyn(vtable, slot, passed);
    0
}

/// `(rt-frame-call-env target env-ptr env-len arg...)` — [`rt_frame_call`] for
/// a callee that also has captures: a `lambda` body, or a `labels` sibling
/// reached under the captures-carrying calling convention.
///
/// `args[1]` is a pointer to `args[2]` captured words, read here and copied,
/// because the array it points at is on the caller's machine stack and the
/// caller is about to return.
///
/// # Safety
///
/// `argc` must be `>= 3`, `args` must point to `argc` valid `i64`s, and
/// `args[1]` must point to `args[2]` valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_call_env(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_frame_call_env: expected at least 3 arguments (callee, captures, capture count)");
    }
    let target = *args as usize;
    let env_ptr = *args.add(1) as usize as *const i64;
    let env_len = *args.add(2);
    if env_len < 0 {
        fatal(&format!("rt_frame_call_env: {} is not a capture count", env_len));
    }
    let mut env = Vec::with_capacity(env_len as usize);
    for i in 0..env_len as usize {
        env.push(*env_ptr.add(i));
    }
    let mut passed = Vec::with_capacity(argc as usize - 3);
    for i in 3..argc as usize {
        passed.push(*args.add(i));
    }
    typelisp_abi::call_state::set_pending_call(target, passed, env);
    0
}

/// `(rt-frame-pc f)` for compiled code — where frame `args[0]` should resume,
/// as a raw `i64`.
///
/// The prologue of every coroutine-ABI function reads this and branches on it:
/// `0` is "start at the top", anything else names a call site this frame was
/// waiting at. That branch is the whole of what makes a compiled function
/// resumable — everything else it needs is already in the frame.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid `i64` decoding to a
/// frame; a `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_pc(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_frame_pc: expected 1 argument");
    }
    let heap = active_heap();
    match decode(*args) {
        Value::Boxed(id) if heap.is_frame(id) => heap.frame_pc(id) as i64,
        other => fatal(&format!("rt_frame_pc: {:?} is not a frame", other)),
    }
}

/// `(rt-frame-set-pc f n)` for compiled code — records where frame `args[0]`
/// resumes, immediately before the function hands control back to the driver.
///
/// `args[1]` is a raw `i64`: a compile-time constant naming a call site, not a
/// value in the program.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to 2 valid `i64`s, the first
/// decoding to a frame; a `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_set_pc(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_frame_set_pc: expected 2 arguments (frame, resume point)");
    }
    let pc = *args.add(1);
    if !(0..=(u32::MAX as i64)).contains(&pc) {
        fatal(&format!("rt_frame_set_pc: {} is not a resume point", pc));
    }
    let heap = active_heap();
    match decode(*args) {
        Value::Boxed(id) if heap.is_frame(id) => {
            heap.set_frame_pc(id, pc as u32);
            0
        }
        other => fatal(&format!("rt_frame_set_pc: {:?} is not a frame", other)),
    }
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
/// tagged `Sexpr`); `args[1]` is a raw index, like [`rt_struct_field_get`],
/// and out-of-range the same recoverable way ([`checked_field_index`]).
/// Returns the compiled representation of `Unit` (`0`), the same convention
/// [`rt_set_car`]/[`rt_set_cdr`] use for their own in-place mutation.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s,
/// the first decoding to a `Value::Boxed` struct; a `Heap` must already be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_struct_field_set(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_struct_field_set: expected 3 arguments");
    }
    let id = match decode(*args) {
        Value::Boxed(id) => id,
        _ => fatal("rt_struct_field_set: first argument is not a boxed Sexpr"),
    };
    let idx = checked_field_index(id, *args.add(1));
    let val = decode(*args.add(2));
    active_heap().struct_set_field(id, idx, val);
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
/// the "caller already checked the count" precondition, since `pop`,
/// returns `Option<T>` (`None` on empty is a legitimate outcome the checked
/// branch builds directly, never by calling this). Fatal if `args[0]` isn't
/// a boxed struct, or if it turns out empty anyway (an internal-invariant
/// trap, the same convention every other `rt_*` bounds violation follows).
///
/// **Deliberately still `extern "C"` and `fatal`**, unlike its
/// [`rt_struct_field_get`]/[`rt_struct_field_set`] neighbours: `pop` on an
/// empty vector is not a user-reachable failure at all. The one caller
/// branches on `rt_struct_field_count` first and builds `None` itself, so
/// reaching the `fatal` below means the compiled caller stopped honouring
/// that contract — runtime corruption, which is exactly what [`fatal`] is
/// for.
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
// payload (unlike a `Vector<T>`/`defstruct`'s `StructPayload::Fields`) —
// hash to bucket, with the mem layer neither hashing a key nor comparing
// two. Both are the key type's own `sxhash`/`equals`, so `get`/`set`/
// `remove` are prelude methods that ask them and then reach the bucket
// through the five shims below. These are thin adapters, the same shape as
// the `rt_struct_*` family above; every key and value crossing them is
// already tagged, exactly as a struct field is.

/// `(rt-hashtable-new key)` for compiled code — an empty `HashTable<K,V>`
/// under the runtime identity `key` spells.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread, and `args[0]` must be
/// a tagged string.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_hashtable_new: expected 1 argument (the type key)");
    }
    // The table's own runtime identity, spelled by the checker and handed
    // down through the island (`Checker::assoc_form` -> `hashtable-op`'s name
    // slot): `HashTable<string,i32>` and `HashTable<string,string>` are
    // different types and nothing in an empty table says which this is.
    let key = type_key_arg(*args, "rt_hashtable_new");
    encode(active_heap().alloc_hashtable(key))
}

/// The bucket family: `(rt-hashtable-bucket-* ht hash ...)` for compiled
/// code. `hash` and every index are raw `i64`s (an `i32` in the language);
/// keys and values are already-tagged `Sexpr`s, as a struct field is.
///
/// # Safety
///
/// `argc >= 2`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_bucket_count(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_hashtable_bucket_count: expected 2 arguments");
    }
    let id = hashtable_arg(args, argc, "rt_hashtable_bucket_count");
    active_heap().hashtable_bucket_count(id, *args.add(1)) as i64
}

/// The `i`th key in `hash`'s bucket, still tagged — the caller decodes it
/// per `K`'s own kind. See [`rt_hashtable_bucket_count`].
///
/// # Safety
///
/// `argc >= 3`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_bucket_key(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_hashtable_bucket_key: expected 3 arguments");
    }
    let id = hashtable_arg(args, argc, "rt_hashtable_bucket_key");
    encode(active_heap().hashtable_bucket_key(id, *args.add(1), bucket_index(*args.add(2), "rt_hashtable_bucket_key")))
}

/// The `i`th value in `hash`'s bucket. See [`rt_hashtable_bucket_key`].
///
/// # Safety
///
/// `argc >= 3`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_bucket_value(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_hashtable_bucket_value: expected 3 arguments");
    }
    let id = hashtable_arg(args, argc, "rt_hashtable_bucket_value");
    encode(active_heap().hashtable_bucket_value(id, *args.add(1), bucket_index(*args.add(2), "rt_hashtable_bucket_value")))
}

/// Writes `key -> val` at index `i` of `hash`'s bucket, appending when `i` is
/// the bucket's current length. Returns the compiled `Unit` (`0`). Stores
/// already-decoded `Value`s (no cons-heap allocation), so triggers no GC.
///
/// # Safety
///
/// `argc >= 5`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_bucket_put(args: *const i64, argc: u32) -> i64 {
    if argc < 5 {
        fatal("rt_hashtable_bucket_put: expected 5 arguments");
    }
    let id = hashtable_arg(args, argc, "rt_hashtable_bucket_put");
    let i = bucket_index(*args.add(2), "rt_hashtable_bucket_put");
    let key = decode(*args.add(3));
    let val = decode(*args.add(4));
    active_heap().hashtable_bucket_put(id, *args.add(1), i, key, val);
    0
}

/// Removes the `i`th entry of `hash`'s bucket. Returns the compiled `Unit`.
///
/// # Safety
///
/// `argc >= 3`, `args[0]` a boxed `HashTable`; a `Heap` must be registered.
#[no_mangle]
pub unsafe extern "C" fn rt_hashtable_bucket_delete(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_hashtable_bucket_delete: expected 3 arguments");
    }
    let id = hashtable_arg(args, argc, "rt_hashtable_bucket_delete");
    let i = bucket_index(*args.add(2), "rt_hashtable_bucket_delete");
    active_heap().hashtable_bucket_delete(id, *args.add(1), i);
    0
}

/// A bucket index. Negative is an internal-invariant break — every index a
/// caller passes came from `bucket-count` — so it traps rather than wrapping
/// into an enormous `usize`.
fn bucket_index(n: i64, who: &str) -> usize {
    match usize::try_from(n) {
        Ok(i) => i,
        Err(_) => fatal(&format!("{}: negative bucket index {}", who, n)),
    }
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

/// The `BoxId` of `args[0]` decoded as a boxed struct — shared preamble of
/// every `HashTable` shim: the bucket family above and the three
/// `Vector`-building enumerators below.
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
    // `args[1]` is the result's own key — a `Vector<K>`, whose instantiation
    // no value here carries. See `rt_hashtable_new`.
    let key = type_key_arg(*args.add(1), "rt_hashtable_keys");
    let fields: Vec<Value> = active_heap().hashtable_pairs(id).into_iter().map(|(k, _)| k).collect();
    encode(active_heap().alloc_struct(key, fields))
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
    let key = type_key_arg(*args.add(1), "rt_hashtable_values");
    let fields: Vec<Value> = active_heap().hashtable_pairs(id).into_iter().map(|(_, v)| v).collect();
    encode(active_heap().alloc_struct(key, fields))
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
    // Two identities, one handed in: the result is a `Vector<cons-cell<K,V>>`,
    // so each cell's key is one level inside the vector's
    // (`typelisp_mem::inner_type_key`, shared with the interpreted side so the
    // two spell the cells the same).
    let vec_key_name = match decode(*args.add(1)) {
        Value::Str(id) => active_heap().string(id).to_string(),
        other => fatal(&format!("rt_hashtable_entries: the result key is not a string: {:?}", other)),
    };
    let cell_key_name = match typelisp_mem::inner_type_key(&vec_key_name) {
        Some(k) => k.to_string(),
        None => fatal("rt_hashtable_entries: the result key is not a one-argument container key"),
    };
    let vec_key = active_heap().intern_type_key(&vec_key_name);
    let cell_key = active_heap().intern_type_key(&cell_key_name);
    let pairs = active_heap().hashtable_pairs(id);
    let mut fields = Vec::with_capacity(pairs.len());
    let mut rooted = 0usize;
    for (k, v) in pairs {
        let cell = active_heap().alloc_struct(cell_key, vec![k, v]);
        active_heap().push_root(cell);
        rooted += 1;
        fields.push(cell);
    }
    let vec = active_heap().alloc_struct(vec_key, fields);
    for _ in 0..rooted {
        active_heap().pop_root();
    }
    encode(vec)
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
/// `compiler.rs`'s `compile-panic`): unwinds with a [`CompiledPanic`]
/// carrying `msg`, so the boundary that entered compiled code can turn it
/// back into the same recoverable `EvalError::Panic` the interpreted path
/// produces.
///
/// **`extern "C-unwind"`, not `extern "C"`, is what makes this legal.** A
/// panic reaching a plain `extern "C"` boundary is defined to abort the
/// process (the Rustonomicon's "FFI and unwinding"), which is exactly what
/// this function used to do on purpose. Rust 1.71 stabilized the `-unwind`
/// ABI strings for this case; `tests/compiled_unwind_test.rs` is the probe
/// that established the rest of the pipeline cooperates — JIT-compiled frames
/// are walkable by the system unwinder, and no `uwtable` attribute is needed
/// on the generated functions.
///
/// One non-obvious constraint comes with it: **the code being unwound through
/// must outlive the unwind.** Freeing it mid-unwind can leave the unwinder
/// unable to finish the *next* unwind. `CompiledFn` frees its code only
/// through `compile::retire_llvm`, when no compiled chain stands.
///
/// # Safety
///
/// `argc` must be `>= 1`, `args[0]` must decode to a `Value::Str`; a `Heap`
/// must already be registered on this thread. Every frame between this call
/// and the catching boundary must tolerate being unwound through — for
/// compiled frames that is what the probe above establishes, and for the
/// `rt_*` frames in between it is why they are `extern "C-unwind"` too.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_panic(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_panic: expected 1 argument");
    }
    let message = match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        _ => fatal("rt_panic: argument is not a Str"),
    };
    raise(message)
}

/// The exit code an AOT executable ends with when a `(panic ...)` reaches its
/// entry point uncaught — the same code `typl <file>` exits with when an
/// interpreted run ends in an error, so the two front ends agree.
///
/// `pub` for `typelisp_front::shim::rt_run_program_interp[_int]`, an
/// `eval`-carrying AOT executable's own entry, which answers with the same
/// code for the same reason — see [`run_entry_payload`].
pub const EXIT_CODE_PANIC: i64 = 1;

/// The exit code of a program whose `()`-returning `main` returned normally.
/// Named, and not the value `main` answered, because that answer is the word
/// `()` is encoded as, which differs between the entry shims' decodings.
pub const EXIT_CODE_SUCCESS: i64 = 0;

/// Runs an AOT executable's compiled entry point (`tl_main`) with a catch
/// around it, returning its exit code — or, if a `(panic ...)` unwound out of
/// it, printing the message and returning [`EXIT_CODE_PANIC`].
///
/// This exists because an AOT executable has nowhere else to put the catch.
/// In the JIT path the interpreter is a Rust frame that can wrap the call
/// (`typelisp::compile::catch_compiled_panic`); AOT's `main` is *generated
/// LLVM code* that calls `rt_heap_init` and then `tl_main` directly
/// (`compile::aot`'s entry-point builder), with no Rust frame in between. So
/// the catch moves into the runtime, and the generated `main` calls this
/// instead of calling `tl_main` itself.
///
/// Letting the panic unwind out of `main` instead is not an option: `main` is
/// the C entry point, and unwinding past it is undefined.
///
/// # Safety
///
/// `entry` must be the address of a function compiled under the standard
/// compiled-function ABI (`typelisp::compile::CompiledSignature`) that
/// tolerates being called with no arguments — which `tl_main` is, since the
/// generated `main` already called it that way. A `Heap` must already be
/// registered on this thread (`rt_heap_init` runs first).
#[no_mangle]
pub unsafe extern "C" fn rt_run_entry(entry: i64) -> i64 {
    let f: unsafe extern "C-unwind" fn(*const i64, u32) -> i64 = std::mem::transmute(entry as usize);
    // Only a `()`-returning `main` reaches this shim (`compile::aot` refuses
    // an `int` one under this ABI), and a `main` that returns normally exits
    // 0 — whatever word its `()` happens to be.
    let call = std::panic::AssertUnwindSafe(|| {
        f(std::ptr::null(), 0);
        EXIT_CODE_SUCCESS
    });
    run_entry_payload(std::panic::catch_unwind(call))
}

/// [`rt_run_entry`] for a program built under the coroutine ABI: the same
/// panic handling with a **scheduler** in front of it, and the program's
/// global initialisers run under that same scheduler ahead of `main`.
///
/// `args` is `[inits, n, entry]`: the address of an array of `n`
/// `coroutine_fn_type` addresses — one zero-argument initialiser per
/// `defvar`, in declaration order — and the entry point's own address. A
/// generated `main` cannot drive any of them itself: the protocol is a loop
/// over status words, and `main` is a handful of constant stores. Which of
/// the two entry shims this executable's `main` calls is decided when it is
/// generated, by the ABI that build emitted.
///
/// The scheduler lives on this frame for as long as the program does
/// (`sched::Scheduler` over `sched::CompiledTask`). Each initialiser is run
/// as the main task of one drive and `main` as the main task of the last —
/// the shape the REPL gives its top-level forms — so an initialiser that
/// makes a channel, starts a task or waits on one is an ordinary program and
/// not a startup special case, and a task an initialiser started is still
/// there when `main` runs. When `main` returns the program ends and every
/// other task is cut off — Go's rule, and `typl`'s. A task's own uncaught
/// failure stops the program with the same line the interpreter prints for
/// it, an initialiser's included.
///
/// # Safety
///
/// `argc` must be 3; `args[0]` must point to `args[1]` valid
/// `coroutine_fn_type` addresses of zero-argument bodies, and `args[2]` must
/// be one such address that tolerates being entered with no arguments; a
/// `Heap` must already be registered on this thread (`rt_heap_init` runs
/// first).
#[no_mangle]
pub unsafe extern "C" fn rt_run_program(args: *const i64, argc: u32) -> i64 {
    let program = read_program(args, argc, "rt_run_program");
    // A `()`-returning `main` that returns normally exits 0; the word its
    // `()` is encoded as is not an exit code.
    let call = std::panic::AssertUnwindSafe(|| match run_program(&program) {
        Ok(_) => EXIT_CODE_SUCCESS,
        Err(code) => code,
    });
    run_entry_payload(std::panic::catch_unwind(call))
}

/// [`rt_run_program`] for a `main` declared to return `int`: the answer is
/// a tagged word, and the exit code is the fixnum's payload.
///
/// Decoded *here* rather than by the generated `main`, because a panic's
/// exit code comes out of the same call raw (`EXIT_CODE_PANIC`), and only
/// this side knows which of the two it is handing back. A bignum has no
/// exit code and is fatal.
///
/// # Safety
///
/// [`rt_run_program`]'s.
#[no_mangle]
pub unsafe extern "C" fn rt_run_program_int(args: *const i64, argc: u32) -> i64 {
    let program = read_program(args, argc, "rt_run_program_int");
    let call = std::panic::AssertUnwindSafe(|| match run_program(&program) {
        Ok(Value::Int(n)) => n,
        Ok(Value::Boxed(id)) if active_heap().is_bignum(id) => {
            fatal(&format!("main returned {}, which is not a process exit code", active_heap().bignum_value(id)))
        }
        Ok(other) => fatal(&format!("main declared to return int answered {:?}", other)),
        Err(code) => code,
    });
    run_entry_payload(std::panic::catch_unwind(call))
}

/// What a generated `main` hands the program entry: the initialisers and
/// the entry point, as coroutine-ABI addresses.
pub struct Program {
    pub inits: Vec<crate::coroutine::CoroutineFn>,
    pub entry: crate::coroutine::CoroutineFn,
}

/// Reads `[inits, n, entry]` off the standard shim argument array.
///
/// # Safety
///
/// [`rt_run_program`]'s.
pub unsafe fn read_program(args: *const i64, argc: u32, what: &str) -> Program {
    if argc < 3 {
        fatal(&format!("{}: expected 3 arguments (the initialiser array, its count and the entry point)", what));
    }
    let words = collect_words(*args as usize as *const i64, *args.add(1));
    let inits = words.into_iter().map(|w| std::mem::transmute::<usize, crate::coroutine::CoroutineFn>(w as usize)).collect();
    let entry: crate::coroutine::CoroutineFn = std::mem::transmute(*args.add(2) as usize);
    Program { inits, entry }
}

/// Runs the program's initialisers and then its `main`, each as the main
/// task of one drive of a scheduler that lives on this frame, and answers
/// with `main`'s value — or with the exit code of a failure that has
/// already been reported.
///
/// The scheduler is a local of this frame and nothing else: it is owned by
/// the frame that owns the program's run, exactly as the interpreter's is a
/// field of the interpreter — never a global, because its tables hold roots
/// into the heap (`sched`'s module comment). The worker threads share it
/// through an `Arc`, and hold nothing else of this frame's.
///
/// `TYPELISP_THREADS` threads step tasks, this one included
/// ([`sched::thread_count`](crate::sched::thread_count)). The workers are
/// started before the first initialiser, share this thread's heap (a view
/// each), its global/vtable/stream tables and its printer's registrations,
/// and are never joined: when `main` returns the program ends, and every
/// task still running with it — Go's rule.
///
/// # Safety
///
/// [`rt_run_program`]'s.
unsafe fn run_program(program: &Program) -> Result<Value, i64> {
    let heap = active_heap();
    let threads = match crate::sched::thread_count() {
        Ok(n) => n,
        Err(m) => {
            eprintln!("error: {}", m);
            return Err(EXIT_CODE_PANIC);
        }
    };
    let sched = match crate::sched::SchedShared::<crate::sched::CompiledTask>::new() {
        Ok(s) => std::sync::Arc::new(s),
        Err(e) => fatal(&format!("the scheduler could not be set up: {}", e)),
    };
    // The printer reads the prelude's control variables from their compiled
    // slots, and the id -> slot table is this crate's.
    typelisp_print::aot::set_global_slots(global_perm_idx);
    let rt = crate::shared::rt_shared();
    let print = typelisp_print::shared::print_shared();
    let hooks = typelisp_print::runtime::print_hooks();
    let started = crate::sched::start_workers(&sched, heap, threads - 1, &(), move || {
        crate::shared::set_rt_shared(std::sync::Arc::clone(&rt));
        typelisp_print::shared::set_print_shared(std::sync::Arc::clone(&print));
        typelisp_print::runtime::set_print_hooks(Some(hooks));
    });
    if let Err(e) = started {
        fatal(&format!("a scheduler worker thread could not be started: {}", e));
    }
    // Built inside `admit`, on the task's own root stack — `entry` pushes
    // the task's state slots, and they belong there and not on this frame's.
    let run = |heap: &mut Heap, f: crate::coroutine::CoroutineFn, initialiser: bool| {
        let main = sched.lock(heap).admit(heap, |heap| {
            if initialiser {
                crate::sched::CompiledTask::initialiser(heap, f)
            } else {
                crate::sched::CompiledTask::entry(heap, f)
            }
        });
        match crate::sched::drive_main(&sched, heap, &(), main) {
            Ok(v) => Ok(v),
            // The same line `run_entry_payload` prints for a panic that
            // reached the entry as a Rust unwind — a task's failure is the
            // program's.
            Err(failure) => {
                eprintln!("{}", failure);
                Err(EXIT_CODE_PANIC)
            }
        }
    };
    for init in &program.inits {
        // An initialiser's value is the global's storage id, already stored
        // by the body itself (`compile-global-init`); nothing here reads it.
        run(heap, *init, true)?;
    }
    run(heap, program.entry, false)
}

/// A classic-signature door onto a coroutine body: `args` is
/// `[address, argument array, count]`, and the answer is the body's value.
///
/// For a caller that has to look like `(ptr, i32) -> i64` and cannot drive
/// anything itself — `typelisp-print` holds the addresses `~/name/` and
/// `print-object` dispatch to, and that crate deliberately does not depend on
/// this one (see its `Cargo.toml`), so it has no `FrameStack` to reach for.
/// `compile::aot` wraps each registered method in four instructions that come
/// here instead, which puts the knowledge of the callee's ABI in the one
/// place that has it: the code generator that emitted the callee.
///
/// # Safety
///
/// `argc` must be 3; `args[0]` must be a `coroutine_fn_type` address and
/// `args[1]`/`args[2]` a valid argument array for it; a `Heap` must already
/// be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_drive_body(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_drive_body: expected 3 arguments (the address, the argument array and its count)");
    }
    let f: crate::coroutine::CoroutineFn = std::mem::transmute(*args as usize);
    let callee_args = *args.add(1) as usize as *const i64;
    let callee_argc = *args.add(2);
    let words = collect_words(callee_args, callee_argc);
    drive_to_completion(f, &words, &[], "rt_drive_body")
}

/// Turns whichever entry point ran into an exit code: a typelisp-level
/// `panic` or an uncaught `throw` that reached it as a Rust unwind is
/// reported and becomes [`EXIT_CODE_PANIC`]; anything else keeps unwinding.
///
/// `pub` for the interpreter's own program entry (`typelisp_front::shim`),
/// which puts the same catch around the same kind of run.
///
/// # Safety
///
/// A `Heap` must be registered on this thread (an uncaught throw's tag is
/// read out of it).
pub unsafe fn run_entry_payload(outcome: std::thread::Result<i64>) -> i64 {
    let call = std::panic::AssertUnwindSafe(|| match outcome {
        Ok(v) => v,
        Err(payload) => std::panic::resume_unwind(payload),
    });
    match std::panic::catch_unwind(call) {
        Ok(code) => code,
        // Only a typelisp-level `panic` is turned into an exit code; anything
        // else is a real bug in the runtime and keeps unwinding, exactly as
        // `catch_compiled_panic` does on the JIT side.
        Err(payload) => {
            let payload = match payload.downcast::<CompiledPanic>() {
                Ok(p) => {
                    eprintln!("panic: {}", p.message);
                    return EXIT_CODE_PANIC;
                }
                Err(other) => other,
            };
            // A `throw` that found no enclosing `catch` anywhere. Reported
            // with the same wording `EvalError::Throw`'s `Display` uses, so
            // the two front ends say the same thing about the same program.
            match payload.downcast::<CompiledThrow>() {
                Ok(_) => {
                    let (tag, _) =
                        take_throw().unwrap_or_else(|| fatal("a compiled throw unwound with no throw in flight"));
                    eprintln!("throw: no enclosing (catch '{}) for this throw", tag);
                    EXIT_CODE_PANIC
                }
                Err(other) => std::panic::resume_unwind(other),
            }
        }
    }
}

// ---- catch / throw / unwind-protect --------------------------------------
//
// A `throw` travels as a Rust panic, because that is the only unwind every
// frame between the throw and its catch can survive: compiled frames are
// walkable (`tests/compiled_unwind_test.rs`), and the interpreted frames that
// may sit in between (compiled -> a machine-frame driver -> interpreted ->
// compiled)
// are Rust ones, which can only catch Rust panics — a foreign exception
// reaching `catch_unwind` aborts with "Rust cannot catch foreign exceptions".
//
// **Which is also why a `catch` cannot be an LLVM landing pad.** Stopping an
// unwind means consuming the exception object, and for a Rust panic only
// `std::panic::catch_unwind` can: the object's own `exception_cleanup` aborts
// ("Rust panics must be rethrown") and `panic_count` is decremented nowhere
// else, so a landing pad that swallowed one would leak the allocation and
// leave `thread::panicking()` true for the rest of the process.
//
// So the Rust frame that does the catching is put where a Rust frame can
// legitimately go, and since C4 there is exactly one such place: the
// **driver**, at the boundary where it enters a compiled activation
// (`coroutine::activation`). One catch for the whole tier, rather than one
// per call inside a protected region, because a Lisp call is a driver round
// trip and not a machine call — so a panic raised in compiled code can only
// travel as far as the activation that raised it, and the driver is standing
// at its exit.
//
// Where it goes next is then frame bookkeeping: the driver asks each frame,
// innermost first, whether it declares a handler
// (`typelisp_abi::FRAME_HANDLER_SLOT`), and resumes the first that does. The
// nine `rt_protected_*` trampolines this section used to hold were the
// per-call version of that, and C4 deleted them.

thread_local! {
    /// The tag of the throw currently in flight *for whichever task's `step`
    /// is running on this thread right now*. Its *value* lives in
    /// `Heap::set_in_flight_throw` instead, where the collector can see it —
    /// see [`CompiledThrow`].
    ///
    /// **Not this thread's for the whole time a task is merely parked.** A
    /// task whose `unwind-protect` cleanup suspends (`(sleep)`/`(recv)`/…)
    /// leaves this set while it is `Blocked`, and cooperative scheduling can
    /// run a *different* task's own throw on this same thread before it
    /// resumes — so every `TaskBody::step` saves this thread-local into the
    /// task itself before returning and restores it before running again
    /// ([`take_parked_unwind`]/[`restore_parked_unwind`]), the same way a
    /// task's continuation stack is put down and picked back up.
    static IN_FLIGHT_TAG: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The panic payload the driver caught and has not yet handed on: to a
    /// `catch` that claims it ([`rt_throw_take_value`], which drops it), to
    /// the interpreter as an `EvalError` ([`take_activation_unwind`]), or back
    /// to the unwinder ([`resume_activation_unwind`]) when the drive's caller
    /// is a machine frame.
    ///
    /// Saved and restored around each `step` exactly like `IN_FLIGHT_TAG`,
    /// for the same reason.
    static CAUGHT_UNWIND: RefCell<Option<Box<dyn std::any::Any + Send>>> = const { RefCell::new(None) };
}

/// What [`IN_FLIGHT_TAG`]/[`CAUGHT_UNWIND`] hold for one task, moved off the
/// live thread-locals between that task's steps — `docs/dev/os-threads-design.md`
/// §3.
///
/// The throw's *value* is deliberately not here: it needs a GC root, and each
/// task's own `RootStack` already gives it one that survives exactly as long
/// as this does (`Heap::in_flight_throw`/`set_in_flight_throw`). Keeping the
/// two halves in separate places is safe only because both are switched in
/// lockstep — whoever installs a `ParkedUnwind` before running a task has
/// also made that task's own root stack the current one first.
#[derive(Default)]
pub struct ParkedUnwind {
    tag: Option<String>,
    caught: Option<Box<dyn std::any::Any + Send>>,
}

impl ParkedUnwind {
    /// A `ParkedUnwind` for a `panic` that arrived with no throw of its
    /// own — what [`sched::CompiledTask::deliver`](crate::sched::CompiledTask)
    /// parks for its own next `step` to install, instead of writing straight
    /// to the live thread-locals the way [`park_activation_unwind`] does (see
    /// that function's caller for why: `deliver` can run while a *different*
    /// task's state is the one installed on this thread).
    pub fn panic(message: String) -> ParkedUnwind {
        ParkedUnwind { tag: None, caught: Some(Box::new(CompiledPanic { message })) }
    }
}

/// Takes this thread's in-flight-unwind state, leaving both thread-locals
/// empty — half of the pair a task's `step` calls around itself, the other
/// half being [`restore_parked_unwind`]. Whether called at a `step`'s start
/// (to make room for this task's own saved state) or its end (to save what
/// it leaves behind), the shape is the same: nothing is left on the thread
/// for a task that is not the one about to run, or that just stopped
/// running, to see.
pub fn take_parked_unwind() -> ParkedUnwind {
    ParkedUnwind { tag: IN_FLIGHT_TAG.with(|c| c.borrow_mut().take()), caught: CAUGHT_UNWIND.with(|c| c.borrow_mut().take()) }
}

/// Installs a previously [`take_parked_unwind`]'s state onto this thread —
/// what a task's `step` does before running, so code that reads
/// `IN_FLIGHT_TAG`/`CAUGHT_UNWIND` (a `catch`'s dispatch, an
/// `unwind-protect`'s resume) finds what *this* task itself last left there,
/// not another task's leftovers or another task's clean slate.
pub fn restore_parked_unwind(p: ParkedUnwind) {
    IN_FLIGHT_TAG.with(|c| *c.borrow_mut() = p.tag);
    CAUGHT_UNWIND.with(|c| *c.borrow_mut() = p.caught);
}

/// Parks a throw's tag and value for the unwind about to be raised for it,
/// rooting the value for the flight.
///
/// `pub` because the `typelisp` crate needs it too: an interpreted callee
/// that throws produces an `EvalError::Throw`, and the boundary hook turns it
/// into this same parked pair so a *compiled* `catch` further up sees one kind
/// of in-flight throw, not two.
///
/// # Safety
///
/// A `Heap` must be registered on this thread, and it must be the heap `value`
/// belongs to.
pub unsafe fn park_throw(tag: String, value: Value) {
    IN_FLIGHT_TAG.with(|cell| *cell.borrow_mut() = Some(tag));
    active_heap().set_in_flight_throw(Some(value));
}

/// Takes back what [`park_throw`] left. `None` when no throw is in flight.
///
/// Clears the tag but **leaves the value's GC root standing**: taking the pair
/// here does not necessarily end the flight. The boundary that turns a
/// compiled throw back into an `EvalError::Throw` takes it and the throw keeps
/// travelling, now through interpreted frames, which can still run
/// `unwind-protect` cleanups that allocate. The root is released where the
/// value actually stops — the `catch` that claims it, on either side
/// ([`rt_throw_take_value`], or `Op::Catch` in the interpreter).
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
pub unsafe fn take_throw() -> Option<(String, Value)> {
    let tag = IN_FLIGHT_TAG.with(|cell| cell.borrow_mut().take())?;
    // `park_throw` sets the two together, and every path that clears the
    // value clears the tag with it.
    let value = active_heap()
        .in_flight_throw()
        .unwrap_or_else(|| fatal(&format!("a throw to '{} is in flight without its value", tag)));
    Some((tag, value))
}

/// Ends the flight: no throw is travelling any more, so the value it carried
/// no longer needs a root of its own.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
pub unsafe fn clear_throw() {
    IN_FLIGHT_TAG.with(|cell| *cell.borrow_mut() = None);
    active_heap().set_in_flight_throw(None);
}

/// Parks an exit raised in *interpreted* code so a standing compiled chain
/// can catch it — the same two channels an unwinding call uses, minus the
/// unwinding.
///
/// Since C5 an interpreted callee reached from compiled code runs on the
/// task's continuation stack rather than on a machine frame inside the
/// compiled activation, so its non-local exit has no Rust frames to travel
/// through. It is handed to the chain as a status instead
/// (`coroutine::FrameStack::raise`), and this is what the chain's `catch`
/// then finds: the tag and value where `rt_throw_matches` looks for them, and
/// the payload kind where the driver's own catch leaves it.
///
/// # Safety
///
/// A `Heap` must be registered on this thread, and it must be the heap
/// `value` belongs to.
pub unsafe fn park_throw_for_chain(tag: String, value: Value) {
    park_throw(tag, value);
    CAUGHT_UNWIND.with(|cell| *cell.borrow_mut() = Some(Box::new(CompiledThrow)));
}

/// [`park_throw_for_chain`] for an exit that is not a throw — a `panic`, a
/// runtime error, a `break` that escaped. The `EvalError` itself waits on the
/// front end's side (`crossing::park_interpreted_error`); this is the marker
/// that says which side to ask.
///
/// Clears any throw still parked, for `park_activation_unwind`'s reason:
/// anything that is not a throw *replaces* what was in flight, and leaving
/// the old tag parked would let a `catch` claim this as its own.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
pub unsafe fn park_error_for_chain() {
    clear_throw();
    CAUGHT_UNWIND.with(|cell| *cell.borrow_mut() = Some(Box::new(InterpretedUnwind)));
}

/// Raises the unwind for a throw already parked by [`park_throw`].
///
/// The counterpart of [`unwind_interpreted_error`] for the throw channel; the
/// `typelisp` crate calls it after parking an interpreted `EvalError::Throw`.
pub fn unwind_throw() -> ! {
    install_quiet_panic_hook();
    std::panic::panic_any(CompiledThrow)
}

/// `(throw 'tag value)` for compiled code (`compiler.rs`'s `compile-throw`):
/// parks the tag and value and unwinds, so the nearest dynamically enclosing
/// `catch` on that tag — compiled or interpreted, in this function or twenty
/// frames up — produces the value in its own place.
///
/// # Safety
///
/// `argc` must be `>= 2`; `args[0]` must decode to a `Value::Symbol` and
/// `args[1]` to the thrown value already in its tagged form (the island
/// converts by the node's baked-in representation kind before calling). A
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_throw(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_throw: expected 2 arguments (tag, value)");
    }
    let tag = match decode(*args) {
        Value::Symbol(id) => active_heap().symbol_name(id).to_string(),
        _ => fatal("rt_throw: tag is not a symbol"),
    };
    let value = decode(*args.add(1));
    park_throw(tag, value);
    unwind_throw()
}

/// Whether the throw currently in flight carries tag `args[0]` — a `catch`'s
/// dispatch block asking "is this one mine?". Returns the compiled `bool`
/// encoding (`1`/`0`), like every `rt_*` predicate.
///
/// False when nothing is in flight at all, which is the case for a caught
/// `CompiledPanic`/`InterpretedUnwind`: those belong to no tag, so every
/// `catch` declines them and only `unwind-protect` cleanups see them pass.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args[0]` must decode to a `Value::Symbol`; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_throw_matches(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_throw_matches: expected 1 argument");
    }
    let tag = match decode(*args) {
        Value::Symbol(id) => active_heap().symbol_name(id).to_string(),
        _ => fatal("rt_throw_matches: tag is not a symbol"),
    };
    IN_FLIGHT_TAG.with(|cell| i64::from(cell.borrow().as_deref() == Some(tag.as_str())))
}

/// Consumes the in-flight throw, returning its value in tagged form — the
/// other half of a `catch` claiming one, called only after
/// [`rt_throw_matches`] said yes.
///
/// Dropping the parked panic payload here is what makes the claim final: a
/// later [`rt_resume_unwind`] has nothing to re-raise, which is exactly right,
/// since control has rejoined the catching function's ordinary flow.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_throw_take_value(_args: *const i64, _argc: u32) -> i64 {
    CAUGHT_UNWIND.with(|cell| *cell.borrow_mut() = None);
    match take_throw() {
        Some((_, value)) => {
            clear_throw();
            encode(value)
        }
        None => fatal("rt_throw_take_value: no throw is in flight"),
    }
}

/// Classifies an unwind caught at a compiled activation's boundary
/// (`coroutine::activation`) and parks it for the driver to carry on with.
///
/// Anything that is not one of the runtime's three deliberate payloads keeps
/// unwinding: a genuine bug in the runtime or in generated code must not come
/// back as a plausible-looking typelisp condition.
///
/// A throw keeps its parked tag and value — its flight is not over, it is
/// being carried by the driver instead of by the unwinder. Anything else
/// *replaces* what was in flight: a cleanup that panics while a throw travels
/// wins, and CLHS says so. Leaving the old tag parked would let a `catch`
/// further up claim this panic as if it were its own throw.
///
/// Unlike [`protected`] this repairs nothing. The GC root stack and the
/// published-frame stack are cut back by the driver, which knows the depths
/// per frame rather than per call.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
pub(crate) unsafe fn park_activation_unwind(payload: Box<dyn std::any::Any + Send>) {
    let ours = payload.is::<CompiledThrow>() || payload.is::<CompiledPanic>() || payload.is::<InterpretedUnwind>();
    if !ours {
        std::panic::resume_unwind(payload);
    }
    if !payload.is::<CompiledThrow>() {
        clear_throw();
    }
    CAUGHT_UNWIND.with(|cell| *cell.borrow_mut() = Some(payload));
}

/// Takes what [`park_activation_unwind`] parked, for a caller that wants the
/// payload itself rather than a re-raise — the interpreter's task driver,
/// which turns it into an `EvalError` and unwinds its own continuation stack
/// through it instead of the machine stack.
pub fn take_activation_unwind() -> Option<Box<dyn std::any::Any + Send>> {
    CAUGHT_UNWIND.with(|cell| cell.borrow_mut().take())
}

/// Re-raises what [`park_activation_unwind`] parked — what a driver whose
/// caller is a machine frame does when the unwind passed its outermost frame.
///
/// # Safety
///
/// Nothing unsafe happens here; it is `fatal` rather than a panic when
/// nothing is carried, because reaching it with an empty slot means the
/// driver reported an unwind that never was.
pub(crate) fn resume_activation_unwind() -> ! {
    match take_activation_unwind() {
        Some(payload) => std::panic::resume_unwind(payload),
        None => fatal("resume_activation_unwind: the driver reported an unwind but none is being carried"),
    }
}

unsafe fn collect_words(p: *const i64, n: i64) -> Vec<i64> {
    if n < 0 {
        fatal(&format!("{} is not a word count", n));
    }
    (0..n as usize).map(|i| *p.add(i)).collect()
}

/// What a driver whose caller is a machine frame does with a pause.
///
/// An unwind is re-raised, so it keeps travelling towards a `catch` above
/// this boundary. Nothing else can come out: every driver standing on a
/// machine frame goes through `FrameStack::run_to_end`, which runs an
/// interpreted `apply` on that frame and resolves a suspension through the
/// driving scheduler (answering it, or refusing it into the chain as a
/// panic). What still stands on a machine frame is `rt_drive_body` (the
/// printer's door); the executable's initialisers and `main` run under a
/// scheduler (`run_program`).
unsafe fn pause_on_a_machine_frame(paused: crate::coroutine::Paused, what: &str) -> ! {
    match paused {
        crate::coroutine::Paused::Unwinding => crate::coroutine::resume_unwinding(),
        // Reaching either means a driver was written to `run`/`resume`
        // directly — a protocol break, not a limitation.
        crate::coroutine::Paused::Applying { .. } | crate::coroutine::Paused::ApplyingDyn { .. } => fatal(&format!(
            "{}: a compiled frame applied an interpreted function value and the drive did not \
             resolve it — a driver on a machine frame must use `run_to_end`",
            what
        )),
        crate::coroutine::Paused::Suspended => fatal(&format!(
            "{}: a compiled frame suspended and the drive did not resolve it — a driver on a \
             machine frame must use `run_to_end`",
            what
        )),
    }
}

/// Applies an interpreted function value on *this* machine frame — what a
/// driver with no continuation stack behind it does with
/// [`coroutine::Paused::Applying`].
///
/// In an AOT executable there is no interpreter and nothing should produce an
/// interpreted closure either, so a missing hook is an invariant break rather
/// than a limitation to work around.
///
/// # Safety
///
/// `closure` must be a tagged interpreted-closure box in the heap registered
/// on this thread, and `args` its arguments in that closure's own declared
/// representations.
pub unsafe fn apply_on_this_frame(closure: i64, args: &[i64], what: &str) -> i64 {
    match APPLY_INTERPRETED.with(|cell| cell.get()) {
        Some(hook) => hook(closure, args.as_ptr(), args.len() as u32),
        None => fatal(&format!(
            "{}: a compiled frame applied an interpreted function value and no interpreter is \
             registered on this thread",
            what
        )),
    }
}

unsafe fn drive_to_completion(
    f: crate::coroutine::CoroutineFn,
    args: &[i64],
    env: &[i64],
    what: &str,
) -> i64 {
    let mut stack = crate::coroutine::FrameStack::new();
    match stack.run_to_end(active_heap(), f, args, env, |closure, argv| {
        apply_on_this_frame(closure, argv, what)
    }) {
        Ok(v) => v,
        Err(paused) => pause_on_a_machine_frame(paused, what),
    }
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
/// ordinary `const-word` operand (`core_bridge::str_form` builds a
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

// ---- the C FFI's string conversions --------------------------------------
//
// A typelisp `string` is a Rust `String` in the heap's string table: not
// NUL-terminated, and free to contain a NUL of its own. A C `const char *` is
// the opposite on both counts. So a string argument is *copied* into a C
// string for the duration of one call and freed after it, and a string result
// is copied back out.
//
// These live here rather than in `os`: that module is the one place that calls
// the C library, and none of this does — `CString` is Rust's, and the
// allocation is Rust's own.

/// `args[0]` (a tagged `Value::Str`) as a freshly allocated C string, returned
/// as its address.
///
/// The caller owns it and must hand it to [`rt_ffi_cstring_free`]; the FFI
/// thunk emits both, around the one call in between.
///
/// # Safety
///
/// `args` must point to at least one readable `i64`, and a `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_ffi_cstring_new(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ffi_cstring_new: expected 1 argument");
    }
    let s = match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        other => fatal(&format!("rt_ffi_cstring_new: argument is not a string, got {:?}", other)),
    };
    // A NUL inside the string is a real value here and cannot be one there:
    // C would read the prefix and call it the whole string. Raised, not
    // aborted — the program handed over a string it is allowed to hold.
    match std::ffi::CString::new(s) {
        Ok(c) => c.into_raw() as i64,
        Err(_) => raise(
            "ffi: a string argument contains a NUL character, which cannot be passed to C — \
             C would see only the part before it"
                .to_string(),
        ),
    }
}

/// Marks the start of a C call: from here until [`rt_ffi_leave_native`]
/// another thread's collection need not wait for this one, however long the
/// C function blocks. The FFI thunk emits the pair around the one call, after
/// the arguments have become C values and before the answer becomes a Lisp
/// one — the C function is handed no heap value, so it has none to touch.
///
/// # Safety
///
/// A `Heap` must be registered on this thread and be running (not already
/// native).
#[no_mangle]
pub unsafe extern "C" fn rt_ffi_enter_native(_args: *const i64, _argc: u32) -> i64 {
    active_heap().enter_native();
    0
}

/// Ends what [`rt_ffi_enter_native`] began, waiting out a collection in
/// progress first.
///
/// # Safety
///
/// A `Heap` must be registered on this thread, inside `rt_ffi_enter_native`.
///
/// Also where a callback's failure kept while C was running is re-raised —
/// the first point after the C call where unwinding is defined again (see
/// [`ffi_callback`]). Hence `C-unwind`.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_ffi_leave_native(_args: *const i64, _argc: u32) -> i64 {
    active_heap().leave_native();
    ffi_callback::resume_pending();
    0
}

/// Frees what [`rt_ffi_cstring_new`] returned.
///
/// # Safety
///
/// `args[0]` must be an address that call returned and that has not been freed.
#[no_mangle]
pub unsafe extern "C" fn rt_ffi_cstring_free(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ffi_cstring_free: expected 1 argument");
    }
    let p = *args as usize;
    if p != 0 {
        drop(std::ffi::CString::from_raw(p as *mut std::ffi::c_char));
    }
    0
}

/// The C string at `args[0]`, copied onto the heap as a tagged `Value::Str`.
///
/// **Copies, and does not free.** What C handed back is C's — it may be a
/// pointer into a static table (`getenv`), and freeing it would be wrong far
/// more often than right. A function that returns memory the caller must free
/// should be declared as returning `ptr`, so the freeing is written down.
///
/// # Safety
///
/// `args[0]` must be null or a pointer to a NUL-terminated string that stays
/// valid for the duration of this call. A `Heap` must be registered on this
/// thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_ffi_string_from_cstr(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_ffi_string_from_cstr: expected 1 argument");
    }
    let p = *args as usize;
    if p == 0 {
        // `string` has no value for "there wasn't one". Saying so beats
        // inventing an empty string, and the fix is in the declaration.
        raise(
            "ffi: the C function returned a null pointer where a `string` was declared. \
             Declare the result as `ptr` if it can be null."
                .to_string(),
        );
    }
    let bytes = std::ffi::CStr::from_ptr(p as *const std::ffi::c_char).to_bytes();
    match std::str::from_utf8(bytes) {
        Ok(s) => {
            let s = s.to_string();
            encode(active_heap().alloc_string(s))
        }
        Err(_) => raise(
            "ffi: the C function returned bytes that are not valid UTF-8, and a typelisp \
             `string` is text. Declare the result as `ptr` to handle the bytes yourself."
                .to_string(),
        ),
    }
}

/// Interns `args[0]` (a tagged `Value::Str`) as a symbol **in the module the
/// remaining arguments name** — zero of them being the root module — and
/// returns it as a tagged `Value::Symbol`.
///
/// The home module is part of the answer, not decoration: two modules that
/// both write `foo` own two different symbols, so interning a bare name would
/// hand back whichever module the runtime defaulted to. A symbol from the
/// fixed vocabulary never comes through here at all — it has an index, and
/// [`rt_wk_symbol`] uses it.
///
/// This is the compiled-code half of a quoted symbol literal (`core_bridge::quoted_form`'s `Sym` arm builds the name as an
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
        fatal("rt_intern_symbol: expected at least 1 argument");
    }
    let str_arg = |i: u32| -> String {
        match decode(*args.add(i as usize)) {
            Value::Str(id) => active_heap().string(id).to_string(),
            _ => fatal("rt_intern_symbol: argument is not a Str"),
        }
    };
    let name = str_arg(0);
    let home: Vec<String> = (1..argc).map(str_arg).collect();
    encode(Value::Symbol(typelisp_mem::symbols::intern_in_path(&home, &name)))
}

/// The system module's symbol at index `args[0]` (a raw `i64`, not a tagged
/// value) of `BUILTIN_SYMBOLS`, as a tagged `Value::Symbol`.
///
/// The compiled-code half of a quoted symbol from the fixed vocabulary
/// (`core_bridge::sym_form`). Its address cannot be baked into a program that
/// will run in another process, but its index can: every process builds the
/// vocabulary from the same list in the same order. Nothing is spelled out,
/// nothing is allocated, and no table is searched — one array index.
///
/// Permanent like every symbol, so no GC root is involved.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid `i64` holding an
/// index within `BUILTIN_SYMBOLS`.
#[no_mangle]
pub unsafe extern "C" fn rt_wk_symbol(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_wk_symbol: expected 1 argument");
    }
    encode(Value::Symbol(typelisp_mem::symbols::well_known_symbol(*args as u32)))
}


/// The receiver type's width and signedness, as the extra raw operand every
/// integer shim below takes: `width * 2 + (signed as i64)`.
///
/// A type name means its width and its signedness and nothing else
/// (`types::int_width_signed`), so the six integer types share one shim per
/// operation and are told apart by this one number rather than by six copies
/// of each. The island builds it as a constant at the call site — the
/// receiver's type name is right there in `compile-assoc`.
pub(crate) fn wsig(code: i64) -> (u32, bool) {
    ((code >> 1) as u32, code & 1 == 1)
}

/// `v` cut back to `width` bits and re-extended into the 64-bit word compiled
/// code carries every integer in.
///
/// One statement of the invariant, in `typelisp-mem` (below both this crate
/// and the checker), rather than a copy here that could drift from it.
use typelisp_mem::normalize_int as normalize;

/// Integer division `a / b` at the receiver's width — the compiled-code half
/// of `/`, which (unlike `+`/`-`/`*`) can't be a bare LLVM instruction
/// because LLVM `sdiv` by zero is undefined behavior. Operands are raw
/// untagged `i64`s (the int branch of `compile-assoc` computes them the same
/// way `build-add` does), so there is no decode/encode here.
///
/// `sdiv` serves the unsigned types too: every width here is at most 32 and
/// a normalized unsigned value is a non-negative `i64`, so the signed
/// quotient *is* the unsigned one.
///
/// A zero divisor [`raise`]s `"divide by zero"` — the very message
/// `eval_int_builtin` gives the interpreted path, since the two paths running
/// the same form must fail the same way (`tests/runtime_error_parity_test.rs`
/// is the guard). The one other failure is the type's most negative value
/// over `-1`, whose quotient is one past its maximum; that is an overflow
/// rather than a divisor problem and says so. Unsigned types have no such
/// case.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_int_div(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_int_div: expected 3 arguments");
    }
    let (a, b) = (*args, *args.add(1));
    let (width, signed) = wsig(*args.add(2));
    if b == 0 {
        raise("divide by zero".to_string());
    }
    if signed && b == -1 && a == normalize(1i64 << (width - 1), width, true) {
        raise(format!("arithmetic overflow: {} / {}", a, b));
    }
    normalize(a / b, width, signed)
}

/// Floored remainder `a mod b` (CL `mod`) at the receiver's width — the
/// compiled-code half of `mod`, matching the interpreter's
/// `eval_int_builtin`. The result takes the sign of the divisor `b` (unlike
/// `srem`/`rem`, which take the sign of the dividend), so `-7 mod 3 = 2`.
/// Same zero-divisor handling as [`rt_int_div`] — a catchable
/// `"mod by zero"`, `eval_int_builtin`'s own wording; the most-negative-over-
/// `-1` case that overflows for `/` is mathematically `0` here, which
/// `wrapping_rem` answers directly. Raw-`i64` operand convention.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_int_mod(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_int_mod: expected 3 arguments");
    }
    let (a, b) = (*args, *args.add(1));
    let (width, signed) = wsig(*args.add(2));
    if b == 0 {
        raise("mod by zero".to_string());
    }
    let r = a.wrapping_rem(b);
    normalize(if r != 0 && (r < 0) != (b < 0) { r + b } else { r }, width, signed)
}

/// `(ash integer count)` at the receiver's width — the compiled-code half,
/// matching `eval_int_builtin`'s. Not a bare LLVM `shl`/`ashr` because both
/// are undefined behavior once the shift amount reaches the operand's bit
/// width; a count at or past the *type's* width is clamped here to its
/// limiting value (`0` left, and right either `0` or, for a signed negative
/// operand, the sign smeared to `-1`). `args[0]` is the integer, `args[1]`
/// the count (receiver-first, CL's own argument order).
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_int_ash(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_int_ash: expected 3 arguments");
    }
    let (n, count) = (*args, *args.add(1));
    let (width, signed) = wsig(*args.add(2));
    let w = i64::from(width);
    let r = if count >= 0 {
        if count >= w {
            0
        } else {
            n.wrapping_shl(count as u32)
        }
    } else if -count >= w {
        if signed && n < 0 {
            -1
        } else {
            0
        }
    } else {
        n >> (-count)
    };
    normalize(r, width, signed)
}

/// `(logbitp index integer)` at the receiver's width — the compiled-code
/// half, matching `eval_int_builtin`'s. Not a bare `lshr`+`and` because a
/// variable shift by an index at or past the width is undefined behavior in
/// LLVM; that case is handled directly (past the width there is no bit to
/// read — every one is the sign for a signed value, zero for an unsigned
/// one). `args[0]` is the index (receiver-first), `args[1]` the integer.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_int_logbitp(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_int_logbitp: expected 3 arguments");
    }
    // Integer first, position second — the order `registry::int_assoc`
    // registers and `eval_int_builtin` reads. The island passes the two
    // operands through unchanged, so this shim is where the order lives.
    let (n, index) = (*args, *args.add(1));
    let (width, signed) = wsig(*args.add(2));
    if index >= i64::from(width) {
        (signed && n < 0) as i64
    } else {
        (n >> index) & 1
    }
}

/// `logcount` at the receiver's width (population count of a nonnegative
/// integer, or of the 0-bits of a negative one — CL's own "infinite two's
/// complement" reading, cut to the type's own bits) — the compiled-code
/// half, matching `eval_int_builtin`'s. Not a bare `llvm.ctpop.i64` call
/// because the negative case first needs a conditional `lognot` and a mask,
/// cheaper to express directly here than as extra IR at every call site.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_int_logcount(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_int_logcount: expected 2 arguments");
    }
    let n = *args;
    let (width, _) = wsig(*args.add(1));
    let mask = if width >= 64 { !0u64 } else { (1u64 << width) - 1 };
    let bits = (n as u64) & mask;
    i64::from(if n >= 0 { bits.count_ones() } else { (!bits & mask).count_ones() })
}

/// `integer-length` at the receiver's width (bits needed, excluding sign) —
/// the compiled-code half, matching `eval_int_builtin`'s: `n`'s own bit
/// length when nonnegative, else `!n`'s (CL's negative-integer-length
/// identity), both read inside the type's own bits.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_int_integer_length(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_int_integer_length: expected 2 arguments");
    }
    let n = *args;
    let (width, _) = wsig(*args.add(1));
    let v = if n >= 0 { n as u64 } else { !(n as u64) };
    let mask = if width >= 64 { !0u64 } else { (1u64 << width) - 1 };
    i64::from(64 - (v & mask).leading_zeros())
}

/// `try-int->i8`/.../`try-int->u32`'s question, for compiled code — does
/// `args[0]` already hold a number the target width and signedness can
/// represent? `args[1]` is that target's `wsig` code.
///
/// The *answer* is all a compiled `try-int->W` needs from a shim: when it is
/// yes the value is already the result (an integer that fits its target is
/// unchanged by the cast — `interp::try_int_to_width`), so the island builds
/// the `some` box around the operand it already has rather than calling back
/// for a converted one.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_int_fits(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_int_fits: expected 2 arguments");
    }
    let n = *args;
    let (width, signed) = wsig(*args.add(1));
    (normalize(n, width, signed) == n) as i64
}

/// `try-int->char`'s question — is `args[0]` a Unicode scalar value?
///
/// Not merely "in `u32` range": the surrogate range `D800..=DFFF` is `u32` and
/// is not a `char`, which is exactly the case `int->char` raises on and this
/// answers `0` for (`interp::try_int_to_char`).
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_int_fits_char(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_int_fits_char: expected 1 argument");
    }
    let n = *args;
    let in_u32_range = n >= 0 && n <= i64::from(u32::MAX);
    (in_u32_range && char::from_u32(n as u32).is_some()) as i64
}

/// `try-float->f32`'s question — does `args[0]` survive the round trip
/// through binary32 unchanged?
///
/// Bit equality rather than `==` so the answer matches
/// `interp::try_float_to_f32` on the values `==` is silent about: a `NaN` is
/// never `==` itself but does round-trip, and `-0.0 == 0.0` while their bits
/// differ.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid `i64` holding an
/// `f64`'s bit pattern.
#[no_mangle]
pub unsafe extern "C" fn rt_f64_fits_f32(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_f64_fits_f32: expected 1 argument");
    }
    let v = f64::from_bits(*args as u64);
    (f64::from(v as f32).to_bits() == v.to_bits()) as i64
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

macro_rules! rt_f64_unary_exports {
    ($($export:ident => $method:ident,)*) => {$(
        #[no_mangle]
        pub unsafe extern "C" fn $export(args: *const i64, argc: u32) -> i64 {
            rt_f64_unary(args, argc, stringify!($export), f64::$method)
        }
    )*};
}
rt_f64_unary_exports! {
    rt_f64_tan => tan,
    rt_f64_asin => asin,
    rt_f64_acos => acos,
    rt_f64_atan => atan,
    rt_f64_sinh => sinh,
    rt_f64_cosh => cosh,
    rt_f64_tanh => tanh,
    rt_f64_asinh => asinh,
    rt_f64_acosh => acosh,
    rt_f64_atanh => atanh,
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
/// `compile-construct-sexpr`'s `path` arm). Unlike [`rt_intern_path`] (whose
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
/// extraction produces. An out-of-range index (negative included) [`raise`]s
/// the message the interpreter's `string_ref` raises for it, character for
/// character — the type system cannot express the bound, so both paths check
/// at run time and must agree on what they say when the check fails.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least 2 valid `i64`s,
/// the first encoding a `Value::Str`; a `Heap` must already be registered on
/// this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_str_ref(args: *const i64, argc: u32) -> i64 {
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
        // The length is only walked on the failing path, so an in-range
        // `ref` still costs one `chars().nth`.
        None => {
            let len = active_heap().string(id).chars().count();
            raise(format!("ref: index {} out of range (length {})", idx, len))
        }
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
    i64::from(*heap.string(a) == *heap.string(b))
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
    i64::from(*heap.string(a) < *heap.string(b))
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
    i64::from(heap.string(a).eq_ignore_ascii_case(&heap.string(b)))
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

/// `char::upcase`/`char::downcase` for compiled code. A compiled `char` is a
/// bare code point, so these take and return one.
///
/// ASCII-only, exactly as the interpreter's own arms are
/// (`Interp::eval_char_builtin`): `char::to_ascii_uppercase` leaves every
/// non-ASCII scalar value alone. The two tiers share the rule by sharing the
/// method, not by each spelling it.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_char_upcase(args: *const i64, argc: u32) -> i64 {
    char_map(args, argc, "rt_char_upcase", |c| c.to_ascii_uppercase() as i64)
}

/// See [`rt_char_upcase`].
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_char_downcase(args: *const i64, argc: u32) -> i64 {
    char_map(args, argc, "rt_char_downcase", |c| c.to_ascii_lowercase() as i64)
}

/// `char::alphap` — CL's `alpha-char-p`, ASCII-only like the interpreter's.
/// Returns a bare `0`/`1`, which is what a compiled `bool` is.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_char_alphap(args: *const i64, argc: u32) -> i64 {
    char_map(args, argc, "rt_char_alphap", |c| i64::from(c.is_ascii_alphabetic()))
}

/// `char::digitp` — the `bool` predicate, not CL's weight-returning
/// `digit-char-p` (which is `digit-weight`, a prelude function). See
/// [`rt_char_alphap`].
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C" fn rt_char_digitp(args: *const i64, argc: u32) -> i64 {
    char_map(args, argc, "rt_char_digitp", |c| i64::from(c.is_ascii_digit()))
}

/// The one-code-point-in, one-word-out shape the four `char` shims above
/// share. A compiled `char` always holds a valid scalar value; a word that is
/// not one would be an internal invariant break, and is passed through
/// unchanged rather than invented into something else — the same reading
/// [`rt_char_equalp`] takes of the same impossibility.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
unsafe fn char_map(args: *const i64, argc: u32, who: &str, f: impl FnOnce(char) -> i64) -> i64 {
    if argc < 1 {
        fatal(&format!("{}: expected 1 argument", who));
    }
    let a = *args;
    match char::from_u32(a as u32) {
        Some(c) => f(c),
        None => a,
    }
}

/// `int->char` for compiled code: the identity on the word, plus the
/// validity check that makes it a `char` at all.
///
/// The check is why this is a shim rather than the no-op `char->int` is.
/// `Interp`'s own `int_to_char` panics on a code point that is not a Unicode
/// scalar value, and a compiled body has to raise the same, catchable error —
/// hence `extern "C-unwind"`, like every other raising shim.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least 1 valid `i64`.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_int_to_char(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_int_to_char: expected 1 argument");
    }
    let n = *args;
    let valid = n >= 0 && n <= i64::from(u32::MAX) && char::from_u32(n as u32).is_some();
    if !valid {
        raise(format!("int->char: {} is not a valid Unicode scalar value", n));
    }
    n
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

/// The shared body of [`rt_str_upcase`] and [`rt_str_downcase`] — decode,
/// map, allocate.
///
/// # Safety
///
/// Same as [`rt_str_eq`], for `args[0]` alone.
unsafe fn str_map_case(args: *const i64, argc: u32, who: &str, up: bool) -> i64 {
    if argc < 1 {
        fatal(&format!("{who}: expected 1 argument"));
    }
    let id = match decode(*args) {
        Value::Str(id) => id,
        _ => fatal(&format!("{who}: argument is not a Str")),
    };
    let heap = active_heap();
    let s = if up { heap.string(id).to_ascii_uppercase() } else { heap.string(id).to_ascii_lowercase() };
    encode(heap.alloc_string(s))
}

/// `string::upcase` for compiled code — `args[0]`'s content with every ASCII
/// letter folded up, as a freshly allocated string, matching the
/// interpreter's own arm in `eval_builtin_method`.
///
/// ASCII-only on both tiers because both tiers run this rule; `char::upcase`
/// (`rt_char_upcase`) has the same contract for the same reason.
///
/// # Safety
///
/// Same as [`str_map_case`].
#[no_mangle]
pub unsafe extern "C" fn rt_str_upcase(args: *const i64, argc: u32) -> i64 {
    str_map_case(args, argc, "rt_str_upcase", true)
}

/// `string::downcase` for compiled code — the folding-down counterpart of
/// [`rt_str_upcase`].
///
/// # Safety
///
/// Same as [`str_map_case`].
#[no_mangle]
pub unsafe extern "C" fn rt_str_downcase(args: *const i64, argc: u32) -> i64 {
    str_map_case(args, argc, "rt_str_downcase", false)
}

/// `str::substring` for compiled code — the `[start, end)` character range of
/// `args[0]`'s content as a freshly allocated string, matching the
/// interpreter's own `string_substring`. `args[1]`/`args[2]` are bare
/// (untagged) `i64` indices, the same convention [`rt_str_ref`] uses, and
/// they count *characters*, not bytes.
///
/// An invalid range [`raise`]s the message `string_substring` raises for it,
/// the same parity [`rt_str_ref`] keeps for its own bounds check.
///
/// # Safety
///
/// `argc` must be `>= 3` and `args` must point to at least 3 valid `i64`s,
/// the first encoding a `Value::Str`; a `Heap` must already be registered on
/// this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_str_substring(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_str_substring: expected 3 arguments");
    }
    let id = match decode(*args) {
        Value::Str(id) => id,
        _ => fatal("rt_str_substring: first argument is not a Str"),
    };
    let start = *args.add(1);
    let end = *args.add(2);
    let heap = active_heap();
    let chars: Vec<char> = heap.string(id).chars().collect();
    let len = chars.len() as i64;
    if start < 0 || end > len || start > end {
        raise(format!("substring: invalid range {}..{} (length {})", start, end, len));
    }
    let s: String = chars[start as usize..end as usize].iter().collect();
    encode(heap.alloc_string(s))
}

// ---- random-state -------------------------------------------------------
//
// A `random-state` is an ordinary boxed heap object holding one `u64` seed,
// so nothing about it lives outside the heap and all three of its builtins
// lower to shims here.

/// One step of a 64-bit xorshift generator (period `2^64 - 1` over the
/// nonzero states — these exact shift/xor constants are a full-cycle
/// permutation of them, so a nonzero seed can never reach `0`).
///
/// Public, and the interpreter's `random-state-next` calls it rather than
/// keeping its own copy: a compiled draw and an interpreted draw from the
/// same seed must produce the same number, which two implementations of the
/// same three lines cannot promise.
pub fn xorshift64_step(x: u64) -> u64 {
    let mut x = x;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// A fresh entropy seed for `make-random-state-fresh` — `| 1` guarantees
/// non-zero (the one fixed point [`xorshift64_step`] can't escape).
///
/// Public for the same reason [`xorshift64_step`] is: the interpreter's
/// `eval_make_random_state_fresh` calls it rather than keeping a second copy.
pub fn fresh_random_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        // A clock set before 1970 is as good a source of entropy as any.
        .map_or_else(|before| before.duration().as_nanos() as u64, |d| d.as_nanos() as u64)
        | 1
}

/// The state a user-supplied seed names, for `seed-random-state`.
///
/// Not the seed itself. [`xorshift64_step`] has exactly one fixed point, 0,
/// and a stream started there stays there forever, so the state space is
/// `1..=u64::MAX` and *some* map from the user's integer into it is
/// unavoidable. The obvious one, `seed as u64 | 1`, is wrong: it sends every
/// even seed onto its odd neighbour, so `(seed-random-state 0)` and
/// `(seed-random-state 1)` would name the same stream and half of all seeds
/// would be aliases of the other half. A caller reaching for a seed wants
/// reproducibility, and silently sharing a stream with a different seed is
/// the one property that breaks it.
///
/// So: SplitMix64's finalizer, which is a *bijection* on `u64`. Exactly one
/// input maps to 0 and is redirected to a constant; every other pair of
/// distinct seeds gets a distinct state. One collision is the least any map
/// into a space one element smaller than its domain can have.
///
/// The seed arrives as `i64` (the language hands over an `i32`, sign
/// extended) and is reinterpreted, not clamped, so `-1` and `1` are
/// different seeds.
pub fn seeded_random_state(seed: i64) -> u64 {
    let mut z = (seed as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    if z == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        z
    }
}

/// `make-random-state-fresh` for compiled code — a brand new, independently
/// seeded stream (`eval_make_random_state_fresh`). Nullary; the result is a
/// fresh allocation, as unrooted as [`rt_str_new`]'s until a caller protects
/// it.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread. Takes no arguments,
/// so `args`/`argc` are unread.
#[no_mangle]
pub unsafe extern "C" fn rt_make_random_state_fresh(_args: *const i64, _argc: u32) -> i64 {
    encode(active_heap().alloc_random_state(fresh_random_seed()))
}

/// `seed-random-state` for compiled code — the stream a given seed names
/// (`eval_seed_random_state`), through [`seeded_random_state`]. Like
/// [`rt_make_random_state_fresh`] the result is a fresh, still-unrooted
/// allocation.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args[0]` must decode to an integer; a `Heap`
/// must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_seed_random_state(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_seed_random_state: expected 1 argument");
    }
    let seed = *args;
    encode(active_heap().alloc_random_state(seeded_random_state(seed)))
}

/// Decodes `args[idx]` as a boxed random-state.
///
/// # Safety
///
/// `args` must point to at least `idx + 1` valid `i64`s, `args[idx]`
/// decoding to a `Value::Boxed` random-state; a `Heap` must already be
/// registered on this thread.
unsafe fn random_state_arg(args: *const i64, idx: isize, who: &str) -> BoxId {
    match decode(*args.offset(idx)) {
        Value::Boxed(id) if active_heap().is_random_state(id) => id,
        _ => fatal(&format!("{who}: argument is not a random-state")),
    }
}

/// `random-state-next` for compiled code — advances `args[0]`'s seed one
/// xorshift step and returns the draw reduced into `[0, args[1])`, matching
/// the interpreter's own `eval_random_state_next` (`args[1]` is a bare
/// `i64` bound, and the result is a bare `i64` too). A non-positive bound
/// [`raise`]s that same function's message, so `(random 0)` fails
/// identically whichever tier the prelude's `random` is running on.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args[0]` must decode to a boxed random-state;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_random_state_next(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_random_state_next: expected 2 arguments");
    }
    let id = random_state_arg(args, 0, "rt_random_state_next");
    let bound = *args.add(1);
    if bound <= 0 {
        raise(format!("random: bound must be positive, got {}", bound));
    }
    let heap = active_heap();
    let next = xorshift64_step(heap.random_state_seed(id));
    heap.set_random_state_seed(id, next);
    (next % bound as u64) as i64
}

/// `random-state-copy` for compiled code — a new state starting where
/// `args[0]` is now, so later draws against either never affect the other
/// (`eval_random_state_copy`). The result is a fresh allocation, tagged and
/// as unrooted as [`rt_str_new`]'s until a caller protects it.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args[0]` must decode to a boxed random-state;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_random_state_copy(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_random_state_copy: expected 1 argument");
    }
    let id = random_state_arg(args, 0, "rt_random_state_copy");
    let heap = active_heap();
    let seed = heap.random_state_seed(id);
    encode(heap.alloc_random_state(seed))
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

// Global id -> the `Heap::permanent_root` position it landed at, in
// `rt_global_new` call order — see that function's doc comment — now
// `shared::RtShared::globals` (`docs/dev/os-threads-design.md` §5/Phase
// 1d), reached through `shared::rt_shared()`. Thread-local for the same
// cross-test-isolation reason `ACTIVE_HEAP` is (module doc comment); unlike
// `ACTIVE_HEAP`, nothing re-points this automatically on every call, so
// [`reset_global_table`] must be called whenever a fresh `Heap` begins its
// lifetime.

/// Clears this thread's global-id table. Must be called whenever a fresh
/// `Heap`/`Interp` pair begins its lifetime (`typelisp::eval::Interp::new`)
/// — otherwise a stale entry left over from an earlier `Heap` on a reused
/// `cargo test` worker thread would resolve to a permanent-root position
/// that has nothing to do with the *current* `Heap`'s (empty, freshly
/// created) `permanent_roots`. AOT's standalone executable never needs to
/// call this itself: [`rt_heap_init`] runs exactly once, before any
/// `rt_global_new` call, in a process that only ever has the one `Heap`.
pub fn reset_global_table() {
    shared::rt_shared().globals.write().clear();
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
    let shared = shared::rt_shared();
    let mut globals = shared.globals.write();
    globals.push(perm_idx);
    globals.len() - 1
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
    shared::rt_shared().globals.read().get(id).copied()
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
// Trait objects and vtables
// ===========================================================================
//
// A trait object is a `BoxedObj::Dyn` fat box: a vtable id plus the concrete
// value. The vtable itself lives *outside* the heap, here — one table per
// (concrete type, trait) pair, holding raw native function pointers in the
// trait's declared method order (`TraitDef::method_order`). A call site knows
// its slot statically, so dispatch is `rt_dyn_vtable` -> naming the slot to
// the driver (`rt_frame_dyn_call`), with no lookup by name or type at run
// time — and the driver is also where a slot whose implementation is
// *interpreted* is handled. Ids are assigned by the interpreter
// (`Interp::vtable_id_for`), which keeps its own parallel table of
// `(type, method)` identities for tree-walking calls.

// vtable id -> slot -> native entry point (`compiled_fn_type`: `i64
// f(i64* args, i32 argc)`), or 0 for a slot never filled in — now
// `shared::RtShared::vtables` (`docs/dev/os-threads-design.md` §5/Phase
// 1d). Each slot is `(entry address, the ABI that address answers to)`.
// The pair travels together because an address alone does not say how to
// call it, and calling one convention as the other is not a type error
// anywhere -- it is a wrong answer or a crash.
//
// `(vtable id, trait id)` -> the vtable id of the *same concrete type* for
// that trait — the supertrait upcast table ([`rt_dyn_upcast`]), now
// `shared::RtShared::upcasts`. Filled in by the interpreter from
// `Expr::DynBox::supers` ([`upcast_define`]) and by AOT startup
// ([`rt_upcast_set`]), and reset alongside the vtables, whose id space it
// is keyed by.

/// Clears this thread's vtable and upcast tables — the trait-object
/// counterpart of [`reset_global_table`], called from `Interp::new` for the
/// same reason: vtable ids are per-`Interp`, so a stale table from an earlier
/// one on a reused `cargo test` worker thread would answer with function
/// pointers into a JIT module that has since been dropped.
pub fn reset_vtable_table() {
    let shared = shared::rt_shared();
    shared.vtables.write().clear();
    shared.upcasts.write().clear();
}

/// Records that a trait object dispatching through vtable `from` becomes one
/// dispatching through vtable `to` when upcast to the trait interned as
/// `trait_id` — the compiled tier's copy of `Interp::dyn_upcasts`, published
/// wherever the interpreter interns a boxing site's supertrait tables.
pub fn upcast_define(from: u32, trait_id: u32, to: u32) {
    shared::rt_shared().upcasts.write().insert((from, trait_id), to);
}

/// Installs (or replaces) vtable `id`'s slots. Called from the interpreter
/// once a compilation unit's function addresses are final
/// (`Interp::compile_scc`) and from AOT startup; a slot holding 0 means "not
/// compiled", which the driver answers by asking the interpreter
/// ([`reify_dyn_slot`]).
pub fn vtable_define(id: u32, slots: Vec<(usize, u8)>) {
    let shared = shared::rt_shared();
    let mut t = shared.vtables.write();
    if t.len() <= id as usize {
        t.resize(id as usize + 1, Vec::new());
    }
    t[id as usize] = slots;
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

pub(crate) fn vtable_slot_addr(id: usize, slot: usize) -> (usize, u8) {
    shared::rt_shared()
        .vtables
        .read()
        .get(id)
        .and_then(|s| s.get(slot).copied())
        .unwrap_or((0, typelisp_abi::BODY_ABI_CLASSIC))
}

/// Asks the interpreter for the closure an unfilled vtable slot stands for.
///
/// In an AOT executable every method the program can reach is compiled and no
/// interpreter exists to ask, so a missing hook is an invariant break rather
/// than a limitation — the same reading [`apply_on_this_frame`] makes of a
/// missing apply hook.
pub(crate) unsafe fn reify_dyn_slot(vtable: u32, slot: u32, what: &str) -> i64 {
    match DYN_SLOT_CLOSURE.with(|cell| cell.get()) {
        Some(hook) => hook(vtable, slot),
        None => fatal(&format!(
            "{}: vtable slot {}/{} is empty and no interpreter is registered on this thread",
            what, vtable, slot
        )),
    }
}

/// How [`reify_dyn_slot`] asks the interpreter for the closure standing
/// behind an unfilled vtable slot — `(vtable id, slot) -> a tagged
/// interpreted closure value`.
pub type DynSlotClosureFn = unsafe extern "C-unwind" fn(u32, u32) -> i64;

thread_local! {
    static DYN_SLOT_CLOSURE: Cell<Option<DynSlotClosureFn>> = const { Cell::new(None) };
}

/// Registers this thread's interpreter re-entry for `:dyn` dispatch —
/// `typelisp::eval::Interp` installs it alongside
/// [`set_apply_interpreted`], for the same reason and at the same moment.
pub fn set_dyn_slot_closure(f: Option<DynSlotClosureFn>) {
    DYN_SLOT_CLOSURE.with(|cell| cell.set(f));
}

/// What a function value turns out to be — the question `apply` asks and a
/// direct call never has to.
pub(crate) enum Callee {
    /// A compiled body under the coroutine ABI, with its captures. It can
    /// only be *driven*, so it joins a chain.
    Coroutine { f: crate::coroutine::CoroutineFn, env: Vec<i64> },
    /// A compiled body under the original ABI, with its captures. It runs to
    /// completion by construction, so calling it is enough.
    Classic { f: ClassicClosureFn, env: Vec<i64> },
    /// Not compiled. Only an interpreter can run it, and only whoever owns a
    /// continuation stack can let it suspend.
    Interpreted,
}

/// A classic-ABI closure body: arguments and captures as two arrays.
pub(crate) type ClassicClosureFn = unsafe extern "C-unwind" fn(*const i64, u32, *const i64, u32) -> i64;

/// A classic-ABI method body: one argument array. Every address in a vtable
/// comes from `CompiledFn::address` for a method compiled under the plain
/// `compiled_fn_type` signature.
pub(crate) type ClassicMethodFn = unsafe extern "C-unwind" fn(*const i64, u32) -> i64;

/// Reads a function value's captures and decides who can run its body.
///
/// The captures come out in the closure box's own mask: a masked slot is a
/// tagged value and an unmasked one a raw word, the same distinction every
/// compiled boundary makes and for the same reason — a machine word does not
/// say which it is.
///
/// # Safety
///
/// `closure` must be a tagged compiled- or interpreted-closure box in the
/// heap registered on this thread.
pub(crate) unsafe fn resolve_closure(closure: i64) -> Callee {
    let id = match decode(closure) {
        Value::Boxed(id) => id,
        other => fatal(&format!("apply: callee is not a function value: {:?}", other)),
    };
    let heap = active_heap();
    if !heap.is_compiled_closure(id) {
        return Callee::Interpreted;
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
                    other => fatal(&format!("apply: unmasked closure slot {} holds a non-raw value {:?}", i, other)),
                }
            }
        })
        .collect();
    let fn_ptr = heap.compiled_closure_fnptr(id);
    if heap.compiled_closure_body_abi(id) == typelisp_abi::BODY_ABI_COROUTINE {
        // SAFETY: `build-make-closure` only ever names a function it declared
        // under `coroutine_fn_type`, which is what `rt_coroutine_closure_new`
        // records by being the entry point that was called.
        Callee::Coroutine { f: std::mem::transmute(fn_ptr), env }
    } else {
        // SAFETY: `rt_closure_new` is the only producer of a classic-ABI
        // `BoxedObj::CompiledClosure`, and `build-make-closure` only ever
        // hands it a function compiled under `compiled_fn_type_with_env`.
        Callee::Classic { f: std::mem::transmute(fn_ptr), env }
    }
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
    let to = shared::rt_shared().upcasts.read().get(&(from, trait_id)).copied();
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
/// `args[2]`, which answers to the ABI in `args[3]`, extending the table as
/// needed. Used by AOT startup, where the pointers are `ptrtoint` constants
/// the linker resolves; the JIT fills tables through [`vtable_define`]
/// instead.
///
/// # Safety
///
/// `argc` must be `>= 4` and `args` must point to at least 4 valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_vtable_set(args: *const i64, argc: u32) -> i64 {
    if argc < 4 {
        fatal("rt_vtable_set: expected 4 arguments (the vtable id, the slot, the function pointer and its ABI)");
    }
    let id = *args as usize;
    let slot = *args.add(1) as usize;
    let ptr = *args.add(2) as usize;
    let body_abi = *args.add(3) as u8;
    let shared = shared::rt_shared();
    let mut t = shared.vtables.write();
    if t.len() <= id {
        t.resize(id + 1, Vec::new());
    }
    let slots = &mut t[id];
    if slots.len() <= slot {
        slots.resize(slot + 1, (0, typelisp_abi::BODY_ABI_CLASSIC));
    }
    slots[slot] = (ptr, body_abi);
    0
}

// ---- stream/file builtins ----------------------------------------------
//
// One shim per builtin (`crate::stream_builtin::stream_builtin` is the
// implementation both these and the interpreter call). What each shim adds is
// the *representation* conversion the compiled ABI needs and the interpreter
// does not: an `i64` handle, an `i32` mode, a `bool` and a `char` are bare
// machine words in compiled code, while a `string` and every `Result` are
// tagged heap values. Which is which comes from the builtin's own signature
// (`registry::register_stream_builtins`), so it is spelled out per shim
// rather than guessed from the value.


/// Generates one `rt_stream_*`/`rt_file_*` shim.
///
/// `$arg` names how each parameter arrives (`int`/`str`/`char`), `$ret` how
/// the result leaves (`tagged` for a heap value, `raw` for a bare `i64`,
/// `bool` for a bare `0`/`1`).
macro_rules! stream_shim {
    // The short form: a `stream-*`/`file-*` builtin. The `net-*` family
    // names its own dispatcher with `via` below — same conversions, a
    // different table.
    ($shim:ident, $name:literal, [$($arg:ident),*], $ret:ident) => {
        stream_shim!($shim, $name, [$($arg),*], $ret, via crate::stream_builtin::stream_builtin);
    };
    ($shim:ident, $name:literal, [$($arg:ident),*], $ret:ident, via $dispatch:path) => {
        /// A `stream-*`/`file-*` builtin for compiled code — see
        /// [`crate::stream_builtin::stream_builtin`], which is also what the
        /// interpreter calls.
        ///
        /// # Safety
        ///
        /// `args` must point to at least as many valid `i64`s as this
        /// builtin has parameters, each in the representation its signature
        /// gives it; a `Heap` must already be registered on this thread.
        #[no_mangle]
        pub unsafe extern "C" fn $shim(args: *const i64, argc: u32) -> i64 {
            // A nullary builtin reads neither, and `decoded` stays empty.
            let _ = (args, argc);
            #[allow(unused_mut)]
            let mut decoded: Vec<Value> = Vec::new();
            $(
                let i = decoded.len();
                if argc as usize <= i {
                    fatal(concat!(stringify!($shim), ": too few arguments"));
                }
                decoded.push(stream_shim!(@arg $arg, args, i));
            )*
            match $dispatch(active_heap(), $name, &decoded) {
                Some(Ok(v)) => stream_shim!(@ret $ret, v),
                Some(Err(e)) => fatal(&e),
                None => fatal(concat!(stringify!($shim), ": ", $name, " is not a builtin of its dispatcher")),
            }
        }
    };
    // A stream handle and an `open` mode are both bare `i64`s.
    (@arg int, $args:expr, $i:expr) => { Value::Int(*$args.add($i)) };
    (@arg str, $args:expr, $i:expr) => { tagged_arg($args, $i) };
    // Any other heap value — a `Vector<int>` datagram — crosses the same way.
    (@arg tagged, $args:expr, $i:expr) => { tagged_arg($args, $i) };
    // A `bool` is a bare 0/1 word, as `@ret bool` writes it.
    (@arg bool, $args:expr, $i:expr) => { Value::Bool(*$args.add($i) != 0) };
    (@arg char, $args:expr, $i:expr) => {
        match char::from_u32(*$args.add($i) as u32) {
            Some(c) => Value::Char(c),
            None => fatal("stream shim: argument is not a valid char scalar value"),
        }
    };
    (@ret tagged, $v:expr) => { encode($v) };
    (@ret raw, $v:expr) => {
        match $v {
            Value::Int(n) => n,
            other => fatal(&format!("stream shim: expected an i64 result, got {:?}", other)),
        }
    };
    (@ret bool, $v:expr) => {
        match $v {
            Value::Bool(b) => i64::from(b),
            other => fatal(&format!("stream shim: expected a bool result, got {:?}", other)),
        }
    };
}

/// The readtable builtins for compiled code — see
/// [`crate::readtable::readtable_builtin`], which is also what the
/// interpreter calls. A character argument is a bare scalar value and a
/// function argument is tagged, per the compiled calling convention;
/// `set-*` answers `()` (the zero word) and `get-*` a tagged `Option`.
///
/// # Safety
///
/// `args` must point to at least as many valid `i64`s as the builtin has
/// parameters, each in the representation its signature gives it; a `Heap`
/// must already be registered on this thread.
macro_rules! readtable_shim {
    ($shim:ident, $name:literal, $chars:literal, $fn_arg:literal, $ret:ident) => {
        #[doc = concat!("`", $name, "` for compiled code.")]
        ///
        /// # Safety
        ///
        /// See [`readtable_shim`].
        #[no_mangle]
        pub unsafe extern "C" fn $shim(args: *const i64, argc: u32) -> i64 {
            let want = $chars + if $fn_arg { 1 } else { 0 };
            if (argc as usize) < want {
                fatal(concat!(stringify!($shim), ": too few arguments"));
            }
            let mut decoded: Vec<Value> = Vec::new();
            for i in 0..$chars {
                match char::from_u32(*args.add(i) as u32) {
                    Some(c) => decoded.push(Value::Char(c)),
                    None => fatal(concat!(stringify!($shim), ": argument is not a valid char scalar value")),
                }
            }
            if $fn_arg {
                decoded.push(tagged_arg(args, $chars));
            }
            match crate::readtable::readtable_builtin(active_heap(), $name, &decoded) {
                Some(Ok(v)) => readtable_shim!(@ret $ret, v),
                Some(Err(e)) => fatal(&e),
                None => fatal(concat!(stringify!($shim), ": ", $name, " is not a readtable builtin")),
            }
        }
    };
    (@ret unit, $v:expr) => {{
        let _ = $v;
        0
    }};
    (@ret tagged, $v:expr) => { encode($v) };
}

readtable_shim!(rt_set_macro_character, "set-macro-character", 1, true, unit);
readtable_shim!(rt_get_macro_character, "get-macro-character", 1, false, tagged);
readtable_shim!(rt_set_dispatch_macro_character, "set-dispatch-macro-character", 2, true, unit);
readtable_shim!(rt_get_dispatch_macro_character, "get-dispatch-macro-character", 2, false, tagged);

stream_shim!(rt_stream_stdin, "stream-stdin", [], raw);
stream_shim!(rt_stream_stdout, "stream-stdout", [], raw);
stream_shim!(rt_stream_stderr, "stream-stderr", [], raw);
stream_shim!(rt_stream_string_input, "stream-string-input", [str], raw);
stream_shim!(rt_stream_string_output, "stream-string-output", [], raw);
stream_shim!(rt_stream_open_file, "stream-open-file", [str, int], tagged);
stream_shim!(rt_stream_close, "stream-close", [int], tagged);
stream_shim!(rt_stream_open_p, "stream-open-p", [int], bool);
stream_shim!(rt_stream_describe, "stream-describe", [int], tagged);
stream_shim!(rt_stream_input_p, "stream-input-p", [int], tagged);
stream_shim!(rt_stream_output_p, "stream-output-p", [int], tagged);
stream_shim!(rt_stream_read_char, "stream-read-char", [int], tagged);
stream_shim!(rt_stream_read_byte, "stream-read-byte", [int], tagged);
stream_shim!(rt_stream_write_byte, "stream-write-byte", [int, int], tagged);
stream_shim!(rt_stream_unread_char, "stream-unread-char", [int, char], tagged);
stream_shim!(rt_stream_listen, "stream-listen", [int], tagged);
stream_shim!(rt_stream_position, "stream-position", [int], tagged);
stream_shim!(rt_stream_write_string, "stream-write-string", [int, str], tagged);
stream_shim!(rt_stream_at_line_start, "stream-at-line-start", [int], tagged);
stream_shim!(rt_stream_finish_output, "stream-finish-output", [int], tagged);
stream_shim!(rt_stream_take_output_string, "stream-take-output-string", [int], tagged);
stream_shim!(rt_file_exists_p, "file-exists-p", [str], bool);
stream_shim!(rt_file_delete, "file-delete", [str], tagged);
stream_shim!(rt_file_rename, "file-rename", [str, str], tagged);
stream_shim!(rt_file_truename, "file-truename", [str], tagged);
stream_shim!(rt_file_modified_date, "file-modified-date", [str], tagged);
stream_shim!(rt_file_owner_name, "file-owner-name", [str], tagged);
stream_shim!(rt_file_directory_p, "file-directory-p", [str], bool);
stream_shim!(rt_file_list_directory, "file-list-directory", [str], tagged);
stream_shim!(rt_file_create_directories, "file-create-directories", [str], tagged);

// The sockets: `crate::net_builtin::net_builtin` is the implementation both
// these and the interpreter call. `net-wait` has no shim here — it suspends,
// and `crate::coroutine::rt_suspend_io` is its compiled form.
stream_shim!(rt_net_resolve_begin, "net-resolve-begin", [str, int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_resolve_finish, "net-resolve-finish", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_connect_begin, "net-connect-begin", [str], tagged, via crate::net_builtin::net_builtin);
// The `Option<string>` arguments niche (`typelisp_mem::option`): `none` is
// the empty word and `some` the string, so they cross as any tagged word.
stream_shim!(rt_net_tls_start, "net-tls-start", [int, str, tagged, tagged, tagged], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_tls_listen, "net-tls-listen", [str, int, str, str, tagged], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_socket_error, "net-socket-error", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_peer_subject, "net-peer-subject", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_server_name, "net-server-name", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_set_nodelay, "net-set-nodelay", [int, bool], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_set_keepalive, "net-set-keepalive", [int, bool], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_set_keepalive_period, "net-set-keepalive-period", [int, int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_tls_add_certificate, "net-tls-add-certificate", [int, str, str, str], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_tls_handshake, "net-tls-handshake", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_udp_bind, "net-udp-bind", [str, int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_udp_send_to, "net-udp-send-to", [int, str, tagged], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_udp_recv, "net-udp-recv", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_udp_last_sender, "net-udp-last-sender", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_connect_finish, "net-connect-finish", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_listen, "net-listen", [str, int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_unix_connect, "net-unix-connect", [str], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_unix_listen, "net-unix-listen", [str], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_accept, "net-accept", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_fill, "net-fill", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_pop_byte, "net-pop-byte", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_pop_char, "net-pop-char", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_buffered_p, "net-buffered-p", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_push_string, "net-push-string", [int, str], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_push_byte, "net-push-byte", [int, int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_flush, "net-flush", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_shutdown_write, "net-shutdown-write", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_local_address, "net-local-address", [int], tagged, via crate::net_builtin::net_builtin);
stream_shim!(rt_net_peer_address, "net-peer-address", [int], tagged, via crate::net_builtin::net_builtin);


#[cfg(test)]
mod tests {
    use typelisp_mem::{Heap, PathId, StrId, Value};

    use super::{
        active_heap, decode, encode, reset_global_table, rt_car, rt_cdr, rt_cons, rt_global_get, rt_global_new, rt_global_set,
        rt_heap_init, rt_heap_live_count, rt_ping, rt_pop_sexpr_root, rt_push_permanent_sexpr_root, rt_push_sexpr_root, rt_root_count,
        rt_char_equalp, rt_set_car, rt_set_cdr, rt_set_sexpr_root, rt_str_append, rt_str_eq, rt_str_equalp, rt_str_length, rt_str_lt,
        rt_str_new, rt_str_ref, rt_hashtable_bucket_count, rt_hashtable_bucket_delete, rt_hashtable_bucket_key,
        rt_hashtable_bucket_put, rt_hashtable_bucket_value, rt_hashtable_count, rt_hashtable_entries,
        rt_hashtable_keys, rt_hashtable_new, rt_data_field, rt_data_new, rt_data_variant,
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
        for n in [0i64, 1, -1, 42, -42, i64::MIN >> 1, i64::MAX >> 1] {
            assert_eq!(decode(encode(Value::Int(n))), Value::Int(n), "Int({})", n);
        }
        for c in ['a', 'Z', '0', '\u{10FFFF}', '\0'] {
            assert_eq!(decode(encode(Value::Char(c))), Value::Char(c), "Char({:?})", c);
        }
        assert_eq!(decode(encode(Value::Bool(true))), Value::Bool(true));
        assert_eq!(decode(encode(Value::Bool(false))), Value::Bool(false));
        assert_eq!(decode(encode(Value::Empty)), Value::Empty);
        // A real interned symbol: the tagged payload is now its header's
        // address, so there is no id to fabricate.
        let sym = typelisp_mem::symbols::intern("round-trip");
        assert_eq!(decode(encode(Value::Symbol(sym))), Value::Symbol(sym));
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
            Value::Str(id) => assert_eq!(&*unsafe { active_heap() }.string(id), "hi"),
            other => panic!("expected a Str, got {:?}", other),
        }
    }

    #[test]
    fn rt_str_new_with_no_characters_builds_an_empty_string() {
        let mut heap = Heap::with_capacity(8);
        set_active_heap(&mut heap as *mut Heap);

        let tagged = unsafe { rt_str_new(std::ptr::null(), 0) };
        match decode(tagged) {
            Value::Str(id) => assert_eq!(&*unsafe { active_heap() }.string(id), ""),
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
            Value::Str(id) => assert_eq!(&*unsafe { active_heap() }.string(id), "foobar"),
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
                assert_eq!(&*h.struct_type_name(id), "point");
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
                assert_eq!(&*h.enum_type_name(id), "option");
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

        let ht = unsafe { rt_hashtable_new([make_str("hashtable<i32,i32>")].as_ptr(), 1) };
        assert_eq!(unsafe { rt_hashtable_count([ht].as_ptr(), 1) }, 0, "a fresh map is empty");

        // Two int->int entries (keys/values as raw tagged `Sexpr` ints). The
        // hash and the index are the caller's: this layer never computes
        // either, which is what lets a `defstruct` be a key.
        assert_eq!(unsafe { rt_hashtable_bucket_put([ht, 1, 0, encode(Value::Int(1)), encode(Value::Int(10))].as_ptr(), 5) }, 0);
        assert_eq!(unsafe { rt_hashtable_bucket_put([ht, 2, 0, encode(Value::Int(2)), encode(Value::Int(20))].as_ptr(), 5) }, 0);
        // Writing at an index that already exists overwrites rather than
        // appending, so the count doesn't grow.
        assert_eq!(unsafe { rt_hashtable_bucket_put([ht, 1, 0, encode(Value::Int(1)), encode(Value::Int(99))].as_ptr(), 5) }, 0);
        assert_eq!(unsafe { rt_hashtable_count([ht].as_ptr(), 1) }, 2, "two distinct keys");

        // `keys` builds a `Vector` whose field count matches the entry count.
        let keys = unsafe { rt_hashtable_keys([ht, make_str("vector<i32>")].as_ptr(), 2) };
        match decode(keys) {
            Value::Boxed(id) => assert_eq!(unsafe { active_heap() }.struct_field_count(id), 2),
            other => panic!("expected a boxed vector, got {:?}", other),
        }

        // `entries` builds a `Vector` of `cons-cell`s, one per entry.
        let entries = unsafe { rt_hashtable_entries([ht, make_str("vector<cons-cell<i32,i32>>")].as_ptr(), 2) };
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

    /// Two keys that share a hash sit in one bucket, in insertion order, and
    /// deleting one keeps the other — the collision handling the prelude's
    /// `get`/`set`/`remove` walk with the key type's own `equals`.
    #[test]
    fn rt_hashtable_bucket_holds_colliding_keys_and_deletes_one() {
        let mut heap = Heap::with_capacity(64);
        set_active_heap(&mut heap as *mut Heap);

        let ht = unsafe { rt_hashtable_new([make_str("hashtable<i32,i32>")].as_ptr(), 1) };
        let hash = 5;
        assert_eq!(unsafe { rt_hashtable_bucket_count([ht, hash].as_ptr(), 2) }, 0, "no bucket yet");

        unsafe { rt_hashtable_bucket_put([ht, hash, 0, encode(Value::Int(7)), encode(Value::Int(70))].as_ptr(), 5) };
        unsafe { rt_hashtable_bucket_put([ht, hash, 1, encode(Value::Int(8)), encode(Value::Int(80))].as_ptr(), 5) };
        assert_eq!(unsafe { rt_hashtable_bucket_count([ht, hash].as_ptr(), 2) }, 2);
        assert_eq!(unsafe { rt_hashtable_count([ht].as_ptr(), 1) }, 2, "the table counts every bucket");

        assert_eq!(decode(unsafe { rt_hashtable_bucket_key([ht, hash, 0].as_ptr(), 3) }), Value::Int(7));
        assert_eq!(decode(unsafe { rt_hashtable_bucket_value([ht, hash, 1].as_ptr(), 3) }), Value::Int(80));

        unsafe { rt_hashtable_bucket_delete([ht, hash, 0].as_ptr(), 3) };
        assert_eq!(unsafe { rt_hashtable_bucket_count([ht, hash].as_ptr(), 2) }, 1, "the other survives");
        assert_eq!(decode(unsafe { rt_hashtable_bucket_key([ht, hash, 0].as_ptr(), 3) }), Value::Int(8), "and moves up");

        unsafe { rt_hashtable_bucket_delete([ht, hash, 0].as_ptr(), 3) };
        assert_eq!(unsafe { rt_hashtable_count([ht].as_ptr(), 1) }, 0, "an emptied bucket is dropped");
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
