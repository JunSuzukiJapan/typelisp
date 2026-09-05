//! The readtable: which function the reader calls when it meets a character
//! that has been given one.
//!
//! The table itself lives on the [`Heap`] (see `Heap::set_macro_character`
//! for why there and not in a thread-local); this module is only the four
//! builtins that read and write it, shared — like
//! [`crate::stream_builtin`] — by the interpreter and by the `rt_*` shims
//! compiled code calls, so a registration cannot mean two different things
//! depending on which side of the compile boundary made it.

use crate::stream_builtin::ArgError;
use typelisp_mem::{Heap, Value};

/// The type key of `Option<(fn (string-input-stream char) Option<Sexpr>)>` — what
/// `get-macro-character` answers with.
///
/// Spelled out because this crate sits below the checker and cannot compute
/// it from a `Type`; `tests/type_identity_guard_test.rs` checks the spelling
/// against what the registry's own declaration produces, for the reason that
/// file gives: a box built under a key nobody else spells is unmatchable.
pub const READER_MACRO_OPTION_KEY: &str = "option<(fn (string-input-stream,char) option<sexpr>)>";

fn character(args: &[Value], i: usize, who: &str) -> Result<char, ArgError> {
    match args.get(i) {
        Some(Value::Char(c)) => Ok(*c),
        other => Err(format!("{}: argument {} is not a character, got {:?}", who, i, other)),
    }
}

/// `Some(f)`/`None` under [`READER_MACRO_OPTION_KEY`], matching `option`'s
/// variant order (`some` = 0, `none` = 1).
fn found(heap: &mut Heap, f: Option<Value>) -> Value {
    let key = heap.intern_type_key(READER_MACRO_OPTION_KEY);
    match f {
        Some(v) => {
            heap.push_root(v);
            let out = heap.alloc_enum(key, 0, vec![v]);
            heap.pop_root();
            out
        }
        None => heap.alloc_enum(key, 1, vec![]),
    }
}

/// Every readtable builtin, or `None` if `name` isn't one.
pub fn readtable_builtin(heap: &mut Heap, name: &str, args: &[Value]) -> Option<Result<Value, ArgError>> {
    macro_rules! ch {
        ($i:expr) => {
            match character(args, $i, name) {
                Ok(c) => c,
                Err(e) => return Some(Err(e)),
            }
        };
    }
    Some(match name {
        "set-macro-character" => {
            let c = ch!(0);
            heap.set_macro_character(c, args[1]);
            Ok(Value::Empty)
        }
        "get-macro-character" => {
            let c = ch!(0);
            let f = heap.macro_character(c);
            Ok(found(heap, f))
        }
        "set-dispatch-macro-character" => {
            let (d, s) = (ch!(0), ch!(1));
            heap.set_dispatch_macro_character(d, s, args[2]);
            Ok(Value::Empty)
        }
        "get-dispatch-macro-character" => {
            let (d, s) = (ch!(0), ch!(1));
            let f = heap.dispatch_macro_character(d, s);
            Ok(found(heap, f))
        }
        _ => return None,
    })
}
