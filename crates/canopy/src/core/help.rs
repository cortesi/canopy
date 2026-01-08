//! Help snapshot API for context-aware help.
//!
//! This module provides types and functions to generate a snapshot of available bindings and
//! commands from a given focus context. The snapshot can be used to build help overlays,
//! command palettes, or discoverable keybinding references.

use crate::{
    commands::{CommandResolution, CommandSet, CommandSpec},
    core::{
        NodeId,
        inputmap::{BindingTarget, InputSpec},
    },
    path::Path,
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
    /// The input (key or mouse) that triggers this binding.
    pub input: InputSpec,
    /// The mode this binding belongs to.
    pub mode: &'a str,
    /// The original path filter string.
    pub path_filter: &'a str,
    /// The binding target (script or command).
    pub target: &'a BindingTarget,
    /// Classification of how this binding matched.
    pub kind: BindingKind,
    /// Human-readable label derived from command docs or script source.
    pub label: String,
}

/// A command in the help snapshot.
#[derive(Debug, Clone)]
pub struct HelpCommand<'a> {
    /// Owner type name (`None` for Free commands).
    pub owner: Option<&'static str>,
    /// Command specification.
    pub spec: &'a CommandSpec,
    /// Resolution if the command has a target, or `None` if no target exists.
    pub resolution: Option<CommandResolution>,
}

impl<'a> HelpCommand<'a> {
    /// Returns true if this command can be dispatched from the current context.
    pub fn is_available(&self) -> bool {
        self.resolution.is_some()
    }
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

impl<'a> HelpSnapshot<'a> {
    /// Return only bindings that would fire as pre-event overrides.
    pub fn pre_event_bindings(&self) -> Vec<&HelpBinding<'a>> {
        self.bindings
            .iter()
            .filter(|b| b.kind == BindingKind::PreEventOverride)
            .collect()
    }

    /// Return only bindings that would fire as post-event fallbacks.
    pub fn fallback_bindings(&self) -> Vec<&HelpBinding<'a>> {
        self.bindings
            .iter()
            .filter(|b| b.kind == BindingKind::PostEventFallback)
            .collect()
    }

    /// Return only commands that are currently available (have a target).
    pub fn available_commands(&self) -> Vec<&HelpCommand<'a>> {
        self.commands.iter().filter(|c| c.is_available()).collect()
    }

    /// Return only commands that are currently unavailable (no target).
    pub fn unavailable_commands(&self) -> Vec<&HelpCommand<'a>> {
        self.commands.iter().filter(|c| !c.is_available()).collect()
    }
}

/// Derive a human-readable label for a binding target.
pub fn binding_label(target: &BindingTarget, commands: &CommandSet) -> String {
    match target {
        BindingTarget::Script(_sid) => {
            // For scripts, we could potentially look up the script source,
            // but for now just return a generic label
            "script".to_string()
        }
        BindingTarget::Command(inv) => {
            // Try to get description from command spec
            if let Some(spec) = commands.get(inv.id.0)
                && let Some(desc) = spec.doc.short
            {
                return desc.to_string();
            }
            // Fall back to command ID
            inv.id.0.to_string()
        }
    }
}
