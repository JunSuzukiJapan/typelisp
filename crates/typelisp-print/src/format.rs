//! The Common Lisp `format` directives (CLHS §22.3): the control string's
//! parser, the checks run over the argument types before the program runs
//! ([`call_sites`]), and the renderers the directives write with.
//!
//! A control string is always a literal, so nothing here reads one when the
//! program runs: the checker parses it ([`parse`]) into a [`Node`] tree (so
//! block directives — `~[…~]`, `~{…~}`, `~<…~>`, `~(…~)` — and their clause
//! separators nest properly) and expands the call into calls of the pieces in
//! [`ops`], one per directive (`typeck::format_expand`).
//!
//! ## Coverage
//!
//! Implemented: `~A ~S ~W`, `~D ~B ~O ~X ~R`, `~P`, `~C`, `~F ~E ~G ~$`,
//! `~% ~& ~| ~~`, `~( ~)`, `~[ ~; ~]`, `~:[`, `~{ ~} ~^`, `~< ~> ~T`, `~*`,
//! `~/name/`, the ignored-`~newline`, the pretty-printer directives
//! `~_ ~I ~:T` and the logical-block form of `~<…~:>`, plus the
//! numeric/`'c`/`v`/`#` prefix parameters and the `:`/`@` modifiers each
//! directive gives meaning to.
//!
//! The pretty-printer directives (and the `*print-pretty*` path of
//! `~A`/`~S`/`~W`) record [`pprint::Op`]s on the output buffer rather than
//! emitting text directly; [`crate::pprint`] turns the finished buffer into
//! laid out text. With `*print-pretty*` false nothing records an op.
//!
//! ## Strictness
//!
//! A parameter or modifier a directive does not take, or a value outside its
//! range, is an error when the control string is parsed ([`Head::validate`]);
//! an argument of the wrong type, or a cursor move outside the arguments, is
//! an error when the checker can see it ([`call_sites`]) and otherwise when
//! it is met. `~:[` takes only a `bool`. A `Sexpr` argument of `~D` that is
//! not an integer is printed as `~A` would print it, in decimal (CL).
//! Refused outright: `~?` (its control string arrives at run time, where
//! nothing can check it), `~@[` (it tests for nil, which this language does
//! not have) and the empty-bodied `~{~}`.

use std::collections::{BTreeSet, HashMap};

use typelisp_mem::{FloatBox, Heap, Value};

use crate::pprint::{self, Opts, Out};
use crate::PrintEnv;

pub mod ops;

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
/// Read from the prelude's globals by [`crate::runtime::read_print_vars`].
/// [`Default`] is every limit off, base 10, and symbols as stored: CL's
/// initial state, which the prelude's `defvar`s also start from.
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
    /// `*print-base*`: the radix integers print in, already checked to be in
    /// CL's 2..=36 when the variable was read.
    pub base: u32,
    /// `*print-radix*`: prefix the digits with the radix marker (`#b`/`#o`/
    /// `#x`/`#NNr`, or a trailing `.` in base 10), so the printed form reads
    /// back as the same number whatever `*read-base*` is.
    pub radix: bool,
    /// `*print-readably*`: print so the result reads back as an equal object.
    /// Here that means escapes on and the `*print-level*`/`*print-length*`
    /// cuts off, which is what CL says it overrides. CL additionally signals
    /// `print-not-readable` for values with no readable form; this language
    /// has nothing to signal (no conditions) and no way to decide the question
    /// for a user type whose `print-object` may print anything, so that half
    /// is deliberately absent — see `docs/ja/reference/functions/printing.md` §6.2.
    pub readably: bool,
    /// `*print-array*`: show a vector's elements, as `#(1 2 3)`. When false
    /// a vector prints as `#<Vector<int> 3>` — its type and length, not its
    /// contents. (`Array<T>`'s own `print-object` reads the same variable.)
    pub array: bool,
}

impl Default for PrintVars {
    fn default() -> Self {
        PrintVars {
            circle: false,
            level: None,
            length: None,
            base: 10,
            radix: false,
            readably: false,
            array: true,
        }
    }
}

impl PrintVars {
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


// ===========================================================================
// Parsing
// ===========================================================================

/// A directive prefix parameter (`~mincol,'padchar…`). `Default` is an omitted
/// slot (`~,3D`'s first parameter); `Arg`/`Count` are the `v`/`#` parameters
/// resolved from the argument list at interpretation time.
#[derive(Clone, Debug)]
pub enum Param {
    Int(i64),
    Char(char),
    Arg,
    Count,
    Default,
}

/// A parsed directive's shared header: its prefix parameters and `:`/`@`
/// modifiers, plus what each parameter slot accepts — filled in by
/// [`Head::validate`] once the directive is known.
#[derive(Clone, Debug, Default)]
pub struct Head {
    pub params: Vec<Param>,
    pub colon: bool,
    pub at: bool,
    pub kinds: &'static [ParamKind],
}

/// What one prefix-parameter slot of a directive accepts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    /// An integer `>= 0`.
    NonNeg,
    /// An integer `>= 1`.
    Pos,
    /// Any integer.
    Int,
    /// A character (`'c`).
    Char,
    /// A radix, `2..=36`.
    Radix,
    /// A slot CL defines and this engine does not implement; naming it is an
    /// error rather than a silently ignored value.
    Unsupported(&'static str),
}

impl ParamKind {
    /// Whether an integer value (a literal, or one read through `v`/`#`)
    /// fits this slot.
    pub fn check_int(self, n: i64) -> Result<(), String> {
        match self {
            ParamKind::NonNeg if n < 0 => Err(format!("{} is negative; this parameter must be 0 or more", n)),
            ParamKind::Pos if n < 1 => Err(format!("{} is less than 1; this parameter must be 1 or more", n)),
            ParamKind::Radix if !(2..=36).contains(&n) => Err(format!("radix {} is outside 2..36", n)),
            ParamKind::Char => Err(format!("{} is an integer; this parameter takes a character ('c)", n)),
            ParamKind::Unsupported(name) => Err(format!("the {} parameter is not supported", name)),
            _ => Ok(()),
        }
    }
}

/// The parameter slots and modifiers one directive accepts.
struct Spec {
    kinds: &'static [ParamKind],
    colon: bool,
    at: bool,
    /// Whether `:` and `@` may be given together.
    both: bool,
}

const fn spec(kinds: &'static [ParamKind], colon: bool, at: bool, both: bool) -> Spec {
    Spec { kinds, colon, at, both }
}

use ParamKind::{Char as PChar, Int as PInt, NonNeg, Pos, Radix, Unsupported};

/// What each directive accepts, keyed by its (lowercased) character. The
/// block directives have their own keys: `[` is a plain conditional, `:[` the
/// boolean one, `<` justification, `L` a logical block (`~<…~:>`), and the
/// separators and closers (`;`, `]`, `}`, `)`, `>`) are listed so a parameter
/// written on one of them is refused like any other.
fn directive_spec(key: char) -> Option<Spec> {
    Some(match key {
        'a' | 's' => spec(&[NonNeg, Pos, NonNeg, PChar], false, true, false),
        'w' => spec(&[], false, false, false),
        'd' | 'b' | 'o' | 'x' => spec(&[NonNeg, PChar, PChar, Pos], true, true, true),
        'r' => spec(&[Radix, NonNeg, PChar, PChar, Pos], true, true, true),
        'p' | 'c' => spec(&[], true, true, true),
        'f' => spec(&[NonNeg, NonNeg, PInt, PChar, PChar], false, true, false),
        'e' => spec(
            &[NonNeg, NonNeg, Unsupported("exponent-digits"), Unsupported("scale"), Unsupported("overflowchar"), PChar, PChar],
            false,
            true,
            false,
        ),
        'g' => spec(&[], false, true, false),
        '$' => spec(&[NonNeg, NonNeg, NonNeg, PChar], true, true, true),
        '%' | '&' | '|' | '~' => spec(&[NonNeg], false, false, false),
        't' => spec(&[NonNeg, NonNeg], true, true, true),
        '*' => spec(&[NonNeg], true, true, false),
        '_' => spec(&[], true, true, true),
        'i' => spec(&[PInt], true, false, false),
        '^' => spec(&[PInt, PInt, PInt], false, false, false),
        '\n' => spec(&[], true, true, false),
        '(' => spec(&[], true, true, true),
        '[' => spec(&[PInt], true, false, false),
        '{' => spec(&[NonNeg], true, true, true),
        '<' => spec(&[NonNeg, Pos, NonNeg, PChar], true, true, true),
        'L' => spec(&[], true, true, true),
        '/' => spec(&[], true, true, true),
        ';' => spec(&[], true, true, false),
        '}' => spec(&[], true, false, false),
        '>' => spec(&[], true, false, false),
        ']' | ')' => spec(&[], false, false, false),
        _ => return None,
    })
}

impl Head {
    /// Checks this header against what directive `key` accepts (see
    /// [`directive_spec`]) and records the slot kinds for interpretation.
    /// `shown` is how the directive is named in the error.
    fn validate(&mut self, key: char, shown: &str) -> Result<(), String> {
        let Some(spec) = directive_spec(key) else {
            return Err(format!("format: unknown directive ~{}", shown));
        };
        if self.colon && !spec.colon {
            return Err(format!("format: ~{} does not take the `:` modifier", shown));
        }
        if self.at && !spec.at {
            return Err(format!("format: ~{} does not take the `@` modifier", shown));
        }
        if self.colon && self.at && !spec.both {
            return Err(format!("format: ~{} does not take `:` and `@` together", shown));
        }
        if self.params.len() > spec.kinds.len() {
            return Err(format!(
                "format: ~{} takes {} parameter{}, and {} were given",
                shown,
                spec.kinds.len(),
                if spec.kinds.len() == 1 { "" } else { "s" },
                self.params.len()
            ));
        }
        for (i, (p, kind)) in self.params.iter().zip(spec.kinds).enumerate() {
            let bad = |why: String| format!("format: ~{}, parameter {}: {}", shown, i + 1, why);
            match (p, kind) {
                (Param::Default, _) => {}
                (_, ParamKind::Unsupported(name)) => return Err(bad(format!("the {} parameter is not supported", name))),
                (Param::Int(n), k) => k.check_int(*n).map_err(bad)?,
                (Param::Char(_), ParamKind::Char) => {}
                (Param::Char(c), _) => return Err(bad(format!("'{} is a character; this parameter takes an integer", c))),
                (Param::Count, ParamKind::Char) => return Err(bad("`#` is a count; this parameter takes a character".to_string())),
                (Param::Count, _) | (Param::Arg, _) => {}
            }
        }
        self.kinds = spec.kinds;
        Ok(())
    }
}

#[derive(Debug)]
pub enum Node {
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
    /// `~/name/` — CL's function-call directive. The checker resolves `name`
    /// as a call `(name arg colon at)` at the site and hands the engine a
    /// control string whose name is what it resolved to ([`call_sites`],
    /// [`rename_calls`]); see [`crate::PrintEnv::format_call`]. `span` is
    /// where the name sits in the control string, in chars.
    Call { head: Head, name: String, span: (usize, usize) },
    /// `~^` — escape upward out of the nearest `~{`/`~<` (or the whole op).
    Escape { head: Head },
}

/// The control directive a [`parse_seq`] run stopped on (a block closer or a
/// `~;` separator), with its own modifiers/parameters.
struct Stop {
    ch: char,
    head: Head,
}

pub fn parse(control: &str) -> Result<Vec<Node>, String> {
    let chars: Vec<char> = control.chars().collect();
    let mut pos = 0;
    let (nodes, stop) = parse_seq(&chars, &mut pos, &[])?;
    if let Some(s) = stop {
        return Err(format!("format: unmatched ~{}", s.ch));
    }
    Ok(nodes)
}

/// Every `~/name/` directive's node in `nodes`, in source order.
pub fn call_nodes(nodes: &[Node]) -> Vec<&Node> {
    fn walk<'a>(nodes: &'a [Node], out: &mut Vec<&'a Node>) {
        for n in nodes {
            match n {
                Node::Call { .. } => out.push(n),
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
    walk(nodes, &mut out);
    out
}

/// Where a `~/name/` directive can take its argument from.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum CallArg {
    /// The call's own argument at this index.
    Arg(usize),
    /// An element of a list argument (`~{`, `~:{`, `~:@{`, `~<…~:>`), whose
    /// type the call's argument types do not say.
    Element,
}

/// One `~/name/` directive of a control string.
#[derive(Debug)]
pub struct CallSite {
    pub name: String,
    pub colon: bool,
    pub at: bool,
    /// Every argument the cursor can hand it. Empty when no path through the
    /// control string reaches it.
    pub args: BTreeSet<CallArg>,
}

/// Checks the arguments of a `format`/`print`/`println` call against its
/// literal control string, before the program runs — that every directive
/// that consumes an argument has one, of a type it accepts — and returns its
/// `~/name/` directives in source order, each with the arguments it can be
/// handed.
///
/// The argument cursor is followed through the whole directive language. Where
/// the path depends on a value only known at run time — which `~[` clause is
/// chosen, whether a `~^` fires, how many times a `~@{` goes round — every
/// possibility is followed, so a mistake on any of them is reported, and a
/// `~/name/` learns every argument it can meet. The elements of a list
/// argument (`~{`, `~:{`, `~<…~:>`) are `Sexpr`s whose count the type does not
/// say; directives consuming them are checked when the elements arrive.
///
/// Extra arguments are allowed, as in CL.
///
/// What a `~/name/` calls is the checker's to decide: it resolves the name as
/// a call at the site, and hands the engine the control string with the names
/// replaced by what they resolved to ([`rename_calls`]).
pub fn call_sites(control: &str, args: &[ArgType]) -> Result<Vec<CallSite>, String> {
    let nodes = parse(control)?;
    let sim = Sim::default();
    sim.seq(Frame::Static(args, 0), &nodes, &BTreeSet::from([Cur::At(0)]))?;
    let met = sim.calls.into_inner();
    Ok(call_nodes(&nodes)
        .into_iter()
        .map(|n| match n {
            Node::Call { head, name, span } => CallSite {
                name: name.clone(),
                colon: head.colon,
                at: head.at,
                args: met.get(&span.0).cloned().unwrap_or_default(),
            },
            _ => unreachable!("call_nodes returns `~/name/` nodes only"),
        })
        .collect())
}

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
        let mut head = parse_head(chars, pos)?;
        let Some(&ch) = chars.get(*pos) else {
            return Err("format: control string ends with a lone ~".to_string());
        };
        *pos += 1;
        let lower = ch.to_ascii_lowercase();
        if stops.contains(&lower) {
            head.validate(lower, &lower.to_string())?;
            return Ok((nodes, Some(Stop { ch: lower, head })));
        }
        match lower {
            '(' => {
                head.validate('(', "(")?;
                let (body, stop) = parse_seq(chars, pos, &[')'])?;
                match stop {
                    Some(s) if s.ch == ')' => nodes.push(Node::Case { head, body }),
                    _ => return Err("format: unterminated ~(".to_string()),
                }
            }
            '[' => {
                if head.at {
                    return Err("format: ~@[ is not supported: it tests its argument for being non-nil, and this \
                                language has no nil. Choose with a bool through ~:[false~;true~] instead."
                        .to_string());
                }
                head.validate('[', "[")?;
                if head.colon && !head.params.is_empty() {
                    return Err("format: ~:[ chooses by its bool argument and takes no parameter".to_string());
                }
                nodes.push(parse_cond(chars, pos, head)?)
            }
            '?' => {
                return Err("format: ~? is not supported: it takes its control string at run time, where \
                            nothing can check the arguments it consumes. Write those directives into this \
                            control string instead."
                    .to_string())
            }
            '{' => {
                head.validate('{', "{")?;
                let (body, stop) = parse_seq(chars, pos, &['}'])?;
                match stop {
                    Some(_) if body.is_empty() => {
                        return Err("format: ~{~} with an empty body takes its directives from an argument \
                                    at run time, which is not supported; write them inside the braces"
                            .to_string())
                    }
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
                let start = *pos;
                let end = loop {
                    match chars.get(*pos) {
                        None => return Err("format: unterminated ~/ — the name needs a closing `/`".to_string()),
                        Some('/') => {
                            *pos += 1;
                            break *pos - 1;
                        }
                        Some(c) => {
                            name.push(*c);
                            *pos += 1;
                        }
                    }
                };
                if name.is_empty() {
                    return Err("format: ~// names nothing to call".to_string());
                }
                head.validate('/', &format!("/{}/", name))?;
                nodes.push(Node::Call { head, name, span: (start, end) });
            }
            '^' => {
                head.validate('^', "^")?;
                nodes.push(Node::Escape { head })
            }
            '\n' => {
                head.validate('\n', "<newline>")?;
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
            _ => {
                head.validate(lower, &ch.to_string())?;
                if lower == 'r' && matches!(head.params.first(), Some(Param::Default)) {
                    return Err("format: ~r with parameters needs the radix as its first one".to_string());
                }
                nodes.push(Node::Dir { head, ch: lower })
            }
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
                if s.head.at {
                    return Err("format: ~@; separates the prefix of a logical block, not a ~[ clause".to_string());
                }
                if s.head.colon {
                    default = Some(clauses.len()); // the *next* clause
                }
            }
            Some(s) if s.ch == ']' => break,
            _ => return Err("format: unterminated ~[".to_string()),
        }
    }
    if head.colon && (clauses.len() != 2 || default.is_some()) {
        return Err("format: ~:[ has exactly two clauses, false and true: ~:[false~;true~]".to_string());
    }
    if default.is_some_and(|d| d + 1 != clauses.len()) {
        return Err("format: ~:; marks the default clause, which has to be the last one".to_string());
    }
    Ok(Node::Cond { head, clauses, default })
}

/// `~<…~;…~>`: segments separated by `~;`. A closing `~:>` (rather than `~>`)
/// makes the whole directive a logical block instead of a justification.
fn parse_just(chars: &[char], pos: &mut usize, mut head: Head) -> Result<Node, String> {
    let mut segments = Vec::new();
    let mut sep_at = Vec::new();
    let block = loop {
        let (body, stop) = parse_seq(chars, pos, &[';', '>'])?;
        segments.push(body);
        match stop {
            Some(s) if s.ch == ';' => {
                if s.head.colon {
                    return Err("format: ~:; (a line-overflow segment) is not supported".to_string());
                }
                sep_at.push(s.head.at);
            }
            Some(s) if s.ch == '>' => break s.head.colon,
            _ => return Err("format: unterminated ~<".to_string()),
        }
    };
    if !block {
        if sep_at.contains(&true) {
            return Err("format: ~@; marks a per-line prefix, which only a logical block (~<…~:>) has".to_string());
        }
        head.validate('<', "<")?;
        return Ok(Node::Just { head, segments });
    }
    head.validate('L', "<…~:>")?;
    if segments.len() > 3 {
        return Err("format: a logical block ~<…~:> has at most three segments: prefix, body, suffix".to_string());
    }
    if sep_at.iter().skip(1).any(|&at| at) {
        return Err("format: only the separator after a logical block's prefix can be ~@;".to_string());
    }
    // The prefix and suffix are literal text; see [`literal_segment`].
    if segments.len() >= 2 {
        literal_segment(&segments[0])?;
    }
    if segments.len() == 3 {
        literal_segment(&segments[2])?;
    }
    Ok(Node::Block { head, segments, sep_at })
}

/// The literal text of a `~<…~:>` prefix/suffix segment. CL requires these to
/// be literal (they are re-emitted at the start of every line of a per-line
/// block, and their width is what the block indents past), so a directive
/// inside one is an error rather than something to interpret per use.
pub fn literal_segment(nodes: &[Node]) -> Result<String, String> {
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
// Static checking
// ===========================================================================

/// What the checker knows about one argument's type, for [`call_sites`].
#[derive(Clone, Debug)]
pub struct ArgType {
    pub kind: ArgKind,
    /// The type as the user would write it, for messages.
    pub shown: String,
}

/// The classes of argument type the directives distinguish.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArgKind {
    /// Any integer type.
    Integer,
    /// A real that is not an integer: `ratio`, `f32`, `f64`.
    Real,
    Char,
    Bool,
    Str,
    /// `Option<Sexpr>`, the type of a list.
    List,
    /// `Sexpr`: it may hold anything, so a directive's demand on it is checked
    /// when the value arrives.
    Sexpr,
    /// A type variable of a generic body that is not specialized yet. The
    /// specialization is checked with the real type.
    Unknown,
    /// Any other type. Only the directives that print any value accept it.
    Other,
}

/// What a directive demands of the argument it consumes.
#[derive(Clone, Copy, PartialEq)]
enum Need {
    Any,
    Integer,
    Number,
    Char,
    Bool,
    List,
}

impl Need {
    fn describe(self) -> &'static str {
        match self {
            Need::Any => "a value",
            Need::Integer => "an integer",
            Need::Number => "a number",
            Need::Char => "a char",
            Need::Bool => "a bool",
            Need::List => "a list (`Option<Sexpr>`)",
        }
    }

    fn accepts(self, kind: ArgKind) -> bool {
        match (self, kind) {
            (_, ArgKind::Unknown | ArgKind::Sexpr) | (Need::Any, _) => true,
            (Need::Integer, k) => k == ArgKind::Integer,
            (Need::Number, k) => matches!(k, ArgKind::Integer | ArgKind::Real),
            (Need::Char, k) => k == ArgKind::Char,
            (Need::Bool, k) => k == ArgKind::Bool,
            (Need::List, k) => k == ArgKind::List,
        }
    }
}

/// The argument list a stretch of directives consumes from: the call's own
/// arguments (or a known slice of them, with the index of its first in the
/// whole call), or the elements of a list argument.
#[derive(Clone, Copy)]
enum Frame<'a> {
    Static(&'a [ArgType], usize),
    Elements,
}

/// Where the argument cursor can be. `Elements` is anywhere in a list whose
/// length is unknown.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Cur {
    At(usize),
    Elements,
}

type Curs = BTreeSet<Cur>;

/// Where a stretch of directives can leave the cursor: by running to its end,
/// or by a `~^` that fired.
#[derive(Default)]
struct Exits {
    normal: Curs,
    escaped: Curs,
}

/// A prefix parameter's value as the check sees it.
#[derive(Clone, Copy)]
enum PVal {
    Omitted,
    Known(i64),
    Unknown,
}

#[derive(Default)]
struct Sim {
    /// What each `~/name/` met, by where its name starts in the control
    /// string.
    calls: std::cell::RefCell<std::collections::BTreeMap<usize, BTreeSet<CallArg>>>,
}

impl Sim {
    /// Consumes one argument at `cur` for `what`, which demands `need`.
    fn consume(&self, frame: Frame, cur: Cur, need: Need, what: &str) -> Result<Cur, String> {
        match (frame, cur) {
            (Frame::Static(args, _), Cur::At(p)) => {
                let Some(arg) = args.get(p) else {
                    return Err(format!(
                        "format: {} has no argument left to consume ({} {} given)",
                        what,
                        args.len(),
                        if args.len() == 1 { "is" } else { "are" }
                    ));
                };
                if !need.accepts(arg.kind) {
                    return Err(format!(
                        "format: {} needs {}, but argument {} is `{}`",
                        what,
                        need.describe(),
                        p + 1,
                        arg.shown
                    ));
                }
                Ok(Cur::At(p + 1))
            }
            _ => Ok(Cur::Elements),
        }
    }

    /// The values of `head`'s prefix parameters, consuming the arguments its
    /// `v` parameters read.
    fn params(&self, frame: Frame, mut cur: Cur, head: &Head, what: &str) -> Result<(Cur, Vec<PVal>), String> {
        let mut vals = Vec::new();
        for (p, kind) in head.params.iter().zip(head.kinds) {
            vals.push(match p {
                Param::Int(n) => PVal::Known(*n),
                Param::Char(c) => PVal::Known(*c as i64),
                Param::Default => PVal::Omitted,
                Param::Count => match (frame, cur) {
                    (Frame::Static(args, _), Cur::At(i)) => {
                        let n = (args.len() - i) as i64;
                        kind.check_int(n).map_err(|why| format!("format: {}'s `#` parameter: {}", what, why))?;
                        PVal::Known(n)
                    }
                    _ => PVal::Unknown,
                },
                Param::Arg => {
                    let need = if *kind == ParamKind::Char { Need::Char } else { Need::Integer };
                    cur = self.consume(frame, cur, need, &format!("{}'s `v` parameter", what))?;
                    PVal::Unknown
                }
            });
        }
        Ok((cur, vals))
    }

    fn seq(&self, frame: Frame, nodes: &[Node], starts: &Curs) -> Result<Exits, String> {
        let mut exits = Exits { normal: starts.clone(), escaped: Curs::new() };
        for node in nodes {
            let mut next = Exits { normal: Curs::new(), escaped: exits.escaped };
            for &cur in &exits.normal {
                let e = self.node(frame, node, cur)?;
                next.normal.extend(e.normal);
                next.escaped.extend(e.escaped);
            }
            exits = next;
        }
        Ok(exits)
    }

    fn node(&self, frame: Frame, node: &Node, cur: Cur) -> Result<Exits, String> {
        let normal = |c: Cur| Exits { normal: Curs::from([c]), escaped: Curs::new() };
        match node {
            Node::Text(_) => Ok(normal(cur)),
            Node::Dir { head, ch } => self.dir(frame, head, *ch, cur).map(normal),
            Node::Call { head, name, span } => {
                let what = format!("~/{}/", name);
                let (cur, _) = self.params(frame, cur, head, &what)?;
                let arg = match (frame, cur) {
                    (Frame::Static(_, base), Cur::At(p)) => CallArg::Arg(base + p),
                    _ => CallArg::Element,
                };
                self.calls.borrow_mut().entry(span.0).or_default().insert(arg);
                self.consume(frame, cur, Need::Any, &what).map(normal)
            }
            Node::Case { body, .. } => self.seq(frame, body, &Curs::from([cur])),
            Node::Escape { head } => {
                let (cur, vals) = self.params(frame, cur, head, "~^")?;
                let known: Option<Vec<i64>> = vals
                    .iter()
                    .filter(|v| !matches!(v, PVal::Omitted))
                    .map(|v| match v {
                        PVal::Known(n) => Some(*n),
                        _ => None,
                    })
                    .collect();
                let fires = match (known, frame, cur) {
                    (Some(present), _, _) if !present.is_empty() => Some(match present.len() {
                        1 => present[0] == 0,
                        2 => present[0] == present[1],
                        _ => present[0] <= present[1] && present[1] <= present[2],
                    }),
                    (Some(_), Frame::Static(args, _), Cur::At(p)) => Some(p == args.len()),
                    _ => None,
                };
                let mut e = Exits::default();
                if fires != Some(false) {
                    e.escaped.insert(cur);
                }
                if fires != Some(true) {
                    e.normal.insert(cur);
                }
                Ok(e)
            }
            Node::Cond { head, clauses, default } => {
                let (selected, cur) = if head.colon {
                    (None, self.consume(frame, cur, Need::Bool, "~:[")?)
                } else if head.params.is_empty() {
                    (None, self.consume(frame, cur, Need::Integer, "~[")?)
                } else {
                    let (cur, vals) = self.params(frame, cur, head, "~[")?;
                    let sel = match vals[0] {
                        PVal::Known(n) => Some(n),
                        _ => None,
                    };
                    (sel, cur)
                };
                let start = Curs::from([cur]);
                let mut e = Exits::default();
                let mut run = |clause: &Vec<Node>| -> Result<(), String> {
                    let r = self.seq(frame, clause, &start)?;
                    e.normal.extend(r.normal);
                    e.escaped.extend(r.escaped);
                    Ok(())
                };
                match selected {
                    Some(n) => {
                        let idx = usize::try_from(n).ok().filter(|&i| i < clauses.len() && Some(i) != *default);
                        match idx.or(*default) {
                            Some(i) => run(&clauses[i])?,
                            None => {
                                e.normal.insert(cur);
                            }
                        }
                    }
                    None => {
                        for clause in clauses {
                            run(clause)?;
                        }
                        // Without a default clause, a selector matching no
                        // clause runs none.
                        if default.is_none() && !head.colon {
                            e.normal.insert(cur);
                        }
                    }
                }
                Ok(e)
            }
            Node::Iter { head, body, close_colon } => {
                let (cur, vals) = self.params(frame, cur, head, "~{")?;
                self.iter(frame, head, body, *close_colon, vals.first().copied().unwrap_or(PVal::Omitted), cur)
                    .map(normal_set)
            }
            Node::Just { head, segments } => {
                let (cur, _) = self.params(frame, cur, head, "~<")?;
                // The segments share the cursor; a `~^` in one skips the rest.
                let mut e = Exits { normal: Curs::from([cur]), escaped: Curs::new() };
                let mut done = Curs::new();
                for seg in segments {
                    let r = self.seq(frame, seg, &e.normal)?;
                    done.extend(r.escaped);
                    e.normal = r.normal;
                }
                e.normal.extend(done);
                Ok(e)
            }
            Node::Block { head, segments, .. } => {
                let body = if segments.len() == 1 { &segments[0] } else { &segments[1] };
                if head.at {
                    let r = self.seq(frame, body, &Curs::from([cur]))?;
                    Ok(Exits { normal: r.normal.into_iter().chain(r.escaped).collect(), escaped: Curs::new() })
                } else {
                    let cur = self.consume(frame, cur, Need::List, "~<…~:>")?;
                    self.seq(Frame::Elements, body, &Curs::from([Cur::Elements]))?;
                    Ok(normal(cur))
                }
            }
        }
    }

    fn dir(&self, frame: Frame, head: &Head, ch: char, cur: Cur) -> Result<Cur, String> {
        let what = format!("~{}", ch);
        let (cur, vals) = self.params(frame, cur, head, &what)?;
        let need = match ch {
            'a' | 's' | 'w' => Need::Any,
            // CL prints a non-integer argument to these as `~a` would, in
            // decimal, so any value is accepted. `~r` without a radix (the
            // English and Roman spellings) has no such fallback.
            'd' | 'b' | 'o' | 'x' => Need::Any,
            'r' if !head.params.is_empty() => Need::Any,
            'r' => Need::Integer,
            'c' => Need::Char,
            'f' | 'e' | 'g' | '$' => Need::Number,
            'p' => {
                let cur = if head.colon {
                    match cur {
                        Cur::At(0) => {
                            return Err("format: ~:p reuses the previous argument, and there is none".to_string())
                        }
                        Cur::At(p) => Cur::At(p - 1),
                        Cur::Elements => Cur::Elements,
                    }
                } else {
                    cur
                };
                return self.consume(frame, cur, Need::Integer, &what);
            }
            '*' => return self.goto(frame, head, vals.first().copied().unwrap_or(PVal::Omitted), cur),
            _ => return Ok(cur),
        };
        self.consume(frame, cur, need, &what)
    }

    /// `~*`, `~:*`, `~@*`: moves the cursor.
    fn goto(&self, frame: Frame, head: &Head, n: PVal, cur: Cur) -> Result<Cur, String> {
        let (Frame::Static(args, _), Cur::At(p)) = (frame, cur) else { return Ok(Cur::Elements) };
        let n = match n {
            PVal::Known(n) => n,
            PVal::Omitted if head.at => 0,
            PVal::Omitted => 1,
            PVal::Unknown => {
                return Err("format: ~v* moves the argument cursor by an amount known only at run time, \
                            so the directives after it cannot be checked"
                    .to_string())
            }
        };
        let target = if head.at {
            n
        } else if head.colon {
            p as i64 - n
        } else {
            p as i64 + n
        };
        if target < 0 || target as usize > args.len() {
            return Err(format!("format: ~* moves to argument {}, outside the {} given", target, args.len()));
        }
        Ok(Cur::At(target as usize))
    }

    /// Every way `~{…~}` (any of its four forms) can leave the cursor.
    fn iter(&self, frame: Frame, head: &Head, body: &[Node], close_colon: bool, max: PVal, cur: Cur) -> Result<Curs, String> {
        let empty = Frame::Static(&[], 0);
        match (head.colon, head.at) {
            // One list argument, whose elements the body consumes.
            (false, false) | (true, false) => {
                let what = if head.colon { "~:{" } else { "~{" };
                let after = self.consume(frame, cur, Need::List, what)?;
                self.seq(Frame::Elements, body, &Curs::from([Cur::Elements]))?;
                // `~:}` runs the body once even for an empty list, with no
                // arguments at all.
                if close_colon {
                    self.seq(empty, body, &Curs::from([Cur::At(0)]))?;
                }
                Ok(Curs::from([after]))
            }
            // `~:@{`: every remaining argument is a list.
            (true, true) => match (frame, cur) {
                (Frame::Static(args, _), Cur::At(p)) => {
                    for (i, arg) in args.iter().enumerate().skip(p) {
                        if !Need::List.accepts(arg.kind) {
                            return Err(format!(
                                "format: ~:@{{ takes each remaining argument as a list, but argument {} is `{}`",
                                i + 1,
                                arg.shown
                            ));
                        }
                    }
                    if p < args.len() {
                        self.seq(Frame::Elements, body, &Curs::from([Cur::Elements]))?;
                    } else if close_colon {
                        self.seq(empty, body, &Curs::from([Cur::At(0)]))?;
                    }
                    Ok(Curs::from([Cur::At(args.len())]))
                }
                _ => {
                    self.seq(Frame::Elements, body, &Curs::from([Cur::Elements]))?;
                    if close_colon {
                        self.seq(empty, body, &Curs::from([Cur::At(0)]))?;
                    }
                    Ok(Curs::from([Cur::Elements]))
                }
            },
            // `~@{`: the body goes round over the remaining arguments.
            (false, true) => match (frame, cur) {
                (Frame::Static(args, base), Cur::At(p)) => {
                    let rest = &args[p..];
                    let ends = self.rounds(Frame::Static(rest, base + p), body, close_colon, max)?;
                    Ok(ends
                        .into_iter()
                        .map(|c| match c {
                            Cur::At(i) => Cur::At(p + i),
                            Cur::Elements => Cur::Elements,
                        })
                        .collect())
                }
                _ => {
                    self.seq(Frame::Elements, body, &Curs::from([Cur::Elements]))?;
                    Ok(Curs::from([Cur::Elements]))
                }
            },
        }
    }

    /// `~@{`'s loop over a known argument list: where it can stop.
    fn rounds(&self, frame: Frame, body: &[Node], close_colon: bool, max: PVal) -> Result<Curs, String> {
        let Frame::Static(args, _) = frame else { unreachable!("`~@{{` over the call's own arguments") };
        if args.is_empty() {
            if close_colon {
                self.seq(frame, body, &Curs::from([Cur::At(0)]))?;
            }
            return Ok(Curs::from([Cur::At(0)]));
        }
        let mut ends = Curs::new();
        let mut frontier = Curs::from([Cur::At(0)]);
        let mut seen = frontier.clone();
        let mut round: i64 = 0;
        while !frontier.is_empty() {
            let mut next = Curs::new();
            for &cur in &frontier {
                let Cur::At(p) = cur else { unreachable!("a known argument list has known positions") };
                if p >= args.len() {
                    ends.insert(cur);
                    continue;
                }
                match max {
                    PVal::Known(k) if round >= k => {
                        ends.insert(cur);
                        continue;
                    }
                    PVal::Unknown => {
                        ends.insert(cur);
                    }
                    _ => {}
                }
                let r = self.seq(frame, body, &Curs::from([cur]))?;
                ends.extend(r.escaped);
                for n in r.normal {
                    if n == cur && matches!(max, PVal::Omitted) {
                        return Err(
                            "format: an iteration of ~@{ can consume no argument, so it would never end".to_string()
                        );
                    }
                    next.insert(n);
                }
            }
            round += 1;
            frontier = match max {
                // A bounded loop may revisit a position on a later round.
                PVal::Known(_) => next,
                _ => next.difference(&seen).copied().collect(),
            };
            seen.extend(frontier.iter().copied());
        }
        Ok(ends)
    }
}

fn normal_set(curs: Curs) -> Exits {
    Exits { normal: curs, escaped: Curs::new() }
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

/// How `prin1` writes the float `f` (CLHS 22.1.3.1.3): `fixed`, its text
/// with a decimal point, when it is zero or its magnitude is at least 10^-3
/// and under 10^7, else `exponent`, its shortest text in exponent notation
/// (`1.5e300`), given a decimal point when its mantissa has none (`1.0e7`).
/// What is not finite has one text either way.
fn float_text(f: f64, fixed: String, exponent: String) -> String {
    let magnitude = f.abs();
    if !f.is_finite() || f == 0.0 || (1e-3..1e7).contains(&magnitude) {
        return fixed;
    }
    match exponent.split_once('e') {
        Some((mantissa, exp)) if !mantissa.contains('.') => format!("{}.0e{}", mantissa, exp),
        _ => exponent,
    }
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
        // An `i64` has at most seven groups, and `SCALES` names all seven.
        let scale = SCALES[i];
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

/// Writes `v` to `out` when it is a number, as `~a`/`prin1` print one (a
/// number prints the same escaped or not). False, with nothing written, for
/// anything else.
///
/// Its own function, apart from [`Renderer`], so that a program printing only
/// numbers and strings links none of the renderer for other values.
pub(crate) fn render_number(heap: &Heap, ctx: RenderCtx<'_>, v: Value, out: &mut String) -> Result<bool, String> {
    match v {
        Value::Int(n) => {
            out.push_str(&Renderer::integer_text(ctx, || n.to_string(), |r| int_to_radix(n, r))?);
        }
        Value::Boxed(id) if heap.is_bignum(id) => {
            let n = heap.bignum_value(id).clone();
            out.push_str(&Renderer::integer_text(ctx, || n.to_string(), |r| n.to_str_radix(r))?);
        }
        Value::Boxed(id) if heap.is_ratio(id) => {
            // A `ratio` whose denominator is 1 is an integer to CL's
            // printer (`(+ 1/3 2/3)` prints `1`). Otherwise both parts
            // go in `*print-base*`; under `*print-radix*` a decimal ratio
            // takes `#10r`, since the trailing `.` is integer-only syntax.
            let r = heap.ratio_value(id).clone();
            let (n, d) = (r.numer(), r.denom());
            let text = if num_traits::One::is_one(d) {
                Renderer::integer_text(ctx, || n.to_string(), |b| n.to_str_radix(b))?
            } else {
                let base = ctx.print_vars.base;
                let body = format!("{}/{}", n.to_str_radix(base), d.to_str_radix(base));
                match (ctx.print_vars.radix, base) {
                    (false, _) => body,
                    (true, 2) => format!("#b{}", body),
                    (true, 8) => format!("#o{}", body),
                    (true, 16) => format!("#x{}", body),
                    (true, b) => format!("#{}r{}", b, body),
                }
            };
            out.push_str(&text);
        }
        // A narrow integer prints as the number it is — the same text an
        // `i32` of that value gives, `*print-base*`/`*print-radix*`
        // included. The box exists so the *type* survives into a `Sexpr`,
        // not to make the number print differently.
        Value::Boxed(id) if heap.narrow_box(id).is_some() => {
            let n = heap.narrow_box(id).expect("just tested").value;
            out.push_str(&Renderer::integer_text(ctx, || n.to_string(), |r| int_to_radix(n, r))?);
        }
        // Positively a float box, as in `float_of`: a fall-through on any
        // other box would turn every unhandled box kind into a float.
        Value::Boxed(id) if heap.float_box(id).is_some() => match heap.float_box(id) {
            Some(FloatBox::F32(f)) => out.push_str(&float_text(f64::from(f), trim_f32(f), format!("{:e}", f))),
            Some(FloatBox::F64(f)) => out.push_str(&float_text(f, trim_f64(f), format!("{:e}", f))),
            None => unreachable!("guarded by float_box above"),
        },
        _ => return Ok(false),
    }
    Ok(true)
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
/// `#<HashTable<K,V> count=N>` without recursing, so they can neither cycle nor
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

    /// A `Sexpr` `array` datum (`#nA(..)`): `#(..)` at rank 1, `#nA(..)` at
    /// any other rank — what `Array<T>`'s prelude `print-object` writes — or
    /// just the shape, `#<array 2x3>`, with `*print-array*` off.
    fn render_data_array(
        &mut self,
        heap: &mut Heap,
        ctx: RenderCtx<'_>,
        id: typelisp_mem::BoxId,
        standard: bool,
        depth: usize,
        out: &mut String,
    ) -> Result<(), String> {
        let (Value::Boxed(dims_id), Value::Boxed(data_id)) = (heap.struct_field(id, 0), heap.struct_field(id, 1)) else {
            return Err("internal error: an array datum's dims/data are not boxes".to_string());
        };
        let dims: Vec<usize> = (0..heap.struct_field_count(dims_id))
            .map(|i| match heap.struct_field(dims_id, i) {
                Value::Int(n) => Ok(n as usize),
                other => Err(format!("internal error: an array dimension is {:?}", other)),
            })
            .collect::<Result<_, _>>()?;
        if !ctx.print_vars.array {
            let shape: Vec<String> = dims.iter().map(|d| d.to_string()).collect();
            out.push_str(&if shape.is_empty() { "#<array>".to_string() } else { format!("#<array {}>", shape.join("x")) });
            return Ok(());
        }
        if dims.len() == 1 {
            out.push('#');
        } else {
            out.push_str(&format!("#{}A", dims.len()));
        }
        let data_ty = crate::stored_type(heap, data_id).expect("a struct box has a type");
        let mut next = 0usize;
        self.render_array_level(heap, ctx, data_id, data_ty, &dims, 0, &mut next, standard, depth, out)
    }

    /// One level of [`Self::render_data_array`]'s nesting: the element at
    /// `*next` (row-major) at the last level, a parenthesized row above it.
    #[allow(clippy::too_many_arguments)]
    fn render_array_level(
        &mut self,
        heap: &mut Heap,
        ctx: RenderCtx<'_>,
        data_id: typelisp_mem::BoxId,
        data_ty: typelisp_mem::TypeId,
        dims: &[usize],
        level: usize,
        next: &mut usize,
        standard: bool,
        depth: usize,
        out: &mut String,
    ) -> Result<(), String> {
        if level == dims.len() {
            let i = *next;
            *next += 1;
            let f = heap.struct_field(data_id, i);
            return self.render_field(heap, ctx, data_ty, None, i, f, standard, depth + 1, out);
        }
        out.push('(');
        let stride: usize = dims[level + 1..].iter().product();
        for i in 0..dims[level] {
            if Self::length_reached(ctx, i) {
                out.push_str(if i == 0 { "..." } else { " ..." });
                // The cells this row did not print are still behind `next`.
                *next += (dims[level] - i) * stride;
                break;
            }
            if i > 0 {
                out.push(' ');
            }
            self.render_array_level(heap, ctx, data_id, data_ty, dims, level + 1, next, standard, depth, out)?;
        }
        out.push(')');
        Ok(())
    }

    /// One field of a struct or enum box: rendered as the value it is —
    /// unless the program's definition says the field is a niche-represented
    /// `Option` (`PrintEnv::field_is_niched_option`), in which case the word
    /// is that `Option`, and prints as `None` or `(Some ...)` around the
    /// payload exactly as a boxed one would. The box is the only place a
    /// niched `Option` can sit with nothing static to say what it is, so
    /// this is the only place the question is asked.
    #[allow(clippy::too_many_arguments)]
    fn render_field(
        &mut self,
        heap: &mut Heap,
        ctx: RenderCtx<'_>,
        ty: typelisp_mem::TypeId,
        variant: Option<usize>,
        index: usize,
        f: Value,
        standard: bool,
        depth: usize,
        out: &mut String,
    ) -> Result<(), String> {
        if !ctx.env.field_is_niched_option(ty, variant, index)? {
            return self.render(heap, ctx, f, standard, depth, out);
        }
        if f == Value::Empty {
            out.push_str("None");
            return Ok(());
        }
        out.push_str("(Some ");
        self.render(heap, ctx, f, standard, depth + 1, out)?;
        out.push(')');
        Ok(())
    }

    /// Renders `v` at nesting `depth` the flat (non-pretty) way.
    pub(crate) fn render(
        &mut self,
        heap: &mut Heap,
        ctx: RenderCtx<'_>,
        v: Value,
        standard: bool,
        depth: usize,
        out: &mut String,
    ) -> Result<(), String> {
        typelisp_mem::grow_stack(|| self.render_here(heap, ctx, v, standard, depth, out))
    }

    /// [`Self::render`] on the stack it is given.
    fn render_here(
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

    /// A symbol as CL's printer writes it. With escaping (`~s`, `prin1`), a
    /// keyword gets its colon and a name the reader would take for something
    /// else goes between `|`s; without (`~a`, `princ`), the name alone.
    fn symbol_text(id: typelisp_mem::SymRef, standard: bool) -> String {
        let name = id.name();
        if id.is_keyword() {
            return if standard { format!(":{}", name) } else { name.to_string() };
        }
        if standard && Self::name_needs_bars(name) {
            return format!("|{}|", name);
        }
        name.to_string()
    }

    /// Whether `name`, written bare, would read back as something other than
    /// the plain symbol: a keyword (leading colon), a number, a boolean, more
    /// than one token, or nothing at all.
    fn name_needs_bars(name: &str) -> bool {
        name.is_empty()
            || name.starts_with(':')
            || name.chars().any(|c| c.is_whitespace() || "()[]{}\"'`;|".contains(c))
            || matches!(name, "true" | "false")
            || name.parse::<f64>().is_ok()
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
        let radix = ctx.print_vars.base;
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
        if render_number(heap, ctx, v, out)? {
            return Ok(());
        }
        match v {
            Value::Int(_) => unreachable!("render_number wrote it above"),
            Value::Empty => out.push_str("()"),
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
            Value::Symbol(id) => out.push_str(&Self::symbol_text(id, standard)),
            Value::Path(id) => {
                let segs: Vec<String> = heap
                    .path_segments(id)
                    .iter()
                    .map(|s| Self::symbol_text(*s, standard))
                    .collect();
                out.push_str(&segs.join("::"));
            }
            Value::Boxed(id) if heap.is_struct(id) => {
                let ty = crate::stored_type(heap, id).expect("a struct box has a type");
                // A `Vector<T>` prints in CL's vector syntax, which reads
                // back as `#(..)` — or, with `*print-array*` off, as its type
                // and length alone.
                let vector = typelisp_mem::type_shape(typelisp_mem::TypeId::VECTOR).named_definition();
                if typelisp_mem::type_shape(ty).named_definition() == vector {
                    let n = heap.struct_field_count(id);
                    if !ctx.print_vars.array {
                        out.push_str(&format!("#<{} {}>", crate::shared::name_view().type_name(ty), n));
                        return Ok(());
                    }
                    out.push_str("#(");
                    for i in 0..n {
                        if Self::length_reached(ctx, i) {
                            out.push_str(if i == 0 { "..." } else { " ..." });
                            break;
                        }
                        if i > 0 {
                            out.push(' ');
                        }
                        let f = heap.struct_field(id, i);
                        self.render_field(heap, ctx, ty, None, i, f, standard, depth + 1, out)?;
                    }
                    out.push(')');
                    return Ok(());
                }
                if typelisp_mem::type_shape(ty).named_definition() == typelisp_mem::type_shape(typelisp_mem::TypeId::CONS_CELL).named_definition() {
                    return self.render_cons_cells(heap, ctx, id, ty, standard, depth, out);
                }
                // A tuple prints as `#{..}`, which reads back as one: a datum
                // read from `#{..}` has no `print-object` made for it, and a
                // typed one whose elements have none would otherwise fall to
                // the `#<..>` below.
                if typelisp_mem::tuple_type_arity(ty).is_some() {
                    out.push_str("#{");
                    for i in 0..heap.struct_field_count(id) {
                        if Self::length_reached(ctx, i) {
                            out.push_str(if i == 0 { "..." } else { " ..." });
                            break;
                        }
                        if i > 0 {
                            out.push(' ');
                        }
                        let f = heap.struct_field(id, i);
                        self.render_field(heap, ctx, ty, None, i, f, standard, depth + 1, out)?;
                    }
                    out.push('}');
                    return Ok(());
                }
                // The `Sexpr` `array` variant — `#nA(..)` read as data. A typed
                // `Array<T>` prints through its prelude `print-object`, but this
                // instantiation appears in no program's types, so no method was
                // made for it; it is S-expression data, and data prints here,
                // in the same syntax that method writes.
                if ty == typelisp_mem::TypeId::SEXPR_ARRAY {
                    return self.render_data_array(heap, ctx, id, standard, depth, out);
                }
                out.push_str(&format!("#<{}", crate::shared::name_view().type_name(ty)));
                for i in 0..heap.struct_field_count(id) {
                    if Self::length_reached(ctx, i) {
                        out.push_str(" ...");
                        break;
                    }
                    out.push(' ');
                    // `x: 1` — the name the definition gives the field, when it
                    // gives one. Not `:x`, which would read as a keyword
                    // argument (`(point::make :x 1)`).
                    if let Some(name) = ctx.env.field_name(ty, i)? {
                        out.push_str(&name);
                        out.push_str(": ");
                    }
                    // Read the field out before the recursive call: `heap` is
                    // `&mut` here (the `print-object` dispatch needs it), so the
                    // read cannot stay borrowed across it.
                    let f = heap.struct_field(id, i);
                    self.render_field(heap, ctx, ty, None, i, f, standard, depth + 1, out)?;
                }
                out.push('>');
            }
            Value::Boxed(id) if heap.is_enum(id) => {
                let ty = crate::stored_type(heap, id).expect("an enum box has a type");
                let variant = heap.enum_variant(id);
                // Every enum is registered — a user `defenum`'s own exec, and
                // the built-in sum types (`Option`/`Result`/the error types),
                // which `Interp::new` seeds from `typeck::builtins::runtime_types`
                // up front — so a lookup that comes up empty is broken (a
                // stale `Path`, an enum this table was never told about), and
                // says so.
                let name = ctx.env.enum_variant_name(ty, variant)?;
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
                        self.render_field(heap, ctx, ty, Some(variant), i, f, standard, depth + 1, out)?;
                    }
                    out.push(')');
                }
            }
            // Named by its whole type, as a `Vector<T>` is: the table carries
            // its instantiation (`HashTable<string,int>`) from `HashTable::new`.
            Value::Boxed(id) if heap.is_hashtable(id) => out.push_str(&format!(
                "#<{} count={}>",
                crate::shared::name_view().type_name(heap.struct_type_key(id)),
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
                    Some(recv) => {
                        format!("#<builtin {}::{}>", crate::shared::name_view().definition(recv.home(), recv), heap.builtin_fn_name(id))
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
            // A `BoxedObj::Cell` — a binding slot, never a value handed to the
            // printer. Reaching here is an interpreter bug, reported rather
            // than mis-rendered as a float.
            Value::Boxed(_) => return Err("print: unprintable boxed object".to_string()),
            Value::Cons(_) if type_syntax(heap, v).is_some() => {
                // `(%type-apply vector int)` is `Vector<int>`, and
                // `(%type-member T m)` is `T::m`: the reader's structure for
                // a generic type, written back the way it is read.
                match type_syntax(heap, v).expect("checked just above") {
                    TypeSyntax::Apply(head, args) => {
                        self.render(heap, ctx, head, standard, depth + 1, out)?;
                        out.push('<');
                        for (i, a) in args.iter().enumerate() {
                            if i > 0 {
                                out.push(',');
                            }
                            // `(:dyn T)` is written `:dyn T` inside `<..>`,
                            // where no parenthesized type but `()` reads.
                            match dyn_type_arg(heap, *a) {
                                Some(t) => {
                                    out.push_str(":dyn ");
                                    self.render(heap, ctx, t, standard, depth + 1, out)?;
                                }
                                None => self.render(heap, ctx, *a, standard, depth + 1, out)?,
                            }
                        }
                        out.push('>');
                    }
                    TypeSyntax::Member(ty, member) => {
                        self.render(heap, ctx, ty, standard, depth + 1, out)?;
                        out.push_str("::");
                        self.render(heap, ctx, member, standard, depth + 1, out)?;
                    }
                }
            }
            Value::Cons(_) => {
                out.push('(');
                self.render_data_tail(heap, ctx, v, 0, standard, depth, out)?;
                out.push(')');
            }
        }
        Ok(())
    }

    /// The elements of the data list `cur` on, `printed` of the list's
    /// elements already written, and its dotted tail: everything of a list
    /// but its parentheses.
    #[allow(clippy::too_many_arguments)]
    fn render_data_tail(
        &mut self,
        heap: &mut Heap,
        ctx: RenderCtx<'_>,
        mut cur: Value,
        mut printed: usize,
        standard: bool,
        depth: usize,
        out: &mut String,
    ) -> Result<(), String> {
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
        Ok(())
    }

    /// The `cons-cell` `id`, of the type `ty`, as the list it begins: CL's
    /// printing of a cons. A cell's cdr goes on the list when its type is
    /// another cell, ends it when it is `()`, goes on as the data list it is
    /// when it is `Option<Sexpr>`, and is a dotted tail otherwise. Its type
    /// decides, not its value: an `Option` field's `None` is the same word
    /// as the empty list.
    #[allow(clippy::too_many_arguments)]
    fn render_cons_cells(
        &mut self,
        heap: &mut Heap,
        ctx: RenderCtx<'_>,
        mut id: typelisp_mem::BoxId,
        mut ty: typelisp_mem::TypeId,
        standard: bool,
        depth: usize,
        out: &mut String,
    ) -> Result<(), String> {
        let cons = typelisp_mem::type_shape(typelisp_mem::TypeId::CONS_CELL).named_definition();
        out.push('(');
        let mut printed = 0usize;
        loop {
            if Self::length_reached(ctx, printed) {
                out.push_str(if printed == 0 { "..." } else { " ..." });
                break;
            }
            if printed > 0 {
                out.push(' ');
            }
            let car = heap.struct_field(id, 0);
            self.render_field(heap, ctx, ty, None, 0, car, standard, depth + 1, out)?;
            printed += 1;
            let typelisp_mem::TypeShape::Named(_, _, args) = typelisp_mem::type_shape(ty) else {
                return Err(format!("print: a `cons-cell` of the type `{}`, which is not one", crate::shared::name_view().type_name(ty)));
            };
            let cdr_ty = args[1];
            let cdr = heap.struct_field(id, 1);
            let cdr_shape = typelisp_mem::type_shape(cdr_ty);
            if cdr_shape == typelisp_mem::TypeShape::Unit {
                break;
            }
            if cdr_ty == typelisp_mem::TypeId::OPTION_SEXPR {
                self.render_data_tail(heap, ctx, cdr, printed, standard, depth, out)?;
                break;
            }
            match cdr {
                // A shared cell goes on as a tail, as a data list's does.
                Value::Boxed(next) if cdr_shape.named_definition() == cons && !(ctx.print_vars.circle && self.is_shared(heap, cdr)) => {
                    id = next;
                    ty = cdr_ty;
                }
                _ => {
                    out.push_str(" . ");
                    self.render_field(heap, ctx, ty, None, 1, cdr, standard, depth + 1, out)?;
                    break;
                }
            }
        }
        out.push(')');
        Ok(())
    }

    /// Whether `v` is one of the nodes [`scan_shared`] found more than once
    /// (labelled already or not).
    fn is_shared(&self, heap: &Heap, v: Value) -> bool {
        container_key(heap, v).is_some_and(|k| self.shared.contains_key(&k))
    }
}

/// The reader's structure for a generic type or a member of one, which
/// prints as it is written: `(%type-apply Name A B)` is `Name<A,B>`,
/// `(%type-member T m)` is `T::m` and `(%value-member p x)` is `p::x`.
pub(crate) enum TypeSyntax {
    Apply(Value, Vec<Value>),
    Member(Value, Value),
}

/// `v` as [`TypeSyntax`], when it is well formed as one: an application with
/// a head and at least one argument, a member with a type and a name.
pub(crate) fn type_syntax(heap: &Heap, v: Value) -> Option<TypeSyntax> {
    let Ok(Value::Symbol(head)) = heap.car(v) else { return None };
    let marker = head.well_known();
    if marker != typelisp_mem::wk::TYPE_APPLY && marker != typelisp_mem::wk::TYPE_MEMBER && marker != typelisp_mem::wk::VALUE_MEMBER {
        return None;
    }
    let mut rest = Vec::new();
    let mut cur = heap.cdr(v).ok()?;
    loop {
        match cur {
            Value::Cons(_) => {
                rest.push(heap.car(cur).ok()?);
                cur = heap.cdr(cur).ok()?;
            }
            Value::Empty => break,
            _ => return None,
        }
    }
    if marker == typelisp_mem::wk::TYPE_APPLY {
        (rest.len() >= 2).then(|| TypeSyntax::Apply(rest[0], rest[1..].to_vec()))
    } else {
        (rest.len() == 2).then(|| TypeSyntax::Member(rest[0], rest[1]))
    }
}

/// The trait of a `(:dyn T)` type argument.
fn dyn_type_arg(heap: &Heap, v: Value) -> Option<Value> {
    let Ok(Value::Symbol(head)) = heap.car(v) else { return None };
    if !head.is(typelisp_mem::wk::DYN) {
        return None;
    }
    let rest = heap.cdr(v).ok()?;
    let t = heap.car(rest).ok()?;
    matches!(heap.cdr(rest).ok()?, Value::Empty).then_some(t)
}
