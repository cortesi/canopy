use std::{
    fmt::Display,
    sync::{mpsc, MutexGuard, PoisonError},
};

#[cfg(test)]
use crate::backend::test::CanvasBuf;
use crate::backend::test::TestBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(PartialEq, Eq, Error, Debug, Clone)]
pub struct ParseError {}

impl Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(PartialEq, Eq, Error, Debug, Clone)]
pub enum Error {
    #[error("focus")]
    Focus(String),
    #[error("render")]
    Render(String),
    #[error("geometry")]
    Geometry(String),
    #[error("layout")]
    Layout(String),
    #[error("runloop")]
    RunLoop(String),
    #[error("internal")]
    Internal(String),
    #[error("invalid")]
    Invalid(String),
    #[error("unknown command")]
    UnknownCommand(String),

    #[error("parse error")]
    Parse(ParseError),

    #[error("script run error")]
    Script(String),

    /// No result was generated on node traversal
    #[error("no result")]
    NoResult,
}

impl From<mpsc::RecvError> for Error {
    fn from(e: mpsc::RecvError) -> Self {
        Error::RunLoop(e.to_string())
    }
}

impl From<PoisonError<MutexGuard<'_, TestBuf>>> for Error {
    fn from(e: PoisonError<MutexGuard<'_, TestBuf>>) -> Self {
        Error::RunLoop(e.to_string())
    }
}

#[cfg(test)]
impl From<PoisonError<MutexGuard<'_, CanvasBuf>>> for Error {
    fn from(e: PoisonError<MutexGuard<'_, CanvasBuf>>) -> Self {
        Error::RunLoop(e.to_string())
    }
}
