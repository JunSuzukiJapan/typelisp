//! Compiled-code shims for the stream and readtable builtins.

use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, encode, fatal, tagged_arg};
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
    ($shim:ident, $sym:expr, $name:literal, [$($arg:ident),*], $ret:ident, via $dispatch:path) => {
        /// A builtin for compiled code — see the dispatcher it calls, which
        /// is also what the interpreter calls.
        ///
        /// # Safety
        ///
        /// `args` must point to at least as many valid `i64`s as this
        /// builtin has parameters, each in the representation its signature
        /// gives it; a `Heap` must already be registered on this thread.
        #[no_mangle]
        pub unsafe extern "C" fn $shim(args: *const i64, argc: u32) -> i64 {
            // A nullary builtin reads neither, and `decoded` stays empty.
            let _ = (args, argc);
            #[allow(unused_mut)]
            let mut decoded: $crate::__shim::Vec<$crate::__shim::Value> = $crate::__shim::Vec::new();
            $(
                let i = decoded.len();
                if argc as usize <= i {
                    $crate::fatal(concat!(stringify!($shim), ": too few arguments"));
                }
                decoded.push($crate::stream_shim!(@arg $arg, args, i));
            )*
            match $dispatch($crate::active_heap(), $crate::__shim::well_known_symbol($sym), &decoded) {
                Some(Ok(v)) => $crate::stream_shim!(@ret $ret, v),
                Some(Err(e)) => $crate::fatal(&e),
                None => $crate::fatal(concat!(stringify!($shim), ": ", $name, " is not a builtin of its dispatcher")),
            }
        }
    };
    // A stream handle and an `open` mode are both bare `i64`s.
    (@arg int, $args:expr, $i:expr) => { $crate::__shim::Value::Int(*$args.add($i)) };
    (@arg str, $args:expr, $i:expr) => { $crate::tagged_arg($args, $i) };
    // Any other heap value — a `Vector<int>` datagram — crosses the same way.
    (@arg tagged, $args:expr, $i:expr) => { $crate::tagged_arg($args, $i) };
    // A `bool` is a bare 0/1 word, as `@ret bool` writes it.
    (@arg bool, $args:expr, $i:expr) => { $crate::__shim::Value::Bool(*$args.add($i) != 0) };
    (@arg char, $args:expr, $i:expr) => {
        match char::from_u32(*$args.add($i) as u32) {
            Some(c) => $crate::__shim::Value::Char(c),
            None => $crate::fatal("stream shim: argument is not a valid char scalar value"),
        }
    };
    (@ret tagged, $v:expr) => { $crate::encode($v) };
    (@ret raw, $v:expr) => {
        match $v {
            $crate::__shim::Value::Int(n) => n,
            other => $crate::fatal(&$crate::__shim::format!("stream shim: expected an i64 result, got {:?}", other)),
        }
    };
    (@ret bool, $v:expr) => {
        match $v {
            $crate::__shim::Value::Bool(b) => i64::from(b),
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
/// `args` must point to at least as many valid `i64`s as the builtin has
/// parameters, each in the representation its signature gives it; a `Heap`
/// must already be registered on this thread.
macro_rules! readtable_shim {
    ($shim:ident, $sym:expr, $name:literal, $chars:literal, $fn_arg:literal, $ret:ident) => {
        #[doc = concat!("`", $name, "` for compiled code.")]
        ///
        /// # Safety
        ///
        /// See [`readtable_shim`].
        #[no_mangle]
        pub unsafe extern "C" fn $shim(args: *const i64, argc: u32) -> i64 {
            let want = $chars + if $fn_arg { 1 } else { 0 };
            if (argc as usize) < want {
                fatal(concat!(stringify!($shim), ": too few arguments"));
            }
            let mut decoded: Vec<Value> = Vec::new();
            for i in 0..$chars {
                match char::from_u32(*args.add(i) as u32) {
                    Some(c) => decoded.push(Value::Char(c)),
                    None => fatal(concat!(stringify!($shim), ": argument is not a valid char scalar value")),
                }
            }
            if $fn_arg {
                decoded.push(tagged_arg(args, $chars));
            }
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

readtable_shim!(rt_set_macro_character, typelisp_mem::wk::F_SET_MACRO_CHARACTER, "set-macro-character", 1, true, unit);
readtable_shim!(rt_get_macro_character, typelisp_mem::wk::F_GET_MACRO_CHARACTER, "get-macro-character", 1, false, tagged);
readtable_shim!(rt_set_dispatch_macro_character, typelisp_mem::wk::F_SET_DISPATCH_MACRO_CHARACTER, "set-dispatch-macro-character", 2, true, unit);
readtable_shim!(rt_get_dispatch_macro_character, typelisp_mem::wk::F_GET_DISPATCH_MACRO_CHARACTER, "get-dispatch-macro-character", 2, false, tagged);

stream_shim!(rt_stream_stdin, typelisp_mem::wk::F_STREAM_STDIN, "stream-stdin", [], raw, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_stdout, typelisp_mem::wk::F_STREAM_STDOUT, "stream-stdout", [], raw, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_stderr, typelisp_mem::wk::F_STREAM_STDERR, "stream-stderr", [], raw, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_string_input, typelisp_mem::wk::F_STREAM_STRING_INPUT, "stream-string-input", [str], raw, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_string_output, typelisp_mem::wk::F_STREAM_STRING_OUTPUT, "stream-string-output", [], raw, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_close, typelisp_mem::wk::F_STREAM_CLOSE, "stream-close", [int], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_open_p, typelisp_mem::wk::F_STREAM_OPEN_P, "stream-open-p", [int], bool, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_describe, typelisp_mem::wk::F_STREAM_DESCRIBE, "stream-describe", [int], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_input_p, typelisp_mem::wk::F_STREAM_INPUT_P, "stream-input-p", [int], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_output_p, typelisp_mem::wk::F_STREAM_OUTPUT_P, "stream-output-p", [int], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_read_char, typelisp_mem::wk::F_STREAM_READ_CHAR, "stream-read-char", [int], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_read_byte, typelisp_mem::wk::F_STREAM_READ_BYTE, "stream-read-byte", [int], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_write_byte, typelisp_mem::wk::F_STREAM_WRITE_BYTE, "stream-write-byte", [int, int], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_unread_char, typelisp_mem::wk::F_STREAM_UNREAD_CHAR, "stream-unread-char", [int, char], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_listen, typelisp_mem::wk::F_STREAM_LISTEN, "stream-listen", [int], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_position, typelisp_mem::wk::F_STREAM_POSITION, "stream-position", [int], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_write_string, typelisp_mem::wk::F_STREAM_WRITE_STRING, "stream-write-string", [int, str], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_at_line_start, typelisp_mem::wk::F_STREAM_AT_LINE_START, "stream-at-line-start", [int], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_finish_output, typelisp_mem::wk::F_STREAM_FINISH_OUTPUT, "stream-finish-output", [int], tagged, via crate::stream_builtin::stream_builtin);
stream_shim!(rt_stream_take_output_string, typelisp_mem::wk::F_STREAM_TAKE_OUTPUT_STRING, "stream-take-output-string", [int], tagged, via crate::stream_builtin::stream_builtin);
