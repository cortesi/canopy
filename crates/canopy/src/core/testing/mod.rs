mod clock;

/// Explicit monotonic clock for deterministic runtime tests.
pub use clock::ManualClock;

/// Backend utilities for tests.
pub mod backend;
/// Buffer testing utilities.
pub mod buf;
/// Shared native and adapter publication contract fixture.
pub mod contracts;
/// Event notifications for stepping adapter integration tests.
pub mod driver;
/// Dummy context for tests.
pub mod dummyctx;
/// Grid test helpers.
pub mod grid;
/// Harness for node testing.
pub mod harness;
/// Typecheck assertions for tracked Luau sources.
pub mod luau;
/// Shared property-model failure diagnostics.
#[cfg(test)]
pub(crate) mod model;
/// Test tree helpers.
#[cfg(test)]
pub(crate) mod ttree;
