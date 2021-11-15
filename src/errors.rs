#[derive(Debug)]
pub enum Error {
    ReadError(String),
}

impl Error {
    pub fn read_error(s: String) -> Error {
        Error::ReadError(s)
    }
}