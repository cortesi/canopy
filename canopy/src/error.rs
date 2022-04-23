use std::sync::mpsc;
use std::sync::{MutexGuard, PoisonError};

use crate::backend::test::TestBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
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
    #[error("unknown action")]
    UnknownAction(String),
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
