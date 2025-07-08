#![allow(clippy::new_without_default)]
//! A library for building terminal UIs.

// Re-export everything from canopy-core
pub use canopy_core::*;

// Re-export widgets from canopy-widgets
pub use canopy_widgets as widgets;

// Re-export canopy-specific types
pub use canopy_core::{
    Binder, Canopy, DefaultBindings, FocusableNode, Input, InputMap, InputMode, Loader, Poller,
    collect_focusable_nodes, find_focus_target, find_focused_node,
};
pub use canopy_widgets::Root;

// Hide the test utils from docs. We need to expose it for integration tests, but it's not for external use.
#[doc(hidden)]
pub use canopy_core::tutils;
