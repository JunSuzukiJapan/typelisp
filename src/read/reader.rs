use crate::{Object, Cons, Error, NumSuffix};

pub struct Reader;

/// Character cursor with arbitrary lookahead over the input.
/// Backed by a `Vec<char>` so we can peek ahead by more than one position
/// (needed to disambiguate `#|`, `-1` vs `(-`, `0x..`, `1e-3`, etc.).
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
    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }
    fn next(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }
}

fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// A character that terminates a symbol or number token.
fn is_terminator(c: char) -> bool {
    is_ws(c) || matches!(c, '(' | ')' | '\'' | '"' | ';')
}

impl Reader {
    pub fn new() -> Reader {
        Reader {}
    }

    pub fn read(&self, s: &str) -> Result<Object, Error> {
        let mut cur = Cursor::new(s);
        self.read_datum(&mut cur)
    }

    /// Read every top-level datum in `s` until end of input.
    pub fn read_all(&self, s: &str) -> Result<Vec<Object>, Error> {
        let mut cur = Cursor::new(s);
        let mut out = Vec::new();
        loop {
            self.skip_atmosphere(&mut cur)?;
            if cur.peek().is_none() {
                break;
            }
            out.push(self.read_datum(&mut cur)?);
        }
        Ok(out)
    }

    fn read_datum(&self, cur: &mut Cursor) -> Result<Object, Error> {
        self.skip_atmosphere(cur)?;

        match cur.peek() {
            None => Ok(Object::Null),
            Some('(') => self.read_list(cur),
            Some(')') => Err(Error::UnmatchedParen),
            Some('\'') => self.read_quote(cur),
            Some('"') => self.read_string(cur),
            Some('#') => self.read_hash(cur),
            Some(_) => self.read_atom(cur),
        }
    }

    /// Skip whitespace, `;` line comments and nestable `#| ... |#` block comments.
    fn skip_atmosphere(&self, cur: &mut Cursor) -> Result<(), Error> {
        loop {
            match cur.peek() {
                Some(c) if is_ws(c) => {
                    cur.next();
                }
                Some(';') => {
                    while let Some(c) = cur.next() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                Some('#') if cur.peek_at(1) == Some('|') => {
                    self.skip_block_comment(cur)?;
                }
                _ => break,
            }
        }
        Ok(())
    }

    /// Consume a nestable block comment, assuming the opening `#|` is at the cursor.
    fn skip_block_comment(&self, cur: &mut Cursor) -> Result<(), Error> {
        cur.next(); // '#'
        cur.next(); // '|'
        let mut depth = 1;
        while depth > 0 {
            match cur.next() {
                None => return Err(Error::IllegalEndOfBlockComment),
                Some('#') if cur.peek() == Some('|') => {
                    cur.next();
                    depth += 1;
                }
                Some('|') if cur.peek() == Some('#') => {
                    cur.next();
                    depth -= 1;
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    fn read_quote(&self, cur: &mut Cursor) -> Result<Object, Error> {
        cur.next(); // skip '
        let obj = self.read_datum(cur)?;
        let quote = Object::new_symbol(String::from("quote"));
        let cons = Cons::new_cons(obj, None);
        let cons = Cons::new_cons(quote, Some(cons));
        Ok(Object::List(cons))
    }

    fn read_list(&self, cur: &mut Cursor) -> Result<Object, Error> {
        cur.next(); // skip '('
        let cons = self.read_cons(cur)?;
        match cons {
            None => Ok(Object::Null),
            Some(l) => Ok(Object::List(l)),
        }
    }

    fn read_cons(&self, cur: &mut Cursor) -> Result<Option<Cons<Object>>, Error> {
        self.skip_atmosphere(cur)?;
        match cur.peek() {
            None => Err(Error::IllegalEndWhileReadingList),
            Some(')') => {
                cur.next(); // skip ')'
                Ok(None)
            }
            _ => {
                let car = self.read_datum(cur)?;
                let cdr = self.read_cons(cur)?;
                Ok(Some(Cons::new_cons(car, cdr)))
            }
        }
    }

    fn read_string(&self, cur: &mut Cursor) -> Result<Object, Error> {
        cur.next(); // skip opening '"'
        let mut buf = String::new();

        loop {
            match cur.next() {
                None => return Err(Error::IllegalEndOfString),
                Some('"') => break,
                Some('\\') => match cur.next() {
                    None => return Err(Error::IllegalEndOfEscapeSequence),
                    Some('n') => buf.push('\n'),
                    Some('t') => buf.push('\t'),
                    Some('r') => buf.push('\r'),
                    Some('\\') => buf.push('\\'),
                    Some('"') => buf.push('"'),
                    Some('0') => buf.push('\0'),
                    Some('u') => buf.push(self.read_unicode_escape(cur)?),
                    Some(c) => return Err(Error::IllegalEscapeSequence(c)),
                },
                Some(c) => buf.push(c),
            }
        }

        Ok(Object::new_string(buf))
    }

    /// Read the `{HHHH}` part of a `\u{...}` escape (the `\u` is already consumed).
    fn read_unicode_escape(&self, cur: &mut Cursor) -> Result<char, Error> {
        if cur.next() != Some('{') {
            return Err(Error::IllegalEscapeSequence('u'));
        }
        let mut hex = String::new();
        loop {
            match cur.next() {
                Some('}') => break,
                Some(c) if c.is_ascii_hexdigit() => hex.push(c),
                _ => return Err(Error::IllegalEscapeSequence('u')),
            }
        }
        let code = u32::from_str_radix(&hex, 16).map_err(|_| Error::IllegalEscapeSequence('u'))?;
        char::from_u32(code).ok_or(Error::IllegalEscapeSequence('u'))
    }

    /// Handle tokens starting with `#`. Block comments are already skipped by
    /// `skip_atmosphere`, so here `#` means a character literal `#\x`.
    fn read_hash(&self, cur: &mut Cursor) -> Result<Object, Error> {
        cur.next(); // skip '#'
        match cur.peek() {
            Some('\\') => {
                cur.next(); // skip '\'
                self.read_char(cur)
            }
            Some(c) => Err(Error::IllegalHashSyntax(c)),
            None => Err(Error::IllegalHashSyntax('\0')),
        }
    }

    /// Read a character literal body following `#\` (already consumed):
    /// a single char, or a named char (`Space`, `Newline`, `Tab`, `Return`,
    /// `Nul`, `Backspace`).
    fn read_char(&self, cur: &mut Cursor) -> Result<Object, Error> {
        let first = cur.next().ok_or(Error::IllegalEndOfChar)?;
        if first.is_alphabetic() {
            let mut name = String::new();
            name.push(first);
            while let Some(c) = cur.peek() {
                if c.is_alphanumeric() {
                    name.push(c);
                    cur.next();
                } else {
                    break;
                }
            }
            if name.chars().count() == 1 {
                Ok(Object::new_char(first))
            } else {
                let c = match name.to_lowercase().as_str() {
                    "space" => ' ',
                    "newline" => '\n',
                    "tab" => '\t',
                    "return" => '\r',
                    "nul" => '\0',
                    "backspace" => '\u{8}',
                    _ => return Err(Error::UnknownCharName(name)),
                };
                Ok(Object::new_char(c))
            }
        } else {
            // Any non-alphabetic char (including '(', ')', ';', space) is taken literally.
            Ok(Object::new_char(first))
        }
    }

    /// Read a maximal token (up to a terminator) and classify it as a number,
    /// a boolean/nil keyword, or a symbol. This mirrors Lisp tokenization: the
    /// whole token is read first, then we *try* to parse it as a number; on
    /// failure it is a symbol (so `1+`, `.foo`, `1.2.3` are all symbols).
    fn read_atom(&self, cur: &mut Cursor) -> Result<Object, Error> {
        let mut buf = String::new();
        while let Some(c) = cur.peek() {
            if is_terminator(c) {
                break;
            }
            buf.push(c);
            cur.next();
        }

        if let Ok(num) = parse_number(&buf) {
            return Ok(num);
        }

        match buf.as_str() {
            "true" => Ok(Object::True),
            "false" => Ok(Object::False),
            "null" | "nil" => Ok(Object::Null),
            _ => Ok(Object::new_symbol(buf)),
        }
    }
}

/// Split a trailing numeric type suffix off `s`, if present.
fn split_suffix(s: &str) -> (&str, Option<NumSuffix>) {
    // Longest suffixes first so `isize`/`usize` win over a hypothetical `i`/`u`.
    const SUFFIXES: &[(&str, NumSuffix)] = &[
        ("isize", NumSuffix::Isize),
        ("usize", NumSuffix::Usize),
        ("i8", NumSuffix::I8),
        ("i16", NumSuffix::I16),
        ("i32", NumSuffix::I32),
        ("i64", NumSuffix::I64),
        ("u8", NumSuffix::U8),
        ("u16", NumSuffix::U16),
        ("u32", NumSuffix::U32),
        ("u64", NumSuffix::U64),
        ("f32", NumSuffix::F32),
        ("f64", NumSuffix::F64),
    ];
    for (suf, ns) in SUFFIXES {
        if s.len() > suf.len() && s.ends_with(suf) {
            return (&s[..s.len() - suf.len()], Some(*ns));
        }
    }
    (s, None)
}

fn is_float_suffix(ns: Option<NumSuffix>) -> bool {
    matches!(ns, Some(NumSuffix::F32) | Some(NumSuffix::F64))
}

/// Parse a fully-buffered numeric token into an `Object`.
fn parse_number(buf: &str) -> Result<Object, Error> {
    let invalid = || Error::InvalidNumberLiteral(buf.to_string());

    let neg = buf.starts_with('-');
    let unsigned = buf.trim_start_matches(['+', '-']);
    let clean: String = unsigned.chars().filter(|c| *c != '_').collect();

    // A number must contain at least one digit; otherwise tokens like ".",
    // "-", "e" would be misread (e.g. "." normalizing to "0.0").
    if !clean.chars().any(|c| c.is_ascii_digit()) {
        return Err(invalid());
    }

    // Radix integer literals: 0x / 0o / 0b
    let radix_prefix = if clean.len() >= 2 { Some(&clean[0..2]) } else { None };
    if let Some(pfx) = radix_prefix {
        let radix = match pfx {
            "0x" | "0X" => Some(16u32),
            "0o" | "0O" => Some(8u32),
            "0b" | "0B" => Some(2u32),
            _ => None,
        };
        if let Some(radix) = radix {
            let rest = &clean[2..];
            let (digits, suffix) = split_suffix(rest);
            if is_float_suffix(suffix) || digits.is_empty() {
                return Err(invalid());
            }
            let mag = i64::from_str_radix(digits, radix).map_err(|_| invalid())?;
            let val = if neg { -mag } else { mag };
            return Ok(make_int(val, suffix));
        }
    }

    // Float when it carries a float suffix, a '.', or an exponent.
    let (body, suffix) = split_suffix(&clean);
    let looks_float = is_float_suffix(suffix)
        || body.contains('.')
        || body.contains('e')
        || body.contains('E');

    if looks_float {
        // normalize ".5" -> "0.5", "1." -> "1.0"
        let mut norm = body.to_string();
        if norm.starts_with('.') {
            norm.insert(0, '0');
        }
        if norm.ends_with('.') {
            norm.push('0');
        }
        let mag: f64 = norm.parse().map_err(|_| invalid())?;
        let val = if neg { -mag } else { mag };
        Ok(make_float(val, suffix))
    } else {
        let mag = i64::from_str_radix(body, 10).map_err(|_| invalid())?;
        let val = if neg { -mag } else { mag };
        Ok(make_int(val, suffix))
    }
}

fn make_int(val: i64, suffix: Option<NumSuffix>) -> Object {
    match suffix {
        Some(ns) => Object::IntWithSuffix(val, ns),
        None => Object::Int(val),
    }
}

fn make_float(val: f64, suffix: Option<NumSuffix>) -> Object {
    match suffix {
        Some(ns) => Object::FloatWithSuffix(val, ns),
        None => Object::Float(val),
    }
}
