//! `read` as compiled code calls it.
//!
//! Here rather than in `typelisp-rt` for the reason [`crate`]'s doc comment
//! gives: `typelisp-rt` cannot depend on this crate without every program
//! linking the reader. The same arrangement `typelisp_print::shim` has.

use typelisp_abi::{active_heap, encode, fatal, tagged_arg};
use typelisp_mem::{Heap, TypeKeyId, Value};

use crate::reader::Reader;

// The types the results here are built with — `TypeKeyId::RESULT`,
// `READ_ERROR`, and `CONS_CELL` — come from `typelisp_mem`'s pre-interned
// table rather than being named here, because this crate has no `Path` to
// derive a name from (the same standing exception `typelisp_rt` has). That
// their names are the ones `type_key::type_key_of` produces is asserted by
// `tests/type_identity_guard_test.rs` — checked, not assumed, because the
// failure it guards is silent: a value built under a key nobody else spells
// would be unmatchable and print as `<unknown-variant>`.
//
// `cons-cell<A,B>`'s box is a `Struct`: the prelude `defstruct`
// `read-datum-at` pairs its datum with its end index in, since this language
// has no multiple values for CL's second return value to be.

/// The runtime type keys these two builtins' results carry.
///
/// A type's identity includes its instantiation
/// (`typelisp::type_key::type_key_of_type`), and this crate has no `Path` to
/// derive one from — so they are spelled here and compared against the
/// registry's own return types by `tests/type_identity_guard_test.rs`, the
/// same arrangement `typelisp_rt::stream_builtin` uses.
///
/// `Option<Sexpr>` inside them is niche-represented — `some v` *is* `v` — so
/// it builds no box and needs no key of its own.
pub const READ_RESULT_KEY: &str = "result<option<sexpr>,readerror>";
pub const READ_DATUM_RESULT_KEY: &str = "result<cons-cell<option<sexpr>,int>,readerror>";
pub const READ_DATUM_PAIR_KEY: &str = "cons-cell<option<sexpr>,int>";

/// The evaluator these two builtins read *with*, so `#.` and macro characters
/// work the same way in a program's own `read` as they do in the loader's —
/// CL's `read` consults the readtable, and a program that installed a macro
/// character means it for its own reads too.
///
/// `None` when no interpreter has registered on this thread: a unit test
/// reading a datum with no program standing behind it, where a `#.` or a
/// macro character could not have been installed in the first place.
fn evaluator() -> Option<&'static dyn crate::reader::ReadEval> {
    crate::runtime::read_hooks().map(|_| &crate::runtime::HookEval as &dyn crate::reader::ReadEval)
}

/// `(read s) => Result<Sexpr, ReadError>` — the whole of the builtin, called
/// from both sides of the compile boundary (`Interp::eval_builtin`'s `read`
/// arm is the other caller).
pub fn read_builtin(heap: &mut Heap, source: &str) -> Value {
    read_builtin_with(heap, source, evaluator())
}

/// [`read_builtin`] against a caller-supplied evaluator.
///
/// The interpreter passes itself (it *is* a `ReadEval`), which is why an
/// interpreted `(read ...)` honours the readtable without anything having
/// been registered on the thread. Only the compiled shim, which has no
/// `Interp` to pass, falls back to [`evaluator`].
pub fn read_builtin_with(heap: &mut Heap, source: &str, eval: Option<&dyn crate::reader::ReadEval>) -> Value {
    let reader = Reader::new();
    let key = heap.intern_type_key(READ_RESULT_KEY);
    match reader.read_in_with(heap, "<input>", source, eval) {
        Ok(v) => heap.alloc_enum(key, 0, vec![v]),
        Err(e) => {
            let msg = heap.alloc_string(format!("read: {}", e));
            let err = heap.alloc_enum(TypeKeyId::READ_ERROR, 0, vec![msg]);
            heap.alloc_enum(key, 1, vec![err])
        }
    }
}

/// `(read-datum-at s start preserve) => Result<cons-cell<Sexpr, i32>, ReadError>`
/// — one datum from `s` beginning at character index `start`, paired with the
/// index reading stopped at.
///
/// The pair is CL's two return values from `read-from-string`; the prelude's
/// `read-from-string`/`read-from-string-preserving-whitespace` are the two
/// `preserve` settings with the default `start`.
pub fn read_datum_at_builtin(heap: &mut Heap, source: &str, start: i64, preserve: bool) -> Value {
    read_datum_at_builtin_with(heap, source, start, preserve, evaluator())
}

/// [`read_datum_at_builtin`] against a caller-supplied evaluator — see
/// [`read_builtin_with`].
pub fn read_datum_at_builtin_with(
    heap: &mut Heap,
    source: &str,
    start: i64,
    preserve: bool,
    eval: Option<&dyn crate::reader::ReadEval>,
) -> Value {
    let reader = Reader::new();
    let start = if start < 0 { 0usize } else { start as usize };
    let key = heap.intern_type_key(READ_DATUM_RESULT_KEY);
    let pair_key = heap.intern_type_key(READ_DATUM_PAIR_KEY);
    match reader.read_from_with(heap, source, start, preserve, eval) {
        Ok((v, end)) => {
            heap.push_root(v);
            let pair = heap.alloc_struct(pair_key, vec![v, Value::Int(end as i64)]);
            heap.pop_root();
            heap.alloc_enum(key, 0, vec![pair])
        }
        Err(e) => {
            let msg = heap.alloc_string(format!("read: {}", e));
            let err = heap.alloc_enum(TypeKeyId::READ_ERROR, 0, vec![msg]);
            heap.alloc_enum(key, 1, vec![err])
        }
    }
}

/// `read` for compiled code: `args[0]` is the source string, tagged; the
/// result is the tagged `Result`.
///
/// # Safety
///
/// `args` must point to at least 1 valid tagged `i64`; a `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_read(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_read: expected 1 argument");
    }
    let heap = active_heap();
    let source = match tagged_arg(args, 0) {
        Value::Str(id) => heap.string(id).to_string(),
        other => fatal(&format!("rt_read: argument is not a string, got {:?}", other)),
    };
    encode(read_builtin(heap, &source))
}

/// `read-datum-at` for compiled code: the source string is tagged, `start` and
/// `preserve` are bare machine words (an `i64` and a `bool`), per the compiled
/// calling convention.
///
/// # Safety
///
/// `args` must point to at least 3 valid `i64`s, the first a tagged `Value`;
/// a `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_read_datum_at(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_read_datum_at: expected 3 arguments");
    }
    let heap = active_heap();
    let source = match tagged_arg(args, 0) {
        Value::Str(id) => heap.string(id).to_string(),
        other => fatal(&format!("rt_read_datum_at: argument is not a string, got {:?}", other)),
    };
    let start = *args.add(1);
    let preserve = *args.add(2) != 0;
    encode(read_datum_at_builtin(heap, &source, start, preserve))
}
