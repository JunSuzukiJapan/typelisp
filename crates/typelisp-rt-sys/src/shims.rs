//! The compiled-code shims of this crate's builtins. What each adds is the
//! *representation* conversion the compiled ABI needs and the interpreter
//! does not: a mode and a `bool` are bare machine words, a `string` and
//! every `Result` are tagged heap values.

use typelisp_mem::Value;
use typelisp_rt::{active_heap, decode, encode, fatal};

typelisp_rt::stream_shim!(rt_stream_open_file, typelisp_mem::wk::F_STREAM_OPEN_FILE, "stream-open-file", [a: str, b: int], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_exists_p, typelisp_mem::wk::F_FILE_EXISTS_P, "file-exists-p", [a: str], bool, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_delete, typelisp_mem::wk::F_FILE_DELETE, "file-delete", [a: str], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_rename, typelisp_mem::wk::F_FILE_RENAME, "file-rename", [a: str, b: str], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_truename, typelisp_mem::wk::F_FILE_TRUENAME, "file-truename", [a: str], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_modified_date, typelisp_mem::wk::F_FILE_MODIFIED_DATE, "file-modified-date", [a: str], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_directory_p, typelisp_mem::wk::F_FILE_DIRECTORY_P, "file-directory-p", [a: str], bool, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_list_directory, typelisp_mem::wk::F_FILE_LIST_DIRECTORY, "file-list-directory", [a: str], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_create_directories, typelisp_mem::wk::F_FILE_CREATE_DIRECTORIES, "file-create-directories", [a: str], tagged, via crate::fs_builtin::fs_builtin);

/// The one-string argument, decoded at the calling convention's edge: a
/// compiled `string` arrives as a tagged `Value::Str`.
///
/// # Safety
///
/// The word must encode a
/// `Value::Str`; a `Heap` must be registered on this thread.
unsafe fn str_arg(x: i64, who: &str) -> String {
    match decode(x) {
        Value::Str(id) => active_heap().string(id).to_string(),
        other => fatal(&format!("{}: argument is not a string, got {:?}", who, other)),
    }
}

/// `(command-line-args)` for compiled code — a freshly allocated
/// `Vector<string>`, as unrooted as every other allocating shim's result.
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_command_line_args() -> i64 {
    encode(crate::env::command_line_args(active_heap()))
}

/// `(getenv name)` for compiled code.
///
/// # Safety
///
/// Same as [`str_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_getenv(x: i64) -> i64 {
    let name = str_arg(x, "rt_getenv");
    encode(crate::env::getenv(active_heap(), &name))
}

/// `(home-directory)` for compiled code.
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_home_directory() -> i64 {
    encode(crate::env::home_directory(active_heap()))
}

/// `(dribble-start path)` for compiled code.
///
/// # Safety
///
/// Same as [`str_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_dribble_start(x: i64) -> i64 {
    let path = str_arg(x, "rt_dribble_start");
    encode(crate::dribble::dribble_start(active_heap(), &path))
}

/// `(dribble-stop)` for compiled code.
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_dribble_stop() -> i64 {
    encode(crate::dribble::dribble_stop(active_heap()))
}
