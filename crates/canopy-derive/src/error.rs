use std::result::Result as StdResult;

use proc_macro_error::{Diagnostic, Level};

/// Local result type for macro parsing.
pub type Result<T> = StdResult<T, Error>;

/// Errors raised while parsing command metadata.
#[derive(PartialEq, Eq, thiserror::Error, Debug, Clone)]
pub enum Error {
    /// Failed to parse an attribute payload.
    #[error("parse error: {0}")]
    Parse(String),
    /// Unsupported argument or return type.
    #[error("unsupported: {0}")]
    Unsupported(String),
}

impl From<Error> for Diagnostic {
    fn from(error: Error) -> Self {
        Self::spanned(
            proc_macro2::Span::call_site(),
            Level::Error,
            format!("{error}"),
        )
    }
}
