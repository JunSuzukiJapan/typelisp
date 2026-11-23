//! The `stream-*` builtins as *values*: one implementation, called from both
//! sides of the compile boundary. Opening a file and the `file-*` builtins
//! are `typelisp-rt-sys`'s, over the same table.
//!
//! [`crate::stream`] owns the OS resources and speaks Rust types
//! (`StreamResult<Option<char>>`, ...). This module is the layer above it,
//! turning those into the heap values the language sees — `Result<_,
//! FileError>` mostly, with `Ok(None)` for end of input — and it is what both
//! `Interp::eval_stream_builtin` (interpreted) and the `rt_stream_*` shims in
//! [`crate::lib`](crate) (compiled) call.
//!
//! Writing it once is the whole point. These builtins are not
//! arithmetic: each has a failure mode, a wrapper shape, and an error message
//! text, and a program that opens a file in compiled code and reads it in
//! interpreted code must see one behaviour. A second implementation on this
//! side of the boundary would be twenty chances to disagree.
//!
//! # Argument and result *representations*
//!
//! [`stream_builtin`] speaks [`Value`] on both ends, the way the interpreter
//! does. The compiled ABI does not — an `i64` handle and a `bool` are bare
//! machine words there, only heap values are tagged — so each shim converts
//! at its own edge, per its own signature (`registry::
//! register_stream_builtins`). That conversion is deliberately *not* here:
//! it belongs to the calling convention, not to the operation.

use typelisp_mem::prelude::*;
use typelisp_mem::{spec, wk, Heap, StdModule, SymRef, TypeId, TypeSpec, Value};

use crate::result_value::{err_in, ok_in, option_in, TypeTable};
use crate::stream::with_streams;

pub const FILE_ERROR: TypeSpec = TypeSpec::Named(StdModule::FS, "FileError", &[]);

/// The runtime type each builtin's *result* carries ([`crate::result_value`]).
/// A builtin whose result is not a box (`stream-stdin`'s handle,
/// `stream-open-p`'s `bool`) has no row and needs none.
pub const RESULT_TYPES: TypeTable = &[
    (wk::F_STREAM_CLOSE, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, FILE_ERROR])),
    (wk::F_STREAM_INPUT_P, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::BOOL, FILE_ERROR])),
    (wk::F_STREAM_OUTPUT_P, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::BOOL, FILE_ERROR])),
    (wk::F_STREAM_LISTEN, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::BOOL, FILE_ERROR])),
    (wk::F_STREAM_POSITION, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::INT, FILE_ERROR])),
    (wk::F_STREAM_AT_LINE_START, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::BOOL, FILE_ERROR])),
    (wk::F_STREAM_READ_CHAR, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::CHAR]), FILE_ERROR])),
    (wk::F_STREAM_UNREAD_CHAR, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, FILE_ERROR])),
    (wk::F_STREAM_READ_BYTE, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT]), FILE_ERROR])),
    (wk::F_STREAM_WRITE_BYTE, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, FILE_ERROR])),
    (wk::F_STREAM_WRITE_STRING, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, FILE_ERROR])),
    (wk::F_STREAM_FINISH_OUTPUT, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, FILE_ERROR])),
    (wk::F_STREAM_TAKE_OUTPUT_STRING, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::STRING, FILE_ERROR])),
];

/// The type of the value *inside* the `Result` — [`RESULT_TYPES`]' companion for
/// the builtins whose payload is itself a box.
pub const INNER_TYPES: TypeTable = &[
    (wk::F_STREAM_READ_CHAR, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::CHAR])),
    (wk::F_STREAM_READ_BYTE, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT])),
];

/// `Ok(v)` under `name`'s row.
fn result_ok(heap: &mut Heap, name: SymRef, v: Value) -> Value {
    ok_in(heap, RESULT_TYPES, name.well_known(), v)
}

/// `Err(FileError(msg))` — the concrete error type every stream operation
/// fails with.
fn result_err(heap: &mut Heap, name: SymRef, msg: String) -> Value {
    err_in(heap, RESULT_TYPES, name.well_known(), TypeId::FILE_ERROR, msg)
}

fn option_value(heap: &mut Heap, name: SymRef, v: Option<Value>) -> Value {
    option_in(heap, INNER_TYPES, name.well_known(), v)
}

/// An argument that isn't the shape its signature promises. Unreachable
/// through the type checker; each caller reports it in its own idiom (a
/// catchable internal error interpreted, [`crate::fatal`] compiled).
pub type ArgError = String;

/// A stream handle, or `open`'s mode — both plain integers.
pub fn int(args: &[Value], i: usize, who: &str) -> Result<i64, ArgError> {
    match args.get(i) {
        Some(Value::Int(n)) => Ok(*n),
        other => Err(format!("{}: argument {} is not an integer, got {:?}", who, i, other)),
    }
}

pub fn text(heap: &Heap, args: &[Value], i: usize, who: &str) -> Result<String, ArgError> {
    match args.get(i) {
        Some(Value::Str(id)) => Ok(heap.string(*id).to_string()),
        other => Err(format!("{}: argument {} is not a string, got {:?}", who, i, other)),
    }
}

pub fn character(args: &[Value], i: usize, who: &str) -> Result<char, ArgError> {
    match args.get(i) {
        Some(Value::Char(c)) => Ok(*c),
        other => Err(format!("{}: argument {} is not a character, got {:?}", who, i, other)),
    }
}

/// Every `stream-*` builtin, or `None` if `name` isn't one.
///
/// The `Ok`/`Err` of the *return* is about the arguments, not about the
/// operation: a missing file or a closed stream is an ordinary
/// `Err(FileError(...))` **value** (`Ok(Value)` here), because programs handle
/// those. Only a wrongly-shaped argument — which the checker rules out —
/// reaches the `Err` arm.
pub fn stream_builtin(heap: &mut Heap, sym: SymRef, args: &[Value]) -> Option<Result<Value, ArgError>> {
    let name = sym.name();
    /// Runs a `StreamResult`-returning table operation and wraps it: `Ok`
    /// through `$ok` into `Result`'s ok variant, `Err(msg)` into a
    /// `FileError`.
    macro_rules! wrap {
        ($e:expr, $ok:expr) => {
            match $e {
                Ok(v) => Ok(result_ok(heap, sym, $ok(v))),
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        };
    }
    /// Decodes an argument, turning a shape violation into this function's
    /// own early return.
    macro_rules! arg {
        ($e:expr) => {
            match $e {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            }
        };
    }

    Some(match sym.well_known() {
        wk::F_STREAM_STDIN => Ok(Value::Int(with_streams(heap, |t| t.stdin()))),
        wk::F_STREAM_STDOUT => Ok(Value::Int(with_streams(heap, |t| t.stdout()))),
        wk::F_STREAM_STDERR => Ok(Value::Int(with_streams(heap, |t| t.stderr()))),
        wk::F_STREAM_STRING_INPUT => {
            let s = arg!(text(heap, args, 0, name));
            Ok(Value::Int(with_streams(heap, |t| t.string_input(&s))))
        }
        wk::F_STREAM_STRING_OUTPUT => Ok(Value::Int(with_streams(heap, |t| t.string_output()))),
        wk::F_STREAM_CLOSE => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.close(h)), |_v: ()| Value::Empty)
        }
        wk::F_STREAM_OPEN_P => {
            let h = arg!(int(args, 0, name));
            Ok(Value::Bool(with_streams(heap, |t| t.is_open(h))))
        }
        // A handle the prelude holds is always one the table issued, so a
        // failure here is a broken invariant, reported on this function's
        // error channel like a malformed argument.
        wk::F_STREAM_DESCRIBE => {
            let h = arg!(int(args, 0, name));
            let text = arg!(with_streams(heap, |t| t.describe(h)).map_err(|e| format!("{}: {}", name, e)));
            Ok(heap.alloc_string(text))
        }
        wk::F_STREAM_INPUT_P => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.is_input(h)), |v: bool| Value::Bool(v))
        }
        wk::F_STREAM_OUTPUT_P => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.is_output(h)), |v: bool| Value::Bool(v))
        }
        wk::F_STREAM_READ_CHAR => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.read_char(h)) {
                Ok(c) => {
                    let inner = option_value(heap, sym, c.map(Value::Char));
                    Ok(result_ok(heap, sym, inner))
                }
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        }
        wk::F_STREAM_READ_BYTE => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.read_byte(h)) {
                Ok(b) => {
                    let inner = option_value(heap, sym, b.map(|b| Value::Int(b as i64)));
                    Ok(result_ok(heap, sym, inner))
                }
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        }
        wk::F_STREAM_WRITE_BYTE => {
            let (h, b) = (arg!(int(args, 0, name)), arg!(int(args, 1, name)));
            match u8::try_from(b) {
                Ok(b) => wrap!(with_streams(heap, |t| t.write_byte(h, b)), |_v: ()| Value::Empty),
                Err(_) => Ok(result_err(heap, sym, format!("write-byte: {} is not a byte (0..255)", b))),
            }
        }
        wk::F_STREAM_UNREAD_CHAR => {
            let (h, c) = (arg!(int(args, 0, name)), arg!(character(args, 1, name)));
            wrap!(with_streams(heap, |t| t.unread_char(h, c)), |_v: ()| Value::Empty)
        }
        wk::F_STREAM_LISTEN => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.listen(h)), |v: bool| Value::Bool(v))
        }
        wk::F_STREAM_POSITION => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.position(h)), |v: i64| Value::Int(v))
        }
        wk::F_STREAM_WRITE_STRING => {
            let (h, s) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            wrap!(with_streams(heap, |t| t.write_str(h, &s)), |_v: ()| Value::Empty)
        }
        wk::F_STREAM_AT_LINE_START => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.at_line_start(h)), |v: bool| Value::Bool(v))
        }
        wk::F_STREAM_FINISH_OUTPUT => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.finish_output(h)), |_v: ()| Value::Empty)
        }
        wk::F_STREAM_TAKE_OUTPUT_STRING => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.take_output_string(h)) {
                Ok(s) => {
                    let sv = heap.alloc_string(s);
                    Ok(result_ok(heap, sym, sv))
                }
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        }
        _ => return None,
    })
}
