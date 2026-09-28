//! The Common Lisp `format` directive engine (CLHS §22.3), shared by the
//! `format`/`print`/`println` special forms (`typelisp::check::checker`) via
//! `Interp::run_format`. A control string is parsed once into a [`Node`] tree
//! (so block directives — `~[…~]`, `~{…~}`, `~<…~>`, `~(…~)` — and their
//! clause separators nest properly) and then interpreted against the argument
//! list.
//!
//! Arguments arrive as a `Sexpr` list whose elements the checker wrapped into
//! their `Sexpr` encodings (`Checker::cons_hetero_sexpr`), so every element is
//! a [`Value`] rendered by [`render_value`]. The list is materialized into a
//! `Vec<Value>` up front so the argument-jumping directives (`~*`, `~:*`,
//! `~@*`, `~:P`) can move the cursor freely.
//!
//! ## Coverage
//!
//! Implemented: `~A ~S ~W`, `~D ~B ~O ~X ~R`, `~P`, `~C`, `~F ~E ~G ~$`,
//! `~% ~& ~| ~~`, `~( ~)`, `~[ ~; ~]`, `~{ ~} ~^`, `~< ~> ~T`, `~* ~?`, the
//! ignored-`~newline`, the pretty-printer directives `~_ ~I ~:T` and the
//! logical-block form of `~<…~:>`, plus the numeric/`'c`/`v`/`#` prefix
//! parameters and the `:`/`@` modifiers each directive gives meaning to.
//!
//! The pretty-printer directives (and the `*print-pretty*` path of
//! `~A`/`~S`/`~W`) record [`Op`]s on the output buffer rather than emitting
//! text directly; [`crate::pprint`] turns the finished buffer into laid
//! out text. With `*print-pretty*` false nothing records an op and the buffer
//! is returned exactly as before.
//!
//! Deliberately unsupported (return a clear error): `~/name/` function-call
//! dispatch — typelisp has no runtime function-by-name lookup with `format`'s
//! calling convention. The `nil`-specific behavior of CL's `~:A`/`~@[` is
//! adapted to typelisp's `false`, which has no `nil`.

use std::collections::HashMap;

use typelisp_mem::{FloatBox, Heap, Value};

use crate::pprint::{self, IndentKind, NewlineKind, Op, Opts, Out, Style, TabKind};
use crate::PrintEnv;

/// Entry point: interpret `control` against the `Sexpr` argument list `args`,
/// returning the buffer it produced (text plus any pretty-printer ops — see
/// [`finish`], which turns one into text). Errors (bad directive, too few
/// arguments, …) are `String`s the caller turns into a recoverable
/// `EvalError::Panic`.
pub fn build(
    heap: &mut Heap,
    ctx: RenderCtx<'_>,
    control: &str,
    args: Value,
    opts: &Opts,
) -> Result<Out, String> {
    let nodes = parse(control)?;
    let items = list_to_vec(heap, args);
    let mut st = State { heap, ctx, args: items, pos: 0, opts: *opts };
    let mut out = Out::new();
    st.interp_seq(&nodes, &mut out)?;
    Ok(out)
}

/// What the value renderers need besides the heap.
///
/// `enums` is the variant-name table `~a`/`~s` need for a boxed enum. `interp`
/// is the running interpreter, present exactly when a *program* is printing
/// (as opposed to a unit test or an embedder rendering a value directly): it
/// is what lets a value whose type implements the `print-object` trait be
/// rendered by its own method instead of the built-in `#<name field…>` form.
/// With `interp: None` no dispatch is attempted and every value renders the
/// built-in way.
///
/// `print_vars` is the snapshot of the CLHS 22.1.1 printer control variables
/// this printing operation runs under (see [`PrintVars`]).
///
/// Copyable (one reference and three scalars) so it can be threaded alongside
/// the `&mut Heap` the dispatch needs without fighting the borrow checker.
#[derive(Clone, Copy)]
pub struct RenderCtx<'a> {
    /// What the printer needs to know about the program — enum variant names
    /// and `print-object` methods. See [`PrintEnv`].
    pub env: &'a dyn PrintEnv,
    pub print_vars: PrintVars,
}

/// The CLHS 22.1.1 printer control variables that decide *what* a value
/// renders as, as opposed to how it is laid out (that is [`Opts`]).
///
/// CL spells "no limit" as `nil`; typelisp has no `nil`, so the prelude's
/// globals are `i64`s where 0 or less means unlimited — the same convention
/// `*print-right-margin*`/`*print-miser-width*` already use. [`Default`] is
/// every limit off, base 10, and symbols as stored: CL's initial state, and
/// what a bare `Interp` (no prelude loaded, as in some unit tests) falls back
/// to.
#[derive(Clone, Copy)]
pub struct PrintVars {
    /// `*print-circle*`: label shared and circular substructure with `#n=` /
    /// `#n#` instead of following it forever.
    pub circle: bool,
    /// `*print-level*`: nested objects at this depth or deeper print as `#`.
    /// The object handed to the printer is at depth 0.
    pub level: Option<usize>,
    /// `*print-length*`: at most this many elements/fields per list, struct or
    /// enum; the rest collapse to `...`.
    pub length: Option<usize>,
    /// `*print-base*`: the radix integers print in. Kept as the raw `i64` the
    /// global holds rather than a validated `u32`, because CL requires 2..=36
    /// and this snapshot is taken where there is no way to report the
    /// violation — [`PrintVars::radix_of`] validates at the point of use,
    /// where the renderer can return an error.
    pub base: i64,
    /// `*print-radix*`: prefix the digits with the radix marker (`#b`/`#o`/
    /// `#x`/`#NNr`, or a trailing `.` in base 10), so the printed form reads
    /// back as the same number whatever `*read-base*` is.
    pub radix: bool,
    /// `*print-case*`: how a symbol's name is rendered. CL stores symbol names
    /// upcased and defaults to `:upcase`; this language's reader stores them
    /// downcased, so the default is [`PrintCase::Downcase`] — both mean "as
    /// stored".
    pub case: PrintCase,
    /// `*print-readably*`: print so the result reads back as an equal object.
    /// Here that means escapes on and the `*print-level*`/`*print-length*`
    /// cuts off, which is what CL says it overrides. CL additionally signals
    /// `print-not-readable` for values with no readable form; this language
    /// has nothing to signal (no conditions) and no way to decide the question
    /// for a user type whose `print-object` may print anything, so that half
    /// is deliberately absent — see `docs/ja/reference/functions/printing.md` §6.2.
    pub readably: bool,
}

impl Default for PrintVars {
    fn default() -> Self {
        PrintVars {
            circle: false,
            level: None,
            length: None,
            base: 10,
            radix: false,
            case: PrintCase::Downcase,
            readably: false,
        }
    }
}

/// `*print-case*`'s three values, written in typelisp as CL's own keywords
/// `:upcase` / `:downcase` / `:capitalize` — self-evaluating `symbol`s whose
/// interned name keeps the colon, not a type of their own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrintCase {
    Upcase,
    Downcase,
    Capitalize,
}

impl PrintVars {
    /// The validated `*print-base*`, or the message CL's "must be between 2
    /// and 36" restriction deserves. Checked here rather than where the
    /// snapshot is taken, because only the renderer has an error to return.
    pub fn radix_of(&self) -> Result<u32, String> {
        match self.base {
            b if (2..=36).contains(&b) => Ok(b as u32),
            b => Err(format!("*print-base* must be between 2 and 36, got {}", b)),
        }
    }

    /// `*print-level*`/`*print-length*` as this printing operation should
    /// apply them: both off under `*print-readably*`, which CL says overrides
    /// them so the output can be read back whole.
    pub fn cuts(&self) -> (Option<usize>, Option<usize>) {
        if self.readably {
            (None, None)
        } else {
            (self.level, self.length)
        }
    }

    /// A symbol name rendered under `*print-case*`. Never applied under
    /// `*print-readably*`: changing the case would change which symbol reads
    /// back (this language's reader folds to lower case, so an upcased name
    /// still reads back equal — but `:capitalize` on a name containing a
    /// digit does not round-trip in CL either, and printing readably is not
    /// the place to find out).
    pub fn render_symbol(&self, name: &str) -> String {
        if self.readably {
            return name.to_string();
        }
        match self.case {
            PrintCase::Downcase => name.to_string(),
            PrintCase::Upcase => name.to_ascii_uppercase(),
            PrintCase::Capitalize => name
                .split_inclusive(|c: char| !c.is_ascii_alphanumeric())
                .map(|word| {
                    let mut cs = word.chars();
                    match cs.next() {
                        Some(first) => {
                            first.to_ascii_uppercase().to_string()
                                + &cs.as_str().to_ascii_lowercase()
                        }
                        None => String::new(),
                    }
                })
                .collect(),
        }
    }
}

/// Turns a finished buffer into text: a buffer with no pretty-printer op in it
/// is already the answer, so the layout pass only runs when one was recorded.
pub fn finish(out: Out, opts: &Opts) -> String {
    if out.is_plain() {
        out.text
    } else {
        pprint::layout(&out, 0, opts)
    }
}

/// Flattens a proper `Sexpr` list into its elements (an improper dotted tail is
/// ignored — `format` arguments are always proper lists).
fn list_to_vec(heap: &Heap, mut v: Value) -> Vec<Value> {
    let mut out = Vec::new();
    while let Value::Cons(_) = v {
        out.push(heap.car(v).expect("cons car"));
        v = heap.cdr(v).expect("cons cdr");
    }
    out
}

// ===========================================================================
// Parsing
// ===========================================================================

/// A directive prefix parameter (`~mincol,'padchar…`). `Default` is an omitted
/// slot (`~,3D`'s first parameter); `Arg`/`Count` are the `v`/`#` parameters
/// resolved from the argument list at interpretation time.
#[derive(Clone, Debug)]
enum Param {
    Int(i64),
    Char(char),
    Arg,
    Count,
    Default,
}

/// A parsed directive's shared header: its prefix parameters and `:`/`@`
/// modifiers.
#[derive(Clone, Debug, Default)]
struct Head {
    params: Vec<Param>,
    colon: bool,
    at: bool,
}

#[derive(Debug)]
enum Node {
    Text(String),
    /// A non-block directive, e.g. `~A`, `~5,'0D`, `~%`.
    Dir { head: Head, ch: char },
    /// `~(…~)` — case conversion applied to the enclosed output.
    Case { head: Head, body: Vec<Node> },
    /// `~[…~;…~]` — conditional; `default` is the index of the `~:;` clause.
    Cond { head: Head, clauses: Vec<Vec<Node>>, default: Option<usize> },
    /// `~{…~}` — iteration; `close_colon` is the `~:}` "at least once" flag.
    Iter { head: Head, body: Vec<Node>, close_colon: bool },
    /// `~<…~;…~>` — justification across segments.
    Just { head: Head, segments: Vec<Vec<Node>> },
    /// `~<…~;…~:>` — a *logical block* (the closing directive carries `:`),
    /// an entirely different directive from the justification above despite
    /// the shared opener. `sep_at` records, per `~;` separator, whether it was
    /// written `~@;` — which marks the preceding prefix segment as a
    /// *per-line* prefix.
    Block { head: Head, segments: Vec<Vec<Node>>, sep_at: Vec<bool> },
    /// `~/name/` — CL's function-call directive. `name` is resolved against
    /// the argument's own type at render time; see [`crate::PrintEnv::format_call`].
    Call { head: Head, name: String },
    /// `~^` — escape upward out of the nearest `~{`/`~<` (or the whole op).
    Escape { head: Head },
}

/// The control directive a [`parse_seq`] run stopped on (a block closer or a
/// `~;` separator), with its own modifiers/parameters.
struct Stop {
    ch: char,
    head: Head,
}

fn parse(control: &str) -> Result<Vec<Node>, String> {
    let chars: Vec<char> = control.chars().collect();
    let mut pos = 0;
    let (nodes, stop) = parse_seq(&chars, &mut pos, &[])?;
    if let Some(s) = stop {
        return Err(format!("format: unmatched ~{}", s.ch));
    }
    Ok(nodes)
}

/// Every `~/name/` directive `control` contains, in source order.
///
/// The compile-time half of [`Node::Call`]'s dispatch. `format`/`print`/
/// `println` take a *literal* control string, so the checker can scan it with
/// this and learn which methods the directive can reach before the program
/// runs — which is what lets an AOT executable register exactly those
/// (`crate::aot`), and what turns a misspelled name into a check error rather
/// than a failure in the middle of printing.
///
/// `Err` is the control string not parsing at all, which the checker reports
/// the same way: a literal that the engine could never render is worth
/// hearing about at the call site rather than at the call.
pub fn call_directive_names(control: &str) -> Result<Vec<String>, String> {
    fn walk(nodes: &[Node], out: &mut Vec<String>) {
        for n in nodes {
            match n {
                Node::Call { name, .. } => out.push(name.clone()),
                Node::Case { body, .. } | Node::Iter { body, .. } => walk(body, out),
                Node::Cond { clauses, .. } => {
                    for c in clauses {
                        walk(c, out);
                    }
                }
                Node::Just { segments, .. } | Node::Block { segments, .. } => {
                    for seg in segments {
                        walk(seg, out);
                    }
                }
                Node::Text(_) | Node::Dir { .. } | Node::Escape { .. } => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&parse(control)?, &mut out);
    Ok(out)
}

/// Parses a run of nodes until end-of-string or a directive whose char is in
/// `stops` (a block closer or `;`), which it returns as the [`Stop`].
fn parse_seq(chars: &[char], pos: &mut usize, stops: &[char]) -> Result<(Vec<Node>, Option<Stop>), String> {
    let mut nodes = Vec::new();
    let mut text = String::new();
    while *pos < chars.len() {
        let c = chars[*pos];
        if c != '~' {
            text.push(c);
            *pos += 1;
            continue;
        }
        // Flush accumulated literal text before the directive.
        if !text.is_empty() {
            nodes.push(Node::Text(std::mem::take(&mut text)));
        }
        *pos += 1; // consume '~'
        let head = parse_head(chars, pos)?;
        let Some(&ch) = chars.get(*pos) else {
            return Err("format: control string ends with a lone ~".to_string());
        };
        *pos += 1;
        let lower = ch.to_ascii_lowercase();
        if stops.contains(&lower) {
            return Ok((nodes, Some(Stop { ch: lower, head })));
        }
        match lower {
            '(' => {
                let (body, stop) = parse_seq(chars, pos, &[')'])?;
                match stop {
                    Some(s) if s.ch == ')' => nodes.push(Node::Case { head, body }),
                    _ => return Err("format: unterminated ~(".to_string()),
                }
            }
            '[' => nodes.push(parse_cond(chars, pos, head)?),
            '{' => {
                let (body, stop) = parse_seq(chars, pos, &['}'])?;
                match stop {
                    Some(s) if s.ch == '}' => nodes.push(Node::Iter { head, body, close_colon: s.head.colon }),
                    _ => return Err("format: unterminated ~{".to_string()),
                }
            }
            '<' => nodes.push(parse_just(chars, pos, head)?),
            '/' => {
                // The name runs to the closing `/`. Case-folded because that
                // is how the reader interns every name the lookup will match
                // against.
                let mut name = String::new();
                loop {
                    match chars.get(*pos) {
                        None => return Err("format: unterminated ~/ — the name needs a closing `/`".to_string()),
                        Some('/') => {
                            *pos += 1;
                            break;
                        }
                        Some(c) => {
                            name.push(*c);
                            *pos += 1;
                        }
                    }
                }
                if name.is_empty() {
                    return Err("format: ~// names no method".to_string());
                }
                nodes.push(Node::Call { head, name: name.to_ascii_lowercase() });
            }
            '^' => nodes.push(Node::Escape { head }),
            '\n' => {
                // `~<newline>`: by default ignore the newline and the following
                // whitespace; `~:` keeps the whitespace, `~@` keeps the newline.
                if head.at {
                    text.push('\n');
                }
                if !head.colon {
                    while matches!(chars.get(*pos), Some(c) if *c == ' ' || *c == '\t') {
                        *pos += 1;
                    }
                }
            }
            _ => nodes.push(Node::Dir { head, ch: lower }),
        }
    }
    if !text.is_empty() {
        nodes.push(Node::Text(text));
    }
    Ok((nodes, None))
}

/// `~[…~;…~]`: clauses separated by `~;`; a `~:;` separator marks the clause
/// that follows it as the default (used when the selector is out of range).
fn parse_cond(chars: &[char], pos: &mut usize, head: Head) -> Result<Node, String> {
    let mut clauses = Vec::new();
    let mut default = None;
    loop {
        let (body, stop) = parse_seq(chars, pos, &[';', ']'])?;
        // A `~:;` separator preceded this clause -> it is the default clause.
        clauses.push(body);
        match stop {
            Some(s) if s.ch == ';' => {
                if s.head.colon {
                    default = Some(clauses.len()); // the *next* clause
                }
            }
            Some(s) if s.ch == ']' => break,
            _ => return Err("format: unterminated ~[".to_string()),
        }
    }
    Ok(Node::Cond { head, clauses, default })
}

/// `~<…~;…~>`: segments separated by `~;`. A closing `~:>` (rather than `~>`)
/// makes the whole directive a logical block instead of a justification.
fn parse_just(chars: &[char], pos: &mut usize, head: Head) -> Result<Node, String> {
    let mut segments = Vec::new();
    let mut sep_at = Vec::new();
    loop {
        let (body, stop) = parse_seq(chars, pos, &[';', '>'])?;
        segments.push(body);
        match stop {
            Some(s) if s.ch == ';' => {
                sep_at.push(s.head.at);
                continue;
            }
            Some(s) if s.ch == '>' => {
                if s.head.colon {
                    return Ok(Node::Block { head, segments, sep_at });
                }
                break;
            }
            _ => return Err("format: unterminated ~<".to_string()),
        }
    }
    Ok(Node::Just { head, segments })
}

/// The literal text of a `~<…~:>` prefix/suffix segment. CL requires these to
/// be literal (they are re-emitted at the start of every line of a per-line
/// block, and their width is what the block indents past), so a directive
/// inside one is an error rather than something to interpret per use.
fn literal_segment(nodes: &[Node]) -> Result<String, String> {
    let mut s = String::new();
    for n in nodes {
        match n {
            Node::Text(t) => s.push_str(t),
            _ => {
                return Err(
                    "format: the prefix/suffix segments of a ~<…~:> logical block must be literal text".to_string()
                )
            }
        }
    }
    Ok(s)
}

/// Parses a directive header — its comma-separated prefix parameters followed
/// by any `:`/`@` modifiers.
fn parse_head(chars: &[char], pos: &mut usize) -> Result<Head, String> {
    let mut head = Head::default();
    // Parameters only begin if the next char looks like one (or a comma, for a
    // leading omitted slot such as `~,3D`).
    let starts = matches!(
        chars.get(*pos),
        Some(c) if c.is_ascii_digit() || matches!(c, '-' | '+' | '\'' | 'v' | 'V' | '#' | ',')
    );
    if starts {
        loop {
            match chars.get(*pos) {
                Some('\'') => {
                    let ch = *chars.get(*pos + 1).ok_or("format: ~' parameter needs a character")?;
                    head.params.push(Param::Char(ch));
                    *pos += 2;
                }
                Some('v') | Some('V') => {
                    head.params.push(Param::Arg);
                    *pos += 1;
                }
                Some('#') => {
                    head.params.push(Param::Count);
                    *pos += 1;
                }
                Some(c) if c.is_ascii_digit() || *c == '-' || *c == '+' => {
                    let mut s = String::new();
                    if *c == '-' || *c == '+' {
                        s.push(*c);
                        *pos += 1;
                    }
                    while matches!(chars.get(*pos), Some(d) if d.is_ascii_digit()) {
                        s.push(chars[*pos]);
                        *pos += 1;
                    }
                    let n = s.parse::<i64>().map_err(|_| "format: bad numeric parameter".to_string())?;
                    head.params.push(Param::Int(n));
                }
                _ => head.params.push(Param::Default),
            }
            if chars.get(*pos) == Some(&',') {
                *pos += 1;
                continue;
            }
            break;
        }
    }
    loop {
        match chars.get(*pos) {
            Some(':') => {
                head.colon = true;
                *pos += 1;
            }
            Some('@') => {
                head.at = true;
                *pos += 1;
            }
            _ => break,
        }
    }
    Ok(head)
}

// ===========================================================================
// Interpretation
// ===========================================================================

/// The outcome of interpreting a node sequence: whether a `~^` fired.
enum Flow {
    Normal,
    Escape,
}

struct State<'a> {
    heap: &'a mut Heap,
    ctx: RenderCtx<'a>,
    args: Vec<Value>,
    pos: usize,
    /// The `*print-pretty*`/`*print-right-margin*`/`*print-miser-width*`
    /// snapshot this operation runs under. Only `pretty` is consulted here
    /// (it gates whether a directive records an [`Op`] or falls back to its
    /// non-pretty behavior); the margins are used by the layout pass.
    opts: Opts,
}

impl State<'_> {
    fn remaining(&self) -> usize {
        self.args.len().saturating_sub(self.pos)
    }

    fn next_arg(&mut self) -> Result<Value, String> {
        let v = *self
            .args
            .get(self.pos)
            .ok_or("format: ran out of arguments for the control string")?;
        self.pos += 1;
        Ok(v)
    }

    fn peek_arg(&self) -> Option<Value> {
        self.args.get(self.pos).copied()
    }

    /// Resolves a directive's prefix parameters to concrete integers, filling
    /// omitted/`Default` slots with `defaults` (positionally) and reading the
    /// `v`/`#` parameters from the argument stream / remaining count. A
    /// `'c` character parameter yields its code point.
    fn resolve_params(&mut self, params: &[Param], defaults: &[Option<i64>]) -> Result<Vec<Option<i64>>, String> {
        let mut out = Vec::new();
        for (i, p) in params.iter().enumerate() {
            let v = match p {
                Param::Int(n) => Some(*n),
                Param::Char(c) => Some(*c as i64),
                Param::Count => Some(self.remaining() as i64),
                Param::Arg => Some(self.param_from_arg()?),
                Param::Default => defaults.get(i).copied().flatten(),
            };
            out.push(v);
        }
        // Pad with defaults for parameters the user omitted entirely.
        for d in defaults.iter().skip(params.len()) {
            out.push(*d);
        }
        Ok(out)
    }

    /// A `v` parameter consumes one argument, which must be an integer or a
    /// character (its code point).
    fn param_from_arg(&mut self) -> Result<i64, String> {
        let arg = self.next_arg()?;
        match arg {
            Value::Char(c) => Ok(c as i64),
            _ => int_of(self.heap, arg)
                .ok_or_else(|| "format: a 'v' parameter requires an integer or character argument".to_string()),
        }
    }

    /// A directive's `'c`-style character parameter (padding character), taken
    /// from the resolved parameter at `idx` (an integer code point) or
    /// `default`.
    fn char_param(vals: &[Option<i64>], idx: usize, default: char) -> char {
        vals.get(idx)
            .copied()
            .flatten()
            .and_then(|n| char::from_u32(n as u32))
            .unwrap_or(default)
    }

    fn int_param(vals: &[Option<i64>], idx: usize, default: i64) -> i64 {
        vals.get(idx).copied().flatten().unwrap_or(default)
    }

    fn interp_seq(&mut self, nodes: &[Node], out: &mut Out) -> Result<Flow, String> {
        for node in nodes {
            match node {
                Node::Text(t) => out.push_str(t),
                Node::Escape { head } => {
                    if self.escape_fires(head)? {
                        return Ok(Flow::Escape);
                    }
                }
                Node::Case { head, body } => {
                    let mut inner = Out::new();
                    let flow = self.interp_seq(body, &mut inner)?;
                    apply_case(&mut inner, head.colon, head.at);
                    out.append(inner);
                    if matches!(flow, Flow::Escape) {
                        return Ok(Flow::Escape);
                    }
                }
                Node::Block { head, segments, sep_at } => {
                    self.interp_block(head, segments, sep_at, out)?;
                }
                Node::Cond { head, clauses, default } => {
                    if matches!(self.interp_cond(head, clauses, *default, out)?, Flow::Escape) {
                        return Ok(Flow::Escape);
                    }
                }
                Node::Iter { head, body, close_colon } => {
                    self.interp_iter(head, body, *close_colon, out)?;
                }
                Node::Just { head, segments } => {
                    self.interp_just(head, segments, out)?;
                }
                Node::Dir { head, ch } => {
                    self.interp_dir(head, *ch, out)?;
                }
                Node::Call { head, name } => {
                    let v = self.next_arg()?;
                    let text = self.ctx.env.format_call(self.heap, name, v, head.colon, head.at)?;
                    out.push_str(&text);
                }
            }
        }
        Ok(Flow::Normal)
    }

    /// `~^` fires when its guard says the block is exhausted. With no
    /// parameters that means "no arguments remain"; `~n^` fires when `n` is
    /// zero; `~n,m^` when `n == m`; `~n,m,o^` when `n <= m <= o`.
    fn escape_fires(&mut self, head: &Head) -> Result<bool, String> {
        let vals = self.resolve_params(&head.params, &[])?;
        let present: Vec<i64> = vals.iter().filter_map(|v| *v).collect();
        Ok(match present.len() {
            0 => self.remaining() == 0,
            1 => present[0] == 0,
            2 => present[0] == present[1],
            _ => present[0] <= present[1] && present[1] <= present[2],
        })
    }

    fn interp_cond(
        &mut self,
        head: &Head,
        clauses: &[Vec<Node>],
        default: Option<usize>,
        out: &mut Out,
    ) -> Result<Flow, String> {
        if head.at {
            // `~@[`: if the next arg is logically true, process the single
            // clause *without* consuming the arg (the clause consumes it);
            // otherwise consume it and output nothing.
            let truthy = match self.peek_arg() {
                Some(Value::Bool(false)) | None => false,
                _ => true,
            };
            if truthy {
                if let Some(clause) = clauses.first() {
                    return self.interp_seq(clause, out);
                }
            } else {
                let _ = self.next_arg();
            }
            return Ok(Flow::Normal);
        }
        if head.colon {
            // `~:[false~;true~]`: choose by a boolean argument.
            let b = match self.next_arg()? {
                Value::Bool(b) => b,
                _ => true, // any non-bool is "true" (generalized boolean)
            };
            let idx = if b { 1 } else { 0 };
            if let Some(clause) = clauses.get(idx) {
                return self.interp_seq(clause, out);
            }
            return Ok(Flow::Normal);
        }
        // Plain `~[`: an integer argument selects the clause by index.
        let arg = self.next_arg()?;
        let Some(sel) = int_of(self.heap, arg) else {
            return Err("format: ~[ requires an integer argument".to_string());
        };
        let chosen = if sel >= 0 && (sel as usize) < clauses.len() {
            // A default clause (if any) is not selectable by index.
            let idx = sel as usize;
            if Some(idx) == default {
                None
            } else {
                Some(idx)
            }
        } else {
            None
        };
        let idx = chosen.or(default);
        if let Some(i) = idx {
            if let Some(clause) = clauses.get(i) {
                return self.interp_seq(clause, out);
            }
        }
        Ok(Flow::Normal)
    }

    fn interp_iter(
        &mut self,
        head: &Head,
        body: &[Node],
        close_colon: bool,
        out: &mut Out,
    ) -> Result<(), String> {
        let vals = self.resolve_params(&head.params, &[])?;
        let max = Self::int_param(&vals, 0, -1); // -1 = unbounded

        // The body may be empty (`~{~}`), meaning the *argument* supplies the
        // control string. Parsing it lazily per use keeps that rare case cheap.
        let run_body = |st: &mut State, sublist: Vec<Value>, out: &mut Out| -> Result<Flow, String> {
            if body.is_empty() {
                return Err("format: ~{~} (empty-body indirection) needs a control-string argument; \
                            write the directives inside the braces instead"
                    .to_string());
            }
            let saved = std::mem::replace(&mut st.args, sublist);
            let saved_pos = std::mem::replace(&mut st.pos, 0);
            let flow = st.interp_seq(body, out);
            st.args = saved;
            st.pos = saved_pos;
            flow
        };

        match (head.colon, head.at) {
            // `~{`: one list argument; iterate its elements.
            (false, false) => {
                let list = self.next_arg()?;
                let elems = list_to_vec(self.heap, list);
                self.iterate_flat(&elems, body, max, close_colon, out)?;
            }
            // `~@{`: iterate over the remaining arguments in place.
            (false, true) => {
                let elems: Vec<Value> = self.args[self.pos..].to_vec();
                let consumed = self.iterate_flat(&elems, body, max, close_colon, out)?;
                self.pos += consumed;
            }
            // `~:{`: one list of sublists; each iteration binds one sublist.
            (true, false) => {
                let list = self.next_arg()?;
                let sublists = list_to_vec(self.heap, list);
                let mut count = 0;
                for (i, sub) in sublists.iter().enumerate() {
                    if max >= 0 && count >= max {
                        break;
                    }
                    if i == 0 || !sublists.is_empty() {
                        let elems = list_to_vec(self.heap, *sub);
                        if matches!(run_body(self, elems, out)?, Flow::Escape) {
                            break;
                        }
                        count += 1;
                    }
                }
                if sublists.is_empty() && close_colon {
                    let _ = run_body(self, Vec::new(), out)?;
                }
            }
            // `~:@{`: remaining arguments, each a sublist.
            (true, true) => {
                let sublists: Vec<Value> = self.args[self.pos..].to_vec();
                self.pos = self.args.len();
                let mut count = 0;
                for sub in &sublists {
                    if max >= 0 && count >= max {
                        break;
                    }
                    let elems = list_to_vec(self.heap, *sub);
                    if matches!(run_body(self, elems, out)?, Flow::Escape) {
                        break;
                    }
                    count += 1;
                }
                if sublists.is_empty() && close_colon {
                    let _ = run_body(self, Vec::new(), out)?;
                }
            }
        }
        Ok(())
    }

    /// Runs `body` once per element of `elems` (as the fresh argument list),
    /// honoring `max` and the `~:}` "at least once" flag. Returns how many
    /// elements were consumed (for `~@{`'s in-place advance).
    fn iterate_flat(
        &mut self,
        elems: &[Value],
        body: &[Node],
        max: i64,
        close_colon: bool,
        out: &mut Out,
    ) -> Result<usize, String> {
        if body.is_empty() {
            return Err("format: ~{~} with an empty body is unsupported".to_string());
        }
        if elems.is_empty() && close_colon {
            let saved = std::mem::take(&mut self.args);
            let saved_pos = std::mem::replace(&mut self.pos, 0);
            let _ = self.interp_seq(body, out);
            self.args = saved;
            self.pos = saved_pos;
            return Ok(0);
        }
        let saved = std::mem::take(&mut self.args);
        let saved_pos = self.pos;
        self.args = elems.to_vec();
        self.pos = 0;
        let mut iterations = 0;
        while self.pos < self.args.len() {
            if max >= 0 && iterations >= max {
                break;
            }
            let flow = self.interp_seq(body, out)?;
            iterations += 1;
            if matches!(flow, Flow::Escape) {
                break;
            }
        }
        let consumed = self.pos;
        self.args = saved;
        self.pos = saved_pos;
        Ok(consumed)
    }

    fn interp_just(&mut self, head: &Head, segments: &[Vec<Node>], out: &mut Out) -> Result<(), String> {
        let vals = self.resolve_params(&head.params, &[Some(0), Some(1), Some(0)])?;
        let mincol = Self::int_param(&vals, 0, 0).max(0) as usize;
        let colinc = Self::int_param(&vals, 1, 1).max(1) as usize;
        let minpad = Self::int_param(&vals, 2, 0).max(0) as usize;
        let padchar = Self::char_param(&vals, 3, ' ');

        // Interpret each segment (they share the argument cursor). A `~^` in a
        // segment stops processing the remaining segments.
        let mut pieces: Vec<Out> = Vec::new();
        for seg in segments {
            let mut s = Out::new();
            let flow = self.interp_seq(seg, &mut s)?;
            pieces.push(s);
            if matches!(flow, Flow::Escape) {
                break;
            }
        }
        out.append(justify(pieces, mincol, colinc, minpad, padchar, head.colon, head.at));
        Ok(())
    }

    /// `~<…~;…~:>` — a logical block, not a justification (see [`Node::Block`]).
    ///
    /// The first segment, if the directive has more than one, is the block's
    /// prefix and the last is its suffix; both must be literal text (CL
    /// requires this too, since they are re-emitted at line starts). A prefix
    /// terminated by `~@;` rather than `~;` is a *per-line* prefix. `~:<`
    /// defaults the prefix/suffix to `(`/`)`, `~<` to empty. `~@<` runs the
    /// body against the remaining arguments in place; otherwise the block
    /// consumes one list argument that supplies them.
    ///
    /// With `*print-pretty*` false the block still runs — its prefix, body and
    /// suffix are emitted — but records no ops, so nothing can break.
    fn interp_block(
        &mut self,
        head: &Head,
        segments: &[Vec<Node>],
        sep_at: &[bool],
        out: &mut Out,
    ) -> Result<(), String> {
        let (default_prefix, default_suffix) = if head.colon { ("(", ")") } else { ("", "") };
        let (prefix, body, suffix, per_line) = match segments.len() {
            0 | 1 => (default_prefix.to_string(), segments.first(), default_suffix.to_string(), false),
            2 => (literal_segment(&segments[0])?, segments.get(1), default_suffix.to_string(), sep_at.first() == Some(&true)),
            _ => (
                literal_segment(&segments[0])?,
                segments.get(1),
                literal_segment(&segments[segments.len() - 1])?,
                sep_at.first() == Some(&true),
            ),
        };
        if self.opts.pretty {
            out.op(Op::BlockStart { prefix: prefix.clone(), per_line, suffix: suffix.clone() });
        } else {
            out.push_str(&prefix);
        }
        if let Some(body) = body {
            if head.at {
                // `~@<`: the body consumes the remaining arguments in place.
                self.interp_seq(body, out)?;
            } else {
                let list = self.next_arg()?;
                let sublist = list_to_vec(self.heap, list);
                let saved = std::mem::replace(&mut self.args, sublist);
                let saved_pos = std::mem::replace(&mut self.pos, 0);
                let r = self.interp_seq(body, out);
                self.args = saved;
                self.pos = saved_pos;
                r?;
            }
        }
        if self.opts.pretty {
            out.op(Op::BlockEnd);
        } else {
            out.push_str(&suffix);
        }
        Ok(())
    }

    fn interp_dir(&mut self, head: &Head, ch: char, out: &mut Out) -> Result<(), String> {
        match ch {
            'a' | 's' | 'w' => {
                let standard = ch != 'a';
                let vals = self.resolve_params(&head.params, &[Some(0), Some(1), Some(0)])?;
                let mincol = Self::int_param(&vals, 0, 0).max(0) as usize;
                let colinc = Self::int_param(&vals, 1, 1).max(1) as usize;
                let minpad = Self::int_param(&vals, 2, 0).max(0) as usize;
                let padchar = Self::char_param(&vals, 3, ' ');
                let arg = self.next_arg()?;
                // CL's `~A`/`~S`/`~W` all consult `*print-pretty*`. Padding
                // parameters and pretty layout can't both hold (padding fixes
                // a width the layout is free to change), so an explicitly
                // padded directive keeps the flat rendering.
                if self.opts.pretty && head.params.is_empty() {
                    pprint::render(self.heap, self.ctx, arg, standard, Style::Default, out)?;
                } else {
                    let mut s = String::new();
                    render_value(self.heap, self.ctx, arg, standard, &mut s)?;
                    out.push_str(&pad(&s, mincol, colinc, minpad, padchar, head.at));
                }
            }
            'd' | 'b' | 'o' | 'x' => {
                let radix = match ch {
                    'b' => 2,
                    'o' => 8,
                    'x' => 16,
                    _ => 10,
                };
                self.emit_radix(head, radix, out)?;
            }
            'r' => self.emit_r(head, out)?,
            'c' => {
                let arg = self.next_arg()?;
                let Value::Char(c) = arg else {
                    return Err("format: ~c requires a character argument".to_string());
                };
                out.push_str(&format_char(c, head.colon, head.at));
            }
            'p' => {
                if head.colon {
                    // Back up to reuse the previous argument.
                    self.pos = self.pos.saturating_sub(1);
                }
                let arg = self.next_arg()?;
                let n = match arg {
                    _ if int_of(self.heap, arg).is_some() => int_of(self.heap, arg).expect("just tested"),
                    Value::Boxed(id) if self.heap.is_bignum(id) => {
                        if self.heap.bignum_value(id).to_string() == "1" { 1 } else { 2 }
                    }
                    _ => 2,
                };
                out.push_str(if head.at {
                    if n == 1 { "y" } else { "ies" }
                } else if n == 1 {
                    ""
                } else {
                    "s"
                });
            }
            'f' => self.emit_f(head, out)?,
            'e' => self.emit_e(head, out)?,
            'g' => self.emit_g(head, out)?,
            '$' => self.emit_dollars(head, out)?,
            '%' => {
                let vals = self.resolve_params(&head.params, &[Some(1)])?;
                for _ in 0..Self::int_param(&vals, 0, 1).max(0) {
                    out.push('\n');
                }
            }
            '&' => {
                let vals = self.resolve_params(&head.params, &[Some(1)])?;
                let n = Self::int_param(&vals, 0, 1).max(0);
                if n > 0 {
                    // Fresh-line: the first newline only if not already at BOL.
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    for _ in 1..n {
                        out.push('\n');
                    }
                }
            }
            '|' => {
                let vals = self.resolve_params(&head.params, &[Some(1)])?;
                for _ in 0..Self::int_param(&vals, 0, 1).max(0) {
                    out.push('\u{000c}');
                }
            }
            '~' => {
                let vals = self.resolve_params(&head.params, &[Some(1)])?;
                for _ in 0..Self::int_param(&vals, 0, 1).max(0) {
                    out.push('~');
                }
            }
            't' => self.emit_tab(head, out)?,
            '*' => {
                let vals = self.resolve_params(&head.params, &[])?;
                if head.at {
                    // Absolute goto (default index 0).
                    let n = Self::int_param(&vals, 0, 0).max(0) as usize;
                    self.pos = n.min(self.args.len());
                } else if head.colon {
                    // Back up n (default 1).
                    let n = Self::int_param(&vals, 0, 1).max(0) as usize;
                    self.pos = self.pos.saturating_sub(n);
                } else {
                    // Skip n forward (default 1).
                    let n = Self::int_param(&vals, 0, 1).max(0) as usize;
                    self.pos = (self.pos + n).min(self.args.len());
                }
            }
            '?' => self.emit_indirection(head, out)?,
            // `~_` — a conditional newline (CL's `pprint-newline`). Plain is
            // `:linear`, `~:_` is `:fill`, `~@_` is `:miser`, `~:@_` is
            // `:mandatory`. Like every pretty directive it is a no-op when
            // `*print-pretty*` is false, exactly as CL's is.
            '_' => {
                if self.opts.pretty {
                    out.op(Op::Newline(match (head.colon, head.at) {
                        (false, false) => NewlineKind::Linear,
                        (true, false) => NewlineKind::Fill,
                        (false, true) => NewlineKind::Miser,
                        (true, true) => NewlineKind::Mandatory,
                    }));
                }
            }
            // `~nI` — `pprint-indent`: set the enclosing logical block's
            // indentation to `n` past the block's own column (`~n:I`: past the
            // current output column).
            'i' => {
                let vals = self.resolve_params(&head.params, &[Some(0)])?;
                let n = Self::int_param(&vals, 0, 0);
                if self.opts.pretty {
                    out.op(Op::Indent(if head.colon { IndentKind::Current } else { IndentKind::Block }, n));
                }
            }
            other => {
                return Err(format!("format: unknown directive ~{}", other));
            }
        }
        Ok(())
    }

    fn emit_radix(&mut self, head: &Head, radix: u32, out: &mut Out) -> Result<(), String> {
        let vals = self.resolve_params(&head.params, &[Some(0), None, None, Some(3)])?;
        let mincol = Self::int_param(&vals, 0, 0).max(0) as usize;
        let padchar = Self::char_param(&vals, 1, ' ');
        let commachar = Self::char_param(&vals, 2, ',');
        let comma_interval = Self::int_param(&vals, 3, 3).max(1) as usize;
        let arg = self.next_arg()?;
        let digits = match self.integer_in_radix(arg, radix) {
            Some(s) => s,
            None => {
                // A non-integer prints in ~A form (CL's rule).
                let mut s = String::new();
                render_value(self.heap, self.ctx, arg, false, &mut s)?;
                out.push_str(&pad(&s, mincol, 1, 0, padchar, true));
                return Ok(());
            }
        };
        let body = decorate_integer(&digits, head.colon, head.at, commachar, comma_interval);
        out.push_str(&pad(&body, mincol, 1, 0, padchar, true));
        Ok(())
    }

    /// Formats an integer `Value` in `radix` as an unsigned digit string plus a
    /// leading `-` for negatives (the sign/commas are added by the caller).
    /// Returns `None` for a non-integer argument.
    fn integer_in_radix(&self, v: Value, radix: u32) -> Option<String> {
        match v {
            _ if int_of(self.heap, v).is_some() => {
                Some(int_to_radix(int_of(self.heap, v).expect("just tested"), radix))
            }
            Value::Boxed(id) if self.heap.is_bignum(id) => {
                let big = self.heap.bignum_value(id);
                Some(big.to_str_radix(radix))
            }
            _ => None,
        }
    }

    fn emit_r(&mut self, head: &Head, out: &mut Out) -> Result<(), String> {
        // `~nR` (a radix parameter present) behaves like `~D` in that radix.
        let has_radix = matches!(head.params.first(), Some(Param::Int(_) | Param::Arg | Param::Count));
        if has_radix {
            let vals = self.resolve_params(&head.params, &[Some(10), Some(0), None, None, Some(3)])?;
            let radix = Self::int_param(&vals, 0, 10).clamp(2, 36) as u32;
            let mincol = Self::int_param(&vals, 1, 0).max(0) as usize;
            let padchar = Self::char_param(&vals, 2, ' ');
            let commachar = Self::char_param(&vals, 3, ',');
            let comma_interval = Self::int_param(&vals, 4, 3).max(1) as usize;
            let arg = self.next_arg()?;
            let digits = self
                .integer_in_radix(arg, radix)
                .ok_or("format: ~R requires an integer argument")?;
            let body = decorate_integer(&digits, head.colon, head.at, commachar, comma_interval);
            out.push_str(&pad(&body, mincol, 1, 0, padchar, true));
            return Ok(());
        }
        // No radix parameter: English/Roman spellings.
        let arg = self.next_arg()?;
        let Some(n) = int_of(self.heap, arg) else {
            return Err("format: ~R without a radix requires an integer argument".to_string());
        };
        let text = match (head.colon, head.at) {
            (false, false) => english_cardinal(n),
            (true, false) => english_ordinal(n),
            (false, true) => roman(n, false)?,
            (true, true) => roman(n, true)?,
        };
        out.push_str(&text);
        Ok(())
    }

    fn emit_f(&mut self, head: &Head, out: &mut Out) -> Result<(), String> {
        let vals = self.resolve_params(&head.params, &[None, None, Some(0), None, Some(' ' as i64)])?;
        let w = Self::int_param(&vals, 0, -1);
        let d = Self::int_param(&vals, 1, -1);
        let k = Self::int_param(&vals, 2, 0);
        let overflow = vals.get(3).copied().flatten().and_then(|n| char::from_u32(n as u32));
        let padchar = Self::char_param(&vals, 4, ' ');
        let f = self.next_float()?;
        let scaled = f * 10f64.powi(k as i32);
        let mut s = if d >= 0 { format!("{:.*}", d as usize, scaled) } else { trim_f64(scaled) };
        if head.at && !s.starts_with('-') {
            s.insert(0, '+');
        }
        if w >= 0 {
            let w = w as usize;
            if s.chars().count() > w {
                if let Some(oc) = overflow {
                    s = std::iter::repeat_n(oc, w).collect();
                }
            }
            s = left_pad(&s, w, padchar);
        }
        out.push_str(&s);
        Ok(())
    }

    fn emit_e(&mut self, head: &Head, out: &mut Out) -> Result<(), String> {
        // `~w,d,e,k,overflow,padchar,exptcharE`. A pragmatic scientific form.
        let vals = self.resolve_params(&head.params, &[None, None, None, Some(1), None, Some(' ' as i64), None])?;
        let w = Self::int_param(&vals, 0, -1);
        let d = Self::int_param(&vals, 1, 6);
        let exptchar = vals.get(6).copied().flatten().and_then(|n| char::from_u32(n as u32)).unwrap_or('e');
        let padchar = Self::char_param(&vals, 5, ' ');
        let f = self.next_float()?;
        let mut s = format!("{:.*e}", d.max(0) as usize, f);
        // Rust prints `1.5e2`; CL uses an explicit sign on the exponent.
        if let Some(idx) = s.find('e') {
            let (mant, exp) = s.split_at(idx);
            let exp = &exp[1..];
            let (sign, mag) = if let Some(m) = exp.strip_prefix('-') { ("-", m) } else { ("+", exp) };
            s = format!("{}{}{}{}", mant, exptchar, sign, mag);
        }
        if head.at && !s.starts_with('-') {
            s.insert(0, '+');
        }
        if w >= 0 {
            s = left_pad(&s, w as usize, padchar);
        }
        out.push_str(&s);
        Ok(())
    }

    fn emit_g(&mut self, head: &Head, out: &mut Out) -> Result<(), String> {
        // General: use fixed notation for moderate magnitudes, else scientific.
        let peeked = self.peek_arg();
        let use_e = matches!(peeked, Some(v) if float_of(self.heap, v).map(|f| {
            let a = f.abs();
            a != 0.0 && !(1e-3..1e7).contains(&a)
        }).unwrap_or(false));
        if use_e {
            self.emit_e(head, out)
        } else {
            self.emit_f(head, out)
        }
    }

    fn emit_dollars(&mut self, head: &Head, out: &mut Out) -> Result<(), String> {
        let vals = self.resolve_params(&head.params, &[Some(2), Some(1), Some(0), Some(' ' as i64)])?;
        let d = Self::int_param(&vals, 0, 2).max(0) as usize;
        let n = Self::int_param(&vals, 1, 1).max(0) as usize;
        let w = Self::int_param(&vals, 2, 0).max(0) as usize;
        let padchar = Self::char_param(&vals, 3, ' ');
        let f = self.next_float()?;
        let neg = f < 0.0;
        let body = format!("{:.*}", d, f.abs());
        // Ensure at least `n` digits before the decimal point.
        let (int_part, frac_part) = match body.split_once('.') {
            Some((i, fr)) => (i.to_string(), format!(".{}", fr)),
            None => (body.clone(), String::new()),
        };
        let int_part = if int_part.len() < n { left_pad(&int_part, n, '0') } else { int_part };
        let mut s = format!("{}{}", int_part, frac_part);
        let sign = if neg { "-" } else if head.at { "+" } else { "" };
        if head.colon {
            // Sign before the padding.
            let padded = left_pad(&s, w.saturating_sub(sign.len()), padchar);
            out.push_str(sign);
            out.push_str(&padded);
        } else {
            s = format!("{}{}", sign, s);
            out.push_str(&left_pad(&s, w, padchar));
        }
        Ok(())
    }

    fn emit_tab(&mut self, head: &Head, out: &mut Out) -> Result<(), String> {
        let vals = self.resolve_params(&head.params, &[Some(1), Some(1)])?;
        let col = Self::int_param(&vals, 0, 1).max(0) as usize;
        let inc = Self::int_param(&vals, 1, 1).max(1) as usize;
        // Under the pretty printer the column a tab lands on isn't known until
        // the layout pass has chosen the line breaks, so record the tab
        // instead of padding here. `~:T`/`~:@T` measure from the enclosing
        // logical block's own column rather than the start of the line.
        if self.opts.pretty {
            out.op(Op::Tab {
                kind: match (head.colon, head.at) {
                    (false, false) => TabKind::Line,
                    (false, true) => TabKind::LineRelative,
                    (true, false) => TabKind::Section,
                    (true, true) => TabKind::SectionRelative,
                },
                colnum: col as i64,
                colinc: inc as i64,
            });
            return Ok(());
        }
        let cur = current_column(&out.text);
        if head.colon || head.at {
            // Relative tab: at least `col` spaces, then round up to `inc`.
            let mut pad = col;
            while !(cur + pad).is_multiple_of(inc) {
                pad += 1;
            }
            for _ in 0..pad {
                out.push(' ');
            }
        } else {
            // Absolute tab: pad to column `col`, or to the next `inc` multiple.
            if cur < col {
                for _ in 0..(col - cur) {
                    out.push(' ');
                }
            } else if inc > 1 {
                let mut target = col;
                while target <= cur {
                    target += inc;
                }
                for _ in 0..(target - cur) {
                    out.push(' ');
                }
            } else {
                out.push(' ');
            }
        }
        Ok(())
    }

    fn emit_indirection(&mut self, head: &Head, out: &mut Out) -> Result<(), String> {
        let control = match self.next_arg()? {
            Value::Str(id) => self.heap.string(id).to_string(),
            _ => return Err("format: ~? requires a control-string argument".to_string()),
        };
        let nodes = parse(&control)?;
        if head.at {
            // Consume further arguments from the current stream.
            self.interp_seq(&nodes, out)?;
        } else {
            // The next argument is a list supplying this sub-format's arguments.
            let list = self.next_arg()?;
            let sublist = list_to_vec(self.heap, list);
            let saved = std::mem::replace(&mut self.args, sublist);
            let saved_pos = std::mem::replace(&mut self.pos, 0);
            let r = self.interp_seq(&nodes, out);
            self.args = saved;
            self.pos = saved_pos;
            r?;
        }
        Ok(())
    }

    fn next_float(&mut self) -> Result<f64, String> {
        let v = self.next_arg()?;
        float_of(self.heap, v).ok_or_else(|| "format: this directive requires a numeric argument".to_string())
    }
}

/// The integer value of `v` for the integer directives: a bare `Value::Int`
/// (which is the `i32` case and nothing else) or a boxed narrow integer,
/// whose stored word already *is* the number its type names. `None` for
/// anything that isn't a fixed-width integer.
///
/// Every directive that wants "an integer argument" goes through this rather
/// than matching `Value::Int` itself, so that `(format nil "~d" (the u8 200))`
/// prints `200` instead of reporting a non-integer argument — the box is how
/// a `u8` reaches the printer at all.
fn int_of(heap: &Heap, v: Value) -> Option<i64> {
    match v {
        Value::Int(n) => Some(n),
        Value::Boxed(id) => heap.narrow_box(id).map(|n| n.value),
        _ => None,
    }
}

/// The numeric value of `v` as `f64` for the float directives (accepts every
/// numeric `Sexpr` scalar).
fn float_of(heap: &Heap, v: Value) -> Option<f64> {
    use num_traits::ToPrimitive;
    match v {
        Value::Int(n) => Some(n as f64),
        // A narrow integer is a number like any other: `~f` on a `u8` is
        // binary64 arithmetic on its value, exactly as on an `i32`.
        Value::Boxed(id) if heap.narrow_box(id).is_some() => {
            heap.narrow_box(id).map(|n| n.value as f64)
        }
        Value::Boxed(id) if heap.is_bignum(id) => heap.bignum_value(id).to_f64(),
        Value::Boxed(id) if heap.is_ratio(id) => heap.ratio_value(id).to_f64(),
        // Both float widths, widened to `f64`. This is the *numeric* reader —
        // `~f`/`~e`/`~$` compute in binary64 whatever the argument's width —
        // so widening is what the caller wants; `render`'s `~a` path is the
        // one that has to keep the width, and it reads `float_box` itself.
        //
        // Positively a float box. This used to be "any box that isn't one of
        // the six aggregate kinds", which meant every new `BoxedObj` variant
        // silently became a float here until someone remembered to extend the
        // negative chain — and the accessor panics on a non-float, so the
        // failure was an abort, not a `None`.
        Value::Boxed(id) => match heap.float_box(id) {
            Some(FloatBox::F32(f)) => Some(f64::from(f)),
            Some(FloatBox::F64(f)) => Some(f),
            None => None,
        },
        _ => None,
    }
}

// ===========================================================================
// Formatting helpers
// ===========================================================================

/// Column of the write cursor: characters since the last newline in `out`.
fn current_column(out: &str) -> usize {
    match out.rfind('\n') {
        Some(i) => out[i + 1..].chars().count(),
        None => out.chars().count(),
    }
}

fn left_pad(s: &str, width: usize, padchar: char) -> String {
    let len = s.chars().count();
    if len >= width {
        return s.to_string();
    }
    let mut out: String = std::iter::repeat_n(padchar, width - len).collect();
    out.push_str(s);
    out
}

/// `~A`/`~S` padding: `minpad` copies of `padchar` are always added, then the
/// field is grown to `mincol` in `colinc` steps. `at` right-justifies (pad on
/// the left) instead of the default left-justify.
fn pad(s: &str, mincol: usize, colinc: usize, minpad: usize, padchar: char, at: bool) -> String {
    let base: String = std::iter::repeat_n(padchar, minpad).collect();
    let mut content_len = s.chars().count() + minpad;
    let mut extra = 0;
    while content_len < mincol {
        extra += colinc;
        content_len += colinc;
    }
    let fill: String = std::iter::repeat_n(padchar, extra).collect();
    if at {
        format!("{}{}{}", fill, base, s)
    } else {
        format!("{}{}{}", s, base, fill)
    }
}

/// `~<…~>` justification: spread `pieces` across at least `mincol` columns (in
/// `colinc` steps) with `minpad` between them. `colon` also pads before the
/// first piece, `at` after the last.
fn justify(
    pieces: Vec<Out>,
    mincol: usize,
    colinc: usize,
    minpad: usize,
    padchar: char,
    colon: bool,
    at: bool,
) -> Out {
    let text_len: usize = pieces.iter().map(|p| p.text.chars().count()).sum();
    // Number of gaps that receive padding.
    let mut gaps = pieces.len().saturating_sub(1);
    if colon {
        gaps += 1;
    }
    if at {
        gaps += 1;
    }
    if gaps == 0 {
        // A single piece with no edge padding: just meet mincol on the right.
        let mut only = pieces.into_iter().next().unwrap_or_default();
        let padding = pad("", mincol.saturating_sub(text_len), colinc, 0, padchar, false);
        only.push_str(&padding);
        return only;
    }
    let min_total = text_len + gaps * minpad;
    let mut total = min_total;
    while total < mincol {
        total += colinc;
    }
    let pad_total = total - text_len;
    let base = pad_total / gaps;
    let extra = pad_total % gaps;
    let gap_str = |i: usize| -> String {
        let n = base + if i < extra { 1 } else { 0 };
        std::iter::repeat_n(padchar, n).collect()
    };
    let mut out = Out::new();
    let mut gap_idx = 0;
    if colon {
        out.push_str(&gap_str(gap_idx));
        gap_idx += 1;
    }
    let count = pieces.len();
    for (i, p) in pieces.into_iter().enumerate() {
        out.append(p);
        if i + 1 < count {
            out.push_str(&gap_str(gap_idx));
            gap_idx += 1;
        }
    }
    if at {
        out.push_str(&gap_str(gap_idx));
    }
    out
}

/// `~C`: `:` spells out non-graphic characters by name; `@` uses `#\` reader
/// syntax; plain prints the character itself.
fn format_char(c: char, colon: bool, at: bool) -> String {
    let name = char_name(c);
    if colon {
        match name {
            Some(n) => n.to_string(),
            None => c.to_string(),
        }
    } else if at {
        match name {
            Some(n) => format!("#\\{}", n),
            None => format!("#\\{}", c),
        }
    } else {
        c.to_string()
    }
}

fn char_name(c: char) -> Option<&'static str> {
    match c {
        ' ' => Some("Space"),
        '\n' => Some("Newline"),
        '\t' => Some("Tab"),
        '\r' => Some("Return"),
        '\u{0}' => Some("Null"),
        '\u{7}' => Some("Bell"),
        '\u{8}' => Some("Backspace"),
        '\u{c}' => Some("Page"),
        '\u{1b}' => Some("Escape"),
        '\u{7f}' => Some("Rubout"),
        _ => None,
    }
}

/// `~(…~)` case conversion. `:@` upcases, `:` capitalizes each word, `@`
/// capitalizes the first word only, plain downcases. Applied through
/// [`Out::map_text`] so any pretty-printer op recorded inside the `~(…~)` keeps
/// pointing at the same place in the (possibly re-lengthened) text, and so the
/// word-boundary state of the capitalizing forms carries across op anchors.
fn apply_case(out: &mut Out, colon: bool, at: bool) {
    // `state` is "a word may start here" for `:`, "no word has started yet"
    // for `@`; the other two forms ignore it.
    out.map_text(true, |s, state| match (colon, at) {
        (true, true) => s.to_uppercase(),
        (true, false) => capitalize_words(s, state),
        (false, true) => capitalize_first(s, state),
        (false, false) => s.to_lowercase(),
    });
}

fn capitalize_words(s: &str, start_of_word: &mut bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_alphanumeric() {
            if *start_of_word {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            *start_of_word = false;
        } else {
            out.push(c);
            *start_of_word = true;
        }
    }
    out
}

fn capitalize_first(s: &str, pending: &mut bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if *pending && c.is_alphanumeric() {
            out.extend(c.to_uppercase());
            *pending = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn int_to_radix(n: i64, radix: u32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let negative = n < 0;
    let mut m = (n as i128).unsigned_abs();
    let mut digits = Vec::new();
    let r = radix as u128;
    while m > 0 {
        let d = (m % r) as u32;
        digits.push(std::char::from_digit(d, radix).unwrap());
        m /= r;
    }
    if negative {
        digits.push('-');
    }
    digits.iter().rev().collect()
}

/// Adds a sign (`@` forces `+`) and, when `colon`, groups the integer digits
/// with `commachar` every `interval` places.
fn decorate_integer(digits: &str, colon: bool, at: bool, commachar: char, interval: usize) -> String {
    let (sign, mag) = if let Some(m) = digits.strip_prefix('-') {
        ("-", m)
    } else if at {
        ("+", digits)
    } else {
        ("", digits)
    };
    let grouped = if colon { group_digits(mag, commachar, interval) } else { mag.to_string() };
    format!("{}{}", sign, grouped)
}

fn group_digits(digits: &str, commachar: char, interval: usize) -> String {
    let chars: Vec<char> = digits.chars().collect();
    let mut out = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(interval) {
            out.push(commachar);
        }
        out.push(*c);
    }
    out.into_iter().collect()
}

fn trim_f64(f: f64) -> String {
    if f == f.trunc() && f.is_finite() {
        format!("{:.1}", f)
    } else {
        f.to_string()
    }
}

/// [`trim_f64`] for an `f32`.
///
/// A separate function rather than widening and reusing the `f64` one:
/// `f32::to_string` gives the shortest text that reads back as the same
/// binary32 value, and `f64::to_string` on the widened number gives the
/// shortest that reads back as the same binary64 one. Those differ — 0.1 as
/// an `f32` is `0.1` here and `0.10000000149011612` there — and the first is
/// the right answer for a value whose type says binary32.
///
/// Widening and then shortening for binary32 is not an option either: it
/// would print a genuine `f64` that happens to be binary32-representable
/// with too few digits, and that one no longer reads back.
fn trim_f32(f: f32) -> String {
    if f == f.trunc() && f.is_finite() {
        format!("{:.1}", f)
    } else {
        f.to_string()
    }
}

// ---- English number spelling -------------------------------------------------

const ONES: [&str; 20] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
    "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
];
const TENS: [&str; 10] =
    ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
const SCALES: [&str; 7] = ["", " thousand", " million", " billion", " trillion", " quadrillion", " quintillion"];

fn english_cardinal(n: i64) -> String {
    if n == 0 {
        return "zero".to_string();
    }
    let negative = n < 0;
    let mut m = (n as i128).unsigned_abs();
    // Split into groups of three digits.
    let mut groups = Vec::new();
    while m > 0 {
        groups.push((m % 1000) as u32);
        m /= 1000;
    }
    let mut parts = Vec::new();
    for (i, g) in groups.iter().enumerate().rev() {
        if *g == 0 {
            continue;
        }
        let scale = SCALES.get(i).copied().unwrap_or("");
        parts.push(format!("{}{}", three_digits(*g), scale));
    }
    let text = parts.join(" ");
    if negative {
        format!("negative {}", text)
    } else {
        text
    }
}

fn three_digits(g: u32) -> String {
    let mut out = String::new();
    let hundreds = g / 100;
    let rest = g % 100;
    if hundreds > 0 {
        out.push_str(ONES[hundreds as usize]);
        out.push_str(" hundred");
        if rest > 0 {
            out.push(' ');
        }
    }
    if rest > 0 {
        if rest < 20 {
            out.push_str(ONES[rest as usize]);
        } else {
            out.push_str(TENS[(rest / 10) as usize]);
            if !rest.is_multiple_of(10) {
                out.push('-');
                out.push_str(ONES[(rest % 10) as usize]);
            }
        }
    }
    out
}

fn english_ordinal(n: i64) -> String {
    let card = english_cardinal(n);
    // Transform the final word into its ordinal form.
    let (prefix, last) = match card.rfind([' ', '-']) {
        Some(i) => (&card[..=i], &card[i + 1..]),
        None => ("", card.as_str()),
    };
    let ord_last = match last {
        "zero" => "zeroth",
        "one" => "first",
        "two" => "second",
        "three" => "third",
        "five" => "fifth",
        "eight" => "eighth",
        "nine" => "ninth",
        "twelve" => "twelfth",
        _ => {
            if let Some(stripped) = last.strip_suffix('y') {
                return format!("{}{}ieth", prefix, stripped);
            } else if last.ends_with("th") {
                return format!("{}{}", prefix, last);
            } else {
                return format!("{}{}th", prefix, last);
            }
        }
    };
    format!("{}{}", prefix, ord_last)
}

fn roman(n: i64, old: bool) -> Result<String, String> {
    if n <= 0 || n >= 4000 {
        return Err("format: ~@R (Roman numerals) only supports 1..3999".to_string());
    }
    let mut n = n as u32;
    let mut out = String::new();
    let table: &[(u32, &str)] = if old {
        &[(1000, "M"), (500, "D"), (100, "C"), (50, "L"), (10, "X"), (5, "V"), (1, "I")]
    } else {
        &[
            (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"),
            (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
        ]
    };
    for (v, s) in table {
        while n >= *v {
            out.push_str(s);
            n -= *v;
        }
    }
    Ok(out)
}

// ===========================================================================
// Value rendering (`~A`/`~S`)
// ===========================================================================

/// Renders one `Sexpr` [`Value`] into `out` for `~a`/`~s`. `standard` selects
/// CL `prin1` (reader syntax: strings quoted, chars `#\c`) over CL `princ`
/// (bare); the flag threads through nested lists — and is handed to a type's
/// own `print-object` method as CL's `*print-escape*`.
///
/// Before any built-in representation is produced, `ctx.interp` (when present)
/// is asked whether `v`'s runtime type implements the `print-object` trait; if
/// it does, that method's string *is* the rendering. This is the whole of the
/// "a user-defined type prints its own way, even nested inside a list"
/// mechanism — it sits here rather than at the call site because only here is
/// it known whether `~a` or `~s` is asking (`Interp::print_object`'s doc
/// comment covers the rest).
///
/// `Err` is the user printer's own failure (a `panic` in its body, say),
/// surfaced rather than swallowed; every built-in path is infallible.
///
/// `ctx.print_vars` bounds the walk: `*print-level*` cuts nesting off with `#`,
/// `*print-length*` cuts element counts off with `...`, and `*print-circle*`
/// labels shared/circular substructure (`#n=` / `#n#`) so a cycle terminates.
/// The label numbering is per call — CL numbers per printed object, and each
/// `~a`/`~s` argument is its own object.
pub(crate) fn render_value(
    heap: &mut Heap,
    ctx: RenderCtx<'_>,
    v: Value,
    standard: bool,
    out: &mut String,
) -> Result<(), String> {
    Renderer::new(heap, ctx, v).render(heap, ctx, v, standard, 0, out)
}

/// Per-node state for the `*print-circle*` passes: [`scan_shared`] records
/// `Once` on first sight and `Repeated` on the second, then `render_value`
/// drops the `Once`s. `Labeled` is assigned by [`Renderer::render`] when a
/// repeated node is first *printed*, so `#n=` numbers appear in reading order.
#[derive(Clone, Copy)]
enum ShareState {
    Once,
    Repeated,
    Labeled(u32),
}

/// Identity of a node that can contain other values, for the `*print-circle*`
/// tables. Cons cells compare by cell address, boxed objects by slot id.
///
/// Floats/bignums/ratios are `Value::Boxed` too but hold no nested value, so
/// they are deliberately *not* keys: two references to one float box are not
/// interesting sharing, and labelling them `#1=` would be noise. Hash tables
/// are excluded for the same practical reason — they render as
/// `#<hashtable<K,V> count=N>` without recursing, so they can neither cycle nor
/// usefully be labelled.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum NodeKey {
    Cons(usize),
    Boxed(u32),
}

fn container_key(heap: &Heap, v: Value) -> Option<NodeKey> {
    match v {
        Value::Cons(c) => Some(NodeKey::Cons(c.addr())),
        Value::Boxed(id) if heap.is_struct(id) || heap.is_enum(id) || heap.is_dyn(id) => {
            Some(NodeKey::Boxed(id.as_u32()))
        }
        _ => None,
    }
}

/// Appends the values `v` directly contains to `into` (nothing for a leaf).
fn children_of(heap: &Heap, v: Value, into: &mut Vec<Value>) {
    match v {
        Value::Cons(_) => {
            into.push(heap.car(v).expect("cons car"));
            into.push(heap.cdr(v).expect("cons cdr"));
        }
        Value::Boxed(id) if heap.is_struct(id) => {
            into.extend((0..heap.struct_field_count(id)).map(|i| heap.struct_field(id, i)));
        }
        Value::Boxed(id) if heap.is_enum(id) => {
            into.extend((0..heap.enum_field_count(id)).map(|i| heap.enum_field(id, i)));
        }
        Value::Boxed(id) if heap.is_dyn(id) => into.push(heap.dyn_value(id)),
        _ => {}
    }
}

/// Pass 1 of `*print-circle*`: mark every container node as seen once or
/// repeated. Iterative (an explicit work stack, not recursion) because the
/// structure being scanned may be a very long list *or* already circular —
/// a node seen a second time is marked and not descended into again, which is
/// what makes the walk terminate on a cycle.
fn scan_shared(heap: &Heap, root: Value, seen: &mut HashMap<NodeKey, ShareState>) {
    let mut stack = vec![root];
    let mut kids = Vec::new();
    while let Some(v) = stack.pop() {
        let Some(key) = container_key(heap, v) else { continue };
        match seen.get_mut(&key) {
            Some(state) => {
                *state = ShareState::Repeated;
                continue;
            }
            None => {
                seen.insert(key, ShareState::Once);
            }
        }
        kids.clear();
        children_of(heap, v, &mut kids);
        stack.extend(kids.iter().copied());
    }
}

/// What [`Renderer::pre`] decided about a node, before any of it is printed.
pub enum Pre {
    /// Print this text and nothing else for the node: a `#n#` back-reference
    /// or the `#` of a `*print-level*` cut-off.
    Stop(String),
    /// Print the node normally, prefixed by this text (a `#n=` label, or
    /// empty).
    Go(String),
}

/// The `*print-circle*` label table for one printing operation, empty (and
/// therefore free) when `*print-circle*` is off.
///
/// Shared by both walkers over a value: this module's [`Renderer::render`] and
/// [`crate::pprint::render`], which lays lists out itself and calls back
/// here for everything else. One [`Renderer`] must span the whole operation,
/// or the two would hand out conflicting `#n=` numbers for the same node.
pub struct Renderer {
    shared: HashMap<NodeKey, ShareState>,
    next_label: u32,
}

impl Renderer {
    /// Scans `root` for shared substructure (only when `*print-circle*` is on)
    /// and returns the renderer that will label it.
    pub(crate) fn new(heap: &Heap, ctx: RenderCtx<'_>, root: Value) -> Renderer {
        let mut shared = HashMap::new();
        if ctx.print_vars.circle {
            // Pass 1: find the nodes reached more than once. Only those get a
            // label, so an acyclic value with no sharing prints exactly as it
            // does with `*print-circle*` off.
            scan_shared(heap, root, &mut shared);
            shared.retain(|_, state| matches!(state, ShareState::Repeated));
        }
        Renderer { shared, next_label: 1 }
    }

    /// Decides what happens to `v` at nesting `depth`: a back-reference, a
    /// level cut-off, or an ordinary rendering (possibly label-prefixed).
    pub(crate) fn pre(&mut self, heap: &Heap, ctx: RenderCtx<'_>, v: Value, depth: usize) -> Pre {
        let mut prefix = String::new();
        if ctx.print_vars.circle {
            if let Some(key) = container_key(heap, v) {
                match self.shared.get(&key).copied() {
                    Some(ShareState::Labeled(n)) => return Pre::Stop(format!("#{}#", n)),
                    // Repeated but not yet printed: this occurrence is the
                    // definition, so it carries the `#n=` label and the rest
                    // of the object still prints normally below.
                    Some(_) => {
                        let n = self.next_label;
                        self.next_label += 1;
                        self.shared.insert(key, ShareState::Labeled(n));
                        prefix = format!("#{}=", n);
                    }
                    None => {}
                }
            }
        }
        // `*print-level*` counts only objects with structure; an atom prints
        // at any depth (CL prints `#` for "a nested object too deep", never
        // for a number or a symbol). A `:dyn` box is transparent — it is a
        // dispatch mechanism, not a level of the datum — so it is not a
        // container for this purpose either.
        let composite = matches!(v, Value::Cons(_))
            || matches!(v, Value::Boxed(id) if heap.is_struct(id) || heap.is_enum(id));
        match ctx.print_vars.cuts().0 {
            // The label is dropped along with the object it would have named:
            // `#` says "something was here", and nothing can refer back to a
            // node that never printed.
            Some(limit) if composite && depth >= limit => Pre::Stop("#".to_string()),
            _ => Pre::Go(prefix),
        }
    }

    /// [`Self::pre`] for the callers that write into a `String`: returns true
    /// when the node is finished (its `#n#`/`#` already written).
    fn prologue(
        &mut self,
        heap: &Heap,
        ctx: RenderCtx<'_>,
        v: Value,
        depth: usize,
        out: &mut String,
    ) -> bool {
        match self.pre(heap, ctx, v, depth) {
            Pre::Stop(text) => {
                out.push_str(&text);
                true
            }
            Pre::Go(prefix) => {
                out.push_str(&prefix);
                false
            }
        }
    }

    /// `*print-length*`: whether the element/field at index `printed` is past
    /// the limit (the caller writes the `...`, since its separator differs).
    pub(crate) fn length_reached(ctx: RenderCtx<'_>, printed: usize) -> bool {
        matches!(ctx.print_vars.cuts().1, Some(limit) if printed >= limit)
    }

    /// Renders `v` at nesting `depth` the flat (non-pretty) way.
    /// One field of a struct or enum box: rendered as the value it is —
    /// unless the program's definition says the field is a niche-represented
    /// `Option` (`PrintEnv::field_is_niched_option`), in which case the word
    /// is that `Option`, and prints as `none` or `(some ...)` around the
    /// payload exactly as a boxed one would. The box is the only place a
    /// niched `Option` can sit with nothing static to say what it is, so
    /// this is the only place the question is asked.
    #[allow(clippy::too_many_arguments)]
    fn render_field(
        &mut self,
        heap: &mut Heap,
        ctx: RenderCtx<'_>,
        type_key: &str,
        variant: Option<usize>,
        index: usize,
        f: Value,
        standard: bool,
        depth: usize,
        out: &mut String,
    ) -> Result<(), String> {
        if !ctx.env.field_is_niched_option(type_key, variant, index) {
            return self.render(heap, ctx, f, standard, depth, out);
        }
        if f == Value::Empty {
            out.push_str("none");
            return Ok(());
        }
        out.push_str("(some ");
        self.render(heap, ctx, f, standard, depth + 1, out)?;
        out.push(')');
        Ok(())
    }

    pub(crate) fn render(
        &mut self,
        heap: &mut Heap,
        ctx: RenderCtx<'_>,
        v: Value,
        standard: bool,
        depth: usize,
        out: &mut String,
    ) -> Result<(), String> {
        if self.prologue(heap, ctx, v, depth, out) {
            return Ok(());
        }
        if let Some(text) = ctx.env.print_object(heap, v, standard)? {
            out.push_str(&text);
            return Ok(());
        }
        self.render_builtin(heap, ctx, v, standard, depth, out)
    }

    /// One integer under `*print-base*`/`*print-radix*`. `decimal` and
    /// `digits` are two ways of writing the same number — the caller supplies
    /// both because `i64` and `BigInt` spell them differently, and base 10
    /// wants the plain `Display` rather than a digit loop.
    ///
    /// CL's radix markers go *before* the sign (`#x-1f`), and base 10 marks
    /// itself with a trailing decimal point instead of a prefix. Both
    /// spellings read back as the same integer whatever `*read-base*` is,
    /// which is the whole point of `*print-radix*`.
    fn integer_text(
        ctx: RenderCtx<'_>,
        decimal: impl FnOnce() -> String,
        digits: impl FnOnce(u32) -> String,
    ) -> Result<String, String> {
        let radix = ctx.print_vars.radix_of()?;
        let body = if radix == 10 {
            decimal()
        } else {
            digits(radix)
        };
        if !ctx.print_vars.radix {
            return Ok(body);
        }
        Ok(match radix {
            2 => format!("#b{}", body),
            8 => format!("#o{}", body),
            16 => format!("#x{}", body),
            10 => format!("{}.", body),
            r => format!("#{}r{}", r, body),
        })
    }

    fn render_builtin(
        &mut self,
        heap: &mut Heap,
        ctx: RenderCtx<'_>,
        v: Value,
        standard: bool,
        depth: usize,
        out: &mut String,
    ) -> Result<(), String> {
        // `*print-readably*` says the output must read back as an equal
        // object, so it turns escaping on whatever the caller asked for —
        // `~a` on a string under it still prints the quotes.
        let standard = standard || ctx.print_vars.readably;
        match v {
            Value::Empty => out.push_str("()"),
            Value::Int(n) => {
                let text = Self::integer_text(ctx, || n.to_string(), |r| int_to_radix(n, r))?;
                out.push_str(&text);
            }
            Value::Bool(b) => out.push_str(&b.to_string()),
            Value::Char(c) => {
                if standard {
                    out.push_str("#\\");
                }
                out.push(c);
            }
            Value::Str(id) => {
                if standard {
                    out.push_str(&format!("{:?}", heap.string(id)));
                } else {
                    out.push_str(&heap.string(id));
                }
            }
            Value::Symbol(id) => {
                let text = ctx.print_vars.render_symbol(heap.symbol_name(id));
                out.push_str(&text);
            }
            Value::Path(id) => {
                let segs: Vec<String> = heap
                    .path_segments(id)
                    .iter()
                    .map(|s| ctx.print_vars.render_symbol(heap.symbol_name(*s)))
                    .collect();
                out.push_str(&segs.join("::"));
            }
            Value::Boxed(id) if heap.is_bignum(id) => {
                let n = heap.bignum_value(id).clone();
                let text = Self::integer_text(ctx, || n.to_string(), |r| n.to_str_radix(r))?;
                out.push_str(&text);
            }
            Value::Boxed(id) if heap.is_ratio(id) => {
                let r = heap.ratio_value(id);
                out.push_str(&format!("{}/{}", r.numer(), r.denom()));
            }
            Value::Boxed(id) if heap.is_struct(id) => {
                let key = crate::stored_type_key(heap, id)
                    .expect("a struct box has a type name")
                    .to_string();
                out.push_str(&format!("#<{}", key));
                for i in 0..heap.struct_field_count(id) {
                    if Self::length_reached(ctx, i) {
                        out.push_str(" ...");
                        break;
                    }
                    out.push(' ');
                    // `x: 1` — the name the definition gives the field, when it
                    // gives one. Not `:x`, which would read as a keyword
                    // argument (`(point::make :x 1)`).
                    if let Some(name) = ctx.env.field_name(&key, i) {
                        out.push_str(&name);
                        out.push_str(": ");
                    }
                    // Read the field out before the recursive call: `heap` is
                    // `&mut` here (the `print-object` dispatch needs it), so the
                    // read cannot stay borrowed across it.
                    let f = heap.struct_field(id, i);
                    self.render_field(heap, ctx, &key, None, i, f, standard, depth + 1, out)?;
                }
                out.push('>');
            }
            Value::Boxed(id) if heap.is_enum(id) => {
                let type_key = crate::stored_type_key(heap, id)
                    .expect("an enum box has a type name")
                    .to_string();
                let variant = heap.enum_variant(id);
                // `enums` holds every `TypeEntry::Enum` in the interpreter's
                // scope tree — a user `defenum`'s own exec, *and* the built-in
                // sum types (`Option`/`Result`/the error types), which
                // `Interp::new` seeds from `registry::builtin_sum_defs` up
                // front (see that function's doc comment) precisely so this
                // lookup never needs a second table to fall back to. Coming
                // up empty here means the lookup itself is broken (a stale
                // `Path`, an enum this table was never told about), not that
                // the name lives somewhere else.
                let name = ctx
                    .env
                    .enum_variant_name(&type_key, variant)
                    .unwrap_or_else(|| "<unknown-variant>".to_string());
                if heap.enum_field_count(id) == 0 {
                    out.push_str(&name);
                } else {
                    out.push('(');
                    out.push_str(&name);
                    for i in 0..heap.enum_field_count(id) {
                        if Self::length_reached(ctx, i) {
                            out.push_str(" ...");
                            break;
                        }
                        out.push(' ');
                        let f = heap.enum_field(id, i);
                        self.render_field(heap, ctx, &type_key, Some(variant), i, f, standard, depth + 1, out)?;
                    }
                    out.push(')');
                }
            }
            // Named by its whole type, as a `Vector<T>` is: the table carries
            // its instantiation (`hashtable<string,int>`) from `HashTable::new`.
            Value::Boxed(id) if heap.is_hashtable(id) => out.push_str(&format!(
                "#<{} count={}>",
                heap.struct_type_name(id),
                heap.hashtable_count(id)
            )),
            Value::Boxed(id) if heap.is_scope(id) => {
                out.push_str(&format!("#<scope depth={}>", heap.scope_frame_count(id)))
            }
            // Opaque, and identical whether the JIT took this `lambda`
            // (`BoxedObj::CompiledClosure`) or it is being tree-walked
            // (`BoxedObj::Closure`) — that is not a property of the value.
            Value::Boxed(id) if heap.is_compiled_closure(id) || heap.is_closure(id) => out.push_str("#<closure>"),
            // Unreadable, like CL's own — the seed is an implementation
            // detail, not part of the value.
            Value::Boxed(id) if heap.is_random_state(id) => out.push_str("#<random-state>"),
            // The other function value: a built-in reified as a value, shown
            // by name (there is nothing else to it).
            Value::Boxed(id) if heap.is_builtin_fn(id) => {
                let text = match heap.builtin_fn_recv(id) {
                    None => format!("#<builtin {}>", heap.builtin_fn_name(id)),
                    Some(pid) => {
                        // The receiver type's path, spelled straight from the
                        // heap's interned segments — `typelisp::types::Path`'s
                        // `Display` is the same `::` join, and this crate has
                        // no `Path` to borrow it from.
                        let segs: Vec<&str> =
                            heap.path_segments(pid).iter().map(|s| heap.symbol_name(*s)).collect();
                        format!("#<builtin {}::{}>", segs.join("::"), heap.builtin_fn_name(id))
                    }
                };
                out.push_str(&text);
            }
            // A trait object prints as the value it wraps: the box is a dispatch
            // mechanism, not part of the datum. (Must precede the float
            // fall-through, which reads any other `Boxed` as an `f64` — see
            // `float_of`'s matching negative guard.) Not a level of its own
            // either, hence `depth` rather than `depth + 1`.
            Value::Boxed(id) if heap.is_dyn(id) => {
                let inner = heap.dyn_value(id);
                self.render(heap, ctx, inner, standard, depth, out)?;
            }
            // Each width printed at its own precision. `trim_f32` gives the
            // shortest text that reads back as the same *binary32* value,
            // which is the whole reason the box carries the width: an `f32`
            // holding 0.1 prints `0.1`, not binary64's `0.10000000149011612`.
            //
            // Positively a float box for the same reason `float_of` is: the
            // bare `Value::Boxed(id)` fall-through this replaces turned every
            // unhandled box kind into an accessor panic.
            // A narrow integer prints as the number it is — the same text an
            // `i32` of that value gives, `*print-base*`/`*print-radix*`
            // included. The box exists so the *type* survives into a `Sexpr`,
            // not to make the number print differently.
            Value::Boxed(id) if heap.narrow_box(id).is_some() => {
                let n = heap.narrow_box(id).expect("just tested").value;
                let text = Self::integer_text(ctx, || n.to_string(), |r| int_to_radix(n, r))?;
                out.push_str(&text);
            }
            Value::Boxed(id) if heap.float_box(id).is_some() => match heap.float_box(id) {
                Some(FloatBox::F32(f)) => out.push_str(&trim_f32(f)),
                Some(FloatBox::F64(f)) => out.push_str(&trim_f64(f)),
                None => unreachable!("guarded by float_box above"),
            },
            // A `BoxedObj::Cell` — a binding slot, never a value handed to the
            // printer. Reaching here is an interpreter bug, reported rather
            // than mis-rendered as a float.
            Value::Boxed(_) => return Err("print: unprintable boxed object".to_string()),
            Value::Cons(_) => {
                out.push('(');
                let mut cur = v;
                let mut printed = 0usize;
                loop {
                    match cur {
                        Value::Cons(_) => {
                            // A later cell that is itself shared has to be
                            // printed as a *tail* (`. #1=(...)` / `. #1#`)
                            // rather than spliced into this list — that is
                            // what stops a circular list, whose last cdr
                            // points back at a cell already printed. The first
                            // cell is skipped: `prologue` labelled it, and
                            // re-entering here would loop.
                            if printed > 0 && ctx.print_vars.circle && self.is_shared(heap, cur) {
                                out.push_str(" . ");
                                self.render(heap, ctx, cur, standard, depth, out)?;
                                break;
                            }
                            if Self::length_reached(ctx, printed) {
                                out.push_str(if printed == 0 { "..." } else { " ..." });
                                break;
                            }
                            if printed > 0 {
                                out.push(' ');
                            }
                            let head = heap.car(cur).expect("cons car");
                            self.render(heap, ctx, head, standard, depth + 1, out)?;
                            printed += 1;
                            cur = heap.cdr(cur).expect("cons cdr");
                        }
                        Value::Empty => break,
                        other => {
                            out.push_str(" . ");
                            self.render(heap, ctx, other, standard, depth + 1, out)?;
                            break;
                        }
                    }
                }
                out.push(')');
            }
        }
        Ok(())
    }

    /// Whether `v` is one of the nodes [`scan_shared`] found more than once
    /// (labelled already or not).
    fn is_shared(&self, heap: &Heap, v: Value) -> bool {
        container_key(heap, v).is_some_and(|k| self.shared.contains_key(&k))
    }
}
