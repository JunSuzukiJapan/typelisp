//! A minimal lexer for the qualified, optionally-generic name syntax shared
//! by the reader (deciding `Value::Symbol` vs `Value::Path` in
//! `read::reader`) and the type parser (`types::parse_type_name`):
//! `ident (:: ident)* (< args >)?`, e.g. `geometry::Pair<K,V>`.
//!
//! Both callers need to track `<>` nesting so a `::` inside a generic
//! argument (as in `Vec<a::b>`) is never mistaken for a path separator.
//! Tokenizing once here, instead of each caller re-scanning the raw string
//! with its own ad hoc depth counter, keeps that nesting logic in one place.

/// One token of the name grammar. `Ident` covers any run of characters that
/// isn't one of the four punctuation marks below — including a lone `:` not
/// paired into a `::` (so operator symbols like `<=` lex into a token
/// sequence that simply contains no `ColonColon`/`Comma`, and callers that
/// only look for those leave such tokens alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameTok<'a> {
    Ident(&'a str),
    ColonColon,
    Lt,
    Gt,
    Comma,
}

/// Tokenizes a name string left to right.
pub(crate) struct NameLexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> NameLexer<'a> {
    pub(crate) fn new(src: &'a str) -> Self {
        NameLexer { src, pos: 0 }
    }

    /// Byte offset just past the most recently returned token (i.e. where
    /// the next token, if any, begins). Lets a caller slice `src` between
    /// token boundaries without re-deriving them.
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }
}

impl<'a> Iterator for NameLexer<'a> {
    type Item = NameTok<'a>;

    fn next(&mut self) -> Option<NameTok<'a>> {
        let bytes = self.src.as_bytes();
        if self.pos >= bytes.len() {
            return None;
        }
        match bytes[self.pos] {
            b'<' => {
                self.pos += 1;
                Some(NameTok::Lt)
            }
            b'>' => {
                self.pos += 1;
                Some(NameTok::Gt)
            }
            b',' => {
                self.pos += 1;
                Some(NameTok::Comma)
            }
            b':' if bytes.get(self.pos + 1) == Some(&b':') => {
                self.pos += 2;
                Some(NameTok::ColonColon)
            }
            _ => {
                let start = self.pos;
                while self.pos < bytes.len()
                    && !matches!(bytes[self.pos], b'<' | b'>' | b',')
                    && !(bytes[self.pos] == b':' && bytes.get(self.pos + 1) == Some(&b':'))
                {
                    self.pos += 1;
                }
                Some(NameTok::Ident(&self.src[start..self.pos]))
            }
        }
    }
}
