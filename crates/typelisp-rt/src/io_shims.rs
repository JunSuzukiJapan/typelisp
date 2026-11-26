//! Compiled-code shims for the stream and readtable builtins.

use typelisp_mem::Word;
use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal};
use typelisp_mem::Value;

// ---- stream builtins ----------------------------------------------------
//
// One shim per builtin (`crate::stream_builtin::stream_builtin` is the
// implementation both these and the interpreter call). What each shim adds is
// the *representation* conversion the compiled ABI needs and the interpreter
// does not: an `i64` handle, an `i32` mode, a `bool` and a `char` are bare
// machine words in compiled code, while a `string` and every `Result` are
// tagged heap values. Which is which comes from the builtin's own signature
// (`registry::register_stream_builtins`), so it is spelled out per shim
// rather than guessed from the value.


/// Generates one shim over a builtin dispatcher: `rt_stream_*` here, and the
/// file and socket shims of the runtime crates that bring those
/// (`typelisp-rt-sys`, `typelisp-rt-posix`), which call it as
/// `typelisp_rt::stream_shim!`.
///
/// `$arg` names how each parameter arrives (`int`/`str`/`char`/`bool`/
/// `tagged`), `$ret` how the result leaves (`tagged` for a heap value, `raw`
/// for a bare `i64`, `bool` for a bare `0`/`1`), and `$dispatch` is the
/// function both the shim and the interpreter call.
#[macro_export]
macro_rules! stream_shim {
    // Through a dispatcher that answers `None` for a name it has not got.
    ($shim:ident, $sym:expr, $name:literal, [$($p:ident: $arg:ident),*], $ret:ident, via $dispatch:path) => {
        $crate::stream_shim!(@shim $shim, $sym, $name, [$($p: $arg),*], $ret, |heap, sym, decoded| $dispatch(heap, sym, decoded));
    };
    // Straight to the one operation, so that the shim links only that one.
    ($shim:ident, $sym:expr, $name:literal, [$($p:ident: $arg:ident),*], $ret:ident, op $op:path) => {
        $crate::stream_shim!(@shim $shim, $sym, $name, [$($p: $arg),*], $ret, |heap, sym, decoded| Some($op(heap, sym, decoded)));
    };
    (@shim $shim:ident, $sym:expr, $name:literal, [$($p:ident: $arg:ident),*], $ret:ident, |$heap:ident, $s:ident, $decoded:ident| $call:expr) => {
        /// A builtin for compiled code — see the operation it calls, which
        /// is also what the interpreter calls.
        ///
        /// # Safety
        ///
        /// Each word must be in the representation the builtin's signature
        /// gives it; a `Heap` must already be registered on this thread.
        #[no_mangle]
        pub unsafe extern "C" fn $shim($($p: $crate::stream_shim!(@ty $arg)),*) -> $crate::stream_shim!(@ret_ty $ret) {
            #[allow(unused_mut)]
            let mut decoded: $crate::__shim::Vec<$crate::__shim::Value> = $crate::__shim::Vec::new();
            $(
                decoded.push($crate::stream_shim!(@arg $arg, $p));
            )*
            let ($heap, $s, $decoded) = ($crate::active_heap(), $crate::__shim::well_known_symbol($sym), &decoded);
            match $call {
                Some(Ok(v)) => $crate::stream_shim!(@ret $ret, v),
                Some(Err(e)) => $crate::fatal(&e),
                None => $crate::fatal(concat!(stringify!($shim), ": ", $name, " is not a builtin of its dispatcher")),
            }
        }
    };
    // What each argument and result is carried as.
    (@ty handle) => { i32 };
    (@ty int) => { $crate::__shim::Word };
    (@ty str) => { $crate::__shim::Word };
    (@ty tagged) => { $crate::__shim::Word };
    (@ty bool) => { bool };
    (@ty char) => { u32 };
    (@ret_ty handle) => { i32 };
    (@ret_ty tagged) => { $crate::__shim::Word };
    (@ret_ty bool) => { bool };
    (@arg handle, $w:expr) => { $crate::__shim::Value::Int(i64::from($w)) };
    // An `open` mode, a port, a byte: an `int` known to be a fixnum.
    (@arg int, $w:expr) => { $crate::__shim::Value::Int($w as i64) };
    (@arg str, $w:expr) => { $crate::decode($w) };
    // Any other heap value — a `Vector<int>` datagram — crosses the same way.
    (@arg tagged, $w:expr) => { $crate::decode($w) };
    (@arg bool, $w:expr) => { $crate::__shim::Value::Bool($w) };
    (@arg char, $w:expr) => {
        match char::from_u32($w) {
            Some(c) => $crate::__shim::Value::Char(c),
            None => $crate::fatal("stream shim: argument is not a valid char scalar value"),
        }
    };
    (@ret tagged, $v:expr) => { $crate::encode($v) };
    (@ret handle, $v:expr) => {
        match $v {
            $crate::__shim::Value::Int(n) => n as i32,
            other => $crate::fatal(&$crate::__shim::format!("stream shim: expected a handle, got {:?}", other)),
        }
    };
    (@ret bool, $v:expr) => {
        match $v {
            $crate::__shim::Value::Bool(b) => b,
            other => $crate::fatal(&$crate::__shim::format!("stream shim: expected a bool result, got {:?}", other)),
        }
    };
}

/// The readtable builtins for compiled code — see
/// [`crate::readtable::readtable_builtin`], which is also what the
/// interpreter calls. A character argument is a bare scalar value and a
/// function argument is tagged, per the compiled calling convention;
/// `set-*` answers `()` (the zero word) and `get-*` a tagged `Option`.
///
/// # Safety
///
/// Each word must be in the representation the builtin's signature gives
/// it; a `Heap` must already be registered on this thread.
macro_rules! readtable_shim {
    ($shim:ident, $sym:expr, $name:literal, [$($c:ident),*], [$($f:ident)?], $ret:ident) => {
        #[doc = concat!("`", $name, "` for compiled code.")]
        ///
        /// # Safety
        ///
        /// See [`readtable_shim`].
        #[no_mangle]
        pub unsafe extern "C" fn $shim($($c: u32,)* $($f: Word)?) -> Word {
            let mut decoded: Vec<Value> = Vec::new();
            $(
                match char::from_u32($c) {
                    Some(c) => decoded.push(Value::Char(c)),
                    None => fatal(concat!(stringify!($shim), ": argument is not a valid char scalar value")),
                }
            )*
            $(decoded.push(decode($f));)?
            match crate::readtable::readtable_builtin(active_heap(), typelisp_mem::symbols::well_known_symbol($sym), &decoded) {
                Some(Ok(v)) => readtable_shim!(@ret $ret, v),
                Some(Err(e)) => fatal(&e),
                None => fatal(concat!(stringify!($shim), ": ", $name, " is not a readtable builtin")),
            }
        }
    };
    (@ret unit, $v:expr) => {{
        let _ = $v;
        0
    }};
    (@ret tagged, $v:expr) => { encode($v) };
}

readtable_shim!(rt_set_macro_character, typelisp_mem::wk::F_SET_MACRO_CHARACTER, "set-macro-character", [c], [f], unit);
readtable_shim!(rt_get_macro_character, typelisp_mem::wk::F_GET_MACRO_CHARACTER, "get-macro-character", [c], [], tagged);
readtable_shim!(rt_set_dispatch_macro_character, typelisp_mem::wk::F_SET_DISPATCH_MACRO_CHARACTER, "set-dispatch-macro-character", [c, sub], [f], unit);
readtable_shim!(rt_get_dispatch_macro_character, typelisp_mem::wk::F_GET_DISPATCH_MACRO_CHARACTER, "get-dispatch-macro-character", [c, sub], [], tagged);

stream_shim!(rt_stream_stdin, typelisp_mem::wk::F_STREAM_STDIN, "stream-stdin", [], handle, op crate::stream_builtin::stdin);
stream_shim!(rt_stream_stdout, typelisp_mem::wk::F_STREAM_STDOUT, "stream-stdout", [], handle, op crate::stream_builtin::stdout);
stream_shim!(rt_stream_stderr, typelisp_mem::wk::F_STREAM_STDERR, "stream-stderr", [], handle, op crate::stream_builtin::stderr);
stream_shim!(rt_stream_string_input, typelisp_mem::wk::F_STREAM_STRING_INPUT, "stream-string-input", [a: str], handle, op crate::stream_builtin::string_input);
stream_shim!(rt_stream_string_output, typelisp_mem::wk::F_STREAM_STRING_OUTPUT, "stream-string-output", [], handle, op crate::stream_builtin::string_output);
stream_shim!(rt_stream_close, typelisp_mem::wk::F_STREAM_CLOSE, "stream-close", [a: handle], tagged, op crate::stream_builtin::close);
stream_shim!(rt_stream_open_p, typelisp_mem::wk::F_STREAM_OPEN_P, "stream-open-p", [a: handle], bool, op crate::stream_builtin::open_p);
stream_shim!(rt_stream_describe, typelisp_mem::wk::F_STREAM_DESCRIBE, "stream-describe", [a: handle], tagged, op crate::stream_builtin::describe);
stream_shim!(rt_stream_input_p, typelisp_mem::wk::F_STREAM_INPUT_P, "stream-input-p", [a: handle], tagged, op crate::stream_builtin::input_p);
stream_shim!(rt_stream_output_p, typelisp_mem::wk::F_STREAM_OUTPUT_P, "stream-output-p", [a: handle], tagged, op crate::stream_builtin::output_p);
stream_shim!(rt_stream_read_char, typelisp_mem::wk::F_STREAM_READ_CHAR, "stream-read-char", [a: handle], tagged, op crate::stream_builtin::read_char);
stream_shim!(rt_stream_read_byte, typelisp_mem::wk::F_STREAM_READ_BYTE, "stream-read-byte", [a: handle], tagged, op crate::stream_builtin::read_byte);
stream_shim!(rt_stream_write_byte, typelisp_mem::wk::F_STREAM_WRITE_BYTE, "stream-write-byte", [a: handle, b: int], tagged, op crate::stream_builtin::write_byte);
stream_shim!(rt_stream_unread_char, typelisp_mem::wk::F_STREAM_UNREAD_CHAR, "stream-unread-char", [a: handle, b: char], tagged, op crate::stream_builtin::unread_char);
stream_shim!(rt_stream_listen, typelisp_mem::wk::F_STREAM_LISTEN, "stream-listen", [a: handle], tagged, op crate::stream_builtin::listen);
stream_shim!(rt_stream_position, typelisp_mem::wk::F_STREAM_POSITION, "stream-position", [a: handle], tagged, op crate::stream_builtin::position);
stream_shim!(rt_stream_write_string, typelisp_mem::wk::F_STREAM_WRITE_STRING, "stream-write-string", [a: handle, b: str], tagged, op crate::stream_builtin::write_string);
stream_shim!(rt_stream_at_line_start, typelisp_mem::wk::F_STREAM_AT_LINE_START, "stream-at-line-start", [a: handle], tagged, op crate::stream_builtin::at_line_start);
stream_shim!(rt_stream_finish_output, typelisp_mem::wk::F_STREAM_FINISH_OUTPUT, "stream-finish-output", [a: handle], tagged, op crate::stream_builtin::finish_output);
stream_shim!(rt_stream_take_output_string, typelisp_mem::wk::F_STREAM_TAKE_OUTPUT_STRING, "stream-take-output-string", [a: handle], tagged, op crate::stream_builtin::take_output_string);
