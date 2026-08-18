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
