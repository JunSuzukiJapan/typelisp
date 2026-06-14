use std::{fmt, error};

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
}

impl Error {
    pub fn read_error(s: String) -> Error {
        Error::ReadError(s)
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