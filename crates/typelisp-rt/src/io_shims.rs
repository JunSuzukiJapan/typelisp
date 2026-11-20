//! Compiled-code shims for the stream, file, readtable and socket builtins.

use typelisp_abi::{active_heap, encode, fatal, tagged_arg};
use typelisp_mem::Value;

// ---- stream/file builtins ----------------------------------------------
//
// One shim per builtin (`crate::stream_builtin::stream_builtin` is the
// implementation both these and the interpreter call). What each shim adds is
// the *representation* conversion the compiled ABI needs and the interpreter
// does not: an `i64` handle, an `i32` mode, a `bool` and a `char` are bare
// machine words in compiled code, while a `string` and every `Result` are
// tagged heap values. Which is which comes from the builtin's own signature
// (`registry::register_stream_builtins`), so it is spelled out per shim
// rather than guessed from the value.


/// Generates one `rt_stream_*`/`rt_file_*` shim.
///
/// `$arg` names how each parameter arrives (`int`/`str`/`char`), `$ret` how
/// the result leaves (`tagged` for a heap value, `raw` for a bare `i64`,
/// `bool` for a bare `0`/`1`).
macro_rules! stream_shim {
    // The short form: a `stream-*`/`file-*` builtin. The `net-*` family
    // names its own dispatcher with `via` below — same conversions, a
    // different table.
    ($shim:ident, $sym:expr, $name:literal, [$($arg:ident),*], $ret:ident) => {
        stream_shim!($shim, $sym, $name, [$($arg),*], $ret, via crate::stream_builtin::stream_builtin);
    };
    ($shim:ident, $sym:expr, $name:literal, [$($arg:ident),*], $ret:ident, via $dispatch:path) => {
        /// A `stream-*`/`file-*` builtin for compiled code — see
        /// [`crate::stream_builtin::stream_builtin`], which is also what the
        /// interpreter calls.
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
            let mut decoded: Vec<Value> = Vec::new();
            $(
                let i = decoded.len();
                if argc as usize <= i {
                    fatal(concat!(stringify!($shim), ": too few arguments"));
                }
                decoded.push(stream_shim!(@arg $arg, args, i));
            )*
            match $dispatch(active_heap(), typelisp_mem::symbols::well_known_symbol($sym), &decoded) {
                Some(Ok(v)) => stream_shim!(@ret $ret, v),
                Some(Err(e)) => fatal(&e),
                None => fatal(concat!(stringify!($shim), ": ", $name, " is not a builtin of its dispatcher")),
            }
        }
    };
    // A stream handle and an `open` mode are both bare `i64`s.
    (@arg int, $args:expr, $i:expr) => { Value::Int(*$args.add($i)) };
    (@arg str, $args:expr, $i:expr) => { tagged_arg($args, $i) };
    // Any other heap value — a `Vector<int>` datagram — crosses the same way.
    (@arg tagged, $args:expr, $i:expr) => { tagged_arg($args, $i) };
    // A `bool` is a bare 0/1 word, as `@ret bool` writes it.
    (@arg bool, $args:expr, $i:expr) => { Value::Bool(*$args.add($i) != 0) };
    (@arg char, $args:expr, $i:expr) => {
        match char::from_u32(*$args.add($i) as u32) {
            Some(c) => Value::Char(c),
            None => fatal("stream shim: argument is not a valid char scalar value"),
        }
    };
    (@ret tagged, $v:expr) => { encode($v) };
    (@ret raw, $v:expr) => {
        match $v {
            Value::Int(n) => n,
            other => fatal(&format!("stream shim: expected an i64 result, got {:?}", other)),
        }
    };
    (@ret bool, $v:expr) => {
        match $v {
            Value::Bool(b) => i64::from(b),
            other => fatal(&format!("stream shim: expected a bool result, got {:?}", other)),
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

stream_shim!(rt_stream_stdin, typelisp_mem::wk::F_STREAM_STDIN, "stream-stdin", [], raw);
stream_shim!(rt_stream_stdout, typelisp_mem::wk::F_STREAM_STDOUT, "stream-stdout", [], raw);
stream_shim!(rt_stream_stderr, typelisp_mem::wk::F_STREAM_STDERR, "stream-stderr", [], raw);
stream_shim!(rt_stream_string_input, typelisp_mem::wk::F_STREAM_STRING_INPUT, "stream-string-input", [str], raw);
stream_shim!(rt_stream_string_output, typelisp_mem::wk::F_STREAM_STRING_OUTPUT, "stream-string-output", [], raw);
stream_shim!(rt_stream_open_file, typelisp_mem::wk::F_STREAM_OPEN_FILE, "stream-open-file", [str, int], tagged);
stream_shim!(rt_stream_close, typelisp_mem::wk::F_STREAM_CLOSE, "stream-close", [int], tagged);
stream_shim!(rt_stream_open_p, typelisp_mem::wk::F_STREAM_OPEN_P, "stream-open-p", [int], bool);
stream_shim!(rt_stream_describe, typelisp_mem::wk::F_STREAM_DESCRIBE, "stream-describe", [int], tagged);
stream_shim!(rt_stream_input_p, typelisp_mem::wk::F_STREAM_INPUT_P, "stream-input-p", [int], tagged);
stream_shim!(rt_stream_output_p, typelisp_mem::wk::F_STREAM_OUTPUT_P, "stream-output-p", [int], tagged);
stream_shim!(rt_stream_read_char, typelisp_mem::wk::F_STREAM_READ_CHAR, "stream-read-char", [int], tagged);
stream_shim!(rt_stream_read_byte, typelisp_mem::wk::F_STREAM_READ_BYTE, "stream-read-byte", [int], tagged);
stream_shim!(rt_stream_write_byte, typelisp_mem::wk::F_STREAM_WRITE_BYTE, "stream-write-byte", [int, int], tagged);
stream_shim!(rt_stream_unread_char, typelisp_mem::wk::F_STREAM_UNREAD_CHAR, "stream-unread-char", [int, char], tagged);
stream_shim!(rt_stream_listen, typelisp_mem::wk::F_STREAM_LISTEN, "stream-listen", [int], tagged);
stream_shim!(rt_stream_position, typelisp_mem::wk::F_STREAM_POSITION, "stream-position", [int], tagged);
stream_shim!(rt_stream_write_string, typelisp_mem::wk::F_STREAM_WRITE_STRING, "stream-write-string", [int, str], tagged);
stream_shim!(rt_stream_at_line_start, typelisp_mem::wk::F_STREAM_AT_LINE_START, "stream-at-line-start", [int], tagged);
stream_shim!(rt_stream_finish_output, typelisp_mem::wk::F_STREAM_FINISH_OUTPUT, "stream-finish-output", [int], tagged);
stream_shim!(rt_stream_take_output_string, typelisp_mem::wk::F_STREAM_TAKE_OUTPUT_STRING, "stream-take-output-string", [int], tagged);
stream_shim!(rt_file_exists_p, typelisp_mem::wk::F_FILE_EXISTS_P, "file-exists-p", [str], bool);
stream_shim!(rt_file_delete, typelisp_mem::wk::F_FILE_DELETE, "file-delete", [str], tagged);
stream_shim!(rt_file_rename, typelisp_mem::wk::F_FILE_RENAME, "file-rename", [str, str], tagged);
stream_shim!(rt_file_truename, typelisp_mem::wk::F_FILE_TRUENAME, "file-truename", [str], tagged);
stream_shim!(rt_file_modified_date, typelisp_mem::wk::F_FILE_MODIFIED_DATE, "file-modified-date", [str], tagged);
stream_shim!(rt_file_owner_name, typelisp_mem::wk::F_FILE_OWNER_NAME, "file-owner-name", [str], tagged);
stream_shim!(rt_file_directory_p, typelisp_mem::wk::F_FILE_DIRECTORY_P, "file-directory-p", [str], bool);
stream_shim!(rt_file_list_directory, typelisp_mem::wk::F_FILE_LIST_DIRECTORY, "file-list-directory", [str], tagged);
stream_shim!(rt_file_create_directories, typelisp_mem::wk::F_FILE_CREATE_DIRECTORIES, "file-create-directories", [str], tagged);

// The sockets: `crate::net_builtin::net_builtin` is the implementation both
// these and the interpreter call. `net-wait` has no shim here — it suspends,
// and `crate::coroutine::rt_suspend_io` is its compiled form.
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_resolve_begin, typelisp_mem::wk::F_NET_RESOLVE_BEGIN, "net-resolve-begin", [str, int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_resolve_finish, typelisp_mem::wk::F_NET_RESOLVE_FINISH, "net-resolve-finish", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_connect_begin, typelisp_mem::wk::F_NET_CONNECT_BEGIN, "net-connect-begin", [str], tagged, via crate::net_builtin::net_builtin);
// The `Option<string>` arguments niche (`typelisp_mem::option`): `None` is
// the empty word and `Some` the string, so they cross as any tagged word.
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_tls_start, typelisp_mem::wk::F_NET_TLS_START, "net-tls-start", [int, str, tagged, tagged, tagged], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_tls_listen, typelisp_mem::wk::F_NET_TLS_LISTEN, "net-tls-listen", [str, int, str, str, tagged], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_socket_error, typelisp_mem::wk::F_NET_SOCKET_ERROR, "net-socket-error", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_peer_subject, typelisp_mem::wk::F_NET_PEER_SUBJECT, "net-peer-subject", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_server_name, typelisp_mem::wk::F_NET_SERVER_NAME, "net-server-name", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_set_nodelay, typelisp_mem::wk::F_NET_SET_NODELAY, "net-set-nodelay", [int, bool], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_set_keepalive, typelisp_mem::wk::F_NET_SET_KEEPALIVE, "net-set-keepalive", [int, bool], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_set_keepalive_period, typelisp_mem::wk::F_NET_SET_KEEPALIVE_PERIOD, "net-set-keepalive-period", [int, int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_tls_add_certificate, typelisp_mem::wk::F_NET_TLS_ADD_CERTIFICATE, "net-tls-add-certificate", [int, str, str, str], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_tls_handshake, typelisp_mem::wk::F_NET_TLS_HANDSHAKE, "net-tls-handshake", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_udp_bind, typelisp_mem::wk::F_NET_UDP_BIND, "net-udp-bind", [str, int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_udp_send_to, typelisp_mem::wk::F_NET_UDP_SEND_TO, "net-udp-send-to", [int, str, tagged], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_udp_recv, typelisp_mem::wk::F_NET_UDP_RECV, "net-udp-recv", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_udp_last_sender, typelisp_mem::wk::F_NET_UDP_LAST_SENDER, "net-udp-last-sender", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_connect_finish, typelisp_mem::wk::F_NET_CONNECT_FINISH, "net-connect-finish", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_listen, typelisp_mem::wk::F_NET_LISTEN, "net-listen", [str, int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_unix_connect, typelisp_mem::wk::F_NET_UNIX_CONNECT, "net-unix-connect", [str], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_unix_listen, typelisp_mem::wk::F_NET_UNIX_LISTEN, "net-unix-listen", [str], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_accept, typelisp_mem::wk::F_NET_ACCEPT, "net-accept", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_fill, typelisp_mem::wk::F_NET_FILL, "net-fill", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_pop_byte, typelisp_mem::wk::F_NET_POP_BYTE, "net-pop-byte", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_pop_char, typelisp_mem::wk::F_NET_POP_CHAR, "net-pop-char", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_buffered_p, typelisp_mem::wk::F_NET_BUFFERED_P, "net-buffered-p", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_push_string, typelisp_mem::wk::F_NET_PUSH_STRING, "net-push-string", [int, str], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_push_byte, typelisp_mem::wk::F_NET_PUSH_BYTE, "net-push-byte", [int, int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_flush, typelisp_mem::wk::F_NET_FLUSH, "net-flush", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_shutdown_write, typelisp_mem::wk::F_NET_SHUTDOWN_WRITE, "net-shutdown-write", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_local_address, typelisp_mem::wk::F_NET_LOCAL_ADDRESS, "net-local-address", [int], tagged, via crate::net_builtin::net_builtin);
#[cfg(not(target_family = "wasm"))]
stream_shim!(rt_net_peer_address, typelisp_mem::wk::F_NET_PEER_ADDRESS, "net-peer-address", [int], tagged, via crate::net_builtin::net_builtin);
