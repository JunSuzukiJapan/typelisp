//! The compiled-code shims of this crate's builtins: the sockets, and the two
//! suspensions only a POSIX system can answer — waiting on a socket and
//! starting an OS thread. A suspending shim records what the task is waiting
//! for; the scheduler answers it (`typelisp_rt::sched`).

// The sockets: `crate::net_builtin::net_builtin` is the implementation both
// these and the interpreter call. `net-wait` suspends, and [`rt_suspend_io`]
// below is its compiled form.
typelisp_rt::stream_shim!(rt_net_resolve_begin, typelisp_mem::wk::F_NET_RESOLVE_BEGIN, "net-resolve-begin", [a: str, b: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_resolve_finish, typelisp_mem::wk::F_NET_RESOLVE_FINISH, "net-resolve-finish", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_connect_begin, typelisp_mem::wk::F_NET_CONNECT_BEGIN, "net-connect-begin", [a: str], tagged, via crate::net_builtin::net_builtin);
// The `Option<string>` arguments niche (`typelisp_mem::option`): `None` is
// the empty word and `Some` the string, so they cross as any tagged word.
typelisp_rt::stream_shim!(rt_net_tls_start, typelisp_mem::wk::F_NET_TLS_START, "net-tls-start", [a: int, b: str, c: tagged, d: tagged, e: tagged], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_tls_listen, typelisp_mem::wk::F_NET_TLS_LISTEN, "net-tls-listen", [a: str, b: int, c: str, d: str, e: tagged], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_socket_error, typelisp_mem::wk::F_NET_SOCKET_ERROR, "net-socket-error", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_peer_subject, typelisp_mem::wk::F_NET_PEER_SUBJECT, "net-peer-subject", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_server_name, typelisp_mem::wk::F_NET_SERVER_NAME, "net-server-name", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_set_nodelay, typelisp_mem::wk::F_NET_SET_NODELAY, "net-set-nodelay", [a: int, b: bool], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_set_keepalive, typelisp_mem::wk::F_NET_SET_KEEPALIVE, "net-set-keepalive", [a: int, b: bool], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_set_keepalive_period, typelisp_mem::wk::F_NET_SET_KEEPALIVE_PERIOD, "net-set-keepalive-period", [a: int, b: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_tls_add_certificate, typelisp_mem::wk::F_NET_TLS_ADD_CERTIFICATE, "net-tls-add-certificate", [a: int, b: str, c: str, d: str], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_tls_handshake, typelisp_mem::wk::F_NET_TLS_HANDSHAKE, "net-tls-handshake", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_udp_bind, typelisp_mem::wk::F_NET_UDP_BIND, "net-udp-bind", [a: str, b: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_udp_send_to, typelisp_mem::wk::F_NET_UDP_SEND_TO, "net-udp-send-to", [a: int, b: str, c: tagged], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_udp_recv, typelisp_mem::wk::F_NET_UDP_RECV, "net-udp-recv", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_udp_last_sender, typelisp_mem::wk::F_NET_UDP_LAST_SENDER, "net-udp-last-sender", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_connect_finish, typelisp_mem::wk::F_NET_CONNECT_FINISH, "net-connect-finish", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_listen, typelisp_mem::wk::F_NET_LISTEN, "net-listen", [a: str, b: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_unix_connect, typelisp_mem::wk::F_NET_UNIX_CONNECT, "net-unix-connect", [a: str], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_unix_listen, typelisp_mem::wk::F_NET_UNIX_LISTEN, "net-unix-listen", [a: str], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_accept, typelisp_mem::wk::F_NET_ACCEPT, "net-accept", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_fill, typelisp_mem::wk::F_NET_FILL, "net-fill", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_pop_byte, typelisp_mem::wk::F_NET_POP_BYTE, "net-pop-byte", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_pop_char, typelisp_mem::wk::F_NET_POP_CHAR, "net-pop-char", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_buffered_p, typelisp_mem::wk::F_NET_BUFFERED_P, "net-buffered-p", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_push_string, typelisp_mem::wk::F_NET_PUSH_STRING, "net-push-string", [a: int, b: str], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_push_byte, typelisp_mem::wk::F_NET_PUSH_BYTE, "net-push-byte", [a: int, b: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_flush, typelisp_mem::wk::F_NET_FLUSH, "net-flush", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_shutdown_write, typelisp_mem::wk::F_NET_SHUTDOWN_WRITE, "net-shutdown-write", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_local_address, typelisp_mem::wk::F_NET_LOCAL_ADDRESS, "net-local-address", [a: int], tagged, via crate::net_builtin::net_builtin);
typelisp_rt::stream_shim!(rt_net_peer_address, typelisp_mem::wk::F_NET_PEER_ADDRESS, "net-peer-address", [a: int], tagged, via crate::net_builtin::net_builtin);

// ---- the suspensions --------------------------------------------------------

/// `(thread ...)` for compiled code: `rt_suspend_task`'s closure, for a task
/// the scheduler runs on an OS thread of its own. `name` is the handle's
/// type (`Thread<int>`). The answer is the `Thread<T>` handle.
///
/// # Safety
///
/// The words must be valid.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_thread(closure: i64, name: i64) -> i64 {
    typelisp_abi::call_state::set_pending_suspend_2(typelisp_abi::call_state::SUSPEND_THREAD, closure, name);
    0
}

/// `(net-wait h interest)` for compiled code: `fd` is the stream
/// handle, `interest` the interest code — both plain integers, which is how
/// they travel. Records the wait; the activation then returns
/// `STATUS_SUSPEND` and the scheduler parks the task on the descriptor.
///
/// # Safety
///
/// Each word must be a value of the type the signature gives it.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_io(fd: i64, interest: i64) -> i64 {
    typelisp_abi::call_state::set_pending_suspend_2(typelisp_abi::call_state::SUSPEND_IO, fd, interest);
    0
}

/// `(net-wait-for h interest secs)` for compiled code: the handle and the
/// interest packed into one word (handle above, interest in the low bit)
/// because the seconds' bits need a whole word of their own.
///
/// # Safety
///
/// Each word must be a value of the type the signature gives it.
#[no_mangle]
pub unsafe extern "C" fn rt_suspend_io_for(packed: i64, interest: i64, deadline: i64) -> i64 {
    let packed = (packed << 1) | (interest & 1);
    typelisp_abi::call_state::set_pending_suspend_2(typelisp_abi::call_state::SUSPEND_IO_FOR, packed, deadline);
    0
}
