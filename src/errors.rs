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