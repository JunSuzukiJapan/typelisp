//! `read` as compiled code calls it.
//!
//! Here rather than in `typelisp-rt` for the reason [`crate`]'s doc comment
//! gives: `typelisp-rt` cannot depend on this crate without every program
//! linking the reader. The same arrangement `typelisp_print::shim` has.

use typelisp_abi::{active_heap, encode, fatal, tagged_arg};
use typelisp_mem::{Heap, Value};

use crate::reader::Reader;

/// The type keys the result is built with — `type_key::type_key_of` of
/// `result`/`readerror`, which for a root path is just the name.
///
/// Spelled out because this crate has no `Path` to derive them from, the same
/// standing exception `typelisp_rt::stream_builtin` has. Their end of that
/// agreement is held up by `tests/type_identity_guard_test.rs`, which asserts
/// each equals what `type_key_of` produces — checked, not assumed, because
/// the failure it guards is silent: a value built under a key nobody else
/// spells would be unmatchable and print as `<unknown-variant>`.
pub const RESULT_TYPE_KEY: &str = "result";
pub const READ_ERROR_TYPE_KEY: &str = "readerror";
/// `cons-cell<A,B>`'s box is a `Struct` under this name — the prelude
/// `defstruct` `read-datum-at` pairs its datum with its end index in, since
/// this language has no multiple values for CL's second return value to be.
pub const CONS_CELL_TYPE_KEY: &str = "cons-cell";

/// `(read s) => Result<Sexpr, ReadError>` — the whole of the builtin, called
/// from both sides of the compile boundary (`Interp::eval_builtin`'s `read`
/// arm is the other caller).
pub fn read_builtin(heap: &mut Heap, source: &str) -> Value {
    let reader = Reader::new();
    match reader.read(heap, source) {
        Ok(v) => heap.alloc_enum(RESULT_TYPE_KEY.to_string(), 0, vec![v]),
        Err(e) => {
            let msg = heap.alloc_string(format!("read: {}", e));
            let err = heap.alloc_enum(READ_ERROR_TYPE_KEY.to_string(), 0, vec![msg]);
            heap.alloc_enum(RESULT_TYPE_KEY.to_string(), 1, vec![err])
        }
    }
}

/// `(read-datum-at s start preserve) => Result<cons-cell<Sexpr, i64>, ReadError>`
/// — one datum from `s` beginning at character index `start`, paired with the
/// index reading stopped at.
///
/// The pair is CL's two return values from `read-from-string`; the prelude's
/// `read-from-string`/`read-from-string-preserving-whitespace` are the two
/// `preserve` settings with the default `start`.
pub fn read_datum_at_builtin(heap: &mut Heap, source: &str, start: i64, preserve: bool) -> Value {
    let reader = Reader::new();
    let start = if start < 0 { 0usize } else { start as usize };
    match reader.read_from(heap, source, start, preserve) {
        Ok((v, end)) => {
            heap.push_root(v);
            let pair = heap.alloc_struct(CONS_CELL_TYPE_KEY.to_string(), vec![v, Value::Int(end as i64)]);
            heap.pop_root();
            heap.alloc_enum(RESULT_TYPE_KEY.to_string(), 0, vec![pair])
        }
        Err(e) => {
            let msg = heap.alloc_string(format!("read: {}", e));
            let err = heap.alloc_enum(READ_ERROR_TYPE_KEY.to_string(), 0, vec![msg]);
            heap.alloc_enum(RESULT_TYPE_KEY.to_string(), 1, vec![err])
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
