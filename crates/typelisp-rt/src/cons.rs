//! Cons cells and the `Sexpr` leaf accessors (`sexpr-int`, `sexpr-str`, ...).

use typelisp_mem::Word;
use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal, raise};
use typelisp_mem::{Heap, Value};

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
/// The words must be valid;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_cons(car: Word, cdr: Word) -> Word {
    let car = decode(car);
    let cdr = decode(cdr);
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
/// The words must be valid; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_car(arg: Word) -> Word {
    cons_half(arg, "sexpr-car", Heap::car)
}

/// `(cdr c)` for compiled code — see [`rt_car`]'s doc comment.
///
/// # Safety
///
/// Same as [`rt_car`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_cdr(arg: Word) -> Word {
    cons_half(arg, "sexpr-cdr", Heap::cdr)
}

/// The body of [`rt_car`] and [`rt_cdr`], which differ only in the half they
/// read.
///
/// The empty list's `car` and `cdr` are the empty list, as in CL. Both sides
/// are `Option<Sexpr>` now, so this is `None` in and `None` out — and it has
/// to agree with the interpreter's `sexpr-car`/`sexpr-cdr`, or the same walk
/// would end differently depending on whether its caller was compiled.
unsafe fn cons_half(arg: Word, who: &str,
    half: fn(&Heap, Value) -> Result<Value, typelisp_mem::Error>,
) -> Word {
    let arg = decode(arg);
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
/// The words must be valid;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_set_car(c: Word, val: Word) -> Word {
    set_cons_half(c, val, "rt_set_car", Heap::set_car)
}

/// `(rplacd c val)` for compiled code — see [`rt_set_car`]'s doc comment.
///
/// # Safety
///
/// Same as [`rt_set_car`].
#[no_mangle]
pub unsafe extern "C" fn rt_set_cdr(c: Word, val: Word) -> Word {
    set_cons_half(c, val, "rt_set_cdr", Heap::set_cdr)
}

/// The body of [`rt_set_car`] and [`rt_set_cdr`], which differ only in the
/// half they write.
unsafe fn set_cons_half(c: Word, val: Word, who: &str,
    set: fn(&mut Heap, Value, Value) -> Result<(), typelisp_mem::Error>,
) -> Word {
    let c = decode(c);
    let val = decode(val);
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
/// Each word must be a value of the type the signature gives it.
#[no_mangle]
pub unsafe extern "C" fn rt_consp(x: Word) -> bool {
    matches!(decode(x), Value::Cons(_))
}

/// `(sexpr-null x)`: `1` if the tagged value is the empty list `()`.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C" fn rt_null(x: Word) -> bool {
    matches!(decode(x), Value::Empty)
}

/// `(sexpr-atom x)`: `1` if the tagged value is *not* a cons cell — the
/// exact negation of [`rt_consp`], mirroring the interpreter's `!v.is_cons()`.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C" fn rt_atom(x: Word) -> bool {
    !matches!(decode(x), Value::Cons(_))
}

/// `(sexpr-symp x)`: `1` if the tagged value is an interned symbol.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C" fn rt_symp(x: Word) -> bool {
    matches!(decode(x), Value::Symbol(_))
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
pub unsafe extern "C-unwind" fn rt_sexpr_int(w: Word) -> Word {
    let w = w;
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

/// `(sexpr-bool x)`: the `Bool` node's payload as compiled `0`/`1`.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_bool(x: Word) -> bool {
    match decode(x) {
        Value::Bool(b) => b,
        _ => raise("sexpr-bool: expected a Bool Sexpr node".to_string()).into_zero(),
    }
}

/// `(sexpr-char x)`: the `Char` node's Unicode scalar value as a raw `i64`
/// — the compiled `char` convention (`compile-char`'s own widened payload).
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_sexpr_char(x: Word) -> u32 {
    match decode(x) {
        Value::Char(c) => c.into(),
        _ => raise("sexpr-char: expected a Char Sexpr node".to_string()).into_zero(),
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
pub unsafe extern "C-unwind" fn rt_sexpr_str(x: Word) -> Word {
    match decode(x) {
        Value::Str(_) => x,
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
pub unsafe extern "C-unwind" fn rt_sym_name(x: Word) -> Word {
    match decode(x) {
        Value::Symbol(id) => {
            let heap = active_heap();
            let name = id.name().to_string();
            encode(heap.alloc_string(name))
        }
        _ => raise("sexpr-sym-name: expected a Sym Sexpr node".to_string()).into_word(),
    }
}
