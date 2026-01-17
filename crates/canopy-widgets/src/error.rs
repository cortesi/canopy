use std::{io::Error as IoError, result::Result as StdResult};

use thiserror::Error;

/// Errors emitted by canopy-widgets helpers.
#[derive(Debug, Error)]
pub enum Error {
    /// Font parsing failed.
    #[error("font loading failed: {0}")]
    FontLoad(&'static str),
    /// Glyph ramp did not include any characters.
    #[error("glyph ramp must contain at least one character")]
    EmptyGlyphRamp,
    /// Font format is not supported.
    #[error("unsupported font format: {0}")]
    UnsupportedFormat(&'static str),
    /// I/O error while reading font bytes.
    #[error("font I/O failed: {0}")]
    Io(#[from] IoError),
}

/// Result type for canopy-widgets helpers.
pub type Result<T> = StdResult<T, Error>;
