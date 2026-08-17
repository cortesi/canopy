//! The canopy integration test binary.
//!
//! Cargo builds one crate per file directly under `tests/`, and every one of them links the
//! Luau-bearing library. Keeping a single root here keeps that to one link step. Cargo resolves
//! a test root's modules against `tests/` itself, so each module names its path explicitly.

/// Command dispatch, argument, and error integration tests.
#[path = "it/commands.rs"]
mod commands;
/// Helpers shared by more than one integration module.
#[path = "it/common.rs"]
mod common;
/// Focus traversal integration tests.
#[path = "it/focus.rs"]
mod focus;
/// Layout integration tests.
#[path = "it/layout.rs"]
mod layout;
/// Tracked Luau source typecheck test.
#[path = "it/luau_check.rs"]
mod luau_check;
/// Node render integration tests.
#[path = "it/node_render.rs"]
mod node_render;
/// Mount hook integration tests.
#[path = "it/on_mount.rs"]
mod on_mount;
/// Luau scripting framework and command integration tests.
#[path = "it/script.rs"]
mod script;
/// Tree traversal and hit-testing integration tests.
#[path = "it/tree.rs"]
mod tree;
/// Viewport scrolling integration tests.
#[path = "it/viewport.rs"]
mod viewport;
