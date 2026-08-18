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
/// isn't one of the six punctuation/space classes below — including a lone
/// `:` not paired into a `::` (so operator symbols like `<=` lex into a token
/// sequence that simply contains no `ColonColon`/`Comma`, and callers that
/// only look for those leave such tokens alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameTok<'a> {
    Ident(&'a str),
    ColonColon,
    Lt,
    Gt,
    Comma,
    /// The unit type spelled `()`, e.g. the first argument of
    /// `result<(), file-error>`. Only ever appears inside a `<...>` argument
    /// list: `(`/`)` are reader delimiters, so the sole way they end up inside
    /// a token at all is `read::reader::extend_angle_token` letting the pair
    /// through while the angle brackets are still open. Recognized here — and
    /// only as the exact two-character pair — so the type parser sees a unit
    /// type rather than an `Ident("()")` that would become a path segment.
    Unit,
    /// A run of spaces/tabs. Only a token the reader deliberately extended
    /// across whitespace can contain one (`read::reader::extend_angle_token`,
    /// e.g. `vector<:dyn drawable>`); every other token is whitespace-free
    /// because whitespace terminates it. Emitting it as its own token — rather
    /// than letting `Ident` swallow it — is what lets the type parser see
    /// `:dyn` and the trait name that follows as two separate idents.
    Space,
}

/// Tokenizes a name string left to right.
pub struct NameLexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> NameLexer<'a> {
    pub fn new(src: &'a str) -> Self {
        NameLexer { src, pos: 0 }
    }

    /// Byte offset just past the most recently returned token (i.e. where
    /// the next token, if any, begins). Lets a caller slice `src` between
    /// token boundaries without re-deriving them.
    pub fn pos(&self) -> usize {
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
            b'(' if bytes.get(self.pos + 1) == Some(&b')') => {
                self.pos += 2;
                Some(NameTok::Unit)
            }
            b' ' | b'\t' => {
                while self.pos < bytes.len() && matches!(bytes[self.pos], b' ' | b'\t') {
                    self.pos += 1;
                }
                Some(NameTok::Space)
            }
            _ => {
                let start = self.pos;
                while !ends_ident(bytes, self.pos) {
                    self.pos += 1;
                }
                Some(NameTok::Ident(&self.src[start..self.pos]))
            }
        }
    }
}

/// Whether an identifier run must stop at `pos` — end of input, or any of
/// the punctuation/space classes [`NameLexer::next`] dispatches on ahead of
/// its `Ident` arm. Kept as its own function so the two stay in sync by
/// being read side by side: the arms below are the same set, in the same
/// order, as the match there.
fn ends_ident(bytes: &[u8], pos: usize) -> bool {
    match bytes.get(pos) {
        None => true,
        Some(b'<' | b'>' | b',' | b' ' | b'\t') => true,
        // The two-character classes end an ident only as a complete pair; a
        // lone `:`/`(` is an ordinary ident character (`:dyn`, and a `(`
        // that only a reader-extended token could contain at all).
        Some(b':') => bytes.get(pos + 1) == Some(&b':'),
        Some(b'(') => bytes.get(pos + 1) == Some(&b')'),
        _ => false,
    }
}
