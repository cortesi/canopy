use crossterm;
use std::error;
use std::sync::mpsc;

use thiserror::Error;

pub type TResult<T> = std::result::Result<T, Box<dyn error::Error + Send + Sync>>;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("unknown")]
    Unknown(String),
    #[error("locate")]
    Locate(String),
    #[error("focus")]
    Focus(String),
    #[error("taint")]
    Taint(String),
    #[error("render")]
    Render(String),
    #[error("geometry")]
    Geometry(String),
    #[error("tick")]
    Tick(String),
    #[error("layout")]
    Layout(String),
    #[error("runloop")]
    RunLoop(String),
}

impl From<crossterm::ErrorKind> for Error {
    fn from(e: crossterm::ErrorKind) -> Self {
        Error::Render(e.to_string())
    }
}

impl From<mpsc::RecvError> for Error {
    fn from(e: mpsc::RecvError) -> Self {
        Error::RunLoop(e.to_string())
    }
}
