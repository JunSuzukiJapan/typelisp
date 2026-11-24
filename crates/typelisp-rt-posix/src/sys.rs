//! The system builtins only a POSIX system can answer: CPU time, the host's
//! name and OS release, a file's owner, and starting an editor.

use typelisp_mem::{spec, wk, Heap, StdModule, TypeId, TypeSpec, Value};
use typelisp_rt::result_value::{err_in, ok_in, option_in, TypeTable};
use typelisp_rt::stream_builtin::FILE_ERROR;
use typelisp_rt::sys_builtin::internal_time_value;
use typelisp_rt::{active_heap, decode, encode, fatal};

/// The runtime type each builtin here answers with
/// (`typelisp_rt::result_value`).
pub const RESULT_TYPES: TypeTable = &[
    (wk::F_MACHINE_INSTANCE, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
    (wk::F_SOFTWARE_VERSION, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
    (wk::F_ED_OPEN, TypeSpec::Named(StdModule::RESULT, "Result", &[spec::UNIT, FILE_ERROR])),
    (wk::F_FILE_OWNER_NAME, TypeSpec::Named(StdModule::RESULT, "Result", &[TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING]), FILE_ERROR])),
];

/// The type of the value inside the `Result`, for the builtins whose payload
/// is itself a box.
pub const INNER_TYPES: TypeTable = &[
    (wk::F_FILE_OWNER_NAME, TypeSpec::Named(StdModule::OPTION, "Option", &[spec::STRING])),
];

fn result_ok(heap: &mut Heap, name: u32, v: Value) -> Value {
    ok_in(heap, RESULT_TYPES, name, v)
}

fn result_err(heap: &mut Heap, name: u32, err_type_key: TypeId, msg: String) -> Value {
    err_in(heap, RESULT_TYPES, name, err_type_key, msg)
}

fn option_value(heap: &mut Heap, name: u32, v: Option<Value>) -> Value {
    option_in(heap, RESULT_TYPES, name, v)
}

/// `(file-owner-name path)`: CL's `file-author`. Two failures that CL keeps
/// apart stay apart: a missing file is an `Err`, an owner with no
/// password-database entry is `Ok(None)` — which is exactly what CL means by
/// "or nil if the author's name cannot be determined".
pub fn file_owner_name(heap: &mut Heap, path: &str) -> Value {
    match crate::os::file_owner(path) {
        Ok(who) => {
            let inner = who.map(|s| heap.alloc_string(s));
            let ov = option_in(heap, INNER_TYPES, wk::F_FILE_OWNER_NAME, inner);
            result_ok(heap, wk::F_FILE_OWNER_NAME, ov)
        }
        Err(e) => result_err(heap, wk::F_FILE_OWNER_NAME, TypeId::FILE_ERROR, format!("file-author: {}: {}", path, e)),
    }
}

/// `(get-internal-run-time)` (CLHS 25.1): CPU time this process has used,
/// user plus system, in the same microseconds `get-internal-real-time`
/// counts.
///
/// The pair is the point: real time says how long you waited, run time says
/// how long the CPU worked, and a program that is mostly blocked on I/O
/// shows a large difference. Substituting one for the other — which is what
/// this returned before there was a `getrusage` to call — is a lie precisely
/// when the number is interesting.
pub fn get_internal_run_time() -> i64 {
    match crate::os::run_time_micros() {
        Some(us) => us,
        // Unreachable: `getrusage(RUSAGE_SELF, &valid)` has no failure mode.
        // Reported rather than papered over, because a zero here would read
        // as "this program used no CPU".
        None => fatal("get-internal-run-time: getrusage(RUSAGE_SELF) failed"),
    }
}

/// `(machine-instance)` (CLHS 25.1): the host's name, or `None`.
pub fn machine_instance(heap: &mut Heap) -> Value {
    let v = crate::os::host_name().map(|s| heap.alloc_string(s));
    option_value(heap, wk::F_MACHINE_INSTANCE, v)
}

/// `(software-version)` (CLHS 25.1): the OS release string (`uname -r`,
/// e.g. `24.6.0`), or `None`. Its companion `software-type` is a
/// compile-time constant and needs no call.
pub fn software_version(heap: &mut Heap) -> Value {
    let v = crate::os::os_release().map(|s| heap.alloc_string(s));
    option_value(heap, wk::F_SOFTWARE_VERSION, v)
}

/// `(ed-open path line)`: run the user's editor on `path`, positioned at
/// `line` — the runtime half of the `ed` special form, which did the *name*
/// resolution at check time and reduced to this call.
///
/// `$VISUAL` first, then `$EDITOR`: that is the order every Unix tool uses,
/// and it is the one the two variables were invented to express (`VISUAL` for
/// a full-screen editor, `EDITOR` for whatever works on a teletype). Neither
/// set is an `Err`, not a guess at `vi` — inventing an editor is how `ed`
/// ends up launching something the user cannot exit.
///
/// The variable's value is split on whitespace so `EDITOR="code -w"` and
/// `EDITOR="emacsclient -nw"` work; the first word is the program and the
/// rest are leading arguments. This is the same treatment `git` gives it
/// short of running a shell, and running a shell would make the value a
/// command injection surface for no benefit.
///
/// `+N` is the line argument, understood by vi, emacs, nano, ed, joe and
/// `code --goto`'s fallback. It is omitted for line 0, which is what the
/// `ed`-with-a-path and `ed`-with-nothing forms produce.
///
/// Waits for the editor to exit (CL's `ed` returns when editing is done), and
/// reports a non-zero exit rather than swallowing it.
pub fn ed_open(heap: &mut Heap, path: &str, line: i64) -> Value {
    let err = |heap: &mut Heap, msg: String| result_err(heap, wk::F_ED_OPEN, TypeId::FILE_ERROR, msg);
    let spec = match std::env::var("VISUAL").ok().filter(|s| !s.trim().is_empty()) {
        Some(v) => v,
        None => match std::env::var("EDITOR").ok().filter(|s| !s.trim().is_empty()) {
            Some(v) => v,
            None => return err(heap, "ed: neither $VISUAL nor $EDITOR is set".to_string()),
        },
    };
    let mut words = spec.split_whitespace();
    let program = match words.next() {
        Some(p) => p,
        None => return err(heap, "ed: the editor variable is empty".to_string()),
    };
    let mut cmd = std::process::Command::new(program);
    cmd.args(words);
    if !path.is_empty() {
        if line > 0 {
            cmd.arg(format!("+{}", line));
        }
        cmd.arg(path);
    }
    // Native for as long as the editor runs.
    match heap.native(|| cmd.status()) {
        Ok(st) if st.success() => result_ok(heap, wk::F_ED_OPEN, Value::Empty),
        Ok(st) => err(heap, format!("ed: {} exited with {}", program, st)),
        Err(e) => err(heap, format!("ed: cannot run {}: {}", program, e)),
    }
}

/// `(get-internal-run-time)` for compiled code — a tagged `internal-time`
/// struct, like [`rt_get_internal_real_time`].
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_get_internal_run_time() -> i64 {
    let v = internal_time_value(active_heap(), get_internal_run_time());
    encode(v)
}

// ---- The compiled-code edge -------------------------------------------

/// `(machine-instance)` for compiled code.
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_machine_instance() -> i64 {
    encode(machine_instance(active_heap()))
}

/// `(software-version)` for compiled code.
///
/// # Safety
///
/// A `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_software_version() -> i64 {
    encode(software_version(active_heap()))
}

/// `(ed-open path line)` for compiled code: `path` is a tagged `string`,
/// `line` a raw `i32` line number.
///
/// # Safety
///
/// The words must be valid, the first
/// encoding a `Value::Str`; a `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_ed_open(path: i64, line: i64) -> i64 {
    let path = match decode(path) {
        Value::Str(id) => active_heap().string(id).to_string(),
        other => fatal(&format!("ed-open: argument is not a string, got {:?}", other)),
    };
    let line = line;
    encode(ed_open(active_heap(), &path, line))
}

/// `(file-owner-name path)` for compiled code: `path` is a tagged
/// `string`.
///
/// # Safety
///
/// The word must encode a
/// `Value::Str`; a `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_file_owner_name(path: i64) -> i64 {
    let path = match decode(path) {
        Value::Str(id) => active_heap().string(id).to_string(),
        other => fatal(&format!("file-owner-name: argument is not a string, got {:?}", other)),
    };
    encode(file_owner_name(active_heap(), &path))
}
