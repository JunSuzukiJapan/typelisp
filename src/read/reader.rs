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
//! Supported v1 syntax: integers (decimal, `0x` hex, signed), floats, booleans
//! `true`/`false`, strings with escapes, characters `#\a` / `#\Space`, symbols
//! (operators, `::` paths, `Vec<T>`-style tokens), lists, dotted pairs `(a . b)`,
//! quote `'`, quasiquote `` ` ``, unquote `,`, unquote-splicing `,@`, and
//! comments (`;` line, `#| ... |#` nested block).

use crate::name_lexer::{NameLexer, NameTok};
use crate::{Error, Heap, SymId, Value};

pub struct Reader;

impl Default for Reader {
    fn default() -> Reader {
        Reader::new()
    }
}

impl Reader {
    pub fn new() -> Reader {
        Reader
    }

    /// Read a single datum from `src`. Trailing input is ignored.
    pub fn read(&self, heap: &mut Heap, src: &str) -> Result<Value, Error> {
        let mut cur = Cursor::new(src);
        read_datum(&mut cur, heap)
    }

    /// Read every top-level datum from `src`.
    ///
    /// Each returned value is registered as a GC root (so later reads cannot
    /// collect earlier results); the caller pops them when done.
    pub fn read_all(&self, heap: &mut Heap, src: &str) -> Result<Vec<Value>, Error> {
        let mut cur = Cursor::new(src);
        let mut out = Vec::new();
        loop {
            skip_ws_comments(&mut cur);
            if cur.at_end() {
                break;
            }
            let v = read_datum(&mut cur, heap)?;
            heap.push_root(v);
            out.push(v);
        }
        Ok(out)
    }
}

// ----------------------------------------------------------------------
// Cursor
// ----------------------------------------------------------------------

struct Cursor {
    chars: Vec<char>,
    pos: usize,
}

impl Cursor {
    fn new(s: &str) -> Cursor {
        Cursor { chars: s.chars().collect(), pos: 0 }
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }
    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }
    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
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
fn is_delimiter(c: char) -> bool {
    is_ws(c) || matches!(c, '(' | ')' | '"' | '\'' | '`' | ';')
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

fn skip_ws_comments(cur: &mut Cursor) {
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
            _ => break,
        }
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

fn read_datum(cur: &mut Cursor, heap: &mut Heap) -> Result<Value, Error> {
    skip_ws_comments(cur);
    match cur.peek() {
        None => Err(Error::ReadError("unexpected end of input".to_string())),
        Some('(') => read_list(cur, heap),
        Some(')') => Err(Error::UnmatchedParen),
        Some('\'') => read_wrapped(cur, heap, "quote"),
        Some('`') => read_wrapped(cur, heap, "quasiquote"),
        Some(',') => {
            cur.next(); // the ','
            if cur.peek() == Some('@') {
                cur.next(); // the '@'
                read_wrapped_body(cur, heap, "unquote-splicing")
            } else {
                read_wrapped_body(cur, heap, "unquote")
            }
        }
        Some('"') => read_string(cur, heap),
        Some('#') => read_hash(cur, heap),
        Some(_) => read_atom(cur, heap),
    }
}

/// Read `<prefix-char><datum>` as `(<head> datum)` — the shared shape behind
/// `'x` -> `(quote x)`, `` `x `` -> `(quasiquote x)`, `,x` -> `(unquote x)`,
/// `,@x` -> `(unquote-splicing x)`. Consumes the single prefix character
/// itself, then delegates the rest (reading `datum` and building the
/// 2-element list) to [`read_wrapped_body`] — `,@` needs to consume *two*
/// prefix characters (`,` then `@`), so its caller in [`read_datum`] does
/// that part itself and calls `read_wrapped_body` directly.
fn read_wrapped(cur: &mut Cursor, heap: &mut Heap, head: &str) -> Result<Value, Error> {
    cur.next(); // the prefix character
    read_wrapped_body(cur, heap, head)
}

/// Read `datum` and build `(head datum)`, the shared tail of [`read_wrapped`]
/// — assumes any prefix character(s) have already been consumed.
fn read_wrapped_body(cur: &mut Cursor, heap: &mut Heap, head: &str) -> Result<Value, Error> {
    let d = read_datum(cur, heap)?;
    heap.push_root(d);
    let q = heap.intern_symbol(head); // symbols are permanent; no rooting needed
    let result = (|| {
        let inner = heap.cons(d, Value::Empty)?;
        heap.push_root(inner);
        let r = heap.cons(q, inner);
        heap.pop_root(); // inner
        r
    })();
    heap.pop_root(); // d
    result
}

// ----------------------------------------------------------------------
// Lists & dotted pairs
// ----------------------------------------------------------------------

fn read_list(cur: &mut Cursor, heap: &mut Heap) -> Result<Value, Error> {
    cur.next(); // '('
    let mark = heap.root_count();
    let mut elems: Vec<Value> = Vec::new();
    let mut tail = Value::Empty;

    loop {
        skip_ws_comments(cur);
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
                let d = match read_datum(cur, heap) {
                    Ok(d) => d,
                    Err(e) => {
                        restore_roots(heap, mark);
                        return Err(e);
                    }
                };
                heap.push_root(d);
                tail = d;
                skip_ws_comments(cur);
                if cur.peek() == Some(')') {
                    cur.next();
                } else {
                    restore_roots(heap, mark);
                    return Err(Error::ReadError("expected ) after dotted cdr".to_string()));
                }
                break;
            }
            Some(_) => {
                let e = match read_datum(cur, heap) {
                    Ok(e) => e,
                    Err(err) => {
                        restore_roots(heap, mark);
                        return Err(err);
                    }
                };
                heap.push_root(e); // keep alive while reading the rest / building
                elems.push(e);
            }
        }
    }

    // Build the cons chain from the back; the accumulator stays rooted so a GC
    // triggered by `cons` cannot reclaim the part already built.
    let mut acc = tail;
    heap.push_root(acc);
    for &e in elems.iter().rev() {
        match heap.cons(e, acc) {
            Ok(cell) => {
                heap.pop_root(); // old acc
                heap.push_root(cell);
                acc = cell;
            }
            Err(err) => {
                restore_roots(heap, mark);
                return Err(err);
            }
        }
    }

    restore_roots(heap, mark);
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

fn read_hash(cur: &mut Cursor, _heap: &mut Heap) -> Result<Value, Error> {
    cur.next(); // '#'
    match cur.peek() {
        Some('\\') => {
            cur.next(); // '\\'
            read_char(cur)
        }
        other => Err(Error::ReadError(format!("unsupported # syntax: #{:?}", other))),
    }
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

fn read_atom(cur: &mut Cursor, heap: &mut Heap) -> Result<Value, Error> {
    let mut tok = String::new();
    while let Some(c) = cur.peek() {
        if is_delimiter(c) {
            break;
        }
        tok.push(c);
        cur.next();
    }
    // tok is non-empty: read_datum only dispatches here on a non-delimiter.
    if let Some(v) = parse_number(heap, &tok) {
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
                let mut segs: Vec<SymId> = Vec::with_capacity(parts.len());
                for p in &parts {
                    if let Value::Symbol(id) = heap.intern_symbol(p) {
                        segs.push(id);
                    }
                }
                return Ok(heap.intern_path(&segs));
            }
            Ok(heap.intern_symbol(&tok))
        }
    }
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

/// Interpret a token as a number, or `None` if it is a symbol. Takes `heap`
/// (unlike an otherwise-pure parser) because a float literal must be
/// heap-boxed (`Heap::alloc_float`, see `BoxedObj`'s doc comment) — an `f64`
/// doesn't fit alongside `Value`'s tag the way an int/char does.
fn parse_number(heap: &mut Heap, tok: &str) -> Option<Value> {
    let (neg, body) = if let Some(r) = tok.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = tok.strip_prefix('+') {
        (false, r)
    } else {
        (false, tok)
    };

    // hexadecimal integer: 0x...
    if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Ok(n) = i64::from_str_radix(hex, 16) {
                return Some(Value::Int(if neg { -n } else { n }));
            }
        }
        return None;
    }

    // decimal integer
    if !body.is_empty() && body.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(n) = body.parse::<i64>() {
            return Some(Value::Int(if neg { -n } else { n }));
        }
        return None;
    }

    // float: starts with a digit or '.', and has a '.' or exponent marker
    let first = body.chars().next()?;
    if (first.is_ascii_digit() || first == '.')
        && (body.contains('.') || body.contains('e') || body.contains('E'))
    {
        if let Ok(f) = body.parse::<f64>() {
            return Some(heap.alloc_float(if neg { -f } else { f }));
        }
    }
    None
}
