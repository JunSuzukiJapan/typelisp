//! The compiled-code shims of this crate's builtins. What each adds is the
//! *representation* conversion the compiled ABI needs and the interpreter
//! does not: a mode and a `bool` are bare machine words, a `string` and
//! every `Result` are tagged heap values.

use typelisp_mem::Value;
use typelisp_rt::{active_heap, decode, encode, fatal};

typelisp_rt::stream_shim!(rt_stream_open_file, typelisp_mem::wk::F_STREAM_OPEN_FILE, "stream-open-file", [str, int], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_exists_p, typelisp_mem::wk::F_FILE_EXISTS_P, "file-exists-p", [str], bool, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_delete, typelisp_mem::wk::F_FILE_DELETE, "file-delete", [str], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_rename, typelisp_mem::wk::F_FILE_RENAME, "file-rename", [str, str], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_truename, typelisp_mem::wk::F_FILE_TRUENAME, "file-truename", [str], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_modified_date, typelisp_mem::wk::F_FILE_MODIFIED_DATE, "file-modified-date", [str], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_directory_p, typelisp_mem::wk::F_FILE_DIRECTORY_P, "file-directory-p", [str], bool, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_list_directory, typelisp_mem::wk::F_FILE_LIST_DIRECTORY, "file-list-directory", [str], tagged, via crate::fs_builtin::fs_builtin);
typelisp_rt::stream_shim!(rt_file_create_directories, typelisp_mem::wk::F_FILE_CREATE_DIRECTORIES, "file-create-directories", [str], tagged, via crate::fs_builtin::fs_builtin);

/// The one-string argument, decoded at the calling convention's edge: a
/// compiled `string` arrives as a tagged `Value::Str`.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to a valid `i64` encoding a
/// `Value::Str`; a `Heap` must be registered on this thread.
unsafe fn str_arg(args: *const i64, argc: u32, who: &str) -> String {
    if argc < 1 {
        fatal(&format!("{}: expected 1 argument", who));
    }
    match decode(*args) {
        Value::Str(id) => active_heap().string(id).to_string(),
        other => fatal(&format!("{}: argument is not a string, got {:?}", who, other)),
    }
}

/// `(command-line-args)` for compiled code — a freshly allocated
/// `Vector<string>`, as unrooted as every other allocating shim's result.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_command_line_args(_args: *const i64, _argc: u32) -> i64 {
    encode(crate::env::command_line_args(active_heap()))
}

/// `(getenv name)` for compiled code.
///
/// # Safety
///
/// Same as [`str_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_getenv(args: *const i64, argc: u32) -> i64 {
    let name = str_arg(args, argc, "rt_getenv");
    encode(crate::env::getenv(active_heap(), &name))
}

/// `(home-directory)` for compiled code.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_home_directory(_args: *const i64, _argc: u32) -> i64 {
    encode(crate::env::home_directory(active_heap()))
}

/// `(dribble-start path)` for compiled code.
///
/// # Safety
///
/// Same as [`str_arg`].
#[no_mangle]
pub unsafe extern "C" fn rt_dribble_start(args: *const i64, argc: u32) -> i64 {
    let path = str_arg(args, argc, "rt_dribble_start");
    encode(crate::dribble::dribble_start(active_heap(), &path))
}

/// `(dribble-stop)` for compiled code.
///
/// # Safety
///
/// `args`/`argc` are unused (the builtin is nullary). A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_dribble_stop(_args: *const i64, _argc: u32) -> i64 {
    encode(crate::dribble::dribble_stop(active_heap()))
}
