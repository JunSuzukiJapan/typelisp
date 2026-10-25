//! A Common Lisp-style reader that produces `Sexpr` values ([`Value`]).
//!
//! Follows the CLHS reader algorithm (2.1–2.4): skip whitespace/comments,
//! dispatch on macro characters, otherwise accumulate a token and interpret it
//! as a number or a (case-insensitive, interned) symbol.
//!
//! Cons cells, symbols and strings are allocated in a [`Heap`]. Because
//! allocation may trigger a GC mid-read, every partially-built value is kept on
//! the heap's root stack until it is linked into its parent, so the collector
//! never reclaims a structure that is still being read.
//!
//! Supported v1 syntax: integers (decimal or a CL radix macro `#b`/`#o`/
//! `#x`/`#NNr`, signed; past `i32`'s range they read as `bignum`s), ratios (`1/3`, normalized like CL — `4/2`
//! reads as the integer `2`), floats, booleans
//! `true`/`false`, strings with escapes, characters `#\a` / `#\Space`, symbols
//! (operators, `::` paths, `Vec<T>`-style tokens), lists, dotted pairs `(a . b)`,
//! quote `'`, quasiquote `` ` ``, unquote `,`, unquote-splicing `,@`, and
//! comments (`;` line, `#| ... |#` nested block).

use std::collections::HashSet;
use std::sync::Arc;

use num_bigint::BigInt;
use num_rational::BigRational;

use typelisp_mem::{spec, symbols, wk, Error, Heap, Loc, NsId, StdModule, SymRef, TypeId, TypeSpec, Value};

/// The feature set consulted by `#+`/`#-` reader conditionals (CLHS 24.1.2 —
/// the closest an s-expression reader has to a preprocessor). A feature is
/// a keyword (`:darwin`, `:unix`, `:my-flag`, ...), held as the keyword
/// symbol itself, so membership is the identity of the keyword read.
///
/// Unlike CL's `*features*` — an ordinary mutable special variable a program
/// can `push`/`pushnew` into mid-file, so a later top-level form sees an
/// earlier one's change — typelisp reads every top-level form of a file
/// before checking or evaluating *any* of them (see `Reader::read_all_in` and
/// callers such as `project::Loader`), so there is no live global left for
/// `#+`/`#-` to consult while reading is still in progress. The set is
/// therefore fixed per `Reader` instance: seeded from the host platform by
/// [`Features::host`] and extendable with caller-supplied names via
/// [`Reader::with_features`] (the `typl` CLI's `--feature` flag).
#[derive(Clone, Debug, Default)]
pub struct Features(HashSet<SymRef>);

impl Features {
    /// The default set: `:typelisp`, the host architecture, and the host OS
    /// — following the CL convention of keyword-named implementation/
    /// platform features (e.g. SBCL's `:unix` / `:darwin` / `:x86-64`).
    pub fn host() -> Features {
        let mut set = HashSet::new();
        let mut add = |name: &str| set.insert(symbols::intern_keyword(name));
        add("typelisp");
        add(std::env::consts::ARCH);
        match std::env::consts::OS {
            "macos" => {
                add("darwin");
                add("macos");
            }
            other => {
                add(other);
            }
        }
        if cfg!(unix) {
            add("unix");
        }
        if cfg!(windows) {
            add("windows");
        }
        Features(set)
    }

    /// This feature set with the keywords `extra` names also present —
    /// callers may pass either `"my-feature"` or `":my-feature"` (the
    /// `--feature` flag's text, where the colon is optional spelling).
    pub fn with(mut self, extra: impl IntoIterator<Item = String>) -> Features {
        for name in extra {
            self.0.insert(symbols::intern_keyword(name.strip_prefix(':').unwrap_or(&name)));
        }
        self
    }

    fn has(&self, feature: SymRef) -> bool {
        self.0.contains(&feature)
    }
}

/// What the reader needs from the evaluator to run code **while reading**.
///
/// The reader is otherwise a pure function of its text, and was entirely so
/// until this: `#.` evaluates the datum after it and reads the value in its
/// place. Somebody has to run that datum, and running anything means the
/// checker and the interpreter — neither of which this crate can see (they
/// depend on it, not the other way round). So the evaluator implements this
/// and the driver hands it in, the same shape `MacroExpander` has on the
/// checker's side.
///
/// It is threaded per call rather than held in the [`Reader`]: a driver's
/// loop holds `&mut Interp` for its own work between reads, and a borrow
/// living as long as the reader would collide with it.
pub trait ReadEval {
    /// Check and evaluate `form`, returning what it produced as a datum.
    ///
    /// The error is a message, not an [`Error`]: what comes back is a *type*
    /// error or an evaluation error, neither of which is a reader error, and
    /// the reader adds its own position to whatever it is told.
    fn read_eval(&self, heap: &mut Heap, form: Value) -> Result<Value, String>;

    /// Call the reader macro `f` on the character `ch` that triggered it and
    /// the unread text after it, answering the datum it produced and **how
    /// many characters of `rest` it consumed**.
    ///
    /// The text is passed rather than the cursor because a reader macro is an
    /// ordinary typelisp function taking a stream, and building one is the
    /// evaluator's side of the fence — this crate knows nothing of streams.
    /// The implementor makes a stream over `rest`, calls `f`, and reads the
    /// position back off it.
    ///
    /// The cost is a copy of the remaining text per call. Worth naming: a
    /// file that is mostly reader macros copies its own tail once per macro.
    /// The alternative is exposing the reader's cursor as a stream backend,
    /// which is a lifetime problem this does not have.
    fn call_reader_macro(
        &self,
        heap: &mut Heap,
        f: Value,
        ch: char,
        rest: &str,
    ) -> Result<(Value, usize), String>;

    /// Bring in what a `(use PATH)` written in the module `ns` names, now:
    /// load the file behind it if one is and nothing has loaded it yet, and
    /// register what it brings in in the session's table of names
    /// (`Heap::names`) — so the forms read after it, the rest of a
    /// `(module ...)` body included, can read a path through it. `path` is
    /// the `use`'s path as read, the symbols of its segments.
    ///
    /// Asked only for a `use` read as a top-level form, and only when the
    /// path does not already lead to a module or type the session has.
    fn bring_in(&self, heap: &mut Heap, ns: NsId, path: &[SymRef]) -> Result<(), String>;
}

/// Everything a read carries besides the cursor: which `#+` features are on,
/// which module bare symbols intern into, and how (if at all) to run code at
/// read time.
///
/// One struct rather than three parameters because the list grows: `#.` added
/// the third, and a readtable would be the fourth.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    features: &'a Features,
    ns: NsId,
    eval: Option<&'a dyn ReadEval>,
    /// How a `::` path read right here is read — see [`PathMode`].
    paths: PathMode,
    /// Whether what is read here is a form a program runs at its top level:
    /// a top-level form, or a form of a `(module ...)` body. Only there is
    /// `(defstruct point ...)` a definition and `(use m)` an import; the same
    /// list inside a quotation is data.
    toplevel: bool,
}

/// How a `::` path is read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PathMode {
    /// A reference: stepped through the module tree and the type tables
    /// (`SessionNames::read_path`), or `(%value-member p x)` after a value.
    Resolve,
    /// The `PATH` of `(module PATH` / `(in-module PATH`, which names the
    /// module it opens: each segment is a child module of the one before,
    /// made if new.
    Declare,
    /// The path a `use` names, which may be a module not loaded yet: each
    /// segment is read in the current module, and the `use` resolves it.
    Raw,
}

pub struct Reader {
    features: Features,
}

impl Default for Reader {
    fn default() -> Reader {
        Reader::new()
    }
}

impl Reader {
    /// A reader whose `#+`/`#-` conditionals see only the host platform's
    /// default features (see [`Features::host`]). Use [`Reader::with_features`]
    /// to also recognize caller-supplied feature names.
    pub fn new() -> Reader {
        Reader { features: Features::host() }
    }

    /// A reader whose `#+`/`#-` conditionals also recognize `extra` feature
    /// names, in addition to the host defaults — e.g. the `typl` CLI's
    /// `--feature` flag.
    pub fn with_features(extra: impl IntoIterator<Item = String>) -> Reader {
        Reader { features: Features::host().with(extra) }
    }

    /// Read a single datum from `src`. Trailing input is ignored.
    ///
    /// Uses the placeholder file name `<input>` in any error's location; call
    /// [`Reader::read_in`] to supply the real source file name.
    pub fn read(&self, heap: &mut Heap, src: &str) -> Result<Value, Error> {
        self.read_in(heap, "<input>", src)
    }

    /// Like [`Reader::read`], but `file` names the source (used in the location
    /// prefix of any error message).
    pub fn read_in(&self, heap: &mut Heap, file: &str, src: &str) -> Result<Value, Error> {
        self.read_in_with(heap, file, src, NsId::ROOT, None)
    }

    /// [`Reader::read_in`] with an evaluator, so `#.` and macro characters
    /// work. The `read` *builtin* takes this path — CL's `read` consults the
    /// readtable, and a program that installed a macro character means it to
    /// apply to its own reads too. Names are read in `ns`: the builtin passes
    /// the session's current module, CL's `*package*`.
    pub fn read_in_with(
        &self,
        heap: &mut Heap,
        file: &str,
        src: &str,
        ns: NsId,
        eval: Option<&dyn ReadEval>,
    ) -> Result<Value, Error> {
        let mut cur = Cursor::new(file, src);
        read_datum(&mut cur, heap, Ctx { features: &self.features, ns, eval, paths: PathMode::Resolve, toplevel: false }).map_err(|e| e.at(cur.loc()))
    }

    /// Read one datum starting at character index `start`, and say where
    /// reading stopped — CL's `read-from-string`, whose second return value
    /// this is. The index counts *characters*, as CL's does.
    ///
    /// `preserve_whitespace` is CL's distinction between `read-from-string`
    /// and `read-from-string-preserving-whitespace`: the datum itself always
    /// ends where it ends, but `read` consumes the one whitespace character
    /// that terminated it and the preserving form does not. It matters to a
    /// caller that reads again from the returned index and wants to know
    /// whether a space is still there.
    ///
    /// `start` past the end of `src` reads nothing and reports the error the
    /// reader reports for empty input; a `start` inside a datum reads
    /// whatever begins there, which is the caller's business, not this
    /// function's.
    pub fn read_from(
        &self,
        heap: &mut Heap,
        src: &str,
        start: usize,
        preserve_whitespace: bool,
    ) -> Result<(Value, usize), Error> {
        self.read_from_with(heap, src, start, preserve_whitespace, NsId::ROOT, None)
    }

    /// [`Reader::read_from`] with an evaluator — see [`Reader::read_in_with`].
    pub fn read_from_with(
        &self,
        heap: &mut Heap,
        src: &str,
        start: usize,
        preserve_whitespace: bool,
        ns: NsId,
        eval: Option<&dyn ReadEval>,
    ) -> Result<(Value, usize), Error> {
        let mut cur = Cursor::new("<input>", src);
        cur.seek(start);
        let v = read_datum(&mut cur, heap, Ctx { features: &self.features, ns, eval, paths: PathMode::Resolve, toplevel: false }).map_err(|e| e.at(cur.loc()))?;
        if !preserve_whitespace {
            if let Some(c) = cur.peek() {
                if c.is_whitespace() {
                    cur.next();
                }
            }
        }
        Ok((v, cur.pos()))
    }

    /// Read every top-level datum from `src`.
    ///
    /// Each returned value is registered as a GC root (so later reads cannot
    /// collect earlier results); the caller pops them when done.
    ///
    /// Uses the placeholder file name `<input>` in any error's location; call
    /// [`Reader::read_all_in`] to supply the real source file name.
    pub fn read_all(&self, heap: &mut Heap, src: &str) -> Result<Vec<Value>, Error> {
        self.read_all_in(heap, "<input>", src)
    }

    /// Like [`Reader::read_all`], but `file` names the source (used in the
    /// location prefix of any error message).
    pub fn read_all_in(&self, heap: &mut Heap, file: &str, src: &str) -> Result<Vec<Value>, Error> {
        Ok(self.read_all_in_spanned(heap, file, src)?.into_iter().map(|(v, _)| v).collect())
    }

    /// Like [`Reader::read_all_in`], but each top-level datum comes with the
    /// source span it was read from. This is the only way a *bare atom* at
    /// top level (e.g. a lone `42`) gets a location: an atom is an immediate or
    /// interned value with no per-occurrence identity, so there is no cell of
    /// its own to record one in — the span must travel alongside the value to
    /// whoever checks it (`Checker::check_form_at`'s `loc_hint`).
    ///
    /// There used to be a `keep_locs` twin of this (and of
    /// [`Reader::read_all_in`]) that skipped wiping the heap's location tables
    /// first, because a dependency read mid-load would otherwise erase the
    /// locations of the outer file's already-read forms. Locations live in the
    /// cells now, so a read cannot disturb another read's forms and there is
    /// nothing to wipe or to opt out of.
    pub fn read_all_in_spanned(&self, heap: &mut Heap, file: &str, src: &str) -> Result<Vec<(Value, Loc)>, Error> {
        self.read_all_in_spanned_within(heap, file, src, NsId::ROOT)
    }

    /// [`Reader::read_all_in_spanned`] reading `src` as the body of module
    /// `ns` — every bare symbol in it is interned there rather than at the
    /// root, which is what makes `m::foo` and `n::foo` different symbols.
    ///
    /// A file *is* a module (see `project::module_segs_for`), so this is the
    /// entry point for loading one; an explicit `(module ...)` inside nests
    /// further, handled while reading. Callers with nothing to say — the REPL,
    /// the prelude, the island — read at the root.
    pub fn read_all_in_spanned_within(
        &self,
        heap: &mut Heap,
        file: &str,
        src: &str,
        ns: NsId,
    ) -> Result<Vec<(Value, Loc)>, Error> {
        let mut forms = self.forms_within(file, src, ns);
        let mut out = Vec::new();
        while let Some(pair) = forms.next_form(heap)? {
            out.push(pair);
        }
        Ok(out)
    }

    /// The same top-level data [`Reader::read_all_in_spanned_within`] returns,
    /// but handed over **one at a time**, so a caller can check and evaluate
    /// form *n* before form *n+1* is read.
    ///
    /// That order is the whole point. Reading a file to the end first makes
    /// the source text a fixed thing decided before any of it runs, which is
    /// why `#+`/`#-` can only see a feature set fixed in advance and why
    /// there is nowhere to put a reader macro: the form that would install
    /// one has not run when the text it is meant to affect is read. Every
    /// driver that loads source goes through here for that reason — see
    /// `prelude::load_interpreted_with`, `project::Loader::load_source_inner`
    /// and `typl`'s own REPL batch.
    ///
    /// `read_all_*` remain, built on this: a caller that only wants the data
    /// (a test, a `read-from-string`, a generator hashing its input) should
    /// keep asking for all of it.
    ///
    /// The returned [`Forms`] borrows the *reader*, not `src` — the cursor
    /// takes its own copy of the characters — so a driver is free to edit or
    /// drop the source string it was given (`typl`'s REPL rewrites its
    /// pending buffer from [`Forms::pos`] while the `Forms` is still alive).
    pub fn forms_within<'a>(&'a self, file: &str, src: &str, ns: NsId) -> Forms<'a> {
        Forms { cur: Cursor::new(file, src), features: &self.features, ns }
    }

    /// [`Reader::forms_within`] at the root namespace — the REPL, the prelude
    /// and the island, which have no module of their own to intern into.
    pub fn forms_in<'a>(&'a self, file: &str, src: &str) -> Forms<'a> {
        self.forms_within(file, src, NsId::ROOT)
    }

    /// [`Reader::forms_in`] with the placeholder file name `<input>`.
    pub fn forms<'a>(&'a self, src: &str) -> Forms<'a> {
        self.forms_in("<input>", src)
    }
}

/// One source text being read a form at a time — [`Reader::forms_within`].
///
/// Holds the read position (and, through the [`Reader`] it borrows, the
/// feature set the `#+`/`#-` conditionals consult), so the text between two
/// calls is text that has not been looked at yet.
pub struct Forms<'a> {
    cur: Cursor,
    features: &'a Features,
    ns: NsId,
}

impl Forms<'_> {
    /// Read the forms after this point in module `ns` — for a driver whose
    /// program moved into another module (`(in-module ...)`) between forms.
    pub fn set_module(&mut self, ns: NsId) {
        self.ns = ns;
    }

    /// The next top-level datum and its span, or `Ok(None)` at end of input.
    ///
    /// Each datum is registered as a GC root before it is returned, exactly
    /// as [`Reader::read_all_in_spanned_within`] does — the caller pops them,
    /// and must not pop one while a later form is still to be read (the root
    /// stack is LIFO).
    pub fn next_form(&mut self, heap: &mut Heap) -> Result<Option<(Value, Loc)>, Error> {
        self.next_form_with(heap, None)
    }

    /// [`Self::next_form`] with an evaluator on hand, so a `#.` in the text
    /// can be run as it is read.
    ///
    /// The hook is passed per call rather than kept in the `Forms`: a driver
    /// holds `&mut Interp` for its own work between reads, and a borrow that
    /// lived as long as the `Forms` would collide with it.
    pub fn next_form_with(
        &mut self,
        heap: &mut Heap,
        eval: Option<&dyn ReadEval>,
    ) -> Result<Option<(Value, Loc)>, Error> {
        let ctx = Ctx { features: self.features, ns: self.ns, eval, paths: PathMode::Resolve, toplevel: true };
        // Reading a text in a module makes it one of this session's modules.
        heap.names().declare_module(self.ns);
        if let Err(e) = skip_ws_comments(&mut self.cur, heap, ctx) {
            return Err(e.at(self.cur.loc()));
        }
        if self.cur.at_end() {
            return Ok(None);
        }
        match read_datum_spanned(&mut self.cur, heap, ctx) {
            Ok((v, loc)) => {
                heap.push_root(v);
                Ok(Some((v, loc)))
            }
            Err(e) => Err(e.at(self.cur.loc())),
        }
    }

    /// How far into the source the reader has got, as a **character** index
    /// (the unit [`Reader::read_from`] uses too).
    ///
    /// Read one form at a time and the text splits in two at this point: what
    /// has been read, and what has not. An interactive driver needs the
    /// split — `typl`'s REPL runs each form as it is read, so when the last
    /// one turns out to be incomplete it has to keep *only* the unread tail
    /// and wait for the rest of it, or the forms it already ran would run
    /// again on the next line.
    ///
    /// A datum ends where it ends: the whitespace that separated it from the
    /// next one has not been consumed and is part of the tail. (Only
    /// [`Reader::read_from`] eats that one character, because CL's
    /// `read-from-string` does.)
    pub fn pos(&self) -> usize {
        self.cur.pos()
    }
}

// ----------------------------------------------------------------------
// Cursor
// ----------------------------------------------------------------------

struct Cursor {
    chars: Vec<char>,
    pos: usize,
    /// 1-based line and column of the character at `pos` (the next one to be
    /// consumed), tracked incrementally in [`Cursor::next`]. Used to build a
    /// [`Loc`] for any read error via [`Cursor::loc`].
    file: Arc<str>,
    line: u32,
    col: u32,
}

impl Cursor {
    fn new(file: &str, s: &str) -> Cursor {
        Cursor { chars: s.chars().collect(), pos: 0, file: Arc::from(file), line: 1, col: 1 }
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }
    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if let Some(ch) = c {
            self.pos += 1;
            // Advance line/column: a consumed newline moves to column 1 of the
            // next line; any other character advances the column by one.
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        c
    }
    /// Jump to character index `n`, keeping `line`/`col` truthful by
    /// counting the characters skipped over — an error reported after a
    /// `seek` must still name the right place in the source.
    fn seek(&mut self, n: usize) {
        let n = n.min(self.chars.len());
        while self.pos < n {
            self.next();
        }
    }
    fn pos(&self) -> usize {
        self.pos
    }
    /// The text from here to the end, as a fresh `String` — what a reader
    /// macro is handed to read from.
    fn rest(&self) -> String {
        self.chars[self.pos..].iter().collect()
    }
    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }
    /// The current source position (where the next character would be read),
    /// which is where a read error is reported.
    fn loc(&self) -> Loc {
        Loc::new(Arc::clone(&self.file), self.line, self.col)
    }
}

// ----------------------------------------------------------------------
// Character classes
// ----------------------------------------------------------------------

fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{0C}')
}

/// Characters that terminate a token / separate data.
///
/// `,` is deliberately *not* here even though it introduces `unquote`:
/// `read_datum`'s dispatch on the *first* character of a fresh datum already
/// recognizes a datum-initial `,` as unquote regardless of whether it's a
/// delimiter, and inside a generic type's `<..>` the type-argument reader
/// (`read_type_application`) separates arguments at it itself.
///
/// `]` and `}` end a token so that `#[test]` and `#{a b}` close where they
/// look like they close. `[` and `{` do not: on their own they are left to
/// the programmer's readtable, as CL leaves them.
pub fn is_delimiter(c: char) -> bool {
    is_ws(c) || matches!(c, '(' | ')' | ']' | '}' | '"' | '\'' | '`' | ';')
}

fn is_delim_or_eof(c: Option<char>) -> bool {
    match c {
        None => true,
        Some(c) => is_delimiter(c),
    }
}

// ----------------------------------------------------------------------
// Whitespace & comments
// ----------------------------------------------------------------------

fn skip_ws_comments(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>) -> Result<(), Error> {
    loop {
        match cur.peek() {
            Some(c) if is_ws(c) => {
                cur.next();
            }
            Some(';') => {
                // line comment: to end of line
                while let Some(c) = cur.next() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            Some('#') if cur.peek2() == Some('|') => {
                skip_block_comment(cur);
            }
            Some('#') if matches!(cur.peek2(), Some('+') | Some('-')) => {
                skip_feature_conditional(cur, heap, ctx)?;
            }
            _ => break,
        }
    }
    Ok(())
}

/// Handle one `#+feature-expr` / `#-feature-expr` reader conditional (CLHS
/// 24.1.2). Consumes the dispatch character and the sign, then reads the
/// feature expression as an ordinary datum and evaluates it against
/// `features` ([`eval_feature_expr`]).
///
/// When the test holds — `#+` and the expression is present, or `#-` and it
/// isn't — the guarded form is left untouched for the caller to read
/// normally, so it gets exactly the `Loc` an unguarded form would. When the
/// test fails, the guarded form is read (so parens stay balanced, nested
/// `#+`/`#-` inside it still apply) and discarded — it never becomes part of
/// any result, exactly as if it had been whitespace.
fn skip_feature_conditional(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>) -> Result<(), Error> {
    cur.next(); // '#'
    let want_present = match cur.next() {
        Some('+') => true,
        Some('-') => false,
        c => unreachable!("skip_ws_comments only dispatches here on #+/#-, got {:?}", c),
    };
    let expr = read_datum(cur, heap, Ctx { ns: NsId::ROOT, ..ctx })?;
    let present = eval_feature_expr(heap, ctx.features, expr)?;
    if present != want_present {
        read_datum(cur, heap, Ctx { ns: NsId::ROOT, ..ctx })?; // test failed: read and discard the guarded form
    }
    Ok(())
}

/// Evaluate a `#+`/`#-` feature expression against `features`: a bare keyword
/// (e.g. `:darwin`) tests membership; `(and e...)`, `(or e...)`, `(not e)`
/// combine nested feature expressions the same way CL does (CLHS 24.1.2).
/// `heap` is only read from (symbol names, `car`/`cdr`), never allocated
/// into.
fn eval_feature_expr(heap: &Heap, features: &Features, v: Value) -> Result<bool, Error> {
    match v {
        Value::Symbol(id) => {
            if !id.is_keyword() {
                return Err(Error::ReadError(format!(
                    "feature expression must be a keyword (e.g. `:darwin`) or an `(and/or/not ...)` form, found `{}`",
                    heap.symbol_spelling(id)
                )));
            }
            Ok(features.has(id))
        }
        Value::Cons(_) => {
            let op_sym = heap.car(v)?;
            let Value::Symbol(op_id) = op_sym else {
                return Err(Error::ReadError(
                    "feature expression list must start with `and`, `or`, or `not`".to_string(),
                ));
            };
            let mut operands = Vec::new();
            let mut rest = heap.cdr(v)?;
            loop {
                match rest {
                    Value::Empty => break,
                    Value::Cons(_) => {
                        operands.push(heap.car(rest)?);
                        rest = heap.cdr(rest)?;
                    }
                    _ => return Err(Error::ReadError("feature expression must be a proper list".to_string())),
                }
            }
            match op_id.well_known() {
                wk::AND => {
                    for o in operands {
                        if !eval_feature_expr(heap, features, o)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                wk::OR => {
                    for o in operands {
                        if eval_feature_expr(heap, features, o)? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                }
                wk::NOT => {
                    if operands.len() != 1 {
                        return Err(Error::ReadError("`not` feature expression takes exactly one operand".to_string()));
                    }
                    Ok(!eval_feature_expr(heap, features, operands[0])?)
                }
                _ => Err(Error::ReadError(format!("unknown feature expression operator: `{}`", heap.symbol_spelling(op_id)))),
            }
        }
        _ => Err(Error::ReadError(
            "feature expression must be a keyword (e.g. `:darwin`) or an `(and/or/not ...)` form".to_string(),
        )),
    }
}

/// Skip a `#| ... |#` block comment (nesting allowed). Consumes to EOF if
/// unterminated (a following read then sees EOF).
fn skip_block_comment(cur: &mut Cursor) {
    cur.next(); // '#'
    cur.next(); // '|'
    let mut depth = 1;
    while depth > 0 {
        match cur.next() {
            None => break,
            Some('#') if cur.peek() == Some('|') => {
                cur.next();
                depth += 1;
            }
            Some('|') if cur.peek() == Some('#') => {
                cur.next();
                depth -= 1;
            }
            _ => {}
        }
    }
}

// ----------------------------------------------------------------------
// Datum dispatch
// ----------------------------------------------------------------------

/// Every nested datum is read through here, once per level, so this is where
/// the stack is grown (`typelisp_mem::grow_stack`).
fn read_datum(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>) -> Result<Value, Error> {
    typelisp_mem::grow_stack(|| read_datum_here(cur, heap, ctx))
}

/// [`read_datum`] on the stack it is given.
fn read_datum_here(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>) -> Result<Value, Error> {
    skip_ws_comments(cur, heap, ctx)?;
    // A registered macro character wins over everything the reader would
    // otherwise do with it — that is what registering one means. Looked up
    // before the built-in dispatch below, so `(` and `'` can be taken over
    // too; a character with no entry costs one hash lookup.
    if let Some(c) = cur.peek() {
        if let Some(f) = heap.macro_character(c) {
            return call_macro_char(cur, heap, ctx, f, c);
        }
        // A *dispatching* character: the one after it selects the function.
        // `#` never arrives here (`is_dispatch_char` excludes it) — it has a
        // built-in dispatch of its own, and that one already looks in the
        // same table first.
        if heap.is_dispatch_char(c) {
            cur.next(); // the dispatching character
            let Some(sub) = cur.peek() else {
                return Err(Error::ReadError(format!("`{}` at end of input: a dispatching character needs one more", c)));
            };
            match heap.dispatch_macro_character(c, sub) {
                Some(f) => return call_macro_char(cur, heap, ctx, f, sub),
                None => {
                    return Err(Error::ReadError(format!("`{}{}` has no reader macro", c, sub)));
                }
            }
        }
    }
    match cur.peek() {
        None => Err(Error::ReadError("unexpected end of input".to_string())),
        Some('(') => read_list(cur, heap, ctx),
        Some(')') => Err(Error::UnmatchedParen),
        // Nothing the reader knows opens with these, so meeting one where a
        // datum starts means its opener is missing (or it is a typo for `)`).
        Some(c @ (']' | '}')) => Err(Error::ReadError(format!("unexpected `{}`: nothing open for it to close", c))),
        Some('\'') => read_wrapped(cur, heap, ctx, symbols::well_known_symbol(wk::QUOTE)),
        Some('`') => read_wrapped(cur, heap, ctx, symbols::well_known_symbol(wk::QUASIQUOTE)),
        Some(',') => {
            let start = cur.loc(); // before the prefix, like read_wrapped
            cur.next(); // the ','
            if cur.peek() == Some('@') {
                cur.next(); // the '@'
                read_wrapped_body(cur, heap, ctx, symbols::well_known_symbol(wk::UNQUOTE_SPLICING), start)
            } else {
                read_wrapped_body(cur, heap, ctx, symbols::well_known_symbol(wk::UNQUOTE), start)
            }
        }
        Some('"') => read_string(cur, heap),
        Some('#') => read_hash(cur, heap, ctx),
        Some(_) => {
            let start = cur.loc();
            let v = read_atom(cur, heap, ctx.ns, ctx.paths)?;
            // `:dyn Trait` (the trait-object type) is written as two
            // whitespace-separated words, so the reader joins them into the
            // single datum `(:dyn Trait)` — exactly the treatment `'x` gets.
            // Every type position (parameter/field pairs, return types, `(fn
            // ...)` types) can then keep its "a type is one `Value`"
            // assumption unchanged; `types::parse_type` recognizes the
            // resulting list. Inside a generic argument the same spelling is
            // read by `read_type_arg`, which builds the same list.
            if matches!(v, Value::Symbol(id) if id.is(wk::DYN)) {
                skip_ws_comments(cur, heap, ctx)?;
                if matches!(cur.peek(), None | Some(')')) {
                    return Err(Error::ReadError("`:dyn` must be followed by a trait name".to_string()));
                }
                return read_wrapped_body(cur, heap, ctx, symbols::well_known_symbol(wk::DYN), start);
            }
            Ok(v)
        }
    }
}

/// [`read_datum`] plus the source span the datum was read from. The start is
/// captured *after* skipping leading whitespace/comments (so `read_datum`'s
/// own leading skip is a no-op) and the end right after the datum's last
/// character — no read function consumes trailing whitespace, so the cursor
/// sits exactly past the datum when `read_datum` returns.
fn read_datum_spanned(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>) -> Result<(Value, Loc), Error> {
    skip_ws_comments(cur, heap, ctx)?;
    let start = cur.loc();
    let v = read_datum(cur, heap, ctx)?;
    Ok((v, start.with_end(cur.line, cur.col)))
}

/// Hand the rest of the text to a reader macro, and advance the cursor by
/// however much of it the macro read.
///
/// The macro character itself is consumed first, so what the function sees
/// begins after it — CL's convention, where the character is passed
/// separately rather than left in the stream.
fn call_macro_char(
    cur: &mut Cursor,
    heap: &mut Heap,
    ctx: Ctx<'_>,
    f: Value,
    ch: char,
) -> Result<Value, Error> {
    let Some(eval) = ctx.eval else {
        return Err(Error::ReadError(format!(
            "`{}` is a reader macro, and running one needs an evaluator this \
             reader was not given (a plain `read`/`read-from-string` has none)",
            ch
        )));
    };
    cur.next(); // the macro character
    let rest: String = cur.rest();
    let (v, used) = eval.call_reader_macro(heap, f, ch, &rest).map_err(Error::ReadError)?;
    cur.seek(cur.pos() + used);
    Ok(v)
}

/// Read `<prefix-char><datum>` as `(<head> datum)` — the shared shape behind
/// `'x` -> `(quote x)`, `` `x `` -> `(quasiquote x)`, `,x` -> `(unquote x)`,
/// `,@x` -> `(unquote-splicing x)`. Consumes the single prefix character
/// itself, then delegates the rest (reading `datum` and building the
/// 2-element list) to [`read_wrapped_body`] — `,@` needs to consume *two*
/// prefix characters (`,` then `@`), so its caller in [`read_datum`] does
/// that part itself and calls `read_wrapped_body` directly.
fn read_wrapped(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>, head: SymRef) -> Result<Value, Error> {
    let start = cur.loc(); // the prefix character — where the whole form begins
    cur.next(); // the prefix character
    read_wrapped_body(cur, heap, ctx, head, start)
}

/// Read `datum` and build `(head datum)`, the shared tail of [`read_wrapped`]
/// — assumes any prefix character(s) have already been consumed. `start` is
/// the position of the first prefix character; the synthesized 2-element list
/// gets the same location treatment as one read from explicit parens: the
/// head cell's `cons_loc` spans prefix through datum end, the `head` symbol's
/// `elem_loc` covers the prefix character(s), and the datum's `elem_loc` its
/// own span.
fn read_wrapped_body(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>, head: SymRef, start: Loc) -> Result<Value, Error> {
    let head_loc = start.clone().with_end(cur.line, cur.col); // the consumed prefix
    let (d, d_loc) = read_datum_spanned(cur, heap, ctx)?;
    let form_loc = start.with_end(cur.line, cur.col);
    heap.push_root(d);
    let q = Value::Symbol(head); // symbols are permanent; no rooting needed
    let result = (|| {
        let inner = heap.cons(d, Value::Empty)?;
        if let Value::Cons(cr) = inner {
            heap.set_elem_loc(cr, d_loc);
        }
        heap.push_root(inner);
        let r = heap.cons(q, inner);
        heap.pop_root(); // inner
        r
    })();
    heap.pop_root(); // d
    let outer = result?;
    if let Value::Cons(cr) = outer {
        heap.set_cons_loc(cr, form_loc);
        heap.set_elem_loc(cr, head_loc);
    }
    Ok(outer)
}

// ----------------------------------------------------------------------
// Lists & dotted pairs
// ----------------------------------------------------------------------

/// The namespace a `(module PATH body...)` form's body is read in, or `None`
/// when `head`/`path` are not that form.
///
/// `head` is compared by identity against the root `module`, which is what
/// `intern_in`'s inheritance rule guarantees every module sees (a nested
/// `module` written inside another still reads as the same symbol).
///
/// The path is relative to `ns`, matching `Checker::enter_module`, except for
/// the reader's absolute spelling `::foo` — encoded as a leading empty
/// segment, which restarts the descent from the root.
fn module_body_ns(heap: &Heap, ns: NsId, head: Value, path: Value) -> Option<NsId> {
    if !matches!(head, Value::Symbol(s) if s.is(wk::MODULE)) {
        return None;
    }
    let segs: Vec<&str> = match path {
        Value::Symbol(s) => vec![s.name()],
        Value::Path(id) => heap.path_segments(id).iter().map(|s| s.name()).collect(),
        _ => return None,
    };
    let mut cur = ns;
    for (i, seg) in segs.iter().enumerate() {
        if i == 0 && seg.is_empty() {
            cur = NsId::ROOT;
            continue;
        }
        cur = symbols::child_ns(cur, seg);
    }
    heap.names().declare_module(cur);
    Some(cur)
}

/// What a definition form makes readable through a type, registered as soon
/// as that part of it has been read — so the rest of the same form, and of
/// the `(module ...)` around it, can already write `point::new` or
/// `color::red`. `elems` is the list read so far, its last element just read.
///
/// - `(defstruct point`, `(defenum color`, `(deftrait shape`, `(deftype t`,
///   `(def-c-struct s`: the name is a type.
/// - each variant of a `defenum`, each field of a `defstruct` or a
///   `def-c-struct`, each method of a `deftrait`: a member of that type.
/// - each item of an `impl`: a member of the type the `impl` is for.
///
/// The names are the symbols as read here, in the module being read. A
/// definition a macro expansion makes is registered the same way by the
/// analysis that expands it.
fn note_definition(heap: &Heap, ns: NsId, elems: &[Value]) {
    let rest = match elems.first() {
        Some(Value::Symbol(s)) if s.is(wk::PUB) => &elems[1..],
        _ => elems,
    };
    let Some(&head) = rest.first() else { return };
    let Some(&last) = rest.last() else { return };
    let head_sym = match head {
        Value::Symbol(s) => s,
        Value::Cons(_) if is_marked(heap, head, wk::TYPE_APPLY) => match heap.list_to_vec(head).ok().and_then(|v| v.get(1).copied()) {
            Some(Value::Symbol(s)) if s.is(wk::IMPL) => s,
            _ => return,
        },
        _ => return,
    };
    match head_sym.well_known() {
        wk::DEFSTRUCT | wk::DEFENUM | wk::DEFTRAIT | wk::DEFTYPE | wk::DEF_C_STRUCT if rest.len() == 2 => {
            // Only a name that is the module's own symbol: one the standard
            // library offers, defined without `shadow`, is refused when the
            // definition is checked.
            if let Some((home, ty)) = defined_type_name(heap, ns, last).filter(|(home, ty)| ty.home() == *home) {
                heap.names().declare_type(home, ty);
            }
        }
        // A `deftrait`'s third element is its list of supertraits.
        wk::DEFENUM | wk::DEFSTRUCT | wk::DEF_C_STRUCT | wk::DEFTRAIT
            if rest.len() > 2 && !(head_sym.is(wk::DEFTRAIT) && rest.len() == 3) =>
        {
            if let (Some(ty), Some(m)) = (defined_type_name(heap, ns, rest[1]).map(|(_, ty)| ty), member_name(heap, last)) {
                heap.names().declare_member(ty, m);
            }
        }
        wk::IMPL if rest.len() > 2 => {
            // `(impl Type item...)` or `(impl Trait Type item...)`: the type
            // is what stands just before the first item.
            let Some(first_item) = rest.iter().position(|&e| impl_item_name(heap, e).is_some()) else { return };
            let Some(name) = impl_item_name(heap, last) else { return };
            if let Some(ty) = rest.get(first_item - 1).and_then(|&t| impl_target(heap, ns, t)) {
                heap.names().declare_member(ty, name);
            }
        }
        _ => {}
    }
}

/// The name a variant, a field or a trait method is written under: `red`,
/// `(blue int)`, `(x int)`, `(pub x int)`, `(area ((self Self)) int)`. A
/// docstring or a keyword option is no member.
fn member_name(heap: &Heap, v: Value) -> Option<SymRef> {
    let name = match v {
        Value::Symbol(s) => s,
        Value::Cons(_) => match heap.list_to_vec(v).ok()?.as_slice() {
            [Value::Symbol(p), Value::Symbol(n), ..] if p.is(wk::PUB) => *n,
            [Value::Symbol(n), ..] => *n,
            _ => return None,
        },
        _ => return None,
    };
    (!name.is_keyword()).then_some(name)
}

/// Whether `head` begins a form that brings names in: `use`, `import`,
/// `shadowing-import`.
fn is_use_head(head: SymRef) -> bool {
    head.is(wk::USE) || head.is(wk::IMPORT) || head.is(wk::SHADOWING_IMPORT)
}

/// A form's elements read so far, without the `pub` a re-export
/// (`(pub use path)`) begins with: what is read as a `use`.
fn without_pub(elems: &[Value]) -> &[Value] {
    match elems {
        [Value::Symbol(p), rest @ ..] if p.is(wk::PUB) => rest,
        _ => elems,
    }
}

/// What a `(use path...)` form brings in, brought in as soon as each path
/// has been read — so the rest of the same form, and of the `(module ...)`
/// around it, can read through it (`point::origin-x` after
/// `(use geo::point)`). A path to a module or type the session has is
/// registered here; anything else is the evaluator's to bring in
/// (`ReadEval::bring_in`), which loads the file behind it. Without an
/// evaluator nothing can be loaded, and the `use` reports the path when it
/// is checked.
fn note_use(heap: &mut Heap, ctx: Ctx<'_>, elems: &[Value]) -> Result<(), Error> {
    let elems = without_pub(elems);
    let (Some(Value::Symbol(head)), Some(&last)) = (elems.first(), elems.last()) else { return Ok(()) };
    if elems.len() < 2 || !is_use_head(*head) {
        return Ok(());
    }
    let syms: Vec<SymRef> = match last {
        Value::Symbol(s) => vec![s],
        Value::Path(id) => heap.path_segments(id).to_vec(),
        _ => return Ok(()),
    };
    let segs: Vec<&'static str> = syms.iter().map(|s| s.name()).collect();
    if heap.names().import_written(ctx.ns, &segs) {
        return Ok(());
    }
    match ctx.eval {
        Some(eval) => eval.bring_in(heap, ctx.ns, &syms).map_err(Error::ReadError),
        None => Ok(()),
    }
}

/// A `(shadow name...)` form's latest name: made the module's own as soon as
/// it has been read, so the rest of the module reads it so, and put in the
/// form as that symbol.
fn note_shadow(heap: &mut Heap, ns: NsId, elems: &mut [Value]) -> Result<(), Error> {
    let [Value::Symbol(head), .., last] = elems else { return Ok(()) };
    if !head.is(wk::SHADOW) {
        return Ok(());
    }
    let Value::Symbol(name) = *last else {
        return Err(Error::ReadError("shadow: expected names".to_string()));
    };
    if name.is_keyword() {
        return Err(Error::ReadError(format!("shadow: `{}` is a keyword, which no module can make its own", name.spelling())));
    }
    let own = heap.names().shadow(ns, name.name()).map_err(Error::ReadError)?;
    *last = Value::Symbol(own);
    Ok(())
}

/// Whether `v` is a list headed by the vocabulary entry `marker`.
fn is_marked(heap: &Heap, v: Value, marker: u32) -> bool {
    matches!(heap.car(v), Ok(Value::Symbol(s)) if s.is(marker))
}

/// The type a definition's name names, written in `ns`: `point`,
/// `m::point`, `(point options...)` or `point<T>` (`(%type-apply point T)`),
/// with the module it is defined in.
fn defined_type_name(heap: &Heap, ns: NsId, v: Value) -> Option<(NsId, SymRef)> {
    match v {
        Value::Symbol(s) => Some((ns, s)),
        Value::Path(id) => {
            let segs = heap.path_segments(id);
            let (last, prefix) = segs.split_last()?;
            match heap.names().reached(ns, prefix).ok()? {
                symbols::PathStep::Module(m) => Some((m, *last)),
                symbols::PathStep::Type(_) => None,
            }
        }
        Value::Cons(_) if is_marked(heap, v, wk::TYPE_APPLY) => defined_type_name(heap, ns, heap.list_to_vec(v).ok()?.get(1).copied()?),
        Value::Cons(_) => defined_type_name(heap, ns, heap.car(v).ok()?),
        _ => None,
    }
}

/// The type an `impl` is for, as its target names it: a plain name refers to
/// a type the way a path's first segment does (`SessionNames::type_named`); a
/// path has been read to the type's own symbol already. `None` when the
/// target names no type the reader knows of — a type parameter, or a type a
/// macro has yet to define.
fn impl_target(heap: &Heap, ns: NsId, v: Value) -> Option<SymRef> {
    match v {
        Value::Symbol(s) => heap.names().type_named(ns, s.name()),
        Value::Path(id) => heap.path_segments(id).last().copied(),
        Value::Cons(_) if is_marked(heap, v, wk::TYPE_APPLY) => impl_target(heap, ns, heap.list_to_vec(v).ok()?.get(1).copied()?),
        _ => None,
    }
}

/// The name an `impl` item defines: `(name (params...) ...)`,
/// `(name<U> (params...) ...)` or `(pub name ...)`. A type argument list or
/// a `(:where ...)` clause is no item.
fn impl_item_name(heap: &Heap, v: Value) -> Option<SymRef> {
    if !matches!(v, Value::Cons(_)) || is_marked(heap, v, wk::TYPE_APPLY) {
        return None;
    }
    let items = heap.list_to_vec(v).ok()?;
    let (written, rest) = match items.as_slice() {
        [Value::Symbol(p), n, rest @ ..] if p.is(wk::PUB) => (*n, rest),
        [n, rest @ ..] => (*n, rest),
        [] => return None,
    };
    let name = match written {
        Value::Symbol(n) => n,
        // `name<U>`: a method with type parameters of its own.
        Value::Cons(_) if is_marked(heap, written, wk::TYPE_APPLY) => match heap.list_to_vec(written).ok()?.get(1) {
            Some(Value::Symbol(n)) => *n,
            _ => return None,
        },
        _ => return None,
    };
    if !matches!(rest.first(), Some(Value::Cons(_) | Value::Empty)) {
        return None;
    }
    (!name.is_keyword()).then_some(name)
}

fn read_list(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>) -> Result<Value, Error> {
    let open_loc = cur.loc(); // position of the '(' — the list form's location
    cur.next(); // '('
    let mark = heap.root_count();
    // The namespace this list's *elements* are read in. It is `ns` until the
    // list turns out to be `(module PATH body...)`, at which point everything
    // after `PATH` belongs to that module — see `module_body_ns`. Nesting is
    // lexical, so one pass settles it with no dynamic `*package*` to bind.
    let mut body_ns = ctx.ns;
    // Whether this list turned out to be `(module PATH body...)`.
    let mut module_body = false;
    let mut elems: Vec<Value> = Vec::new();
    // Source location of each element in `elems`, captured just before reading
    // it — parallel to `elems`, so the build loop below can tag each spine
    // cell's `car` with where that element began (see `Heap::set_elem_loc`).
    let mut elem_locs: Vec<Loc> = Vec::new();
    let mut tail = Value::Empty;

    loop {
        if let Err(e) = skip_ws_comments(cur, heap, ctx) {
            restore_roots(heap, mark);
            return Err(e);
        }
        match cur.peek() {
            None => {
                restore_roots(heap, mark);
                return Err(Error::IllegalEndWhileReadingList);
            }
            Some(')') => {
                cur.next();
                break;
            }
            // consing dot: a lone `.` (followed by a delimiter) after >=1 element
            Some('.') if is_delim_or_eof(cur.peek2()) => {
                cur.next(); // '.'
                if elems.is_empty() {
                    restore_roots(heap, mark);
                    return Err(Error::ReadError("dotted pair has no car".to_string()));
                }
                let d = match read_datum(cur, heap, ctx) {
                    Ok(d) => d,
                    Err(e) => {
                        restore_roots(heap, mark);
                        return Err(e);
                    }
                };
                heap.push_root(d);
                tail = d;
                if let Err(e) = skip_ws_comments(cur, heap, ctx) {
                    restore_roots(heap, mark);
                    return Err(e);
                }
                if cur.peek() == Some(')') {
                    cur.next();
                } else {
                    restore_roots(heap, mark);
                    return Err(Error::ReadError("expected ) after dotted cdr".to_string()));
                }
                break;
            }
            Some(_) => {
                let paths = match without_pub(&elems).first() {
                    Some(Value::Symbol(h)) if is_use_head(*h) => PathMode::Raw,
                    Some(Value::Symbol(h)) if elems.len() == 1 && (h.is(wk::MODULE) || h.is(wk::IN_MODULE)) => PathMode::Declare,
                    _ => PathMode::Resolve,
                };
                // A `(module PATH form...)` body's forms are top-level forms too.
                let toplevel = ctx.toplevel && module_body;
                let (e, elem_loc) = match read_datum_spanned(cur, heap, Ctx { ns: body_ns, paths, toplevel, ..ctx }) {
                    Ok(pair) => pair,
                    Err(err) => {
                        restore_roots(heap, mark);
                        return Err(err);
                    }
                };
                heap.push_root(e); // keep alive while reading the rest / building
                elems.push(e);
                elem_locs.push(elem_loc);
                if elems.len() == 2 {
                    if let Some(inner) = module_body_ns(heap, ctx.ns, elems[0], elems[1]) {
                        body_ns = inner;
                        module_body = true;
                    }
                }
                if ctx.toplevel {
                    if let Err(err) = note_shadow(heap, ctx.ns, &mut elems) {
                        restore_roots(heap, mark);
                        return Err(err);
                    }
                    note_definition(heap, ctx.ns, &elems);
                    if let Err(err) = note_use(heap, ctx, &elems) {
                        restore_roots(heap, mark);
                        return Err(err);
                    }
                }
            }
        }
    }
    // The cursor now sits just past the closing ')' (both loop exits consume
    // it), which is the list form's exclusive end; the cons-chain build below
    // never moves the cursor.
    let list_loc = open_loc.with_end(cur.line, cur.col);

    // Build the cons chain from the back; the accumulator stays rooted so a GC
    // triggered by `cons` cannot reclaim the part already built. Each spine
    // cell is tagged with its `car`'s source location (`elem_locs` below,
    // walked in the same reverse order as `elems`) so the checker can give a
    // bare-atom element its own `Loc` — see `Heap::set_elem_loc`, which records
    // it in the spine cell itself.
    let mut acc = tail;
    heap.push_root(acc);
    for (&e, loc) in elems.iter().rev().zip(elem_locs.iter().rev()) {
        match heap.cons(e, acc) {
            Ok(cell) => {
                heap.pop_root(); // old acc
                heap.push_root(cell);
                acc = cell;
                if let Value::Cons(cr) = cell {
                    heap.set_elem_loc(cr, loc.clone());
                }
            }
            Err(err) => {
                restore_roots(heap, mark);
                return Err(err);
            }
        }
    }

    restore_roots(heap, mark);
    // Record this list form's span ('(' through ')'), so the checker/
    // interpreter can point an error at it. Only the head cell is tagged;
    // each nested list records its own head via its own `read_list` call. An
    // empty list `()` reads as `Value::Empty` (no cell), so there is nothing
    // to tag then.
    if let Value::Cons(cr) = acc {
        heap.set_cons_loc(cr, list_loc);
    }
    Ok(acc)
}

/// Pop roots back down to `mark`.
fn restore_roots(heap: &mut Heap, mark: usize) {
    while heap.root_count() > mark {
        heap.pop_root();
    }
}

// ----------------------------------------------------------------------
// Strings & characters
// ----------------------------------------------------------------------

fn read_string(cur: &mut Cursor, heap: &mut Heap) -> Result<Value, Error> {
    cur.next(); // opening '"'
    let mut s = String::new();
    loop {
        match cur.next() {
            None => return Err(Error::IllegalEndOfString),
            Some('"') => break,
            Some('\\') => match cur.next() {
                None => return Err(Error::IllegalEndOfEscapeSequence),
                Some('n') => s.push('\n'),
                Some('t') => s.push('\t'),
                Some('r') => s.push('\r'),
                Some('0') => s.push('\0'),
                Some('\\') => s.push('\\'),
                Some('"') => s.push('"'),
                Some(other) => s.push(other),
            },
            Some(c) => s.push(c),
        }
    }
    Ok(heap.alloc_string(s))
}

fn read_hash(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>) -> Result<Value, Error> {
    cur.next(); // '#'
    // A registered `#<sub>` wins over the built-in ones, which is what makes
    // `set-dispatch-macro-character` worth having: `#b`/`#x`/`#.` are exactly
    // the shape a program wants to extend, and overriding one is the caller's
    // business.
    if let Some(sub) = cur.peek() {
        if let Some(f) = heap.dispatch_macro_character('#', sub) {
            return call_macro_char(cur, heap, ctx, f, sub);
        }
    }
    match cur.peek() {
        Some('\\') => {
            cur.next(); // '\\'
            read_char(cur)
        }
        // `#(a b c)` — a vector, as in CL.
        Some('(') => read_vector(cur, heap, ctx),
        // `#{a b c}` — a tuple.
        Some('{') => read_tuple(cur, heap, ctx),
        // `#[test] form` — the form, with attributes (Rust's spelling).
        Some('[') => read_attribute(cur, heap, ctx),
        // `#.` — read the next datum, run it, and read the value in its
        // place. The one place the reader is not a function of its text
        // alone, and the reason `ReadEval` exists.
        Some('.') => {
            cur.next(); // '.'
            let form = read_datum(cur, heap, ctx)?;
            let Some(eval) = ctx.eval else {
                return Err(Error::ReadError(
                    "`#.` needs to run code while reading, and this reader was given no evaluator \
                     (a plain `read`/`read-from-string` has none — `#.` works where source is \
                     being loaded)"
                        .to_string(),
                ));
            };
            // Rooted across the call: evaluating allocates, and `form` is
            // reachable from nothing else until the value replaces it.
            heap.push_root(form);
            let result = eval.read_eval(heap, form);
            heap.pop_root();
            result.map_err(Error::ReadError)
        }
        // CL's radix macros. The counterpart of `*print-radix*`, which prints
        // exactly these: without them the marker it adds so a number "reads
        // back whatever `*read-base*` is" would be unreadable here.
        Some('b') | Some('B') => {
            cur.next();
            read_radix(cur, heap, 2)
        }
        Some('o') | Some('O') => {
            cur.next();
            read_radix(cur, heap, 8)
        }
        Some('x') | Some('X') => {
            cur.next();
            read_radix(cur, heap, 16)
        }
        Some(c) if c.is_ascii_digit() => {
            let mut digits = String::new();
            while let Some(d) = cur.peek() {
                if d.is_ascii_digit() {
                    digits.push(d);
                    cur.next();
                } else {
                    break;
                }
            }
            match cur.peek() {
                Some('r') | Some('R') => cur.next(),
                // `#2A((1 2) (3 4))` — an array of that many dimensions.
                Some('a') | Some('A') => {
                    cur.next();
                    let rank: usize = digits
                        .parse()
                        .map_err(|_| Error::ReadError(format!("#{}A: rank does not fit", digits)))?;
                    return read_array(cur, heap, ctx, rank);
                }
                other => {
                    return Err(Error::ReadError(format!(
                        "#{} must be followed by `r` (the radix macro `#NNrDIGITS`) or `A` (an array), not {:?}",
                        digits, other
                    )))
                }
            };
            let radix: u32 = digits
                .parse()
                .map_err(|_| Error::ReadError(format!("#{}r: radix does not fit", digits)))?;
            if !(2..=36).contains(&radix) {
                return Err(Error::ReadError(format!(
                    "#{}r: radix must be between 2 and 36",
                    digits
                )));
            }
            read_radix(cur, heap, radix)
        }
        other => Err(Error::ReadError(format!("unsupported # syntax: #{:?}", other))),
    }
}

/// The `Vector<int>` box an array's dimensions are kept in. Written here
/// because the reader has no checker to ask; `tests/type_identity_guard_test.rs`
/// checks it against the type the checker gives it.
pub const ARRAY_DIMS_TYPE: TypeSpec = TypeSpec::Named(StdModule::VEC, "Vector", &[spec::INT]);

/// `#(a b c)`: the elements, read as data, in a `Vector<Option<Sexpr>>` box —
/// the `Sexpr` `vector` variant. The cursor is on the `(`.
fn read_vector(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>) -> Result<Value, Error> {
    cur.next(); // '('
    let mark = heap.root_count();
    let elems = read_elements(cur, heap, ctx, ')', "#(")?;
    let v = heap.alloc_struct(TypeId::SEXPR_VECTOR, elems);
    restore_roots(heap, mark);
    Ok(v)
}

/// `#{a b c}`: the elements, read as data, in a tuple box whose elements are
/// all `Option<Sexpr>` — the `Sexpr` `tuple` variant. The cursor is on the
/// `{`.
fn read_tuple(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>) -> Result<Value, Error> {
    cur.next(); // '{'
    let mark = heap.root_count();
    let elems = read_elements(cur, heap, ctx, '}', "#{")?;
    if !(1..=typelisp_mem::TUPLE_MAX_ARITY).contains(&elems.len()) {
        restore_roots(heap, mark);
        return Err(Error::ReadError(format!(
            "`#{{..}}`: a tuple has 1 to {} elements, not {}",
            typelisp_mem::TUPLE_MAX_ARITY,
            elems.len()
        )));
    }
    let key = typelisp_mem::sexpr_tuple_type(elems.len());
    let v = heap.alloc_struct(key, elems);
    restore_roots(heap, mark);
    Ok(v)
}

/// The head of the form `#[ATTR...] FORM` reads as:
/// `(%internal::attribute (ATTR...) FORM)`. Which attributes exist, and what
/// they do to `FORM`, is the checker's business.
pub const ATTRIBUTE_HEAD: [u32; 2] = [wk::INTERNAL_MODULE_NAME, wk::ATTRIBUTE];

/// `#[ATTR...] FORM` -> `(%internal::attribute (ATTR...) FORM)`. The cursor
/// is on the `[`. The attributes are data, and so is `FORM`: an attribute
/// belongs to the form after it, so there has to be one.
fn read_attribute(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>) -> Result<Value, Error> {
    cur.next(); // '['
    let mark = heap.root_count();
    let attrs = read_elements(cur, heap, ctx, ']', "#[")?;
    if attrs.is_empty() {
        restore_roots(heap, mark);
        return Err(Error::ReadError("`#[]` names no attribute".to_string()));
    }
    let result = (|| {
        skip_ws_comments(cur, heap, ctx)?;
        if matches!(cur.peek(), None | Some(')')) {
            return Err(Error::ReadError("`#[..]` must be followed by the form it belongs to".to_string()));
        }
        let form = read_datum(cur, heap, ctx)?;
        heap.push_root(form);
        let mut list = Value::Empty;
        for &a in attrs.iter().rev() {
            list = heap.cons(a, list)?;
            heap.push_root(list);
        }
        let segs: Vec<SymRef> = ATTRIBUTE_HEAD.iter().map(|&i| symbols::well_known_symbol(i)).collect();
        let head = heap.intern_path(&segs);
        let tail = heap.cons(form, Value::Empty)?;
        heap.push_root(tail);
        let tail = heap.cons(list, tail)?;
        heap.push_root(tail);
        heap.cons(head, tail)
    })();
    restore_roots(heap, mark);
    result
}

/// The data up to `close`, each rooted as it is read so that reading the next
/// cannot collect it. The roots stay pushed for the caller to release once it
/// has put the elements somewhere; on an error they are already released.
/// `opener` names the syntax for the end-of-input message.
fn read_elements(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>, close: char, opener: &str) -> Result<Vec<Value>, Error> {
    let mark = heap.root_count();
    let mut elems: Vec<Value> = Vec::new();
    loop {
        if let Err(e) = skip_ws_comments(cur, heap, ctx) {
            restore_roots(heap, mark);
            return Err(e);
        }
        match cur.peek() {
            None => {
                restore_roots(heap, mark);
                return Err(Error::ReadError(format!("end of input inside `{}`", opener)));
            }
            Some(c) if c == close => {
                cur.next();
                return Ok(elems);
            }
            Some(_) => match read_datum(cur, heap, ctx) {
                Ok(e) => {
                    heap.push_root(e);
                    elems.push(e);
                }
                Err(e) => {
                    restore_roots(heap, mark);
                    return Err(e);
                }
            },
        }
    }
}

/// `#NA datum`: an `Array<Option<Sexpr>>` box — the `Sexpr` `array` variant.
/// As in CL, `datum` is one object whose first `rank` levels of list nesting
/// are the dimensions (`#2A((1 2) (3 4))` is 2 by 2), and `#0A x` is the
/// zero-dimensional array holding `x`. A level whose lists differ in length
/// is an error: an array is rectangular. The cursor is just past the `A`.
///
/// The box has the fields of the prelude's `defstruct Array` in its order —
/// `dims`, `data` (row-major), `fill-pointer` (`none`).
fn read_array(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>, rank: usize) -> Result<Value, Error> {
    let mark = heap.root_count();
    let datum = read_datum(cur, heap, ctx)?;
    heap.push_root(datum);
    let mut dims: Vec<usize> = Vec::with_capacity(rank);
    let mut data: Vec<Value> = Vec::new();
    if let Err(e) = array_level(heap, datum, rank, 0, &mut dims, &mut data) {
        restore_roots(heap, mark);
        return Err(e);
    }
    // Every element is reachable from `datum`, which stays rooted until the
    // array exists.
    let dims: Vec<Value> = dims.iter().map(|&d| Value::Int(d as i64)).collect();
    let array = alloc_sexpr_array(heap, dims, data);
    restore_roots(heap, mark);
    Ok(array)
}

/// The `Sexpr` `array` variant's box: an `Array<Option<Sexpr>>` with the
/// prelude's `defstruct Array` fields in its order — `dims` (a `Vector<int>`
/// of `Value::Int`s), `data` (row-major), `fill-pointer` (`none`). The one
/// place that layout is written outside the prelude; the reader and compiled
/// code's quoted arrays both build through here.
///
/// The caller keeps every element of `data` rooted across the call.
pub fn alloc_sexpr_array(heap: &mut Heap, dims: Vec<Value>, data: Vec<Value>) -> Value {
    let mark = heap.root_count();
    let data_box = heap.alloc_struct(TypeId::SEXPR_VECTOR, data);
    heap.push_root(data_box);
    let dims_key = typelisp_mem::intern_spec(&ARRAY_DIMS_TYPE);
    let dims_box = heap.alloc_struct(dims_key, dims);
    heap.push_root(dims_box);
    let array = heap.alloc_struct(TypeId::SEXPR_ARRAY, vec![dims_box, data_box, Value::Empty]);
    restore_roots(heap, mark);
    array
}

/// Walk one level of `#NA`'s nested lists: at `depth == rank` the value is an
/// element; above it, a proper list whose length is this level's dimension —
/// set by the first list met at this depth and required of every other.
fn array_level(
    heap: &Heap,
    v: Value,
    rank: usize,
    depth: usize,
    dims: &mut Vec<usize>,
    data: &mut Vec<Value>,
) -> Result<(), Error> {
    if depth == rank {
        data.push(v);
        return Ok(());
    }
    let mut items = Vec::new();
    let mut cur = v;
    loop {
        match cur {
            Value::Empty => break,
            Value::Cons(_) => {
                items.push(heap.car(cur)?);
                cur = heap.cdr(cur)?;
            }
            _ => {
                return Err(Error::ReadError(format!(
                    "#{}A: level {} of the contents must be a proper list",
                    rank,
                    depth + 1
                )))
            }
        }
    }
    match dims.get(depth) {
        None => dims.push(items.len()),
        Some(&d) if d == items.len() => {}
        Some(&d) => {
            return Err(Error::ReadError(format!(
                "#{}A: the contents are not rectangular — dimension {} is {} in one place and {} in another",
                rank,
                depth + 1,
                d,
                items.len()
            )))
        }
    }
    if items.is_empty() {
        // An empty level has no element to take the deeper dimensions from;
        // they are 0, which is also what makes the array hold nothing.
        while dims.len() < rank {
            dims.push(0);
        }
        return Ok(());
    }
    for item in items {
        array_level(heap, item, rank, depth + 1, dims, data)?;
    }
    Ok(())
}

/// An integer literal as an `int`: a fixnum when it fits 63 bits, a bignum
/// box otherwise — `Heap::int_from_bigint`, the one place that decides, so
/// the datum the reader makes is the same canonical shape arithmetic makes.
///
/// The literal's *type* is the checker's business (a literal in an `i32`
/// position is an `i32` when it fits, and a range error when it does not);
/// what the reader decides is only the datum's shape, and that is decided by
/// size alone.
fn int_or_bignum(heap: &mut Heap, n: BigInt) -> Value {
    heap.int_from_bigint(n)
}

/// The rational `numer/denom` (`numer` already signed), normalized on read
/// exactly as CL specifies: `4/2` *is* the integer `2` (and `2/4` is `1/2`) —
/// an integer-valued ratio reads as an `int`, never a denominator-1 `ratio`.
fn ratio_value(heap: &mut Heap, numer: BigInt, denom: BigInt, tok: &str) -> Result<Value, Error> {
    if denom == BigInt::from(0) {
        return Err(Error::ReadError(format!("ratio literal with zero denominator: {}", tok)));
    }
    let r = BigRational::new(numer, denom);
    if r.is_integer() {
        return Ok(int_or_bignum(heap, r.to_integer()));
    }
    Ok(heap.alloc_ratio(r))
}

/// The rational after a radix macro (`#x-1f`, `#b101`, `#36rZZ`, `#x1/F`).
/// The sign comes *after* the marker, which is where CL's printer puts it.
///
/// Past `i32`'s range an integer reads as a `bignum`, the same
/// fixnum-or-bignum split [`parse_number`] makes for decimal tokens.
fn read_radix(cur: &mut Cursor, heap: &mut Heap, radix: u32) -> Result<Value, Error> {
    let mut tok = String::new();
    while let Some(c) = cur.peek() {
        if is_delimiter(c) {
            break;
        }
        tok.push(c);
        cur.next();
    }
    let (neg, body) = match tok.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, tok.strip_prefix('+').unwrap_or(&tok)),
    };
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_digit(radix));
    let parse = |s: &str| BigInt::parse_bytes(s.as_bytes(), radix).expect("digits of this radix parse as BigInt");
    if let Some((numer, denom)) = body.split_once('/') {
        if !digits(numer) || !digits(denom) {
            return Err(Error::ReadError(format!("`{}` is not a base-{} rational", tok, radix)));
        }
        let n = parse(numer);
        return ratio_value(heap, if neg { -n } else { n }, parse(denom), &tok);
    }
    if !digits(body) {
        return Err(Error::ReadError(format!(
            "`{}` is not a base-{} integer",
            tok, radix
        )));
    }
    let n = parse(body);
    Ok(int_or_bignum(heap, if neg { -n } else { n }))
}

fn read_char(cur: &mut Cursor) -> Result<Value, Error> {
    let first = cur
        .next()
        .ok_or_else(|| Error::ReadError("end of input after #\\".to_string()))?;
    // A multi-letter name (e.g. Space, Newline) only if it starts alphabetic.
    let mut name = String::new();
    name.push(first);
    if first.is_alphabetic() {
        while let Some(c) = cur.peek() {
            if c.is_alphanumeric() || c == '-' {
                name.push(c);
                cur.next();
            } else {
                break;
            }
        }
    }
    if name.chars().count() == 1 {
        return Ok(Value::Char(first));
    }
    let ch = match name.to_lowercase().as_str() {
        "space" => ' ',
        "newline" | "linefeed" => '\n',
        "tab" => '\t',
        "return" => '\r',
        "page" => '\u{0C}',
        "nul" | "null" => '\0',
        "backspace" => '\u{08}',
        _ => return Err(Error::ReadError(format!("unknown character name: {}", name))),
    };
    Ok(Value::Char(ch))
}

// ----------------------------------------------------------------------
// Atoms: numbers & symbols
// ----------------------------------------------------------------------

fn read_atom(cur: &mut Cursor, heap: &mut Heap, ns: NsId, paths: PathMode) -> Result<Value, Error> {
    let start = cur.loc();
    let (tok, escaped) = lex_token(cur, false)?;
    if paths != PathMode::Resolve && !escaped && tok.contains("::") && !tok.starts_with(':') {
        return path_as_named(heap, ns, &tok, paths);
    }
    if let Some(r) = read_value_member(heap, ns, &tok, escaped, &start) {
        if cur.peek() == Some('<') && cur.peek2().is_some_and(starts_type) {
            return Err(Error::ReadError(format!("`{}` is a member of a value, so it takes no `<..>`", tok)));
        }
        return r;
    }
    let head = token_value(heap, ns, &tok, escaped)?;
    // A name followed directly by `<` and a type is a generic type: what
    // follows, up to the matching `>`, is its type arguments (`read_type_application`).
    // Anything else after the `<` was taken into the token above, which is
    // what `<`, `<=`, `string<` and `(string< a b)` need.
    if !(cur.peek() == Some('<') && cur.peek2().is_some_and(starts_type)) {
        return Ok(head);
    }
    if escaped || !matches!(head, Value::Symbol(_) | Value::Path(_)) {
        return Err(Error::ReadError(format!("`{}` is not a type name, so it takes no `<..>`", tok)));
    }
    let head_loc = start.clone().with_end(cur.line, cur.col);
    let applied = read_type_application(cur, heap, ns, (head, head_loc), start.clone())?;
    // `Name<A>::member` — a member of that instantiation.
    if cur.peek() == Some(':') && cur.peek2() == Some(':') {
        let applied_loc = start.clone().with_end(cur.line, cur.col);
        cur.next();
        cur.next();
        let member_start = cur.loc();
        let (member_tok, member_escaped) = lex_token(cur, false)?;
        let member = token_value(heap, ns, &member_tok, member_escaped)?;
        if !matches!(member, Value::Symbol(_)) || cur.peek() == Some('<') {
            return Err(Error::ReadError(format!("`::{}` after a generic type must name one of its members", member_tok)));
        }
        let member_loc = member_start.with_end(cur.line, cur.col);
        heap.push_root(applied);
        let r = located_list(
            heap,
            &[
                (Value::Symbol(symbols::well_known_symbol(wk::TYPE_MEMBER)), start.clone().with_end(start.line, start.col)),
                (applied, applied_loc),
                (member, member_loc),
            ],
            start.with_end(cur.line, cur.col),
        );
        heap.pop_root();
        return r;
    }
    if !is_delim_or_eof(cur.peek()) {
        return Err(Error::ReadError(format!("unexpected `{}` after the type arguments of `{}`", cur.peek().unwrap_or(' '), tok)));
    }
    Ok(applied)
}

/// A path read in a position that names it rather than refers through it
/// ([`PathMode::Declare`], [`PathMode::Raw`]).
fn path_as_named(heap: &mut Heap, ns: NsId, tok: &str, paths: PathMode) -> Result<Value, Error> {
    let parts: Vec<&str> = tok.split("::").collect();
    if parts[1..].iter().any(|p| p.is_empty()) {
        return Err(Error::ReadError(format!("malformed path: {}", tok)));
    }
    let mut at = ns;
    let mut segs = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        if i == 0 && part.is_empty() {
            segs.push(symbols::intern_in(NsId::ROOT, ""));
            at = NsId::ROOT;
            continue;
        }
        segs.push(symbols::intern_in(at, part));
        if paths == PathMode::Declare {
            at = symbols::child_ns(at, part);
        }
    }
    if paths == PathMode::Declare {
        heap.names().declare_module(at);
    }
    Ok(heap.intern_path(&segs))
}

/// `p::x` as `(%value-member p x)`, when `p` names neither a module nor a
/// type (`SessionNames::names_scope`): a member of a value. Which `x` it is
/// depends on `p`'s type, and `p`'s binding exists only once the analysis
/// has registered it — after expanding the macros around it — so the reader
/// hands both names on as written, each read in the current module. `None`
/// when `tok` is not such a token.
fn read_value_member(heap: &mut Heap, ns: NsId, tok: &str, escaped: bool, start: &Loc) -> Option<Result<Value, Error>> {
    if escaped {
        return None;
    }
    let (head, member) = tok.split_once("::")?;
    if tok.starts_with(':') || member.contains("::") || member.is_empty() || heap.names().names_scope(ns, head) {
        return None;
    }
    let head_end = start.col + head.chars().count() as u32;
    let member_col = head_end + 2;
    let end = member_col + member.chars().count() as u32;
    let head_loc = start.clone().with_end(start.line, head_end);
    let mut member_loc = start.clone();
    member_loc.col = member_col;
    let member_loc = member_loc.with_end(start.line, end);
    let head = Value::Symbol(heap.names().read(ns, head));
    let member = heap.intern_symbol_in(ns, member);
    Some(located_list(
        heap,
        &[
            (Value::Symbol(symbols::well_known_symbol(wk::VALUE_MEMBER)), start.clone().with_end(start.line, start.col)),
            (head, head_loc),
            (member, member_loc),
        ],
        start.clone().with_end(start.line, end),
    ))
}

/// One token's characters, up to a delimiter — and, in a type argument
/// (`in_type_args`), up to the `<`, `>`, `,`, `{` or `}` the type grammar
/// separates names with. A `<` followed by a type ends the token too: the
/// caller reads what follows as type arguments. Whether any `|...|` escape
/// was in it is the second half of the answer.
fn lex_token(cur: &mut Cursor, in_type_args: bool) -> Result<(String, bool), Error> {
    let mut tok = String::new();
    // CL's multiple escape: what stands between `|`s is taken as written,
    // delimiters included, and a token with any is a plain symbol — never a
    // keyword, a number, a boolean or a path. `|:foo|` is how the printer
    // writes a symbol whose name starts with a colon.
    let mut escaped = false;
    while let Some(c) = cur.peek() {
        if c == '|' {
            cur.next();
            loop {
                match cur.next() {
                    Some('|') => break,
                    Some(c) => tok.push(c),
                    None => return Err(Error::ReadError("end of input inside `|...|`".to_string())),
                }
            }
            escaped = true;
            continue;
        }
        if is_delimiter(c) {
            break;
        }
        if in_type_args && matches!(c, '<' | '>' | ',' | '{' | '}') {
            break;
        }
        if c == '<' && !tok.is_empty() && cur.peek2().is_some_and(starts_type) {
            break;
        }
        tok.push(c);
        cur.next();
    }
    Ok((tok, escaped))
}

/// What a token reads as in module `ns`: a keyword, a number, a boolean, a
/// `::` path of symbols, or a symbol.
fn token_value(heap: &mut Heap, ns: NsId, tok: &str, escaped: bool) -> Result<Value, Error> {
    if escaped {
        return Ok(heap.intern_symbol_in(ns, tok));
    }
    if tok.is_empty() {
        return Err(Error::ReadError("expected a name".to_string()));
    }
    if let Some(name) = keyword_name(tok)? {
        return Ok(Value::Symbol(symbols::intern_keyword(name)));
    }
    if let Some(v) = parse_number(heap, tok)? {
        return Ok(v);
    }
    match tok {
        "true" => Ok(Value::Bool(true)),
        "false" => Ok(Value::Bool(false)),
        _ => {
            // A `::`-qualified token becomes a path of interned symbol segments.
            // A leading `::` (e.g. `::foo`) encodes an absolute path: the first
            // segment is the empty string, which the checker treats as "from root".
            if tok.contains("::") {
                let parts: Vec<&str> = tok.split("::").collect();
                // Interior empty segments (e.g. `foo::::bar`) are malformed.
                if parts[1..].iter().any(|p| p.is_empty()) {
                    return Err(Error::ReadError(format!("malformed path: {}", tok)));
                }
                // `foo::bar::zot` is `foo`'s `bar`'s `zot`: each segment is
                // read where the one before it led (`SessionNames::read_path`).
                let segs: Vec<SymRef> = heap.names().read_path(ns, &parts).map_err(Error::ReadError)?;
                return Ok(heap.intern_path(&segs));
            }
            let sym = heap.names().read(ns, tok);
            Ok(Value::Symbol(sym))
        }
    }
}

/// Whether `c` can begin a type: a name, `#{` (a tuple type), `()` (the
/// unit type), `:dyn`, or `!`.
fn starts_type(c: char) -> bool {
    c.is_alphabetic() || matches!(c, '_' | '%' | '*' | '#' | '(' | ':' | '!')
}

/// `head<A,B>` as `(%type-apply head A B)`: the cursor is on the `<`, and on
/// success just past the matching `>`. Each argument is a type read as data
/// ([`read_type_arg`]), so a name in it is read exactly as anywhere else.
///
/// A malformed argument list is a read error, not a reason to read the text
/// some other way: once a name is followed by `<` and a type, it is a type.
fn read_type_application(cur: &mut Cursor, heap: &mut Heap, ns: NsId, head: (Value, Loc), start: Loc) -> Result<Value, Error> {
    let mark = heap.root_count();
    let marker_loc = head.1.clone();
    let mut elems = vec![(Value::Symbol(symbols::well_known_symbol(wk::TYPE_APPLY)), marker_loc), head];
    cur.next(); // '<'
    let result = (|| loop {
        skip_type_ws(cur);
        let arg = read_type_arg(cur, heap, ns)?;
        heap.push_root(arg.0);
        elems.push(arg);
        skip_type_ws(cur);
        match cur.peek() {
            Some(',') => {
                cur.next();
            }
            Some('>') => {
                cur.next();
                return Ok(());
            }
            Some(c) => return Err(type_args_error(&format!("expected `,` or `>`, found `{}`", c))),
            None => return Err(type_args_error("end of input before the closing `>`")),
        }
    })();
    let built = result.and_then(|()| located_list(heap, &elems, start.with_end(cur.line, cur.col)));
    restore_roots(heap, mark);
    built
}

/// One type inside `<..>`, as data, with where it was written: `()`,
/// `#{T ...}`, `:dyn T` (as `(:dyn T)`), or a name with its own optional
/// `<..>`. A parenthesized type other than `()` is refused: a function type
/// is given a name with `deftype`.
fn read_type_arg(cur: &mut Cursor, heap: &mut Heap, ns: NsId) -> Result<(Value, Loc), Error> {
    let start = cur.loc();
    match cur.peek() {
        Some('(') => {
            if cur.peek2() != Some(')') {
                return Err(type_args_error(
                    "the only type written in parentheses inside `<..>` is `()`; give a function type a name with `deftype`",
                ));
            }
            cur.next();
            cur.next();
            Ok((Value::Empty, start.with_end(cur.line, cur.col)))
        }
        Some('#') if cur.peek2() == Some('{') => {
            cur.next();
            cur.next();
            let mark = heap.root_count();
            let mut elems: Vec<Value> = Vec::new();
            let result = (|| loop {
                skip_type_ws(cur);
                match cur.peek() {
                    Some('}') => {
                        cur.next();
                        return Ok(());
                    }
                    Some(_) => {
                        let (e, _) = read_type_arg(cur, heap, ns)?;
                        heap.push_root(e);
                        elems.push(e);
                    }
                    None => return Err(type_args_error("end of input before the closing `}` of a tuple type")),
                }
            })();
            if let Err(e) = result {
                restore_roots(heap, mark);
                return Err(e);
            }
            if !(1..=typelisp_mem::TUPLE_MAX_ARITY).contains(&elems.len()) {
                restore_roots(heap, mark);
                return Err(type_args_error(&format!("a tuple type has 1 to {} elements", typelisp_mem::TUPLE_MAX_ARITY)));
            }
            let key = typelisp_mem::sexpr_tuple_type(elems.len());
            let v = heap.alloc_struct(key, elems);
            restore_roots(heap, mark);
            Ok((v, start.with_end(cur.line, cur.col)))
        }
        Some(_) => {
            let (tok, escaped) = lex_token(cur, true)?;
            if tok.is_empty() {
                return Err(type_args_error("expected a type"));
            }
            let head_loc = start.clone().with_end(cur.line, cur.col);
            let head = token_value(heap, ns, &tok, escaped)?;
            if matches!(head, Value::Symbol(id) if id.is(wk::DYN)) {
                if !skip_type_ws(cur) {
                    return Err(type_args_error("`:dyn` needs a space before its trait"));
                }
                let inner = read_type_arg(cur, heap, ns)?;
                heap.push_root(inner.0);
                let r = located_list(heap, &[(head, head_loc), inner], start.clone().with_end(cur.line, cur.col));
                heap.pop_root();
                return Ok((r?, start.with_end(cur.line, cur.col)));
            }
            if cur.peek() == Some('<') {
                let v = read_type_application(cur, heap, ns, (head, head_loc), start.clone())?;
                return Ok((v, start.with_end(cur.line, cur.col)));
            }
            Ok((head, head_loc))
        }
        None => Err(type_args_error("end of input where a type was expected")),
    }
}

/// The list of `elems` — each element's car tagged with where it was written,
/// the list itself with `loc` — as [`read_list`] builds one. The elements
/// are the caller's to keep rooted.
fn located_list(heap: &mut Heap, elems: &[(Value, Loc)], loc: Loc) -> Result<Value, Error> {
    let mut acc = Value::Empty;
    let mark = heap.root_count();
    for (e, l) in elems.iter().rev() {
        let cell = match heap.cons(*e, acc) {
            Ok(c) => c,
            Err(err) => {
                restore_roots(heap, mark);
                return Err(err);
            }
        };
        heap.push_root(cell);
        if let Value::Cons(cr) = cell {
            heap.set_elem_loc(cr, l.clone());
        }
        acc = cell;
    }
    restore_roots(heap, mark);
    if let Value::Cons(cr) = acc {
        heap.set_cons_loc(cr, loc);
    }
    Ok(acc)
}

/// Skip whitespace inside type arguments, newlines included; whether there
/// was any.
fn skip_type_ws(cur: &mut Cursor) -> bool {
    let mut any = false;
    while cur.peek().is_some_and(is_ws) {
        cur.next();
        any = true;
    }
    any
}

fn type_args_error(what: &str) -> Error {
    Error::ReadError(format!("in type arguments: {}", what))
}

/// Reject malformed keyword tokens. A keyword is a token starting with a
/// single `:` (CL's self-evaluating `:name`, `Checker::check_inner`'s
/// `Value::Symbol` case) — it carries no package/path structure, so any
/// further `:` in it is a mistake rather than a path separator.
///
/// The leading-`::` exclusion is load-bearing: `::foo` is the *absolute path*
/// syntax (`token_value` turns it into a `Value::Path` whose first segment
/// is empty, "from root"), not a keyword. Only a lone leading `:`
/// starts a keyword.
///
/// A well-formed keyword token gives the keyword's name — the token without
/// its colon, which spells the keyword and is no part of the name.
fn keyword_name(tok: &str) -> Result<Option<&str>, Error> {
    if !tok.starts_with(':') || tok.starts_with("::") {
        return Ok(None);
    }
    if tok.len() == 1 {
        return Err(Error::ReadError("`:` alone is not a keyword — write `:name`".to_string()));
    }
    if tok[1..].contains(':') {
        return Err(Error::ReadError(format!(
            "`:` may not appear inside a keyword: `{}` (a keyword has no path segments)",
            tok
        )));
    }
    Ok(Some(&tok[1..]))
}

/// Interpret a token as a number, or `Ok(None)` if it is a symbol. Takes
/// `heap` (unlike an otherwise-pure parser) because a float/bignum/ratio
/// literal must be heap-boxed (`Heap::alloc_f64`/`int_from_bigint`/
/// `alloc_ratio`, see `BoxedObj`'s doc comment) — those payloads don't fit
/// alongside `Value`'s tag the way an int/char does. The only `Err` is a
/// ratio literal with a zero denominator (`1/0`), which CL's reader also
/// rejects — silently reading it as a *symbol* would hide the mistake.
fn parse_number(heap: &mut Heap, tok: &str) -> Result<Option<Value>, Error> {
    let (neg, body) = if let Some(r) = tok.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = tok.strip_prefix('+') {
        (false, r)
    } else {
        (false, tok)
    };

    // No `0x` hexadecimal literal: CL has no such syntax — its radix macros
    // are `#x`/`#b`/`#o`/`#NNr` (`read_radix`) — and `0xFF` there is not a
    // number token at all but the symbol `|0XFF|`. This reader used to accept
    // `0x` as a C/Rust import; it was removed on 2026-09-01 so that a token
    // falls through to the symbol case here exactly as CL says it does.

    // decimal integer: an integer past `i32`'s range reads as a `bignum`
    // (CL: fixnum vs bignum is a value-range distinction the reader makes,
    // not separate syntax).
    if !body.is_empty() && body.chars().all(|c| c.is_ascii_digit()) {
        let n = BigInt::parse_bytes(body.as_bytes(), 10).expect("all-digit token parses as BigInt");
        return Ok(Some(int_or_bignum(heap, if neg { -n } else { n })));
    }

    // ratio: `numer/denom`, both all-digit (CL ratio syntax; the other
    // bases are `read_radix`'s), normalized by `ratio_value`.
    if let Some((numer, denom)) = body.split_once('/') {
        if !numer.is_empty()
            && !denom.is_empty()
            && numer.chars().all(|c| c.is_ascii_digit())
            && denom.chars().all(|c| c.is_ascii_digit())
        {
            let n = BigInt::parse_bytes(numer.as_bytes(), 10).expect("all-digit numerator parses as BigInt");
            let d = BigInt::parse_bytes(denom.as_bytes(), 10).expect("all-digit denominator parses as BigInt");
            return ratio_value(heap, if neg { -n } else { n }, d, tok).map(Some);
        }
        return Ok(None);
    }

    // float: starts with a digit or '.', and has a '.' or exponent marker
    let Some(first) = body.chars().next() else { return Ok(None) };
    if (first.is_ascii_digit() || first == '.')
        && (body.contains('.') || body.contains('e') || body.contains('E'))
    {
        if let Ok(f) = body.parse::<f64>() {
            return Ok(Some(heap.alloc_f64(if neg { -f } else { f })));
        }
    }
    Ok(None)
}
