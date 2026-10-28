//! The `net-*` builtins as *values*: [`crate::stream_builtin`]'s shape, over
//! [`crate::net`].
//!
//! One implementation for both sides of the compile boundary, for the reason
//! that module gives — and one more here: every one of these is a step of a
//! loop the prelude writes (`try; if not yet, wait; again`), and an
//! interpreted turn of that loop and a compiled one must see the same
//! answers or the loop would behave differently depending on which side
//! took the turn.
//!
//! `net-wait` is **not** here. It is the one `net-` name that does not answer
//! with a value: it puts the task down, and only the scheduler can do that.
//! Interpreted it is intercepted before the builtins are reached
//! (`Interp::finish_call`, next to `sleep`); compiled it lowers to
//! `rt_suspend_io` (`crate::coroutine`), whose `rt_suspend_` prefix is what
//! makes the bridge emit a `(suspend ...)` node instead of a call. Its
//! signature still lives in the registry with the others, since the checker
//! needs it.

use typelisp_mem::{spec, wk, Heap, StdModule, SymRef, TypeId, TypeSpec, Value};

use crate::stream::with_streams;
use crate::stream_builtin::{int, lookup, text, ArgError};

const NET_ERROR: TypeSpec = TypeSpec::Named(StdModule::NET, "NetError", &[]);

/// The runtime type each builtin's *result* carries — [`crate::
/// stream_builtin::RESULT_TYPES`]'s companion, checked against the registry
/// by the same guard test and for the same reason.
pub const RESULT_TYPES: &[(u32, TypeSpec)] = &[
    (wk::F_NET_RESOLVE_BEGIN, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    (wk::F_NET_RESOLVE_FINISH, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[TypeSpec::Named(StdModule::VEC, "Vector", &[spec::STRING])]), NET_ERROR])),
    (wk::F_NET_CONNECT_BEGIN, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    (wk::F_NET_CONNECT_FINISH, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    (wk::F_NET_LISTEN, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    (wk::F_NET_TLS_LISTEN, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    (wk::F_NET_UNIX_CONNECT, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    (wk::F_NET_UNIX_LISTEN, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    (wk::F_NET_ACCEPT, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::I32]), NET_ERROR])),
    (wk::F_NET_FILL, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT]), NET_ERROR])),
    (wk::F_NET_POP_BYTE, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT]), NET_ERROR])),
    (wk::F_NET_POP_CHAR, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::CHAR]), NET_ERROR])),
    (wk::F_NET_BUFFERED_P, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::BOOL, NET_ERROR])),
    (wk::F_NET_PUSH_STRING, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    (wk::F_NET_PUSH_BYTE, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    (wk::F_NET_FLUSH, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT]), NET_ERROR])),
    (wk::F_NET_SOCKET_ERROR, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING]), NET_ERROR])),
    (wk::F_NET_PEER_SUBJECT, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING]), NET_ERROR])),
    (wk::F_NET_SERVER_NAME, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING]), NET_ERROR])),
    (wk::F_NET_SET_NODELAY, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    (wk::F_NET_SET_KEEPALIVE, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    (wk::F_NET_SET_KEEPALIVE_PERIOD, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    (wk::F_NET_TLS_ADD_CERTIFICATE, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    (wk::F_NET_SHUTDOWN_WRITE, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    (wk::F_NET_LOCAL_ADDRESS, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::STRING, NET_ERROR])),
    (wk::F_NET_PEER_ADDRESS, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::STRING, NET_ERROR])),
    (wk::F_NET_TLS_START, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    (wk::F_NET_TLS_HANDSHAKE, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT]), NET_ERROR])),
    (wk::F_NET_UDP_BIND, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    (wk::F_NET_UDP_SEND_TO, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::BOOL, NET_ERROR])),
    (wk::F_NET_UDP_RECV, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[TypeSpec::Named(StdModule::VEC, "Vector", &[spec::INT])]), NET_ERROR])),
    (wk::F_NET_UDP_LAST_SENDER, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::STRING, NET_ERROR])),
];

/// The type of the value inside the `Result`, for the builtins whose payload
/// is itself a box.
pub const INNER_TYPES: &[(u32, TypeSpec)] = &[
    (wk::F_NET_RESOLVE_FINISH, TypeSpec::Named(StdModule::OPTION, "Option", &[TypeSpec::Named(StdModule::VEC, "Vector", &[spec::STRING])])),
    (wk::F_NET_TLS_HANDSHAKE, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT])),
    (wk::F_NET_UDP_RECV, TypeSpec::Named(StdModule::OPTION, "Option", &[TypeSpec::Named(StdModule::VEC, "Vector", &[spec::INT])])),
    (wk::F_NET_ACCEPT, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::I32])),
    (wk::F_NET_FILL, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT])),
    (wk::F_NET_FLUSH, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT])),
    (wk::F_NET_SOCKET_ERROR, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
    (wk::F_NET_PEER_SUBJECT, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
    (wk::F_NET_SERVER_NAME, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
    (wk::F_NET_POP_BYTE, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT])),
    (wk::F_NET_POP_CHAR, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::CHAR])),
];

/// A `bool` argument.
fn flag(args: &[Value], i: usize, who: &str) -> Result<bool, ArgError> {
    match args.get(i) {
        Some(Value::Bool(b)) => Ok(*b),
        other => Err(format!("{}: argument {} is not a bool, got {:?}", who, i, other)),
    }
}

/// An `Option<string>` argument. The type niches (`option.rs`): `none` is
/// the empty word, `some` the string itself.
fn opt_text(heap: &Heap, args: &[Value], i: usize, who: &str) -> Result<Option<String>, ArgError> {
    match args.get(i) {
        Some(Value::Empty) => Ok(None),
        _ => text(heap, args, i, who).map(Some),
    }
}

fn result_ok(heap: &mut Heap, name: SymRef, v: Value) -> Value {
    let key = lookup(RESULT_TYPES, name, "result");
    heap.alloc_enum(key, 0, vec![v])
}

/// `Err(NetError(msg))`. `NetError` has one variant (index 0) carrying the
/// message, the shape every built-in error type has.
fn result_err(heap: &mut Heap, name: SymRef, msg: String) -> Value {
    let msg_val = heap.alloc_string(msg);
    let err_val = heap.alloc_enum(TypeId::NET_ERROR, 0, vec![msg_val]);
    let key = lookup(RESULT_TYPES, name, "result");
    heap.alloc_enum(key, 1, vec![err_val])
}

fn option_value(heap: &mut Heap, name: SymRef, v: Option<Value>) -> Value {
    heap.alloc_option(lookup(INNER_TYPES, name, "inner"), v)
}

/// The elements of a `Vector<int>` argument, as bytes. A `Vector<T>`'s box
/// is a struct whose fields are its elements (`value.rs`'s `BoxedObj`), so
/// this is a walk over the fields; an element outside `0..255` is the
/// caller's error, reported by name.
fn bytes(heap: &Heap, args: &[Value], i: usize, who: &str) -> Result<Result<Vec<u8>, String>, ArgError> {
    let id = match args.get(i) {
        Some(Value::Boxed(id)) => *id,
        other => return Err(format!("{}: argument {} is not a vector, got {:?}", who, i, other)),
    };
    let n = heap.struct_field_count(id);
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        match heap.struct_field(id, k) {
            Value::Int(b) => match u8::try_from(b) {
                Ok(b) => out.push(b),
                Err(_) => return Ok(Err(format!("{}: {} is not a byte (0..255)", who, b))),
            },
            other => return Err(format!("{}: element {} is not an integer, got {:?}", who, k, other)),
        }
    }
    Ok(Ok(out))
}

/// Every `net-*` builtin that answers with a value, or `None` if `name`
/// isn't one. The `Ok`/`Err` of the return is about the arguments' shape,
/// exactly as in [`crate::stream_builtin::stream_builtin`].
pub fn net_builtin(heap: &mut Heap, sym: SymRef, args: &[Value]) -> Option<Result<Value, ArgError>> {
    let name = sym.name();
    macro_rules! wrap {
        ($e:expr, $ok:expr) => {
            match $e {
                Ok(v) => Ok(result_ok(heap, sym, $ok(v))),
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        };
    }
    /// A result whose payload is an `Option` box.
    macro_rules! wrap_option {
        ($e:expr, $ok:expr) => {
            match $e {
                Ok(v) => {
                    let inner = option_value(heap, sym, v.map($ok));
                    Ok(result_ok(heap, sym, inner))
                }
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        };
    }
    macro_rules! arg {
        ($e:expr) => {
            match $e {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            }
        };
    }

    Some(match sym.well_known() {
        wk::F_NET_RESOLVE_BEGIN => {
            let (host, port) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_resolve_begin(&host, port)), |v: i64| Value::Int(v))
        }
        // The addresses as a `Vector<string>` box — `file-list-directory`'s
        // shape, safe for its reason: nothing between the element
        // allocations and the box can collect.
        wk::F_NET_RESOLVE_FINISH => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.net_resolve_finish(h)) {
                Ok(Some(all)) => {
                    let elems: Vec<Value> = all.into_iter().map(|a| heap.alloc_string(a)).collect();
                    let key = typelisp_mem::intern_spec(&TypeSpec::Named(StdModule::VEC, "Vector", &[spec::STRING]));
                    let vec_val = heap.alloc_struct(key, elems);
                    let ov = option_value(heap, sym, Some(vec_val));
                    Ok(result_ok(heap, sym, ov))
                }
                Ok(None) => {
                    let ov = option_value(heap, sym, None);
                    Ok(result_ok(heap, sym, ov))
                }
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        }
        wk::F_NET_CONNECT_BEGIN => {
            let addr = arg!(text(heap, args, 0, name));
            wrap!(with_streams(heap, |t| t.net_connect_begin(&addr)), |v: i64| Value::Int(v))
        }
        wk::F_NET_TLS_START => {
            let (h, host) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            let ca = arg!(opt_text(heap, args, 2, name));
            let (cert, key) = (arg!(opt_text(heap, args, 3, name)), arg!(opt_text(heap, args, 4, name)));
            wrap!(
                with_streams(heap, |t| t.net_tls_start(h, &host, ca.as_deref(), cert.as_deref(), key.as_deref())),
                |_v: ()| Value::Empty
            )
        }
        wk::F_NET_TLS_LISTEN => {
            let (host, port) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            let (cert, key) = (arg!(text(heap, args, 2, name)), arg!(text(heap, args, 3, name)));
            let client_ca = arg!(opt_text(heap, args, 4, name));
            wrap!(with_streams(heap, |t| t.net_tls_listen(&host, port, &cert, &key, client_ca.as_deref())), |v: i64| Value::Int(v))
        }
        // The text as a string box inside the `Option`: allocated first,
        // then boxed, with nothing that can collect in between.
        wk::F_NET_SOCKET_ERROR | wk::F_NET_PEER_SUBJECT | wk::F_NET_SERVER_NAME => {
            let h = arg!(int(args, 0, name));
            let answer = with_streams(heap, |t| match sym.well_known() {
                wk::F_NET_SOCKET_ERROR => t.net_socket_error(h),
                wk::F_NET_PEER_SUBJECT => t.net_peer_subject(h),
                _ => t.net_server_name(h),
            });
            match answer {
                Ok(what) => {
                    let inner = what.map(|m| heap.alloc_string(m));
                    let ov = option_value(heap, sym, inner);
                    Ok(result_ok(heap, sym, ov))
                }
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        }
        wk::F_NET_SET_NODELAY => {
            let (h, on) = (arg!(int(args, 0, name)), arg!(flag(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_set_nodelay(h, on)), |_v: ()| Value::Empty)
        }
        wk::F_NET_SET_KEEPALIVE => {
            let (h, on) = (arg!(int(args, 0, name)), arg!(flag(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_set_keepalive(h, on)), |_v: ()| Value::Empty)
        }
        wk::F_NET_SET_KEEPALIVE_PERIOD => {
            let (h, secs) = (arg!(int(args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_set_keepalive_period(h, secs)), |_v: ()| Value::Empty)
        }
        wk::F_NET_TLS_ADD_CERTIFICATE => {
            let (h, host) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            let (cert, key) = (arg!(text(heap, args, 2, name)), arg!(text(heap, args, 3, name)));
            wrap!(with_streams(heap, |t| t.net_tls_add_certificate(h, &host, &cert, &key)), |_v: ()| Value::Empty)
        }
        wk::F_NET_TLS_HANDSHAKE => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_tls_handshake(h)), Value::Int)
        }
        wk::F_NET_UDP_BIND => {
            let (host, port) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_udp_bind(&host, port)), |v: i64| Value::Int(v))
        }
        wk::F_NET_UDP_SEND_TO => {
            let (h, addr) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            match arg!(bytes(heap, args, 2, name)) {
                Ok(data) => wrap!(with_streams(heap, |t| t.net_udp_send_to(h, &addr, &data)), |v: bool| Value::Bool(v)),
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        }
        // The datagram as a `Vector<int>` box. Allocating every element
        // before the box is safe for `file-list-directory`'s reason:
        // `Value::Int` allocates nothing and `alloc_struct` never collects.
        wk::F_NET_UDP_RECV => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.net_udp_recv(h)) {
                Ok(Some(data)) => {
                    let elems: Vec<Value> = data.into_iter().map(|b| Value::Int(b as i64)).collect();
                    let key = typelisp_mem::intern_spec(&TypeSpec::Named(StdModule::VEC, "Vector", &[spec::INT]));
                    let vec_val = heap.alloc_struct(key, elems);
                    let ov = option_value(heap, sym, Some(vec_val));
                    Ok(result_ok(heap, sym, ov))
                }
                Ok(None) => {
                    let ov = option_value(heap, sym, None);
                    Ok(result_ok(heap, sym, ov))
                }
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        }
        wk::F_NET_UDP_LAST_SENDER => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.net_udp_last_sender(h)) {
                Ok(s) => {
                    let sv = heap.alloc_string(s);
                    Ok(result_ok(heap, sym, sv))
                }
                Err(m) => Ok(result_err(heap, sym, m)),
            }
        }
        wk::F_NET_CONNECT_FINISH => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.net_connect_finish(h)), |_v: ()| Value::Empty)
        }
        wk::F_NET_LISTEN => {
            let (host, port) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_listen(&host, port)), |v: i64| Value::Int(v))
        }
        wk::F_NET_UNIX_CONNECT => {
            let path = arg!(text(heap, args, 0, name));
            wrap!(with_streams(heap, |t| t.net_unix_connect(&path)), |v: i64| Value::Int(v))
        }
        wk::F_NET_UNIX_LISTEN => {
            let path = arg!(text(heap, args, 0, name));
            wrap!(with_streams(heap, |t| t.net_unix_listen(&path)), |v: i64| Value::Int(v))
        }
        wk::F_NET_ACCEPT => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_accept(h)), Value::Int)
        }
        wk::F_NET_FILL => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_fill(h)), |n: usize| Value::Int(n as i64))
        }
        wk::F_NET_POP_BYTE => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_pop_byte(h)), |b: u8| Value::Int(b as i64))
        }
        wk::F_NET_POP_CHAR => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_pop_char(h)), Value::Char)
        }
        wk::F_NET_BUFFERED_P => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.net_buffered(h)), |v: bool| Value::Bool(v))
        }
        wk::F_NET_PUSH_STRING => {
            let (h, s) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_push_string(h, &s)), |_v: ()| Value::Empty)
        }
        wk::F_NET_PUSH_BYTE => {
            let (h, b) = (arg!(int(args, 0, name)), arg!(int(args, 1, name)));
            match u8::try_from(b) {
                Ok(b) => wrap!(with_streams(heap, |t| t.net_push_byte(h, b)), |_v: ()| Value::Empty),
                Err(_) => Ok(result_err(heap, sym, format!("write-byte: {} is not a byte (0..255)", b))),
            }
        }
        wk::F_NET_FLUSH => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_flush(h)), Value::Int)
        }
        wk::F_NET_SHUTDOWN_WRITE => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.net_shutdown_write(h)), |_v: ()| Value::Empty)
        }
        wk::F_NET_LOCAL_ADDRESS | wk::F_NET_PEER_ADDRESS => {
            let h = arg!(int(args, 0, name));
            let r = if sym.is(wk::F_NET_LOCAL_ADDRESS) {
                with_streams(heap, |t| t.net_local_address(h))
            } else {
                with_streams(heap, |t| t.net_peer_address(h))
            };
            match r {
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
