//! The pretty printer (CLHS §22.2 — the Common Lisp Pretty Printer, i.e. the
//! system R. Waters described as XP in *"XP: A Common Lisp Pretty Printing
//! System"*, MIT AI Memo 1102a, 1989).
//!
//! ## Why this is a two-pass layout rather than XP's streaming one
//!
//! XP wraps a *real* output stream and decides each line break with a bounded
//! lookahead (one line's worth of buffered characters), which is what makes it
//! linear-time and constant-memory over an unbounded stream. typelisp's
//! `format` already builds its result as an in-memory `String`
//! ([`crate::eval::format`]), and every printing entry point goes through it,
//! so the whole document is available before any line break has to be chosen.
//! That removes the only reason for the lookahead machinery: instead of asking
//! "has the buffer overflowed before the section ended?", this module can ask
//! the question XP is *approximating* — "does this section fit?" — directly.
//! The observable layout is the same; see `docs/dev/TODO.md`'s T5 notes, which
//! sanction exactly this simplification.
//!
//! ## The document model
//!
//! [`Out`] is a plain text buffer with pretty-printer [`Op`]s anchored at byte
//! offsets in it. `format`'s directive interpreter writes text into it exactly
//! as it wrote into a `String` before, and the pretty directives (`~_`, `~i`,
//! `~<…~:>`, `~:t`, and the `~a`/`~s`/`~w` pretty path) additionally record an
//! `Op`. When no `Op` was recorded — the overwhelmingly common case, and
//! always the case with `*print-pretty*` false — [`layout`] is skipped
//! entirely and the text is returned untouched.
//!
//! ## The break rules (CLHS `pprint-newline`)
//!
//! - `:mandatory` — always breaks.
//! - `:linear` — breaks iff the immediately containing logical block does not
//!   fit on one line. Because the test is per *block* (not per section), all
//!   the `:linear` newlines of one block break together, which is CLHS's
//!   "if a line break is inserted by any conditional newline in a logical
//!   block, all the linear-style conditional newlines in that block are also
//!   broken" rule, and what makes `pprint-linear` print either everything on
//!   one line or one element per line.
//! - `:fill` — breaks iff (a) the following section does not fit on the rest
//!   of the current line, (b) the preceding section of the same block was not
//!   printed on one line, or (c) miser style is in effect and the block does
//!   not fit.
//! - `:miser` — `:linear`, but only when miser style is in effect (the block
//!   started within `*print-miser-width*` columns of the right margin).

use std::collections::HashMap;

use typelisp_mem::{Heap, SymId, Value};

use crate::format::{Pre, RenderCtx, Renderer};

/// The kinds of conditional newline `pprint-newline` (and `~_`) can emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewlineKind {
    Linear,
    Fill,
    Miser,
    Mandatory,
}

/// `pprint-indent`'s two origins: the column the block started at (`:block`)
/// or the column output has reached (`:current`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndentKind {
    Block,
    Current,
}

/// `pprint-tab`'s four flavors (`~t` / `~:t` / `~@t` / `~:@t` inside a block).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    Line,
    Section,
    LineRelative,
    SectionRelative,
}

/// One pretty-printer instruction anchored in an [`Out`].
#[derive(Clone, Debug)]
pub enum Op {
    /// Opens a logical block. `prefix`/`suffix` are emitted by [`layout`]
    /// (not held in the text) so that a block whose prefix lands at a
    /// different column than it was built at still measures correctly. When
    /// `per_line` is set, `prefix` is re-emitted at the start of every line of
    /// the block instead of being indented past.
    BlockStart { prefix: String, per_line: bool, suffix: String },
    BlockEnd,
    Newline(NewlineKind),
    Indent(IndentKind, i64),
    Tab { kind: TabKind, colnum: i64, colinc: i64 },
}

/// The `*print-pretty*` / `*print-right-margin*` / `*print-miser-width*`
/// triple, read out of the interpreter's globals once per printing operation
/// (`Interp::pretty_opts`). typelisp has no dynamic binding, so these are
/// ordinary assignable globals rather than CL's special variables.
#[derive(Clone, Copy, Debug)]
pub struct Opts {
    pub pretty: bool,
    pub margin: usize,
    /// `None` when `*print-miser-width*` is 0 or negative ("off", CL's `nil`).
    pub miser: Option<usize>,
    /// `*print-lines*`: stop after this many lines and mark the cut with
    /// `..`, CL's own marker. `None` is no limit (CL's `nil`), which is both
    /// the default and what 0 or a negative global means.
    pub lines: Option<usize>,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { pretty: false, margin: DEFAULT_MARGIN, miser: None, lines: None }
    }
}

/// Cuts `text` to `opts.lines` lines, marking the cut with CL's `..`.
///
/// CLHS 22.1.1 puts the marker at the point the output was abandoned; here it
/// goes at the end of the last line kept, which is the same place for every
/// layout this printer produces (a line break is where the cut can fall).
/// Nothing to do when the limit is off or the text already fits.
fn cut_lines(text: String, opts: &Opts) -> String {
    let Some(limit) = opts.lines else { return text };
    if limit == 0 {
        return text;
    }
    let mut kept: Vec<&str> = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        if i == limit {
            let mut out = kept.join("\n");
            out.push_str(" ..");
            return out;
        }
        kept.push(line);
    }
    text
}

/// The `*print-right-margin*` default, matching the conventional 80-column
/// line every CL implementation falls back to when the stream can't report a
/// width.
pub const DEFAULT_MARGIN: usize = 80;

/// A text buffer plus the pretty-printer ops anchored inside it.
///
/// `format`'s interpreter treats this as its output `String` (it grew from
/// one): [`Out::push_str`]/[`Out::push`] append text, [`Out::op`] records an
/// instruction at the current end. Sub-results built in their own `Out`
/// (`~(…~)`'s case conversion, `~<…~>`'s justification segments) are spliced
/// back with [`Out::append`], which re-anchors their ops.
#[derive(Clone, Debug, Default)]
pub struct Out {
    pub text: String,
    ops: Vec<(usize, Op)>,
}

impl Out {
    pub fn new() -> Out {
        Out::default()
    }

    pub(crate) fn push_str(&mut self, s: &str) {
        self.text.push_str(s);
    }

    pub fn push(&mut self, c: char) {
        self.text.push(c);
    }

    pub fn op(&mut self, op: Op) {
        self.ops.push((self.text.len(), op));
    }

    /// Whether any pretty-printer op was recorded — when false the text is
    /// already the final output and [`layout`] is skipped.
    pub fn is_plain(&self) -> bool {
        self.ops.is_empty()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty() && self.ops.is_empty()
    }

    pub(crate) fn ends_with(&self, c: char) -> bool {
        self.text.ends_with(c)
    }

    /// Appends `other`, shifting its op anchors by the current text length.
    pub fn append(&mut self, other: Out) {
        let base = self.text.len();
        self.text.push_str(&other.text);
        self.ops.extend(other.ops.into_iter().map(|(at, op)| (at + base, op)));
    }

    /// Rewrites the text piecewise (the pieces being the runs between op
    /// anchors) so that op anchoring survives a transformation — `~(…~)`'s
    /// case conversion, whose replacement text need not have the same byte
    /// length as the original. `f` is threaded a mutable state value so a
    /// transformation that depends on preceding text (capitalization's
    /// word-boundary tracking) still sees the pieces as one stream.
    pub(crate) fn map_text<S>(&mut self, mut state: S, mut f: impl FnMut(&str, &mut S) -> String) {
        let mut text = String::with_capacity(self.text.len());
        let mut prev = 0usize;
        for (at, _) in self.ops.iter_mut() {
            let piece = f(&self.text[prev..*at], &mut state);
            prev = *at;
            text.push_str(&piece);
            *at = text.len();
        }
        text.push_str(&f(&self.text[prev..], &mut state));
        self.text = text;
    }
}

impl From<String> for Out {
    fn from(text: String) -> Out {
        Out { text, ops: Vec::new() }
    }
}

// ===========================================================================
// Layout
// ===========================================================================

/// One element of the flattened document the layout pass walks.
enum Item<'a> {
    Text(&'a str),
    Op(&'a Op),
    /// A literal newline that was embedded in the text (`~%`, `\n` in a
    /// string). Inside a logical block CL treats these as mandatory line
    /// breaks, so the block's indentation is restored after them.
    Hard,
}

/// Per-open-block layout state.
struct Block {
    /// Column the block's prefix started at — `pprint-indent :block`'s origin
    /// and the section-relative tab origin.
    start_col: usize,
    /// Column continuation lines of this block indent to.
    indent: usize,
    /// Literal text emitted at the start of every continuation line (the
    /// enclosing blocks' per-line prefixes, plus this block's own when it was
    /// opened with one).
    line_prefix: String,
    suffix: String,
    /// Whether the whole block fits on one line from where it started.
    fits: bool,
    /// Miser style is in effect for this block.
    miser: bool,
    /// A break has been emitted since this block's previous top-level
    /// conditional newline — CLHS's `:fill` rule (b) reads this.
    section_broke: bool,
    /// Carried from the previous section, since `section_broke` is reset at
    /// each of the block's own top-level conditional newlines.
    prev_section_broke: bool,
}

/// Lays `doc` out, starting at column `start_col`, under `opts`.
pub fn layout(doc: &Out, start_col: usize, opts: &Opts) -> String {
    cut_lines(layout_uncut(doc, start_col, opts), opts)
}

/// [`layout`] before `*print-lines*` is applied.
fn layout_uncut(doc: &Out, start_col: usize, opts: &Opts) -> String {
    let items = flatten(doc);
    let margin = if opts.margin == 0 { usize::MAX } else { opts.margin };
    let metrics = Metrics::new(&items);
    let ends = block_ends(&items);
    let sections = section_ends(&items, &ends);

    let mut out = String::with_capacity(doc.text.len() + 16);
    let mut col = start_col;
    // The whole document behaves as one implicit outermost logical block, so
    // ops that appear outside any `~<…~:>` still have a block to act on.
    let mut blocks = vec![Block {
        start_col,
        indent: start_col,
        line_prefix: String::new(),
        suffix: String::new(),
        fits: metrics.fits(0, items.len(), start_col, margin),
        miser: miser_style(start_col, margin, opts),
        section_broke: false,
        prev_section_broke: false,
    }];

    for (i, item) in items.iter().enumerate() {
        match item {
            Item::Text(t) => {
                out.push_str(t);
                col += width(t);
            }
            Item::Hard => {
                break_line(&mut out, &mut col, &blocks);
                note_break(&mut blocks);
            }
            Item::Op(Op::BlockStart { prefix, per_line, suffix }) => {
                let start = col;
                out.push_str(prefix);
                col += width(prefix);
                let parent = blocks.last().expect("the implicit outer block is never popped");
                let line_prefix = if *per_line {
                    let mut p = parent.line_prefix.clone();
                    p.push_str(prefix);
                    p
                } else {
                    parent.line_prefix.clone()
                };
                let end = ends.get(&i).copied().unwrap_or(items.len());
                blocks.push(Block {
                    start_col: start,
                    indent: col.max(width(&line_prefix)),
                    line_prefix,
                    suffix: suffix.clone(),
                    fits: metrics.fits(i, end.min(items.len()) + 1, start, margin),
                    miser: miser_style(start, margin, opts),
                    section_broke: false,
                    prev_section_broke: false,
                });
            }
            Item::Op(Op::BlockEnd) => {
                // A break taken inside this block already marked every
                // enclosing block (`note_break` walks the whole stack), so
                // closing one needs nothing beyond its suffix.
                if blocks.len() > 1 {
                    let b = blocks.pop().expect("checked len");
                    out.push_str(&b.suffix);
                    col += width(&b.suffix);
                }
            }
            Item::Op(Op::Newline(kind)) => {
                let b = blocks.last().expect("the implicit outer block is never popped");
                // Whether the section just ending needed a break *inside* it —
                // `:fill`'s rule (b). A break taken at this newline itself is
                // the boundary, not part of the section that precedes it, so
                // it must not be folded in here (doing so makes one `:fill`
                // break cascade into "one element per line").
                let section_broke = b.section_broke;
                let broke = match kind {
                    NewlineKind::Mandatory => true,
                    NewlineKind::Linear => !b.fits,
                    NewlineKind::Miser => b.miser && !b.fits,
                    NewlineKind::Fill => {
                        (b.miser && !b.fits) || b.prev_section_broke || {
                            let end = sections.get(&i).copied().unwrap_or(items.len());
                            let (from, at) = skip_tab(&items, i + 1, col, b.indent);
                            !metrics.fits(from, end, at, margin)
                        }
                    }
                };
                if broke {
                    break_line(&mut out, &mut col, &blocks);
                    note_break(&mut blocks);
                }
                // This newline closes one section of its own block.
                let b = blocks.last_mut().expect("the implicit outer block is never popped");
                b.prev_section_broke = section_broke;
                b.section_broke = false;
            }
            Item::Op(Op::Indent(kind, n)) => {
                let b = blocks.last_mut().expect("the implicit outer block is never popped");
                let origin = match kind {
                    IndentKind::Block => b.start_col,
                    IndentKind::Current => col,
                };
                b.indent = (origin as i64 + *n).max(0) as usize;
            }
            Item::Op(Op::Tab { kind, colnum, colinc }) => {
                let b = blocks.last().expect("the implicit outer block is never popped");
                // A section-relative tab counts from where the block's content
                // starts on the line — its indentation column — so a column
                // grid survives the line breaks (this is what makes
                // `pprint-tabular` line its columns up after a break).
                let origin = match kind {
                    TabKind::Line | TabKind::LineRelative => 0,
                    TabKind::Section | TabKind::SectionRelative => b.indent,
                };
                let relative = matches!(kind, TabKind::LineRelative | TabKind::SectionRelative);
                let target = tab_target(col, origin, *colnum, *colinc, relative);
                for _ in col..target {
                    out.push(' ');
                }
                col = col.max(target);
            }
        }
    }
    // Close any block the caller left open (a malformed `~<…~:>` can't reach
    // here — `format` matches those at parse time — but the user-callable
    // `pprint-logical-block` layer can).
    while blocks.len() > 1 {
        let b = blocks.pop().expect("checked len");
        out.push_str(&b.suffix);
    }
    out
}

/// A tab immediately after a conditional newline decides where the next
/// section really starts (`pprint-tabular` emits exactly that pair), so the
/// "does the next section fit" measurement has to step over it and start from
/// the column it lands on — [`Metrics`] charges every tab zero width, having
/// no columns to work from. Returns the item index to measure from and the
/// column to measure at.
fn skip_tab(items: &[Item<'_>], from: usize, col: usize, indent: usize) -> (usize, usize) {
    if let Some(Item::Op(Op::Tab { kind, colnum, colinc })) = items.get(from) {
        let origin = match kind {
            TabKind::Line | TabKind::LineRelative => 0,
            TabKind::Section | TabKind::SectionRelative => indent,
        };
        let relative = matches!(kind, TabKind::LineRelative | TabKind::SectionRelative);
        return (from + 1, col.max(tab_target(col, origin, *colnum, *colinc, relative)));
    }
    (from, col)
}

/// Miser style: on iff `*print-miser-width*` is set and the block starts
/// within that many columns of the right margin.
fn miser_style(start_col: usize, margin: usize, opts: &Opts) -> bool {
    match opts.miser {
        Some(w) => margin.saturating_sub(start_col) <= w,
        None => false,
    }
}

/// Emits a line break: trailing blanks on the finished line are dropped (as
/// XP does, so a `" "` written before a conditional newline doesn't leave the
/// line ragged), then the innermost block's per-line prefix and indentation
/// start the new one.
fn break_line(out: &mut String, col: &mut usize, blocks: &[Block]) {
    while out.ends_with(' ') {
        out.pop();
    }
    out.push('\n');
    let b = blocks.last().expect("the implicit outer block is never popped");
    out.push_str(&b.line_prefix);
    let mut c = width(&b.line_prefix);
    while c < b.indent {
        out.push(' ');
        c += 1;
    }
    *col = c;
}

/// Records that a break happened inside every currently open block, so their
/// `:fill` newlines see rule (b) ("the preceding section was not printed on
/// one line").
fn note_break(blocks: &mut [Block]) {
    for b in blocks.iter_mut() {
        b.section_broke = true;
    }
}

/// Where a `pprint-tab` lands (CLHS `pprint-tab` / `~T`). A *relative* tab
/// advances `colnum` columns from the current one and then to the next column
/// that is a multiple of `colinc` counting from `origin`; an *absolute* tab
/// goes to `origin + colnum`, or — when output is already past that — on in
/// `colinc` steps until it reaches the current column.
fn tab_target(col: usize, origin: usize, colnum: i64, colinc: i64, relative: bool) -> usize {
    let colnum = colnum.max(0) as usize;
    let colinc = colinc.max(0) as usize;
    if relative {
        let mut target = col + colnum;
        if colinc > 1 {
            while target.saturating_sub(origin) % colinc != 0 {
                target += 1;
            }
        }
        return target;
    }
    let mut target = origin + colnum;
    if target < col {
        if colinc == 0 {
            return col;
        }
        while target < col {
            target += colinc;
        }
    }
    target
}

fn width(s: &str) -> usize {
    s.chars().count()
}

/// Splits `doc`'s text at its op anchors (and at embedded newlines) into the
/// flat item sequence the layout pass and the measurements work over.
fn flatten(doc: &Out) -> Vec<Item<'_>> {
    let mut items = Vec::with_capacity(doc.ops.len() * 2 + 1);
    let mut prev = 0usize;
    for (at, op) in &doc.ops {
        push_split(&mut items, &doc.text[prev..*at]);
        items.push(Item::Op(op));
        prev = *at;
    }
    push_split(&mut items, &doc.text[prev..]);
    items
}

/// Pushes `s` as text items, turning embedded newlines into [`Item::Hard`].
fn push_split<'a>(items: &mut Vec<Item<'a>>, s: &'a str) {
    if s.is_empty() {
        return;
    }
    let mut rest = s;
    while let Some(i) = rest.find('\n') {
        if i > 0 {
            items.push(Item::Text(&rest[..i]));
        }
        items.push(Item::Hard);
        rest = &rest[i + 1..];
    }
    if !rest.is_empty() {
        items.push(Item::Text(rest));
    }
}

/// Cumulative flat widths (what the items would occupy with no break taken)
/// and cumulative hard-break counts, so any range's "does it fit" question is
/// two subtractions.
struct Metrics {
    width: Vec<usize>,
    hard: Vec<usize>,
}

impl Metrics {
    fn new(items: &[Item<'_>]) -> Metrics {
        let mut width = Vec::with_capacity(items.len() + 1);
        let mut hard = Vec::with_capacity(items.len() + 1);
        let (mut w, mut h) = (0usize, 0usize);
        // A block's suffix is declared on its `BlockStart`, so its width is
        // charged at the matching `BlockEnd` off this stack.
        let mut suffixes: Vec<usize> = Vec::new();
        width.push(0);
        hard.push(0);
        for item in items {
            match item {
                Item::Text(t) => w += self::width(t),
                Item::Hard => h += 1,
                Item::Op(Op::BlockStart { prefix, suffix, .. }) => {
                    w += self::width(prefix);
                    suffixes.push(self::width(suffix));
                }
                Item::Op(Op::BlockEnd) => w += suffixes.pop().unwrap_or(0),
                Item::Op(Op::Newline(NewlineKind::Mandatory)) => h += 1,
                Item::Op(_) => {}
            }
            width.push(w);
            hard.push(h);
        }
        Metrics { width, hard }
    }

    /// Whether items `[from, to)` laid out flat from column `col` stay inside
    /// `margin`. A mandatory break in the range means "no" — the range cannot
    /// be one line at all.
    fn fits(&self, from: usize, to: usize, col: usize, margin: usize) -> bool {
        let to = to.min(self.width.len() - 1);
        if from >= to {
            return col <= margin;
        }
        if self.hard[to] > self.hard[from] {
            return false;
        }
        col.saturating_add(self.width[to] - self.width[from]) <= margin
    }
}

/// A `BlockEnd` for each `BlockStart`, by item index. An unmatched
/// `BlockStart` is simply absent (its block runs to the end of the document).
fn block_ends(items: &[Item<'_>]) -> HashMap<usize, usize> {
    let mut ends = HashMap::new();
    let mut open: Vec<usize> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        match item {
            Item::Op(Op::BlockStart { .. }) => open.push(i),
            Item::Op(Op::BlockEnd) => {
                if let Some(start) = open.pop() {
                    ends.insert(start, i);
                }
            }
            _ => {}
        }
    }
    ends
}

/// For each conditional newline, the item index its *section* ends at: the
/// next conditional newline at the same block depth, or the end of the
/// enclosing block.
fn section_ends(items: &[Item<'_>], ends: &HashMap<usize, usize>) -> HashMap<usize, usize> {
    let mut sections = HashMap::new();
    // Per open block, the index of the last top-level conditional newline seen
    // (still awaiting its section end).
    let mut pending: Vec<Option<usize>> = vec![None];
    let mut opened: Vec<usize> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        match item {
            Item::Op(Op::BlockStart { .. }) => {
                pending.push(None);
                opened.push(i);
            }
            Item::Op(Op::BlockEnd) => {
                if pending.len() > 1 {
                    if let Some(prev) = pending.pop().expect("checked len") {
                        sections.insert(prev, i);
                    }
                    opened.pop();
                }
            }
            Item::Op(Op::Newline(_)) => {
                if let Some(slot) = pending.last_mut() {
                    if let Some(prev) = slot.replace(i) {
                        sections.insert(prev, i);
                    }
                }
            }
            // A hard break ends the section before it, but is not itself a
            // conditional newline awaiting a section end of its own.
            Item::Hard => {
                if let Some(slot) = pending.last_mut() {
                    if let Some(prev) = slot.take() {
                        sections.insert(prev, i);
                    }
                }
            }
            _ => {}
        }
    }
    // Blocks still open at the end of the document run to the document's end.
    for (depth, slot) in pending.iter().enumerate() {
        if let Some(prev) = slot {
            let end = if depth == 0 {
                items.len()
            } else {
                opened.get(depth - 1).and_then(|s| ends.get(s)).copied().unwrap_or(items.len())
            };
            sections.insert(*prev, end);
        }
    }
    sections
}

// ===========================================================================
// The default layout for `Sexpr` data
// ===========================================================================

/// How a list is laid out by [`render`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// The default `*print-pprint-dispatch*` behavior: word-wrap ordinary
    /// lists (`Fill`), but lay code-shaped forms out with their head and
    /// distinguished arguments on the first line (see [`code_style`]).
    Default,
    /// `pprint-fill`: as many elements per line as fit.
    Fill,
    /// `pprint-linear`: all elements on one line, or one per line.
    Linear,
    /// `pprint-tabular`: elements in columns `colinc` wide.
    Tabular(i64),
}

/// Renders `v` into `out` with pretty-printer ops, honoring `style` for lists.
/// Atoms are rendered exactly as `~a`/`~s` render them (`standard` selects the
/// `prin1` reader syntax), so the only difference pretty printing makes is
/// where the line breaks fall.
pub fn render(
    heap: &mut Heap,
    ctx: RenderCtx<'_>,
    v: Value,
    standard: bool,
    style: Style,
    out: &mut Out,
) -> Result<(), String> {
    let mut st = Renderer::new(heap, ctx, v);
    render_at(heap, ctx, &mut st, v, standard, style, 0, out)
}

/// [`render`] one level in: `depth` feeds `*print-level*` and `st` carries the
/// `*print-circle*` labels, both of which must span the whole object rather
/// than restart per sublist — hence one shared [`Renderer`] threaded through
/// instead of a fresh one per call.
fn render_at(
    heap: &mut Heap,
    ctx: RenderCtx<'_>,
    st: &mut Renderer,
    v: Value,
    standard: bool,
    style: Style,
    depth: usize,
    out: &mut Out,
) -> Result<(), String> {
    // Anything that is not a list has no layout of its own — including a
    // struct/enum with a `print-object` method, whose own text is whatever
    // that method returns (`Renderer::render` performs the dispatch, and
    // applies the print_vars to whatever it walks into).
    if !matches!(v, Value::Cons(_)) {
        let mut s = String::new();
        st.render(heap, ctx, v, standard, depth, &mut s)?;
        out.push_str(&s);
        return Ok(());
    }
    match st.pre(heap, ctx, v, depth) {
        Pre::Stop(text) => {
            out.push_str(&text);
            return Ok(());
        }
        Pre::Go(prefix) => out.push_str(&prefix),
    }
    // `(quote x)` prints as `'x`, as CL's default dispatch table does.
    if let Some(inner) = quote_form(heap, v) {
        out.push('\'');
        return render_at(heap, ctx, st, inner, standard, style, depth + 1, out);
    }
    // `*print-length*` cuts the element list here, once, so every layout below
    // sees the already-shortened list plus a flag for the trailing `...`.
    // (A cut list has no dotted tail left to print, as in CL.)
    let (mut elems, mut tail) = list_items(heap, v);
    let mut cut = false;
    if let Some(limit) = ctx.print_vars.cuts().1 {
        // Only the *elements* count against the limit — a dotted tail that is
        // still within it prints as usual (`(1 2 . 3)` under a limit of 2),
        // matching what `Renderer::render`'s flat cons loop does. A tail past
        // the cut disappears with the elements it followed.
        if elems.len() > limit {
            elems.truncate(limit);
            tail = None;
            cut = true;
        }
    }
    let items = (elems, tail);
    match style {
        Style::Tabular(colinc) => render_tabular(heap, ctx, st, &items, standard, colinc, depth, cut, out),
        Style::Linear => render_seq(heap, ctx, st, &items, standard, style, NewlineKind::Linear, depth, cut, out),
        Style::Fill => render_seq(heap, ctx, st, &items, standard, style, NewlineKind::Fill, depth, cut, out),
        Style::Default => match code_style(&items) {
            Some(distinguished) => render_code(heap, ctx, st, &items, standard, distinguished, depth, cut, out),
            None => render_seq(heap, ctx, st, &items, standard, style, NewlineKind::Fill, depth, cut, out),
        },
    }
}

/// `(quote x)` — a two-element list whose head is the symbol `quote`.
fn quote_form(heap: &Heap, v: Value) -> Option<Value> {
    let head = heap.car(v).ok()?;
    let Value::Symbol(id) = head else { return None };
    if id != SymId::QUOTE {
        return None;
    }
    let rest = heap.cdr(v).ok()?;
    let inner = heap.car(rest).ok()?;
    matches!(heap.cdr(rest).ok()?, Value::Empty).then_some(inner)
}

/// The elements of a (possibly dotted) list, plus its improper tail.
fn list_items(heap: &Heap, v: Value) -> (Vec<Value>, Option<Value>) {
    let mut elems = Vec::new();
    let mut cur = v;
    loop {
        match cur {
            Value::Cons(_) => {
                elems.push(heap.car(cur).expect("cons car"));
                cur = heap.cdr(cur).expect("cons cdr");
            }
            Value::Empty => return (elems, None),
            other => return (elems, Some(other)),
        }
    }
}

type Items = (Vec<Value>, Option<Value>);

/// `pprint-linear`/`pprint-fill`: `(e1 e2 …)` with a conditional newline of
/// `kind` between elements.
#[allow(clippy::too_many_arguments)]
fn render_seq(
    heap: &mut Heap,
    ctx: RenderCtx<'_>,
    st: &mut Renderer,
    (elems, tail): &Items,
    standard: bool,
    style: Style,
    kind: NewlineKind,
    depth: usize,
    cut: bool,
    out: &mut Out,
) -> Result<(), String> {
    out.op(Op::BlockStart { prefix: "(".to_string(), per_line: false, suffix: ")".to_string() });
    for (i, e) in elems.iter().enumerate() {
        if i > 0 {
            out.push(' ');
            out.op(Op::Newline(kind));
        }
        render_at(heap, ctx, st, *e, standard, style, depth + 1, out)?;
    }
    if let Some(t) = tail {
        out.push_str(" . ");
        render_at(heap, ctx, st, *t, standard, style, depth + 1, out)?;
    }
    push_ellipsis(cut, !elems.is_empty(), kind, out);
    out.op(Op::BlockEnd);
    Ok(())
}

/// The `...` that stands for the elements `*print-length*` cut, on the same
/// conditional-newline footing as a real element so it wraps with them.
fn push_ellipsis(cut: bool, after_elems: bool, kind: NewlineKind, out: &mut Out) {
    if !cut {
        return;
    }
    if after_elems {
        out.push(' ');
        out.op(Op::Newline(kind));
    }
    out.push_str("...");
}

/// `pprint-tabular`: elements laid out in columns `colinc` wide, wrapping when
/// the next column would overflow.
#[allow(clippy::too_many_arguments)]
fn render_tabular(
    heap: &mut Heap,
    ctx: RenderCtx<'_>,
    st: &mut Renderer,
    (elems, tail): &Items,
    standard: bool,
    colinc: i64,
    depth: usize,
    cut: bool,
    out: &mut Out,
) -> Result<(), String> {
    let colinc = if colinc <= 0 { 1 } else { colinc };
    out.op(Op::BlockStart { prefix: "(".to_string(), per_line: false, suffix: ")".to_string() });
    for (i, e) in elems.iter().enumerate() {
        if i > 0 {
            out.push(' ');
            out.op(Op::Newline(NewlineKind::Fill));
            out.op(Op::Tab { kind: TabKind::SectionRelative, colnum: 0, colinc });
        }
        render_at(heap, ctx, st, *e, standard, Style::Fill, depth + 1, out)?;
    }
    if let Some(t) = tail {
        out.push_str(" . ");
        render_at(heap, ctx, st, *t, standard, Style::Fill, depth + 1, out)?;
    }
    push_ellipsis(cut, !elems.is_empty(), NewlineKind::Fill, out);
    out.op(Op::BlockEnd);
    Ok(())
}

/// How many arguments after the head of a code-shaped form stay on the head's
/// line before the body is indented under it — the analogue of CL's default
/// `*print-pprint-dispatch*` entries for `quote`/`let`/`defun`/… , spelled for
/// typelisp's own special forms and definition forms.
fn code_style((elems, tail): &Items) -> Option<usize> {
    if tail.is_some() || elems.is_empty() {
        return None;
    }
    let Value::Symbol(id) = elems[0] else { return None };
    // Identity on the head symbol, not its name: a form is recognized the way
    // CL's own `*print-pprint-dispatch*` recognizes one, by `eq`.
    Some(match id {
        SymId::PROGN | SymId::COND | SymId::LOOP | SymId::AND | SymId::OR | SymId::LIST | SymId::BLOCK => 0,
        SymId::IF
        | SymId::WHEN
        | SymId::UNLESS
        | SymId::WHILE
        | SymId::LET
        | SymId::LET_STAR
        | SymId::MATCH
        | SymId::CASE
        | SymId::SETF
        | SymId::MODULE
        | SymId::DEFSTRUCT
        | SymId::DEFENUM
        | SymId::DEFTRAIT
        | SymId::DEFTYPE
        | SymId::THE
        | SymId::AS
        | SymId::DOLIST
        | SymId::DOTIMES
        | SymId::DOITER
        | SymId::UNTIL => 1,
        SymId::LAMBDA | SymId::DEFMACRO | SymId::IMPL | SymId::LABELS | SymId::DO => 2,
        SymId::DEFVAR | SymId::DEFCONSTANT => 2,
        SymId::DEFUN => 3,
        SymId::DEFMETHOD => 4,
        _ => return None,
    })
}

/// A code-shaped form: `(head d1 … dn` on one line, then the remaining
/// subforms one per line indented two columns past the head's own column.
#[allow(clippy::too_many_arguments)]
fn render_code(
    heap: &mut Heap,
    ctx: RenderCtx<'_>,
    st: &mut Renderer,
    (elems, _): &Items,
    standard: bool,
    distinguished: usize,
    depth: usize,
    cut: bool,
    out: &mut Out,
) -> Result<(), String> {
    out.op(Op::BlockStart { prefix: "(".to_string(), per_line: false, suffix: ")".to_string() });
    // The body indents relative to the block, not to the (variable-width) head.
    out.op(Op::Indent(IndentKind::Block, 2));
    for (i, e) in elems.iter().enumerate() {
        if i > 0 {
            out.push(' ');
            // Distinguished arguments fill onto the head's line; everything
            // after them is one subform per line whenever the form is broken.
            out.op(Op::Newline(if i <= distinguished { NewlineKind::Fill } else { NewlineKind::Linear }));
        }
        render_at(heap, ctx, st, *e, standard, Style::Default, depth + 1, out)?;
    }
    push_ellipsis(cut, !elems.is_empty(), NewlineKind::Linear, out);
    out.op(Op::BlockEnd);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lay(build: impl FnOnce(&mut Out), margin: usize) -> String {
        let mut out = Out::new();
        build(&mut out);
        layout(&out, 0, &Opts { pretty: true, margin, miser: None, lines: None })
    }

    fn block(out: &mut Out, kind: NewlineKind, elems: &[&str]) {
        out.op(Op::BlockStart { prefix: "(".into(), per_line: false, suffix: ")".into() });
        for (i, e) in elems.iter().enumerate() {
            if i > 0 {
                out.push(' ');
                out.op(Op::Newline(kind));
            }
            out.push_str(e);
        }
        out.op(Op::BlockEnd);
    }

    #[test]
    fn linear_keeps_everything_on_one_line_when_it_fits() {
        assert_eq!(lay(|o| block(o, NewlineKind::Linear, &["1", "2", "3"]), 40), "(1 2 3)");
    }

    #[test]
    fn linear_breaks_every_newline_together() {
        // The classic `pprint-linear` guarantee: one line, or one element per
        // line — never a partially filled line.
        assert_eq!(lay(|o| block(o, NewlineKind::Linear, &["11", "2", "3"]), 6), "(11\n 2\n 3)");
    }

    #[test]
    fn fill_wraps_like_words() {
        assert_eq!(lay(|o| block(o, NewlineKind::Fill, &["11", "2", "3"]), 6), "(11 2\n 3)");
    }

    #[test]
    fn mandatory_always_breaks() {
        assert_eq!(lay(|o| block(o, NewlineKind::Mandatory, &["1", "2"]), 80), "(1\n 2)");
    }

    #[test]
    fn miser_only_fires_near_the_margin() {
        let mut out = Out::new();
        block(&mut out, NewlineKind::Miser, &["11", "2", "3"]);
        // Miser off: the newlines are ignored even though the block overflows.
        assert_eq!(layout(&out, 0, &Opts { pretty: true, margin: 6, miser: None, lines: None }), "(11 2 3)");
        // Miser on (the block starts within 6 columns of the margin): breaks.
        assert_eq!(layout(&out, 0, &Opts { pretty: true, margin: 6, miser: Some(6), lines: None }), "(11\n 2\n 3)");
    }

    #[test]
    fn indent_moves_continuation_lines() {
        let out = {
            let mut o = Out::new();
            o.op(Op::BlockStart { prefix: "(".into(), per_line: false, suffix: ")".into() });
            o.op(Op::Indent(IndentKind::Block, 4));
            o.push_str("aa ");
            o.op(Op::Newline(NewlineKind::Mandatory));
            o.push_str("bb");
            o.op(Op::BlockEnd);
            o
        };
        assert_eq!(layout(&out, 0, &Opts { pretty: true, margin: 80, miser: None, lines: None }), "(aa\n    bb)");
    }

    #[test]
    fn per_line_prefix_starts_every_line() {
        let out = {
            let mut o = Out::new();
            o.op(Op::BlockStart { prefix: ";; ".into(), per_line: true, suffix: "".into() });
            o.push_str("one");
            o.op(Op::Newline(NewlineKind::Mandatory));
            o.push_str("two");
            o.op(Op::BlockEnd);
            o
        };
        assert_eq!(layout(&out, 0, &Opts { pretty: true, margin: 80, miser: None, lines: None }), ";; one\n;; two");
    }

    #[test]
    fn nested_blocks_break_independently() {
        let out = {
            let mut o = Out::new();
            o.op(Op::BlockStart { prefix: "(".into(), per_line: false, suffix: ")".into() });
            o.push_str("outer ");
            o.op(Op::Newline(NewlineKind::Linear));
            block(&mut o, NewlineKind::Linear, &["a", "b"]);
            o.op(Op::BlockEnd);
            o
        };
        // Wide: everything on one line.
        assert_eq!(layout(&out, 0, &Opts { pretty: true, margin: 80, miser: None, lines: None }), "(outer (a b))");
        // Narrow: the outer block breaks, the inner one still fits.
        assert_eq!(layout(&out, 0, &Opts { pretty: true, margin: 12, miser: None, lines: None }), "(outer\n (a b))");
    }

    #[test]
    fn trailing_blanks_are_dropped_at_a_break() {
        assert_eq!(lay(|o| block(o, NewlineKind::Linear, &["aaaa", "b"]), 4), "(aaaa\n b)");
    }

    #[test]
    fn a_plain_document_is_returned_untouched() {
        let out: Out = "no ops here\n".to_string().into();
        assert!(out.is_plain());
    }
}
