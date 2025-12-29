use std::result;

use canopy::error as canopy_error;
use image::ImageError;
use thiserror::Error;

/// Errors produced by the imgview example.
#[derive(Debug, Error)]
pub enum Error {
    /// Image loading or decoding failed.
    #[error("image error: {0}")]
    Image(#[from] ImageError),

    /// Canopy returned an error.
    #[error("canopy error: {0}")]
    Canopy(#[from] canopy_error::Error),
}

/// Result type for the imgview example.
pub type Result<T> = result::Result<T, Error>;
