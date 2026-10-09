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

use crate::name_lexer::{NameLexer, NameTok};
use typelisp_mem::{symbols, wk, Error, Heap, Loc, NsId, SymRef, Value};

/// The feature set consulted by `#+`/`#-` reader conditionals (CLHS 24.1.2 —
/// the closest an s-expression reader has to a preprocessor). A feature is
/// named by its keyword spelling (`:darwin`, `:unix`, `:my-flag`, ...);
/// membership is checked case-insensitively, matching `Heap::intern_symbol`'s
/// own folding.
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
pub struct Features(HashSet<String>);

impl Features {
    /// The default set: `:typelisp`, the host architecture, and the host OS
    /// — following the CL convention of keyword-named implementation/
    /// platform features (e.g. SBCL's `:unix` / `:darwin` / `:x86-64`).
    pub fn host() -> Features {
        let mut set = HashSet::new();
        set.insert(":typelisp".to_string());
        set.insert(format!(":{}", std::env::consts::ARCH));
        match std::env::consts::OS {
            "macos" => {
                set.insert(":darwin".to_string());
                set.insert(":macos".to_string());
            }
            other => {
                set.insert(format!(":{}", other));
            }
        }
        if cfg!(unix) {
            set.insert(":unix".to_string());
        }
        if cfg!(windows) {
            set.insert(":windows".to_string());
        }
        Features(set)
    }

    /// This feature set with `extra` names also present, each folded to
    /// lowercase and given a leading `:` if it doesn't already have one —
    /// callers may pass either `"my-feature"` or `":my-feature"`.
    pub fn with(mut self, extra: impl IntoIterator<Item = String>) -> Features {
        for name in extra {
            let name = name.to_lowercase();
            let name = if name.starts_with(':') { name } else { format!(":{}", name) };
            self.0.insert(name);
        }
        self
    }

    fn has(&self, name: &str) -> bool {
        self.0.contains(name)
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
        self.read_in_with(heap, file, src, None)
    }

    /// [`Reader::read_in`] with an evaluator, so `#.` and macro characters
    /// work. The `read` *builtin* takes this path — CL's `read` consults the
    /// readtable, and a program that installed a macro character means it to
    /// apply to its own reads too.
    pub fn read_in_with(
        &self,
        heap: &mut Heap,
        file: &str,
        src: &str,
        eval: Option<&dyn ReadEval>,
    ) -> Result<Value, Error> {
        let mut cur = Cursor::new(file, src);
        read_datum(&mut cur, heap, Ctx { features: &self.features, ns: NsId::ROOT, eval }).map_err(|e| e.at(cur.loc()))
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
        self.read_from_with(heap, src, start, preserve_whitespace, None)
    }

    /// [`Reader::read_from`] with an evaluator — see [`Reader::read_in_with`].
    pub fn read_from_with(
        &self,
        heap: &mut Heap,
        src: &str,
        start: usize,
        preserve_whitespace: bool,
        eval: Option<&dyn ReadEval>,
    ) -> Result<(Value, usize), Error> {
        let mut cur = Cursor::new("<input>", src);
        cur.seek(start);
        let v = read_datum(&mut cur, heap, Ctx { features: &self.features, ns: NsId::ROOT, eval }).map_err(|e| e.at(cur.loc()))?;
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
        let ctx = Ctx { features: self.features, ns: self.ns, eval };
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
    /// Snapshot the full cursor state, for a speculative scan that may need to
    /// be undone ([`read_atom`]'s angle-bracket extension). Line/column are
    /// part of it so a rewind restores exact `Loc` reporting too.
    fn mark(&self) -> (usize, u32, u32) {
        (self.pos, self.line, self.col)
    }
    fn reset(&mut self, m: (usize, u32, u32)) {
        self.pos = m.0;
        self.line = m.1;
        self.col = m.2;
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
/// `,` is deliberately *not* here even though it introduces `unquote`: a
/// generic type token like `Pair<K,V>` is read as one plain symbol (split on
/// `<>`/`,` later, in `parse_type_name` — see `types.rs`), so `,` must stay
/// usable mid-token. `read_datum`'s dispatch on the *first* character of a
/// fresh datum already recognizes a datum-initial `,` as unquote regardless
/// of whether it's a delimiter; nothing relies on it being one.
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
            let name = heap.symbol_name(id);
            if !name.starts_with(':') {
                return Err(Error::ReadError(format!(
                    "feature expression must be a keyword (e.g. `:darwin`) or an `(and/or/not ...)` form, found `{}`",
                    name
                )));
            }
            Ok(features.has(name))
        }
        Value::Cons(_) => {
            let op_sym = heap.car(v)?;
            let Value::Symbol(op_id) = op_sym else {
                return Err(Error::ReadError(
                    "feature expression list must start with `and`, `or`, or `not`".to_string(),
                ));
            };
            let op = heap.symbol_name(op_id).to_string();
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
            match op.as_str() {
                "and" => {
                    for o in operands {
                        if !eval_feature_expr(heap, features, o)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                "or" => {
                    for o in operands {
                        if eval_feature_expr(heap, features, o)? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                }
                "not" => {
                    if operands.len() != 1 {
                        return Err(Error::ReadError("`not` feature expression takes exactly one operand".to_string()));
                    }
                    Ok(!eval_feature_expr(heap, features, operands[0])?)
                }
                other => Err(Error::ReadError(format!("unknown feature expression operator: `{}`", other))),
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

fn read_datum(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>) -> Result<Value, Error> {
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
        Some('\'') => read_wrapped(cur, heap, ctx, "quote"),
        Some('`') => read_wrapped(cur, heap, ctx, "quasiquote"),
        Some(',') => {
            let start = cur.loc(); // before the prefix, like read_wrapped
            cur.next(); // the ','
            if cur.peek() == Some('@') {
                cur.next(); // the '@'
                read_wrapped_body(cur, heap, ctx, "unquote-splicing", start)
            } else {
                read_wrapped_body(cur, heap, ctx, "unquote", start)
            }
        }
        Some('"') => read_string(cur, heap),
        Some('#') => read_hash(cur, heap, ctx),
        Some(_) => {
            let start = cur.loc();
            let v = read_atom(cur, heap, ctx.ns)?;
            // `:dyn Trait` (the trait-object type) is written as two
            // whitespace-separated words, so the reader joins them into the
            // single datum `(:dyn Trait)` — exactly the treatment `'x` gets.
            // Every type position (parameter/field pairs, return types, `(fn
            // ...)` types) can then keep its "a type is one `Value`"
            // assumption unchanged; `types::parse_type` recognizes the
            // resulting list. Inside a generic argument the same spelling is
            // handled a level down, by `extend_angle_token` + the type
            // parser, since there is no datum boundary there at all.
            if matches!(v, Value::Symbol(id) if id.is(wk::DYN)) {
                skip_ws_comments(cur, heap, ctx)?;
                if matches!(cur.peek(), None | Some(')')) {
                    return Err(Error::ReadError("`:dyn` must be followed by a trait name".to_string()));
                }
                return read_wrapped_body(cur, heap, ctx, ":dyn", start);
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
fn read_wrapped(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>, head: &str) -> Result<Value, Error> {
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
fn read_wrapped_body(cur: &mut Cursor, heap: &mut Heap, ctx: Ctx<'_>, head: &str, start: Loc) -> Result<Value, Error> {
    let head_loc = start.clone().with_end(cur.line, cur.col); // the consumed prefix
    let (d, d_loc) = read_datum_spanned(cur, heap, ctx)?;
    let form_loc = start.with_end(cur.line, cur.col);
    heap.push_root(d);
    let q = heap.intern_symbol(head); // symbols are permanent; no rooting needed
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
    Some(cur)
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
                let (e, elem_loc) = match read_datum_spanned(cur, heap, Ctx { ns: body_ns, ..ctx }) {
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
                other => {
                    return Err(Error::ReadError(format!(
                        "#{} must be followed by `r` (the radix macro `#NNrDIGITS`), not {:?}",
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

/// The integer after a radix macro (`#x-1f`, `#b101`, `#36rZZ`). The sign
/// comes *after* the marker, which is where CL's printer puts it.
///
/// Past `i32`'s range it reads as a `bignum`, the same fixnum-or-bignum split
/// [`parse_number`] makes for decimal tokens.
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
    if body.is_empty() || !body.chars().all(|c| c.is_digit(radix)) {
        return Err(Error::ReadError(format!(
            "`{}` is not a base-{} integer",
            tok, radix
        )));
    }
    let n = BigInt::parse_bytes(body.as_bytes(), radix).expect("digits of this radix parse as BigInt");
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

fn read_atom(cur: &mut Cursor, heap: &mut Heap, ns: NsId) -> Result<Value, Error> {
    let mut tok = String::new();
    // `<>` nesting, so an unterminated generic type token can be extended past
    // the whitespace that would otherwise end it (see `extend_angle_token`).
    // A `<` at the *start* of a token never opens a bracket: that's the
    // comparison operator `<`/`<=`, not a generic argument list. A `>` at
    // depth 0 likewise stays an ordinary character (`->`, `string>`).
    let mut depth: i32 = 0;
    while let Some(c) = cur.peek() {
        if is_delimiter(c) {
            break;
        }
        if c == '<' && !tok.is_empty() {
            depth += 1;
        } else if c == '>' && depth > 0 {
            depth -= 1;
        }
        tok.push(c);
        cur.next();
    }
    if depth > 0 {
        extend_angle_token(cur, &mut tok, depth);
    }
    // tok is non-empty: read_datum only dispatches here on a non-delimiter.
    validate_keyword(&tok)?;
    if let Some(v) = parse_number(heap, &tok)? {
        return Ok(v);
    }
    match tok.to_lowercase().as_str() {
        "true" => Ok(Value::Bool(true)),
        "false" => Ok(Value::Bool(false)),
        _ => {
            // A `::`-qualified token becomes a path of interned symbol segments.
        // A leading `::` (e.g. `::foo`) encodes an absolute path: the first
        // segment is the empty string, which the checker treats as "from root".
            if let Some(parts) = split_path_top_level(&tok) {
                // Interior empty segments (e.g. `foo::::bar`) are malformed.
                if parts[1..].iter().any(|p| p.is_empty()) {
                    return Err(Error::ReadError(format!("malformed path: {}", tok)));
                }
                // A written path's segments stay plain root names: which
                // module `m::foo` means is a resolution question (`m` may be
                // relative to wherever this is written), and the reader does
                // not resolve. Only *bare* symbols, whose module is lexical,
                // are placed in one.
                let mut segs: Vec<SymRef> = Vec::with_capacity(parts.len());
                for p in &parts {
                    if let Value::Symbol(id) = heap.intern_symbol(p) {
                        segs.push(id);
                    }
                }
                return Ok(heap.intern_path(&segs));
            }
            Ok(heap.intern_symbol_in(ns, &tok))
        }
    }
}

/// Continue a token whose `<...>` are still unbalanced, letting whitespace
/// through so a multi-word type argument reads as the single symbol the type
/// parser expects — `vector<:dyn drawable>` is one token, not the three
/// (`vector<:dyn`, `drawable>`) plain whitespace-delimited tokenizing would
/// give. `types::parse_qualified_generic` then re-splits it on the `NameTok::
/// Space` its lexer now emits.
///
/// **Speculative, with full rewind.** If the brackets don't close before a
/// newline or a hard delimiter (paren / string / quote / comment), both the
/// cursor and the token are restored and the caller keeps the ordinary short
/// token. That rewind is what keeps every pre-existing spelling reading
/// exactly as before, without needing to enumerate the tokens that legally
/// contain a `<`: `(string< a b)` starts an extension after `string<`, hits
/// the `)`, and rewinds; so does `(a<b c)`. Only a genuine generic type
/// token — balanced, on one line, with no parens inside — survives.
fn extend_angle_token(cur: &mut Cursor, tok: &mut String, mut depth: i32) {
    let mark = cur.mark();
    let base_len = tok.len();
    while depth > 0 {
        match cur.peek() {
            // `()` — the unit type as a generic argument, as in
            // `result<(), file-error>`. The only paren spelling admitted
            // here, and only as the adjacent pair: a lone paren still ends
            // the speculation (and rewinds), which is what keeps `(a<b c)`
            // and friends reading as before. `NameTok::Unit` picks the pair
            // back out on the type-parser side.
            Some('(') if cur.peek2() == Some(')') => {
                tok.push_str("()");
                cur.next();
                cur.next();
            }
            None | Some('\n') | Some('\r') | Some('(') | Some(')') | Some(']') | Some('}') | Some('"') | Some('\'') | Some('`') | Some(';') => {
                cur.reset(mark);
                tok.truncate(base_len);
                return;
            }
            Some(c) => {
                if c == '<' {
                    depth += 1;
                } else if c == '>' {
                    depth -= 1;
                }
                tok.push(c);
                cur.next();
            }
        }
    }
}

/// Reject malformed keyword tokens. A keyword is a token starting with a
/// single `:` (CL's self-evaluating `:name`, `Checker::check_inner`'s
/// `Value::Symbol` case) — it carries no package/path structure, so any
/// further `:` in it is a mistake rather than a path separator.
///
/// The leading-`::` exclusion is load-bearing: `::foo` is the *absolute path*
/// syntax (`split_path_top_level` turns it into a `Value::Path` whose first
/// segment is empty, "from root"), not a keyword. Only a lone leading `:`
/// starts a keyword.
fn validate_keyword(tok: &str) -> Result<(), Error> {
    if !tok.starts_with(':') || tok.starts_with("::") {
        return Ok(());
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
    Ok(())
}

/// Split a token on `::` occurring at `<>` nesting depth 0, so that
/// `geometry::Point` and `geometry::Vec<T>` split but `Vec<a::b>` does not.
/// Returns `None` when there is no top-level `::`.
fn split_path_top_level(tok: &str) -> Option<Vec<&str>> {
    let mut depth: i32 = 0;
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut found = false;
    let mut lex = NameLexer::new(tok);
    while let Some(t) = lex.next() {
        match t {
            NameTok::Lt => depth += 1,
            NameTok::Gt => depth -= 1,
            NameTok::ColonColon if depth == 0 => {
                let sep_end = lex.pos(); // just past this "::"
                parts.push(&tok[start..sep_end - 2]);
                start = sep_end;
                found = true;
            }
            _ => {}
        }
    }
    if !found {
        return None;
    }
    parts.push(&tok[start..]);
    Some(parts)
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

    // ratio: `numer/denom`, both all-digit (CL ratio syntax, decimal only).
    // Normalized on read exactly as CL specifies: `4/2` *is* the integer `2`
    // (and `2/4` is `1/2`) — an integer-valued ratio literal reads as an
    // `Int`/`bignum`, never a denominator-1 `ratio` value.
    if let Some((numer, denom)) = body.split_once('/') {
        if !numer.is_empty()
            && !denom.is_empty()
            && numer.chars().all(|c| c.is_ascii_digit())
            && denom.chars().all(|c| c.is_ascii_digit())
        {
            let n = BigInt::parse_bytes(numer.as_bytes(), 10).expect("all-digit numerator parses as BigInt");
            let d = BigInt::parse_bytes(denom.as_bytes(), 10).expect("all-digit denominator parses as BigInt");
            if d == BigInt::from(0) {
                return Err(Error::ReadError(format!("ratio literal with zero denominator: {}", tok)));
            }
            let r = BigRational::new(if neg { -n } else { n }, d);
            if r.is_integer() {
                let i = r.to_integer();
                return Ok(Some(int_or_bignum(heap, i)));
            }
            return Ok(Some(heap.alloc_ratio(r)));
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
