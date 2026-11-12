//! The built-in functions that reach outside the program: streams and
//! files, sockets, the host the program runs on, and the readtable.

use crate::types::{std_module, Path, Type};
use typelisp_mem::symbols::NsId;
use typelisp_mem::wk;

use super::super::registry::Registry;
use super::{error_ty, function, option_of, result_of, sexpr_ty, universal_time, vector_of};

/// The streams, the sockets and the REPL tools' runtime half in `internal`,
/// the host's environment in `env`, `sleep`/`yield` in `thread`, the local
/// time zone in `time`, and the readtable in `reader`.
pub(super) fn define(reg: &mut Registry, env: NsId, thread: NsId, time: NsId, internal: NsId, reader: NsId) {
    streams(reg, internal);
    net(reg, internal);
    host(reg, env, thread, time, internal);
    readtable(reg, reader);
}

/// The `stream-*` / `file-*` primitives (`eval::interp::Interp::
/// eval_stream_builtin`). Deliberately minimal and untyped-looking: a stream
/// is an opaque `i32` handle here, and the whole CL-shaped surface — the
/// `Stream`/`InputStream`/`OutputStream` traits, the concrete stream types
/// that wrap a handle in a `defstruct`, every composite stream, and the
/// `with-...` macros — is written in `prelude.rs` on top of these.
///
/// Anything that can fail returns `Result<_, FileError>`; a missing file or a
/// closed stream is something programs handle, not a bug.
fn streams(reg: &mut Registry, ns: NsId) {
    let file_err = error_ty(wk::FILE_ERROR);
    let mut native = |name: &str, params: Vec<Type>, ret: Type| function(reg, ns, name, params, ret);
    let h = Type::I32;
    let unit_or_err = result_of(Type::Unit, file_err.clone());

    // Constructors. The three standard streams hand out a fresh handle per
    // call; the prelude opens each exactly once, into a global.
    native("stream-stdin", vec![], h.clone());
    native("stream-stdout", vec![], h.clone());
    native("stream-stderr", vec![], h.clone());
    native("stream-string-input", vec![Type::Str], h.clone());
    native("stream-string-output", vec![], h.clone());
    // mode: 0 input, 1 output (truncate), 2 output (append).
    // The direction is a code (`direction-input`/`-output`/`-append`), an
    // ordinary `int`; only the handle stays an `i32`.
    native("stream-open-file", vec![Type::Str, Type::Int], result_of(h.clone(), file_err.clone()));

    // Lifetime and interrogation.
    native("stream-close", vec![h.clone()], unit_or_err.clone());
    native("stream-open-p", vec![h.clone()], Type::Bool);
    // What the stream is attached to, for its printed form (`for "file
    // /tmp/a.txt"`) — `StreamTable::describe`.
    native("stream-describe", vec![h.clone()], Type::Str);
    native("stream-input-p", vec![h.clone()], result_of(Type::Bool, file_err.clone()));
    native("stream-output-p", vec![h.clone()], result_of(Type::Bool, file_err.clone()));

    // Input. `stream-read-char` returns `Ok(none)` at end of input and `Err`
    // only for a real failure, so end-of-input never has to be a panic.
    native(
        "stream-read-char",
        vec![h.clone()],
        result_of(option_of(Type::Char), file_err.clone()),
    );
    native("stream-unread-char", vec![h.clone(), Type::Char], unit_or_err.clone());
    // Byte I/O, the same shape one level down: `Ok(none)` at end of file.
    // Only a file stream (and stdin/stdout/stderr) answers these — a string
    // stream is a sequence of characters, and CL calls `read-byte` on a
    // character stream an error rather than handing back a UTF-8 encoding
    // nobody wrote.
    native("stream-read-byte", vec![h.clone()], result_of(option_of(Type::Int), file_err.clone()));
    native("stream-write-byte", vec![h.clone(), Type::Int], unit_or_err.clone());
    native("stream-listen", vec![h.clone()], result_of(Type::Bool, file_err.clone()));
    // How many characters have been read out of a *string* input stream.
    // Narrow on purpose: this is what tells a reader macro's caller how much
    // of the text the macro consumed, and no other backend has a position in
    // the same unit. Not CL's `file-position` — that one also seeks.
    native("stream-position", vec![h.clone()], result_of(Type::Int, file_err.clone()));

    // Output. Only whole strings cross this boundary — a per-character
    // built-in call would dominate the cost of writing anything.
    native("stream-write-string", vec![h.clone(), Type::Str], unit_or_err.clone());
    native("stream-at-line-start", vec![h.clone()], result_of(Type::Bool, file_err.clone()));
    native("stream-finish-output", vec![h.clone()], unit_or_err.clone());
    native("stream-take-output-string", vec![h.clone()], result_of(Type::Str, file_err.clone()));

    // Filesystem operations that need no open stream. Every one of these is
    // a *primitive* under a `Pathish` prelude wrapper that carries the CL
    // name (`probe-file`, `delete-file`, `rename-file`, `truename`,
    // `file-write-date`, `directory-p`, `directory`,
    // `ensure-directories-exist`).
    //
    // The `file-` prefix is load-bearing, not decorative: `Interp::
    // eval_builtin` routes every `stream-`/`file-` name to
    // `stream_builtin::stream_builtin`, so a filesystem builtin spelled
    // otherwise would be unreachable interpreted.
    native("file-exists-p", vec![Type::Str], Type::Bool);
    native("file-delete", vec![Type::Str], unit_or_err.clone());
    native("file-rename", vec![Type::Str, Type::Str], unit_or_err.clone());
    native("file-truename", vec![Type::Str], result_of(Type::Str, file_err.clone()));
    // A universal time, on `get-universal-time`'s 1900-epoch scale, so the
    // two are comparable and either decodes with the same prelude function.
    native("file-modified-date", vec![Type::Str], result_of(universal_time(), file_err.clone()));
    native("file-directory-p", vec![Type::Str], Type::Bool);
    native(
        "file-list-directory",
        vec![Type::Str],
        result_of(vector_of(Type::Str), file_err.clone()),
    );
    native("file-create-directories", vec![Type::Str], unit_or_err);
    // CL's `file-author`, spelled with the routing prefix. `Option` inside
    // the `Result` because CL separates the two failures: the file not being
    // there is an error, an owner with no password-database entry is `NIL`.
    native("file-owner-name", vec![Type::Str], result_of(option_of(Type::Str), file_err.clone()));
}

/// The `net-*` primitives (`typelisp_rt::net_builtin`): a TCP socket as an
/// opaque `i32` handle in the same table the streams live in, and every
/// operation on it in its **non-blocking** form. The prelude's `socket-stream`,
/// `socket-byte-stream` and `socket-listener` are written on top of these, and the
/// waiting is written there too: each "not yet" answer below (`none` from
/// `net-fill`/`net-accept`, `some interest` from `net-flush`) is followed by
/// a `(net-wait h interest)`, which parks the task until `poll` says the
/// socket is ready.
///
/// A failure the *peer* causes (a reset, a TLS alert) is not an `Err` of any
/// of these: it is recorded on the socket, after which `net-fill` says end
/// of input and `net-flush` says done, and `net-socket-error` reports it.
/// The `Err`s are the program's own mistakes and the outcomes of the calls
/// that make a connection.
///
/// `net-wait` is the one entry here that answers with nothing: like `sleep`
/// and `yield` it is registered as an ordinary builtin and intercepted by the
/// evaluator, because only a `State` can say "stop here". Its `interest` is
/// `0` for readable and `1` for writable — `typelisp_rt::os::Interest`.
fn net(reg: &mut Registry, ns: NsId) {
    let net_err = error_ty(wk::NET_ERROR);
    let mut native = |name: &str, params: Vec<Type>, ret: Type| function(reg, ns, name, params, ret);
    let h = Type::I32;
    let unit_or_err = result_of(Type::Unit, net_err.clone());

    // Connecting is two steps with a wait between: begin (the handle comes
    // back with the connect in flight), wait for writable, finish (did it
    // succeed). A single blocking `connect` would stop every task for as
    // long as an unreachable host takes to time out.
    // Name resolution first, on a thread the task waits for through a pipe
    // (`Ok(none)` from `finish`: not yet), then the connect proper on the
    // `ip:port` it answered with.
    native("net-resolve-begin", vec![Type::Str, Type::Int], result_of(h.clone(), net_err.clone()));
    native(
        "net-resolve-finish",
        vec![h.clone()],
        result_of(option_of(vector_of(Type::Str)), net_err.clone()),
    );
    native("net-connect-begin", vec![Type::Str], result_of(h.clone(), net_err.clone()));
    native("net-connect-finish", vec![h.clone()], unit_or_err.clone());
    // TLS on a connected socket: `start` names the server the certificate
    // must be for; `handshake` is one non-blocking step, answering `none`
    // when done and `some interest` when the socket must become ready
    // before the next.
    // `start`'s options: a CA file to trust instead of the public roots, and
    // a client certificate and key to present (both or neither).
    native(
        "net-tls-start",
        vec![h.clone(), Type::Str, option_of(Type::Str), option_of(Type::Str), option_of(Type::Str)],
        unit_or_err.clone(),
    );
    native("net-tls-handshake", vec![h.clone()], result_of(option_of(Type::Int), net_err.clone()));
    // A listener whose accepted connections speak TLS with the certificate
    // and key in the two PEM files, requiring a client certificate issued
    // by the optional CA file; the handshake is the connection's first read
    // or write, not `accept`'s.
    native(
        "net-tls-listen",
        vec![Type::Str, Type::Int, Type::Str, Type::Str, option_of(Type::Str)],
        result_of(h.clone(), net_err.clone()),
    );
    // UDP: whole datagrams. `send-to` answers `false` when the send buffer
    // is full (wait for writable); `recv` answers `none` when nothing has
    // arrived (wait for readable), and `last-sender` says who sent what
    // `recv` last returned.
    native("net-udp-bind", vec![Type::Str, Type::Int], result_of(h.clone(), net_err.clone()));
    native(
        "net-udp-send-to",
        vec![h.clone(), Type::Str, vector_of(Type::Int)],
        result_of(Type::Bool, net_err.clone()),
    );
    native(
        "net-udp-recv",
        vec![h.clone()],
        result_of(option_of(vector_of(Type::Int)), net_err.clone()),
    );
    native("net-udp-last-sender", vec![h.clone()], result_of(Type::Str, net_err.clone()));
    native("net-listen", vec![Type::Str, Type::Int], result_of(h.clone(), net_err.clone()));
    // Unix-domain sockets: the same connected-socket handle, made from a
    // path. A local connect has no handshake to wait for, so it is one step.
    native("net-unix-connect", vec![Type::Str], result_of(h.clone(), net_err.clone()));
    native("net-unix-listen", vec![Type::Str], result_of(h.clone(), net_err.clone()));
    // `Ok(none)`: nobody is waiting to be accepted — wait for readable.
    native("net-accept", vec![h.clone()], result_of(option_of(h.clone()), net_err.clone()));

    // Input is a buffer the OS fills and the program drains. `net-fill`
    // answers `Ok(some 0)` at end of input and `Ok(none)` when nothing has
    // arrived — wait for readable. The two `pop`s answer `none` when the
    // buffer has no whole item (`net-pop-char` will not split a UTF-8
    // character), which is the cue to fill.
    native("net-fill", vec![h.clone()], result_of(option_of(Type::Int), net_err.clone()));
    native("net-pop-byte", vec![h.clone()], result_of(option_of(Type::Int), net_err.clone()));
    native("net-pop-char", vec![h.clone()], result_of(option_of(Type::Char), net_err.clone()));
    native("net-buffered-p", vec![h.clone()], result_of(Type::Bool, net_err.clone()));

    // Output is the mirror: push into a buffer, then flush. `net-flush`
    // answers `some interest` when the OS took only part of it (or a TLS
    // handshake is waiting on the peer) — wait for that and flush again.
    native("net-push-string", vec![h.clone(), Type::Str], unit_or_err.clone());
    native("net-push-byte", vec![h.clone(), Type::Int], unit_or_err.clone());
    native("net-flush", vec![h.clone()], result_of(option_of(Type::Int), net_err.clone()));
    native("net-socket-error", vec![h.clone()], result_of(option_of(Type::Str), net_err.clone()));
    // Who the peer is (its certificate's subject) and which name the client
    // asked for (SNI): `none` where there is no TLS or no such thing.
    native("net-peer-subject", vec![h.clone()], result_of(option_of(Type::Str), net_err.clone()));
    native("net-server-name", vec![h.clone()], result_of(option_of(Type::Str), net_err.clone()));
    // Socket options: Nagle off, keepalive on/off and its period in seconds.
    native("net-set-nodelay", vec![h.clone(), Type::Bool], unit_or_err.clone());
    native("net-set-keepalive", vec![h.clone(), Type::Bool], unit_or_err.clone());
    native("net-set-keepalive-period", vec![h.clone(), Type::Int], unit_or_err.clone());
    // One more certificate on a `tls-listen` listener, for clients asking
    // for that name.
    native("net-tls-add-certificate", vec![h.clone(), Type::Str, Type::Str, Type::Str], unit_or_err.clone());
    native("net-shutdown-write", vec![h.clone()], unit_or_err.clone());
    native("net-local-address", vec![h.clone()], result_of(Type::Str, net_err.clone()));
    native("net-peer-address", vec![h.clone()], result_of(Type::Str, net_err.clone()));

    // The wait itself. Not a `Result`: a closed handle here is a program
    // error (the stream was closed and then waited on), which is a panic.
    native("net-wait", vec![h.clone(), Type::Int], Type::Unit);
    // The same wait with a clock: `true` when the socket became ready,
    // `false` when `secs` ran out first. Seconds as `f64`, `sleep`'s unit.
    native("net-wait-for", vec![h, Type::Int, Type::F64], Type::Bool);
}

/// The environment the program is running in (CLHS 25.1), in `std::env` —
/// plus the two things CL has no equivalent of at all and a script cannot do
/// without, `command-line-args` and `getenv`. Also the host primitives that
/// belong elsewhere: `sleep`/`yield` in `std::thread`, the local time zone in
/// `std::time`, and the REPL tools' runtime half in `std::%internal`.
///
/// All free functions: none has a receiver to dispatch on, the same reason
/// the `random-state` primitives above are free functions.
/// `lisp-implementation-type` is deliberately *not* here — it is a constant
/// string, so the prelude defines it in typelisp rather than spending a
/// builtin on it.
fn host(reg: &mut Registry, env: NsId, thread: NsId, time: NsId, internal: NsId) {
    let mut native = |ns: NsId, name: &str, params: Vec<Type>, ret: Type| function(reg, ns, name, params, ret);
    // Element 0 names the program in both worlds — the script path under
    // `typl`, the executable under AOT — so one source file can be run either
    // way and index its arguments identically. See
    // `sys_builtin::COMMAND_LINE_ARGS` for how the two are made to agree.
    native(env, "command-line-args", vec![], vector_of(Type::Str));
    // `none` covers both "unset" and "not valid Unicode": a `string` here is
    // Rust's, so bytes that aren't UTF-8 have no value to hand back.
    native(env, "getenv", vec![Type::Str], option_of(Type::Str));
    // `$HOME`, or `none` — the primitive under `user-homedir-pathname`, which
    // CL explicitly allows to answer `NIL`.
    native(env, "home-directory", vec![], option_of(Type::Str));
    // CL's `sleep`, in CL's unit. `f64` and not an integer type because a
    // fractional wait has to be writable; an integer literal does not become
    // a float here, so `(sleep 1)` is a type error and `(sleep 1.0)` is not
    // (the rule Rust has, and the reason the docs say so out loud).
    native(thread, "sleep", vec![Type::F64], Type::Unit);
    // `(yield)` — give up the rest of this task's turn (Go's `runtime.Gosched`).
    // Registered as an ordinary nullary builtin, but the evaluator intercepts
    // it: only a `State` can say "stop here", which a builtin's return value
    // cannot.
    native(thread, "yield", vec![], Type::Unit);
    native(env, "lisp-implementation-version", vec![], Type::Str);
    native(env, "machine-type", vec![], Type::Str);
    native(env, "software-type", vec![], Type::Str);
    // The three CLHS 25.1 names that need the host rather than a build-time
    // constant, and that CL explicitly lets answer `NIL`. `machine-version`
    // is the chip this is *running* on; `machine-type` above is the
    // architecture the binary was *built* for.
    native(env, "machine-instance", vec![], option_of(Type::Str));
    native(env, "machine-version", vec![], option_of(Type::Str));
    native(env, "software-version", vec![], option_of(Type::Str));
    // The local time zone at a given universal time — the primitive under
    // `decode-`/`encode-universal-time`'s no-zone default, which is CL's
    // local time. Split into `day`/`second` because a whole universal time
    // does not fit an `i32`, and reported in *seconds* west because the
    // offset is not always a whole number of hours (India is +5:30).
    // `none` when the C library cannot represent that instant.
    native(time, "timezone-offset-seconds", vec![Type::Int, Type::Int], option_of(Type::Int));
    native(time, "timezone-daylight-p", vec![Type::Int, Type::Int], option_of(Type::Bool));

    // The REPL tool layer's runtime half (CLHS 25.2 / 25.1), with `heap-info`
    // in `std::repl`. The other four tools — `trace`/`untrace`/`step`/
    // `disassemble` — have no entry here and never will: they are
    // interpreter-only special forms, the same category `compile`/
    // `compile-file`/`dump` are in. These are ordinary builtins, so a `defun`
    // that calls them still compiles.
    let file_err = error_ty(wk::FILE_ERROR);
    // `dribble` itself is a prelude `defun` with an `&optional` path; these
    // two are the halves it dispatches to. Both `Result`, since both touch a
    // file — and stopping *flushes*, which is where a full disk is found.
    native(internal, "dribble-start", vec![Type::Str], result_of(Type::Unit, file_err.clone()));
    native(internal, "dribble-stop", vec![], result_of(Type::Unit, file_err.clone()));
    // The runtime half of the `ed` special form, which resolved the name at
    // check time and reduced to this. Line 0 means "no line".
    native(internal, "ed-open", vec![Type::Str, Type::Int], result_of(Type::Unit, file_err));
}

/// The type of a reader macro: what `set-macro-character` stores and what the
/// reader calls when the character turns up.
///
/// The stream is a `string-input-stream` and not a `:dyn PeekInput`, because
/// that is honestly what it is. The reader hands its unread text to the
/// evaluator and gets back a character count (`Interp::call_reader_macro_fn`),
/// so the stream a macro sees is *always* one made over that text — a trait
/// object would be a generality with exactly one inhabitant. Nothing is lost
/// with it: every reader operation a macro wants (`read-sexpr`, `read-char`,
/// `peek-char`, `unread-char`) is generic over `(:where (PeekInput S))` and
/// takes the concrete type directly.
///
/// The character comes second, as in CL, and is the *sub*-character for a
/// two-character sequence — `#\{` for `(set-dispatch-macro-character #\# #\{ ...)`
/// — so one function can serve several of them by looking at it.
///
/// `Option<Sexpr>` rather than `Sexpr` because `()` is `none`: a macro that
/// reads an empty list has to be able to say so. A macro that wants to
/// produce *nothing at all* (CL's zero values, what a comment reader macro
/// returns) has no way to say that and is not supported; there is one datum
/// per call.
fn reader_macro_fn_type() -> Type {
    Type::Fn(
        vec![Type::Named(Path::in_ns(std_module("io"), "string-input-stream"), vec![]), Type::Char],
        None,
        Box::new(option_of(sexpr_ty())),
    )
}

/// The readtable (CLHS 23.1): the character-to-function table the reader
/// consults before it does anything else with a character, in `std::reader`.
///
/// Four functions, not CL's eight. There is no `*readtable*` and no
/// `copy-readtable`, because there is no readtable *object* — a first-class
/// one would have to be a value the reader can be handed, and the reader
/// here is called from Rust drivers that have nowhere to take it from.
/// `make-dispatch-macro-character` is absent for a different reason:
/// `set-dispatch-macro-character` makes the character dispatching by itself,
/// which leaves the separate step nothing to do.
fn readtable(reg: &mut Registry, reader: NsId) {
    let mut native = |name: &str, params: Vec<Type>, ret: Type| function(reg, reader, name, params, ret);
    let f = reader_macro_fn_type();
    native("set-macro-character", vec![Type::Char, f.clone()], Type::Unit);
    native("get-macro-character", vec![Type::Char], option_of(f.clone()));
    native("set-dispatch-macro-character", vec![Type::Char, Type::Char, f.clone()], Type::Unit);
    native("get-dispatch-macro-character", vec![Type::Char, Type::Char], option_of(f));
}
