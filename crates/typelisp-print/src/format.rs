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
//! `~% ~& ~| ~~`, `~( ~)`, `~[ ~; ~]`, `~:[`, `~{ ~} ~^`, `~< ~> ~T`, `~*`,
//! `~/name/`, the ignored-`~newline`, the pretty-printer directives
//! `~_ ~I ~:T` and the logical-block form of `~<…~:>`, plus the
//! numeric/`'c`/`v`/`#` prefix parameters and the `:`/`@` modifiers each
//! directive gives meaning to.
//!
//! The pretty-printer directives (and the `*print-pretty*` path of
//! `~A`/`~S`/`~W`) record [`Op`]s on the output buffer rather than emitting
//! text directly; [`crate::pprint`] turns the finished buffer into laid
//! out text. With `*print-pretty*` false nothing records an op and the buffer
//! is returned exactly as before.
//!
//! ## Strictness
//!
//! Nothing is coerced, clamped or ignored. A parameter or modifier a directive
//! does not take, or a value outside its range, is an error when the control
//! string is parsed ([`Head::validate`]); an argument of the wrong type, or a
//! cursor move outside the arguments, is an error when it is met. CL's lenient
//! rules — `~D` printing a non-integer as `~A`, `~:[` taking any value as a
//! boolean — are not followed. Refused outright: `~?` (its control string
//! arrives at run time, where nothing can check it), `~@[` (it tests for nil,
//! which this language does not have) and the empty-bodied `~{~}`.
//!
//! [`check_arguments`] runs the same rules over the argument *types* before the
//! program runs, which is where the checker reports them.

use std::collections::{BTreeSet, HashMap};

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
    let items = list_to_vec(heap, args, "the argument list")?;
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

/// The elements of a proper `Sexpr` list. `what` names the list in the error
/// for anything else — an atom, or a list whose last cdr is not the empty list.
fn list_to_vec(heap: &Heap, v: Value, what: &str) -> Result<Vec<Value>, String> {
    heap.list_to_vec(v).map_err(|_| format!("format: {} is not a proper list", what))
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
/// modifiers, plus what each parameter slot accepts — filled in by
/// [`Head::validate`] once the directive is known.
#[derive(Clone, Debug, Default)]
struct Head {
    params: Vec<Param>,
    colon: bool,
    at: bool,
    kinds: &'static [ParamKind],
}

/// What one prefix-parameter slot of a directive accepts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ParamKind {
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
    fn check_int(self, n: i64) -> Result<(), String> {
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
                head.validate('/', &format!("/{}/", name))?;
                nodes.push(Node::Call { head, name: name.to_ascii_lowercase() });
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
// Static checking
// ===========================================================================

/// What the checker knows about one argument's type, for [`check_arguments`].
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

/// Checks the arguments of a `format`/`print`/`println` call against its
/// literal control string, before the program runs: that every directive
/// that consumes an argument has one, of a type it accepts.
///
/// The argument cursor is followed through the whole directive language. Where
/// the path depends on a value only known at run time — which `~[` clause is
/// chosen, whether a `~^` fires, how many times a `~@{` goes round — every
/// possibility is followed, so a mistake on any of them is reported. The
/// elements of a list argument (`~{`, `~:{`, `~<…~:>`) are `Sexpr`s whose
/// count the type does not say; directives consuming them are checked when
/// the elements arrive.
///
/// Extra arguments are allowed, as in CL.
pub fn check_arguments(control: &str, args: &[ArgType]) -> Result<(), String> {
    let nodes = parse(control)?;
    Sim.seq(Frame::Static(args), &nodes, &BTreeSet::from([Cur::At(0)]))?;
    Ok(())
}

/// The argument list a stretch of directives consumes from: the call's own
/// arguments (or a known slice of them), or the elements of a list argument.
#[derive(Clone, Copy)]
enum Frame<'a> {
    Static(&'a [ArgType]),
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

struct Sim;

impl Sim {
    /// Consumes one argument at `cur` for `what`, which demands `need`.
    fn consume(&self, frame: Frame, cur: Cur, need: Need, what: &str) -> Result<Cur, String> {
        match (frame, cur) {
            (Frame::Static(args), Cur::At(p)) => {
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
                    (Frame::Static(args), Cur::At(i)) => {
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
            Node::Call { head, name } => {
                let what = format!("~/{}/", name);
                let (cur, _) = self.params(frame, cur, head, &what)?;
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
                    (Some(_), Frame::Static(args), Cur::At(p)) => Some(p == args.len()),
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
            'd' | 'b' | 'o' | 'x' | 'r' => Need::Integer,
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
        let (Frame::Static(args), Cur::At(p)) = (frame, cur) else { return Ok(Cur::Elements) };
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
        let empty = Frame::Static(&[]);
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
                (Frame::Static(args), Cur::At(p)) => {
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
                (Frame::Static(args), Cur::At(p)) => {
                    let rest = &args[p..];
                    let ends = self.rounds(Frame::Static(rest), body, close_colon, max)?;
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
        let Frame::Static(args) = frame else { unreachable!("`~@{{` over the call's own arguments") };
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
        self.args.len() - self.pos
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
    /// character parameter yields its code point.
    ///
    /// Literal parameters were checked against the directive when the control
    /// string was parsed ([`Head::validate`]); what `v` and `#` supply is
    /// checked here, against the same slot kinds.
    fn resolve_params(&mut self, head: &Head, defaults: &[Option<i64>]) -> Result<Vec<Option<i64>>, String> {
        let mut out = Vec::new();
        for (i, (p, kind)) in head.params.iter().zip(head.kinds).enumerate() {
            let bad = |why: String| format!("format: parameter {}: {}", i + 1, why);
            let v = match p {
                Param::Int(n) => Some(*n),
                Param::Char(c) => Some(*c as i64),
                Param::Count => {
                    let n = self.remaining() as i64;
                    kind.check_int(n).map_err(bad)?;
                    Some(n)
                }
                Param::Arg => {
                    let arg = self.next_arg()?;
                    match (kind, arg) {
                        (ParamKind::Char, Value::Char(c)) => Some(c as i64),
                        (ParamKind::Char, _) => return Err(bad("its `v` argument must be a character".to_string())),
                        (k, _) => {
                            let n = int_of(self.heap, arg).ok_or_else(|| bad("its `v` argument must be an integer".to_string()))?;
                            k.check_int(n).map_err(bad)?;
                            Some(n)
                        }
                    }
                }
                Param::Default => defaults.get(i).copied().flatten(),
            };
            out.push(v);
        }
        // Pad with defaults for parameters the user omitted entirely.
        for d in defaults.iter().skip(head.params.len()) {
            out.push(*d);
        }
        Ok(out)
    }

    /// A directive's character parameter (a padding character) at `idx`, or
    /// `default` when it was omitted.
    fn char_param(vals: &[Option<i64>], idx: usize, default: char) -> char {
        match vals.get(idx).copied().flatten() {
            Some(n) => char::from_u32(n as u32).expect("a character slot holds a checked character"),
            None => default,
        }
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
        let vals = self.resolve_params(head, &[])?;
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
        if head.colon {
            // `~:[false~;true~]`: choose by a boolean argument.
            let Value::Bool(b) = self.next_arg()? else {
                return Err("format: ~:[ requires a bool argument".to_string());
            };
            let idx = if b { 1 } else { 0 };
            if let Some(clause) = clauses.get(idx) {
                return self.interp_seq(clause, out);
            }
            return Ok(Flow::Normal);
        }
        // Plain `~[`: an integer selects the clause by index — the prefix
        // parameter when there is one (`~1[`, `~v[`, `~#[`), otherwise the
        // next argument.
        let sel = if head.params.is_empty() {
            let arg = self.next_arg()?;
            match int_of(self.heap, arg) {
                Some(n) => Some(n),
                // An integer too large for a word selects no clause.
                None if matches!(arg, Value::Boxed(id) if self.heap.is_bignum(id)) => None,
                None => return Err("format: ~[ requires an integer argument".to_string()),
            }
        } else {
            self.resolve_params(head, &[])?[0]
        };
        let chosen = sel.filter(|&n| n >= 0 && (n as usize) < clauses.len() && Some(n as usize) != default);
        if let Some(clause) = chosen.map(|n| n as usize).or(default).and_then(|i| clauses.get(i)) {
            return self.interp_seq(clause, out);
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
        let vals = self.resolve_params(head, &[])?;
        let max = Self::int_param(&vals, 0, -1); // -1 = unbounded

        let run_body = |st: &mut State, sublist: Vec<Value>, out: &mut Out| -> Result<Flow, String> {
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
                let elems = list_to_vec(self.heap, list, "~{'s argument")?;
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
                let sublists = list_to_vec(self.heap, list, "~:{'s argument")?;
                let mut count = 0;
                for sub in &sublists {
                    if max >= 0 && count >= max {
                        break;
                    }
                    let elems = list_to_vec(self.heap, *sub, "an element of ~:{'s argument")?;
                    if matches!(run_body(self, elems, out)?, Flow::Escape) {
                        break;
                    }
                    count += 1;
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
                    let elems = list_to_vec(self.heap, *sub, "an argument of ~:@{")?;
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
        if elems.is_empty() && close_colon {
            let saved = std::mem::take(&mut self.args);
            let saved_pos = std::mem::replace(&mut self.pos, 0);
            let r = self.interp_seq(body, out);
            self.args = saved;
            self.pos = saved_pos;
            r?;
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
            let before = self.pos;
            let flow = self.interp_seq(body, out)?;
            iterations += 1;
            if matches!(flow, Flow::Escape) {
                break;
            }
            // A pass that consumed nothing would repeat forever.
            if self.pos == before && max < 0 {
                return Err("format: an iteration of ~{ consumed no argument, so it would never end".to_string());
            }
        }
        let consumed = self.pos;
        self.args = saved;
        self.pos = saved_pos;
        Ok(consumed)
    }

    fn interp_just(&mut self, head: &Head, segments: &[Vec<Node>], out: &mut Out) -> Result<(), String> {
        let vals = self.resolve_params(head, &[Some(0), Some(1), Some(0)])?;
        let mincol = Self::int_param(&vals, 0, 0) as usize;
        let colinc = Self::int_param(&vals, 1, 1) as usize;
        let minpad = Self::int_param(&vals, 2, 0) as usize;
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
                let sublist = list_to_vec(self.heap, list, "a logical block's argument")?;
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
                let vals = self.resolve_params(head, &[Some(0), Some(1), Some(0)])?;
                let mincol = Self::int_param(&vals, 0, 0) as usize;
                let colinc = Self::int_param(&vals, 1, 1) as usize;
                let minpad = Self::int_param(&vals, 2, 0) as usize;
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
                self.emit_integer(head, ch, Some(radix), out)?;
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
                    if self.pos == 0 {
                        return Err("format: ~:p reuses the previous argument, and there is none".to_string());
                    }
                    self.pos -= 1;
                }
                let arg = self.next_arg()?;
                let n = match arg {
                    _ if int_of(self.heap, arg).is_some() => int_of(self.heap, arg).expect("just tested"),
                    // Every bignum is far from 1.
                    Value::Boxed(id) if self.heap.is_bignum(id) => 2,
                    _ => return Err("format: ~p requires an integer argument".to_string()),
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
                let vals = self.resolve_params(head, &[Some(1)])?;
                for _ in 0..Self::int_param(&vals, 0, 1) {
                    out.push('\n');
                }
            }
            '&' => {
                let vals = self.resolve_params(head, &[Some(1)])?;
                let n = Self::int_param(&vals, 0, 1);
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
                let vals = self.resolve_params(head, &[Some(1)])?;
                for _ in 0..Self::int_param(&vals, 0, 1) {
                    out.push('\u{000c}');
                }
            }
            '~' => {
                let vals = self.resolve_params(head, &[Some(1)])?;
                for _ in 0..Self::int_param(&vals, 0, 1) {
                    out.push('~');
                }
            }
            't' => self.emit_tab(head, out)?,
            '*' => {
                let vals = self.resolve_params(head, &[])?;
                let target = if head.at {
                    // Absolute goto (default index 0).
                    Self::int_param(&vals, 0, 0)
                } else if head.colon {
                    // Back up n (default 1).
                    self.pos as i64 - Self::int_param(&vals, 0, 1)
                } else {
                    // Skip n forward (default 1).
                    self.pos as i64 + Self::int_param(&vals, 0, 1)
                };
                if target < 0 || target as usize > self.args.len() {
                    return Err(format!(
                        "format: ~* moves to argument {}, outside the {} there are",
                        target,
                        self.args.len()
                    ));
                }
                self.pos = target as usize;
            }
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
                let vals = self.resolve_params(head, &[Some(0)])?;
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

    /// `~D ~B ~O ~X`, and `~R` with a radix. `radix` is the directive's own,
    /// or `None` for `~R`, which reads it from its first parameter; the
    /// mincol, padchar, commachar and interval parameters follow.
    fn emit_integer(&mut self, head: &Head, ch: char, radix: Option<u32>, out: &mut Out) -> Result<(), String> {
        let at = usize::from(radix.is_none());
        let defaults: &[Option<i64>] = if radix.is_none() {
            &[None, Some(0), None, None, Some(3)]
        } else {
            &[Some(0), None, None, Some(3)]
        };
        let vals = self.resolve_params(head, defaults)?;
        let radix = match radix {
            Some(r) => r,
            None => vals[0].expect("~r reaches here only with its radix parameter") as u32,
        };
        let mincol = Self::int_param(&vals, at, 0) as usize;
        let padchar = Self::char_param(&vals, at + 1, ' ');
        let commachar = Self::char_param(&vals, at + 2, ',');
        let comma_interval = Self::int_param(&vals, at + 3, 3) as usize;
        let arg = self.next_arg()?;
        let digits = self
            .integer_in_radix(arg, radix)
            .ok_or_else(|| format!("format: ~{} requires an integer argument", ch))?;
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
        // A parse-time check made sure the first parameter, when there is
        // any, is the radix.
        if !head.params.is_empty() {
            return self.emit_integer(head, 'r', None, out);
        }
        // No radix parameter: English/Roman spellings.
        let arg = self.next_arg()?;
        let n = match int_of(self.heap, arg) {
            Some(n) => n,
            None if matches!(arg, Value::Boxed(id) if self.heap.is_bignum(id)) => {
                return Err("format: ~r cannot spell an integer this large in words".to_string())
            }
            None => return Err("format: ~r requires an integer argument".to_string()),
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
        let vals = self.resolve_params(head, &[None, None, Some(0), None, Some(' ' as i64)])?;
        let w = Self::int_param(&vals, 0, -1);
        let d = Self::int_param(&vals, 1, -1);
        let k = Self::int_param(&vals, 2, 0);
        let overflow = vals[3].map(|_| Self::char_param(&vals, 3, ' '));
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
        let vals = self.resolve_params(head, &[None, None, None, Some(1), None, Some(' ' as i64), None])?;
        let w = Self::int_param(&vals, 0, -1);
        let d = Self::int_param(&vals, 1, 6);
        let exptchar = Self::char_param(&vals, 6, 'e');
        let padchar = Self::char_param(&vals, 5, ' ');
        let f = self.next_float()?;
        let mut s = format!("{:.*e}", d as usize, f);
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
        let vals = self.resolve_params(head, &[Some(2), Some(1), Some(0), Some(' ' as i64)])?;
        let d = Self::int_param(&vals, 0, 2) as usize;
        let n = Self::int_param(&vals, 1, 1) as usize;
        let w = Self::int_param(&vals, 2, 0) as usize;
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
        let vals = self.resolve_params(head, &[Some(1), Some(1)])?;
        let col = Self::int_param(&vals, 0, 1);
        let inc = Self::int_param(&vals, 1, 1);
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
                colnum: col,
                colinc: inc,
            });
            return Ok(());
        }
        // `~:T` is `pprint-tab :section`, which does nothing when
        // `*print-pretty*` is false (CLHS `pprint-tab`).
        if head.colon {
            return Ok(());
        }
        let cur = current_column(&out.text);
        let target = pprint::tab_target(cur, 0, col, inc, head.at);
        for _ in cur..target {
            out.push(' ');
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
