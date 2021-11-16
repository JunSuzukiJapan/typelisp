#[derive(Debug)]
pub enum Error {
    ReadError(String),
    IllegalEndOfString,
    IllegalEndOfEscapeSequence,
}

impl Error {
    pub fn read_error(s: String) -> Error {
        Error::ReadError(s)
    }
}