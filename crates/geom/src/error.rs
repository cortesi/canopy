#[derive(Debug, Clone)]
pub enum Error {
    Geometry(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Geometry(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
