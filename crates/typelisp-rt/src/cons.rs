//! Cons cells and the `Sexpr` leaf accessors (`sexpr-int`, `sexpr-str`, ...).

use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal, raise};
use typelisp_mem::{Heap, Value};
use crate::sexpr_narrow_value;

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
/// are `Option<Sexpr>` now, so this is `None` in and `None` out — and it has
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
        Err(_) => raise(format!("{}: not a cons", who)).into_word(),
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
        _ => raise("sexpr-int: expected an int Sexpr node".to_string()).into_word(),
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
        _ => raise("sexpr-bool: expected a Bool Sexpr node".to_string()).into_word(),
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
        _ => raise("sexpr-char: expected a Char Sexpr node".to_string()).into_word(),
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
        _ => raise("sexpr-str: expected a Str Sexpr node".to_string()).into_word(),
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
            let name = id.name().to_string();
            encode(heap.alloc_string(name))
        }
        _ => raise("sexpr-sym-name: expected a Sym Sexpr node".to_string()).into_word(),
    }
}
