//! The file builtins as *values* — `open`'s `stream-open-file` and the
//! `file-*` queries — called from both sides of the compile boundary, as
//! `typelisp_rt::stream_builtin` is for the rest of the streams.

use typelisp_mem::{spec, wk, Heap, StdModule, SymRef, TypeId, TypeSpec, Value};
use typelisp_rt::result_value::{err_in, ok_in, option_in, vector_in, TypeTable};
use typelisp_rt::stream::with_streams;
use typelisp_rt::stream_builtin::{int, text, ArgError, FILE_ERROR};

/// The runtime type each builtin's *result* carries
/// (`typelisp_rt::result_value`).
pub const RESULT_TYPES: TypeTable = &[
    (wk::F_STREAM_OPEN_FILE, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, FILE_ERROR])),
    (wk::F_FILE_DELETE, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, FILE_ERROR])),
    (wk::F_FILE_TRUENAME, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::STRING, FILE_ERROR])),
    (wk::F_FILE_MODIFIED_DATE, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::TIME, "universal-time", &[]), FILE_ERROR])),
    (wk::F_FILE_OWNER_NAME, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING]), FILE_ERROR])),
    (wk::F_FILE_LIST_DIRECTORY, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::VEC, "Vector", &[spec::STRING]), FILE_ERROR])),
    (wk::F_FILE_CREATE_DIRECTORIES, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, FILE_ERROR])),
    (wk::F_FILE_RENAME, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, FILE_ERROR])),
];

/// The type of the value *inside* the `Result`, for the builtins whose
/// payload is itself a box.
pub const INNER_TYPES: TypeTable = &[
    (wk::F_FILE_LIST_DIRECTORY, TypeSpec::Named(StdModule::VEC, "Vector", &[spec::STRING])),
    (wk::F_FILE_OWNER_NAME, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
];

fn result_ok(heap: &mut Heap, name: SymRef, v: Value) -> Value {
    ok_in(heap, RESULT_TYPES, name.well_known(), v)
}

/// `Err(FileError(msg))`.
fn result_err(heap: &mut Heap, name: SymRef, msg: String) -> Value {
    err_in(heap, RESULT_TYPES, name.well_known(), TypeId::FILE_ERROR, msg)
}

fn option_value(heap: &mut Heap, name: SymRef, v: Option<Value>) -> Value {
    option_in(heap, INNER_TYPES, name.well_known(), v)
}

/// Every file builtin, or `None` if `name` isn't one.
///
/// The `Ok`/`Err` of the *return* is about the arguments, not about the
/// operation: a missing file is an ordinary `Err(FileError(...))` **value**
/// (`Ok(Value)` here), because programs handle those. Only a wrongly-shaped
/// argument — which the checker rules out — reaches the `Err` arm.
pub fn fs_builtin(heap: &mut Heap, sym: SymRef, args: &[Value]) -> Option<Result<Value, ArgError>> {
    let name = sym.name();
    macro_rules! arg {
        ($e:expr) => {
            match $e {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            }
        };
    }

    Some(match sym.well_known() {
        wk::F_STREAM_OPEN_FILE => {
            let (p, mode) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            match with_streams(heap, |t| crate::file::open_file(t, &p, mode)) {
                Ok(h) => Ok(result_ok(heap, sym, Value::Int(h))),
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        }
        wk::F_FILE_EXISTS_P => {
            let p = arg!(text(heap, args, 0, name));
            Ok(Value::Bool(std::path::Path::new(&p).exists()))
        }
        wk::F_FILE_DELETE => {
            let p = arg!(text(heap, args, 0, name));
            match std::fs::remove_file(&p) {
                Ok(()) => Ok(result_ok(heap, sym, Value::Empty)),
                Err(e) => Ok(result_err(heap, sym, format!("delete-file: {}: {}", p, e))),
            }
        }
        // The queries that only look at the filesystem, no stream involved
        // (CLHS 20.1). Each is the primitive under a `Pathish` prelude
        // wrapper that carries the CL name: `truename`, `file-write-date`,
        // `directory-p`, `directory`, `ensure-directories-exist`.
        //
        wk::F_FILE_TRUENAME => {
            let p = arg!(text(heap, args, 0, name));
            match std::fs::canonicalize(&p) {
                Ok(c) => {
                    let sv = heap.alloc_string(c.to_string_lossy().into_owned());
                    Ok(result_ok(heap, sym, sv))
                }
                Err(e) => Ok(result_err(heap, sym, format!("truename: {}: {}", p, e))),
            }
        }
        // The result is a *universal* time — seconds since 1900-01-01 UTC,
        // the scale `get-universal-time` counts on — so a file's timestamp
        // and the clock are directly comparable and either can be handed to
        // the prelude's `decode-universal-time`.
        wk::F_FILE_MODIFIED_DATE => {
            let p = arg!(text(heap, args, 0, name));
            const UNIX_TO_CL_EPOCH_SECS: i64 = 2_208_988_800;
            match std::fs::metadata(&p).and_then(|m| m.modified()) {
                Ok(t) => match t.duration_since(std::time::UNIX_EPOCH) {
                    Ok(d) => {
                        let tv = typelisp_rt::sys_builtin::universal_time_value(
                            heap,
                            d.as_secs() as i64 + UNIX_TO_CL_EPOCH_SECS,
                        );
                        Ok(result_ok(heap, sym, tv))
                    }
                    Err(e) => Ok(result_err(heap, sym, format!("file-write-date: {}: timestamp precedes the Unix epoch: {}", p, e))),
                },
                Err(e) => Ok(result_err(heap, sym, format!("file-write-date: {}: {}", p, e))),
            }
        }
        // CL's `file-author`. Two failures that CL keeps apart stay apart: a missing file is an
        // `Err`, an owner with no password-database entry is `Ok(None)` —
        // which is exactly what CL means by "or nil if the author's name
        // cannot be determined".
        wk::F_FILE_OWNER_NAME => {
            let p = arg!(text(heap, args, 0, name));
            match typelisp_rt::os::file_owner(&p) {
                Ok(who) => {
                    let inner = who.map(|s| heap.alloc_string(s));
                    let ov = option_value(heap, sym, inner);
                    Ok(result_ok(heap, sym, ov))
                }
                Err(e) => Ok(result_err(heap, sym, format!("file-author: {}: {}", p, e))),
            }
        }
        // `false` for a plain file *and* for something that isn't there —
        // the same "a question, not a fallible operation" shape
        // `file-exists-p` has. A caller that must tell the two apart asks
        // `file-exists-p` as well.
        wk::F_FILE_DIRECTORY_P => {
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
        wk::F_FILE_LIST_DIRECTORY => {
            let p = arg!(text(heap, args, 0, name));
            let entries = match std::fs::read_dir(&p) {
                Ok(rd) => rd,
                Err(e) => return Some(Ok(result_err(heap, sym, format!("directory: {}: {}", p, e)))),
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
                                sym,
                                format!("directory: {}: {}", e.path().display(), err),
                            )))
                        }
                    },
                    Err(e) => return Some(Ok(result_err(heap, sym, format!("directory: {}: {}", p, e)))),
                }
            }
            let elems: Vec<Value> = names.into_iter().map(|n| heap.alloc_string(n)).collect();
            let vec_val = vector_in(heap, INNER_TYPES, sym.well_known(), elems);
            Ok(result_ok(heap, sym, vec_val))
        }
        // Succeeds when the directory already exists: `create_dir_all` is
        // idempotent, which is exactly what "ensure" means in CL's name.
        wk::F_FILE_CREATE_DIRECTORIES => {
            let p = arg!(text(heap, args, 0, name));
            match std::fs::create_dir_all(&p) {
                Ok(()) => Ok(result_ok(heap, sym, Value::Empty)),
                Err(e) => Ok(result_err(heap, sym, format!("ensure-directories-exist: {}: {}", p, e))),
            }
        }
        wk::F_FILE_RENAME => {
            let (a, b) = (arg!(text(heap, args, 0, name)), arg!(text(heap, args, 1, name)));
            match std::fs::rename(&a, &b) {
                Ok(()) => Ok(result_ok(heap, sym, Value::Empty)),
                Err(e) => Ok(result_err(heap, sym, format!("rename-file: {}: {}", a, e))),
            }
        }
        _ => return None,
    })
}
