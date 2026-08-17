//! Help snapshot API for context-aware help.
//!
//! This module provides types and functions to generate a snapshot of available bindings and
//! commands from a given focus context. The snapshot can be used to build help overlays,
//! command palettes, or discoverable keybinding references.

use crate::{
    commands::{CommandResolution, CommandSpec},
    core::{
        NodeId,
        inputmap::{BindingId, InputSpec},
    },
    path::Path,
    script::LuauFunctionId,
};

/// Classification of how a binding matched the focus path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingKind {
    /// Binding matched exactly at the focus path (pre-event override).
    PreEventOverride,
    /// Binding matched as a fallback after event bubbling (post-event fallback).
    PostEventFallback,
}

/// A binding in the help snapshot.
#[derive(Debug, Clone)]
pub struct HelpBinding<'a> {
    /// Identifier of the matched binding.
    pub id: BindingId,
    /// The input (key or mouse) that triggers this binding.
    pub input: InputSpec,
    /// The mode this binding belongs to.
    pub mode: &'a str,
    /// The original path filter string.
    pub path_filter: &'a str,
    /// The stored Luau closure this binding calls.
    pub target: LuauFunctionId,
    /// Classification of how this binding matched.
    pub kind: BindingKind,
    /// Human-readable label derived from command docs or script source.
    pub label: String,
}

/// A command in the help snapshot.
#[derive(Debug, Clone)]
pub struct HelpCommand<'a> {
    /// Command specification.
    pub spec: &'a CommandSpec,
    /// Resolution if the command has a target, or `None` if no target exists.
    pub resolution: Option<CommandResolution>,
}

/// A contextual help snapshot combining bindings and commands.
#[derive(Debug)]
pub struct HelpSnapshot<'a> {
    /// Current focus node ID.
    pub focus: NodeId,
    /// Path from root to focus.
    pub focus_path: Path,
    /// Current input mode name.
    pub input_mode: &'a str,
    /// Bindings that match the current context.
    pub bindings: Vec<HelpBinding<'a>>,
    /// Commands with their availability status.
    pub commands: Vec<HelpCommand<'a>>,
}

impl HelpSnapshot<'_> {
    /// Convert to an owned version for storage.
    pub fn to_owned(&self) -> OwnedHelpSnapshot {
        let bindings = self
            .bindings
            .iter()
            .map(|b| OwnedHelpBinding {
                input: b.input,
                kind: b.kind,
                label: b.label.clone(),
            })
            .collect();

        OwnedHelpSnapshot {
            focus_path: self.focus_path.clone(),
            input_mode: self.input_mode.to_string(),
            bindings,
        }
    }
}

/// Derive a human-readable label for a binding.
///
/// Falls back to a generic label when the stored closure carries no label.
pub fn binding_label(
    target: LuauFunctionId,
    luau_label: impl Fn(LuauFunctionId) -> Option<String>,
) -> String {
    luau_label(target).unwrap_or_else(|| "script".to_string())
}

// ============================================================================
// Owned types for storage
// ============================================================================

/// Owned version of [`HelpBinding`] for storage without lifetimes.
#[derive(Debug, Clone)]
pub struct OwnedHelpBinding {
    /// The input (key or mouse) that triggers this binding.
    pub input: InputSpec,
    /// Classification of how this binding matched.
    pub kind: BindingKind,
    /// Human-readable label derived from the stored closure.
    pub label: String,
}

/// Owned version of [`HelpSnapshot`] for storage without lifetimes.
#[derive(Debug, Clone)]
pub struct OwnedHelpSnapshot {
    /// Path from root to focus.
    pub focus_path: Path,
    /// Current input mode name.
    pub input_mode: String,
    /// Bindings that match the current context.
    pub bindings: Vec<OwnedHelpBinding>,
}
