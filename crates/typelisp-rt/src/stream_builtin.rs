//! The `stream-*`/`file-*` builtins as *values*: one implementation, called
//! from both sides of the compile boundary.
//!
//! [`crate::stream`] owns the OS resources and speaks Rust types
//! (`StreamResult<Option<char>>`, ...). This module is the layer above it,
//! turning those into the heap values the language sees — `Result<_,
//! FileError>` mostly, with `Ok(none)` for end of input — and it is what both
//! `Interp::eval_stream_builtin` (interpreted) and the `rt_stream_*` shims in
//! [`crate::lib`](crate) (compiled) call.
//!
//! Writing it once is the whole point. These twenty builtins are not
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

use typelisp_mem::{Heap, TypeKeyId, Value};

use crate::stream::with_streams;

// The types the results here are built with — `TypeKeyId::OPTION`, `RESULT`,
// and `FILE_ERROR` — come from `typelisp_mem`'s pre-interned table rather
// than being named here, because this crate has no `Path` to derive a name
// from (the same standing exception `rt_data_new` has, which receives its key
// as a string from compiled code). That their names are the ones
// `type_key::type_key_of` produces is asserted by
// `tests/type_identity_guard_test.rs` — checked, not assumed, because the
// failure it guards is silent: a value built here under a key nobody else
// spells would be unmatchable and print as `<unknown-variant>`.
//
// `Vector<T>`'s box is a `Struct`, not an `Enum` — the "just a box + a name"
// representation `value.rs`'s `BoxedObj` doc comment describes, whose *fields
// are its elements*. `file-list-directory` builds one directly.

/// `Ok(v)`, matching `result_def`'s variant order (`ok` = 0, `err` = 1).
fn result_ok(heap: &mut Heap, v: Value) -> Value {
    heap.alloc_enum(TypeKeyId::RESULT, 0, vec![v])
}

/// `Err(FileError(msg))` — the concrete error type every stream and file
/// operation fails with, wrapped in `Result`'s `err` variant. `FileError` has
/// exactly one variant (index 0) carrying the message.
fn result_err(heap: &mut Heap, msg: String) -> Value {
    let msg_val = heap.alloc_string(msg);
    let err_val = heap.alloc_enum(TypeKeyId::FILE_ERROR, 0, vec![msg_val]);
    heap.alloc_enum(TypeKeyId::RESULT, 1, vec![err_val])
}

/// `Some(v)`/`None`, matching `option_def`'s variant order (`some` = 0,
/// `none` = 1).
fn option_value(heap: &mut Heap, v: Option<Value>) -> Value {
    let (variant, fields) = match v {
        Some(x) => (0, vec![x]),
        None => (1, vec![]),
    };
    heap.alloc_enum(TypeKeyId::OPTION, variant, fields)
}

/// An argument that isn't the shape its signature promises. Unreachable
/// through the type checker; each caller reports it in its own idiom (a
/// catchable internal error interpreted, [`crate::fatal`] compiled).
pub type ArgError = String;

/// A stream handle, or `open`'s mode — both plain integers.
fn int(args: &[Value], i: usize, who: &str) -> Result<i64, ArgError> {
    match args.get(i) {
        Some(Value::Int(n)) => Ok(*n),
        other => Err(format!("{}: argument {} is not an integer, got {:?}", who, i, other)),
    }
}

fn text(heap: &Heap, args: &[Value], i: usize, who: &str) -> Result<String, ArgError> {
    match args.get(i) {
        Some(Value::Str(id)) => Ok(heap.string(*id).to_string()),
        other => Err(format!("{}: argument {} is not a string, got {:?}", who, i, other)),
    }
}

fn character(args: &[Value], i: usize, who: &str) -> Result<char, ArgError> {
    match args.get(i) {
        Some(Value::Char(c)) => Ok(*c),
        other => Err(format!("{}: argument {} is not a character, got {:?}", who, i, other)),
    }
}

/// Every `stream-*`/`file-*` builtin, or `None` if `name` isn't one.
///
/// The `Ok`/`Err` of the *return* is about the arguments, not about the
/// operation: a missing file or a closed stream is an ordinary
/// `Err(FileError(...))` **value** (`Ok(Value)` here), because programs handle
/// those. Only a wrongly-shaped argument — which the checker rules out —
/// reaches the `Err` arm.
pub fn stream_builtin(heap: &mut Heap, name: &str, args: &[Value]) -> Option<Result<Value, ArgError>> {
    /// Runs a `StreamResult`-returning table operation and wraps it: `Ok`
    /// through `$ok` into `Result`'s ok variant, `Err(msg)` into a
    /// `FileError`.
    macro_rules! wrap {
        ($e:expr, $ok:expr) => {
            match $e {
                Ok(v) => Ok(result_ok(heap, $ok(v))),
                Err(m) => Ok(result_err(heap, m)),
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

    Some(match name {
        "stream-stdin" => Ok(Value::Int(with_streams(|t| t.stdin()))),
        "stream-stdout" => Ok(Value::Int(with_streams(|t| t.stdout()))),
        "stream-stderr" => Ok(Value::Int(with_streams(|t| t.stderr()))),
        "stream-string-input" => {
            let s = arg!(text(heap, args, 0, name));
            Ok(Value::Int(with_streams(|t| t.string_input(&s))))
        }
        "stream-string-output" => Ok(Value::Int(with_streams(|t| t.string_output()))),
        "stream-open-file" => {
            let (p, mode) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(|t| t.open_file(&p, mode)), |v: i64| Value::Int(v))
        }
        "stream-close" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(|t| t.close(h)), |_v: ()| Value::Empty)
        }
        "stream-open-p" => {
            let h = arg!(int(args, 0, name));
            Ok(Value::Bool(with_streams(|t| t.is_open(h))))
        }
        "stream-input-p" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(|t| t.is_input(h)), |v: bool| Value::Bool(v))
        }
        "stream-output-p" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(|t| t.is_output(h)), |v: bool| Value::Bool(v))
        }
        "stream-read-char" => {
            let h = arg!(int(args, 0, name));
            match with_streams(|t| t.read_char(h)) {
                Ok(c) => {
                    let inner = option_value(heap, c.map(Value::Char));
                    Ok(result_ok(heap, inner))
                }
                Err(m) => Ok(result_err(heap, m)),
            }
        }
        "stream-read-byte" => {
            let h = arg!(int(args, 0, name));
            match with_streams(|t| t.read_byte(h)) {
                Ok(b) => {
                    let inner = option_value(heap, b.map(|b| Value::Int(b as i64)));
                    Ok(result_ok(heap, inner))
                }
                Err(m) => Ok(result_err(heap, m)),
            }
        }
        "stream-write-byte" => {
            let (h, b) = (arg!(int(args, 0, name)), arg!(int(args, 1, name)));
            match u8::try_from(b) {
                Ok(b) => wrap!(with_streams(|t| t.write_byte(h, b)), |_v: ()| Value::Empty),
                Err(_) => Ok(result_err(heap, format!("write-byte: {} is not a byte (0..255)", b))),
            }
        }
        "stream-unread-char" => {
            let (h, c) = (arg!(int(args, 0, name)), arg!(character(args, 1, name)));
            wrap!(with_streams(|t| t.unread_char(h, c)), |_v: ()| Value::Empty)
        }
        "stream-listen" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(|t| t.listen(h)), |v: bool| Value::Bool(v))
        }
        "stream-write-string" => {
            let (h, s) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            wrap!(with_streams(|t| t.write_str(h, &s)), |_v: ()| Value::Empty)
        }
        "stream-at-line-start" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(|t| t.at_line_start(h)), |v: bool| Value::Bool(v))
        }
        "stream-finish-output" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(|t| t.finish_output(h)), |_v: ()| Value::Empty)
        }
        "stream-take-output-string" => {
            let h = arg!(int(args, 0, name));
            match with_streams(|t| t.take_output_string(h)) {
                Ok(s) => {
                    let sv = heap.alloc_string(s);
                    Ok(result_ok(heap, sv))
                }
                Err(m) => Ok(result_err(heap, m)),
            }
        }
        "file-exists-p" => {
            let p = arg!(text(heap, args, 0, name));
            Ok(Value::Bool(std::path::Path::new(&p).exists()))
        }
        "file-delete" => {
            let p = arg!(text(heap, args, 0, name));
            match std::fs::remove_file(&p) {
                Ok(()) => Ok(result_ok(heap, Value::Empty)),
                Err(e) => Ok(result_err(heap, format!("delete-file: {}: {}", p, e))),
            }
        }
        // The queries that only look at the filesystem, no stream involved
        // (CLHS 20.1). Each is the primitive under a `Pathish` prelude
        // wrapper that carries the CL name: `truename`, `file-write-date`,
        // `directory-p`, `directory`, `ensure-directories-exist`.
        //
        // They live here rather than in `sys_builtin` because the `file-`
        // prefix is a *routing rule*, not a naming convention:
        // `Interp::eval_builtin` sends every name starting `stream-`/`file-`
        // straight to this dispatch, so a `file-*` builtin implemented
        // anywhere else would never be reached interpreted.
        "file-truename" => {
            let p = arg!(text(heap, args, 0, name));
            match std::fs::canonicalize(&p) {
                Ok(c) => {
                    let sv = heap.alloc_string(c.to_string_lossy().into_owned());
                    Ok(result_ok(heap, sv))
                }
                Err(e) => Ok(result_err(heap, format!("truename: {}: {}", p, e))),
            }
        }
        // The result is a *universal* time — seconds since 1900-01-01 UTC,
        // the scale `get-universal-time` counts on — so a file's timestamp
        // and the clock are directly comparable and either can be handed to
        // the prelude's `decode-universal-time`.
        "file-modified-date" => {
            let p = arg!(text(heap, args, 0, name));
            const UNIX_TO_CL_EPOCH_SECS: i64 = 2_208_988_800;
            match std::fs::metadata(&p).and_then(|m| m.modified()) {
                Ok(t) => match t.duration_since(std::time::UNIX_EPOCH) {
                    Ok(d) => Ok(result_ok(heap, Value::Int(d.as_secs() as i64 + UNIX_TO_CL_EPOCH_SECS))),
                    Err(e) => Ok(result_err(heap, format!("file-write-date: {}: timestamp precedes the Unix epoch: {}", p, e))),
                },
                Err(e) => Ok(result_err(heap, format!("file-write-date: {}: {}", p, e))),
            }
        }
        // `false` for a plain file *and* for something that isn't there —
        // the same "a question, not a fallible operation" shape
        // `file-exists-p` has. A caller that must tell the two apart asks
        // `file-exists-p` as well.
        "file-directory-p" => {
            let p = arg!(text(heap, args, 0, name));
            Ok(Value::Bool(std::path::Path::new(&p).is_dir()))
        }
        // Entries as full paths, in whatever order the OS reports them:
        // `readdir` order carries no meaning, so sorting belongs to the
        // caller that wants a stable listing, not here. `.` and `..` are not
        // entries — `read_dir` does not yield them.
        //
        // Allocating every element before the vector box is safe as written:
        // `alloc_string` and `alloc_struct` never collect (only `Heap::cons`
        // does), so nothing can invalidate the `Vec<Value>` in between. A
        // version of this that consed would need each element rooted.
        "file-list-directory" => {
            let p = arg!(text(heap, args, 0, name));
            let entries = match std::fs::read_dir(&p) {
                Ok(rd) => rd,
                Err(e) => return Some(Ok(result_err(heap, format!("directory: {}: {}", p, e)))),
            };
            let mut names: Vec<String> = Vec::new();
            for entry in entries {
                match entry {
                    Ok(e) => names.push(e.path().to_string_lossy().into_owned()),
                    Err(e) => return Some(Ok(result_err(heap, format!("directory: {}: {}", p, e)))),
                }
            }
            let elems: Vec<Value> = names.into_iter().map(|n| heap.alloc_string(n)).collect();
            let vec_val = heap.alloc_struct(TypeKeyId::VECTOR, elems);
            Ok(result_ok(heap, vec_val))
        }
        // Succeeds when the directory already exists: `create_dir_all` is
        // idempotent, which is exactly what "ensure" means in CL's name.
        "file-create-directories" => {
            let p = arg!(text(heap, args, 0, name));
            match std::fs::create_dir_all(&p) {
                Ok(()) => Ok(result_ok(heap, Value::Empty)),
                Err(e) => Ok(result_err(heap, format!("ensure-directories-exist: {}: {}", p, e))),
            }
        }
        "file-rename" => {
            let (a, b) = (arg!(text(heap, args, 0, name)), arg!(text(heap, args, 1, name)));
            match std::fs::rename(&a, &b) {
                Ok(()) => Ok(result_ok(heap, Value::Empty)),
                Err(e) => Ok(result_err(heap, format!("rename-file: {}: {}", a, e))),
            }
        }
        _ => return None,
    })
}
