/// Backend utilities for tests.
pub mod backend;
/// Buffer testing utilities.
pub mod buf;
/// Dummy context for tests.
pub mod dummyctx;
/// Grid test helpers.
pub mod grid;
/// Harness for node testing.
pub mod harness;
/// Shared property-model failure diagnostics.
#[cfg(test)]
pub(crate) mod model;
/// Test tree helpers.
pub mod ttree;
