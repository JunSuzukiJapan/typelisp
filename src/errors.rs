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
        match *self {



            _ => write!(f, "some error occurred"),
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