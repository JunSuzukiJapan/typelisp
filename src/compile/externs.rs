//! Which Rust function a builtin call lowers to, and how the two tiers find
//! it.
//!
//! Three tables, all of them backend concerns even though they name no LLVM
//! type:
//!
//! - [`rt_builtin_symbol`]: the `rt_*` shim a *free* builtin lowers to. Read
//!   by `compile::symbols::callee_symbol_name` when it decides whether to
//!   mangle a name, and by the driver when it decides whether a call target
//!   needs compiling first.
//! - [`native_lowered_primitive_methods`]: which builtin *methods* on a
//!   primitive receiver the island lowers in place rather than calling.
//! - [`rt_extern_functions`]: every shim, with its address — the JIT's
//!   `add_global_mapping` list and the AOT module's forward-declaration list,
//!   which have to be the same list.
//!
//! They live here rather than in `eval::interp` because nothing in the
//! evaluator asks any of these questions: they are asked while *compiling*,
//! by `symbols`, `core_bridge`, `driver`, `aot` and the two bootstrappers.

/// The `typelisp_rt` shim a *free* builtin call lowers to, or
/// `None` for an ordinary function whose name gets mangled instead.
///
/// These builtins can never be `compile`d themselves (no typelisp body —
/// direct cons-heap or OS access, Rust-only), so [`Interp::compile_function`]
/// excludes them from its normal "every call target must already be compiled"
/// check ([`is_rt_builtin_name`]) and instead always wires them via
/// [`rt_extern_functions`]. The `sexpr-*` family is the island layer
/// (Symbol/Sexpr redesign Phase 4b — the whole family since closure
/// unification Stage 8, tag predicates and typed payload extractors included,
/// not just `car`/`cdr`/`cons`). The free `car`/`cdr`/`cons` names are the
/// `cons<T,U>` pair (an ordinary `defstruct` method / `defun`, compiled the
/// normal way), not `rt_*` shims.
///
/// **One table, on purpose.** The renaming used to happen twice: `core_bridge`
/// decided whether to mangle a name, and `compile-call` in the island's SOURCE
/// re-derived the shim it stood for, from a 15-deep nested `if`. Two lists
/// that had to agree — and the failure mode of a disagreement is the worst one
/// this compiler has: the island's `get-function` **aborts the process** on a
/// name it cannot find. Naming the shim here, where the "is it a builtin?"
/// decision already lives, leaves the island nothing to re-derive; it calls
/// whatever the bridge named.
pub(crate) fn rt_builtin_symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "sexpr-car" => "rt_car",
        "sexpr-cdr" => "rt_cdr",
        "sexpr-cons" => "rt_cons",
        "sexpr-consp" => "rt_consp",
        "sexpr-null" => "rt_null",
        "sexpr-atom" => "rt_atom",
        "sexpr-symp" => "rt_symp",
        "sexpr-int" => "rt_sexpr_int",
        "sexpr-i32" => "rt_sexpr_i32",
        "sexpr-bool" => "rt_sexpr_bool",
        "sexpr-char" => "rt_sexpr_char",
        "sexpr-f64" => "rt_f64_value",
        "sexpr-f32" => "rt_f32_value",
        // One shim per narrow width, not one taking the width as an operand:
        // a builtin accessor is called with exactly its own arguments, and
        // the type it reads is in its name.
        "sexpr-i8" => "rt_sexpr_i8",
        "sexpr-i16" => "rt_sexpr_i16",
        "sexpr-u8" => "rt_sexpr_u8",
        "sexpr-u16" => "rt_sexpr_u16",
        "sexpr-u32" => "rt_sexpr_u32",
        "sexpr-str" => "rt_sexpr_str",
        "sexpr-sym-name" => "rt_sym_name",
        // `symbol->string` is `sexpr-sym-name` under the checker's `symbol`
        // type rather than `sexpr`'s — the same shim, since both spellings
        // carry the same interned `Value::Symbol`.
        "symbol->string" => "rt_sym_name",
        // `string->symbol` is the other direction, and is likewise a shim
        // that already existed: `rt_intern_symbol` is what a compiled quoted
        // symbol literal already goes through, and interning a `Str` is the
        // whole of this builtin.
        "string->symbol" => "rt_intern_symbol",
        // The small system builtins (`sys_builtin`): parsing, the clock, and
        // process exit. Each is a thin edge over one shared implementation
        // the interpreter calls too.
        // `equal`/`equalp` on `Sexpr` are free *functions*
        // (`registry`'s root `fns`), not methods, so they map here rather
        // than through `native_lowered_primitive_methods`. Both are the same
        // `typelisp_rt::equality` the interpreter calls.
        "equal" => "rt_sexpr_equal",
        "equalp" => "rt_sexpr_equalp",
        "parse-int" => "rt_parse_int",
        "parse-float" => "rt_parse_float",
        "get-universal-time" => "rt_get_universal_time",
        "get-internal-real-time" => "rt_get_internal_real_time",
        // The environment and the running implementation (CLHS 25.1, plus
        // `command-line-args`/`getenv`, which CL has no equivalent of).
        "command-line-args" => "rt_command_line_args",
        "getenv" => "rt_getenv",
        "home-directory" => "rt_home_directory",
        "lisp-implementation-version" => "rt_lisp_implementation_version",
        "machine-type" => "rt_machine_type",
        "software-type" => "rt_software_type",
        // The four CLHS 25.1 names that ask the running host, and the two
        // time-zone primitives under `decode-`/`encode-universal-time`.
        "get-internal-run-time" => "rt_get_internal_run_time",
        "machine-instance" => "rt_machine_instance",
        "machine-version" => "rt_machine_version",
        "software-version" => "rt_software_version",
        "timezone-offset-seconds" => "rt_timezone_offset_seconds",
        "timezone-daylight-p" => "rt_timezone_daylight_p",
        "exit" => "rt_exit",
        // `read` (`typelisp_read::shim`), which could not lower while the
        // reader was a module of this crate: a shim naming it would have had
        // to reach up into `typelisp`, which depends on the runtime rather
        // than the other way round.
        "read" => "rt_read",
        "read-datum-at" => "rt_read_datum_at",
        // `eval` (`typelisp_front::shim`), the last of the six builtins that
        // used to make a caller uncompilable. Its implementation is the
        // checker and the interpreter, which is why they are a crate below
        // this one — see that module's doc comment for how the shim finds an
        // environment on each of the two paths.
        "eval" => "rt_eval",
        // `macroexpand-1`/`macroexpand` reach the same two environments
        // `eval` does, and for the same reason: expanding a macro means
        // running its body, which is the interpreter.
        "macroexpand-1" => "rt_macroexpand_1",
        "macroexpand" => "rt_macroexpand",
        // The printing family. `format`/`print`/`println`/`pprint` and
        // `pprint-logical-block` are special forms; the names here are what
        // the checker lowered them to (`Checker::check_format`/
        // `check_print_like`/`check_pprint`/`check_pprint_logical_block`).
        // The last four are ordinary builtins (`registry`'s root `fns`).
        // All eleven run `typelisp_print::runtime::print_builtin`, the same
        // implementation `Interp::eval_builtin` runs.
        "format-rt" => "rt_format",
        "print-rt" => "rt_print",
        "println-rt" => "rt_println",
        "pprint-rt" => "rt_pprint",
        "pprint-block-start-rt" => "rt_pprint_block_start",
        "pprint-block-end-rt" => "rt_pprint_block_end",
        "pprint-newline" => "rt_pprint_newline",
        "pprint-indent" => "rt_pprint_indent",
        "pprint-tab" => "rt_pprint_tab",
        "pprint-pop" => "rt_pprint_pop",
        "pprint-list-exhausted" => "rt_pprint_list_exhausted",
        // The `random-state` builtins. Nothing about a random-state lives
        // outside the heap, so all four lower.
        "random-state-next" => "rt_random_state_next",
        "random-state-copy" => "rt_random_state_copy",
        "make-random-state-fresh" => "rt_make_random_state_fresh",
        "seed-random-state" => "rt_seed_random_state",
        // The stream/file builtins. Every one is a thin conversion around
        // `typelisp_rt::stream_builtin::stream_builtin`, which the
        // interpreter calls too — see `Interp::eval_stream_builtin`. They can
        // lower at all only because the stream *table* lives in
        // `typelisp-rt` (see `typelisp_rt::stream`'s module docs): an
        // AOT-linked executable has no interpreter to keep it on.
        "stream-stdin" => "rt_stream_stdin",
        "stream-stdout" => "rt_stream_stdout",
        "stream-stderr" => "rt_stream_stderr",
        "stream-string-input" => "rt_stream_string_input",
        "stream-string-output" => "rt_stream_string_output",
        "stream-open-file" => "rt_stream_open_file",
        "stream-close" => "rt_stream_close",
        "stream-open-p" => "rt_stream_open_p",
        "stream-input-p" => "rt_stream_input_p",
        "stream-output-p" => "rt_stream_output_p",
        "stream-read-char" => "rt_stream_read_char",
        "stream-read-byte" => "rt_stream_read_byte",
        "stream-write-byte" => "rt_stream_write_byte",
        "stream-unread-char" => "rt_stream_unread_char",
        "set-macro-character" => "rt_set_macro_character",
        "get-macro-character" => "rt_get_macro_character",
        "set-dispatch-macro-character" => "rt_set_dispatch_macro_character",
        "get-dispatch-macro-character" => "rt_get_dispatch_macro_character",
        // **The `rt_suspend_` prefix is load-bearing**, and this table is the
        // only place that decides which builtins suspend: `core_bridge` reads
        // the prefix and emits a `(suspend ...)` node instead of a `(call
        // ...)`, so the island needs no list of its own.
        //
        // `sleep` used to lower to `rt_sleep`, which stops the *thread*. That
        // was the reason it could not be compiled at all: with tasks the same
        // source would mean two different things on the two sides, silently.
        "sleep" => "rt_suspend_sleep",
        "yield" => "rt_suspend_yield",
        // `(net-wait h interest)` parks the task on a socket: the third of
        // the free builtins that suspend, and the reason the prelude's
        // `socket-stream` methods can wait without any of them blocking.
        "net-wait" => "rt_suspend_io",
        "net-wait-for" => "rt_suspend_io_for",
        // The REPL tool layer's runtime half. `trace`/`untrace`/`step`/
        // `disassemble` have no row and never will — those are
        // interpreter-only forms, the same category `compile`/`compile-file`/
        // `dump` are in (docs/syntax.md §10). These three are ordinary
        // builtins: the heap statistics and the dribble sink are the
        // runtime's own, and launching an editor is a process call.
        "heap-info" => "rt_heap_info",
        "dribble-start" => "rt_dribble_start",
        "dribble-stop" => "rt_dribble_stop",
        "ed-open" => "rt_ed_open",
        "stream-listen" => "rt_stream_listen",
        "stream-position" => "rt_stream_position",
        "stream-write-string" => "rt_stream_write_string",
        "stream-at-line-start" => "rt_stream_at_line_start",
        "stream-finish-output" => "rt_stream_finish_output",
        "stream-take-output-string" => "rt_stream_take_output_string",
        "file-exists-p" => "rt_file_exists_p",
        "file-delete" => "rt_file_delete",
        "file-truename" => "rt_file_truename",
        "file-modified-date" => "rt_file_modified_date",
        "file-owner-name" => "rt_file_owner_name",
        "file-directory-p" => "rt_file_directory_p",
        "file-list-directory" => "rt_file_list_directory",
        "file-create-directories" => "rt_file_create_directories",
        "file-rename" => "rt_file_rename",
        "net-resolve-begin" => "rt_net_resolve_begin",
        "net-resolve-finish" => "rt_net_resolve_finish",
        "net-tls-start" => "rt_net_tls_start",
        "net-tls-handshake" => "rt_net_tls_handshake",
        "net-udp-bind" => "rt_net_udp_bind",
        "net-udp-send-to" => "rt_net_udp_send_to",
        "net-udp-recv" => "rt_net_udp_recv",
        "net-udp-last-sender" => "rt_net_udp_last_sender",
        "net-connect-begin" => "rt_net_connect_begin",
        "net-connect-finish" => "rt_net_connect_finish",
        "net-listen" => "rt_net_listen",
        "net-unix-connect" => "rt_net_unix_connect",
        "net-unix-listen" => "rt_net_unix_listen",
        "net-accept" => "rt_net_accept",
        "net-fill" => "rt_net_fill",
        "net-pop-byte" => "rt_net_pop_byte",
        "net-pop-char" => "rt_net_pop_char",
        "net-buffered-p" => "rt_net_buffered_p",
        "net-push-string" => "rt_net_push_string",
        "net-push-byte" => "rt_net_push_byte",
        "net-flush" => "rt_net_flush",
        "net-shutdown-write" => "rt_net_shutdown_write",
        "net-local-address" => "rt_net_local_address",
        "net-peer-address" => "rt_net_peer_address",
        _ => return None,
    })
}

/// Whether [`rt_builtin_symbol`] names a shim for `name` — the "this is not a
/// call target to compile" question, asked where the shim's own name is not
/// needed.
/// The shim for a **method** that suspends the running task, or `None`.
///
/// A second table because the free-function one is keyed by bare name and a
/// method needs its receiver type too. One row, and it is here rather than
/// spelled in `core_bridge` so that "which builtins suspend" stays a question
/// this module answers — the free functions are already decided by the
/// `rt_suspend_` prefix in [`rt_builtin_symbol`].
pub(crate) fn rt_suspend_method_symbol(type_local: &str, method: &str) -> Option<&'static str> {
    match (type_local, method) {
        ("task", "wait") => Some("rt_suspend_wait"),
        // Every channel operation, including the four that never wait: the
        // table of channels is the scheduler's, and the driver is the only
        // way to it (`typelisp_abi::call_state::SUSPEND_CHAN_NEW` says why
        // it cannot live anywhere lower).
        ("chan", "new") => Some("rt_suspend_chan_new"),
        ("chan", "len") => Some("rt_suspend_chan_len"),
        ("chan", "cap") => Some("rt_suspend_chan_cap"),
        ("chan", "close") => Some("rt_suspend_chan_close"),
        ("chan", "send") => Some("rt_suspend_chan_send"),
        ("chan", "recv") => Some("rt_suspend_chan_recv"),
        _ => None,
    }
}

/// The prefix every suspending shim's name carries, and the whole of how the
/// bridge tells one from an ordinary runtime entry point.
pub(crate) const RT_SUSPEND_PREFIX: &str = "rt_suspend_";

/// The one free suspending builtin whose answer is *typed*: `net-wait-for`
/// wakes with a `bool` (ready, or the clock ran out). The others wake with
/// unit. Named here, next to the prefix rule, so the bridge has one place to
/// ask what a free suspension answers with.
pub(crate) const RT_SUSPEND_BOOL_ANSWER: &str = "rt_suspend_io_for";

pub(crate) fn is_rt_builtin_name(name: &str) -> bool {
    rt_builtin_symbol(name).is_some()
}

/// Whether a builtin method on a primitive receiver (`i64`/`i32`/`char`/
/// `string`/`f64`/`bignum`/`ratio`) is one `compiler.rs`'s `compile-assoc`
/// lowers *natively* — to an inline LLVM instruction or an `rt_*` call —
/// rather than to an ordinary function call that would need the method
/// `compile`d as its own function first. The Rust-side twin of the island's
/// own `int-native-method?`/`string-native-method?`/`char-native-method?`/
/// `float-native-method?`/`bignum-native-method?`/`ratio-native-method?`
/// predicates (`compiler.rs`'s `SOURCE`); the lists must stay in lockstep,
/// the same way [`Interp::heap_repr_kind`] mirrors the checker's
/// `is_heap_repr`.
///
/// [`Interp::call_graph_edges`] needs this so it can reject — cleanly, up
/// front — a compile whose body calls a *non*-native primitive builtin
/// (`i64::int->char`, `char::equalp`, ...): those have no compiled lowering
/// *and* no function to link, so left to reach the island they hit its
/// `get-function` guard, which under the AOT-native island is a hard
/// `rt_llvm_call` process abort rather than a catchable error. Catching them
/// here keeps `(compile bad-fn)` a clean `EvalError` — the behavior the
/// interpreted island used to give from `get-function` directly.
pub(crate) fn is_native_lowered_primitive_method(type_local: &str, method: &str) -> bool {
    native_lowered_primitive_methods(type_local).contains(&method)
}

/// The methods [`is_native_lowered_primitive_method`] answers `true` for, as
/// data.
///
/// A table rather than a `matches!` so the set can be *read*, not only
/// queried: `native_method_list_tests` compares it against the island's own
/// predicates in both directions, and the direction that matters — a method
/// Rust claims and the island does not lower — is unaskable of a predicate,
/// because there is nothing to enumerate. That gap is not hypothetical; it is
/// how `char->string` survived here.
pub(crate) fn native_lowered_primitive_methods(type_local: &str) -> &'static [&'static str] {
    match type_local {
        // `eq`/`eql`/`equal`/`equalp` are four names for `=` on an integer
        // (`registry::int_assoc`'s doc comment: same-type operands, nothing to
        // fold or recurse into), so all four are the same `icmp eq`. Only
        // `eq` used to be here, which is what made `case` — whose expansion
        // compares with `equal` — uncompilable for every integer scrutinee.
        // `c-long`/`c-ulong` share this arm: to the island they are integer
        // receivers like any other (`int-receiver-type?`), and what makes them
        // different is upstream — the checker registers only conversions for
        // them, so the arithmetic listed here can never be asked for.
        "i32" | "i8" | "i16" | "u8" | "u16" | "u32" | "c-long" | "c-ulong" => &[
            "+", "-", "*", "/", "mod", "<", "<=", ">", ">=", "=", "eq", "eql", "equal", "equalp", "/=",
            "int->int", "int->ratio", "int->float", "int->char",
            "int->i8", "int->i16", "int->i32", "int->u8", "int->u16", "int->u32",
            "int->c-long", "int->c-ulong",
            // The `Option`-returning halves. No prelude definition reaches
            // them; they are lowered so a user's own `(try-as u8 n)` can be
            // compiled — see `docs/syntax.md` §10.
            "try-int->char",
            "try-int->i8", "try-int->i16", "try-int->i32", "try-int->u8", "try-int->u16", "try-int->u32",
            "try-int->c-long", "try-int->c-ulong",
            "max", "min", "logand", "logior", "logxor", "logtest", "lognot", "logcount", "integer-length",
            "ash", "logbitp",
        ],
        // `eq`/`eql` are `StrId` identity (`string_identity_eq`) and lower to
        // an `icmp eq` on the two tagged words; `equal`/`equalp` are content
        // comparisons and go through `rt_str_eq`/`rt_str_equalp`.
        "string" => &[
            "length", "ref", "eq", "eql", "equal", "equalp", "lt", "<", "<=", ">", ">=", "append", "substring",
            // `upcase`/`downcase` are one `rt_str_*` call each, ASCII-only on
            // both tiers because both tiers run the same shim's rule — the
            // same shape and the same reasoning as `char`'s pair below.
            "upcase", "downcase",
        ],
        // `char->string` is here *and* in `char-native-method?` now. It was
        // here alone once, and that is worth remembering: this list is what
        // decides whether a call is a real graph edge, so claiming a method is
        // lowered natively when the island has no case for it means the edge
        // is dropped, no declaration is emitted, and `compile-call`'s
        // `get-function` **aborts the process** at compile time. The
        // disagreement is what `the_rust_and_island_native_method_lists_agree`
        // exists to catch.
        // `upcase`/`downcase`/`alphap`/`digitp` are one `rt_char_*` call each,
        // ASCII-only on both tiers because both tiers run the same shim's
        // rule. Until they were lowered, a `defun` that so much as mentioned
        // one of them could not be compiled at all — which is why the whole
        // `char`/`string` catalog in the prelude is written on code points
        // and `string::ref` instead (cl-parity-plan.md Phase 2's note 1).
        "char" => &[
            "eq", "eql", "equal", "equalp", "lt", "<", "<=", ">", ">=", "char->int", "char->string",
            "upcase", "downcase", "alphap", "digitp",
        ],
        "f64" | "f32" => &[
            "+", "-", "*", "/", "expt", "sqrt", "floor", "ceiling", "round", "truncate",
            "float->int", "float->ratio", "float->f32", "float->f64",
            // Same reason as the integer `try-*` group above: no prelude
            // caller, lowered so a user's `(try-as f32 x)` can compile.
            "try-float->f32", "try-float->f64",
            "<", "<=", ">", ">=", "=", "/=", "eq", "eql", "equal", "equalp",
            "max", "min", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh",
            "asinh", "acosh", "atanh", "exp", "log",
        ],
        // `int` (fixnum ∪ bignum): everything `integer_assoc` registers is
        // lowered — the arithmetic and comparisons as an overflow-checked
        // fast path over two fixnum words with an `rt_integer_*` slow path,
        // the rest as one `rt_integer_*` call each.
        "int" => &[
            "+", "-", "*", "/", "mod", "<", "<=", ">", ">=", "=", "/=", "eq", "eql", "equal", "equalp",
            "max", "min", "logand", "logior", "logxor", "logtest", "lognot", "logcount", "integer-length",
            "ash", "logbitp",
            "int->float", "int->ratio", "int->int", "int->char", "try-int->char",
            "int->i8", "int->i16", "int->i32", "int->u8", "int->u16", "int->u32",
            "int->c-long", "int->c-ulong",
            "try-int->i8", "try-int->i16", "try-int->i32", "try-int->u8", "try-int->u16", "try-int->u32",
            "try-int->c-long", "try-int->c-ulong",
        ],
        "ratio" => &[
            "+", "-", "*", "/", "<", "<=", ">", ">=", "=", "/=", "eq", "eql", "equal", "equalp",
            "ratio->int", "ratio->float", "numerator", "denominator", "max", "min",
        ],
        // `Sexpr` values are raw tagged `i64` handles in compiled code, and
        // interned symbols/`nil`/small atoms are handle-identical, so `eq`
        // (CL identity) lowers to a plain `icmp eq` on the two handles —
        // `compile-assoc`'s `sexpr` arm, mirroring the `char` branch. `eql`
        // cannot be that instruction (two separately boxed but equal
        // `Float`/`bignum`/`ratio` values are `eql` and not `eq`), so it
        // lowers to the `rt_sexpr_eql` shim over the same
        // `typelisp_rt::equality` the interpreter uses. The free-function
        // `equal`/`equalp` reach their shims through `rt_builtin_symbol`
        // instead, being calls rather than methods.
        "sexpr" => &["eq", "eql"],
        // `bool`'s four comparison names are one operation
        // (`registry::bool_assoc`: two immediate values, nothing to fold or
        // recurse into), and a compiled `bool` is a raw `0`/`1`, so all four
        // are the same `icmp eq`.
        "bool" => &["eq", "eql", "equal", "equalp"],
        // A compiled `symbol` is the interned `Value::Symbol` handle, so
        // identity *is* handle equality — the same `icmp eq` as `sexpr`'s
        // `eq`. `registry::symbol_assoc` registers only these two.
        "symbol" => &["eq", "eql"],
        _ => &[],
    }
}

/// The fixed set of `typelisp_rt` shims every compiled function
/// gets forward-declared and (JIT only — AOT resolves them as ordinary
/// linker symbols against `typelisp-rt`'s `staticlib`, see
/// `compile::aot::compile_file`) `add_global_mapping`-wired to, regardless
/// of whether its own body actually calls any of them. Cheap enough (9
/// extra declarations/mappings) to always include rather than checking
/// which ones a given body's call targets actually need. `pub(crate)`:
/// `compile::aot::compile_file` declares the same names (no JIT mapping
/// needed there — ordinary linker symbol resolution against `typelisp-rt`'s
/// `staticlib` instead) from this one source of truth.
///
/// `rt_push_sexpr_root`/`rt_pop_sexpr_root` (Stage 6 of the
/// Sexpr-representation plan, `docs/implementation-log.md` — the "Sexprルート挿入パス"):
/// unlike `rt_car`/.../`rt_match_fail`, no *user-visible* call name maps to
/// these (they are not in [`rt_builtin_symbol`]'s table) —
/// `compiler.rs`'s `retain-bindings`/`release-bindings`/`bind-let-values`
/// call them directly via `get-function`/`build-call`, the same way
/// `build-make-closure` calls `rt_closure_*` directly
/// rather than through the `(call name args)` tag. They still need
/// the same forward-declaration/global-mapping treatment as every other
/// `rt_*` shim, so they belong in this one shared list regardless.
///
/// `rt_push_permanent_sexpr_root` (general-ADT box field GC root
/// protection): `compiler.rs`'s `compile-construct-box-fields` calls this
/// directly for the same reason, one level down from a box's own
/// never-`build-free`'d field storage rather than a call-stack scope — see
/// `typelisp_rt::rt_push_permanent_sexpr_root`'s doc comment for why it has
/// no `rt_pop_permanent_sexpr_root` counterpart.
///
/// `rt_root_count`/`rt_set_sexpr_root` (the `setf`-reassignment GC-root fix):
/// `compiler.rs`'s `retain-bindings`/`bind-let-values` call `rt_root_count`
/// right before their own `rt_push_sexpr_root` call for a `kind = 2` binding,
/// to record the exact root-stack index that push lands at; `compile-set`
/// later hands that same index to `rt_set_sexpr_root` so a `setf` updates the
/// binding's *existing* root in place instead of leaving a freshly assigned
/// value with no root at all — see `typelisp_rt::rt_set_sexpr_root`'s doc
/// comment for the corruption this closes.
///
/// `rt_float_new`/`rt_float_value` (Sexpr/RtValue unification, Stage 0):
/// `compiler.rs`'s `compile-construct-sexpr`/`compile-sexpr-field` variant-2
/// arms call these to box/unbox a `Sexpr::f64` (`Value::Boxed`, see
/// `BoxedObj`) — the first `rt_*` pair for the new boxed-object store, same
/// declare-into-every-module mechanism every other `rt_*` function here
/// already uses.
///
/// `rt_struct_new`/`rt_struct_field_get`/`rt_struct_field_set`
/// (Sexpr/RtValue unification, Stage 3): `compiler.rs`'s
/// `compile-construct-boxed-struct`/`compile-field-get`/`compile-field-set`
/// call these to build/read/write a `BoxedObj::Struct` — the same
/// `BoxedObj::Struct` mem/rt-layer plumbing Stage 1 already exercised in
/// isolation, wired to the compiler for the first time here.
///
/// `rt_closure_*`/`rt_cell_*` (closure unification, Stage 1): the GC-heap
/// `BoxedObj::CompiledClosure` that replaces the raw `malloc`'d
/// reference-counted `ClosureBox`, plus the shared binding cells
/// (`BoxedObj::Cell`) captured names live in so compiled and interpreted
/// `setf` mutate the very same object.
pub(crate) fn rt_extern_functions() -> [(&'static str, usize); 292] {
    use typelisp_rt::equality::{rt_sexpr_eql, rt_sexpr_equal, rt_sexpr_equalp};
    // The printing family. These are the one group of shims defined outside
    // `typelisp-rt` — see `typelisp_print::shim`'s module doc comment for why
    // the linker requires that.
    use typelisp_print::shim::{
        rt_format, rt_pprint, rt_pprint_block_end, rt_pprint_block_start, rt_pprint_indent,
        rt_pprint_list_exhausted, rt_pprint_newline, rt_pprint_pop, rt_pprint_tab, rt_print, rt_println,
    };
    use typelisp_print::aot::{rt_format_call_method, rt_print_enum_variant, rt_print_field_template, rt_print_object_method};
    use typelisp_front::shim::{rt_eval, rt_eval_init, rt_eval_state, rt_macroexpand, rt_macroexpand_1};
    use typelisp_read::shim::{rt_read, rt_read_datum_at};
    use typelisp_rt::coroutine::{
        rt_loop_safepoint, rt_suspend_chan_cap, rt_suspend_chan_close, rt_suspend_chan_len,
        rt_suspend_chan_new, rt_suspend_chan_recv, rt_suspend_chan_select, rt_suspend_chan_send,
        rt_suspend_io, rt_suspend_io_for, rt_suspend_sleep,
        rt_suspend_wait, rt_suspend_yield,
    };
    use typelisp_rt::sys_builtin::{
        rt_command_line_args, rt_dribble_start, rt_dribble_stop, rt_ed_open, rt_exit,
        rt_get_internal_real_time, rt_get_internal_run_time,
        rt_get_universal_time, rt_getenv, rt_heap_info, rt_home_directory, rt_lisp_implementation_version,
        rt_machine_instance, rt_machine_type, rt_machine_version, rt_parse_float, rt_parse_int,
        rt_software_type, rt_software_version, rt_timezone_daylight_p,
        rt_timezone_offset_seconds,
    };
    use typelisp_rt::integer::{
        rt_integer_add, rt_integer_ash, rt_integer_cmp, rt_integer_div, rt_integer_fits, rt_integer_fits_char,
        rt_integer_from_word, rt_integer_integer_length, rt_integer_logand, rt_integer_logbitp, rt_integer_logcount,
        rt_integer_logior, rt_integer_lognot, rt_integer_logtest, rt_integer_logxor, rt_integer_mod, rt_integer_mul,
        rt_integer_narrow, rt_integer_sub, rt_integer_to_char, rt_integer_to_float,
        rt_integer_to_ratio, rt_int_not_fixnum,
    };
    use typelisp_rt::{
        rt_atom, rt_bignum_new,
        rt_make_random_state_fresh, rt_random_state_copy, rt_random_state_next, rt_seed_random_state,

        rt_file_create_directories, rt_file_delete, rt_file_directory_p, rt_file_exists_p,
        rt_file_list_directory, rt_file_modified_date, rt_file_owner_name, rt_file_rename,
        rt_file_truename,
        rt_net_accept, rt_net_buffered_p, rt_net_connect_begin, rt_net_connect_finish, rt_net_fill, rt_net_flush,
        rt_net_listen, rt_net_local_address, rt_net_peer_address, rt_net_pop_byte, rt_net_pop_char,
        rt_net_push_byte, rt_net_push_string, rt_net_shutdown_write,
        rt_net_resolve_begin, rt_net_resolve_finish, rt_net_tls_handshake, rt_net_tls_start,
        rt_net_udp_bind, rt_net_udp_last_sender, rt_net_udp_recv, rt_net_udp_send_to,
        rt_net_unix_connect, rt_net_unix_listen,
        rt_stream_at_line_start, rt_stream_close,
        rt_stream_finish_output, rt_stream_input_p, rt_stream_listen, rt_stream_open_file, rt_stream_open_p,
        rt_stream_output_p, rt_stream_position, rt_stream_read_byte, rt_stream_read_char, rt_stream_stderr, rt_stream_stdin, rt_stream_stdout,
        rt_set_macro_character, rt_get_macro_character, rt_set_dispatch_macro_character, rt_get_dispatch_macro_character,
        rt_stream_string_input, rt_stream_string_output, rt_stream_take_output_string, rt_stream_unread_char,
        rt_stream_write_byte, rt_stream_write_string,
        rt_box_kind, rt_car, rt_cdr,
        rt_cell_get, rt_cell_new, rt_cell_set, rt_char_alphap, rt_char_digitp, rt_char_downcase,
        rt_char_equalp, rt_char_upcase, rt_int_to_char, rt_closure_env_get, rt_closure_env_len,
        rt_closure_fnptr, rt_closure_new, rt_coroutine_closure_new, rt_cons, rt_consp, rt_data_field, rt_data_new, rt_data_variant, rt_f64_new, rt_f32_new, rt_narrow_new, rt_narrow_value, rt_float_to_int,
        rt_float_to_ratio, rt_f64_value, rt_f32_value, rt_global_get, rt_global_new, rt_global_set, rt_int_div, rt_int_mod,
        rt_int_ash, rt_int_logbitp, rt_int_logcount, rt_int_integer_length,
        rt_f64_tan, rt_f64_asin, rt_f64_acos, rt_f64_atan, rt_f64_sinh, rt_f64_cosh, rt_f64_tanh, rt_f64_asinh, rt_f64_acosh,
        rt_f64_atanh,
        rt_hashtable_bucket_count, rt_hashtable_bucket_delete, rt_hashtable_bucket_key, rt_hashtable_bucket_put,
        rt_hashtable_bucket_value,
        rt_hashtable_clear, rt_hashtable_count, rt_hashtable_entries, rt_hashtable_keys,
        rt_hashtable_new, rt_hashtable_values, rt_int_to_ratio,
        rt_intern_path, rt_intern_symbol, rt_wk_symbol, rt_list_to_path, rt_match_fail, rt_null, rt_panic, rt_path_to_list, rt_pop_sexpr_root, rt_push_permanent_sexpr_root,
        rt_push_sexpr_root, rt_ratio_add, rt_ratio_cmp, rt_ratio_denominator, rt_ratio_div, rt_ratio_from_bignums, rt_ratio_mul,
        rt_ratio_numerator, rt_ratio_sub, rt_ratio_to_int, rt_ratio_to_float, rt_root_count, rt_set_car, rt_set_cdr,
        rt_set_sexpr_root, rt_sexpr_bool, rt_sexpr_char, rt_sexpr_instance_test, rt_sexpr_int, rt_sexpr_i32, rt_sexpr_i8, rt_sexpr_i16, rt_sexpr_u8, rt_sexpr_u16, rt_sexpr_u32, rt_sexpr_str, rt_str_append, rt_str_eq, rt_str_equalp,
        rt_ffi_cstring_new, rt_ffi_cstring_free, rt_ffi_string_from_cstr,
        rt_str_length, rt_str_lt, rt_str_new, rt_str_ref, rt_str_substring, rt_str_upcase, rt_str_downcase, rt_int_fits, rt_int_fits_char, rt_f64_fits_f32, rt_struct_field_count, rt_struct_field_get, rt_struct_field_set,
        rt_struct_new, rt_struct_pop_field, rt_struct_push_field, rt_sym_name, rt_symp, rt_truncate_sexpr_roots,
        rt_dyn_new, rt_dyn_upcast, rt_dyn_value, rt_dyn_vtable, rt_upcast_set, rt_vtable_set,
        rt_throw, rt_throw_matches, rt_throw_take_value,
        rt_go,
        rt_frame_new, rt_frame_data, rt_frame_mask_bit, rt_frame_pc, rt_frame_set_pc,
        rt_frame_entered, rt_frame_call, rt_frame_call_env, rt_frame_apply, rt_frame_dyn_call, rt_pending_arg, rt_pending_argc,
        rt_pending_env, rt_pending_envc,
    };
    [
        // The one main-crate entry: the generic `llvm-*`/native-scope
        // builtin dispatch shim (interp-closure removal Stage 1) — it can't
        // live in `typelisp-rt` because it calls into the `inkwell`-backed
        // [`eval_llvm_builtin_method`]. Never referenced by AOT-linked user
        // executables (LLVM handle types are unreachable from user code, so
        // `compile-file` output never emits a call to it — an unreferenced
        // declaration emits no symbol for the linker to miss).
        ("rt_llvm_call", crate::compile::llvm_builtins::rt_llvm_call as usize),
        // Trait objects and vtables: `rt_dyn_new` boxes and
        // `rt_dyn_vtable`/`rt_dyn_value` decode (`compiler.rs`'s
        // `compile-dyn-*`). The dispatch itself is the *driver*'s since C5
        // (`rt_frame_dyn_call` names the slot and it decides who runs it),
        // because the slot's implementation need not be compiled at all.
        // `rt_dyn_upcast` swaps a box's table for a supertrait's when the two
        // layouts share no prefix.
        // `rt_vtable_set`/`rt_upcast_set` fill the two tables from AOT
        // startup; the JIT fills them Rust-side instead
        // (`Interp::publish_vtables`/`register_dyn_box`), so nothing emits a
        // call to either there.
        ("rt_dyn_new", rt_dyn_new as usize),
        ("rt_dyn_vtable", rt_dyn_vtable as usize),
        ("rt_dyn_value", rt_dyn_value as usize),
        ("rt_dyn_upcast", rt_dyn_upcast as usize),
        ("rt_vtable_set", rt_vtable_set as usize),
        ("rt_upcast_set", rt_upcast_set as usize),
        // The small system builtins (`typelisp_rt::sys_builtin`): parsing,
        // the clock, and process exit. Each was a gap that made every
        // function calling it uncompilable, and each closed the same way the
        // stream family did — one implementation, two edges.
        // The three structural equality predicates
        // (`typelisp_rt::equality`). `eq` needs no shim — it is the `icmp eq`
        // the island emits in place.
        ("rt_sexpr_eql", rt_sexpr_eql as usize),
        ("rt_sexpr_equal", rt_sexpr_equal as usize),
        ("rt_sexpr_equalp", rt_sexpr_equalp as usize),
        ("rt_parse_int", rt_parse_int as usize),
        ("rt_parse_float", rt_parse_float as usize),
        ("rt_get_universal_time", rt_get_universal_time as usize),
        ("rt_get_internal_real_time", rt_get_internal_real_time as usize),
        ("rt_exit", rt_exit as usize),
        ("rt_read", rt_read as usize),
        ("rt_read_datum_at", rt_read_datum_at as usize),
        // `eval` and the two startup calls an AOT executable makes to give it
        // an environment (`aot::build_main_wrapper`). The two are useless under
        // JIT — there is already an interpreter — but they are declared in
        // every module all the same, because this table is both the
        // forward-declaration list and the JIT address map, and the wrapper
        // looks them up by name.
        ("rt_eval", rt_eval as usize),
        ("rt_macroexpand_1", rt_macroexpand_1 as usize),
        ("rt_macroexpand", rt_macroexpand as usize),
        ("rt_eval_state", rt_eval_state as usize),
        ("rt_eval_init", rt_eval_init as usize),
        // The printing family (`typelisp_print::shim`). `format`/`print`/
        // `println`/`pprint` and `pprint-logical-block` are special forms, so
        // what reaches here are the `*-rt` names the checker lowered them to;
        // the four `pprint-*` operators are ordinary builtins.
        ("rt_format", rt_format as usize),
        ("rt_print", rt_print as usize),
        ("rt_println", rt_println as usize),
        ("rt_pprint", rt_pprint as usize),
        ("rt_pprint_block_start", rt_pprint_block_start as usize),
        ("rt_pprint_block_end", rt_pprint_block_end as usize),
        ("rt_pprint_newline", rt_pprint_newline as usize),
        ("rt_pprint_indent", rt_pprint_indent as usize),
        ("rt_pprint_tab", rt_pprint_tab as usize),
        ("rt_pprint_pop", rt_pprint_pop as usize),
        ("rt_pprint_list_exhausted", rt_pprint_list_exhausted as usize),
        // The printer's AOT startup registration (`typelisp_print::aot`):
        // an enum's variant names and each type's `print-object`, which a
        // standalone executable cannot look up the way the interpreter does.
        // AOT-only, like `rt_vtable_set`/`rt_upcast_set` above — the JIT
        // installs `INTERP_PRINT_HOOKS` instead, so nothing emits a call to
        // either there.
        ("rt_print_enum_variant", rt_print_enum_variant as usize),
        ("rt_print_field_template", rt_print_field_template as usize),
        ("rt_print_object_method", rt_print_object_method as usize),
        // The `~/name/` directive's own table: one entry per `(type, method)`
        // the checker found by scanning this program's literal control
        // strings (`Checker::format_call_methods`). AOT-only for the same
        // reason as the two above — the interpreter looks the name up in its
        // own method table when the directive runs.
        ("rt_format_call_method", rt_format_call_method as usize),
        ("rt_car", rt_car as usize),
        ("rt_cdr", rt_cdr as usize),
        ("rt_cons", rt_cons as usize),
        ("rt_consp", rt_consp as usize),
        ("rt_null", rt_null as usize),
        ("rt_atom", rt_atom as usize),
        ("rt_symp", rt_symp as usize),
        ("rt_sexpr_int", rt_sexpr_int as usize),
        ("rt_sexpr_i32", rt_sexpr_i32 as usize),
        ("rt_sexpr_i8", rt_sexpr_i8 as usize),
        ("rt_sexpr_i16", rt_sexpr_i16 as usize),
        ("rt_sexpr_u8", rt_sexpr_u8 as usize),
        ("rt_sexpr_u16", rt_sexpr_u16 as usize),
        ("rt_sexpr_u32", rt_sexpr_u32 as usize),
        ("rt_sexpr_bool", rt_sexpr_bool as usize),
        ("rt_sexpr_char", rt_sexpr_char as usize),
        ("rt_sexpr_str", rt_sexpr_str as usize),
        ("rt_sym_name", rt_sym_name as usize),
        ("rt_set_car", rt_set_car as usize),
        ("rt_set_cdr", rt_set_cdr as usize),
        ("rt_match_fail", rt_match_fail as usize),
        ("rt_panic", rt_panic as usize),
        ("rt_go", rt_go as usize),
        ("rt_frame_new", rt_frame_new as usize),
        ("rt_frame_data", rt_frame_data as usize),
        ("rt_frame_mask_bit", rt_frame_mask_bit as usize),
        ("rt_frame_pc", rt_frame_pc as usize),
        ("rt_frame_set_pc", rt_frame_set_pc as usize),
        ("rt_frame_entered", rt_frame_entered as usize),
        ("rt_frame_call", rt_frame_call as usize),
        ("rt_frame_apply", rt_frame_apply as usize),
        ("rt_frame_dyn_call", rt_frame_dyn_call as usize),
        ("rt_frame_call_env", rt_frame_call_env as usize),
        ("rt_pending_arg", rt_pending_arg as usize),
        ("rt_pending_argc", rt_pending_argc as usize),
        ("rt_pending_env", rt_pending_env as usize),
        ("rt_pending_envc", rt_pending_envc as usize),
        // `catch`/`throw`/`unwind-protect` (`compiler.rs`'s `compile-catch`/
        // `compile-throw`/`compile-unwind-protect`). `rt_throw` raises the
        // unwind; the two queries are what a region's dispatch block asks
        // about the one in flight. Nothing here catches: since C4 that is the
        // driver's job, at the boundary of the activation that raised — see
        // the catch/throw section of `typelisp-rt` for why a landing pad
        // cannot do it and why one catch per tier is enough.
        ("rt_throw", rt_throw as usize),
        ("rt_throw_matches", rt_throw_matches as usize),
        ("rt_throw_take_value", rt_throw_take_value as usize),
        ("rt_push_sexpr_root", rt_push_sexpr_root as usize),
        ("rt_pop_sexpr_root", rt_pop_sexpr_root as usize),
        ("rt_push_permanent_sexpr_root", rt_push_permanent_sexpr_root as usize),
        ("rt_root_count", rt_root_count as usize),
        ("rt_set_sexpr_root", rt_set_sexpr_root as usize),
        ("rt_truncate_sexpr_roots", rt_truncate_sexpr_roots as usize),
        // The C FFI's string conversions, called by a thunk rather than by
        // any compiled typelisp — see `crate::compile::ffi`.
        ("rt_ffi_cstring_new", rt_ffi_cstring_new as usize),
        ("rt_ffi_cstring_free", rt_ffi_cstring_free as usize),
        ("rt_ffi_string_from_cstr", rt_ffi_string_from_cstr as usize),
        ("rt_str_new", rt_str_new as usize),
        ("rt_str_length", rt_str_length as usize),
        ("rt_str_ref", rt_str_ref as usize),
        ("rt_str_eq", rt_str_eq as usize),
        ("rt_str_equalp", rt_str_equalp as usize),
        ("rt_char_equalp", rt_char_equalp as usize),
        // The four `char` methods the island can now lower in place
        // (cl-parity-plan.md Phase 1's leftover from Phase 2), plus
        // `int->char` — the only one of the five that can fail, and so the
        // only one declared `extern "C-unwind"`.
        ("rt_char_upcase", rt_char_upcase as usize),
        ("rt_char_downcase", rt_char_downcase as usize),
        ("rt_char_alphap", rt_char_alphap as usize),
        ("rt_char_digitp", rt_char_digitp as usize),
        ("rt_int_to_char", rt_int_to_char as usize),
        ("rt_str_lt", rt_str_lt as usize),
        ("rt_str_append", rt_str_append as usize),
        ("rt_str_substring", rt_str_substring as usize),
        ("rt_str_upcase", rt_str_upcase as usize),
        ("rt_int_fits", rt_int_fits as usize),
        ("rt_int_fits_char", rt_int_fits_char as usize),
        ("rt_f64_fits_f32", rt_f64_fits_f32 as usize),
        ("rt_str_downcase", rt_str_downcase as usize),
        ("rt_f64_new", rt_f64_new as usize),
        ("rt_f64_value", rt_f64_value as usize),
        ("rt_f32_new", rt_f32_new as usize),
        ("rt_f32_value", rt_f32_value as usize),
        ("rt_narrow_new", rt_narrow_new as usize),
        ("rt_narrow_value", rt_narrow_value as usize),
        ("rt_box_kind", rt_box_kind as usize),
        ("rt_struct_new", rt_struct_new as usize),
        ("rt_struct_field_get", rt_struct_field_get as usize),
        ("rt_struct_field_set", rt_struct_field_set as usize),
        ("rt_struct_field_count", rt_struct_field_count as usize),
        ("rt_struct_push_field", rt_struct_push_field as usize),
        ("rt_struct_pop_field", rt_struct_pop_field as usize),
        ("rt_closure_new", rt_closure_new as usize),
        ("rt_coroutine_closure_new", rt_coroutine_closure_new as usize),
        ("rt_closure_fnptr", rt_closure_fnptr as usize),
        ("rt_closure_env_len", rt_closure_env_len as usize),
        ("rt_closure_env_get", rt_closure_env_get as usize),
        ("rt_cell_new", rt_cell_new as usize),
        ("rt_cell_get", rt_cell_get as usize),
        ("rt_cell_set", rt_cell_set as usize),
        ("rt_data_new", rt_data_new as usize),
        ("rt_data_variant", rt_data_variant as usize),
        ("rt_data_field", rt_data_field as usize),
        ("rt_sexpr_instance_test", rt_sexpr_instance_test as usize),
        ("rt_hashtable_new", rt_hashtable_new as usize),
        ("rt_hashtable_count", rt_hashtable_count as usize),
        ("rt_hashtable_clear", rt_hashtable_clear as usize),
        ("rt_hashtable_keys", rt_hashtable_keys as usize),
        ("rt_hashtable_values", rt_hashtable_values as usize),
        ("rt_hashtable_entries", rt_hashtable_entries as usize),
        // The bucket family. `set`/`get`/`remove` used to be here as three
        // whole-lookup shims; they are prelude methods now, because hashing a
        // key and comparing two keys are the key type's own methods and a
        // shim cannot call them.
        ("rt_hashtable_bucket_count", rt_hashtable_bucket_count as usize),
        ("rt_hashtable_bucket_key", rt_hashtable_bucket_key as usize),
        ("rt_hashtable_bucket_value", rt_hashtable_bucket_value as usize),
        ("rt_hashtable_bucket_put", rt_hashtable_bucket_put as usize),
        ("rt_hashtable_bucket_delete", rt_hashtable_bucket_delete as usize),
        ("rt_global_new", rt_global_new as usize),
        ("rt_global_get", rt_global_get as usize),
        ("rt_global_set", rt_global_set as usize),
        ("rt_bignum_new", rt_bignum_new as usize),
        ("rt_ratio_from_bignums", rt_ratio_from_bignums as usize),
        ("rt_integer_add", rt_integer_add as usize),
        ("rt_integer_sub", rt_integer_sub as usize),
        ("rt_integer_mul", rt_integer_mul as usize),
        ("rt_integer_div", rt_integer_div as usize),
        ("rt_integer_mod", rt_integer_mod as usize),
        ("rt_integer_cmp", rt_integer_cmp as usize),
        ("rt_integer_logand", rt_integer_logand as usize),
        ("rt_integer_logior", rt_integer_logior as usize),
        ("rt_integer_logxor", rt_integer_logxor as usize),
        ("rt_integer_logtest", rt_integer_logtest as usize),
        ("rt_integer_ash", rt_integer_ash as usize),
        ("rt_integer_logbitp", rt_integer_logbitp as usize),
        ("rt_integer_lognot", rt_integer_lognot as usize),
        ("rt_integer_logcount", rt_integer_logcount as usize),
        ("rt_integer_integer_length", rt_integer_integer_length as usize),
        ("rt_integer_to_float", rt_integer_to_float as usize),
        ("rt_integer_to_ratio", rt_integer_to_ratio as usize),
        ("rt_integer_to_char", rt_integer_to_char as usize),
        ("rt_integer_fits_char", rt_integer_fits_char as usize),
        ("rt_integer_narrow", rt_integer_narrow as usize),
        ("rt_integer_fits", rt_integer_fits as usize),
        ("rt_integer_from_word", rt_integer_from_word as usize),
        ("rt_int_not_fixnum", rt_int_not_fixnum as usize),
        ("rt_random_state_next", rt_random_state_next as usize),
        ("rt_random_state_copy", rt_random_state_copy as usize),
        ("rt_make_random_state_fresh", rt_make_random_state_fresh as usize),
        ("rt_seed_random_state", rt_seed_random_state as usize),
        ("rt_stream_stdin", rt_stream_stdin as usize),
        ("rt_stream_stdout", rt_stream_stdout as usize),
        ("rt_stream_stderr", rt_stream_stderr as usize),
        ("rt_stream_string_input", rt_stream_string_input as usize),
        ("rt_stream_string_output", rt_stream_string_output as usize),
        ("rt_stream_open_file", rt_stream_open_file as usize),
        ("rt_stream_close", rt_stream_close as usize),
        ("rt_stream_open_p", rt_stream_open_p as usize),
        ("rt_stream_input_p", rt_stream_input_p as usize),
        ("rt_stream_output_p", rt_stream_output_p as usize),
        ("rt_stream_read_char", rt_stream_read_char as usize),
        ("rt_stream_read_byte", rt_stream_read_byte as usize),
        ("rt_stream_write_byte", rt_stream_write_byte as usize),
        ("rt_stream_unread_char", rt_stream_unread_char as usize),
        ("rt_suspend_sleep", rt_suspend_sleep as usize),
        ("rt_suspend_io", rt_suspend_io as usize),
        ("rt_suspend_io_for", rt_suspend_io_for as usize),
        ("rt_net_resolve_begin", rt_net_resolve_begin as usize),
        ("rt_net_resolve_finish", rt_net_resolve_finish as usize),
        ("rt_net_tls_start", rt_net_tls_start as usize),
        ("rt_net_tls_handshake", rt_net_tls_handshake as usize),
        ("rt_net_udp_bind", rt_net_udp_bind as usize),
        ("rt_net_udp_send_to", rt_net_udp_send_to as usize),
        ("rt_net_udp_recv", rt_net_udp_recv as usize),
        ("rt_net_udp_last_sender", rt_net_udp_last_sender as usize),
        ("rt_net_connect_begin", rt_net_connect_begin as usize),
        ("rt_net_connect_finish", rt_net_connect_finish as usize),
        ("rt_net_listen", rt_net_listen as usize),
        ("rt_net_unix_connect", rt_net_unix_connect as usize),
        ("rt_net_unix_listen", rt_net_unix_listen as usize),
        ("rt_net_accept", rt_net_accept as usize),
        ("rt_net_fill", rt_net_fill as usize),
        ("rt_net_pop_byte", rt_net_pop_byte as usize),
        ("rt_net_pop_char", rt_net_pop_char as usize),
        ("rt_net_buffered_p", rt_net_buffered_p as usize),
        ("rt_net_push_string", rt_net_push_string as usize),
        ("rt_net_push_byte", rt_net_push_byte as usize),
        ("rt_net_flush", rt_net_flush as usize),
        ("rt_net_shutdown_write", rt_net_shutdown_write as usize),
        ("rt_net_local_address", rt_net_local_address as usize),
        ("rt_net_peer_address", rt_net_peer_address as usize),
        ("rt_suspend_wait", rt_suspend_wait as usize),
        ("rt_suspend_yield", rt_suspend_yield as usize),
        ("rt_suspend_chan_new", rt_suspend_chan_new as usize),
        ("rt_suspend_chan_len", rt_suspend_chan_len as usize),
        ("rt_suspend_chan_cap", rt_suspend_chan_cap as usize),
        ("rt_suspend_chan_close", rt_suspend_chan_close as usize),
        ("rt_suspend_chan_send", rt_suspend_chan_send as usize),
        ("rt_suspend_chan_recv", rt_suspend_chan_recv as usize),
        ("rt_suspend_chan_select", rt_suspend_chan_select as usize),
        ("rt_loop_safepoint", rt_loop_safepoint as usize),
        ("rt_stream_listen", rt_stream_listen as usize),
        ("rt_stream_position", rt_stream_position as usize),
        ("rt_set_macro_character", rt_set_macro_character as usize),
        ("rt_get_macro_character", rt_get_macro_character as usize),
        ("rt_set_dispatch_macro_character", rt_set_dispatch_macro_character as usize),
        ("rt_get_dispatch_macro_character", rt_get_dispatch_macro_character as usize),
        ("rt_stream_write_string", rt_stream_write_string as usize),
        ("rt_stream_at_line_start", rt_stream_at_line_start as usize),
        ("rt_stream_finish_output", rt_stream_finish_output as usize),
        ("rt_stream_take_output_string", rt_stream_take_output_string as usize),
        ("rt_file_exists_p", rt_file_exists_p as usize),
        ("rt_file_delete", rt_file_delete as usize),
        ("rt_file_rename", rt_file_rename as usize),
        ("rt_file_truename", rt_file_truename as usize),
        ("rt_file_modified_date", rt_file_modified_date as usize),
        ("rt_file_owner_name", rt_file_owner_name as usize),
        ("rt_file_directory_p", rt_file_directory_p as usize),
        ("rt_file_list_directory", rt_file_list_directory as usize),
        ("rt_file_create_directories", rt_file_create_directories as usize),
        ("rt_command_line_args", rt_command_line_args as usize),
        ("rt_getenv", rt_getenv as usize),
        ("rt_home_directory", rt_home_directory as usize),
        ("rt_lisp_implementation_version", rt_lisp_implementation_version as usize),
        ("rt_machine_type", rt_machine_type as usize),
        ("rt_get_internal_run_time", rt_get_internal_run_time as usize),
        ("rt_machine_instance", rt_machine_instance as usize),
        ("rt_machine_version", rt_machine_version as usize),
        ("rt_software_version", rt_software_version as usize),
        ("rt_timezone_offset_seconds", rt_timezone_offset_seconds as usize),
        ("rt_timezone_daylight_p", rt_timezone_daylight_p as usize),
        ("rt_software_type", rt_software_type as usize),
        ("rt_heap_info", rt_heap_info as usize),
        ("rt_dribble_start", rt_dribble_start as usize),
        ("rt_dribble_stop", rt_dribble_stop as usize),
        ("rt_ed_open", rt_ed_open as usize),
        ("rt_int_to_ratio", rt_int_to_ratio as usize),
        ("rt_float_to_int", rt_float_to_int as usize),
        ("rt_float_to_ratio", rt_float_to_ratio as usize),
        ("rt_ratio_add", rt_ratio_add as usize),
        ("rt_ratio_sub", rt_ratio_sub as usize),
        ("rt_ratio_mul", rt_ratio_mul as usize),
        ("rt_ratio_div", rt_ratio_div as usize),
        ("rt_ratio_cmp", rt_ratio_cmp as usize),
        ("rt_ratio_to_int", rt_ratio_to_int as usize),
        ("rt_ratio_to_float", rt_ratio_to_float as usize),
        ("rt_ratio_numerator", rt_ratio_numerator as usize),
        ("rt_ratio_denominator", rt_ratio_denominator as usize),
        ("rt_intern_symbol", rt_intern_symbol as usize),
        ("rt_wk_symbol", rt_wk_symbol as usize),
        ("rt_intern_path", rt_intern_path as usize),
        ("rt_path_to_list", rt_path_to_list as usize),
        ("rt_list_to_path", rt_list_to_path as usize),
        ("rt_int_div", rt_int_div as usize),
        ("rt_int_mod", rt_int_mod as usize),
        ("rt_int_ash", rt_int_ash as usize),
        ("rt_int_logbitp", rt_int_logbitp as usize),
        ("rt_int_logcount", rt_int_logcount as usize),
        ("rt_int_integer_length", rt_int_integer_length as usize),
        ("rt_f64_tan", rt_f64_tan as usize),
        ("rt_f64_asin", rt_f64_asin as usize),
        ("rt_f64_acos", rt_f64_acos as usize),
        ("rt_f64_atan", rt_f64_atan as usize),
        ("rt_f64_sinh", rt_f64_sinh as usize),
        ("rt_f64_cosh", rt_f64_cosh as usize),
        ("rt_f64_tanh", rt_f64_tanh as usize),
        ("rt_f64_asinh", rt_f64_asinh as usize),
        ("rt_f64_acosh", rt_f64_acosh as usize),
        ("rt_f64_atanh", rt_f64_atanh as usize),
    ]
}

#[cfg(test)]
mod scc_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use typelisp_mem::{Heap, Value};

    use crate::check::core;
    use crate::check::repr::Repr;
    use crate::eval::interp::{FnDef, Interp};
    use crate::types::Path;

    /// Surface `defun`/`defmethod` syntax can never actually exercise the
    /// mutual-recursion branch of [`Interp::compute_sccs`]/
    /// [`Interp::compile_scc`]: `Checker::check_form_at` checks one top-level
    /// form at a time, in file order, so a `defun` can only ever call a name
    /// already registered *earlier* — `docs/syntax.md`'s own description of
    /// `labels` ("相互再帰可能なローカル関数定義") confirms mutual recursion
    /// is deliberately a `labels`-only, local-scope feature, not something a
    /// pair of top-level `defun`s can express. So this bypasses the checker
    /// entirely — inserting two hand-built [`FnDef`]s that call each other
    /// straight into [`Interp::fns`], the same "same-module direct access"
    /// trick this file's own [`Interp`] fields allow — to prove the SCC
    /// machinery itself (labels/closures Stage 5) handles a genuine cycle
    /// between two *separately* JIT'd top-level functions: forward-declares
    /// both in one shared module before either body is translated, JITs the
    /// module once via `CompiledFn::new_multi`, and both end up in
    /// `Interp::compiled` with no "mutual recursion ... is not supported"
    /// `Panic` (the pre-Stage-5 behavior this replaces).
    #[test]
    fn compile_function_compiles_a_genuine_two_node_cycle_bypassing_the_checker() {
        let mut heap = Heap::with_capacity(1 << 16);
        let mut checker = crate::Checker::new();
        let mut interp = Interp::new();
        crate::load_compiler(&mut heap, &mut checker, &mut interp);

        // `(defun a () i64 (b))` / `(defun b () i64 (a))` — never checked,
        // built directly as core forms, so the checker's forward-reference
        // restriction never comes into play. Each body is a single
        // `(call () () NAME ())`: no written/home segments (the callee is
        // already fully resolved) and no arguments, hence no argument reprs.
        for (name, callee) in [("a", "b"), ("b", "a")] {
            let path = heap.intern_symbol(callee);
            heap.push_root(path);
            let call = core::tagged(&mut heap, "call", &[Value::Empty, Value::Empty, path, Value::Empty])
                .expect("building a 4-field node cannot exhaust a 1<<16 heap");
            // Registered `FnDef` bodies are reachable only through
            // `Interp::fns`, which the collector does not scan — the same
            // permanent root `Interp::exec` takes for a checked definition.
            heap.push_permanent_root(call);
            heap.pop_root();
            interp.root.borrow_mut().define_fn(
                name.to_string(),
                Rc::new(FnDef { ffi: false,
                    name: name.to_string(),
                    params: vec![],
                    body: vec![call],
                    rest: false,
                    lambda: None,
                    sig: Some((vec![], Repr::Narrow)),
                    public: true,
                    compiled: RefCell::new(None),
                }),
            );
        }

        crate::compile::driver::compile_function(&interp, &mut heap, &crate::CompileTarget::Fn(crate::check::resolved::Ref::synthetic(Path::root("a"))))
            .expect("mutual recursion across separate top-level functions should now compile");
        assert!(interp.root.borrow().fn_compiled(&Path::root("a")), "\"a\" should have ended up compiled");
        assert!(interp.root.borrow().fn_compiled(&Path::root("b")), "\"b\", pulled in transitively as part of the same SCC, should have ended up compiled too");
    }
}

#[cfg(test)]
mod native_method_list_tests {
    use super::native_lowered_primitive_methods;

    /// Every primitive receiver whose builtin methods either side lowers, with
    /// the island predicate that decides for it.
    const PRIMITIVES: &[(&str, &[&str])] = &[
        ("int", &["i32", "i8", "i16", "u8", "u16", "u32", "c-long", "c-ulong"]),
        ("integer", &["int"]),
        ("string", &["string"]),
        ("char", &["char"]),
        ("float", &["f64", "f32"]),
        ("ratio", &["ratio"]),
        ("bool", &["bool"]),
        ("symbol", &["symbol"]),
        ("sexpr", &["sexpr"]),
    ];

    /// Every method-name key inside the island's `<name>-native-method?`
    /// definition.
    ///
    /// The predicates are `icase` key lists now (`(("+" "-" "*") true)`), not
    /// the `(equal method "X")` chains this used to scan for, so a key is a
    /// bare string literal in key position. Every string literal in the body
    /// *is* a key — the clause bodies are all `true`/`false` and the only other
    /// tokens are symbols — which makes "collect the literals" the whole parse.
    /// The one thing that needs real lexing is `;`: skipping comments by line
    /// is wrong in general (a `;` inside a string is not a comment), and here a
    /// mis-skip would drop keys silently, so the scan tracks string state the
    /// way the reader does. `!island.is_empty()` at the call site is the
    /// backstop for the whole scan going stale again.
    fn island_methods(name: &str) -> Vec<String> {
        let src = crate::compiler::SOURCE;
        let head = format!("(defun {}-native-method? ", name);
        let start = src.find(&head).unwrap_or_else(|| panic!("`{}` not found in the island SOURCE", head));
        let rest = &src[start + head.len()..];
        let body = match rest.find("\n(defun ") {
            Some(end) => &rest[..end],
            None => rest,
        };
        let mut out = Vec::new();
        let mut chars = body.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                ';' => {
                    for (_, c) in chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                '"' => {
                    let from = i + 1;
                    let mut end = None;
                    for (j, c) in chars.by_ref() {
                        if c == '"' {
                            end = Some(j);
                            break;
                        }
                    }
                    let to = end.expect("unterminated method-name string in the island SOURCE");
                    out.push(body[from..to].to_string());
                }
                _ => {}
            }
        }
        out.sort();
        out
    }

    /// `native_lowered_primitive_methods` and the island's
    /// `*-native-method?` predicates must agree, method for method.
    ///
    /// They are one decision written twice, and disagreement is not a missed
    /// optimization: this list decides whether a call is a real edge of the
    /// compile call graph, so a method it claims and the island does not lower
    /// gets no declaration emitted — and `compile-call`'s `get-function`
    /// **aborts the process** rather than reporting an error. The Rust side
    /// claimed `char->string` for exactly this reason, and nothing noticed
    /// until the precompiled prelude became the first thing to compile every
    /// prelude body instead of the few a test happens to name.
    ///
    /// Reading the island's own source is the point: a second hand-written
    /// list here would be a third copy to keep in sync.
    #[test]
    fn the_rust_and_island_native_method_lists_agree() {
        for (island_name, type_locals) in PRIMITIVES {
            let island = island_methods(island_name);
            assert!(!island.is_empty(), "`{}-native-method?` parsed as empty — the scan is broken", island_name);
            for type_local in *type_locals {
                let mut rust: Vec<String> =
                    native_lowered_primitive_methods(type_local).iter().map(|m| m.to_string()).collect();
                rust.sort();
                let island_only: Vec<&String> = island.iter().filter(|m| !rust.contains(m)).collect();
                let rust_only: Vec<&String> = rust.iter().filter(|m| !island.contains(m)).collect();
                assert!(
                    island_only.is_empty() && rust_only.is_empty(),
                    "`{}` disagrees between the two native-method lists:\n  island-only (a missed \
                     optimization): {:?}\n  Rust-only (a call to one aborts the process at compile \
                     time): {:?}",
                    type_local,
                    island_only,
                    rust_only
                );
            }
        }
    }

    /// Neither list above is checked against the **registry**, and that gap is
    /// the one a user falls into: a builtin the registry registers and neither
    /// side lowers is a method an ordinary call can name, and naming it makes
    /// the calling `defun` uncompilable. `docs/syntax.md` §10 carried a table
    /// of exactly those, filled in by hand as they were discovered; this is
    /// the check that keeps it empty without anyone having to notice.
    ///
    /// `print`/`println` are the standing exception, and are not a hole. The
    /// checker intercepts both as special forms before any method resolution
    /// happens (`Checker::check_print_like` — the control string has to be a
    /// literal, since its directives decide what the remaining arguments may
    /// be), so neither ever becomes an `Expr::Assoc` for `call_graph_edges` to
    /// reject or the island to lower; a qualified `(i32::print x)` is refused
    /// at check time as an instance method. `registry::int_assoc` and its
    /// siblings still register them because they are the receiver-typed
    /// printing entry points `eval_builtin_method` dispatches on.
    #[test]
    fn every_registered_builtin_method_on_a_native_receiver_lowers() {
        // See this test's doc comment: registered, but unreachable as a method
        // call, so no lowering can be asked for.
        const CHECKER_SPECIAL_FORMS: [&str; 2] = ["print", "println"];
        let registry = typelisp_front::check::registry::Registry::with_builtins();
        let mut gaps: Vec<String> = Vec::new();
        for (_, type_locals) in PRIMITIVES {
            for type_local in *type_locals {
                let def = registry
                    .root
                    .types
                    .get(*type_local)
                    .unwrap_or_else(|| panic!("`{}` is not a built-in type in the registry", type_local));
                for (method, f) in &def.assoc {
                    if !f.builtin || CHECKER_SPECIAL_FORMS.contains(&method.as_str()) {
                        continue;
                    }
                    if !native_lowered_primitive_methods(type_local).contains(&method.as_str()) {
                        gaps.push(format!("{}::{}", type_local, method));
                    }
                }
            }
        }
        assert!(
            gaps.is_empty(),
            "these builtin methods are registered but have no compiled lowering, so a `defun` \
             calling one cannot be compiled: {:?}\n  Add each to `native_lowered_primitive_methods` \
             *and* the island's matching `*-native-method?`, or — if it is unreachable as a method \
             call the way `print` is — say so in `CHECKER_SPECIAL_FORMS`.",
            gaps
        );
    }
}

