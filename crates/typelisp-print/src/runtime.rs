//! The printer's runtime edge: the pretty-printing *session*, and the
//! program facts [`typelisp_print`] needs but cannot read off the heap.
//!
//! The renderer proper ([`crate::format`]/[`crate::pprint`]) is pure — hand it a value, a control string and a
//! [`PrintEnv`] and it produces text. Two things are not pure and live here
//! instead:
//!
//! - **the open logical block.** CL's pretty printer works against a *pretty
//!   stream*; typelisp has no first-class streams, so `pprint-logical-block`
//!   opens an implicit session that every `print`/`format`/`pprint` inside it
//!   appends to instead of writing through. That session is process state, and
//!   it has to be *one* piece of state: a compiled `println` nested inside an
//!   interpreted `pprint-logical-block` must land in the same buffer.
//! - **[`PrintHooks`].** Enum variant names and `print-object` methods are
//!   properties of a program. The interpreter answers both directly; an
//!   AOT-linked executable has no interpreter and answers from tables its
//!   startup code registers.
//!
//! Thread-local for the same reason `typelisp_rt::set_active_heap` is: one
//! program per thread, many per process under `cargo test`.

use std::cell::RefCell;
use std::io::Write;

use typelisp_mem::{BoxId, Heap, Value};
use crate::format::{self, PrintVars, RenderCtx};
use crate::pprint::{self, IndentKind, NewlineKind, Op, Opts, Out, Style, TabKind};
use crate::PrintEnv;

/// What the environment around the printer must be able to answer.
///
/// Plain Rust `fn` pointers rather than the `extern "C-unwind"` shape
/// `typelisp_rt::ApplyInterpretedFn` uses: these are only ever called Rust-side
/// (from this module, on behalf of a shim), never emitted as a call by the
/// compiler, so there is no ABI to pin down and `String`/`Result` can cross
/// directly.
#[derive(Clone, Copy)]
pub struct PrintHooks {
    /// `(type_key, variant) -> name`. See [`PrintEnv::enum_variant_name`].
    pub enum_variant_name: fn(&str, usize) -> Result<String, String>,
    /// `(type_key, variant, index) -> is a niched Option`. See
    /// [`PrintEnv::field_is_niched_option`].
    pub field_is_niched_option: fn(&str, Option<usize>, usize) -> Result<bool, String>,
    /// `(type_key, index) -> field name`. See [`PrintEnv::field_name`].
    pub field_name: fn(&str, usize) -> Result<Option<String>, String>,
    /// `(heap, value, escape) -> rendering`. See [`PrintEnv::print_object`].
    pub print_object: fn(&mut Heap, Value, bool) -> Result<Option<String>, String>,
    /// `(heap, name, value, colon, at) -> rendering`, for `~/name/`. See
    /// [`PrintEnv::format_call`].
    pub format_call: fn(&mut Heap, &str, Value, bool, bool) -> Result<String, String>,
    /// The layout control variables ([`read_opts`]), read fresh per printing
    /// operation — they are ordinary globals a program may have `setf`'d
    /// since the last one. An error is a value the printer cannot use.
    pub opts: fn(&Heap) -> Result<Opts, String>,
    /// The "what to print" control variables ([`read_print_vars`]), read the
    /// same way.
    pub print_vars: fn(&Heap) -> Result<PrintVars, String>,
}

/// The hooks a program with no environment installed gets: every question
/// about the program — its types, its methods, its control variables — is
/// an error, because there is no program to answer it.
///
/// Reaching these is a registration bug, not something a program can provoke.
/// A unit test that prints without standing a program up installs its own
/// hooks for what it needs.
pub const BARE_HOOKS: PrintHooks = PrintHooks {
    enum_variant_name: |key, _| Err(format!("printing a `{}` needs a program that defines it, and none is installed", key)),
    field_is_niched_option: |key, _, _| Err(format!("printing a `{}` needs a program that defines it, and none is installed", key)),
    field_name: |key, _| Err(format!("printing a `{}` needs a program that defines it, and none is installed", key)),
    print_object: |_, _, _| Err(NO_PROGRAM_FOR_METHODS.to_string()),
    format_call: |_, name, _, _, _| Err(format!("format: ~/{}/ needs a program to look the method up in", name)),
    opts: |_| Err(NO_PROGRAM.to_string()),
    print_vars: |_| Err(NO_PROGRAM.to_string()),
};

const NO_PROGRAM: &str = "printing needs a program to read the printer control variables from, and none is installed";
const NO_PROGRAM_FOR_METHODS: &str = "printing needs a program to look up `print-object` methods in, and none is installed";

/// Every printer control variable the renderer reads, as the prelude names
/// them: what [`read_print_vars`] and [`read_opts`] ask for between them, and
/// what an AOT executable's startup registers (`typelisp_print::aot`).
pub const PRINTER_GLOBALS: [&str; 11] = [
    "*print-circle*",
    "*print-level*",
    "*print-length*",
    "*print-base*",
    "*print-radix*",
    "*print-case*",
    "*print-readably*",
    "*print-pretty*",
    "*print-right-margin*",
    "*print-miser-width*",
    "*print-lines*",
];

/// Reads one printer control variable: `global` answers its current value, or
/// why it has none. The checker fixed each one's type at its `defvar`, so a
/// value of any other shape is a broken invariant, reported as such.
fn read_bool(global: &dyn Fn(&str) -> Result<Value, String>, name: &str) -> Result<bool, String> {
    match global(name)? {
        Value::Bool(b) => Ok(b),
        other => Err(format!("internal error: {} holds {:?}, not a bool", name, other)),
    }
}

/// An `int` control variable: a fixnum, or a bignum no printer setting can
/// use.
fn read_int(heap: &Heap, global: &dyn Fn(&str) -> Result<Value, String>, name: &str) -> Result<i64, String> {
    match global(name)? {
        Value::Int(n) => Ok(n),
        Value::Boxed(id) if heap.is_bignum(id) => Err(format!("{} is {}, which is out of range", name, heap.bignum_value(id))),
        other => Err(format!("internal error: {} holds {:?}, not an int", name, other)),
    }
}

/// A limit: CL spells "no limit" `nil`, which this language does not have, so
/// 0 stands in for it. A negative limit means nothing and is an error.
fn read_limit(heap: &Heap, global: &dyn Fn(&str) -> Result<Value, String>, name: &str) -> Result<Option<usize>, String> {
    match read_int(heap, global, name)? {
        0 => Ok(None),
        n if n > 0 => Ok(Some(n as usize)),
        n => Err(format!("{} must be 0 (no limit) or positive, got {}", name, n)),
    }
}

/// The "what to print" control variables, through `global`.
pub fn read_print_vars(heap: &Heap, global: &dyn Fn(&str) -> Result<Value, String>) -> Result<PrintVars, String> {
    let base = match read_int(heap, global, "*print-base*")? {
        b if (2..=36).contains(&b) => b as u32,
        b => return Err(format!("*print-base* must be between 2 and 36, got {}", b)),
    };
    // A keyword is an ordinary symbol whose interned name keeps the leading
    // colon, not a type of its own — so the checker lets any symbol through
    // and the three names CL defines are checked here.
    let case = match global("*print-case*")? {
        Value::Symbol(id) => match heap.symbol_name(id) {
            ":upcase" => format::PrintCase::Upcase,
            ":downcase" => format::PrintCase::Downcase,
            ":capitalize" => format::PrintCase::Capitalize,
            other => return Err(format!("*print-case* must be :upcase, :downcase or :capitalize, got {}", other)),
        },
        other => return Err(format!("internal error: *print-case* holds {:?}, not a symbol", other)),
    };
    Ok(PrintVars {
        circle: read_bool(global, "*print-circle*")?,
        level: read_limit(heap, global, "*print-level*")?,
        length: read_limit(heap, global, "*print-length*")?,
        base,
        radix: read_bool(global, "*print-radix*")?,
        case,
        readably: read_bool(global, "*print-readably*")?,
    })
}

/// The layout control variables, through `global`. Inside a
/// `pprint-logical-block` the "stream" *is* a pretty stream, so everything
/// printed into it pretty-prints regardless of `*print-pretty*` — the same
/// thing CL's stream-type dispatch achieves.
pub fn read_opts(heap: &Heap, global: &dyn Fn(&str) -> Result<Value, String>) -> Result<Opts, String> {
    let pretty = read_bool(global, "*print-pretty*")?;
    // A margin of 0: nothing is ever too wide, so nothing breaks.
    let margin: usize = read_limit(heap, global, "*print-right-margin*")?.unwrap_or_default();
    Ok(Opts {
        pretty: pretty || session_open(),
        margin,
        miser: read_limit(heap, global, "*print-miser-width*")?,
        lines: read_limit(heap, global, "*print-lines*")?,
    })
}

thread_local! {
    static HOOKS: RefCell<PrintHooks> = const { RefCell::new(BARE_HOOKS) };
    static SESSION: RefCell<Option<PrettySession>> = const { RefCell::new(None) };
}

/// Installs this thread's [`PrintHooks`] (or, with `None`, restores
/// [`BARE_HOOKS`]).
pub fn set_print_hooks(hooks: Option<PrintHooks>) {
    HOOKS.with(|cell| *cell.borrow_mut() = hooks.unwrap_or(BARE_HOOKS));
}

/// This thread's [`PrintHooks`] — for a thread that starts another one to
/// run on the same program (a scheduler worker) and hands it the same
/// hooks with [`set_print_hooks`].
pub fn print_hooks() -> PrintHooks {
    HOOKS.with(|cell| *cell.borrow())
}

/// The [`PrintEnv`] every printing operation here runs under: the installed
/// hooks, behind the trait [`typelisp_print`] asks its questions through.
struct RtPrintEnv {
    hooks: PrintHooks,
}

impl PrintEnv for RtPrintEnv {
    fn enum_variant_name(&self, type_key: &str, variant: usize) -> Result<String, String> {
        (self.hooks.enum_variant_name)(type_key, variant)
    }

    fn field_is_niched_option(&self, type_key: &str, variant: Option<usize>, index: usize) -> Result<bool, String> {
        (self.hooks.field_is_niched_option)(type_key, variant, index)
    }

    fn field_name(&self, type_key: &str, index: usize) -> Result<Option<String>, String> {
        (self.hooks.field_name)(type_key, index)
    }

    fn print_object(&self, heap: &mut Heap, v: Value, escape: bool) -> Result<Option<String>, String> {
        (self.hooks.print_object)(heap, v, escape)
    }

    fn format_call(&self, heap: &mut Heap, name: &str, v: Value, colon: bool, at: bool) -> Result<String, String> {
        (self.hooks.format_call)(heap, name, v, colon, at)
    }
}

/// An in-progress pretty-printing session: what CL would call "the output
/// stream is a pretty stream right now".
///
/// `lists` is the stack of open blocks' `pprint-pop` cursors, one heap cell
/// per block holding the untraversed tail. Its depth is also how the outermost
/// block is recognised — closing it is what lays the buffer out and writes it.
struct PrettySession {
    out: Out,
    lists: Vec<std::sync::Arc<BoxId>>,
    opts: Opts,
}

/// Whether a logical block is open on this thread. Inside one the "stream"
/// *is* a pretty stream, so everything printed into it pretty-prints
/// regardless of `*print-pretty*` — the same thing CL's stream-type dispatch
/// achieves. [`read_opts`] consults this.
pub fn session_open() -> bool {
    SESSION.with(|cell| cell.borrow().is_some())
}

/// The layout control variables, as of right now.
pub fn current_opts(heap: &Heap) -> Result<Opts, String> {
    (print_hooks().opts)(heap)
}

/// The "what to print" control variables, as of right now.
pub fn current_print_vars(heap: &Heap) -> Result<PrintVars, String> {
    (print_hooks().print_vars)(heap)
}

/// Builds `control`'s output without committing it — the shared half of
/// `format`/`print`/`println`.
///
/// Kept separate from [`emit`] because a buffer that still carries
/// pretty-printer ops must be merged into an open session *un*-laid-out:
/// laying it out early would freeze line breaks chosen against the wrong
/// starting column and without the enclosing block's indentation.
pub fn build_format(heap: &mut Heap, control: &str, args: Value) -> Result<Out, String> {
    let env = RtPrintEnv { hooks: print_hooks() };
    let opts = current_opts(heap)?;
    let ctx = RenderCtx { env: &env, print_vars: current_print_vars(heap)? };
    format::build(heap, ctx, control, args, &opts)
}

/// Renders one value with a ready-made layout — the `pprint`/`pprint-fill`/
/// `pprint-linear`/`pprint-tabular` family.
///
/// Unconditionally pretty: `render` always records the layout ops and
/// [`emit`]'s layout pass runs whenever any op is present. `*print-pretty*`
/// gates `~a`/`~s`/`~w`, not these.
pub fn build_pprint(heap: &mut Heap, which: &str, value: Value, colinc: i64) -> Result<Out, String> {
    let style = match which {
        "pprint-fill" => Style::Fill,
        "pprint-linear" => Style::Linear,
        // The checker passes CL's default of 16 when the caller omitted it.
        "pprint-tabular" if colinc < 0 => {
            return Err(format!("pprint-tabular: the column width must be non-negative, got {}", colinc))
        }
        "pprint-tabular" => Style::Tabular(colinc),
        _ => Style::Default,
    };
    let env = RtPrintEnv { hooks: print_hooks() };
    let ctx = RenderCtx { env: &env, print_vars: current_print_vars(heap)? };
    let mut out = Out::new();
    if which == "pprint" {
        // CLHS: `pprint` outputs a newline *before* the object.
        out.push('\n');
    }
    pprint::render(heap, ctx, value, true, style, &mut out)?;
    Ok(out)
}

/// Commits printed output: into the open session if there is one, otherwise
/// laid out and written straight to stdout.
///
/// `opts` is the caller's own snapshot of the layout control variables: the
/// interpreter reads them from its globals, a shim from [`PrintHooks`]. They
/// are only consulted when this is the *outermost* write (no session open) —
/// inside a session the buffer is merged un-laid-out and the block's own
/// snapshot decides.
pub fn emit(heap: &mut Heap, mut out: Out, newline: bool, opts: &Opts) -> Result<(), String> {
    if newline {
        out.push('\n');
    }
    // `current_opts` reads the heap, so it must not run while the session
    // borrow is held.
    let buffered = SESSION.with(|cell| match cell.borrow_mut().as_mut() {
        Some(s) => {
            s.out.append(out);
            None
        }
        None => Some(out),
    });
    let _ = heap;
    match buffered {
        Some(out) => write_stdout(heap, &format::finish(out, opts)),
        None => Ok(()),
    }
}

/// Opens a logical block, starting a session if this is the outermost one.
/// `obj` is the list `pprint-pop` walks (`()` for a block that iterates
/// nothing).
///
/// `opts` is the caller's snapshot of the layout control variables, taken
/// once here so a `setf` of `*print-right-margin*` inside the block cannot
/// change the margin halfway through laying one document out. `pretty` is
/// forced on: an explicit `pprint-logical-block` is a request to
/// pretty-print, whatever `*print-pretty*` says.
pub fn block_start(heap: &mut Heap, obj: Value, prefix: &str, per_line: bool, suffix: &str, opts: &Opts) {
    let opts = Opts { pretty: true, ..*opts };
    let cell = heap.alloc_cell(obj);
    SESSION.with(|s| {
        let mut session = s.borrow_mut();
        let session = session.get_or_insert_with(|| PrettySession { out: Out::new(), lists: Vec::new(), opts });
        session.out.op(Op::BlockStart {
            prefix: prefix.to_string(),
            per_line,
            suffix: suffix.to_string(),
        });
        session.lists.push(cell.clone());
    });
}

/// Closes a logical block; closing the outermost one lays the whole session
/// out and writes it to stdout.
pub fn block_end(heap: &mut Heap) -> Result<(), String> {
    let finished = SESSION.with(|cell| {
        let mut session = cell.borrow_mut();
        let s = session.as_mut()?;
        s.out.op(Op::BlockEnd);
        s.lists.pop();
        if s.lists.is_empty() {
            session.take()
        } else {
            None
        }
    });
    match finished {
        // `s.opts` was snapshotted with `pretty` forced on at block start: an
        // explicit `pprint-logical-block` is a request to pretty-print,
        // exactly as `pprint` is.
        Some(s) => write_stdout(heap, &format::finish(s.out, &s.opts)),
        None => Ok(()),
    }
}

/// Closes and writes out a session left open by a non-local exit. A no-op in
/// the normal case, where the matching block end already flushed it.
pub fn flush(heap: &mut Heap) -> Result<(), String> {
    let Some(s) = SESSION.with(|cell| cell.borrow_mut().take()) else {
        return Ok(());
    };
    let opts = Opts { pretty: true, ..s.opts };
    // `layout` closes whatever blocks are still open, emitting their suffixes,
    // so the partial output is still well-formed.
    write_stdout(heap, &format::finish(s.out, &opts))
}

/// Records a pretty-printer op on the open session. A no-op with no session
/// open, matching CL, where `pprint-newline` and friends do nothing unless
/// the stream really is a pretty stream.
pub fn record_op(op: Op) {
    SESSION.with(|cell| {
        if let Some(s) = cell.borrow_mut().as_mut() {
            s.out.op(op);
        }
    });
}

/// The innermost open block's `pprint-pop` cursor cell, if any.
fn innermost_list() -> Option<std::sync::Arc<BoxId>> {
    SESSION.with(|cell| cell.borrow().as_ref().and_then(|s| s.lists.last().cloned()))
}

/// `pprint-pop`: the next element of the innermost open block's list, or `()`
/// when it is exhausted. Advances the stored tail in place.
pub fn pop(heap: &mut Heap) -> Value {
    let Some(cell) = innermost_list() else { return Value::Empty };
    let rest = heap.cell_get(*cell);
    let Ok(head) = heap.car(rest) else { return Value::Empty };
    let tail = heap.cdr(rest).unwrap_or(Value::Empty);
    heap.cell_set(*cell, tail);
    head
}

/// `pprint-list-exhausted`: whether the innermost open block's list has
/// nothing left (also true when there is no open block at all).
pub fn list_exhausted(heap: &Heap) -> bool {
    match innermost_list() {
        Some(cell) => !matches!(heap.cell_get(*cell), Value::Cons(_)),
        None => true,
    }
}

/// Writes `text` to stdout and flushes immediately — a script's stdout isn't
/// a terminal when piped, so it isn't line-buffered there, and a prompt
/// printed without a newline must still be visible before the process blocks
/// reading stdin.
///
/// Native (`docs/dev/os-threads-design.md` §4): the write can wait on a full
/// pipe, and on stdout's lock while another thread waits on one.
fn write_stdout(heap: &mut Heap, text: &str) -> Result<(), String> {
    // One of the three doors a session's output leaves by; see
    // `typelisp_abi::dribble`'s module docs for the other two.
    typelisp_abi::dribble::note(text);
    heap.native(|| {
        let mut out = std::io::stdout();
        write!(out, "{}", text).and_then(|()| out.flush())
    })
    .map_err(|e| format!("print: {}", e))
}

// ---- the printing builtins as *values* ---------------------------------
//
// One implementation, called from both sides of the compile boundary — the
// same arrangement `typelisp_rt::stream_builtin` has, and for the same
// reason. These ten builtins are not arithmetic: each has a wrapper shape, a
// keyword vocabulary and an error message text, and a program that opens a
// `pprint-logical-block` in interpreted code and `println`s inside it from
// compiled code must see one behaviour. A second implementation on the other
// side of the boundary would be ten chances to disagree.
//
// The *representations* are deliberately not here: an `i64` and a `bool`
// arrive as bare machine words compiled-side and as `Value`s interpreted, and
// that conversion belongs to the calling convention, not to the operation.
// [`crate::shim`] does it at its own edge; the interpreter does it at its.

/// How a printing builtin fails.
///
/// The distinction is the one `typelisp_rt::fatal` and `typelisp_rt::raise`
/// draw, decided here because only this layer knows which is which.
pub enum PrintError {
    /// A failure the *language* defines — an unknown `~` directive, a
    /// keyword argument that isn't one of the four this operator takes, a
    /// `print-object` method that panicked. Catchable: `EvalError::Panic`
    /// interpreted, an unwind compiled.
    Raise(String),
    /// An argument that isn't the shape its signature promises. Unreachable
    /// through the checker; a runtime-corruption abort, not a program error.
    Shape(String),
}

fn shape<T>(who: &str, i: usize, v: &Value) -> Result<T, PrintError> {
    Err(PrintError::Shape(format!("{}: argument {} has the wrong representation, got {:?}", who, i, v)))
}

/// Owned rather than `&'a str`: `Heap::string` hands back a read-locked
/// guard now (`docs/dev/os-threads-design.md` §3), whose lifetime is its
/// own, not `heap`'s — every caller here already cloned the result
/// immediately with `.to_string()` anyway.
fn arg_str(heap: &Heap, args: &[Value], i: usize, who: &str) -> Result<String, PrintError> {
    match args.get(i) {
        Some(Value::Str(id)) => Ok(heap.string(*id).to_string()),
        Some(v) => shape(who, i, v),
        None => Err(PrintError::Shape(format!("{}: expected at least {} arguments", who, i + 1))),
    }
}

fn arg_bool(args: &[Value], i: usize, who: &str) -> Result<bool, PrintError> {
    match args.get(i) {
        Some(Value::Bool(b)) => Ok(*b),
        Some(v) => shape(who, i, v),
        None => Err(PrintError::Shape(format!("{}: expected at least {} arguments", who, i + 1))),
    }
}

fn arg_int(args: &[Value], i: usize, who: &str) -> Result<i64, PrintError> {
    match args.get(i) {
        Some(Value::Int(n)) => Ok(*n),
        Some(v) => shape(who, i, v),
        None => Err(PrintError::Shape(format!("{}: expected at least {} arguments", who, i + 1))),
    }
}

/// A `pprint-*` operator's keyword argument (`:linear`, `:block`, …).
/// Keywords are ordinary interned symbols whose name keeps the leading colon,
/// so this is just "the symbol's name".
fn arg_keyword<'a>(heap: &'a Heap, args: &[Value], i: usize, who: &str) -> Result<&'a str, PrintError> {
    match args.get(i) {
        Some(Value::Symbol(id)) => Ok(heap.symbol_name(*id)),
        Some(v) => shape(who, i, v),
        None => Err(PrintError::Shape(format!("{}: expected at least {} arguments", who, i + 1))),
    }
}

/// Runs one printing builtin, or returns `None` when `name` is not one.
///
/// `name` is the *lowered* spelling the checker emits (`format-rt`,
/// `print-rt`, …), not the surface syntax: `format`/`print`/`println`/
/// `pprint` and `pprint-logical-block` are special forms, and what survives
/// checking is a call to one of the names below.
pub fn print_builtin(heap: &mut Heap, name: &str, args: &[Value]) -> Option<Result<Value, PrintError>> {
    print_builtin_inner(heap, name, args)
}

fn print_builtin_inner(heap: &mut Heap, name: &str, args: &[Value]) -> Option<Result<Value, PrintError>> {
    let r = match name {
        // `format`'s `bool` destination: builds the text, and additionally
        // writes it when the destination is `true` (CL's `t`). The string it
        // returns is always the laid-out text; the *write* goes through
        // `emit`, which merges into an open logical block rather than jumping
        // the queue past that block's buffered output.
        "format-rt" => (|| {
            let dest = arg_bool(args, 0, "format")?;
            let control = arg_str(heap, args, 1, "format")?.to_string();
            let list = *args.get(2).ok_or_else(|| PrintError::Shape("format: expected 3 arguments".into()))?;
            let out = build_format(heap, &control, list).map_err(PrintError::Raise)?;
            let opts = current_opts(heap).map_err(PrintError::Raise)?;
            let text = format::finish(out.clone(), &opts);
            if dest {
                emit(heap, out, false, &opts).map_err(PrintError::Raise)?;
            }
            Ok(heap.alloc_string(text))
        })(),
        "print-rt" | "println-rt" => (|| {
            let control = arg_str(heap, args, 0, name)?.to_string();
            let list = *args.get(1).ok_or_else(|| PrintError::Shape(format!("{}: expected 2 arguments", name)))?;
            let out = build_format(heap, &control, list).map_err(PrintError::Raise)?;
            let opts = current_opts(heap).map_err(PrintError::Raise)?;
            emit(heap, out, name == "println-rt", &opts).map_err(PrintError::Raise)?;
            Ok(Value::Empty)
        })(),
        // `pprint`/`pprint-fill`/`pprint-linear`/`pprint-tabular`, which pass
        // their own name so one builtin serves all four.
        "pprint-rt" => (|| {
            let which = arg_str(heap, args, 0, "pprint")?.to_string();
            let value = *args.get(1).ok_or_else(|| PrintError::Shape("pprint: expected 3 arguments".into()))?;
            let colinc = arg_int(args, 2, "pprint")?;
            let out = build_pprint(heap, &which, value, colinc).map_err(PrintError::Raise)?;
            let opts = current_opts(heap).map_err(PrintError::Raise)?;
            emit(heap, out, false, &opts).map_err(PrintError::Raise)?;
            Ok(Value::Empty)
        })(),
        "pprint-block-start-rt" => (|| {
            let obj = *args.first().ok_or_else(|| PrintError::Shape("pprint-logical-block: expected 4 arguments".into()))?;
            let prefix = arg_str(heap, args, 1, "pprint-logical-block")?.to_string();
            let per_line = arg_bool(args, 2, "pprint-logical-block")?;
            let suffix = arg_str(heap, args, 3, "pprint-logical-block")?.to_string();
            let opts = current_opts(heap).map_err(PrintError::Raise)?;
            block_start(heap, obj, &prefix, per_line, &suffix, &opts);
            Ok(Value::Empty)
        })(),
        "pprint-block-end-rt" => block_end(heap).map(|()| Value::Empty).map_err(PrintError::Raise),
        "pprint-newline" => (|| {
            let kind = match arg_keyword(heap, args, 0, "pprint-newline")? {
                ":linear" => NewlineKind::Linear,
                ":fill" => NewlineKind::Fill,
                ":miser" => NewlineKind::Miser,
                ":mandatory" => NewlineKind::Mandatory,
                other => {
                    return Err(PrintError::Raise(format!(
                        "pprint-newline: expected :linear, :fill, :miser or :mandatory, got {}",
                        other
                    )))
                }
            };
            record_op(Op::Newline(kind));
            Ok(Value::Empty)
        })(),
        "pprint-indent" => (|| {
            let kind = match arg_keyword(heap, args, 0, "pprint-indent")? {
                ":block" => IndentKind::Block,
                ":current" => IndentKind::Current,
                other => return Err(PrintError::Raise(format!("pprint-indent: expected :block or :current, got {}", other))),
            };
            let n = arg_int(args, 1, "pprint-indent")?;
            record_op(Op::Indent(kind, n));
            Ok(Value::Empty)
        })(),
        "pprint-tab" => (|| {
            let kind = match arg_keyword(heap, args, 0, "pprint-tab")? {
                ":line" => TabKind::Line,
                ":section" => TabKind::Section,
                ":line-relative" => TabKind::LineRelative,
                ":section-relative" => TabKind::SectionRelative,
                other => {
                    return Err(PrintError::Raise(format!(
                        "pprint-tab: expected :line, :section, :line-relative or :section-relative, got {}",
                        other
                    )))
                }
            };
            let colnum = arg_int(args, 1, "pprint-tab")?;
            let colinc = arg_int(args, 2, "pprint-tab")?;
            if colnum < 0 || colinc < 0 {
                return Err(PrintError::Raise(format!(
                    "pprint-tab: colnum and colinc must be non-negative, got {} and {}",
                    colnum, colinc
                )));
            }
            record_op(Op::Tab { kind, colnum, colinc });
            Ok(Value::Empty)
        })(),
        "pprint-pop" => Ok(pop(heap)),
        "pprint-list-exhausted" => Ok(Value::Bool(list_exhausted(heap))),
        _ => return None,
    };
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`BARE_HOOKS`] with the control variables at CL's initial values and
    /// no `print-object` methods — what these tests print under, having no
    /// program to hold either.
    const TEST_HOOKS: PrintHooks = PrintHooks {
        print_object: |_, _, _| Ok(None),
        opts: |_| Ok(Opts::default()),
        print_vars: |_| Ok(PrintVars::default()),
        ..BARE_HOOKS
    };

    /// With no program installed there are no control variables, and printing
    /// says so rather than guessing their values.
    #[test]
    fn bare_hooks_refuse_to_print() {
        let mut heap = Heap::with_capacity(1 << 12);
        set_print_hooks(None);

        let args = heap.cons(Value::Int(7), Value::Empty).expect("heap has room");
        let err = build_format(&mut heap, "~a", args).expect_err("no program, no printer variables");
        assert!(err.contains("printer control variables"), "{}", err);
    }

    /// The hook path — what a shim will use — renders through the same
    /// [`typelisp_print`] the interpreter does, and answers the program's
    /// questions from whatever environment is installed.
    #[test]
    fn installed_hooks_supply_enum_names_and_print_object() {
        let mut heap = Heap::with_capacity(1 << 12);

        set_print_hooks(Some(PrintHooks {
            enum_variant_name: |key, variant| match (key, variant) {
                ("option", 0) => Ok("some".to_string()),
                _ => Err(format!("no variant {} of {}", variant, key)),
            },
            field_is_niched_option: |_, _, _| Ok(false),
            print_object: |_, _, _| Ok(None),
            ..TEST_HOOKS
        }));

        let boxed = heap.alloc_enum(typelisp_mem::TypeKeyId::OPTION, 0, vec![Value::Int(7)]);
        let args = heap.cons(boxed, Value::Empty).expect("heap has room");
        let out = build_format(&mut heap, "~a", args).expect("format should succeed");
        assert_eq!(format::finish(out, &Opts::default()), "(some 7)");

        set_print_hooks(None);
    }

    /// An enum whose type the environment has never heard of is a printing
    /// error, not a guessed rendering: every enum a value can be of is
    /// registered, so reaching this means the registration is broken.
    #[test]
    fn an_unregistered_enum_is_a_printing_error() {
        let mut heap = Heap::with_capacity(1 << 12);
        set_print_hooks(Some(TEST_HOOKS));

        let boxed = heap.alloc_enum(typelisp_mem::TypeKeyId::OPTION, 0, vec![Value::Int(7)]);
        let args = heap.cons(boxed, Value::Empty).expect("heap has room");
        let err = build_format(&mut heap, "~a", args).expect_err("an unregistered enum");
        assert!(err.contains("option"), "{}", err);

        set_print_hooks(None);
    }

    /// A `print-object` hook wins over the built-in representation, and sees
    /// the `escape` flag `~s` sets and `~a` clears.
    #[test]
    fn a_print_object_hook_replaces_the_builtin_rendering() {
        let mut heap = Heap::with_capacity(1 << 12);

        set_print_hooks(Some(PrintHooks {
            print_object: |_, _, escape| Ok(Some(if escape { "#<S>" } else { "#<A>" }.to_string())),
            ..TEST_HOOKS
        }));

        let pt = heap.intern_type_key("pt");
        let boxed = heap.alloc_struct(pt, vec![Value::Int(1)]);
        // `(v v)` — the two arguments `~a ~s` consumes.
        let tail = heap.cons(boxed, Value::Empty).expect("heap has room");
        let args = heap.cons(boxed, tail).expect("heap has room");
        let out = build_format(&mut heap, "~a ~s", args).expect("format should succeed");
        assert_eq!(format::finish(out, &Opts::default()), "#<A> #<S>");

        set_print_hooks(None);
    }

    /// `pprint`'s leading newline (CLHS) and the session's merge-then-flush
    /// behaviour: nothing reaches stdout until the outermost block closes.
    #[test]
    fn a_logical_block_buffers_until_its_outermost_close() {
        let mut heap = Heap::with_capacity(1 << 12);
        set_print_hooks(None);

        assert!(!session_open());
        block_start(&mut heap, Value::Empty, "(", false, ")", &Opts::default());
        assert!(session_open());
        // Emitting into an open session buffers rather than writing.
        let mut out = Out::new();
        out.push('x');
        emit(&mut heap, out, false, &Opts::default()).expect("emit into a session");
        assert!(session_open());
        block_end(&mut heap).expect("close");
        assert!(!session_open());
    }
}
