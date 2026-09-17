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

use typelisp_mem::{Heap, TypeKeyId, Value};

use crate::stream::with_streams;
use crate::stream_builtin::{int, lookup, text, ArgError};

/// The runtime type key each builtin's *result* carries — [`crate::
/// stream_builtin::RESULT_KEYS`]'s companion, checked against the registry
/// by the same guard test and for the same reason.
pub const RESULT_KEYS: &[(&str, &str)] = &[
    ("net-connect-begin", "result<i32,neterror>"),
    ("net-connect-finish", "result<(),neterror>"),
    ("net-listen", "result<i32,neterror>"),
    ("net-accept", "result<option<i32>,neterror>"),
    ("net-fill", "result<option<int>,neterror>"),
    ("net-pop-byte", "result<option<int>,neterror>"),
    ("net-pop-char", "result<option<char>,neterror>"),
    ("net-buffered-p", "result<bool,neterror>"),
    ("net-push-string", "result<(),neterror>"),
    ("net-push-byte", "result<(),neterror>"),
    ("net-flush", "result<bool,neterror>"),
    ("net-shutdown-write", "result<(),neterror>"),
    ("net-local-address", "result<string,neterror>"),
    ("net-peer-address", "result<string,neterror>"),
];

/// The key of the value inside the `Result`, for the builtins whose payload
/// is itself a box.
pub const INNER_KEYS: &[(&str, &str)] = &[
    ("net-accept", "option<i32>"),
    ("net-fill", "option<int>"),
    ("net-pop-byte", "option<int>"),
    ("net-pop-char", "option<char>"),
];

fn result_ok(heap: &mut Heap, name: &str, v: Value) -> Value {
    let key = heap.intern_type_key(lookup(RESULT_KEYS, name, "result"));
    heap.alloc_enum(key, 0, vec![v])
}

/// `Err(NetError(msg))`. `NetError` has one variant (index 0) carrying the
/// message, the shape every built-in error type has.
fn result_err(heap: &mut Heap, name: &str, msg: String) -> Value {
    let msg_val = heap.alloc_string(msg);
    let err_val = heap.alloc_enum(TypeKeyId::NET_ERROR, 0, vec![msg_val]);
    let key = heap.intern_type_key(lookup(RESULT_KEYS, name, "result"));
    heap.alloc_enum(key, 1, vec![err_val])
}

fn option_value(heap: &mut Heap, name: &str, v: Option<Value>) -> Value {
    heap.alloc_option(lookup(INNER_KEYS, name, "inner"), v)
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
        "net-connect-begin" => {
            let (host, port) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(|t| t.net_connect_begin(&host, port)), |v: i64| Value::Int(v))
        }
        "net-connect-finish" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(|t| t.net_connect_finish(h)), |_v: ()| Value::Empty)
        }
        "net-listen" => {
            let (host, port) = (arg!(text(heap, args, 0, name)), arg!(int(args, 1, name)));
            wrap!(with_streams(|t| t.net_listen(&host, port)), |v: i64| Value::Int(v))
        }
        "net-accept" => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(|t| t.net_accept(h)), Value::Int)
        }
        "net-fill" => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(|t| t.net_fill(h)), |n: usize| Value::Int(n as i64))
        }
        "net-pop-byte" => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(|t| t.net_pop_byte(h)), |b: u8| Value::Int(b as i64))
        }
        "net-pop-char" => {
            let h = arg!(int(args, 0, name));
            wrap_option!(with_streams(|t| t.net_pop_char(h)), Value::Char)
        }
        "net-buffered-p" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(|t| t.net_buffered(h)), |v: bool| Value::Bool(v))
        }
        "net-push-string" => {
            let (h, s) = (arg!(int(args, 0, name)), arg!(text(heap, args, 1, name)));
            wrap!(with_streams(|t| t.net_push_string(h, &s)), |_v: ()| Value::Empty)
        }
        "net-push-byte" => {
            let (h, b) = (arg!(int(args, 0, name)), arg!(int(args, 1, name)));
            match u8::try_from(b) {
                Ok(b) => wrap!(with_streams(|t| t.net_push_byte(h, b)), |_v: ()| Value::Empty),
                Err(_) => Ok(result_err(heap, name, format!("write-byte: {} is not a byte (0..255)", b))),
            }
        }
        "net-flush" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(|t| t.net_flush(h)), |v: bool| Value::Bool(v))
        }
        "net-shutdown-write" => {
            let h = arg!(int(args, 0, name));
            wrap!(with_streams(|t| t.net_shutdown_write(h)), |_v: ()| Value::Empty)
        }
        "net-local-address" | "net-peer-address" => {
            let h = arg!(int(args, 0, name));
            let r = if name == "net-local-address" {
                with_streams(|t| t.net_local_address(h))
            } else {
                with_streams(|t| t.net_peer_address(h))
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
