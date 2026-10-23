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

use typelisp_mem::{spec, Heap, NsId, TypeId, TypeSpec, Value};

use crate::stream::with_streams;

// The types the results here are built with — `TypeId::OPTION`, `RESULT`,
// and `FILE_ERROR` — come from `typelisp_mem`'s pre-interned table rather
// than being named here, because this crate has no `Path` to derive a name
// from (the same standing exception `rt_data_new` has, which receives its key
// as a string from compiled code). That their names are the ones
// `type_key::type_key_of` produces is asserted by
// `tests/type_identity_guard_test.rs` — checked, not assumed, because the
// failure it guards is silent: a value built here under a key nobody else
// spells would be unmatchable and fail to print.
//
// `Vector<T>`'s box is a `Struct`, not an `Enum` — the "just a box + a name"
// representation `value.rs`'s `BoxedObj` doc comment describes, whose *fields
// are its elements*. `file-list-directory` builds one directly.

const FILE_ERROR: TypeSpec = TypeSpec::Named(NsId::STD, "fileerror", &[]);

/// The runtime type each builtin's *result* carries, and — for the ones
/// that wrap something generic inside it — the type of that inner value.
///
/// A type's identity includes its instantiation, and these shims build their
/// `Result`/`Option`/`Vector` boxes with no checker to ask: this crate sits
/// below it. So the type is written here, once per builtin, and
/// `tests/type_identity_guard_test.rs` compares every row against that
/// builtin's own registry return type. Checked rather than assumed for the
/// reason the module comment gives: a value built under a type nobody else
/// gives it is unmatchable and fails to print.
///
/// A builtin whose result is not a box (`stream-stdin`'s handle,
/// `stream-open-p`'s `bool`) has no row and needs none.
pub const RESULT_TYPES: &[(&str, TypeSpec)] = &[
    ("stream-open-file", TypeSpec::Named(NsId::STD, "result", &[spec::I32, FILE_ERROR])),
    ("stream-close", TypeSpec::Named(NsId::STD, "result", &[spec::UNIT, FILE_ERROR])),
    ("stream-input-p", TypeSpec::Named(NsId::STD, "result", &[spec::BOOL, FILE_ERROR])),
    ("stream-output-p", TypeSpec::Named(NsId::STD, "result", &[spec::BOOL, FILE_ERROR])),
    ("stream-listen", TypeSpec::Named(NsId::STD, "result", &[spec::BOOL, FILE_ERROR])),
    ("stream-position", TypeSpec::Named(NsId::STD, "result", &[spec::INT, FILE_ERROR])),
    ("stream-at-line-start", TypeSpec::Named(NsId::STD, "result", &[spec::BOOL, FILE_ERROR])),
    ("stream-read-char", TypeSpec::Named(NsId::STD, "result", &[TypeSpec::Named(NsId::STD, "option", &[spec::CHAR]), FILE_ERROR])),
    ("stream-unread-char", TypeSpec::Named(NsId::STD, "result", &[spec::UNIT, FILE_ERROR])),
    ("stream-read-byte", TypeSpec::Named(NsId::STD, "result", &[TypeSpec::Named(NsId::STD, "option", &[spec::INT]), FILE_ERROR])),
    ("stream-write-byte", TypeSpec::Named(NsId::STD, "result", &[spec::UNIT, FILE_ERROR])),
    ("stream-write-string", TypeSpec::Named(NsId::STD, "result", &[spec::UNIT, FILE_ERROR])),
    ("stream-finish-output", TypeSpec::Named(NsId::STD, "result", &[spec::UNIT, FILE_ERROR])),
    ("stream-take-output-string", TypeSpec::Named(NsId::STD, "result", &[spec::STRING, FILE_ERROR])),
    ("file-delete", TypeSpec::Named(NsId::STD, "result", &[spec::UNIT, FILE_ERROR])),
    ("file-truename", TypeSpec::Named(NsId::STD, "result", &[spec::STRING, FILE_ERROR])),
    ("file-modified-date", TypeSpec::Named(NsId::STD, "result", &[TypeSpec::Named(NsId::STD, "universal-time", &[]), FILE_ERROR])),
    ("file-owner-name", TypeSpec::Named(NsId::STD, "result", &[TypeSpec::Named(NsId::STD, "option", &[spec::STRING]), FILE_ERROR])),
    ("file-list-directory", TypeSpec::Named(NsId::STD, "result", &[TypeSpec::Named(NsId::STD, "vector", &[spec::STRING]), FILE_ERROR])),
    ("file-create-directories", TypeSpec::Named(NsId::STD, "result", &[spec::UNIT, FILE_ERROR])),
    ("file-rename", TypeSpec::Named(NsId::STD, "result", &[spec::UNIT, FILE_ERROR])),
];

/// The type of the value *inside* the `Result` — [`RESULT_TYPES`]' companion for
/// the three builtins whose payload is itself a box.
pub const INNER_TYPES: &[(&str, TypeSpec)] = &[
    ("stream-read-char", TypeSpec::Named(NsId::STD, "option", &[spec::CHAR])),
    ("stream-read-byte", TypeSpec::Named(NsId::STD, "option", &[spec::INT])),
    ("file-list-directory", TypeSpec::Named(NsId::STD, "vector", &[spec::STRING])),
    ("file-owner-name", TypeSpec::Named(NsId::STD, "option", &[spec::STRING])),
];

pub(crate) fn lookup(table: &[(&str, TypeSpec)], name: &str, what: &str) -> TypeId {
    match table.iter().find(|(n, _)| *n == name) {
        Some((_, ty)) => typelisp_mem::intern_spec(ty),
        // Not a recoverable condition: a builtin that builds a box without a
        // row would give that value an identity nothing else spells.
        None => crate::fatal(&format!("stream_builtin: no {} type for `{}`", what, name)),
    }
}

/// `Ok(v)`, matching `result_def`'s variant order (`ok` = 0, `err` = 1).
fn result_ok(heap: &mut Heap, name: &str, v: Value) -> Value {
    let key = lookup(RESULT_TYPES, name, "result");
    heap.alloc_enum(key, 0, vec![v])
}

/// `Err(FileError(msg))` — the concrete error type every stream and file
/// operation fails with, wrapped in `Result`'s `err` variant. `FileError` has
/// exactly one variant (index 0) carrying the message.
fn result_err(heap: &mut Heap, name: &str, msg: String) -> Value {
    let msg_val = heap.alloc_string(msg);
    // `FileError` takes no type arguments, so its key is unchanged by the
    // instantiation rule and stays the pre-interned constant.
    let err_val = heap.alloc_enum(TypeId::FILE_ERROR, 0, vec![msg_val]);
    let key = lookup(RESULT_TYPES, name, "result");
    heap.alloc_enum(key, 1, vec![err_val])
}

/// `Some(v)`/`None` under the instantiation the inner key spells — the
/// niche when the payload allows one, else a box in `option_def`'s variant
/// order (`Heap::alloc_option` decides).
fn option_value(heap: &mut Heap, name: &str, v: Option<Value>) -> Value {
    heap.alloc_option(lookup(INNER_TYPES, name, "inner"), v)
}

/// An argument that isn't the shape its signature promises. Unreachable
/// through the type checker; each caller reports it in its own idiom (a
/// catchable internal error interpreted, [`crate::fatal`] compiled).
pub type ArgError = String;

/// A stream handle, or `open`'s mode — both plain integers.
pub(crate) fn int(args: &[Value], i: usize, who: &str) -> Result<i64, ArgError> {
    match args.get(i) {
        Some(Value::Int(n)) => Ok(*n),
        other => Err(format!("{}: argument {} is not an integer, got {:?}", who, i, other)),
    }
}

pub(crate) fn text(heap: &Heap, args: &[Value], i: usize, who: &str) -> Result<String, ArgError> {
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
                Ok(v) => Ok(result_ok(heap, name, $ok(v))),
                Err(m) => Ok(result_err(heap, name, m)),
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
        "stream-stdin" => Ok(Value::Int(with_streams(heap, |t| t.stdin()))),
        "stream-stdout" => Ok(Value::Int(with_streams(heap, |t| t.stdout()))),
        "stream-stderr" => Ok(Value::Int(with_streams(heap, |t| t.stderr()))),
        "stream-string-input" => {
            let s = arg!(text(heap, args, 0, name));
            Ok(Value::Int(with_streams(heap, |t| t.string_input(&s))))
        }
        "stream-string-output" => Ok(Value::Int(with_streams(heap, |t| t.string_output()))),
        "stream-open-file" => {
            let (p, mode) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(heap, |t| t.open_file(&p, mode)), |v: i64| Value::Int(v))
        }
        "stream-close" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.close(h)), |_v: ()| Value::Empty)
        }
        "stream-open-p" => {
            let h = arg!(int(args, 0, name));
            Ok(Value::Bool(with_streams(heap, |t| t.is_open(h))))
        }
        // A handle the prelude holds is always one the table issued, so a
        // failure here is a broken invariant, reported on this function's
        // error channel like a malformed argument.
        "stream-describe" => {
            let h = arg!(int(args, 0, name));
            let text = arg!(with_streams(heap, |t| t.describe(h)).map_err(|e| format!("{}: {}", name, e)));
            Ok(heap.alloc_string(text))
        }
        "stream-input-p" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.is_input(h)), |v: bool| Value::Bool(v))
        }
        "stream-output-p" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.is_output(h)), |v: bool| Value::Bool(v))
        }
        "stream-read-char" => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.read_char(h)) {
                Ok(c) => {
                    let inner = option_value(heap, name, c.map(Value::Char));
                    Ok(result_ok(heap, name, inner))
                }
                Err(m) => Ok(result_err(heap, name, m)),
            }
        }
        "stream-read-byte" => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.read_byte(h)) {
                Ok(b) => {
                    let inner = option_value(heap, name, b.map(|b| Value::Int(b as i64)));
                    Ok(result_ok(heap, name, inner))
                }
                Err(m) => Ok(result_err(heap, name, m)),
            }
        }
        "stream-write-byte" => {
            let (h, b) = (arg!(int(args, 0, name)), arg!(int(args, 1, name)));
            match u8::try_from(b) {
                Ok(b) => wrap!(with_streams(heap, |t| t.write_byte(h, b)), |_v: ()| Value::Empty),
                Err(_) => Ok(result_err(heap, name, format!("write-byte: {} is not a byte (0..255)", b))),
            }
        }
        "stream-unread-char" => {
            let (h, c) = (arg!(int(args, 0, name)), arg!(character(args, 1, name)));
            wrap!(with_streams(heap, |t| t.unread_char(h, c)), |_v: ()| Value::Empty)
        }
        "stream-listen" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.listen(h)), |v: bool| Value::Bool(v))
        }
        "stream-position" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.position(h)), |v: i64| Value::Int(v))
        }
        "stream-write-string" => {
            let (h, s) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            wrap!(with_streams(heap, |t| t.write_str(h, &s)), |_v: ()| Value::Empty)
        }
        "stream-at-line-start" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.at_line_start(h)), |v: bool| Value::Bool(v))
        }
        "stream-finish-output" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.finish_output(h)), |_v: ()| Value::Empty)
        }
        "stream-take-output-string" => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.take_output_string(h)) {
                Ok(s) => {
                    let sv = heap.alloc_string(s);
                    Ok(result_ok(heap, name, sv))
                }
                Err(m) => Ok(result_err(heap, name, m)),
            }
        }
        "file-exists-p" => {
            let p = arg!(text(heap, args, 0, name));
            Ok(Value::Bool(std::path::Path::new(&p).exists()))
        }
        "file-delete" => {
            let p = arg!(text(heap, args, 0, name));
            match std::fs::remove_file(&p) {
                Ok(()) => Ok(result_ok(heap, name, Value::Empty)),
                Err(e) => Ok(result_err(heap, name, format!("delete-file: {}: {}", p, e))),
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
                    Ok(result_ok(heap, name, sv))
                }
                Err(e) => Ok(result_err(heap, name, format!("truename: {}: {}", p, e))),
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
                    Ok(d) => {
                        let tv = crate::sys_builtin::universal_time_value(
                            heap,
                            d.as_secs() as i64 + UNIX_TO_CL_EPOCH_SECS,
                        );
                        Ok(result_ok(heap, name, tv))
                    }
                    Err(e) => Ok(result_err(heap, name, format!("file-write-date: {}: timestamp precedes the Unix epoch: {}", p, e))),
                },
                Err(e) => Ok(result_err(heap, name, format!("file-write-date: {}: {}", p, e))),
            }
        }
        // CL's `file-author`, under a `file-`-prefixed name because that
        // prefix is what routes a builtin here at all (see the note above).
        // Two failures that CL keeps apart stay apart: a missing file is an
        // `Err`, an owner with no password-database entry is `Ok(none)` —
        // which is exactly what CL means by "or nil if the author's name
        // cannot be determined".
        "file-owner-name" => {
            let p = arg!(text(heap, args, 0, name));
            match crate::os::file_owner(&p) {
                Ok(who) => {
                    let inner = who.map(|s| heap.alloc_string(s));
                    let ov = option_value(heap, name, inner);
                    Ok(result_ok(heap, name, ov))
                }
                Err(e) => Ok(result_err(heap, name, format!("file-author: {}: {}", p, e))),
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
        // Entries as truenames, in whatever order the OS reports them: CL's
        // `directory` answers with "the truenames of those files", so each
        // entry is absolute with its symlinks resolved — the same thing
        // `file-truename` answers for it. A symlink whose target is missing
        // names no file that is present, so it is not an entry (CL lists
        // files present in the file system; `probe-file` says `false` for
        // it too). `readdir` order carries no meaning, so sorting belongs to
        // the caller that wants a stable listing, not here. `.` and `..` are
        // not entries — `read_dir` does not yield them.
        //
        // Allocating every element before the vector box is safe as written:
        // `alloc_string` and `alloc_struct` never collect (only `Heap::cons`
        // does), so nothing can invalidate the `Vec<Value>` in between. A
        // version of this that consed would need each element rooted.
        "file-list-directory" => {
            let p = arg!(text(heap, args, 0, name));
            let entries = match std::fs::read_dir(&p) {
                Ok(rd) => rd,
                Err(e) => return Some(Ok(result_err(heap, name, format!("directory: {}: {}", p, e)))),
            };
            let mut names: Vec<String> = Vec::new();
            for entry in entries {
                match entry {
                    Ok(e) => match std::fs::canonicalize(e.path()) {
                        Ok(t) => names.push(t.to_string_lossy().into_owned()),
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                        Err(err) => {
                            return Some(Ok(result_err(
                                heap,
                                name,
                                format!("directory: {}: {}", e.path().display(), err),
                            )))
                        }
                    },
                    Err(e) => return Some(Ok(result_err(heap, name, format!("directory: {}: {}", p, e)))),
                }
            }
            let elems: Vec<Value> = names.into_iter().map(|n| heap.alloc_string(n)).collect();
            let vec_val = { let k = lookup(INNER_TYPES, name, "inner"); heap.alloc_struct(k, elems) };
            Ok(result_ok(heap, name, vec_val))
        }
        // Succeeds when the directory already exists: `create_dir_all` is
        // idempotent, which is exactly what "ensure" means in CL's name.
        "file-create-directories" => {
            let p = arg!(text(heap, args, 0, name));
            match std::fs::create_dir_all(&p) {
                Ok(()) => Ok(result_ok(heap, name, Value::Empty)),
                Err(e) => Ok(result_err(heap, name, format!("ensure-directories-exist: {}: {}", p, e))),
            }
        }
        "file-rename" => {
            let (a, b) = (arg!(text(heap, args, 0, name)), arg!(text(heap, args, 1, name)));
            match std::fs::rename(&a, &b) {
                Ok(()) => Ok(result_ok(heap, name, Value::Empty)),
                Err(e) => Ok(result_err(heap, name, format!("rename-file: {}: {}", a, e))),
            }
        }
        _ => return None,
    })
}
