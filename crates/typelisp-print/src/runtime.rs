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
    pub enum_variant_name: fn(&str, usize) -> Option<String>,
    /// `(heap, value, escape) -> rendering`. See [`PrintEnv::print_object`].
    pub print_object: fn(&mut Heap, Value, bool) -> Result<Option<String>, String>,
    /// The three layout control variables (`*print-pretty*`,
    /// `*print-right-margin*`, `*print-miser-width*`), read fresh per
    /// printing operation — typelisp has no dynamic binding, so they are
    /// ordinary globals a program may have `setf`'d since the last one.
    pub opts: fn(&Heap) -> Opts,
    /// The three "what to print" control variables (`*print-circle*`,
    /// `*print-level*`, `*print-length*`), read the same way.
    pub print_vars: fn(&Heap) -> PrintVars,
}

/// The hooks a program with no environment installed gets: every enum prints
/// `<unknown-variant>`, no type has a `print-object` method, and both sets of
/// control variables are at their CL initial values.
///
/// Reaching these is a registration bug, not something a program can provoke
/// — but they are silent-and-plausible rather than a crash, because the one
/// place it can legitimately happen is a unit test printing a value without
/// standing a program up.
pub const BARE_HOOKS: PrintHooks = PrintHooks {
    enum_variant_name: |_, _| None,
    print_object: |_, _, _| Ok(None),
    opts: |_| Opts::default(),
    print_vars: |_| PrintVars::default(),
};

thread_local! {
    static HOOKS: RefCell<PrintHooks> = const { RefCell::new(BARE_HOOKS) };
    static SESSION: RefCell<Option<PrettySession>> = const { RefCell::new(None) };
}

/// Installs this thread's [`PrintHooks`] (or, with `None`, restores
/// [`BARE_HOOKS`]).
pub fn set_print_hooks(hooks: Option<PrintHooks>) {
    HOOKS.with(|cell| *cell.borrow_mut() = hooks.unwrap_or(BARE_HOOKS));
}

fn hooks() -> PrintHooks {
    HOOKS.with(|cell| *cell.borrow())
}

/// The [`PrintEnv`] every printing operation here runs under: the installed
/// hooks, behind the trait [`typelisp_print`] asks its questions through.
struct RtPrintEnv {
    hooks: PrintHooks,
}

impl PrintEnv for RtPrintEnv {
    fn enum_variant_name(&self, type_key: &str, variant: usize) -> Option<String> {
        (self.hooks.enum_variant_name)(type_key, variant)
    }

    fn print_object(&self, heap: &mut Heap, v: Value, escape: bool) -> Result<Option<String>, String> {
        (self.hooks.print_object)(heap, v, escape)
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
    lists: Vec<std::rc::Rc<BoxId>>,
    opts: Opts,
}

/// Whether a logical block is open on this thread. Inside one the "stream"
/// *is* a pretty stream, so everything printed into it pretty-prints
/// regardless of `*print-pretty*` — the same thing CL's stream-type dispatch
/// achieves. The interpreter's own `pretty_opts` consults this.
pub fn session_open() -> bool {
    SESSION.with(|cell| cell.borrow().is_some())
}

/// The three layout control variables, as of right now.
pub fn current_opts(heap: &Heap) -> Opts {
    (hooks().opts)(heap)
}

/// The three "what to print" control variables, as of right now.
pub fn current_print_vars(heap: &Heap) -> PrintVars {
    (hooks().print_vars)(heap)
}

/// Builds `control`'s output without committing it — the shared half of
/// `format`/`print`/`println`.
///
/// Kept separate from [`emit`] because a buffer that still carries
/// pretty-printer ops must be merged into an open session *un*-laid-out:
/// laying it out early would freeze line breaks chosen against the wrong
/// starting column and without the enclosing block's indentation.
pub fn build_format(heap: &mut Heap, control: &str, args: Value) -> Result<Out, String> {
    let env = RtPrintEnv { hooks: hooks() };
    let opts = current_opts(heap);
    let ctx = RenderCtx { env: &env, print_vars: current_print_vars(heap) };
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
        // CL's `pprint-tabular` defaults its column width to 16; the checker
        // passes 0 when the caller omitted it.
        "pprint-tabular" => Style::Tabular(if colinc <= 0 { 16 } else { colinc }),
        _ => Style::Default,
    };
    let env = RtPrintEnv { hooks: hooks() };
    let ctx = RenderCtx { env: &env, print_vars: current_print_vars(heap) };
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
pub fn emit(heap: &Heap, mut out: Out, newline: bool, opts: &Opts) -> Result<(), String> {
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
        Some(out) => write_stdout(&format::finish(out, opts)),
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
    let opts = Opts { pretty: true, ..opts.clone() };
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
pub fn block_end() -> Result<(), String> {
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
        Some(s) => write_stdout(&format::finish(s.out, &s.opts)),
        None => Ok(()),
    }
}

/// Closes and writes out a session left open by a non-local exit. A no-op in
/// the normal case, where the matching block end already flushed it.
pub fn flush() -> Result<(), String> {
    let Some(s) = SESSION.with(|cell| cell.borrow_mut().take()) else {
        return Ok(());
    };
    let opts = Opts { pretty: true, ..s.opts };
    // `layout` closes whatever blocks are still open, emitting their suffixes,
    // so the partial output is still well-formed.
    write_stdout(&format::finish(s.out, &opts))
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
fn innermost_list() -> Option<std::rc::Rc<BoxId>> {
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
fn write_stdout(text: &str) -> Result<(), String> {
    let mut out = std::io::stdout();
    write!(out, "{}", text)
        .and_then(|()| out.flush())
        .map_err(|e| format!("print: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hook path — what a shim will use — renders through the same
    /// [`typelisp_print`] the interpreter does, and answers the two program
    /// questions from whatever environment is installed.
    #[test]
    fn installed_hooks_supply_enum_names_and_print_object() {
        let mut heap = Heap::with_capacity(1 << 12);

        set_print_hooks(Some(PrintHooks {
            enum_variant_name: |key, variant| match (key, variant) {
                ("option", 0) => Some("some".to_string()),
                _ => None,
            },
            print_object: |_, _, _| Ok(None),
            ..BARE_HOOKS
        }));

        let boxed = heap.alloc_enum("option".to_string(), 0, vec![Value::Int(7)]);
        let args = heap.cons(boxed, Value::Empty).expect("heap has room");
        let out = build_format(&mut heap, "~a", args).expect("format should succeed");
        assert_eq!(format::finish(out, &Opts::default()), "(some 7)");

        set_print_hooks(None);
    }

    /// With no environment installed the renderer still runs — it just cannot
    /// name the variant. Loud rather than silent, on purpose: a
    /// `<unknown-variant>` in real output means a registration bug.
    #[test]
    fn bare_hooks_render_an_unnamed_variant() {
        let mut heap = Heap::with_capacity(1 << 12);
        set_print_hooks(None);

        let boxed = heap.alloc_enum("option".to_string(), 0, vec![Value::Int(7)]);
        let args = heap.cons(boxed, Value::Empty).expect("heap has room");
        let out = build_format(&mut heap, "~a", args).expect("format should succeed");
        assert_eq!(format::finish(out, &Opts::default()), "(<unknown-variant> 7)");
    }

    /// A `print-object` hook wins over the built-in representation, and sees
    /// the `escape` flag `~s` sets and `~a` clears.
    #[test]
    fn a_print_object_hook_replaces_the_builtin_rendering() {
        let mut heap = Heap::with_capacity(1 << 12);

        set_print_hooks(Some(PrintHooks {
            print_object: |_, _, escape| Ok(Some(if escape { "#<S>" } else { "#<A>" }.to_string())),
            ..BARE_HOOKS
        }));

        let boxed = heap.alloc_struct("pt".to_string(), vec![Value::Int(1)]);
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
        emit(&heap, out, false, &Opts::default()).expect("emit into a session");
        assert!(session_open());
        block_end().expect("close");
        assert!(!session_open());
    }
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

fn arg_str<'a>(heap: &'a Heap, args: &[Value], i: usize, who: &str) -> Result<&'a str, PrintError> {
    match args.get(i) {
        Some(Value::Str(id)) => Ok(heap.string(*id)),
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
    Some(print_builtin_inner(heap, name, args)?)
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
            let opts = printing_opts(heap);
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
            let opts = printing_opts(heap);
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
            let opts = printing_opts(heap);
            emit(heap, out, false, &opts).map_err(PrintError::Raise)?;
            Ok(Value::Empty)
        })(),
        "pprint-block-start-rt" => (|| {
            let obj = *args.first().ok_or_else(|| PrintError::Shape("pprint-logical-block: expected 4 arguments".into()))?;
            let prefix = arg_str(heap, args, 1, "pprint-logical-block")?.to_string();
            let per_line = arg_bool(args, 2, "pprint-logical-block")?;
            let suffix = arg_str(heap, args, 3, "pprint-logical-block")?.to_string();
            let opts = printing_opts(heap);
            block_start(heap, obj, &prefix, per_line, &suffix, &opts);
            Ok(Value::Empty)
        })(),
        "pprint-block-end-rt" => block_end().map(|()| Value::Empty).map_err(PrintError::Raise),
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
            record_op(Op::Tab { kind, colnum, colinc });
            Ok(Value::Empty)
        })(),
        "pprint-pop" => Ok(pop(heap)),
        "pprint-list-exhausted" => Ok(Value::Bool(list_exhausted(heap))),
        _ => return None,
    };
    Some(r)
}

/// The layout options one printing operation runs under: the control globals,
/// with `pretty` forced on inside an open logical block — there the "stream"
/// *is* a pretty stream, which is what CL's stream-type dispatch achieves.
fn printing_opts(heap: &Heap) -> Opts {
    let mut opts = current_opts(heap);
    opts.pretty |= session_open();
    opts
}
