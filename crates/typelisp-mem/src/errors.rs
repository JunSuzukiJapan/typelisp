use std::{fmt, error};
use std::sync::Arc;

/// A source location: which file, and the 1-based line/column span within it.
///
/// `line`/`col` are where the located thing *starts*; `end_line`/`end_col`
/// are the position just past its last character (exclusive, still 1-based),
/// so a span covers `[start, end)` and containment is `start <= p < end`.
/// A `Loc` whose end equals its start is a *degenerate* span: only the start
/// point is known (e.g. a read error, or a node synthesized by macro
/// expansion). [`fmt::Display`] prints the start point only (`file:line:col`).
///
/// Attached to an [`Error`] via [`Error::at`] / [`Error::At`] so every message
/// can point at the exact spot in the user's `.typl` source where the problem
/// occurred. `file` is shared (`Arc<str>`) because a single source string
/// produces many data (and thus potentially many located errors), all naming
/// the same file — and an `Arc` because the heap's location table, which
/// holds one per interned location, is shared by every thread stepping tasks
/// on that heap (`docs/dev/os-threads-design.md`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Loc {
    pub file: Arc<str>,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl Loc {
    /// A degenerate (point-only) location: the end is set equal to the start.
    /// Use [`Loc::with_end`] afterwards when the true extent is known.
    pub fn new(file: Arc<str>, line: u32, col: u32) -> Loc {
        Loc { file, line, col, end_line: line, end_col: col }
    }

    /// This location with its exclusive end position set.
    pub fn with_end(mut self, end_line: u32, end_col: u32) -> Loc {
        self.end_line = end_line;
        self.end_col = end_col;
        self
    }

    /// Whether this is a point-only location (no known extent).
    pub fn is_degenerate(&self) -> bool {
        self.end_line == self.line && self.end_col == self.col
    }

    /// Whether the (1-based) position `line:col` falls inside this span
    /// (`start <= p < end`). Always false for a degenerate span.
    pub fn contains(&self, line: u32, col: u32) -> bool {
        (self.line, self.col) <= (line, col) && (line, col) < (self.end_line, self.end_col)
    }
}

impl fmt::Display for Loc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.col)
    }
}

#[derive(Debug)]
pub enum Error {
    ReadError(String),
    IllegalEndOfString,
    IllegalEndOfEscapeSequence,
    UnmatchedParen,
    IllegalEndWhileReadingList,
    UnmatchedParenWhileReadingConsPair,
    InternalErrorEvalNotCons,
    CallNotFunction,
    NoSuchFunction(String),
    ArgIsNotCons,
    AddNotNumber,
    FailInTryAsBasicValue,
    /// The cons arena is full and a GC could not reclaim any cell.
    HeapExhausted,
    /// `car`/`cdr`/`set-car`/`set-cdr` applied to a non-cons, non-nil value.
    NotACons,
    /// An improper list where a proper list was required.
    ImproperList,
    /// A malformed type expression or a type error during checking.
    TypeError(String),
    /// A `use` named a module that is not in the registry and might live in a
    /// not-yet-loaded source file. Carries the path's segments so a driver
    /// that knows about the filesystem (`src/project.rs`'s loader) can map
    /// them to a `.typl` file, load it, and retry the `use` — the checker
    /// itself never touches the filesystem. A driver with no loader (or one
    /// whose load attempt finds no file) reports this as "unresolved".
    ModuleNotLoaded(Vec<String>),
    /// An error carrying the source location where it occurred. Wraps the
    /// underlying error unchanged; [`fmt::Display`] prefixes it with
    /// `file:line:col: `. Constructed via [`Error::at`], which never
    /// double-wraps (an already-located error keeps its original, innermost
    /// location).
    At(Loc, Box<Error>),
}

impl Error {
    /// Attach a source location to this error. If the error already carries a
    /// location, it is returned unchanged — the first (innermost, most
    /// specific) location attached wins.
    pub fn at(self, loc: Loc) -> Error {
        match self {
            Error::At(..) => self,
            other => Error::At(loc, Box::new(other)),
        }
    }

    /// The underlying error with any location wrapper(s) stripped — for code
    /// that needs to match on the error *kind* (e.g. deciding whether a read
    /// error means "need more input") regardless of location.
    pub fn kind(&self) -> &Error {
        match self {
            Error::At(_, inner) => inner.kind(),
            other => other,
        }
    }

    /// The source location attached to this error, if any.
    pub fn loc(&self) -> Option<&Loc> {
        match self {
            Error::At(loc, _) => Some(loc),
            _ => None,
        }
    }

    /// Consume this error and return its underlying kind with any location
    /// wrapper(s) stripped — the owned counterpart of [`Error::kind`], for
    /// code (mainly tests) that wants to pattern-match the error by value
    /// without caring about location.
    pub fn into_kind(self) -> Error {
        match self {
            Error::At(_, inner) => inner.into_kind(),
            other => other,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::ReadError(s) => write!(f, "read error: {}", s),
            Error::IllegalEndOfString => write!(f, "unexpected end of input inside a string"),
            Error::IllegalEndOfEscapeSequence => {
                write!(f, "unexpected end of input inside an escape sequence")
            }
            Error::UnmatchedParen => write!(f, "unmatched `)`"),
            Error::IllegalEndWhileReadingList => write!(f, "unexpected end of input while reading a list"),
            Error::UnmatchedParenWhileReadingConsPair => {
                write!(f, "unmatched `)` while reading a cons pair")
            }
            Error::InternalErrorEvalNotCons => write!(f, "internal error: eval of a non-cons form"),
            Error::CallNotFunction => write!(f, "attempt to call a non-function"),
            Error::NoSuchFunction(name) => write!(f, "no such function: {}", name),
            Error::ArgIsNotCons => write!(f, "argument is not a cons"),
            Error::AddNotNumber => write!(f, "argument to `+` is not a number"),
            Error::FailInTryAsBasicValue => write!(f, "failed to convert to a basic value"),
            Error::HeapExhausted => write!(f, "heap exhausted: no cons cell could be reclaimed"),
            Error::NotACons => write!(f, "value is not a cons"),
            Error::ImproperList => write!(f, "improper list where a proper list was required"),
            Error::TypeError(s) => write!(f, "type error: {}", s),
            Error::ModuleNotLoaded(segs) => write!(f, "use: unresolved `{}`", segs.join("::")),
            Error::At(loc, inner) => write!(f, "{}: {}", loc, inner),
        }
    }
}

impl error::Error for Error {
    // fn source(&self) -> Option<&(dyn error::Error + 'static)> {
    //     match *self {
    //         _ => None,
    //     }
    // }
}