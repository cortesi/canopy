//! The canopy integration test binary.
//!
//! Cargo builds one crate per file directly under `tests/`, and every one of
//! them links the Luau-bearing library. Keeping a single root here keeps that
//! to one link step. The conventional `it/main.rs` root resolves its
//! modules from the same directory.

/// API capture path checks.
mod api_capture;
/// Command dispatch, argument, and error integration tests.
mod commands;
/// Shared native and adapter contract trace.
mod contracts;
/// Focus traversal integration tests.
mod focus;
/// Layout integration tests.
mod layout;
/// Tracked Luau source typecheck test.
mod luau_check;
/// Node render integration tests.
mod node_render;
/// Mount hook integration tests.
mod on_mount;
/// Luau scripting framework and command integration tests.
mod script;
/// Callback completion and deferred teardown integration tests.
mod teardown;
/// Tree traversal and hit-testing integration tests.
mod tree;
/// Native mutation and publication integration tests.
mod turn;
/// Viewport scrolling integration tests.
mod viewport;

/// Scoped script lookup integration tests.
mod semantic_script;

/// Immutable script observation integration tests.
mod snapshot_script;
