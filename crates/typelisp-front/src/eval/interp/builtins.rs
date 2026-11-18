//! Dispatch of the built-in functions and built-in methods to their
//! implementations.

use crate::{wk, Heap, Path, SymRef, TypeId, Value};
use crate::eval::value::EvalError;
use super::{backend, bool_eq, char_compare, char_eq, char_eqp, char_lt, char_to_int, eval_float_builtin, eval_get_internal_real_time, eval_get_internal_run_time, eval_get_universal_time, eval_int_builtin, eval_integer_builtin, eval_llvm_builtin_method, eval_make_random_state_fresh, eval_parse_float, eval_random_state_copy, eval_random_state_next, eval_ratio_builtin, eval_rational_builtin, eval_seed_random_state, expect_char, expect_int, expect_str, expect_struct_box, float_expt, float_to_int, float_to_ratio, float_to_width, float_unary, hashtable_bucket_count, hashtable_bucket_delete, hashtable_bucket_key, hashtable_bucket_put, hashtable_bucket_value, hashtable_clear, hashtable_count, hashtable_entries, hashtable_keys, hashtable_values, int_integer_length, int_logcount, int_lognot, int_pair, int_to_char, int_to_float, int_to_ratio, int_to_width, integer_conversion, integer_to_width, integer_unary, ratio_denominator, ratio_numerator, ratio_to_float, ratio_to_int, rational_unary, rt_f64, rt_i64, rt_str, scope_clone_frames_heap, scope_get_heap, scope_pop_frame_heap, scope_push_frame_heap, scope_set_heap, sexpr_eq, sexpr_eql, sexpr_equal, sexpr_equalp, str_rt, string_append, string_compare, string_content_eq, string_content_eqp, string_identity_eq, string_length, string_lt, string_ref, string_substring, try_float_to_f32, try_int_to_char, try_int_to_width, vector_get, vector_len, vector_pop, vector_push, vector_set, width_cast_target, width_to_int, Interp};

impl Interp {
    /// Evaluate a built-in operator. Returns `None` if `name` is not a
    /// builtin, so the caller can fall through to a "no such function" error.
    /// (Arithmetic/comparison operators are *instance* methods, not free
    /// functions — see `eval_builtin_method` — so they don't appear here.
    /// `cons`/`car`/`cdr` operate on `Sexpr`; `car`/`cdr` of a non-`Cons`
    /// `Sexpr` — including `Nil` — panics. `gensym` returns a fresh
    /// `Sexpr::Sym` each call. `make-random-state-fresh`/`random-state-copy`/
    /// `random-state-next` have no natural receiver to dispatch on, so they
    /// stay free functions too — `random`/`make-random-state`/
    /// `random-state-p` are ordinary prelude `defun`s built on top of them.)
    pub(super) fn eval_builtin(&self, heap: &mut Heap, name: SymRef, args: &[Value]) -> Option<Result<Value, EvalError>> {
        // The printing builtins, ahead of the match so there is one list of
        // their names rather than two. Most are what a special form lowered
        // to — `format`/`print`/`println`/`pprint` and `pprint-logical-block`
        // are checked forms, and what survives checking is a call to one of
        // the `*-rt` names; the four `pprint-*` operators are ordinary
        // builtins. All of them run the same
        // `typelisp_print::runtime::print_builtin` that compiled code reaches
        // through `rt_format`/`rt_print`/..., so a `println` inside a
        // `pprint-logical-block` lands in the same buffer whichever side of
        // the compile boundary each half was on.
        self.install_print_hooks();
        if let Some(r) = typelisp_print::runtime::print_builtin(heap, name, args) {
            use typelisp_print::runtime::PrintError;
            return Some(r.map_err(|e| match e {
                // A `~` directive the engine doesn't know, or a
                // `print-object` method that failed: catchable, like the
                // `(panic ...)` it would be if a program wrote it.
                PrintError::Raise(msg) => EvalError::Panic(msg),
                // An argument the checker cannot have produced.
                PrintError::Shape(msg) => EvalError::Internal(msg),
            }));
        }
        match name.well_known() {
            // `(compile-file "source.typl" "output")`: AOT-compiles an
            // independent source file straight to a native executable —
            // see `compile::aot::compile_file`'s doc comment for why this
            // runs against a *fresh* `Heap`/`Checker`/`Interp` rather than
            // the caller's (`self`'s), unlike `compile` above.
            wk::F_COMPILE_FILE => {
                let source_path = match expect_str(heap, &args[0]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                let output_path = match expect_str(heap, &args[1]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                Some(
                    (match backend("compile-file") {
                        Ok(b) => b,
                        Err(e) => return Some(Err(e)),
                    }
                    .compile_file)(&source_path, &output_path)
                        .map(|()| Value::Bool(true))
                        .map_err(|e| EvalError::Panic(format!("compile-file: {}", e))),
                )
            }
            // `(dump "path")`: this session's environment, written where
            // `typl --image` can pick it up — see `compile::dump::dump_image`.
            wk::F_DUMP => {
                let path = match expect_str(heap, &args[0]) {
                    Ok(s) => s.to_string(),
                    Err(e) => return Some(Err(e)),
                };
                let backend = match backend("dump") {
                    Ok(b) => b,
                    Err(e) => return Some(Err(e)),
                };
                Some(
                    (backend.dump_image)(self, heap, &path)
                        .map(|()| Value::Bool(true))
                        .map_err(EvalError::Panic),
                )
            }
            wk::F_FILE_CREATE_DIRECTORIES | wk::F_FILE_DELETE | wk::F_FILE_DIRECTORY_P | wk::F_FILE_EXISTS_P | wk::F_FILE_LIST_DIRECTORY | wk::F_FILE_MODIFIED_DATE | wk::F_FILE_OWNER_NAME | wk::F_FILE_RENAME | wk::F_FILE_TRUENAME | wk::F_STREAM_AT_LINE_START | wk::F_STREAM_CLOSE | wk::F_STREAM_DESCRIBE | wk::F_STREAM_FINISH_OUTPUT | wk::F_STREAM_INPUT_P | wk::F_STREAM_LISTEN | wk::F_STREAM_OPEN_FILE | wk::F_STREAM_OPEN_P | wk::F_STREAM_OUTPUT_P | wk::F_STREAM_POSITION | wk::F_STREAM_READ_BYTE | wk::F_STREAM_READ_CHAR | wk::F_STREAM_STDERR | wk::F_STREAM_STDIN | wk::F_STREAM_STDOUT | wk::F_STREAM_STRING_INPUT | wk::F_STREAM_STRING_OUTPUT | wk::F_STREAM_TAKE_OUTPUT_STRING | wk::F_STREAM_UNREAD_CHAR | wk::F_STREAM_WRITE_BYTE | wk::F_STREAM_WRITE_STRING => {
                self.eval_stream_builtin(heap, name, args)
            }
            // The sockets. `net-wait` never arrives here: `finish_call`
            // intercepts it, as it does `sleep`, because it stops the task.
            wk::F_NET_ACCEPT | wk::F_NET_BUFFERED_P | wk::F_NET_CONNECT_BEGIN | wk::F_NET_CONNECT_FINISH | wk::F_NET_FILL | wk::F_NET_FLUSH | wk::F_NET_LISTEN | wk::F_NET_LOCAL_ADDRESS | wk::F_NET_PEER_ADDRESS | wk::F_NET_PEER_SUBJECT | wk::F_NET_POP_BYTE | wk::F_NET_POP_CHAR | wk::F_NET_PUSH_BYTE | wk::F_NET_PUSH_STRING | wk::F_NET_RESOLVE_BEGIN | wk::F_NET_RESOLVE_FINISH | wk::F_NET_SERVER_NAME | wk::F_NET_SET_KEEPALIVE | wk::F_NET_SET_KEEPALIVE_PERIOD | wk::F_NET_SET_NODELAY | wk::F_NET_SHUTDOWN_WRITE | wk::F_NET_SOCKET_ERROR | wk::F_NET_TLS_ADD_CERTIFICATE | wk::F_NET_TLS_HANDSHAKE | wk::F_NET_TLS_LISTEN | wk::F_NET_TLS_START | wk::F_NET_UDP_BIND | wk::F_NET_UDP_LAST_SENDER | wk::F_NET_UDP_RECV | wk::F_NET_UDP_SEND_TO | wk::F_NET_UNIX_CONNECT | wk::F_NET_UNIX_LISTEN | wk::F_NET_WAIT | wk::F_NET_WAIT_FOR => {
                Some(match typelisp_rt::net_builtin::net_builtin(heap, name, args)? {
                    Ok(v) => Ok(v),
                    Err(e) => Err(EvalError::Internal(e)),
                })
            }
            // The readtable (CLHS 23.1). Routed by name rather than by
            // prefix: `set-`/`get-` are far too common to claim, and there
            // are only four.
            wk::F_SET_MACRO_CHARACTER | wk::F_GET_MACRO_CHARACTER | wk::F_SET_DISPATCH_MACRO_CHARACTER | wk::F_GET_DISPATCH_MACRO_CHARACTER => {
                Some(match typelisp_rt::readtable::readtable_builtin(heap, name, args)? {
                    Ok(v) => Ok(v),
                    Err(e) => Err(EvalError::Internal(e)),
                })
            }
            wk::F_MAKE_RANDOM_STATE_FRESH => Some(eval_make_random_state_fresh(heap, args)),
            wk::F_SEED_RANDOM_STATE => Some(eval_seed_random_state(heap, args)),
            wk::F_RANDOM_STATE_COPY => Some(eval_random_state_copy(heap, args)),
            wk::F_RANDOM_STATE_NEXT => Some(eval_random_state_next(heap, args)),
            wk::F_GET_UNIVERSAL_TIME => Some(eval_get_universal_time(heap, args)),
            wk::F_GET_INTERNAL_REAL_TIME => Some(eval_get_internal_real_time(heap, args)),
            wk::F_GET_INTERNAL_RUN_TIME => Some(eval_get_internal_run_time(heap, args)),
            // The environment and the running implementation. Each is one
            // call into `typelisp_rt::sys_builtin`, the same implementation
            // the compiled shims wrap — the `file-*` half of Phase 9c needs
            // no arm here at all, since the `starts_with("file-")` guard
            // above already routes it to `eval_stream_builtin`.
            wk::F_COMMAND_LINE_ARGS => Some(Ok(typelisp_rt::sys_builtin::command_line_args(heap))),
            wk::F_GETENV => Some(match &args[0] {
                Value::Str(id) => {
                    let name = heap.string(*id).to_string();
                    Ok(typelisp_rt::sys_builtin::getenv(heap, &name))
                }
                other => Err(EvalError::Panic(format!("getenv: argument is not a string, got {:?}", other))),
            }),
            wk::F_HOME_DIRECTORY => Some(Ok(typelisp_rt::sys_builtin::home_directory(heap))),
            // The REPL tool layer's runtime half. `trace`/`untrace`/`step`/
            // `disassemble` are *not* here — those are special forms with
            // their own core nodes, the same way `compile` is.
            wk::F_HEAP_INFO => Some(Ok(typelisp_rt::sys_builtin::heap_info(heap))),
            wk::F_DRIBBLE_START => Some(match &args[0] {
                Value::Str(id) => {
                    let path = heap.string(*id).to_string();
                    Ok(typelisp_rt::sys_builtin::dribble_start(heap, &path))
                }
                other => Err(EvalError::Panic(format!("dribble: argument is not a string, got {:?}", other))),
            }),
            wk::F_DRIBBLE_STOP => Some(Ok(typelisp_rt::sys_builtin::dribble_stop(heap))),
            wk::F_ED_OPEN => Some(match (&args[0], &args[1]) {
                (Value::Str(id), Value::Int(line)) => {
                    let path = heap.string(*id).to_string();
                    let line = *line;
                    Ok(typelisp_rt::sys_builtin::ed_open(heap, &path, line))
                }
                other => Err(EvalError::Panic(format!("ed: arguments are not a string and a line, got {:?}", other))),
            }),
            wk::F_SLEEP => Some(match rt_f64(heap, &args[0]) {
                Ok(secs) => typelisp_rt::sys_builtin::sleep(heap, secs).map(|()| Value::Empty).map_err(EvalError::Panic),
                Err(e) => Err(e),
            }),
            wk::F_LISP_IMPLEMENTATION_VERSION => Some(Ok(typelisp_rt::sys_builtin::lisp_implementation_version(heap))),
            wk::F_MACHINE_TYPE => Some(Ok(typelisp_rt::sys_builtin::machine_type(heap))),
            wk::F_SOFTWARE_TYPE => Some(Ok(typelisp_rt::sys_builtin::software_type(heap))),
            // The three that have to ask the host, and the two that ask it
            // what the local time zone was at a given instant. `file-author`
            // needs no arm for the same reason the rest of the `file-*`
            // family does not — the prefix guard above routed it already.
            wk::F_MACHINE_INSTANCE => Some(Ok(typelisp_rt::sys_builtin::machine_instance(heap))),
            wk::F_MACHINE_VERSION => Some(Ok(typelisp_rt::sys_builtin::machine_version(heap))),
            wk::F_SOFTWARE_VERSION => Some(Ok(typelisp_rt::sys_builtin::software_version(heap))),
            wk::F_TIMEZONE_OFFSET_SECONDS => Some(match int_pair(args, "timezone-offset-seconds") {
                Ok((day, sec)) => Ok(typelisp_rt::sys_builtin::timezone_offset_seconds(heap, day, sec)),
                Err(e) => Err(e),
            }),
            wk::F_TIMEZONE_DAYLIGHT_P => Some(match int_pair(args, "timezone-daylight-p") {
                Ok((day, sec)) => Ok(typelisp_rt::sys_builtin::timezone_daylight_p(heap, day, sec)),
                Err(e) => Err(e),
            }),
            wk::F_PARSE_FLOAT => Some(eval_parse_float(heap, args)),
            // Both pass `self` as the evaluator: an interpreted `(read ...)`
            // honours `#.` and the readtable, the way CL's does. The compiled
            // shims have no `Interp` to pass and go through the thread-local
            // registration instead (`typelisp_read::runtime`).
            wk::F_READ => Some(self.eval_read(heap, args)),
            wk::F_READ_DATUM_AT => Some(self.eval_read_datum_at(heap, args)),
            wk::F_EVAL => Some(self.eval_form(heap, &args[0])),
            wk::F_MACROEXPAND_1 => Some(self.macroexpand_1_form(heap, args[0], args[1])),
            wk::F_MACROEXPAND => Some(self.macroexpand_form(heap, args[0], args[1])),
            // `eq`/`eql`/`equalp` on any type: the free functions (the
            // free-function overloads; the per-scalar-type `equal`
            // *methods* — `string`/`char`/`int`/... — are dispatched separately
            // in `eval_builtin_method`). Rust builtins since Phase 5 fenced
            // `match` off `Sexpr`; see `sexpr_equal`/`sexpr_equalp`.
            wk::M_EQ => Some(sexpr_eq(heap, args)),
            wk::M_EQL => Some(sexpr_eql(heap, args)),
            wk::M_EQUALP => Some(sexpr_equalp(heap, args)),
            wk::F_SXHASH_EQ => Some(Ok(Value::Int(typelisp_rt::equality::sxhash_eq_val(heap, args[0])))),
            wk::F_SXHASH_EQL => Some(Ok(Value::Int(typelisp_rt::equality::sxhash_eql_val(heap, args[0])))),
            wk::F_SXHASH_EQUALP => Some(Ok(Value::Int(typelisp_rt::equality::sxhash_equalp_val(heap, args[0])))),
            // `exit`: terminates the process immediately via the OS, never
            // returning — `rt_i64` truncates to `i32` the same way every
            // other `i32`-typed builtin extracts its argument.
            wk::F_EXIT => match rt_i64(&args[0]) {
                Ok(code) => std::process::exit(code as i32),
                Err(e) => Some(Err(e)),
            },
            // `symbol->string`/`intern`: the `symbol`<->`string` bridges.
            // `intern` reads the name in the module the way the reader would
            // there (`Names::read`), so it hands back the symbol a source
            // spelling of it in that module is, creating one only when the
            // module sees none — CL's `intern`.
            wk::F_SYMBOL_TO_STRING => Some(match args[0] {
                Value::Symbol(id) => {
                    let name = id.name().to_string();
                    Ok(str_rt(heap, name))
                }
                _ => Err(EvalError::Panic("symbol->string: not a symbol".into())),
            }),
            wk::INTERN => Some((|| {
                let name = rt_str(heap, &args[0])?;
                let module = rt_str(heap, &args[1])?;
                let ns = typelisp_mem::symbols::ns_named(&module)
                    .ok_or_else(|| EvalError::Panic(format!("intern: no module named `{}`", module)))?;
                Ok(Value::Symbol(heap.names().read(ns, &name)))
            })()),
            wk::F_KEYWORDP => Some(match args[0] {
                Value::Symbol(id) => Ok(Value::Bool(id.is_keyword())),
                _ => Err(EvalError::Panic("keywordp: not a symbol".into())),
            }),
            // `cons`/`car`/`cdr`/`set-car`/`set-cdr` are no longer `Sexpr`
            // builtins (Symbol/Sexpr redesign Phase 4b): `cons`/`car`/`cdr` are
            // the `cons<T,U>` pair (a prelude `defun` + `cons-cell` accessor
            // methods, run as ordinary user code), and `set-car`/`set-cdr` were
            // dropped. `Sexpr` cons/nil operations live in the `sexpr-*` layer
            // below.
            // Internal `Sexpr` navigation layer (Symbol/Sexpr redesign Phase 1):
            // `sexpr-cons`/`sexpr-car`/`sexpr-cdr` are the island's own aliases
            // for the identical heap operations `cons`/`car`/`cdr` perform, kept
            // separate so Phase 4 can repurpose the user-facing names to a
            // generic `cons<T,U>` pair. `sexpr-consp`/`sexpr-null`/`sexpr-atom`
            // read the runtime tag directly (no `match`), so they survive
            // Phase 5's `match`-to-enum fence.
            wk::F_SEXPR_CONS => match (args.first(), args.get(1)) {
                (Some(a), Some(b)) => {
                    // core-build-ok: this *is* `cons`, over user data; both
                    // arguments are already rooted as evaluated call arguments.
                    Some(heap.cons(*a, *b).map_err(|e| EvalError::Panic(e.to_string())))
                }
                _ => Some(Err(EvalError::Internal("sexpr-cons: expected two Sexpr arguments".into()))),
            },
            // `car`/`cdr` of the empty list is the empty list, as in CL —
            // `(car nil)` is `nil` there, and here both the argument and the
            // result are `Option<Sexpr>`, so "the empty list" is `None` on
            // each side. Only a non-empty non-cons (an int, a string, ...)
            // is still an error: that is a genuine type confusion, where the
            // empty list is an ordinary end of a list walk.
            wk::F_SEXPR_CAR => match args.first() {
                Some(v) if v.is_empty() => Some(Ok(Value::Empty)),
                Some(v) => {
                    Some(heap.car(*v).map_err(|_| EvalError::Panic("sexpr-car: not a cons".into())))
                }
                None => Some(Err(EvalError::Internal("sexpr-car: expected one argument".into()))),
            },
            wk::F_SEXPR_CDR => match args.first() {
                Some(v) if v.is_empty() => Some(Ok(Value::Empty)),
                Some(v) => {
                    Some(heap.cdr(*v).map_err(|_| EvalError::Panic("sexpr-cdr: not a cons".into())))
                }
                None => Some(Err(EvalError::Internal("sexpr-cdr: expected one argument".into()))),
            },
            wk::F_SEXPR_CONSP => match args.first() {
                Some(v) => Some(Ok(Value::Bool(v.is_cons()))),
                None => Some(Err(EvalError::Internal("sexpr-consp: expected one argument".into()))),
            },
            wk::F_SEXPR_NULL => match args.first() {
                Some(v) => Some(Ok(Value::Bool(v.is_empty()))),
                None => Some(Err(EvalError::Internal("sexpr-null: expected one argument".into()))),
            },
            wk::F_SEXPR_ATOM => match args.first() {
                Some(v) => Some(Ok(Value::Bool(!v.is_cons()))),
                None => Some(Err(EvalError::Internal("sexpr-atom: expected one argument".into()))),
            },
            // Internal `Sexpr` payload extractors (Symbol/Sexpr redesign Phase 2):
            // the island's own typed field readers, moved out of `compiler.rs`'s
            // `match`-based typelisp defuns so the island stops depending on the
            // user-facing `match` (Phase 5 fences `match` to enum-only). Each
            // reads the runtime `Value` payload directly, mirroring exactly what
            // `match_sexpr_ctor`'s corresponding arm binds — panicking (not
            // `None`-matching) on a tag mismatch, the same contract the old
            // `(_ (panic ...))` catch-all arms had.
            // `sexpr-int`: an `int` node is its own value — a fixnum or a
            // bignum box — so the read is the identity, after the check that
            // it is one (and canonical).
            wk::F_SEXPR_INT => match args.first() {
                Some(v @ Value::Int(_)) | Some(v @ Value::Boxed(_)) => Some(expect_int(heap, v).map(|_| *v).map_err(|_| {
                    EvalError::Panic("sexpr-int: expected an int Sexpr node".into())
                })),
                Some(_) => Some(Err(EvalError::Panic("sexpr-int: expected an int Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-int: expected a Sexpr argument".into()))),
            },
            // `sexpr-i8`/`sexpr-i16`/`sexpr-u8`/`sexpr-u16`/`sexpr-u32`/
            // `sexpr-i32`: the fixed widths' readers. Each accepts only its own
            // box, for exactly the reason `sexpr-f64` refuses an `f32` one —
            // answering with a width the caller did not ask for is the loss
            // these variants exist to prevent. The result is the plain
            // normalized word, because that is what a statically typed `u8`
            // is everywhere outside a `Sexpr`.
            wk::F_SEXPR_I8 | wk::F_SEXPR_I16 | wk::F_SEXPR_U8 | wk::F_SEXPR_U16 | wk::F_SEXPR_U32 | wk::F_SEXPR_I32 => {
                let (width, signed) = match name.well_known() {
                    wk::F_SEXPR_I8 => (8u8, true),
                    wk::F_SEXPR_I16 => (16, true),
                    wk::F_SEXPR_U8 => (8, false),
                    wk::F_SEXPR_U16 => (16, false),
                    wk::F_SEXPR_U32 => (32, false),
                    _ => (32, true),
                };
                match args.first() {
                    Some(Value::Boxed(id)) if heap.is_narrow(*id, width, signed) => {
                        Some(Ok(Value::Int(heap.narrow_box(*id).expect("just tested").value)))
                    }
                    Some(_) => Some(Err(EvalError::Panic(format!("{name}: expected a {} Sexpr node", &name.name()[6..])))),
                    _ => Some(Err(EvalError::Internal(format!("{name}: expected a Sexpr argument")))),
                }
            }
            wk::F_SEXPR_BOOL => match args.first() {
                Some(Value::Bool(b)) => Some(Ok(Value::Bool(*b))),
                Some(_) => Some(Err(EvalError::Panic("sexpr-bool: expected a Bool Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-bool: expected a Sexpr argument".into()))),
            },
            // `sexpr-char`: peer of `sexpr-i32`/`sexpr-bool` for a `Char` node
            // (`Value::Char`, an ordinary immediate, unlike `Float`'s boxed
            // payload) — added alongside `compile-char`/the `char` dispatch tag
            // (compiled code previously had no way to build/read a bare `char`
            // literal at all, an oversight discovered while implementing
            // `quote`, whose `Char` leaf needs exactly this).
            wk::F_SEXPR_CHAR => match args.first() {
                Some(Value::Char(c)) => Some(Ok(Value::Char(*c))),
                Some(_) => Some(Err(EvalError::Panic("sexpr-char: expected a Char Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-char: expected a Sexpr argument".into()))),
            },
            // `sexpr-f64`/`sexpr-f32`: peers of `sexpr-i32` for a float node (heap-boxed,
            // `Value::Boxed` — see `BoxedObj`). Added with the Phase 5 `match`
            // fence so a `Sexpr::f64` payload can still be read out without a
            // `(match s ((Float f) f) ..)`.
            // One accessor per width, and each accepts only its own box. A
            // single `sexpr-float` would have to widen an `f32` to answer
            // with the `f64` its return type promised, which is the width
            // being thrown away at exactly the point the value is read.
            wk::F_SEXPR_F64 => match args.first() {
                // The node *is* the float box since the scalar
                // unification, so reading the payload out is the identity.
                Some(v @ Value::Boxed(id)) if heap.is_f64(*id) => Some(Ok(*v)),
                Some(_) => Some(Err(EvalError::Panic("sexpr-f64: expected an f64 Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-f64: expected a Sexpr argument".into()))),
            },
            wk::F_SEXPR_F32 => match args.first() {
                Some(v @ Value::Boxed(id)) if heap.is_f32(*id) => Some(Ok(*v)),
                Some(_) => Some(Err(EvalError::Panic("sexpr-f32: expected an f32 Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-f32: expected a Sexpr argument".into()))),
            },
            wk::F_SEXPR_STR => match args.first() {
                // The node *is* the heap string since the scalar
                // unification, so reading the payload out is the identity.
                Some(v @ Value::Str(_)) => Some(Ok(*v)),
                Some(_) => Some(Err(EvalError::Panic("sexpr-str: expected a Str Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-str: expected a Sexpr argument".into()))),
            },
            // `(Sym v)` binds `v : Symbol`, then `symbol->string` reads its name;
            // this fuses the two, matching the old defun `(symbol->string v)`.
            wk::F_SEXPR_SYM_NAME => match args.first() {
                Some(Value::Symbol(id)) => {
                    let name = id.name().to_string();
                    Some(Ok(str_rt(heap, name)))
                }
                Some(_) => Some(Err(EvalError::Panic("sexpr-sym-name: expected a Sym Sexpr node".into()))),
                _ => Some(Err(EvalError::Internal("sexpr-sym-name: expected a Sexpr argument".into()))),
            },
            // `sexpr-symp`: the tag predicate a `match (car x) ((Sym s) ...) (_ ...))`
            // with a *non-panic* fallback rewrites to (`form-is-borrowed?` in
            // `compiler.rs`) — a peer of `sexpr-consp`/`sexpr-null`/`sexpr-atom`,
            // reading the tag directly.
            wk::F_SEXPR_SYMP => match args.first() {
                Some(v) => Some(Ok(Value::Bool(matches!(v, Value::Symbol(_))))),
                None => Some(Err(EvalError::Internal("sexpr-symp: expected one argument".into()))),
            },
            _ => None,
        }
    }
}

/// The width and signedness of `type_name`, when it is one of the integer
/// types — every one of which shares the same catalog and the same runtime
/// carrier (`Value::Int`, an `i64`) but *not* the same arithmetic: the pair
/// this returns is what `eval_int_builtin` cuts every result back to. `None`
/// for anything that is not an integer type.
fn int_receiver_width(type_name: &Path) -> Option<(u32, bool)> {
    crate::types::scalar_type(type_name).and_then(|t| crate::types::int_width_signed(&t))
}

/// [`int_receiver_width`]'s float counterpart: `f32` and `f64` are one
/// `Value::Float` (an `f64`) apart from which type labels them.
fn is_float_receiver(type_name: &Path) -> bool {
    crate::types::scalar_type(type_name).is_some_and(|t| t.is_float())
}

/// Dispatch a built-in type's method — a `defmethod` with no typelisp body, so
/// the implementation lives here rather than in the registry. `assoc`'s eval
/// arm tries the program's own `defmethod`s first and falls back to this.
///
/// Argument count and types are trusted (the checker validated them against
/// the methods' signatures), so arms index `args` directly rather than
/// re-checking shape.
///
/// The one type it is handed is `ret_type`, the result's runtime identity, for
/// the builders that make a new container and have no field to read an
/// instantiation off. Reading a stored value back needs none: with one value
/// world a stored field *is* the value, so the container methods that return
/// `Option<V>` (`HashTable::get`/`remove`, `Vector::pop`, `Scope::get`) decode
/// nothing and need not know `V`.
pub(crate) fn eval_builtin_method(
    heap: &mut Heap,
    type_name: &Path,
    method: SymRef,
    args: &[Value],
    ret_type: TypeId,
) -> Option<Result<Value, EvalError>> {
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::HASHTABLE) {
        return match method.well_known() {
            // `ret_type` is the *result's* runtime identity, spelled by the
            // checker (`typeck::method`): these builders have no field to
            // read an instantiation off, and `Vector<i32>` and `Vector<i64>`
            // share a representation, so nothing here could derive it.
            wk::M_NEW => Some(Ok(crate::type_key::alloc_hashtable_of(heap, ret_type, args[0], args[1]))),
            wk::M_HASH_FUNCTION => Some(expect_struct_box(&args[0]).map(|id| heap.hashtable_functions(id)[0])),
            wk::M_TEST_FUNCTION => Some(expect_struct_box(&args[0]).map(|id| heap.hashtable_functions(id)[1])),
            // `get`/`set`/`remove` are prelude methods, written on these
            // five — see `HashTable`'s definition (`typeck::builtins`).
            wk::M_BUCKET_COUNT => Some(hashtable_bucket_count(heap, args)),
            wk::M_BUCKET_KEY => Some(hashtable_bucket_key(heap, args)),
            wk::M_BUCKET_VALUE => Some(hashtable_bucket_value(heap, args)),
            wk::M_BUCKET_PUT => Some(hashtable_bucket_put(heap, args)),
            wk::M_BUCKET_DELETE => Some(hashtable_bucket_delete(heap, args)),
            wk::M_COUNT => Some(hashtable_count(heap, args)),
            wk::M_CLEAR => Some(hashtable_clear(heap, args)),
            wk::M_KEYS => Some(hashtable_keys(heap, args, ret_type)),
            wk::M_VALUES => Some(hashtable_values(heap, args, ret_type)),
            wk::M_ENTRIES => Some(hashtable_entries(heap, args, ret_type)),
            _ => None,
        };
    }
    // `join` is intercepted before this, as `Task<T>::wait` is.
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::THREAD_TYPE) {
        return match method.well_known() {
            wk::M_CURRENT_ID => Some(Ok(Value::Int(typelisp_rt::sys_builtin::thread_current_id()))),
            wk::M_AVAILABLE_PARALLELISM => {
                Some(typelisp_rt::sys_builtin::thread_available_parallelism().map(Value::Int).map_err(EvalError::Panic))
            }
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::VECTOR) {
        return match method.well_known() {
            // type-identity-ok: the built-in `Vector`, a root name spelled in full
            wk::M_NEW => Some(Ok(crate::type_key::alloc_struct_of(heap, ret_type, Vec::new()))),
            wk::M_PUSH => Some(vector_push(heap, args)),
            wk::M_GET => Some(vector_get(heap, args)),
            wk::SET => Some(vector_set(heap, args)),
            wk::M_LEN => Some(vector_len(heap, args)),
            wk::M_POP => Some(vector_pop(heap, args, ret_type)),
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::SCOPE) {
        // One representation: every `Scope<V>` is a `StructPayload::Frames`
        // heap box, and the element needs no encode or decode, so `V` never
        // enters into it — see this function's doc comment.
        if method.is(wk::M_NEW) {
            return Some(Ok(heap.alloc_scope()));
        }
        return match method.well_known() {
            wk::M_CLONE_FRAMES => Some(scope_clone_frames_heap(heap, args)),
            wk::M_PUSH_FRAME => Some(scope_push_frame_heap(heap, args)),
            wk::M_POP_FRAME => Some(scope_pop_frame_heap(heap, args)),
            wk::M_GET => Some(scope_get_heap(heap, args, ret_type)),
            wk::SET => Some(scope_set_heap(heap, args)),
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::STRING) {
        return match method.well_known() {
            wk::M_UPCASE => Some(expect_str(heap, &args[0]).map(|s| s.to_ascii_uppercase()).map(|s| str_rt(heap, s))),
            wk::M_DOWNCASE => Some(expect_str(heap, &args[0]).map(|s| s.to_ascii_lowercase()).map(|s| str_rt(heap, s))),
            wk::M_LENGTH => Some(string_length(heap, args)),
            wk::M_REF => Some(string_ref(heap, args)),
            wk::M_SUBSTRING => Some(string_substring(heap, args)),
            wk::M_APPEND => Some(string_append(heap, args)),
            wk::M_LT => Some(string_lt(heap, args)),
            wk::M_OP_LT | wk::M_OP_LE | wk::M_OP_GT | wk::M_OP_GE => Some(string_compare(heap, method, args)),
            // `eq`/`eql`: true identity (`Rc::ptr_eq` — see the string table's
            // doc comment). `equal`/`equalp`: content comparison, the
            // (case-sensitive/-insensitive) CL predicates a naive "string
            // equality" actually means — see `registry::string_assoc`'s doc
            // comment and `docs/cl-equivalence-catalog.md`'s eq/eql/equal/
            // equalp section.
            wk::M_EQ | wk::M_EQL => Some(string_identity_eq(args)),
            wk::M_EQUAL => Some(string_content_eq(heap, args)),
            wk::M_EQUALP => Some(string_content_eqp(heap, args)),
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::CHAR) {
        return match method.well_known() {
            wk::M_UPCASE => Some(expect_char(&args[0]).map(|c| Value::Char(c.to_ascii_uppercase()))),
            wk::M_DOWNCASE => Some(expect_char(&args[0]).map(|c| Value::Char(c.to_ascii_lowercase()))),
            wk::M_LT => Some(char_lt(args)),
            wk::M_OP_LT | wk::M_OP_LE | wk::M_OP_GT | wk::M_OP_GE => Some(char_compare(method, args)),
            wk::M_ALPHAP => Some(expect_char(&args[0]).map(|c| Value::Bool(c.is_ascii_alphabetic()))),
            wk::M_DIGITP => Some(expect_char(&args[0]).map(|c| Value::Bool(c.is_ascii_digit()))),
            // `eq`/`eql`/`equal` all coincide (immediate scalar, and CL's
            // own `equal` on characters is defined to be `eql`); `equalp`
            // is case-insensitive (see `registry::char_assoc`'s doc comment).
            wk::M_EQ | wk::M_EQL | wk::M_EQUAL => Some(char_eq(args)),
            wk::M_EQUALP => Some(char_eqp(args)),
            wk::M_CHAR_TO_STRING => Some(expect_char(&args[0]).map(|c| c.to_string()).map(|s| str_rt(heap, s))),
            wk::M_CHAR_TO_INT => Some(char_to_int(args)),
            _ => None,
        };
    }
    if let Some((width, signed)) = int_receiver_width(type_name) {
        return match method.well_known() {
            wk::M_OP_ADD | wk::M_OP_SUB | wk::M_OP_MUL | wk::M_OP_DIV | wk::M_MOD | wk::M_OP_LT | wk::M_OP_LE | wk::M_OP_GT | wk::M_OP_GE | wk::RETIRED_EQUALS | wk::M_OP_NE | wk::M_MAX | wk::M_MIN | wk::M_LOGAND | wk::M_LOGIOR | wk::M_LOGXOR | wk::M_ASH | wk::M_LOGBITP | wk::M_LOGTEST => eval_int_builtin(method, args, width, signed),
            wk::M_LOGNOT => Some(int_lognot(args, width, signed)),
            wk::M_LOGCOUNT => Some(int_logcount(args, width)),
            wk::M_INTEGER_LENGTH => Some(int_integer_length(args, width)),
            // `eq`/`eql`/`equal`/`equalp` are all registered as aliases for
            // `=` (see `registry::int_assoc`'s doc comment for why every one
            // of these four is meaningful to register even though none can
            // diverge from `=` here).
            wk::M_EQ | wk::M_EQL | wk::M_EQUAL | wk::M_EQUALP => eval_int_builtin(typelisp_mem::symbols::well_known_symbol(wk::RETIRED_EQUALS), args, width, signed),
            wk::M_INT_TO_FLOAT => Some(int_to_float(heap, args)),
            wk::M_INT_TO_CHAR => Some(int_to_char(args)),
            wk::M_TRY_INT_TO_CHAR => Some(try_int_to_char(heap, args, ret_type)),
            wk::M_INT_TO_INT => Some(width_to_int(heap, args, width, signed)),
            wk::M_INT_TO_RATIO => Some(int_to_ratio(heap, args)),
            _ if width_cast_target(method).is_some_and(|(try_variant, _, _)| !try_variant) => {
                let (_, w, sg) = width_cast_target(method).expect("just matched");
                Some(int_to_width(args, w, sg))
            }
            _ if width_cast_target(method).is_some_and(|(try_variant, _, _)| try_variant) => {
                let (_, w, sg) = width_cast_target(method).expect("just matched");
                Some(try_int_to_width(heap, args, w, sg, ret_type))
            }
            _ => None,
        };
    }
    if is_float_receiver(type_name) {
        // `f32` is binary32 and `f64` binary64: the receiver's own width says
        // what each result is rounded to (`float_at`).
        let single = crate::types::path_is_builtin(type_name, typelisp_mem::wk::F32);
        return match method.well_known() {
            wk::M_OP_ADD | wk::M_OP_SUB | wk::M_OP_MUL | wk::M_OP_DIV | wk::M_OP_LT | wk::M_OP_LE | wk::M_OP_GT | wk::M_OP_GE | wk::RETIRED_EQUALS | wk::M_OP_NE | wk::M_MAX | wk::M_MIN => {
                eval_float_builtin(heap, method, args, single)
            }
            wk::M_EQ | wk::M_EQL | wk::M_EQUAL | wk::M_EQUALP => eval_float_builtin(heap, typelisp_mem::symbols::well_known_symbol(wk::RETIRED_EQUALS), args, single),
            wk::M_FLOAT_TO_F32 => Some(float_to_width(heap, args, true)),
            wk::M_FLOAT_TO_F64 => Some(float_to_width(heap, args, false)),
            wk::M_TRY_FLOAT_TO_F32 => Some(try_float_to_f32(heap, args, ret_type)),
            // Always `Some`: widening is exact.
            wk::M_TRY_FLOAT_TO_F64 => Some(float_to_width(heap, args, false).map(|v| {
                let some = Some(v);
                heap.alloc_option(ret_type, some)
            })),
            wk::M_EXPT => Some(float_expt(heap, args, single)),
            wk::M_SQRT => Some(float_unary(heap, args, f64::sqrt, single)),
            wk::M_FLOOR => Some(float_unary(heap, args, f64::floor, single)),
            wk::M_CEILING => Some(float_unary(heap, args, f64::ceil, single)),
            wk::M_ROUND => Some(float_unary(heap, args, f64::round_ties_even, single)),
            wk::M_TRUNCATE => Some(float_unary(heap, args, f64::trunc, single)),
            wk::M_SIN => Some(float_unary(heap, args, f64::sin, single)),
            wk::M_COS => Some(float_unary(heap, args, f64::cos, single)),
            wk::M_TAN => Some(float_unary(heap, args, f64::tan, single)),
            wk::M_ASIN => Some(float_unary(heap, args, f64::asin, single)),
            wk::M_ACOS => Some(float_unary(heap, args, f64::acos, single)),
            wk::M_ATAN => Some(float_unary(heap, args, f64::atan, single)),
            wk::M_SINH => Some(float_unary(heap, args, f64::sinh, single)),
            wk::M_COSH => Some(float_unary(heap, args, f64::cosh, single)),
            wk::M_TANH => Some(float_unary(heap, args, f64::tanh, single)),
            wk::M_ASINH => Some(float_unary(heap, args, f64::asinh, single)),
            wk::M_ACOSH => Some(float_unary(heap, args, f64::acosh, single)),
            wk::M_ATANH => Some(float_unary(heap, args, f64::atanh, single)),
            wk::M_EXP => Some(float_unary(heap, args, f64::exp, single)),
            wk::M_LOG => Some(float_unary(heap, args, f64::ln, single)),
            wk::M_FLOAT_TO_INT => Some(float_to_int(heap, args)),
            wk::M_FLOAT_TO_RATIO => Some(float_to_ratio(heap, args)),
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::INT) {
        return match method.well_known() {
            wk::M_OP_ADD | wk::M_OP_SUB | wk::M_OP_MUL | wk::M_TRUNCATE | wk::M_MOD | wk::M_OP_LT | wk::M_OP_LE | wk::M_OP_GT | wk::M_OP_GE | wk::RETIRED_EQUALS | wk::M_OP_NE | wk::M_MAX | wk::M_MIN | wk::M_LOGAND | wk::M_LOGIOR | wk::M_LOGXOR | wk::M_ASH | wk::M_LOGBITP | wk::M_LOGTEST => eval_integer_builtin(heap, method, args),
            // CL's `/`: the `rational` quotient.
            wk::M_OP_DIV => eval_rational_builtin(heap, method, args),
            wk::M_LOGNOT | wk::M_LOGCOUNT | wk::M_INTEGER_LENGTH => Some(integer_unary(heap, args, method)),
            wk::M_EQ | wk::M_EQL | wk::M_EQUAL | wk::M_EQUALP => eval_integer_builtin(heap, typelisp_mem::symbols::well_known_symbol(wk::RETIRED_EQUALS), args),
            wk::M_INT_TO_FLOAT | wk::M_INT_TO_RATIO | wk::M_INT_TO_CHAR | wk::M_TRY_INT_TO_CHAR | wk::M_INT_TO_INT => {
                Some(integer_conversion(heap, method, args, ret_type))
            }
            _ if width_cast_target(method).is_some() => {
                let (try_variant, w, sg) = width_cast_target(method).expect("just matched");
                Some(integer_to_width(heap, args, w, sg, try_variant, ret_type))
            }
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::RATIO) {
        return match method.well_known() {
            wk::M_OP_ADD | wk::M_OP_SUB | wk::M_OP_MUL | wk::M_OP_DIV | wk::M_OP_LT | wk::M_OP_LE | wk::M_OP_GT | wk::M_OP_GE | wk::RETIRED_EQUALS | wk::M_OP_NE | wk::M_MAX | wk::M_MIN => {
                eval_ratio_builtin(heap, method, args)
            }
            wk::M_EQ | wk::M_EQL | wk::M_EQUAL | wk::M_EQUALP => eval_ratio_builtin(heap, typelisp_mem::symbols::well_known_symbol(wk::RETIRED_EQUALS), args),
            wk::M_RATIO_TO_INT => Some(ratio_to_int(heap, args)),
            wk::M_RATIO_TO_FLOAT => Some(ratio_to_float(heap, args)),
            wk::M_NUMERATOR => Some(ratio_numerator(heap, args)),
            wk::M_DENOMINATOR => Some(ratio_denominator(heap, args)),
            wk::M_RATIO_TO_RATIONAL => Some(rational_unary(heap, method, args, ret_type)),
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::REPR_RATIONAL) {
        return match method.well_known() {
            wk::M_OP_ADD | wk::M_OP_SUB | wk::M_OP_MUL | wk::M_OP_DIV | wk::M_OP_LT | wk::M_OP_LE | wk::M_OP_GT | wk::M_OP_GE | wk::RETIRED_EQUALS | wk::M_OP_NE | wk::M_MAX | wk::M_MIN => {
                eval_rational_builtin(heap, method, args)
            }
            wk::M_EQ | wk::M_EQL | wk::M_EQUAL | wk::M_EQUALP => eval_rational_builtin(heap, typelisp_mem::symbols::well_known_symbol(wk::RETIRED_EQUALS), args),
            wk::M_RATIONAL_TO_INT | wk::M_TRY_RATIONAL_TO_INT | wk::M_RATIONAL_TO_RATIO | wk::M_RATIONAL_TO_FLOAT | wk::M_NUMERATOR | wk::M_DENOMINATOR => Some(rational_unary(heap, method, args, ret_type)),
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::BOOL) {
        return match method.well_known() {
            wk::M_EQ | wk::M_EQL | wk::M_EQUAL | wk::M_EQUALP => Some(bool_eq(args)),
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::SYMBOL) {
        // A `Symbol` value shares the `Value::Symbol(id)` carrier of a
        // `Sexpr::Sym`, so `eq`/`eql` reuse the `Sexpr` comparisons (interned
        // id identity — same name => same id => `eq`).
        return match method.well_known() {
            wk::M_EQ => Some(sexpr_eq(heap, args)),
            wk::M_EQL => Some(sexpr_eql(heap, args)),
            _ => None,
        };
    }
    if crate::types::path_is_builtin(type_name, typelisp_mem::wk::SEXPR) {
        return match method.well_known() {
            // `eq`/`eql` now diverge, as anticipated by this arm's own prior
            // history (see `sexpr_eql`'s doc comment): `Sexpr::f64` became
            // heap-boxed (`Value::Boxed`, see `BoxedObj`) for the `Sexpr`/
            // `RtValue` unification plan, the "boxed numeric representation"
            // this comment used to say didn't exist yet. `equal` is CL's
            // `equal`, the method `impl Equal Sexpr` takes as the trait's.
            wk::M_EQ => Some(sexpr_eq(heap, args)),
            wk::M_EQL => Some(sexpr_eql(heap, args)),
            wk::M_EQUAL => Some(sexpr_equal(heap, args)),
            _ => None,
        };
    }
    eval_llvm_builtin_method(heap, type_name, method, args)
}

impl Interp {
    /// The `stream-*` / `file-*` built-ins: a thin adapter over
    /// [`typelisp_rt::stream_builtin::stream_builtin`], which is
    /// the implementation — and is also what the compiled `rt_stream_*` shims
    /// call, so an interpreted and a compiled call to `read-char` are the same
    /// code reading the same stream table.
    ///
    /// Every one of those is deliberately dumb. The CL-shaped surface —
    /// `open`'s `&key` arguments, the `with-...` macros, `read-line`'s
    /// end-of-input convention, and every composite stream — is written in
    /// typelisp on top of the `Stream`/`InputStream`/`OutputStream` traits,
    /// where a default method body can express it once for all
    /// implementations. Nothing down there knows those traits exist.
    ///
    /// Fallible operations return `Result<_, FileError>` values; the `Err`
    /// arm here is something else entirely — an argument of the wrong shape,
    /// which the checker rules out, so it can only mean an internal bug.
    fn eval_stream_builtin(
        &self,
        heap: &mut Heap,
        name: SymRef,
        args: &[Value],
    ) -> Option<Result<Value, EvalError>> {
        Some(match typelisp_rt::stream_builtin::stream_builtin(heap, name, args)? {
            Ok(v) => Ok(v),
            Err(e) => Err(EvalError::Internal(e)),
        })
    }
}
