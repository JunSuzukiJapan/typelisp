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
    // --- reader (M1) ---
    InvalidNumberLiteral(String),
    IllegalEndOfBlockComment,
    IllegalEndOfChar,
    UnknownCharName(String),
    IllegalEscapeSequence(char),
    IllegalHashSyntax(char),
    // --- types (M2) ---
    InvalidTypeSyntax(String),
    UnknownType(String),
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
            Error::IllegalEndOfString => write!(f, "unexpected end of input while reading string"),
            Error::IllegalEndOfEscapeSequence => write!(f, "unexpected end of input in escape sequence"),
            Error::UnmatchedParen => write!(f, "unmatched ')'"),
            Error::IllegalEndWhileReadingList => write!(f, "unexpected end of input while reading list"),
            Error::UnmatchedParenWhileReadingConsPair => write!(f, "unmatched ')' while reading cons pair"),
            Error::InternalErrorEvalNotCons => write!(f, "internal error: eval target is not a cons"),
            Error::CallNotFunction => write!(f, "attempt to call a non-function"),
            Error::NoSuchFunction(name) => write!(f, "no such function: {}", name),
            Error::ArgIsNotCons => write!(f, "argument is not a cons"),
            Error::AddNotNumber => write!(f, "argument to + is not a number"),
            Error::FailInTryAsBasicValue => write!(f, "failed to convert value to a basic value"),
            Error::InvalidNumberLiteral(s) => write!(f, "invalid number literal: {}", s),
            Error::IllegalEndOfBlockComment => write!(f, "unexpected end of input in block comment (missing |#)"),
            Error::IllegalEndOfChar => write!(f, "unexpected end of input in character literal"),
            Error::UnknownCharName(s) => write!(f, "unknown character name: #\\{}", s),
            Error::IllegalEscapeSequence(c) => write!(f, "illegal escape sequence: \\{}", c),
            Error::IllegalHashSyntax(c) => write!(f, "illegal '#' syntax: #{}", c),
            Error::InvalidTypeSyntax(s) => write!(f, "invalid type syntax: {}", s),
            Error::UnknownType(s) => write!(f, "unknown type: {}", s),
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