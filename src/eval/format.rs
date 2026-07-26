//! The Common Lisp `format` directive engine (CLHS §22.3), shared by the
//! `format`/`print`/`println` special forms (`crate::check::checker`) via
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
//! text directly; [`crate::eval::pprint`] turns the finished buffer into laid
//! out text. With `*print-pretty*` false nothing records an op and the buffer
//! is returned exactly as before.
//!
//! Deliberately unsupported (return a clear error): `~/name/` function-call
//! dispatch — typelisp has no runtime function-by-name lookup with `format`'s
//! calling convention. The `nil`-specific behavior of CL's `~:A`/`~@[` is
//! adapted to typelisp's `false`, which has no `nil`.

use std::collections::HashMap;

use crate::mem::{Heap, Value};
use crate::Path;

use super::interp::EnumDef;
use super::pprint::{self, IndentKind, NewlineKind, Op, Opts, Out, Style, TabKind};

/// Entry point: interpret `control` against the `Sexpr` argument list `args`,
/// returning the buffer it produced (text plus any pretty-printer ops — see
/// [`finish`], which turns one into text). Errors (bad directive, too few
/// arguments, …) are `String`s the caller turns into a recoverable
/// `EvalError::Panic`.
pub(crate) fn build(
    heap: &Heap,
    enums: &HashMap<Path, EnumDef>,
    control: &str,
    args: Value,
    opts: &Opts,
) -> Result<Out, String> {
    let nodes = parse(control)?;
    let items = list_to_vec(heap, args);
    let mut st = State { heap, enums, args: items, pos: 0, opts: *opts };
    let mut out = Out::new();
    st.interp_seq(&nodes, &mut out)?;
    Ok(out)
}

/// Turns a finished buffer into text: a buffer with no pretty-printer op in it
/// is already the answer, so the layout pass only runs when one was recorded.
pub(crate) fn finish(out: Out, opts: &Opts) -> String {
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
    heap: &'a Heap,
    enums: &'a HashMap<Path, EnumDef>,
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
        match self.next_arg()? {
            Value::Int(n) => Ok(n),
            Value::Char(c) => Ok(c as i64),
            _ => Err("format: a 'v' parameter requires an integer or character argument".to_string()),
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
        let sel = match self.next_arg()? {
            Value::Int(n) => n,
            _ => return Err("format: ~[ requires an integer argument".to_string()),
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
            let saved = std::mem::replace(&mut self.args, Vec::new());
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
                    pprint::render(self.heap, self.enums, arg, standard, Style::Default, out);
                } else {
                    let mut s = String::new();
                    render_value(self.heap, self.enums, arg, standard, &mut s);
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
                let n = match self.next_arg()? {
                    Value::Int(n) => n,
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
            '/' => {
                return Err("format: ~/name/ function-call directives are not supported".to_string());
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
                render_value(self.heap, self.enums, arg, false, &mut s);
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
            Value::Int(n) => Some(int_to_radix(n, radix)),
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
        let n = match arg {
            Value::Int(n) => n,
            _ => return Err("format: ~R without a radix requires an integer argument".to_string()),
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
        let mut s = if d >= 0 { format!("{:.*}", d as usize, scaled) } else { trim_float(scaled) };
        if head.at && !s.starts_with('-') {
            s.insert(0, '+');
        }
        if w >= 0 {
            let w = w as usize;
            if s.chars().count() > w {
                if let Some(oc) = overflow {
                    s = std::iter::repeat(oc).take(w).collect();
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
            a != 0.0 && (a < 1e-3 || a >= 1e7)
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
            while (cur + pad) % inc != 0 {
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

/// The numeric value of `v` as `f64` for the float directives (accepts every
/// numeric `Sexpr` scalar).
fn float_of(heap: &Heap, v: Value) -> Option<f64> {
    use num_traits::ToPrimitive;
    match v {
        Value::Int(n) => Some(n as f64),
        Value::Boxed(id) if heap.is_bignum(id) => heap.bignum_value(id).to_f64(),
        Value::Boxed(id) if heap.is_ratio(id) => heap.ratio_value(id).to_f64(),
        Value::Boxed(id) if !heap.is_struct(id) && !heap.is_enum(id) && !heap.is_hashtable(id)
            && !heap.is_scope(id) && !heap.is_compiled_closure(id) && !heap.is_dyn(id) =>
        {
            Some(heap.float_value(id))
        }
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
    let mut out: String = std::iter::repeat(padchar).take(width - len).collect();
    out.push_str(s);
    out
}

/// `~A`/`~S` padding: `minpad` copies of `padchar` are always added, then the
/// field is grown to `mincol` in `colinc` steps. `at` right-justifies (pad on
/// the left) instead of the default left-justify.
fn pad(s: &str, mincol: usize, colinc: usize, minpad: usize, padchar: char, at: bool) -> String {
    let base: String = std::iter::repeat(padchar).take(minpad).collect();
    let mut content_len = s.chars().count() + minpad;
    let mut extra = 0;
    while content_len < mincol {
        extra += colinc;
        content_len += colinc;
    }
    let fill: String = std::iter::repeat(padchar).take(extra).collect();
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
        std::iter::repeat(padchar).take(n).collect()
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
        if i > 0 && (chars.len() - i) % interval == 0 {
            out.push(commachar);
        }
        out.push(*c);
    }
    out.into_iter().collect()
}

fn trim_float(f: f64) -> String {
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
            if rest % 10 > 0 {
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
    let (prefix, last) = match card.rfind(|c| c == ' ' || c == '-') {
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
/// (bare); the flag threads through nested lists. The runtime-library
/// counterpart of `crate::main`'s `format_sexpr`; live boxed structs/enums are
/// defensive (a program can't normally place one inside a `Sexpr`).
pub(crate) fn render_value(
    heap: &Heap,
    enums: &HashMap<Path, EnumDef>,
    v: Value,
    standard: bool,
    out: &mut String,
) {
    match v {
        Value::Empty => out.push_str("()"),
        Value::Int(n) => out.push_str(&n.to_string()),
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
                out.push_str(heap.string(id));
            }
        }
        Value::Symbol(id) => out.push_str(heap.symbol_name(id)),
        Value::Path(id) => {
            let segs: Vec<&str> = heap.path_segments(id).iter().map(|s| heap.symbol_name(*s)).collect();
            out.push_str(&segs.join("::"));
        }
        Value::Boxed(id) if heap.is_bignum(id) => out.push_str(&heap.bignum_value(id).to_string()),
        Value::Boxed(id) if heap.is_ratio(id) => {
            let r = heap.ratio_value(id);
            out.push_str(&format!("{}/{}", r.numer(), r.denom()));
        }
        Value::Boxed(id) if heap.is_struct(id) => {
            out.push_str(&format!("#<{}", heap.struct_type_name(id)));
            for i in 0..heap.struct_field_count(id) {
                out.push(' ');
                render_value(heap, enums, heap.struct_field(id, i), standard, out);
            }
            out.push('>');
        }
        Value::Boxed(id) if heap.is_enum(id) => {
            let type_path =
                Path::from_segments(heap.enum_type_name(id).split("::").map(|s| s.to_string()).collect());
            let variant = heap.enum_variant(id);
            let name = enums
                .get(&type_path)
                .and_then(|d| d.variants.get(variant))
                .map(|v| v.name.clone())
                .unwrap_or_else(|| "<unknown-variant>".to_string());
            if heap.enum_field_count(id) == 0 {
                out.push_str(&name);
            } else {
                out.push('(');
                out.push_str(&name);
                for i in 0..heap.enum_field_count(id) {
                    out.push(' ');
                    render_value(heap, enums, heap.enum_field(id, i), standard, out);
                }
                out.push(')');
            }
        }
        Value::Boxed(id) if heap.is_hashtable(id) => {
            out.push_str(&format!("#<hashtable count={}>", heap.hashtable_count(id)))
        }
        Value::Boxed(id) if heap.is_scope(id) => {
            out.push_str(&format!("#<scope depth={}>", heap.scope_frame_count(id)))
        }
        Value::Boxed(id) if heap.is_compiled_closure(id) => out.push_str("#<closure>"),
        // A trait object prints as the value it wraps: the box is a dispatch
        // mechanism, not part of the datum. (Must precede the float
        // fall-through, which reads any other `Boxed` as an `f64` — see
        // `float_of`'s matching negative guard.)
        Value::Boxed(id) if heap.is_dyn(id) => render_value(heap, enums, heap.dyn_value(id), standard, out),
        Value::Boxed(id) => out.push_str(&trim_float(heap.float_value(id))),
        Value::Cons(_) => {
            out.push('(');
            let mut cur = v;
            let mut first = true;
            loop {
                match cur {
                    Value::Cons(_) => {
                        if !first {
                            out.push(' ');
                        }
                        first = false;
                        render_value(heap, enums, heap.car(cur).expect("cons car"), standard, out);
                        cur = heap.cdr(cur).expect("cons cdr");
                    }
                    Value::Empty => break,
                    other => {
                        out.push_str(" . ");
                        render_value(heap, enums, other, standard, out);
                        break;
                    }
                }
            }
            out.push(')');
        }
    }
}
