//! Symbols and symbol paths.

use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal, raise};
use typelisp_mem::Value;

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
    encode(Value::Symbol(typelisp_mem::symbols::intern_at_home_path(&home, &name)))
}

/// `(intern NAME NAMESPACE)`: the symbol `NAME` reads as in the module the
/// string `NAMESPACE` names (`symbols::ns_named`) — `"keyword"` for a
/// keyword — the interpreter's `intern` exactly. The checker supplies
/// `NAMESPACE` when a call leaves it out, so here it is always present; a
/// module that does not exist is the language's error, not the runtime's.
///
/// # Safety
///
/// Same as [`rt_intern_symbol`].
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_intern(name: i64, module: i64) -> i64 {
    let str_arg = |w: i64| -> String {
        match decode(w) {
            Value::Str(id) => active_heap().string(id).to_string(),
            _ => fatal("rt_intern: argument is not a Str"),
        }
    };
    let (name, module) = (str_arg(name), str_arg(module));
    let Some(ns) = typelisp_mem::symbols::ns_named(&module) else {
        return raise(format!("intern: no module named `{}`", module)).into_word();
    };
    encode(Value::Symbol(active_heap().names().read(ns, &name)))
}

/// `(keywordp s)`: `1` if the symbol is a keyword — owned by the keyword
/// module, which is the whole of what being a keyword is.
///
/// # Safety
///
/// Same as [`rt_consp`].
#[no_mangle]
pub unsafe extern "C" fn rt_keywordp(x: i64) -> i64 {
    match decode(x) {
        Value::Symbol(id) => i64::from(id.is_keyword()),
        _ => fatal("rt_keywordp: argument is not a Symbol"),
    }
}

/// The system module's symbol at index `x` (a raw `i64`, not a tagged
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
/// The word must hold an
/// index within `BUILTIN_SYMBOLS`.
#[no_mangle]
pub unsafe extern "C" fn rt_wk_symbol(x: i64) -> i64 {
    encode(Value::Symbol(typelisp_mem::symbols::well_known_symbol(x as u32)))
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
/// The words must be valid
/// encoding a `Value::Path`; a `Heap` must already be registered on this
/// thread.
#[no_mangle]
pub unsafe extern "C" fn rt_path_to_list(id: i64) -> i64 {
    let id = match decode(id) {
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
/// The words must be valid
/// encoding a proper `Sexpr` list of `Value::Symbol`s; a `Heap` must already
/// be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_list_to_path(cur: i64) -> i64 {
    let heap = active_heap();
    let mut segs = Vec::new();
    let mut cur = decode(cur);
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
