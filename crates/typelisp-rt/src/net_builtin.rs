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

use typelisp_mem::{spec, Heap, StdModule, TypeId, TypeSpec, Value};

use crate::stream::with_streams;
use crate::stream_builtin::{int, lookup, text, ArgError};

const NET_ERROR: TypeSpec = TypeSpec::Named(StdModule::STD, "NetError", &[]);

/// The runtime type each builtin's *result* carries — [`crate::
/// stream_builtin::RESULT_TYPES`]'s companion, checked against the registry
/// by the same guard test and for the same reason.
pub const RESULT_TYPES: &[(&str, TypeSpec)] = &[
    ("net-resolve-begin", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    ("net-resolve-finish", TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[TypeSpec::Named(StdModule::VEC, "Vector", &[spec::STRING])]), NET_ERROR])),
    ("net-connect-begin", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    ("net-connect-finish", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    ("net-listen", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    ("net-tls-listen", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    ("net-unix-connect", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    ("net-unix-listen", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    ("net-accept", TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::I32]), NET_ERROR])),
    ("net-fill", TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT]), NET_ERROR])),
    ("net-pop-byte", TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT]), NET_ERROR])),
    ("net-pop-char", TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::CHAR]), NET_ERROR])),
    ("net-buffered-p", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::BOOL, NET_ERROR])),
    ("net-push-string", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    ("net-push-byte", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    ("net-flush", TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT]), NET_ERROR])),
    ("net-socket-error", TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING]), NET_ERROR])),
    ("net-peer-subject", TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING]), NET_ERROR])),
    ("net-server-name", TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING]), NET_ERROR])),
    ("net-set-nodelay", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    ("net-set-keepalive", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    ("net-set-keepalive-period", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    ("net-tls-add-certificate", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    ("net-shutdown-write", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    ("net-local-address", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::STRING, NET_ERROR])),
    ("net-peer-address", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::STRING, NET_ERROR])),
    ("net-tls-start", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, NET_ERROR])),
    ("net-tls-handshake", TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT]), NET_ERROR])),
    ("net-udp-bind", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::I32, NET_ERROR])),
    ("net-udp-send-to", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::BOOL, NET_ERROR])),
    ("net-udp-recv", TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[TypeSpec::Named(StdModule::VEC, "Vector", &[spec::INT])]), NET_ERROR])),
    ("net-udp-last-sender", TypeSpec::Named(StdModule::RESULT, "Result", &[spec::STRING, NET_ERROR])),
];

/// The type of the value inside the `Result`, for the builtins whose payload
/// is itself a box.
pub const INNER_TYPES: &[(&str, TypeSpec)] = &[
    ("net-resolve-finish", TypeSpec::Named(StdModule::OPTION, "Option", &[TypeSpec::Named(StdModule::VEC, "Vector", &[spec::STRING])])),
    ("net-tls-handshake", TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT])),
    ("net-udp-recv", TypeSpec::Named(StdModule::OPTION, "Option", &[TypeSpec::Named(StdModule::VEC, "Vector", &[spec::INT])])),
    ("net-accept", TypeSpec::Named(StdModule::OPTION, "Option", &[spec::I32])),
    ("net-fill", TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT])),
    ("net-flush", TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT])),
    ("net-socket-error", TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
    ("net-peer-subject", TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
    ("net-server-name", TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
    ("net-pop-byte", TypeSpec::Named(StdModule::OPTION, "Option", &[spec::INT])),
    ("net-pop-char", TypeSpec::Named(StdModule::OPTION, "Option", &[spec::CHAR])),
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

fn result_ok(heap: &mut Heap, name: &str, v: Value) -> Value {
    let key = lookup(RESULT_TYPES, name, "result");
    heap.alloc_enum(key, 0, vec![v])
}

/// `Err(NetError(msg))`. `NetError` has one variant (index 0) carrying the
/// message, the shape every built-in error type has.
fn result_err(heap: &mut Heap, name: &str, msg: String) -> Value {
    let msg_val = heap.alloc_string(msg);
    let err_val = heap.alloc_enum(TypeId::NET_ERROR, 0, vec![msg_val]);
    let key = lookup(RESULT_TYPES, name, "result");
    heap.alloc_enum(key, 1, vec![err_val])
}

fn option_value(heap: &mut Heap, name: &str, v: Option<Value>) -> Value {
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
pub fn net_builtin(heap: &mut Heap, name: &str, args: &[Value]) -> Option<Result<Value, ArgError>> {
    macro_rules! wrap {
        ($e:expr, $ok:expr) => {
            match $e {
                Ok(v) => Ok(result_ok(heap, name, $ok(v))),
                Err(m) => Ok(result_err(heap, name, m)),
            }
        };
    }
    /// A result whose payload is an `Option` box.
    macro_rules! wrap_option {
        ($e:expr, $ok:expr) => {
            match $e {
                Ok(v) => {
                    let inner = option_value(heap, name, v.map($ok));
                    Ok(result_ok(heap, name, inner))
                }
                Err(m) => Ok(result_err(heap, name, m)),
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

    Some(match name {
        "net-resolve-begin" => {
            let (host, port) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_resolve_begin(&host, port)), |v: i64| Value::Int(v))
        }
        // The addresses as a `Vector<string>` box — `file-list-directory`'s
        // shape, safe for its reason: nothing between the element
        // allocations and the box can collect.
        "net-resolve-finish" => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.net_resolve_finish(h)) {
                Ok(Some(all)) => {
                    let elems: Vec<Value> = all.into_iter().map(|a| heap.alloc_string(a)).collect();
                    let key = typelisp_mem::intern_spec(&TypeSpec::Named(StdModule::VEC, "Vector", &[spec::STRING]));
                    let vec_val = heap.alloc_struct(key, elems);
                    let ov = option_value(heap, name, Some(vec_val));
                    Ok(result_ok(heap, name, ov))
                }
                Ok(None) => {
                    let ov = option_value(heap, name, None);
                    Ok(result_ok(heap, name, ov))
                }
                Err(m) => Ok(result_err(heap, name, m)),
            }
        }
        "net-connect-begin" => {
            let addr = arg!(text(heap, args, 0, name));
            wrap!(with_streams(heap, |t| t.net_connect_begin(&addr)), |v: i64| Value::Int(v))
        }
        "net-tls-start" => {
            let (h, host) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            let ca = arg!(opt_text(heap, args, 2, name));
            let (cert, key) = (arg!(opt_text(heap, args, 3, name)), arg!(opt_text(heap, args, 4, name)));
            wrap!(
                with_streams(heap, |t| t.net_tls_start(h, &host, ca.as_deref(), cert.as_deref(), key.as_deref())),
                |_v: ()| Value::Empty
            )
        }
        "net-tls-listen" => {
            let (host, port) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            let (cert, key) = (arg!(text(heap, args, 2, name)), arg!(text(heap, args, 3, name)));
            let client_ca = arg!(opt_text(heap, args, 4, name));
            wrap!(with_streams(heap, |t| t.net_tls_listen(&host, port, &cert, &key, client_ca.as_deref())), |v: i64| Value::Int(v))
        }
        // The text as a string box inside the `Option`: allocated first,
        // then boxed, with nothing that can collect in between.
        "net-socket-error" | "net-peer-subject" | "net-server-name" => {
            let h = arg!(int(args, 0, name));
            let answer = with_streams(heap, |t| match name {
                "net-socket-error" => t.net_socket_error(h),
                "net-peer-subject" => t.net_peer_subject(h),
                _ => t.net_server_name(h),
            });
            match answer {
                Ok(what) => {
                    let inner = what.map(|m| heap.alloc_string(m));
                    let ov = option_value(heap, name, inner);
                    Ok(result_ok(heap, name, ov))
                }
                Err(m) => Ok(result_err(heap, name, m)),
            }
        }
        "net-set-nodelay" => {
            let (h, on) = (arg!(int(args, 0, name)), arg!(flag(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_set_nodelay(h, on)), |_v: ()| Value::Empty)
        }
        "net-set-keepalive" => {
            let (h, on) = (arg!(int(args, 0, name)), arg!(flag(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_set_keepalive(h, on)), |_v: ()| Value::Empty)
        }
        "net-set-keepalive-period" => {
            let (h, secs) = (arg!(int(args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_set_keepalive_period(h, secs)), |_v: ()| Value::Empty)
        }
        "net-tls-add-certificate" => {
            let (h, host) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            let (cert, key) = (arg!(text(heap, args, 2, name)), arg!(text(heap, args, 3, name)));
            wrap!(with_streams(heap, |t| t.net_tls_add_certificate(h, &host, &cert, &key)), |_v: ()| Value::Empty)
        }
        "net-tls-handshake" => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_tls_handshake(h)), Value::Int)
        }
        "net-udp-bind" => {
            let (host, port) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_udp_bind(&host, port)), |v: i64| Value::Int(v))
        }
        "net-udp-send-to" => {
            let (h, addr) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            match arg!(bytes(heap, args, 2, name)) {
                Ok(data) => wrap!(with_streams(heap, |t| t.net_udp_send_to(h, &addr, &data)), |v: bool| Value::Bool(v)),
                Err(m) => Ok(result_err(heap, name, m)),
            }
        }
        // The datagram as a `Vector<int>` box. Allocating every element
        // before the box is safe for `file-list-directory`'s reason:
        // `Value::Int` allocates nothing and `alloc_struct` never collects.
        "net-udp-recv" => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.net_udp_recv(h)) {
                Ok(Some(data)) => {
                    let elems: Vec<Value> = data.into_iter().map(|b| Value::Int(b as i64)).collect();
                    let key = typelisp_mem::intern_spec(&TypeSpec::Named(StdModule::VEC, "Vector", &[spec::INT]));
                    let vec_val = heap.alloc_struct(key, elems);
                    let ov = option_value(heap, name, Some(vec_val));
                    Ok(result_ok(heap, name, ov))
                }
                Ok(None) => {
                    let ov = option_value(heap, name, None);
                    Ok(result_ok(heap, name, ov))
                }
                Err(m) => Ok(result_err(heap, name, m)),
            }
        }
        "net-udp-last-sender" => {
            let h = arg!(int(args, 0, name));
            match with_streams(heap, |t| t.net_udp_last_sender(h)) {
                Ok(s) => {
                    let sv = heap.alloc_string(s);
                    Ok(result_ok(heap, name, sv))
                }
                Err(m) => Ok(result_err(heap, name, m)),
            }
        }
        "net-connect-finish" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.net_connect_finish(h)), |_v: ()| Value::Empty)
        }
        "net-listen" => {
            let (host, port) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_listen(&host, port)), |v: i64| Value::Int(v))
        }
        "net-unix-connect" => {
            let path = arg!(text(heap, args, 0, name));
            wrap!(with_streams(heap, |t| t.net_unix_connect(&path)), |v: i64| Value::Int(v))
        }
        "net-unix-listen" => {
            let path = arg!(text(heap, args, 0, name));
            wrap!(with_streams(heap, |t| t.net_unix_listen(&path)), |v: i64| Value::Int(v))
        }
        "net-accept" => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_accept(h)), Value::Int)
        }
        "net-fill" => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_fill(h)), |n: usize| Value::Int(n as i64))
        }
        "net-pop-byte" => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_pop_byte(h)), |b: u8| Value::Int(b as i64))
        }
        "net-pop-char" => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_pop_char(h)), Value::Char)
        }
        "net-buffered-p" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.net_buffered(h)), |v: bool| Value::Bool(v))
        }
        "net-push-string" => {
            let (h, s) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            wrap!(with_streams(heap, |t| t.net_push_string(h, &s)), |_v: ()| Value::Empty)
        }
        "net-push-byte" => {
            let (h, b) = (arg!(int(args, 0, name)), arg!(int(args, 1, name)));
            match u8::try_from(b) {
                Ok(b) => wrap!(with_streams(heap, |t| t.net_push_byte(h, b)), |_v: ()| Value::Empty),
                Err(_) => Ok(result_err(heap, name, format!("write-byte: {} is not a byte (0..255)", b))),
            }
        }
        "net-flush" => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(heap, |t| t.net_flush(h)), Value::Int)
        }
        "net-shutdown-write" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(heap, |t| t.net_shutdown_write(h)), |_v: ()| Value::Empty)
        }
        "net-local-address" | "net-peer-address" => {
            let h = arg!(int(args, 0, name));
            let r = if name == "net-local-address" {
                with_streams(heap, |t| t.net_local_address(h))
            } else {
                with_streams(heap, |t| t.net_peer_address(h))
            };
            match r {
                Ok(s) => {
                    let sv = heap.alloc_string(s);
                    Ok(result_ok(heap, name, sv))
                }
                Err(m) => Ok(result_err(heap, name, m)),
            }
        }
        _ => return None,
    })
}
