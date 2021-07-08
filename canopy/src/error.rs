use crossterm;
use std::error;
use std::sync::mpsc;

use thiserror::Error;

pub type TResult<T> = std::result::Result<T, Box<dyn error::Error + Send + Sync>>;

#[derive(Error, Debug)]
pub enum CanopyError {
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

impl From<crossterm::ErrorKind> for CanopyError {
    fn from(e: crossterm::ErrorKind) -> Self {
        CanopyError::Render(e.to_string())
    }
}

impl From<mpsc::RecvError> for CanopyError {
    fn from(e: mpsc::RecvError) -> Self {
        CanopyError::RunLoop(e.to_string())
    }
}
