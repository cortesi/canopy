#![deny(unsafe_code)]
//! Draw pictures and large text with block mosaic characters.
//!
//! Each terminal cell is an 8×8 matrix of pixels. For each cell, the matcher
//! finds the symbol and the two pens, foreground and background, that
//! reproduce the matrix at the least cost: the squared error, plus the stray
//! of any mark that nothing near it in the matrix explains, such as a spur
//! of ink below a stroke. The symbols come from families of block mosaic
//! characters, grouped into tiers by how widely terminals draw them well.
//!
//! - [`geometry`] describes the shape of each character in cell coordinates.
//!   The masks that the matcher searches, the screenshots that Canopy draws,
//!   and the evaluation all come from it.
//! - [`symbols`] holds the families, the tiers, and the compiled sets.
//! - [`rgb_into`], [`coverage_into`], and [`fixed_into`] match a raster.
//!   [`clear_into`] matches a raster with alpha against a clear ground.
//! - [`raster`] prepares an image's raster: resampling in linear light, and
//!   compositing over the ground of each cell.

/// Matching against a clear ground.
mod clear;
pub mod geometry;
/// Matching cells to symbols.
mod matcher;
pub mod raster;
pub mod symbols;

pub use clear::clear_into;
pub use matcher::{Cell, RasterError, RasterView, coverage_into, fixed_into, rgb_into};
pub use symbols::{Family, SymbolSet, Tier};
