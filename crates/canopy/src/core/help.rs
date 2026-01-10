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
    script::ScriptId,
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

/// Extract a command ID from a script source if it's a simple command call.
///
/// Detects patterns like `owner::command()` or `owner::command(args)`.
fn extract_command_id(source: &str) -> Option<&str> {
    let source = source.trim();
    // Match: identifier::identifier( or identifier(
    // Stop at the opening paren
    let paren_pos = source.find('(')?;
    let candidate = source[..paren_pos].trim();

    // Validate it looks like a command call (alphanumeric + underscores + optional ::)
    if candidate.is_empty() {
        return None;
    }
    for c in candidate.chars() {
        if !c.is_alphanumeric() && c != '_' && c != ':' {
            return None;
        }
    }
    // Must contain :: for namespaced commands
    if !candidate.contains("::") {
        return None;
    }
    Some(candidate)
}

/// Derive a human-readable label for a binding target.
///
/// For scripts that are simple command calls (e.g., `root::focus_next()`), looks up
/// the command's documentation. For compound scripts, falls back to the source.
pub fn binding_label<F>(target: &BindingTarget, commands: &CommandSet, script_source: F) -> String
where
    F: Fn(ScriptId) -> Option<String>,
{
    match target {
        BindingTarget::Script(sid) => {
            if let Some(source) = script_source(*sid) {
                let source = source.trim();
                // Try to extract a simple command call and use its docs
                if let Some(cmd_id) = extract_command_id(source)
                    && let Some(spec) = commands.get(cmd_id)
                    && let Some(desc) = spec.doc.short
                {
                    return desc.to_string();
                }
                // Fall back to showing the script source
                return source.to_string();
            }
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

// ============================================================================
// Owned types for storage
// ============================================================================

use crate::path::{PathMatch, PathMatcher};

/// Owned version of [`HelpBinding`] for storage without lifetimes.
#[derive(Debug, Clone)]
pub struct OwnedHelpBinding {
    /// The input (key or mouse) that triggers this binding.
    pub input: InputSpec,
    /// The mode this binding belongs to.
    pub mode: String,
    /// The original path filter string.
    pub path_filter: String,
    /// Classification of how this binding matched.
    pub kind: BindingKind,
    /// Human-readable label derived from command docs or script source.
    pub label: String,
    /// Match metadata for sorting.
    pub path_match: PathMatch,
}

/// Owned version of [`HelpCommand`] for storage without lifetimes.
#[derive(Debug, Clone)]
pub struct OwnedHelpCommand {
    /// Command identifier.
    pub id: String,
    /// Owner type name (None for Free commands).
    pub owner: Option<String>,
    /// Short description.
    pub short: Option<String>,
    /// Resolution if the command has a target.
    pub resolution: Option<CommandResolution>,
    /// Whether this command is hidden from help.
    pub hidden: bool,
}

impl OwnedHelpCommand {
    /// Returns true if this command can be dispatched from the current context.
    pub fn is_available(&self) -> bool {
        self.resolution.is_some()
    }
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
    /// Commands with their availability status.
    pub commands: Vec<OwnedHelpCommand>,
}

impl<'a> HelpSnapshot<'a> {
    /// Convert to an owned version for storage.
    pub fn to_owned(&self) -> OwnedHelpSnapshot {
        let bindings = self
            .bindings
            .iter()
            .map(|b| {
                let path_match = PathMatcher::new(b.path_filter)
                    .ok()
                    .and_then(|matcher| matcher.check_match(&self.focus_path))
                    .unwrap_or(PathMatch {
                        literals: 0,
                        depth: 0,
                        anchored_end: false,
                    });
                OwnedHelpBinding {
                    input: b.input,
                    mode: b.mode.to_string(),
                    path_filter: b.path_filter.to_string(),
                    kind: b.kind,
                    label: b.label.clone(),
                    path_match,
                }
            })
            .collect();

        let commands = self
            .commands
            .iter()
            .filter(|c| !c.spec.doc.hidden)
            .map(|c| OwnedHelpCommand {
                id: c.spec.id.0.to_string(),
                owner: c.owner.map(|s| s.to_string()),
                short: c.spec.doc.short.map(|s| s.to_string()),
                resolution: c.resolution,
                hidden: c.spec.doc.hidden,
            })
            .collect();

        OwnedHelpSnapshot {
            focus_path: self.focus_path.clone(),
            input_mode: self.input_mode.to_string(),
            bindings,
            commands,
        }
    }
}
