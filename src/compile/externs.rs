//! Which Rust function a builtin call lowers to, and how the two tiers find
//! it.
//!
//! Three tables, all of them backend concerns even though they name no LLVM
//! type:
//!
//! - [`rt_builtin_symbol`]: the `rt_*` shim a *free* builtin lowers to, asked
//!   through [`builtin_shim`], which also checks the call's path is where that
//!   builtin lives. Read by `compile::symbols::callee_symbol_name` when it
//!   decides whether to mangle a name, and by the driver when it decides
//!   whether a call target needs compiling first.
//! - [`native_lowered_primitive_methods`]: which builtin *methods* on a
//!   primitive receiver the island lowers in place rather than calling.
//! - [`rt_extern_functions`]: every shim, with its address — the JIT's
//!   `externals` and the AOT module's forward-declaration list,
//!   which have to be the same list.
//!
//! They live here rather than in `eval::interp` because nothing in the
//! evaluator asks any of these questions: they are asked while *compiling*,
//! by `symbols`, `core_bridge`, `driver`, `aot` and the two bootstrappers.

use typelisp_mem::{wk, SymRef};

use crate::compile::runtime_abi::{shim, Shim};

/// The `typelisp_rt` shim a *free* builtin call lowers to, or
/// `None` for an ordinary function whose name gets mangled instead.
///
/// These builtins can never be `compile`d themselves (no typelisp body —
/// direct cons-heap or OS access, Rust-only), so [`crate::compile::driver::compile_function`]
/// excludes them from its normal "every call target must already be compiled"
/// check ([`is_rt_builtin`]) and instead always wires them via
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
pub(crate) fn rt_builtin_symbol(name: SymRef) -> Option<&'static str> {
    // The memory behind `def-c-struct`s and typed pointers.
    if let Some(sym) = typelisp_front::c_struct::rt_symbol(name) {
        return Some(sym);
    }
    Some(match name.well_known() {
        // A C callback's entry address — `typelisp_front::ffi_callback`.
        typelisp_front::ffi_callback::ADDRESS_BUILTIN => "rt_ffi_callback_address",
        wk::F_SEXPR_CAR => "rt_car",
        wk::F_SEXPR_CDR => "rt_cdr",
        wk::F_SEXPR_CONS => "rt_cons",
        wk::F_SEXPR_CONSP => "rt_consp",
        wk::F_SEXPR_NULL => "rt_null",
        wk::F_SEXPR_ATOM => "rt_atom",
        wk::F_SEXPR_SYMP => "rt_symp",
        wk::F_SEXPR_INT => "rt_sexpr_int",
        wk::F_SEXPR_I32 => "rt_sexpr_i32",
        wk::F_SEXPR_BOOL => "rt_sexpr_bool",
        wk::F_SEXPR_CHAR => "rt_sexpr_char",
        wk::F_SEXPR_F64 => "rt_sexpr_f64",
        wk::F_SEXPR_F32 => "rt_sexpr_f32",
        // One shim per narrow width, not one taking the width as an operand:
        // a builtin accessor is called with exactly its own arguments, and
        // the type it reads is in its name.
        wk::F_SEXPR_I8 => "rt_sexpr_i8",
        wk::F_SEXPR_I16 => "rt_sexpr_i16",
        wk::F_SEXPR_U8 => "rt_sexpr_u8",
        wk::F_SEXPR_U16 => "rt_sexpr_u16",
        wk::F_SEXPR_U32 => "rt_sexpr_u32",
        wk::F_SEXPR_STR => "rt_sexpr_str",
        wk::F_SEXPR_SYM_NAME => "rt_sym_name",
        // `symbol->string` is `sexpr-sym-name` under the checker's `symbol`
        // type rather than `sexpr`'s — the same shim, since both spellings
        // carry the same interned `Value::Symbol`.
        wk::F_SYMBOL_TO_STRING => "rt_sym_name",
        wk::INTERN => "rt_intern",
        wk::F_KEYWORDP => "rt_keywordp",
        // The small system builtins (`sys_builtin`): parsing, the clock, and
        // process exit. Each is a thin edge over one shared implementation
        // the interpreter calls too.
        // `eq`/`eql`/`equalp` on any type are free *functions*
        // (`registry`'s root `fns`), not methods, so they map here rather
        // than through `native_lowered_primitive_methods`. Both are the same
        // `typelisp_rt::equality` the interpreter calls.
        wk::M_EQ => "rt_sexpr_eq",
        wk::M_EQL => "rt_sexpr_eql",
        wk::M_EQUALP => "rt_sexpr_equalp",
        wk::F_SXHASH_EQ => "rt_sxhash_eq",
        wk::F_SXHASH_EQL => "rt_sxhash_eql",
        wk::F_SXHASH_EQUALP => "rt_sxhash_equalp",
        wk::F_PARSE_FLOAT => "rt_parse_float",
        wk::F_GET_UNIVERSAL_TIME => "rt_get_universal_time",
        wk::F_GET_INTERNAL_REAL_TIME => "rt_get_internal_real_time",
        // The environment and the running implementation (CLHS 25.1, plus
        // `command-line-args`/`getenv`, which CL has no equivalent of).
        wk::F_COMMAND_LINE_ARGS => "rt_command_line_args",
        wk::F_GETENV => "rt_getenv",
        wk::F_HOME_DIRECTORY => "rt_home_directory",
        wk::F_LISP_IMPLEMENTATION_VERSION => "rt_lisp_implementation_version",
        wk::F_MACHINE_TYPE => "rt_machine_type",
        wk::F_SOFTWARE_TYPE => "rt_software_type",
        // The four CLHS 25.1 names that ask the running host, and the two
        // time-zone primitives under `decode-`/`encode-universal-time`.
        wk::F_GET_INTERNAL_RUN_TIME => "rt_get_internal_run_time",
        wk::F_MACHINE_INSTANCE => "rt_machine_instance",
        wk::F_MACHINE_VERSION => "rt_machine_version",
        wk::F_SOFTWARE_VERSION => "rt_software_version",
        wk::F_TIMEZONE_OFFSET_SECONDS => "rt_timezone_offset_seconds",
        wk::F_TIMEZONE_DAYLIGHT_P => "rt_timezone_daylight_p",
        wk::F_EXIT => "rt_exit",
        // `read` (`typelisp_read::shim`), which could not lower while the
        // reader was a module of this crate: a shim naming it would have had
        // to reach up into `typelisp`, which depends on the runtime rather
        // than the other way round.
        wk::F_READ => "rt_read",
        wk::F_READ_DATUM_AT => "rt_read_datum_at",
        // `eval` (`typelisp_front::shim`), the last of the six builtins that
        // used to make a caller uncompilable. Its implementation is the
        // checker and the interpreter, which is why they are a crate below
        // this one — see that module's doc comment for how the shim finds an
        // environment on each of the two paths.
        wk::F_EVAL => "rt_eval",
        // `macroexpand-1`/`macroexpand` reach the same two environments
        // `eval` does, and for the same reason: expanding a macro means
        // running its body, which is the interpreter.
        wk::F_MACROEXPAND_1 => "rt_macroexpand_1",
        wk::F_MACROEXPAND => "rt_macroexpand",
        // The printing family. `pprint` and `pprint-logical-block` are
        // special forms; the names here are what the checker lowered them to
        // (`check_pprint`/`check_pprint_logical_block`). The last five are
        // ordinary builtins (`registry`'s root `fns`). All of them run
        // `typelisp_print::runtime::print_builtin`, the same implementation
        // `Interp::eval_builtin` runs. `format`/`print`/`println` are
        // expanded into the `rt_fmt_*` pieces below.
        wk::F_PPRINT_RT => "rt_pprint",
        wk::F_PPRINT_BLOCK_START_RT => "rt_pprint_block_start",
        wk::F_PPRINT_BLOCK_END_RT => "rt_pprint_block_end",
        // The pieces an expanded `format` is made of
        // (`typeck::format_expand::PIECES`), one shim each.
        wk::F_FMT_OPEN => "rt_fmt_open",
        wk::F_FMT_TEXT => "rt_fmt_text",
        wk::F_FMT_FRESH_LINE => "rt_fmt_fresh_line",
        wk::F_FMT_STRING => "rt_fmt_string",
        wk::F_FMT_NUMBER => "rt_fmt_number",
        wk::F_FMT_VALUE => "rt_fmt_value",
        wk::F_FMT_INTEGER => "rt_fmt_integer",
        wk::F_FMT_INTEGER_ANY => "rt_fmt_integer_any",
        wk::F_FMT_WORDS => "rt_fmt_words",
        wk::F_FMT_PLURAL => "rt_fmt_plural",
        wk::F_FMT_CHAR => "rt_fmt_char",
        wk::F_FMT_FIXED => "rt_fmt_fixed",
        wk::F_FMT_EXPONENT => "rt_fmt_exponent",
        wk::F_FMT_GENERAL => "rt_fmt_general",
        wk::F_FMT_DOLLARS => "rt_fmt_dollars",
        wk::F_FMT_TAB => "rt_fmt_tab",
        wk::F_FMT_NEWLINE => "rt_fmt_newline",
        wk::F_FMT_INDENT => "rt_fmt_indent",
        wk::F_FMT_CASE_OPEN => "rt_fmt_case_open",
        wk::F_FMT_CASE_CLOSE => "rt_fmt_case_close",
        wk::F_FMT_JUST_OPEN => "rt_fmt_just_open",
        wk::F_FMT_JUST_SEGMENT => "rt_fmt_just_segment",
        wk::F_FMT_JUST_CLOSE => "rt_fmt_just_close",
        wk::F_FMT_BLOCK_OPEN => "rt_fmt_block_open",
        wk::F_FMT_BLOCK_CLOSE => "rt_fmt_block_close",
        wk::F_FMT_RESULT => "rt_fmt_result",
        wk::F_FMT_EMIT => "rt_fmt_emit",
        wk::F_FMT_FIRST => "rt_fmt_first",
        wk::F_FMT_REST => "rt_fmt_rest",
        wk::F_FMT_EMPTY => "rt_fmt_empty",
        wk::F_FMT_LENGTH => "rt_fmt_length",
        wk::F_FMT_GOTO => "rt_fmt_goto",
        wk::F_FMT_AS_LIST => "rt_fmt_as_list",
        wk::F_FMT_PARAM => "rt_fmt_param",
        wk::F_FMT_SELECT => "rt_fmt_select",
        wk::F_FMT_BOOL => "rt_fmt_bool",
        wk::F_PPRINT_NEWLINE => "rt_pprint_newline",
        wk::F_PPRINT_INDENT => "rt_pprint_indent",
        wk::F_PPRINT_TAB => "rt_pprint_tab",
        wk::F_PPRINT_POP => "rt_pprint_pop",
        wk::F_PPRINT_LIST_EXHAUSTED => "rt_pprint_list_exhausted",
        // The `random-state` builtins. Nothing about a random-state lives
        // outside the heap, so all four lower.
        wk::F_RANDOM_STATE_NEXT => "rt_random_state_next",
        wk::F_RANDOM_STATE_COPY => "rt_random_state_copy",
        wk::F_MAKE_RANDOM_STATE_FRESH => "rt_make_random_state_fresh",
        wk::F_SEED_RANDOM_STATE => "rt_seed_random_state",
        // The stream/file builtins. Every one is a thin conversion around
        // `typelisp_rt::stream_builtin::stream_builtin`, which the
        // interpreter calls too — see `Interp::eval_stream_builtin`. They can
        // lower at all only because the stream *table* lives in
        // `typelisp-rt` (see `typelisp_rt::stream`'s module docs): an
        // AOT-linked executable has no interpreter to keep it on.
        wk::F_STREAM_STDIN => "rt_stream_stdin",
        wk::F_STREAM_STDOUT => "rt_stream_stdout",
        wk::F_STREAM_STDERR => "rt_stream_stderr",
        wk::F_STREAM_STRING_INPUT => "rt_stream_string_input",
        wk::F_STREAM_STRING_OUTPUT => "rt_stream_string_output",
        wk::F_STREAM_OPEN_FILE => "rt_stream_open_file",
        wk::F_STREAM_CLOSE => "rt_stream_close",
        wk::F_STREAM_OPEN_P => "rt_stream_open_p",
        wk::F_STREAM_DESCRIBE => "rt_stream_describe",
        wk::F_STREAM_INPUT_P => "rt_stream_input_p",
        wk::F_STREAM_OUTPUT_P => "rt_stream_output_p",
        wk::F_STREAM_READ_CHAR => "rt_stream_read_char",
        wk::F_STREAM_READ_BYTE => "rt_stream_read_byte",
        wk::F_STREAM_WRITE_BYTE => "rt_stream_write_byte",
        wk::F_STREAM_UNREAD_CHAR => "rt_stream_unread_char",
        wk::F_SET_MACRO_CHARACTER => "rt_set_macro_character",
        wk::F_GET_MACRO_CHARACTER => "rt_get_macro_character",
        wk::F_SET_DISPATCH_MACRO_CHARACTER => "rt_set_dispatch_macro_character",
        wk::F_GET_DISPATCH_MACRO_CHARACTER => "rt_get_dispatch_macro_character",
        // **The `rt_suspend_` prefix is load-bearing**, and this table is the
        // only place that decides which builtins suspend: `core_bridge` reads
        // the prefix and emits a `(suspend ...)` node instead of a `(call
        // ...)`, so the island needs no list of its own.
        //
        // `sleep` used to lower to `rt_sleep`, which stops the *thread*. That
        // was the reason it could not be compiled at all: with tasks the same
        // source would mean two different things on the two sides, silently.
        wk::F_SLEEP => "rt_suspend_sleep",
        wk::F_ENABLE_INTERRUPT => "rt_interrupt_enable",
        wk::F_DISABLE_INTERRUPT => "rt_interrupt_disable",
        wk::F_YIELD => RT_SUSPEND_YIELD,
        // `(net-wait h interest)` parks the task on a socket: the third of
        // the free builtins that suspend, and the reason the prelude's
        // `socket-stream` methods can wait without any of them blocking.
        wk::F_NET_WAIT => "rt_suspend_io",
        wk::F_NET_WAIT_FOR => "rt_suspend_io_for",
        // The REPL tool layer's runtime half. `trace`/`untrace`/`step`/
        // `disassemble` have no row and never will — those are
        // interpreter-only forms, the same category `compile`/`compile-file`/
        // `dump` are in (`docs/ja/reference/syntax.md` §10). These three are ordinary
        // builtins: the heap statistics and the dribble sink are the
        // runtime's own, and launching an editor is a process call.
        wk::F_HEAP_INFO => "rt_heap_info",
        wk::F_DRIBBLE_START => "rt_dribble_start",
        wk::F_DRIBBLE_STOP => "rt_dribble_stop",
        wk::F_ED_OPEN => "rt_ed_open",
        wk::F_STREAM_LISTEN => "rt_stream_listen",
        wk::F_STREAM_POSITION => "rt_stream_position",
        wk::F_STREAM_WRITE_STRING => "rt_stream_write_string",
        wk::F_STREAM_AT_LINE_START => "rt_stream_at_line_start",
        wk::F_STREAM_FINISH_OUTPUT => "rt_stream_finish_output",
        wk::F_STREAM_TAKE_OUTPUT_STRING => "rt_stream_take_output_string",
        wk::F_FILE_EXISTS_P => "rt_file_exists_p",
        wk::F_FILE_DELETE => "rt_file_delete",
        wk::F_FILE_TRUENAME => "rt_file_truename",
        wk::F_FILE_MODIFIED_DATE => "rt_file_modified_date",
        wk::F_FILE_OWNER_NAME => "rt_file_owner_name",
        wk::F_FILE_DIRECTORY_P => "rt_file_directory_p",
        wk::F_FILE_LIST_DIRECTORY => "rt_file_list_directory",
        wk::F_FILE_CREATE_DIRECTORIES => "rt_file_create_directories",
        wk::F_FILE_RENAME => "rt_file_rename",
        wk::F_NET_RESOLVE_BEGIN => "rt_net_resolve_begin",
        wk::F_NET_RESOLVE_FINISH => "rt_net_resolve_finish",
        wk::F_NET_TLS_START => "rt_net_tls_start",
        wk::F_NET_TLS_LISTEN => "rt_net_tls_listen",
        wk::F_NET_SOCKET_ERROR => "rt_net_socket_error",
        wk::F_NET_PEER_SUBJECT => "rt_net_peer_subject",
        wk::F_NET_SERVER_NAME => "rt_net_server_name",
        wk::F_NET_SET_NODELAY => "rt_net_set_nodelay",
        wk::F_NET_SET_KEEPALIVE => "rt_net_set_keepalive",
        wk::F_NET_SET_KEEPALIVE_PERIOD => "rt_net_set_keepalive_period",
        wk::F_NET_TLS_ADD_CERTIFICATE => "rt_net_tls_add_certificate",
        wk::F_NET_TLS_HANDSHAKE => "rt_net_tls_handshake",
        wk::F_NET_UDP_BIND => "rt_net_udp_bind",
        wk::F_NET_UDP_SEND_TO => "rt_net_udp_send_to",
        wk::F_NET_UDP_RECV => "rt_net_udp_recv",
        wk::F_NET_UDP_LAST_SENDER => "rt_net_udp_last_sender",
        wk::F_NET_CONNECT_BEGIN => "rt_net_connect_begin",
        wk::F_NET_CONNECT_FINISH => "rt_net_connect_finish",
        wk::F_NET_LISTEN => "rt_net_listen",
        wk::F_NET_UNIX_CONNECT => "rt_net_unix_connect",
        wk::F_NET_UNIX_LISTEN => "rt_net_unix_listen",
        wk::F_NET_ACCEPT => "rt_net_accept",
        wk::F_NET_FILL => "rt_net_fill",
        wk::F_NET_POP_BYTE => "rt_net_pop_byte",
        wk::F_NET_POP_CHAR => "rt_net_pop_char",
        wk::F_NET_BUFFERED_P => "rt_net_buffered_p",
        wk::F_NET_PUSH_STRING => "rt_net_push_string",
        wk::F_NET_PUSH_BYTE => "rt_net_push_byte",
        wk::F_NET_FLUSH => "rt_net_flush",
        wk::F_NET_SHUTDOWN_WRITE => "rt_net_shutdown_write",
        wk::F_NET_LOCAL_ADDRESS => "rt_net_local_address",
        wk::F_NET_PEER_ADDRESS => "rt_net_peer_address",
        _ => return None,
    })
}

/// The shim for a **method** that suspends the running task, or `None`.
///
/// A second table because the free-function one is keyed by bare name and a
/// method needs its receiver type too. One row, and it is here rather than
/// spelled in `core_bridge` so that "which builtins suspend" stays a question
/// this module answers — the free functions are already decided by the
/// `rt_suspend_` prefix in [`rt_builtin_symbol`].
pub(crate) fn rt_suspend_method_symbol(type_name: &crate::Path, method: SymRef) -> Option<&'static str> {
    let ty = typelisp_front::types::builtin_type(type_name)?;
    match (ty, method.well_known()) {
        (wk::TASK_TYPE, wk::M_WAIT) => Some("rt_suspend_wait"),
        // A thread is a task to the scheduler, and `join` is `wait`.
        (wk::THREAD_TYPE, wk::M_JOIN) => Some("rt_suspend_wait"),
        // Every channel operation, including the four that never wait: the
        // table of channels is the scheduler's, and the driver is the only
        // way to it (`typelisp_abi::call_state::SUSPEND_CHAN_NEW` says why
        // it cannot live anywhere lower).
        (wk::CHAN, wk::M_NEW) => Some("rt_suspend_chan_new"),
        (wk::CHAN, wk::M_LEN) => Some("rt_suspend_chan_len"),
        (wk::CHAN, wk::M_CAP) => Some("rt_suspend_chan_cap"),
        (wk::CHAN, wk::M_CLOSE) => Some("rt_suspend_chan_close"),
        (wk::CHAN, wk::SEND) => Some("rt_suspend_chan_send"),
        (wk::CHAN, wk::RECV) => Some("rt_suspend_chan_recv"),
        _ => None,
    }
}

/// The shim a builtin **static** method that does not suspend lowers to, or
/// `None`. The free-function table is keyed by bare name, and these need
/// their owner type too — the same reason [`rt_suspend_method_symbol`] is a
/// table of its own.
pub(crate) fn rt_static_method_symbol(type_name: &crate::Path, method: SymRef) -> Option<&'static str> {
    let ty = typelisp_front::types::builtin_type(type_name)?;
    match (ty, method.well_known()) {
        (wk::THREAD_TYPE, wk::M_CURRENT_ID) => Some("rt_thread_current_id"),
        (wk::THREAD_TYPE, wk::M_AVAILABLE_PARALLELISM) => Some("rt_thread_available_parallelism"),
        _ => None,
    }
}

/// Whether the runtime entry point `name` needs the interpreter to answer:
/// it evaluates (`eval`), expands macros, or reads with the readtable, whose
/// `#.` and macro characters run program code. The bridge puts a
/// [`RT_SUSPEND_MAIN`] in front of every call to one, so that a compiled task
/// a worker thread is stepping moves to the interpreter's thread before it
/// makes the call — and there is no other way into these from compiled code.
///
/// `rt_ffi_callback_address` is here for a session: the first time a
/// callback is asked for, the interpreter's backend compiles its entry. An
/// executable registered every entry at startup and only looks one up.
pub(crate) fn needs_interpreter(name: &str) -> bool {
    matches!(
        name,
        "rt_eval" | "rt_macroexpand" | "rt_macroexpand_1" | "rt_read" | "rt_read_datum_at" | "rt_ffi_callback_address"
    )
}

/// The shims generated code calls without checking for a raise afterwards
/// (`compile::raise_check`): ones whose bodies cannot raise, and are called
/// often enough that the check would be most of the code around them. Every
/// other call is checked. A shim listed here that does raise is caught by the
/// driver — an activation that goes on after a raise is fatal there — so a
/// mistake here is loud, not a wrong answer.
const NEVER_RAISING_SHIMS: [&str; 26] = [
    "rt_unwinding",
    "rt_pending_arg",
    "rt_frame_open",
    "rt_local_frame_open",
    "rt_local_frame_close",
    "rt_frame_call",
    "rt_str_new",
    "rt_wk_symbol",
    "rt_intern_symbol",
    "rt_push_permanent_sexpr_root",
    "rt_push_sexpr_root",
    "rt_pop_sexpr_root",
    "rt_truncate_sexpr_roots",
    "rt_print_field_template",
    "rt_print_field_name",
    "rt_print_names",
    "rt_global_new",
    "rt_f64_new",
    "rt_f64_value",
    "rt_data_variant",
    "rt_data_field",
    "rt_data_new",
    "rt_struct_new",
    "rt_cons",
    "rt_integer_cmp",
    "rt_loop_safepoint",
];

/// Whether a call to the function `name` needs no raise check after it.
pub(crate) fn never_raises(name: &str) -> bool {
    NEVER_RAISING_SHIMS.contains(&name)
}

/// The shims that take cells from the cons arena, each with the operation a
/// program reaches it through: what `typl -c --no-cons` refuses a program
/// for (`compile::aot::reach`).
///
/// The arena has one allocator, `typelisp_mem::Heap::cons`, and these are
/// the shims whose bodies reach it: building a `Sexpr` cons, turning a path
/// into a list, the reader (`read`, and a datum the program quotes), and the
/// interpreter an `eval`-carrying executable
/// holds. A shim that calls compiled code back (a closure, a
/// `print-object` method) is not here: what it calls is in the program's
/// module, where the check follows it itself.
pub(crate) const CONS_MAKING_SHIMS: [(&str, &str); 9] = [
    ("rt_cons", "building a `Sexpr` cons"),
    ("rt_path_to_list", "turning a path into a list"),
    ("rt_read", "`read`"),
    ("rt_read_datum_at", "a quoted datum"),
    ("rt_eval", "`eval`"),
    ("rt_eval_init", "`eval`"),
    ("rt_eval_state", "`eval`"),
    ("rt_macroexpand", "`macroexpand`"),
    ("rt_macroexpand_1", "`macroexpand-1`"),
];

/// Which rows of a [`BuiltinTypeTable`] say: the type a builtin answers
/// with, or the type of the value inside that answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuiltinTypeRow {
    Result,
    Inner,
}

/// One runtime crate's table of the types its builtins build their values
/// as (`typelisp_rt::result_value`).
pub struct BuiltinTypeTable {
    pub name: &'static str,
    pub row: BuiltinTypeRow,
    pub rows: typelisp_rt::result_value::TypeTable,
}

/// Every table of the types the runtime's builtins build their values as:
/// what is checked against the registry (`type_identity_guard_test`), what
/// says which types the runtime makes (`aot::made`), and what the ABI
/// describes (`abi_signature`).
pub const BUILTIN_TYPE_TABLES: [BuiltinTypeTable; 11] = [
    BuiltinTypeTable { name: "typelisp_rt::sys_builtin::RESULT_TYPES", row: BuiltinTypeRow::Result, rows: typelisp_rt::sys_builtin::RESULT_TYPES },
    BuiltinTypeTable { name: "typelisp_rt::stream_builtin::RESULT_TYPES", row: BuiltinTypeRow::Result, rows: typelisp_rt::stream_builtin::RESULT_TYPES },
    BuiltinTypeTable { name: "typelisp_rt::stream_builtin::INNER_TYPES", row: BuiltinTypeRow::Inner, rows: typelisp_rt::stream_builtin::INNER_TYPES },
    BuiltinTypeTable { name: "typelisp_rt_posix::net_builtin::RESULT_TYPES", row: BuiltinTypeRow::Result, rows: typelisp_rt_posix::net_builtin::RESULT_TYPES },
    BuiltinTypeTable { name: "typelisp_rt_posix::net_builtin::INNER_TYPES", row: BuiltinTypeRow::Inner, rows: typelisp_rt_posix::net_builtin::INNER_TYPES },
    BuiltinTypeTable { name: "typelisp_rt_posix::sys::RESULT_TYPES", row: BuiltinTypeRow::Result, rows: typelisp_rt_posix::sys::RESULT_TYPES },
    BuiltinTypeTable { name: "typelisp_rt_posix::sys::INNER_TYPES", row: BuiltinTypeRow::Inner, rows: typelisp_rt_posix::sys::INNER_TYPES },
    BuiltinTypeTable { name: "typelisp_rt_sys::fs_builtin::RESULT_TYPES", row: BuiltinTypeRow::Result, rows: typelisp_rt_sys::fs_builtin::RESULT_TYPES },
    BuiltinTypeTable { name: "typelisp_rt_sys::fs_builtin::INNER_TYPES", row: BuiltinTypeRow::Inner, rows: typelisp_rt_sys::fs_builtin::INNER_TYPES },
    BuiltinTypeTable { name: "typelisp_rt_sys::env::RESULT_TYPES", row: BuiltinTypeRow::Result, rows: typelisp_rt_sys::env::RESULT_TYPES },
    BuiltinTypeTable { name: "typelisp_rt_sys::dribble::RESULT_TYPES", row: BuiltinTypeRow::Result, rows: typelisp_rt_sys::dribble::RESULT_TYPES },
];

/// A runtime crate that some targets have not got, so that a program reaching
/// one of its shims is refused for them (`compile::aot::reach`): it fails to
/// compile rather than to link or to run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RuntimeCrate {
    /// `typelisp-rt-sys`: files, the environment, the command line and
    /// `dribble`, which WASI has and `wasm32-unknown-unknown` has not.
    Sys,
    /// `typelisp-rt-posix`: sockets, OS threads and what else only an
    /// operating system answers, which WASI has not.
    Posix,
    /// `typelisp-front`'s: `eval`, which needs the checker and the
    /// interpreter, which a board without std has not.
    Front,
}

impl RuntimeCrate {
    /// The crate's shims, each with its address and what a program reaches
    /// it through.
    pub(crate) fn shims(self) -> &'static [(&'static str, Shim, &'static str)] {
        match self {
            RuntimeCrate::Sys => {
                static SHIMS: std::sync::OnceLock<[(&str, Shim, &str); 14]> = std::sync::OnceLock::new();
                SHIMS.get_or_init(sys_shims)
            }
            RuntimeCrate::Posix => {
                static SHIMS: std::sync::OnceLock<[(&str, Shim, &str); 40]> = std::sync::OnceLock::new();
                SHIMS.get_or_init(posix_shims)
            }
            RuntimeCrate::Front => {
                static SHIMS: std::sync::OnceLock<[(&str, Shim, &str); 5]> = std::sync::OnceLock::new();
                SHIMS.get_or_init(front_shims)
            }
        }
    }
}

/// What `name` is, when it is a shim of one of `missing`.
pub(crate) fn unavailable_in(missing: &[RuntimeCrate], name: &str) -> Option<&'static str> {
    missing.iter().flat_map(|runtime| runtime.shims()).find(|(shim, _, _)| *shim == name).map(|(_, _, what)| *what)
}

/// `typelisp-front`'s shims: `eval` and the two startup calls an AOT
/// executable makes to give it an environment (`aot::build_main_wrapper`).
/// The two are useless under JIT — there is already an interpreter — but they
/// are declared in every module all the same, because this table is both the
/// forward-declaration list and the JIT address map, and the wrapper looks
/// them up by name.
fn front_shims() -> [(&'static str, Shim, &'static str); 5] {
    use typelisp_front::shim::{rt_eval, rt_eval_init, rt_eval_state, rt_macroexpand, rt_macroexpand_1};
    [
        ("rt_eval", shim(rt_eval as unsafe extern "C-unwind" fn(i64) -> i64), "`eval`"),
        ("rt_macroexpand_1", shim(rt_macroexpand_1 as unsafe extern "C-unwind" fn(i64, i64) -> i64), "`macroexpand-1`"),
        ("rt_macroexpand", shim(rt_macroexpand as unsafe extern "C-unwind" fn(i64, i64) -> i64), "`macroexpand`"),
        ("rt_eval_state", shim(rt_eval_state as unsafe extern "C" fn(i64, i64, i64, i64, i64) -> i64), "`eval`"),
        ("rt_eval_init", shim(rt_eval_init as unsafe extern "C" fn() -> i64), "`eval`"),
    ]
}

/// `typelisp-rt-sys`'s shims.
fn sys_shims() -> [(&'static str, Shim, &'static str); 14] {
    use typelisp_rt_sys::{
        rt_command_line_args, rt_dribble_start, rt_dribble_stop, rt_getenv, rt_home_directory,
        rt_file_create_directories, rt_file_delete, rt_file_directory_p, rt_file_exists_p,
        rt_file_list_directory, rt_file_modified_date, rt_file_rename,
        rt_file_truename, rt_stream_open_file,
    };
    [
        ("rt_stream_open_file", shim(rt_stream_open_file as unsafe extern "C" fn(i64, i64) -> i64), "opening a file"),
        ("rt_file_exists_p", shim(rt_file_exists_p as unsafe extern "C" fn(i64) -> i64), "a file or directory operation"),
        ("rt_file_delete", shim(rt_file_delete as unsafe extern "C" fn(i64) -> i64), "a file or directory operation"),
        ("rt_file_rename", shim(rt_file_rename as unsafe extern "C" fn(i64, i64) -> i64), "a file or directory operation"),
        ("rt_file_truename", shim(rt_file_truename as unsafe extern "C" fn(i64) -> i64), "a file or directory operation"),
        ("rt_file_modified_date", shim(rt_file_modified_date as unsafe extern "C" fn(i64) -> i64), "a file or directory operation"),
        ("rt_file_directory_p", shim(rt_file_directory_p as unsafe extern "C" fn(i64) -> i64), "a file or directory operation"),
        ("rt_file_list_directory", shim(rt_file_list_directory as unsafe extern "C" fn(i64) -> i64), "a file or directory operation"),
        ("rt_file_create_directories", shim(rt_file_create_directories as unsafe extern "C" fn(i64) -> i64), "a file or directory operation"),
        ("rt_command_line_args", shim(rt_command_line_args as unsafe extern "C" fn() -> i64), "the command line's arguments"),
        ("rt_getenv", shim(rt_getenv as unsafe extern "C" fn(i64) -> i64), "an environment variable"),
        ("rt_home_directory", shim(rt_home_directory as unsafe extern "C" fn() -> i64), "the home directory"),
        ("rt_dribble_start", shim(rt_dribble_start as unsafe extern "C" fn(i64) -> i64), "`dribble`"),
        ("rt_dribble_stop", shim(rt_dribble_stop as unsafe extern "C" fn() -> i64), "`dribble`"),
    ]
}

/// `typelisp-rt-posix`'s shims. The local time zone is not here: a call
/// that passes a zone is the same function as one that asks for the local
/// one, so refusing the shim would refuse both; the runtime answers that the
/// local zone is unknown instead (`typelisp_mem::host::timezone_at`).
fn posix_shims() -> [(&'static str, Shim, &'static str); 40] {
    use typelisp_rt_posix::{
        rt_ed_open, rt_file_owner_name, rt_get_internal_run_time, rt_machine_instance, rt_software_version,
        rt_suspend_io, rt_suspend_io_for, rt_suspend_thread,
        rt_net_accept, rt_net_buffered_p, rt_net_connect_begin, rt_net_connect_finish, rt_net_fill, rt_net_flush,
        rt_net_listen, rt_net_local_address, rt_net_peer_address, rt_net_pop_byte, rt_net_pop_char,
        rt_net_push_byte, rt_net_push_string, rt_net_shutdown_write,
        rt_net_resolve_begin, rt_net_resolve_finish, rt_net_tls_handshake, rt_net_tls_start,
        rt_net_tls_listen, rt_net_socket_error, rt_net_peer_subject, rt_net_server_name, rt_net_set_nodelay,
        rt_net_set_keepalive, rt_net_set_keepalive_period, rt_net_tls_add_certificate,
        rt_net_udp_bind, rt_net_udp_last_sender, rt_net_udp_recv, rt_net_udp_send_to,
        rt_net_unix_connect, rt_net_unix_listen,
    };
    [
        ("rt_suspend_io", shim(rt_suspend_io as unsafe extern "C" fn(i64, i64) -> i64), "waiting on a socket"),
        ("rt_suspend_io_for", shim(rt_suspend_io_for as unsafe extern "C" fn(i64, i64, i64) -> i64), "waiting on a socket"),
        ("rt_net_resolve_begin", shim(rt_net_resolve_begin as unsafe extern "C" fn(i64, i64) -> i64), "a socket"),
        ("rt_net_resolve_finish", shim(rt_net_resolve_finish as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_tls_start", shim(rt_net_tls_start as unsafe extern "C" fn(i64, i64, i64, i64, i64) -> i64), "a socket"),
        ("rt_net_tls_listen", shim(rt_net_tls_listen as unsafe extern "C" fn(i64, i64, i64, i64, i64) -> i64), "a socket"),
        ("rt_net_socket_error", shim(rt_net_socket_error as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_peer_subject", shim(rt_net_peer_subject as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_server_name", shim(rt_net_server_name as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_set_nodelay", shim(rt_net_set_nodelay as unsafe extern "C" fn(i64, i64) -> i64), "a socket"),
        ("rt_net_set_keepalive", shim(rt_net_set_keepalive as unsafe extern "C" fn(i64, i64) -> i64), "a socket"),
        ("rt_net_set_keepalive_period", shim(rt_net_set_keepalive_period as unsafe extern "C" fn(i64, i64) -> i64), "a socket"),
        ("rt_net_tls_add_certificate", shim(rt_net_tls_add_certificate as unsafe extern "C" fn(i64, i64, i64, i64) -> i64), "a socket"),
        ("rt_net_tls_handshake", shim(rt_net_tls_handshake as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_udp_bind", shim(rt_net_udp_bind as unsafe extern "C" fn(i64, i64) -> i64), "a socket"),
        ("rt_net_udp_send_to", shim(rt_net_udp_send_to as unsafe extern "C" fn(i64, i64, i64) -> i64), "a socket"),
        ("rt_net_udp_recv", shim(rt_net_udp_recv as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_udp_last_sender", shim(rt_net_udp_last_sender as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_connect_begin", shim(rt_net_connect_begin as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_connect_finish", shim(rt_net_connect_finish as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_listen", shim(rt_net_listen as unsafe extern "C" fn(i64, i64) -> i64), "a socket"),
        ("rt_net_unix_connect", shim(rt_net_unix_connect as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_unix_listen", shim(rt_net_unix_listen as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_accept", shim(rt_net_accept as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_fill", shim(rt_net_fill as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_pop_byte", shim(rt_net_pop_byte as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_pop_char", shim(rt_net_pop_char as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_buffered_p", shim(rt_net_buffered_p as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_push_string", shim(rt_net_push_string as unsafe extern "C" fn(i64, i64) -> i64), "a socket"),
        ("rt_net_push_byte", shim(rt_net_push_byte as unsafe extern "C" fn(i64, i64) -> i64), "a socket"),
        ("rt_net_flush", shim(rt_net_flush as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_shutdown_write", shim(rt_net_shutdown_write as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_local_address", shim(rt_net_local_address as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_net_peer_address", shim(rt_net_peer_address as unsafe extern "C" fn(i64) -> i64), "a socket"),
        ("rt_suspend_thread", shim(rt_suspend_thread as unsafe extern "C" fn(i64, i64) -> i64), "an OS thread (`thread`)"),
        ("rt_file_owner_name", shim(rt_file_owner_name as unsafe extern "C" fn(i64) -> i64), "`file-author`"),
        ("rt_get_internal_run_time", shim(rt_get_internal_run_time as unsafe extern "C" fn() -> i64), "`get-internal-run-time`"),
        ("rt_machine_instance", shim(rt_machine_instance as unsafe extern "C" fn() -> i64), "`machine-instance`"),
        ("rt_software_version", shim(rt_software_version as unsafe extern "C" fn() -> i64), "`software-version`"),
        ("rt_ed_open", shim(rt_ed_open as unsafe extern "C" fn(i64, i64) -> i64), "`ed`, which starts an editor"),
    ]
}

/// The suspension [`needs_interpreter`]'s calls are preceded by —
/// `call_state::SUSPEND_MAIN`.
pub(crate) const RT_SUSPEND_MAIN: &str = "rt_suspend_main";

/// The shims that always wait, which an interrupt handler cannot reach
/// (`typelisp_rt::interrupt`), with what each is in a message. The others
/// that suspend may be answered at once — a `recv` with something buffered,
/// a finished task's `wait` — and are a panic in a handler only when they
/// would wait.
pub(crate) const ALWAYS_WAITING_SHIMS: &[(&str, &str)] =
    &[("rt_suspend_sleep", "`sleep`"), ("rt_suspend_io", "waiting on a socket"), ("rt_suspend_io_for", "waiting on a socket")];

/// Whether a program that reaches the shim `name` needs a scheduler: it
/// suspends for something a scheduler answers — a task, a thread, a
/// channel, a `sleep`, a socket. The two other suspensions go on at once
/// with nobody else to run: `(yield)` offers a turn, and the move to the
/// interpreter's thread ([`RT_SUSPEND_MAIN`]) has nowhere to go in an
/// executable without an interpreter. A program that reaches none of these
/// runs without a scheduler (`typelisp_rt::rt_run_program_alone`).
pub(crate) fn needs_scheduler(name: &str) -> bool {
    name.starts_with(RT_SUSPEND_PREFIX) && name != RT_SUSPEND_YIELD && name != RT_SUSPEND_MAIN
}

/// `(yield)`'s shim.
pub(crate) const RT_SUSPEND_YIELD: &str = "rt_suspend_yield";

/// The shims of what only a board has, which a program for any other target
/// cannot reach, with what each is in a message.
pub(crate) const BOARD_ONLY_SHIMS: &[(&str, &str)] =
    &[("rt_interrupt_enable", "`enable-interrupt`"), ("rt_interrupt_disable", "`disable-interrupt`")];

/// The prefix every suspending shim's name carries, and the whole of how the
/// bridge tells one from an ordinary runtime entry point.
pub(crate) const RT_SUSPEND_PREFIX: &str = "rt_suspend_";

/// The one free suspending builtin whose answer is *typed*: `net-wait-for`
/// wakes with a `bool` (ready, or the clock ran out). The others wake with
/// unit. Named here, next to the prefix rule, so the bridge has one place to
/// ask what a free suspension answers with.
pub(crate) const RT_SUSPEND_BOOL_ANSWER: &str = "rt_suspend_io_for";

/// The shim a call to `path` lowers to, when `path` is a free builtin.
///
/// [`rt_builtin_symbol`] answers for a bare name, and a name is not a
/// function: a builtin lives in the module of the standard library the
/// registry keeps it in ([`typelisp_front::types::Path::std_builtin`]),
/// and the same last segment anywhere else is somebody's own function, which
/// has to reach its own compiled body.
pub(crate) fn builtin_shim(path: &crate::Path) -> Option<&'static str> {
    let name = path.std_builtin()?;
    let shim = rt_builtin_symbol(name)?;
    (builtin_homes().get(&name.well_known()) == Some(&path.ns())).then_some(shim)
}

/// Whether [`rt_builtin_symbol`] names a shim for `name` — the "this is not a
/// call target to compile" question, asked where the shim's own name is not
/// needed.
pub(crate) fn is_rt_builtin(path: &crate::Path) -> bool {
    builtin_shim(path).is_some()
}

/// The module the registry keeps each free builtin in, by the builtin's
/// vocabulary symbol — the internal module for one the checker lowers to
/// without a signature
/// ([`typelisp_front::builtins::lowered_builtins`]).
fn builtin_homes() -> &'static std::collections::HashMap<u32, typelisp_mem::NsId> {
    static HOMES: std::sync::OnceLock<std::collections::HashMap<u32, typelisp_mem::NsId>> = std::sync::OnceLock::new();
    HOMES.get_or_init(|| {
        let mut homes: std::collections::HashMap<u32, typelisp_mem::NsId> = typelisp_front::builtins::lowered_builtins()
            .map(|k| (k, typelisp_front::types::internal_ns()))
            .collect();
        for (path, _) in typelisp_front::builtins::Builtins::new().functions() {
            let k = path.name_symbol().well_known();
            assert!(k != typelisp_mem::NOT_WELL_KNOWN, "builtin `{}` is not in the vocabulary", path);
            let prev = homes.insert(k, path.ns());
            assert!(prev.is_none(), "builtin `{}` is defined in two modules", path.last_segment());
        }
        homes
    })
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
/// [`crate::compile::driver::call_graph_edges`] needs this so it can reject — cleanly, up
/// front — a compile whose body calls a *non*-native primitive builtin
/// (`i64::int->char`, `char::equalp`, ...): those have no compiled lowering
/// *and* no function to link, so left to reach the island they hit its
/// `get-function` guard, which under the AOT-native island is a hard
/// `rt_llvm_call` process abort rather than a catchable error. Catching them
/// here keeps `(compile bad-fn)` a clean `EvalError` — the behavior the
/// interpreted island used to give from `get-function` directly.
pub(crate) fn is_native_lowered_primitive_method(type_name: &crate::Path, method: SymRef) -> bool {
    typelisp_front::types::builtin_type(type_name).is_some_and(|ty| native_lowered_primitive_methods(ty).contains(&method.well_known()))
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
pub(crate) fn native_lowered_primitive_methods(ty: u32) -> &'static [u32] {
    match ty {
        // `eq`/`eql`/`equal`/`equalp` are four names for `=` on an integer
        // (`registry::int_assoc`'s doc comment: same-type operands, nothing to
        // fold or recurse into), so all four are the same `icmp eq`. Only
        // `eq` used to be here, which is what made `case` — whose expansion
        // compares with `equal` — uncompilable for every integer scrutinee.
        // `c-long`/`c-ulong` share this arm: to the island they are integer
        // receivers like any other (`int-receiver-type?`), and what makes them
        // different is upstream — the checker registers only conversions for
        // them, so the arithmetic listed here can never be asked for.
        wk::I32 | wk::I8 | wk::I16 | wk::U8 | wk::U16 | wk::U32 | wk::C_LONG | wk::C_ULONG => &[
            wk::M_OP_ADD, wk::M_OP_SUB, wk::M_OP_MUL, wk::M_OP_DIV, wk::M_MOD, wk::M_REM, wk::M_OP_LT, wk::M_OP_LE, wk::M_OP_GT, wk::M_OP_GE, wk::RETIRED_EQUALS, wk::M_EQ, wk::M_EQL, wk::M_EQUAL, wk::M_EQUALP, wk::M_OP_NE,
            wk::M_INT_TO_INT, wk::M_INT_TO_RATIO, wk::M_INT_TO_FLOAT, wk::M_INT_TO_CHAR,
            wk::M_INT_TO_I8, wk::M_INT_TO_I16, wk::M_INT_TO_I32, wk::M_INT_TO_U8, wk::M_INT_TO_U16, wk::M_INT_TO_U32,
            wk::M_INT_TO_C_LONG, wk::M_INT_TO_C_ULONG,
            // The `Option`-returning halves. No prelude definition reaches
            // them; they are lowered so a user's own `(try-as u8 n)` can be
            // compiled — see `docs/ja/reference/syntax.md` §10.
            wk::M_TRY_INT_TO_CHAR,
            wk::M_TRY_INT_TO_I8, wk::M_TRY_INT_TO_I16, wk::M_TRY_INT_TO_I32, wk::M_TRY_INT_TO_U8, wk::M_TRY_INT_TO_U16, wk::M_TRY_INT_TO_U32,
            wk::M_MAX, wk::M_MIN, wk::M_LOGAND, wk::M_LOGIOR, wk::M_LOGXOR, wk::M_LOGTEST, wk::M_LOGNOT, wk::M_LOGCOUNT, wk::M_INTEGER_LENGTH,
            wk::M_ASH, wk::M_LOGBITP,
        ],
        // `eq`/`eql` are `StrId` identity (`string_identity_eq`) and lower to
        // an `icmp eq` on the two tagged words; `equal`/`equalp` are content
        // comparisons and go through `rt_str_eq`/`rt_str_equalp`.
        wk::STRING => &[
            wk::M_LENGTH, wk::M_REF, wk::M_EQ, wk::M_EQL, wk::M_EQUAL, wk::M_EQUALP, wk::M_LT, wk::M_OP_LT, wk::M_OP_LE, wk::M_OP_GT, wk::M_OP_GE, wk::M_APPEND, wk::M_SUBSTRING,
            // `upcase`/`downcase` are one `rt_str_*` call each, ASCII-only on
            // both tiers because both tiers run the same shim's rule — the
            // same shape and the same reasoning as `char`'s pair below.
            wk::M_UPCASE, wk::M_DOWNCASE,
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
        wk::CHAR => &[
            wk::M_EQ, wk::M_EQL, wk::M_EQUAL, wk::M_EQUALP, wk::M_LT, wk::M_OP_LT, wk::M_OP_LE, wk::M_OP_GT, wk::M_OP_GE, wk::M_CHAR_TO_INT, wk::M_CHAR_TO_STRING,
            wk::M_UPCASE, wk::M_DOWNCASE, wk::M_ALPHAP, wk::M_DIGITP,
        ],
        wk::F64 | wk::F32 => &[
            wk::M_OP_ADD, wk::M_OP_SUB, wk::M_OP_MUL, wk::M_OP_DIV, wk::M_EXPT, wk::M_SQRT, wk::M_FLOOR, wk::M_CEILING, wk::M_ROUND, wk::M_TRUNCATE,
            wk::M_FLOAT_TO_INT, wk::M_FLOAT_TO_RATIO, wk::M_FLOAT_TO_F32, wk::M_FLOAT_TO_F64,
            // Same reason as the integer `try-*` group above: no prelude
            // caller, lowered so a user's `(try-as f32 x)` can compile.
            wk::M_TRY_FLOAT_TO_F32, wk::M_TRY_FLOAT_TO_F64,
            wk::M_OP_LT, wk::M_OP_LE, wk::M_OP_GT, wk::M_OP_GE, wk::RETIRED_EQUALS, wk::M_OP_NE, wk::M_EQ, wk::M_EQL, wk::M_EQUAL, wk::M_EQUALP,
            wk::M_MAX, wk::M_MIN, wk::M_SIN, wk::M_COS, wk::M_TAN, wk::M_ASIN, wk::M_ACOS, wk::M_ATAN, wk::M_SINH, wk::M_COSH, wk::M_TANH,
            wk::M_ASINH, wk::M_ACOSH, wk::M_ATANH, wk::M_EXP, wk::M_LOG,
        ],
        // `int` (fixnum ∪ bignum): everything `integer_assoc` registers is
        // lowered — the arithmetic and comparisons as an overflow-checked
        // fast path over two fixnum words with an `rt_integer_*` slow path,
        // the rest as one `rt_integer_*` call each.
        wk::INT => &[
            wk::M_OP_ADD, wk::M_OP_SUB, wk::M_OP_MUL, wk::M_OP_DIV, wk::M_TRUNCATE, wk::M_MOD, wk::M_OP_LT, wk::M_OP_LE, wk::M_OP_GT, wk::M_OP_GE, wk::RETIRED_EQUALS, wk::M_OP_NE, wk::M_EQ, wk::M_EQL, wk::M_EQUAL, wk::M_EQUALP,
            wk::M_MAX, wk::M_MIN, wk::M_LOGAND, wk::M_LOGIOR, wk::M_LOGXOR, wk::M_LOGTEST, wk::M_LOGNOT, wk::M_LOGCOUNT, wk::M_INTEGER_LENGTH,
            wk::M_ASH, wk::M_LOGBITP,
            wk::M_INT_TO_FLOAT, wk::M_INT_TO_RATIO, wk::M_INT_TO_INT, wk::M_INT_TO_CHAR, wk::M_TRY_INT_TO_CHAR,
            wk::M_INT_TO_I8, wk::M_INT_TO_I16, wk::M_INT_TO_I32, wk::M_INT_TO_U8, wk::M_INT_TO_U16, wk::M_INT_TO_U32,
            wk::M_INT_TO_C_LONG, wk::M_INT_TO_C_ULONG,
            wk::M_TRY_INT_TO_I8, wk::M_TRY_INT_TO_I16, wk::M_TRY_INT_TO_I32, wk::M_TRY_INT_TO_U8, wk::M_TRY_INT_TO_U16, wk::M_TRY_INT_TO_U32,
        ],
        wk::RATIO => &[
            wk::M_OP_ADD, wk::M_OP_SUB, wk::M_OP_MUL, wk::M_OP_DIV, wk::M_OP_LT, wk::M_OP_LE, wk::M_OP_GT, wk::M_OP_GE, wk::RETIRED_EQUALS, wk::M_OP_NE, wk::M_EQ, wk::M_EQL, wk::M_EQUAL, wk::M_EQUALP,
            wk::M_RATIO_TO_INT, wk::M_RATIO_TO_FLOAT, wk::M_NUMERATOR, wk::M_DENOMINATOR, wk::M_MAX, wk::M_MIN, wk::M_RATIO_TO_RATIONAL,
        ],
        // `rational` (`int` ∪ `ratio`): one `rt_rational_*` call each.
        wk::REPR_RATIONAL => &[
            wk::M_OP_ADD, wk::M_OP_SUB, wk::M_OP_MUL, wk::M_OP_DIV, wk::M_OP_LT, wk::M_OP_LE, wk::M_OP_GT, wk::M_OP_GE, wk::RETIRED_EQUALS, wk::M_OP_NE, wk::M_EQ, wk::M_EQL, wk::M_EQUAL, wk::M_EQUALP, wk::M_MAX, wk::M_MIN,
            wk::M_RATIONAL_TO_INT, wk::M_TRY_RATIONAL_TO_INT, wk::M_RATIONAL_TO_RATIO, wk::M_RATIONAL_TO_FLOAT, wk::M_NUMERATOR, wk::M_DENOMINATOR,
        ],
        // `Sexpr` values are raw tagged `i64` handles in compiled code, and
        // interned symbols/`nil`/small atoms are handle-identical, so `eq`
        // (CL identity) lowers to a plain `icmp eq` on the two handles —
        // `compile-assoc`'s `sexpr` arm, mirroring the `char` branch. `eql`
        // cannot be that instruction (two separately boxed but equal
        // `Float`/`bignum`/`ratio` values are `eql` and not `eq`), so it
        // lowers to the `rt_sexpr_eql` shim over the same
        // `typelisp_rt::equality` the interpreter uses, and `equal` to the
        // `rt_sexpr_equal` shim. The free-function `eq`/`eql`/`equalp` reach
        // their shims through `rt_builtin_symbol` instead, being calls rather
        // than methods.
        wk::SEXPR => &[wk::M_EQ, wk::M_EQL, wk::M_EQUAL],
        // `bool`'s four comparison names are one operation
        // (`registry::bool_assoc`: two immediate values, nothing to fold or
        // recurse into), and a compiled `bool` is a raw `0`/`1`, so all four
        // are the same `icmp eq`.
        wk::BOOL => &[wk::M_EQ, wk::M_EQL, wk::M_EQUAL, wk::M_EQUALP],
        // A compiled `symbol` is the interned `Value::Symbol` handle, so
        // identity *is* handle equality — the same `icmp eq` as `sexpr`'s
        // `eq`. `registry::symbol_assoc` registers only these two.
        wk::SYMBOL => &[wk::M_EQ, wk::M_EQL],
        _ => &[],
    }
}

/// The fixed set of `typelisp_rt` shims every compiled function
/// gets forward-declared and (JIT only — AOT resolves them as ordinary
/// linker symbols against `typelisp-rt`'s `staticlib`, see
/// `compile::aot::compile_file`) linked against by address, regardless
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
pub(crate) fn rt_extern_functions() -> &'static [(&'static str, Shim)] {
    static ALL: std::sync::OnceLock<Vec<(&'static str, Shim)>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut shims = rt_shims().to_vec();
        for runtime in [RuntimeCrate::Sys, RuntimeCrate::Posix, RuntimeCrate::Front] {
            shims.extend(runtime.shims().iter().map(|(name, shim, _)| (*name, shim.clone())));
        }
        shims
    })
}

/// The functions only an AOT executable's generated `main` calls: the heap
/// it makes and the entries that run the program. Not in
/// [`rt_extern_functions`], which every module declares; `main` declares
/// the ones it calls.
fn entry_shims() -> &'static [(&'static str, Shim)] {
    static ENTRIES: std::sync::OnceLock<[(&'static str, Shim); 13]> = std::sync::OnceLock::new();
    ENTRIES.get_or_init(|| {
        use typelisp_rt::{
            rt_drive_body, rt_heap_init, rt_main_answered, rt_main_returned, rt_run_entry, rt_run_program, rt_run_program_alone, rt_run_program_alone_int,
            rt_run_program_int,
        };
        [
            ("rt_heap_init", shim(rt_heap_init as unsafe extern "C" fn(i64, i64, i64) -> i64)),
            ("rt_main_answered", shim(rt_main_answered as unsafe extern "C" fn(i64) -> i64)),
            ("rt_main_returned", shim(rt_main_returned as unsafe extern "C" fn() -> i64)),
            ("rt_run_entry", shim(rt_run_entry as unsafe extern "C" fn(i64) -> i64)),
            ("rt_drive_body", shim(rt_drive_body as unsafe extern "C-unwind" fn(i64, i64, i64) -> i64)),
            ("rt_run_program", shim(rt_run_program as unsafe extern "C" fn(i64, i64, i64) -> i64)),
            ("rt_run_program_int", shim(rt_run_program_int as unsafe extern "C" fn(i64, i64, i64) -> i64)),
            ("rt_run_program_alone", shim(rt_run_program_alone as unsafe extern "C" fn(i64, i64, i64) -> i64)),
            ("rt_run_program_alone_int", shim(rt_run_program_alone_int as unsafe extern "C" fn(i64, i64, i64) -> i64)),
            ("rt_run_program_threads", shim(typelisp_rt_posix::threads::rt_run_program_threads as unsafe extern "C" fn(i64, i64, i64) -> i64)),
            ("rt_run_program_threads_int", shim(typelisp_rt_posix::threads::rt_run_program_threads_int as unsafe extern "C" fn(i64, i64, i64) -> i64)),
            ("rt_run_program_interp", shim(typelisp_front::shim::rt_run_program_interp as unsafe extern "C" fn(i64, i64, i64) -> i64)),
            ("rt_run_program_interp_int", shim(typelisp_front::shim::rt_run_program_interp_int as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        ]
    })
}

/// Every runtime function compiled code can call: [`rt_extern_functions`]
/// and the program entries a generated `main` calls.
pub(crate) fn runtime_function_names_with_entries() -> impl Iterator<Item = &'static str> {
    rt_extern_functions().iter().chain(entry_shims()).map(|(name, _)| *name)
}

/// The signature of the runtime function `name`, if it is one.
pub(crate) fn runtime_sig(name: &str) -> Option<&'static crate::compile::runtime_abi::Sig> {
    static BY_NAME: std::sync::OnceLock<std::collections::HashMap<&'static str, &'static Shim>> = std::sync::OnceLock::new();
    BY_NAME
        .get_or_init(|| rt_extern_functions().iter().chain(entry_shims()).map(|(name, shim)| (*name, shim)).collect())
        .get(name)
        .map(|shim| &shim.sig)
}

/// Declares every runtime function in `module`, each under its own
/// signature: what a module the island compiles into needs before any body
/// is translated, since its `get-function` aborts on a name it cannot find.
pub(crate) fn declare_runtime_functions(module: &inkwell::module::Module<'static>) {
    for (name, shim) in rt_extern_functions() {
        crate::compile::runtime_abi::declare(module, name, &shim.sig);
    }
}

/// `(name, address)` of every runtime function, for a JIT's symbol map.
pub(crate) fn runtime_addresses() -> impl Iterator<Item = (&'static str, usize)> {
    rt_extern_functions().iter().map(|(name, shim)| (*name, shim.addr))
}

/// The shims every target has: `typelisp-rt`'s, the printer's, the reader's
/// and `rt_llvm_call`.
fn rt_shims() -> [(&'static str, Shim); 353] {
    use typelisp_rt::c_mem::{
        rt_c_alloc, rt_c_arena_close, rt_c_arena_open, rt_c_index, rt_c_load, rt_c_offset, rt_c_ptr_check, rt_c_store,
        rt_c_volatile_load, rt_c_volatile_store,
    };
    use typelisp_rt::interrupt::{rt_interrupt_disable, rt_interrupt_enable, rt_interrupt_register};
    use typelisp_rt::equality::{rt_sexpr_eq, rt_sexpr_eql, rt_sexpr_equal, rt_sexpr_equalp, rt_sxhash_eq, rt_sxhash_eql, rt_sxhash_equalp};
    use typelisp_rt::ffi_callback::{
        rt_ffi_callback_address, rt_ffi_callback_enter, rt_ffi_callback_invoke, rt_ffi_callback_leave,
        rt_ffi_callback_register,
    };
    // The printing family. These are the one group of shims defined outside
    // `typelisp-rt` — see `typelisp_print::shim`'s module doc comment for why
    // the linker requires that.
    use typelisp_print::shim::{
        rt_pprint, rt_pprint_block_end, rt_pprint_block_start, rt_pprint_indent,
        rt_pprint_list_exhausted, rt_pprint_newline, rt_pprint_pop, rt_pprint_tab,
    };
    use typelisp_print::shim::{
        rt_fmt_open, rt_fmt_text, rt_fmt_fresh_line, rt_fmt_string, rt_fmt_number, rt_fmt_value, rt_fmt_integer, rt_fmt_integer_any, rt_fmt_words, rt_fmt_plural, rt_fmt_char, rt_fmt_fixed, rt_fmt_exponent, rt_fmt_general, rt_fmt_dollars, rt_fmt_tab, rt_fmt_newline, rt_fmt_indent, rt_fmt_case_open, rt_fmt_case_close, rt_fmt_just_open, rt_fmt_just_segment, rt_fmt_just_close, rt_fmt_block_open, rt_fmt_block_close, rt_fmt_result, rt_fmt_emit, rt_fmt_first, rt_fmt_rest, rt_fmt_empty, rt_fmt_length, rt_fmt_goto, rt_fmt_as_list, rt_fmt_param, rt_fmt_select, rt_fmt_bool,
    };
    use typelisp_print::aot::{
        rt_print_enum_variant, rt_print_field_name, rt_print_field_template, rt_print_global,
        rt_print_names, rt_print_object_method,
    };
    use typelisp_read::shim::{rt_read, rt_read_datum_at};
    use typelisp_rt::coroutine::{
        rt_loop_safepoint, rt_suspend_chan_cap, rt_suspend_chan_close, rt_suspend_chan_len,
        rt_suspend_chan_new, rt_suspend_chan_recv, rt_suspend_chan_select, rt_suspend_chan_send,
        rt_suspend_task, rt_suspend_main, rt_suspend_sleep,
        rt_suspend_wait, rt_suspend_yield,
    };
    use typelisp_rt::sys_builtin::{
        rt_exit,
        rt_get_internal_real_time,
        rt_get_universal_time, rt_heap_info, rt_lisp_implementation_version,
        rt_machine_type, rt_machine_version, rt_parse_float,
        rt_software_type, rt_thread_available_parallelism, rt_thread_current_id,
        rt_timezone_daylight_p, rt_timezone_offset_seconds,
    };
    use typelisp_rt::rational::{
        rt_rational_add, rt_rational_sub, rt_rational_mul, rt_rational_div, rt_rational_cmp, rt_rational_from_ratio, rt_rational_to_ratio, rt_rational_to_int, rt_rational_is_int, rt_rational_to_float, rt_rational_numerator, rt_rational_denominator,
    };
    use typelisp_rt::integer::{
        rt_integer_add, rt_integer_ash, rt_integer_cmp, rt_integer_div, rt_integer_fits_char,
        rt_integer_from_word, rt_integer_from_c_ulong, rt_integer_integer_length, rt_integer_logand, rt_integer_logbitp, rt_integer_logcount,
        rt_integer_logior, rt_integer_lognot, rt_integer_logtest, rt_integer_logxor, rt_integer_mod, rt_integer_mul,
        rt_integer_to_i8, rt_integer_to_i16, rt_integer_to_i32, rt_integer_to_c_long, rt_integer_to_u8, rt_integer_to_u16, rt_integer_to_u32, rt_integer_to_c_ulong,
        rt_integer_fits_i8, rt_integer_fits_i16, rt_integer_fits_i32, rt_integer_fits_u8, rt_integer_fits_u16, rt_integer_fits_u32, rt_integer_sub, rt_integer_to_char, rt_integer_to_float,
        rt_integer_to_ratio, rt_int_not_fixnum,
    };
    use typelisp_rt::{
        rt_atom, rt_bignum_new,
        rt_make_random_state_fresh, rt_random_state_copy, rt_random_state_next, rt_seed_random_state,
        rt_stream_at_line_start, rt_stream_close,
        rt_stream_finish_output, rt_stream_input_p, rt_stream_listen, rt_stream_open_p, rt_stream_describe,
        rt_stream_output_p, rt_stream_position, rt_stream_read_byte, rt_stream_read_char, rt_stream_stderr, rt_stream_stdin, rt_stream_stdout,
        rt_set_macro_character, rt_get_macro_character, rt_set_dispatch_macro_character, rt_get_dispatch_macro_character,
        rt_stream_string_input, rt_stream_string_output, rt_stream_take_output_string, rt_stream_unread_char,
        rt_stream_write_byte, rt_stream_write_string,
        rt_box_kind, rt_car, rt_cdr,
        rt_cell_get, rt_cell_new, rt_cell_set, rt_char_alphap, rt_char_digitp, rt_char_downcase,
        rt_char_equalp, rt_char_upcase, rt_int_to_char, rt_closure_env_get, rt_closure_env_len,
        rt_closure_fnptr, rt_closure_new, rt_coroutine_closure_new, rt_cons, rt_consp, rt_data_field, rt_data_new, rt_data_variant, rt_f64_new, rt_f32_new, rt_float_to_int,
        rt_i8_box, rt_i16_box, rt_i32_box, rt_u8_box, rt_u16_box, rt_u32_box, rt_i8_unbox, rt_i16_unbox, rt_i32_unbox, rt_u8_unbox, rt_u16_unbox, rt_u32_unbox,
        rt_float_to_ratio, rt_f64_value, rt_f32_value, rt_global_get, rt_global_new, rt_global_set, rt_int_div_fault, rt_int_mod_fault,
        rt_f32_tan, rt_f32_asin, rt_f32_acos, rt_f32_atan, rt_f32_sinh, rt_f32_cosh, rt_f32_tanh, rt_f32_asinh, rt_f32_acosh, rt_f32_atanh,
        rt_f64_tan, rt_f64_asin, rt_f64_acos, rt_f64_atan, rt_f64_sinh, rt_f64_cosh, rt_f64_tanh, rt_f64_asinh, rt_f64_acosh,
        rt_f64_atanh,
        rt_hashtable_bucket_count, rt_hashtable_bucket_delete, rt_hashtable_bucket_key, rt_hashtable_bucket_put,
        rt_hashtable_bucket_value,
        rt_hashtable_clear, rt_hashtable_count, rt_hashtable_entries, rt_hashtable_keys,
        rt_hashtable_function, rt_hashtable_new, rt_hashtable_values, rt_int_to_ratio,
        rt_intern, rt_intern_path, rt_intern_symbol, rt_keywordp, rt_wk_symbol, rt_list_to_path, rt_list_to_sexpr_vector, rt_lists_to_sexpr_array, rt_list_to_sexpr_tuple, rt_vector_to_sexpr_tuple, rt_sexpr_tuple_items, rt_match_fail, rt_null, rt_panic, rt_path_to_list, rt_pop_sexpr_root, rt_push_permanent_sexpr_root,
        rt_push_sexpr_root, rt_ratio_add, rt_ratio_cmp, rt_ratio_denominator, rt_ratio_div, rt_ratio_from_bignums, rt_ratio_mul,
        rt_ratio_numerator, rt_ratio_sub, rt_ratio_to_int, rt_ratio_to_float, rt_root_count, rt_set_car, rt_set_cdr,
        rt_set_sexpr_root, rt_sexpr_bool, rt_sexpr_char, rt_sexpr_f32, rt_sexpr_f64, rt_sexpr_instance_test, rt_sexpr_int, rt_sexpr_i32, rt_sexpr_i8, rt_sexpr_i16, rt_sexpr_u8, rt_sexpr_u16, rt_sexpr_u32, rt_sexpr_str, rt_str_append, rt_str_eq, rt_str_equalp,
        rt_ffi_cstring_new, rt_ffi_cstring_free, rt_ffi_string_from_cstr, rt_ffi_enter_native, rt_ffi_leave_native,
        rt_str_length, rt_str_lt, rt_str_new, rt_str_ref, rt_str_substring, rt_str_upcase, rt_str_downcase, rt_int_fits_char, rt_f64_fits_f32, rt_struct_field_count, rt_struct_field_get, rt_struct_field_set,
        rt_struct_new, rt_struct_pop_field, rt_struct_push_field, rt_sym_name, rt_symp, rt_truncate_sexpr_roots,
        rt_dyn_new, rt_dyn_upcast, rt_dyn_value, rt_dyn_vtable, rt_types_define, rt_upcast_set, rt_vtable_set,
        rt_throw, rt_throw_matches, rt_throw_take_value, rt_unwinding,
        rt_frame_call, rt_frame_open, rt_frame_call_env, rt_frame_apply, rt_frame_dyn_call, rt_pending_arg, rt_pending_argc,
        rt_pending_env, rt_pending_envc, rt_local_frame_open, rt_local_frame_close,
    };
    [
        // The one main-crate entry: the generic `llvm-*`/native-scope
        // builtin dispatch shim (interp-closure removal Stage 1) — it can't
        // live in `typelisp-rt` because it calls into the `inkwell`-backed
        // [`eval_llvm_builtin_method`]. Never referenced by AOT-linked user
        // executables (LLVM handle types are unreachable from user code, so
        // `compile-file` output never emits a call to it — an unreferenced
        // declaration emits no symbol for the linker to miss).
        ("rt_llvm_call", shim(crate::compile::llvm_builtins::rt_llvm_call as unsafe extern "C" fn(*const i64, u32) -> i64)),
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
        ("rt_dyn_new", shim(rt_dyn_new as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_dyn_vtable", shim(rt_dyn_vtable as unsafe extern "C" fn(i64) -> i64)),
        ("rt_dyn_value", shim(rt_dyn_value as unsafe extern "C" fn(i64) -> i64)),
        ("rt_dyn_upcast", shim(rt_dyn_upcast as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_vtable_set", shim(rt_vtable_set as unsafe extern "C" fn(i64, i64, i64, i64) -> i64)),
        ("rt_upcast_set", shim(rt_upcast_set as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        // The type table an executable's startup fills from the shapes it
        // carries (`typelisp_mem::type_words`); AOT-only like the two above.
        ("rt_types_define", shim(rt_types_define as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        // The small system builtins (`typelisp_rt::sys_builtin`): parsing,
        // the clock, and process exit. Each was a gap that made every
        // function calling it uncompilable, and each closed the same way the
        // stream family did — one implementation, two edges.
        // The three structural equality predicates
        // (`typelisp_rt::equality`). `eq` needs no shim — it is the `icmp eq`
        // the island emits in place.
        ("rt_sexpr_eq", shim(rt_sexpr_eq as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_sxhash_eq", shim(rt_sxhash_eq as unsafe extern "C" fn(i64) -> i64)),
        ("rt_sxhash_eql", shim(rt_sxhash_eql as unsafe extern "C" fn(i64) -> i64)),
        ("rt_sxhash_equalp", shim(rt_sxhash_equalp as unsafe extern "C" fn(i64) -> i64)),
        ("rt_hashtable_function", shim(rt_hashtable_function as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_sexpr_eql", shim(rt_sexpr_eql as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_sexpr_equal", shim(rt_sexpr_equal as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_sexpr_equalp", shim(rt_sexpr_equalp as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_parse_float", shim(rt_parse_float as unsafe extern "C" fn(i64) -> i64)),
        ("rt_get_universal_time", shim(rt_get_universal_time as unsafe extern "C" fn() -> i64)),
        ("rt_get_internal_real_time", shim(rt_get_internal_real_time as unsafe extern "C" fn() -> i64)),
        ("rt_exit", shim(rt_exit as unsafe extern "C" fn(i64) -> i64)),
        ("rt_read", shim(rt_read as unsafe extern "C" fn(i64) -> i64)),
        ("rt_read_datum_at", shim(rt_read_datum_at as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        // The printing family (`typelisp_print::shim`). `format`/`print`/
        // `println`/`pprint` and `pprint-logical-block` are special forms, so
        // what reaches here are the `*-rt` names the checker lowered them to;
        // the four `pprint-*` operators are ordinary builtins.
        ("rt_pprint", shim(rt_pprint as unsafe extern "C-unwind" fn(i64, i64, i64) -> i64)),
        ("rt_pprint_block_start", shim(rt_pprint_block_start as unsafe extern "C-unwind" fn(i64, i64, i64, i64) -> i64)),
        ("rt_pprint_block_end", shim(rt_pprint_block_end as unsafe extern "C-unwind" fn() -> i64)),
        ("rt_pprint_newline", shim(rt_pprint_newline as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_pprint_indent", shim(rt_pprint_indent as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_pprint_tab", shim(rt_pprint_tab as unsafe extern "C-unwind" fn(i64, i64, i64) -> i64)),
        ("rt_pprint_pop", shim(rt_pprint_pop as unsafe extern "C-unwind" fn() -> i64)),
        ("rt_pprint_list_exhausted", shim(rt_pprint_list_exhausted as unsafe extern "C-unwind" fn() -> i64)),
        ("rt_fmt_open", shim(rt_fmt_open as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_text", shim(rt_fmt_text as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_fresh_line", shim(rt_fmt_fresh_line as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_string", shim(rt_fmt_string as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_number", shim(rt_fmt_number as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_value", shim(rt_fmt_value as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_integer", shim(rt_fmt_integer as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_integer_any", shim(rt_fmt_integer_any as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_words", shim(rt_fmt_words as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_plural", shim(rt_fmt_plural as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_char", shim(rt_fmt_char as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_fixed", shim(rt_fmt_fixed as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_exponent", shim(rt_fmt_exponent as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_general", shim(rt_fmt_general as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_dollars", shim(rt_fmt_dollars as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_tab", shim(rt_fmt_tab as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_newline", shim(rt_fmt_newline as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_indent", shim(rt_fmt_indent as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_case_open", shim(rt_fmt_case_open as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_case_close", shim(rt_fmt_case_close as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_just_open", shim(rt_fmt_just_open as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_just_segment", shim(rt_fmt_just_segment as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_just_close", shim(rt_fmt_just_close as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_block_open", shim(rt_fmt_block_open as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_block_close", shim(rt_fmt_block_close as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_result", shim(rt_fmt_result as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_emit", shim(rt_fmt_emit as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_first", shim(rt_fmt_first as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_rest", shim(rt_fmt_rest as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_empty", shim(rt_fmt_empty as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_length", shim(rt_fmt_length as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_goto", shim(rt_fmt_goto as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_as_list", shim(rt_fmt_as_list as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_param", shim(rt_fmt_param as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_select", shim(rt_fmt_select as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_fmt_bool", shim(rt_fmt_bool as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        // The printer's AOT startup registration (`typelisp_print::aot`):
        // an enum's variant names and each type's `print-object`, which a
        // standalone executable cannot look up the way the interpreter does.
        // AOT-only, like `rt_vtable_set`/`rt_upcast_set` above — the JIT
        // installs `INTERP_PRINT_HOOKS` instead, so nothing emits a call to
        // either there.
        ("rt_print_enum_variant", shim(rt_print_enum_variant as unsafe extern "C" fn(i64, i64, i64, i64) -> i64)),
        ("rt_print_field_template", shim(rt_print_field_template as unsafe extern "C" fn(i64, i64, i64, i64) -> i64)),
        ("rt_print_field_name", shim(rt_print_field_name as unsafe extern "C" fn(i64, i64, i64, i64) -> i64)),
        ("rt_print_object_method", shim(rt_print_object_method as unsafe extern "C" fn(i64, i64) -> i64)),
        // Each printer control variable's compiled global id, so the
        // executable's printer reads the value the program `setf`'d. AOT-only:
        // the interpreter reads its own globals.
        ("rt_print_global", shim(rt_print_global as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        // The module the executable's type names are written for, and the
        // types it has brought in. AOT-only: the interpreter's checker sets
        // them as it moves between modules.
        ("rt_print_names", shim(rt_print_names as unsafe extern "C" fn(i64) -> i64)),
        ("rt_car", shim(rt_car as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_cdr", shim(rt_cdr as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_cons", shim(rt_cons as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_consp", shim(rt_consp as unsafe extern "C" fn(i64) -> i64)),
        ("rt_null", shim(rt_null as unsafe extern "C" fn(i64) -> i64)),
        ("rt_atom", shim(rt_atom as unsafe extern "C" fn(i64) -> i64)),
        ("rt_symp", shim(rt_symp as unsafe extern "C" fn(i64) -> i64)),
        ("rt_sexpr_int", shim(rt_sexpr_int as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_i32", shim(rt_sexpr_i32 as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_i8", shim(rt_sexpr_i8 as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_i16", shim(rt_sexpr_i16 as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_u8", shim(rt_sexpr_u8 as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_u16", shim(rt_sexpr_u16 as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_u32", shim(rt_sexpr_u32 as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_bool", shim(rt_sexpr_bool as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_char", shim(rt_sexpr_char as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_f64", shim(rt_sexpr_f64 as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_f32", shim(rt_sexpr_f32 as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_str", shim(rt_sexpr_str as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sym_name", shim(rt_sym_name as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_set_car", shim(rt_set_car as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_set_cdr", shim(rt_set_cdr as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_match_fail", shim(rt_match_fail as unsafe extern "C" fn() -> i64)),
        ("rt_panic", shim(rt_panic as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_frame_call", shim(rt_frame_call as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_frame_open", shim(rt_frame_open as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_local_frame_open", shim(rt_local_frame_open as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_local_frame_close", shim(rt_local_frame_close as unsafe extern "C" fn(i64) -> i64)),
        ("rt_frame_apply", shim(rt_frame_apply as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_frame_dyn_call", shim(rt_frame_dyn_call as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_frame_call_env", shim(rt_frame_call_env as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_pending_arg", shim(rt_pending_arg as unsafe extern "C" fn(i64) -> i64)),
        ("rt_pending_argc", shim(rt_pending_argc as unsafe extern "C" fn() -> i64)),
        ("rt_pending_env", shim(rt_pending_env as unsafe extern "C" fn(i64) -> i64)),
        ("rt_pending_envc", shim(rt_pending_envc as unsafe extern "C" fn() -> i64)),
        // `catch`/`throw`/`unwind-protect` (`compiler.rs`'s `compile-catch`/
        // `compile-throw`/`compile-unwind-protect`). `rt_throw` raises the
        // unwind; the two queries are what a region's dispatch block asks
        // about the one in flight. Nothing here catches: since C4 that is the
        // driver's job, at the boundary of the activation that raised — see
        // the catch/throw section of `typelisp-rt` for why a landing pad
        // cannot do it and why one catch per tier is enough.
        ("rt_throw", shim(rt_throw as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_throw_matches", shim(rt_throw_matches as unsafe extern "C" fn(i64) -> i64)),
        ("rt_throw_take_value", shim(rt_throw_take_value as unsafe extern "C" fn() -> i64)),
        ("rt_unwinding", shim(rt_unwinding as unsafe extern "C" fn() -> i64)),
        ("rt_push_sexpr_root", shim(rt_push_sexpr_root as unsafe extern "C" fn(i64) -> i64)),
        ("rt_pop_sexpr_root", shim(rt_pop_sexpr_root as unsafe extern "C" fn() -> i64)),
        ("rt_push_permanent_sexpr_root", shim(rt_push_permanent_sexpr_root as unsafe extern "C" fn(i64) -> i64)),
        ("rt_root_count", shim(rt_root_count as unsafe extern "C" fn() -> i64)),
        ("rt_set_sexpr_root", shim(rt_set_sexpr_root as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_truncate_sexpr_roots", shim(rt_truncate_sexpr_roots as unsafe extern "C" fn(i64) -> i64)),
        // The C FFI's string conversions, called by a thunk rather than by
        // any compiled typelisp — see `crate::compile::ffi`.
        ("rt_ffi_cstring_new", shim(rt_ffi_cstring_new as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_ffi_cstring_free", shim(rt_ffi_cstring_free as unsafe extern "C" fn(i64) -> i64)),
        ("rt_ffi_string_from_cstr", shim(rt_ffi_string_from_cstr as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_ffi_enter_native", shim(rt_ffi_enter_native as unsafe extern "C" fn() -> i64)),
        ("rt_ffi_leave_native", shim(rt_ffi_leave_native as unsafe extern "C-unwind" fn() -> i64)),
        ("rt_ffi_callback_address", shim(rt_ffi_callback_address as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_ffi_callback_register", shim(rt_ffi_callback_register as unsafe extern "C" fn(i64, i64, i64, i64) -> i64)),
        ("rt_ffi_callback_enter", shim(rt_ffi_callback_enter as unsafe extern "C" fn() -> i64)),
        ("rt_ffi_callback_invoke", shim(rt_ffi_callback_invoke as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_ffi_callback_leave", shim(rt_ffi_callback_leave as unsafe extern "C" fn() -> i64)),
        // The memory behind `def-c-struct`s and typed pointers
        // (`typelisp_front::c_struct`).
        ("rt_c_arena_open", shim(rt_c_arena_open as unsafe extern "C" fn() -> i64)),
        ("rt_c_arena_close", shim(rt_c_arena_close as unsafe extern "C" fn(i64) -> i64)),
        ("rt_c_alloc", shim(rt_c_alloc as unsafe extern "C-unwind" fn(*const i64, u32) -> i64)),
        ("rt_c_ptr_check", shim(rt_c_ptr_check as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_c_index", shim(rt_c_index as unsafe extern "C-unwind" fn(i64, i64, i64, i64) -> i64)),
        ("rt_c_offset", shim(rt_c_offset as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_c_load", shim(rt_c_load as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        ("rt_c_store", shim(rt_c_store as unsafe extern "C" fn(i64, i64, i64, i64) -> i64)),
        ("rt_c_volatile_load", shim(rt_c_volatile_load as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_c_volatile_store", shim(rt_c_volatile_store as unsafe extern "C-unwind" fn(i64, i64, i64) -> i64)),
        // A board's interrupt handlers (`typelisp_rt::interrupt`).
        ("rt_interrupt_register", shim(rt_interrupt_register as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_interrupt_enable", shim(rt_interrupt_enable as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_interrupt_disable", shim(rt_interrupt_disable as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_str_new", shim(rt_str_new as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_str_length", shim(rt_str_length as unsafe extern "C" fn(i64) -> i64)),
        ("rt_str_ref", shim(rt_str_ref as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_str_eq", shim(rt_str_eq as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_str_equalp", shim(rt_str_equalp as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_char_equalp", shim(rt_char_equalp as unsafe extern "C" fn(i64, i64) -> i64)),
        // The four `char` methods the island can now lower in place
        // (cl-parity-plan.md Phase 1's leftover from Phase 2), plus
        // `int->char` — the only one of the five that can fail, and so the
        // only one declared `extern "C-unwind"`.
        ("rt_char_upcase", shim(rt_char_upcase as unsafe extern "C" fn(i64) -> i64)),
        ("rt_char_downcase", shim(rt_char_downcase as unsafe extern "C" fn(i64) -> i64)),
        ("rt_char_alphap", shim(rt_char_alphap as unsafe extern "C" fn(i64) -> i64)),
        ("rt_char_digitp", shim(rt_char_digitp as unsafe extern "C" fn(i64) -> i64)),
        ("rt_int_to_char", shim(rt_int_to_char as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_str_lt", shim(rt_str_lt as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_str_append", shim(rt_str_append as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_str_substring", shim(rt_str_substring as unsafe extern "C-unwind" fn(i64, i64, i64) -> i64)),
        ("rt_str_upcase", shim(rt_str_upcase as unsafe extern "C" fn(i64) -> i64)),
        ("rt_int_fits_char", shim(rt_int_fits_char as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_fits_f32", shim(rt_f64_fits_f32 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_str_downcase", shim(rt_str_downcase as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_new", shim(rt_f64_new as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_value", shim(rt_f64_value as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f32_new", shim(rt_f32_new as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f32_value", shim(rt_f32_value as unsafe extern "C" fn(i64) -> i64)),
        ("rt_i8_box", shim(rt_i8_box as unsafe extern "C" fn(i64) -> i64)),
        ("rt_i16_box", shim(rt_i16_box as unsafe extern "C" fn(i64) -> i64)),
        ("rt_i32_box", shim(rt_i32_box as unsafe extern "C" fn(i64) -> i64)),
        ("rt_u8_box", shim(rt_u8_box as unsafe extern "C" fn(i64) -> i64)),
        ("rt_u16_box", shim(rt_u16_box as unsafe extern "C" fn(i64) -> i64)),
        ("rt_u32_box", shim(rt_u32_box as unsafe extern "C" fn(i64) -> i64)),
        ("rt_i8_unbox", shim(rt_i8_unbox as unsafe extern "C" fn(i64) -> i64)),
        ("rt_i16_unbox", shim(rt_i16_unbox as unsafe extern "C" fn(i64) -> i64)),
        ("rt_i32_unbox", shim(rt_i32_unbox as unsafe extern "C" fn(i64) -> i64)),
        ("rt_u8_unbox", shim(rt_u8_unbox as unsafe extern "C" fn(i64) -> i64)),
        ("rt_u16_unbox", shim(rt_u16_unbox as unsafe extern "C" fn(i64) -> i64)),
        ("rt_u32_unbox", shim(rt_u32_unbox as unsafe extern "C" fn(i64) -> i64)),
        ("rt_box_kind", shim(rt_box_kind as unsafe extern "C" fn(i64) -> i64)),
        ("rt_struct_new", shim(rt_struct_new as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_struct_field_get", shim(rt_struct_field_get as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_struct_field_set", shim(rt_struct_field_set as unsafe extern "C-unwind" fn(i64, i64, i64) -> i64)),
        ("rt_struct_field_count", shim(rt_struct_field_count as unsafe extern "C" fn(i64) -> i64)),
        ("rt_struct_push_field", shim(rt_struct_push_field as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_struct_pop_field", shim(rt_struct_pop_field as unsafe extern "C" fn(i64) -> i64)),
        ("rt_closure_new", shim(rt_closure_new as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_coroutine_closure_new", shim(rt_coroutine_closure_new as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_closure_fnptr", shim(rt_closure_fnptr as unsafe extern "C" fn(i64) -> i64)),
        ("rt_closure_env_len", shim(rt_closure_env_len as unsafe extern "C" fn(i64) -> i64)),
        ("rt_closure_env_get", shim(rt_closure_env_get as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_cell_new", shim(rt_cell_new as unsafe extern "C" fn(i64) -> i64)),
        ("rt_cell_get", shim(rt_cell_get as unsafe extern "C" fn(i64) -> i64)),
        ("rt_cell_set", shim(rt_cell_set as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_data_new", shim(rt_data_new as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_data_variant", shim(rt_data_variant as unsafe extern "C" fn(i64) -> i64)),
        ("rt_data_field", shim(rt_data_field as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_sexpr_instance_test", shim(rt_sexpr_instance_test as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        ("rt_hashtable_new", shim(rt_hashtable_new as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        ("rt_hashtable_count", shim(rt_hashtable_count as unsafe extern "C" fn(i64) -> i64)),
        ("rt_hashtable_clear", shim(rt_hashtable_clear as unsafe extern "C" fn(i64) -> i64)),
        ("rt_hashtable_keys", shim(rt_hashtable_keys as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_hashtable_values", shim(rt_hashtable_values as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_hashtable_entries", shim(rt_hashtable_entries as unsafe extern "C" fn(i64, i64) -> i64)),
        // The bucket family. `set`/`get`/`remove` used to be here as three
        // whole-lookup shims; they are prelude methods now, because hashing a
        // key and comparing two keys are the key type's own methods and a
        // shim cannot call them.
        ("rt_hashtable_bucket_count", shim(rt_hashtable_bucket_count as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_hashtable_bucket_key", shim(rt_hashtable_bucket_key as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        ("rt_hashtable_bucket_value", shim(rt_hashtable_bucket_value as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        ("rt_hashtable_bucket_put", shim(rt_hashtable_bucket_put as unsafe extern "C" fn(i64, i64, i64, i64, i64) -> i64)),
        ("rt_hashtable_bucket_delete", shim(rt_hashtable_bucket_delete as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        ("rt_global_new", shim(rt_global_new as unsafe extern "C" fn(i64) -> i64)),
        ("rt_global_get", shim(rt_global_get as unsafe extern "C" fn(i64) -> i64)),
        ("rt_global_set", shim(rt_global_set as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_bignum_new", shim(rt_bignum_new as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_ratio_from_bignums", shim(rt_ratio_from_bignums as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_integer_add", shim(rt_integer_add as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_integer_sub", shim(rt_integer_sub as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_integer_mul", shim(rt_integer_mul as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_integer_div", shim(rt_integer_div as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_rational_add", shim(rt_rational_add as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_rational_sub", shim(rt_rational_sub as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_rational_mul", shim(rt_rational_mul as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_rational_div", shim(rt_rational_div as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_rational_cmp", shim(rt_rational_cmp as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_rational_from_ratio", shim(rt_rational_from_ratio as unsafe extern "C" fn(i64) -> i64)),
        ("rt_rational_to_ratio", shim(rt_rational_to_ratio as unsafe extern "C" fn(i64) -> i64)),
        ("rt_rational_to_int", shim(rt_rational_to_int as unsafe extern "C" fn(i64) -> i64)),
        ("rt_rational_is_int", shim(rt_rational_is_int as unsafe extern "C" fn(i64) -> i64)),
        ("rt_rational_to_float", shim(rt_rational_to_float as unsafe extern "C" fn(i64) -> i64)),
        ("rt_rational_numerator", shim(rt_rational_numerator as unsafe extern "C" fn(i64) -> i64)),
        ("rt_rational_denominator", shim(rt_rational_denominator as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_mod", shim(rt_integer_mod as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_integer_cmp", shim(rt_integer_cmp as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_integer_logand", shim(rt_integer_logand as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_integer_logior", shim(rt_integer_logior as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_integer_logxor", shim(rt_integer_logxor as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_integer_logtest", shim(rt_integer_logtest as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_integer_ash", shim(rt_integer_ash as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_integer_logbitp", shim(rt_integer_logbitp as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_integer_lognot", shim(rt_integer_lognot as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_logcount", shim(rt_integer_logcount as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_integer_length", shim(rt_integer_integer_length as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_to_float", shim(rt_integer_to_float as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_to_ratio", shim(rt_integer_to_ratio as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_to_char", shim(rt_integer_to_char as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_integer_fits_char", shim(rt_integer_fits_char as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_to_i8", shim(rt_integer_to_i8 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_to_i16", shim(rt_integer_to_i16 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_to_i32", shim(rt_integer_to_i32 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_to_c_long", shim(rt_integer_to_c_long as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_to_u8", shim(rt_integer_to_u8 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_to_u16", shim(rt_integer_to_u16 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_to_u32", shim(rt_integer_to_u32 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_to_c_ulong", shim(rt_integer_to_c_ulong as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_fits_i8", shim(rt_integer_fits_i8 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_fits_i16", shim(rt_integer_fits_i16 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_fits_i32", shim(rt_integer_fits_i32 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_fits_u8", shim(rt_integer_fits_u8 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_fits_u16", shim(rt_integer_fits_u16 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_fits_u32", shim(rt_integer_fits_u32 as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_from_word", shim(rt_integer_from_word as unsafe extern "C" fn(i64) -> i64)),
        ("rt_integer_from_c_ulong", shim(rt_integer_from_c_ulong as unsafe extern "C" fn(i64) -> i64)),
        ("rt_int_not_fixnum", shim(rt_int_not_fixnum as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_random_state_next", shim(rt_random_state_next as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_random_state_copy", shim(rt_random_state_copy as unsafe extern "C" fn(i64) -> i64)),
        ("rt_make_random_state_fresh", shim(rt_make_random_state_fresh as unsafe extern "C" fn() -> i64)),
        ("rt_seed_random_state", shim(rt_seed_random_state as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_stdin", shim(rt_stream_stdin as unsafe extern "C" fn() -> i64)),
        ("rt_stream_stdout", shim(rt_stream_stdout as unsafe extern "C" fn() -> i64)),
        ("rt_stream_stderr", shim(rt_stream_stderr as unsafe extern "C" fn() -> i64)),
        ("rt_stream_string_input", shim(rt_stream_string_input as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_string_output", shim(rt_stream_string_output as unsafe extern "C" fn() -> i64)),
        ("rt_stream_close", shim(rt_stream_close as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_open_p", shim(rt_stream_open_p as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_describe", shim(rt_stream_describe as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_input_p", shim(rt_stream_input_p as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_output_p", shim(rt_stream_output_p as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_read_char", shim(rt_stream_read_char as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_read_byte", shim(rt_stream_read_byte as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_write_byte", shim(rt_stream_write_byte as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_stream_unread_char", shim(rt_stream_unread_char as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_suspend_sleep", shim(rt_suspend_sleep as unsafe extern "C" fn(i64) -> i64)),
        ("rt_suspend_wait", shim(rt_suspend_wait as unsafe extern "C" fn(i64) -> i64)),
        ("rt_suspend_yield", shim(rt_suspend_yield as unsafe extern "C" fn() -> i64)),
        ("rt_suspend_task", shim(rt_suspend_task as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_suspend_main", shim(rt_suspend_main as unsafe extern "C" fn() -> i64)),
        ("rt_thread_current_id", shim(rt_thread_current_id as unsafe extern "C" fn() -> i64)),
        ("rt_thread_available_parallelism", shim(rt_thread_available_parallelism as unsafe extern "C-unwind" fn() -> i64)),
        ("rt_suspend_chan_new", shim(rt_suspend_chan_new as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_suspend_chan_len", shim(rt_suspend_chan_len as unsafe extern "C" fn(i64) -> i64)),
        ("rt_suspend_chan_cap", shim(rt_suspend_chan_cap as unsafe extern "C" fn(i64) -> i64)),
        ("rt_suspend_chan_close", shim(rt_suspend_chan_close as unsafe extern "C" fn(i64) -> i64)),
        ("rt_suspend_chan_send", shim(rt_suspend_chan_send as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_suspend_chan_recv", shim(rt_suspend_chan_recv as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_suspend_chan_select", shim(rt_suspend_chan_select as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_loop_safepoint", shim(rt_loop_safepoint as unsafe extern "C" fn() -> i64)),
        ("rt_stream_listen", shim(rt_stream_listen as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_position", shim(rt_stream_position as unsafe extern "C" fn(i64) -> i64)),
        ("rt_set_macro_character", shim(rt_set_macro_character as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_get_macro_character", shim(rt_get_macro_character as unsafe extern "C" fn(i64) -> i64)),
        ("rt_set_dispatch_macro_character", shim(rt_set_dispatch_macro_character as unsafe extern "C" fn(i64, i64, i64) -> i64)),
        ("rt_get_dispatch_macro_character", shim(rt_get_dispatch_macro_character as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_stream_write_string", shim(rt_stream_write_string as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_stream_at_line_start", shim(rt_stream_at_line_start as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_finish_output", shim(rt_stream_finish_output as unsafe extern "C" fn(i64) -> i64)),
        ("rt_stream_take_output_string", shim(rt_stream_take_output_string as unsafe extern "C" fn(i64) -> i64)),
        ("rt_lisp_implementation_version", shim(rt_lisp_implementation_version as unsafe extern "C" fn() -> i64)),
        ("rt_machine_type", shim(rt_machine_type as unsafe extern "C" fn() -> i64)),
        ("rt_machine_version", shim(rt_machine_version as unsafe extern "C" fn() -> i64)),
        ("rt_timezone_offset_seconds", shim(rt_timezone_offset_seconds as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_timezone_daylight_p", shim(rt_timezone_daylight_p as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_software_type", shim(rt_software_type as unsafe extern "C" fn() -> i64)),
        ("rt_heap_info", shim(rt_heap_info as unsafe extern "C" fn() -> i64)),
        ("rt_int_to_ratio", shim(rt_int_to_ratio as unsafe extern "C" fn(i64) -> i64)),
        ("rt_float_to_int", shim(rt_float_to_int as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_float_to_ratio", shim(rt_float_to_ratio as unsafe extern "C" fn(i64) -> i64)),
        ("rt_ratio_add", shim(rt_ratio_add as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_ratio_sub", shim(rt_ratio_sub as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_ratio_mul", shim(rt_ratio_mul as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_ratio_div", shim(rt_ratio_div as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_ratio_cmp", shim(rt_ratio_cmp as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_ratio_to_int", shim(rt_ratio_to_int as unsafe extern "C" fn(i64) -> i64)),
        ("rt_ratio_to_float", shim(rt_ratio_to_float as unsafe extern "C" fn(i64) -> i64)),
        ("rt_ratio_numerator", shim(rt_ratio_numerator as unsafe extern "C" fn(i64) -> i64)),
        ("rt_ratio_denominator", shim(rt_ratio_denominator as unsafe extern "C" fn(i64) -> i64)),
        ("rt_intern_symbol", shim(rt_intern_symbol as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_intern", shim(rt_intern as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_keywordp", shim(rt_keywordp as unsafe extern "C" fn(i64) -> i64)),
        ("rt_wk_symbol", shim(rt_wk_symbol as unsafe extern "C" fn(i64) -> i64)),
        ("rt_intern_path", shim(rt_intern_path as unsafe extern "C" fn(*const i64, u32) -> i64)),
        ("rt_path_to_list", shim(rt_path_to_list as unsafe extern "C" fn(i64) -> i64)),
        ("rt_list_to_path", shim(rt_list_to_path as unsafe extern "C" fn(i64) -> i64)),
        ("rt_list_to_sexpr_vector", shim(rt_list_to_sexpr_vector as unsafe extern "C" fn(i64) -> i64)),
        ("rt_lists_to_sexpr_array", shim(rt_lists_to_sexpr_array as unsafe extern "C" fn(i64, i64) -> i64)),
        ("rt_list_to_sexpr_tuple", shim(rt_list_to_sexpr_tuple as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_vector_to_sexpr_tuple", shim(rt_vector_to_sexpr_tuple as unsafe extern "C-unwind" fn(i64) -> i64)),
        ("rt_sexpr_tuple_items", shim(rt_sexpr_tuple_items as unsafe extern "C" fn(i64) -> i64)),
        ("rt_int_div_fault", shim(rt_int_div_fault as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_int_mod_fault", shim(rt_int_mod_fault as unsafe extern "C-unwind" fn(i64, i64) -> i64)),
        ("rt_f32_tan", shim(rt_f32_tan as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f32_asin", shim(rt_f32_asin as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f32_acos", shim(rt_f32_acos as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f32_atan", shim(rt_f32_atan as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f32_sinh", shim(rt_f32_sinh as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f32_cosh", shim(rt_f32_cosh as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f32_tanh", shim(rt_f32_tanh as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f32_asinh", shim(rt_f32_asinh as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f32_acosh", shim(rt_f32_acosh as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f32_atanh", shim(rt_f32_atanh as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_tan", shim(rt_f64_tan as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_asin", shim(rt_f64_asin as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_acos", shim(rt_f64_acos as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_atan", shim(rt_f64_atan as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_sinh", shim(rt_f64_sinh as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_cosh", shim(rt_f64_cosh as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_tanh", shim(rt_f64_tanh as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_asinh", shim(rt_f64_asinh as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_acosh", shim(rt_f64_acosh as unsafe extern "C" fn(i64) -> i64)),
        ("rt_f64_atanh", shim(rt_f64_atanh as unsafe extern "C" fn(i64) -> i64)),
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

    /// Exercises the mutual-recursion branch of [`crate::compile::driver::compute_sccs`]/
    /// [`crate::compile::driver::compile_scc`] without going through surface syntax.
    /// `Checker::check_form_at` checks one top-level form at a time, in file
    /// order, so a pair of top-level `defun`s can call each other only when
    /// the later one is declared first with `defsignature`
    /// (`docs/ja/reference/syntax.md` §3.2) — which makes the test depend on
    /// the checker's forward-declaration rules. So this bypasses the checker
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
        // `(call NAME ())`: no arguments, hence no argument reprs.
        for (name, callee) in [("a", "b"), ("b", "a")] {
            let path = crate::check::core::definition_form(&Path::root(callee));
            heap.push_root(path);
            let call = core::tagged(&mut heap, typelisp_mem::wk::CALL, &[path, Value::Empty])
                .expect("building a 2-field node cannot exhaust a 1<<16 heap");
            // Registered `FnDef` bodies are reachable only through
            // `Interp::fns`, which the collector does not scan — the same
            // permanent root `Interp::exec` takes for a checked definition.
            heap.push_permanent_root(call);
            heap.pop_root();
            interp.root.borrow_mut().define_fn(
                &Path::root(name),
                Rc::new(FnDef { ffi: false,
                    target: crate::CompileTarget::Fn(Path::root(name)),
                    params: vec![],
                    body: vec![call],
                    rest: false,
                    lambda: None,
                    sig: Some((vec![], Repr::Narrow(typelisp_mem::fixed::FixedType::I32))),
                    compiled: RefCell::new(None),
                }),
            );
        }

        crate::compile::driver::compile_function(&interp, &mut heap, &crate::CompileTarget::Fn(Path::root("a")))
            .expect("mutual recursion across separate top-level functions should now compile");
        assert!(interp.root.borrow().get_fn(&Path::root("a")).is_some_and(|f| f.compiled.borrow().is_some()), "\"a\" should have ended up compiled");
        assert!(interp.root.borrow().get_fn(&Path::root("b")).is_some_and(|f| f.compiled.borrow().is_some()), "\"b\", pulled in transitively as part of the same SCC, should have ended up compiled too");
    }
}

#[cfg(test)]
mod native_method_list_tests {
    use super::native_lowered_primitive_methods;
    use typelisp_mem::wk;

    /// Every primitive receiver whose builtin methods either side lowers, with
    /// the island predicate that decides for it.
    const PRIMITIVES: &[(&str, &[u32])] = &[
        ("int", &[wk::I32, wk::I8, wk::I16, wk::U8, wk::U16, wk::U32, wk::C_LONG, wk::C_ULONG]),
        ("integer", &[wk::INT]),
        ("string", &[wk::STRING]),
        ("char", &[wk::CHAR]),
        ("float", &[wk::F64, wk::F32]),
        ("ratio", &[wk::RATIO]),
        ("rational", &[wk::REPR_RATIONAL]),
        ("bool", &[wk::BOOL]),
        ("symbol", &[wk::SYMBOL]),
        ("sexpr", &[wk::SEXPR]),
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
        let src: &str = &crate::compiler::SOURCE;
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
            for &ty in *type_locals {
                let type_local = typelisp_mem::symbols::well_known_symbol(ty).name();
                let mut rust: Vec<String> =
                    native_lowered_primitive_methods(ty).iter().map(|&m| typelisp_mem::symbols::well_known_symbol(m).name().to_string()).collect();
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
    /// the calling `defun` uncompilable. The syntax reference's §10 carried a table
    /// of exactly those, filled in by hand as they were discovered; this is
    /// the check that keeps it empty without anyone having to notice.
    ///
    /// No exceptions: a method no call can name is still reachable as a
    /// function value (`methodref`), which compiles to a call of the method.
    #[test]
    fn every_registered_builtin_method_on_a_native_receiver_lowers() {
        let builtins = typelisp_front::builtins::Builtins::new();
        let mut gaps: Vec<String> = Vec::new();
        for (_, type_locals) in PRIMITIVES {
            for &ty in *type_locals {
                let type_local = typelisp_mem::symbols::well_known_symbol(ty).name();
                let path = crate::Path::builtin(ty);
                assert!(builtins.type_def(&path).is_some(), "`{}` is not a built-in type", type_local);
                for (method, f) in builtins.methods(&path) {
                    if !f.sig.builtin {
                        continue;
                    }
                    if !native_lowered_primitive_methods(ty).contains(&method.well_known()) {
                        gaps.push(format!("{}::{}", type_local, method));
                    }
                }
            }
        }
        assert!(
            gaps.is_empty(),
            "these builtin methods are registered but have no compiled lowering, so a `defun` \
             calling one cannot be compiled: {:?}\n  Add each to `native_lowered_primitive_methods` \
             *and* the island's matching `*-native-method?`.",
            gaps
        );
    }
}


#[cfg(test)]
mod cons_making_shim_tests {
    use super::{rt_extern_functions, CONS_MAKING_SHIMS};

    /// A name here that no shim answers to would never match, and the check
    /// would let its operation through.
    #[test]
    fn every_never_raising_shim_is_a_shim() {
        let shims = rt_extern_functions();
        for name in super::NEVER_RAISING_SHIMS {
            assert!(shims.iter().any(|(n, _)| *n == name), "{} is not in rt_extern_functions", name);
        }
    }

    /// A shim in two runtime crates' tables would be refused for a target
    /// by the one it has not got though the other one gives it.
    #[test]
    fn no_shim_is_in_two_runtime_crates() {
        let mut seen = std::collections::HashSet::new();
        for name in rt_extern_functions().iter().map(|(name, _)| *name) {
            assert!(seen.insert(name), "{} is in rt_extern_functions twice", name);
        }
    }

    #[test]
    fn every_cons_making_shim_is_a_shim() {
        let shims = rt_extern_functions();
        for (name, _) in CONS_MAKING_SHIMS {
            assert!(shims.iter().any(|(n, _)| *n == name), "{} is not in rt_extern_functions", name);
        }
    }
}
