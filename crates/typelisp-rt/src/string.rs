//! Strings and characters.

use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal, raise};
use typelisp_mem::Value;

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

/// `str::length` for compiled code — the character count (not byte length)
/// of `id`, matching the interpreter's own `string_length`. Returns a
/// bare `i64` (a `Type::I64` result is never itself Sexpr-tagged).
///
/// # Safety
///
/// The words must be valid
/// encoding a `Value::Str`; a `Heap` must already be registered on this
/// thread.
#[no_mangle]
pub unsafe extern "C" fn rt_str_length(id: i64) -> i64 {
    let id = match decode(id) {
        Value::Str(id) => id,
        _ => fatal("rt_str_length: argument is not a Str"),
    };
    active_heap().string(id).chars().count() as i64
}

/// `str::ref` for compiled code — the `i`-th Unicode scalar value of
/// `id`'s content (`idx`, a bare `i64` index), matching the
/// interpreter's own `string_ref`. Returns a bare (untagged) scalar, the
/// same `Type::Char` representation `compile-sexpr-field`'s own `char`
/// extraction produces. An out-of-range index (negative included) [`raise`]s
/// the message the interpreter's `string_ref` raises for it, character for
/// character — the type system cannot express the bound, so both paths check
/// at run time and must agree on what they say when the check fails.
///
/// # Safety
///
/// The words must be valid,
/// the first encoding a `Value::Str`; a `Heap` must already be registered on
/// this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_str_ref(id: i64, idx: i64) -> i64 {
    let id = match decode(id) {
        Value::Str(id) => id,
        _ => fatal("rt_str_ref: first argument is not a Str"),
    };
    let idx = idx;
    let found = if idx >= 0 { active_heap().string(id).chars().nth(idx as usize) } else { None };
    match found {
        Some(c) => c as i64,
        // The length is only walked on the failing path, so an in-range
        // `ref` still costs one `chars().nth`.
        None => {
            let len = active_heap().string(id).chars().count();
            raise(format!("ref: index {} out of range (length {})", idx, len)).into_word()
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
/// The words must be valid,
/// both encoding a `Value::Str`; a `Heap` must already be registered on this
/// thread.
#[no_mangle]
pub unsafe extern "C" fn rt_str_eq(a: i64, b: i64) -> i64 {
    let a = match decode(a) {
        Value::Str(id) => id,
        _ => fatal("rt_str_eq: first argument is not a Str"),
    };
    let b = match decode(b) {
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
pub unsafe extern "C" fn rt_str_lt(a: i64, b: i64) -> i64 {
    let a = match decode(a) {
        Value::Str(id) => id,
        _ => fatal("rt_str_lt: first argument is not a Str"),
    };
    let b = match decode(b) {
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
pub unsafe extern "C" fn rt_str_equalp(a: i64, b: i64) -> i64 {
    let a = match decode(a) {
        Value::Str(id) => id,
        _ => fatal("rt_str_equalp: first argument is not a Str"),
    };
    let b = match decode(b) {
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
/// has no single-instruction form. `a`/`b` are the *raw* (untagged)
/// `i64` Unicode scalar values a compiled `char` is represented as — not
/// tagged `Sexpr`s — matching `compile-assoc`'s char branch, which compiles
/// its operands to bare code points. Returns a bare `0`/`1`.
///
/// # Safety
///
/// Each word must be a value of the type the signature gives it.
/// No `Heap` is needed (both operands are immediates).
#[no_mangle]
pub unsafe extern "C" fn rt_char_equalp(a: i64, b: i64) -> i64 {
    let a = a;
    let b = b;
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
/// Each word must be a value of the type the signature gives it.
#[no_mangle]
pub unsafe extern "C" fn rt_char_upcase(a: i64) -> i64 {
    char_map(a, |c| c.to_ascii_uppercase() as i64)
}

/// See [`rt_char_upcase`].
///
/// # Safety
///
/// Each word must be a value of the type the signature gives it.
#[no_mangle]
pub unsafe extern "C" fn rt_char_downcase(a: i64) -> i64 {
    char_map(a, |c| c.to_ascii_lowercase() as i64)
}

/// `char::alphap` — CL's `alpha-char-p`, ASCII-only like the interpreter's.
/// Returns a bare `0`/`1`, which is what a compiled `bool` is.
///
/// # Safety
///
/// Each word must be a value of the type the signature gives it.
#[no_mangle]
pub unsafe extern "C" fn rt_char_alphap(a: i64) -> i64 {
    char_map(a, |c| i64::from(c.is_ascii_alphabetic()))
}

/// `char::digitp` — the `bool` predicate, not CL's weight-returning
/// `digit-char-p` (which is `digit-weight`, a prelude function). See
/// [`rt_char_alphap`].
///
/// # Safety
///
/// Each word must be a value of the type the signature gives it.
#[no_mangle]
pub unsafe extern "C" fn rt_char_digitp(a: i64) -> i64 {
    char_map(a, |c| i64::from(c.is_ascii_digit()))
}

/// The one-code-point-in, one-word-out shape the four `char` shims above
/// share. A compiled `char` always holds a valid scalar value; a word that is
/// not one would be an internal invariant break, and is passed through
/// unchanged rather than invented into something else — the same reading
/// [`rt_char_equalp`] takes of the same impossibility.
///
/// # Safety
///
/// None: `a` is a character's scalar value.
unsafe fn char_map(a: i64, f: impl FnOnce(char) -> i64) -> i64 {
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
/// Each word must be a value of the type the signature gives it.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_int_to_char(n: i64) -> i64 {
    let n = n;
    let valid = n >= 0 && n <= i64::from(u32::MAX) && char::from_u32(n as u32).is_some();
    if !valid {
        return raise(format!("int->char: {} is not a valid Unicode scalar value", n)).into_word();
    }
    n
}

/// `str::append` for compiled code — concatenates the content of
/// `a`/`b` into a freshly allocated string, matching the
/// interpreter's own `string_append`. Returns the tagged form, exactly like
/// [`rt_str_new`] (and just as unrooted until a caller protects it).
///
/// # Safety
///
/// Same as [`rt_str_eq`].
#[no_mangle]
pub unsafe extern "C" fn rt_str_append(a: i64, b: i64) -> i64 {
    let a = match decode(a) {
        Value::Str(id) => id,
        _ => fatal("rt_str_append: first argument is not a Str"),
    };
    let b = match decode(b) {
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
/// Same as [`rt_str_eq`], for `id` alone.
unsafe fn str_map_case(id: i64, who: &str, up: bool) -> i64 {
    let id = match decode(id) {
        Value::Str(id) => id,
        _ => fatal(&format!("{who}: argument is not a Str")),
    };
    let heap = active_heap();
    let s = if up { heap.string(id).to_ascii_uppercase() } else { heap.string(id).to_ascii_lowercase() };
    encode(heap.alloc_string(s))
}

/// `string::upcase` for compiled code — `id`'s content with every ASCII
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
pub unsafe extern "C" fn rt_str_upcase(id: i64) -> i64 {
    str_map_case(id, "rt_str_upcase", true)
}

/// `string::downcase` for compiled code — the folding-down counterpart of
/// [`rt_str_upcase`].
///
/// # Safety
///
/// Same as [`str_map_case`].
#[no_mangle]
pub unsafe extern "C" fn rt_str_downcase(id: i64) -> i64 {
    str_map_case(id, "rt_str_downcase", false)
}

/// `str::substring` for compiled code — the `[start, end)` character range of
/// `id`'s content as a freshly allocated string, matching the
/// interpreter's own `string_substring`. `start`/`end` are bare
/// (untagged) `i64` indices, the same convention [`rt_str_ref`] uses, and
/// they count *characters*, not bytes.
///
/// An invalid range [`raise`]s the message `string_substring` raises for it,
/// the same parity [`rt_str_ref`] keeps for its own bounds check.
///
/// # Safety
///
/// The words must be valid,
/// the first encoding a `Value::Str`; a `Heap` must already be registered on
/// this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_str_substring(id: i64, start: i64, end: i64) -> i64 {
    let id = match decode(id) {
        Value::Str(id) => id,
        _ => fatal("rt_str_substring: first argument is not a Str"),
    };
    let start = start;
    let end = end;
    let heap = active_heap();
    let chars: Vec<char> = heap.string(id).chars().collect();
    let len = chars.len() as i64;
    if start < 0 || end > len || start > end {
        return raise(format!("substring: invalid range {}..{} (length {})", start, end, len)).into_word();
    }
    let s: String = chars[start as usize..end as usize].iter().collect();
    encode(heap.alloc_string(s))
}
