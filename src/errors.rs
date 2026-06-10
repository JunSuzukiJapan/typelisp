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
    // --- AST construction (M3) ---
    MalformedSpecialForm(String),
    ExpectedSymbol(String),
    ExpectedTypedParam(String),
    InvalidReceiver(String),
    InvalidLetBinding(String),
    InvalidPattern(String),
    // --- type checking (M5) ---
    TypeMismatch { expected: crate::Type, found: crate::Type },
    BranchTypeMismatch(String),
    NotNumeric(crate::Type),
    NotMutable(String),
    UndefinedVariable(String),
    UnknownField { struct_name: String, field: String },
    NoSuchMethod { recv: String, name: String },
    AmbiguousMethod(String),
    NotCallable(String),
    ArityMismatch { name: String, expected: usize, found: usize },
    DuplicateDefinition(String),
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
            Error::MalformedSpecialForm(s) => write!(f, "malformed special form: {}", s),
            Error::ExpectedSymbol(s) => write!(f, "expected a symbol: {}", s),
            Error::ExpectedTypedParam(s) => write!(f, "expected a typed parameter (name type): {}", s),
            Error::InvalidReceiver(s) => write!(f, "invalid defmethod receiver: {}", s),
            Error::InvalidLetBinding(s) => write!(f, "invalid let binding: {}", s),
            Error::InvalidPattern(s) => write!(f, "invalid match pattern: {}", s),
            Error::TypeMismatch { expected, found } => {
                write!(f, "type mismatch: expected {:?}, found {:?}", expected, found)
            }
            Error::BranchTypeMismatch(s) => write!(f, "branches have incompatible types: {}", s),
            Error::NotNumeric(t) => write!(f, "expected a numeric type, found {:?}", t),
            Error::NotMutable(s) => write!(f, "cannot assign to immutable place: {}", s),
            Error::UndefinedVariable(s) => write!(f, "undefined variable: {}", s),
            Error::UnknownField { struct_name, field } => {
                write!(f, "struct {} has no field {}", struct_name, field)
            }
            Error::NoSuchMethod { recv, name } => {
                write!(f, "no method {} for receiver type {}", name, recv)
            }
            Error::AmbiguousMethod(s) => write!(f, "ambiguous method call: {}", s),
            Error::NotCallable(s) => write!(f, "not callable: {}", s),
            Error::ArityMismatch { name, expected, found } => {
                write!(f, "{} expects {} argument(s), got {}", name, expected, found)
            }
            Error::DuplicateDefinition(s) => write!(f, "duplicate definition: {}", s),
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