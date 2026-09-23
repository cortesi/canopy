//! Single-line status bar container.

use std::mem;

use canopy::{
    Context, ContextExt, NodeName, Register, Setup, ViewContext, ViewContextExt, Widget,
    commands::CommandCall,
    error::Result,
    geom::{Line, Size},
    input::{BindingTarget, IntentName},
    layout::{Layout, LayoutOverride, MeasureConstraints, Measurement, Sizing},
    render::Render,
    style::roles,
};
use unicode_width::UnicodeWidthStr;

use crate::Container;

/// Columns between two widgets pinned to the right edge.
const HINT_GAP: u32 = 2;

/// A single-line status bar for the top or bottom edge of an application.
///
/// The bar is a container. It fills its row with `status_bar`, keeps the
/// widgets added with [`StatusBar::with_left`] at the start, and pins the
/// widgets added with [`StatusBar::with_right`] to the end. The left zone
/// takes the columns that remain; the right zone measures its widgets, so it
/// keeps its natural width and squeezes the left zone first.
///
/// A widget that paints a `status_bar` style path takes the bar's ground: a
/// role that names no paint falls back to the `status_bar` rule, so
/// `status_bar/text` and `status_bar/key` inherit the panel background.
///
/// The bar holds no focus and takes no input. A host gives it one row, usually
/// as a column's last or first child.
pub struct StatusBar {
    /// Widgets pinned to the left edge, in reading order.
    left: Vec<Box<dyn Widget>>,
    /// Widgets pinned to the right edge, in reading order.
    right: Vec<Box<dyn Widget>>,
}

impl StatusBar {
    /// Construct an empty status bar.
    pub fn new() -> Self {
        Self {
            left: Vec::new(),
            right: Vec::new(),
        }
    }

    /// Add a widget at the left edge, after the widgets already added.
    #[must_use]
    pub fn with_left(mut self, widget: impl Into<Box<dyn Widget>>) -> Self {
        self.left.push(widget.into());
        self
    }

    /// Add a widget at the right edge, after the widgets already added.
    #[must_use]
    pub fn with_right(mut self, widget: impl Into<Box<dyn Widget>>) -> Self {
        self.right.push(widget.into());
        self
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for StatusBar {
    fn layout(&self) -> Layout {
        // The bar spans its parent's width and keeps one row, so a host can
        // add it as a column's first or last child without an override.
        Layout::row().flex_horizontal(1).fixed_height(1)
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        rndr.push_layer("status_bar");
        rndr.fill(roles::BACKGROUND, area, ' ')
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
        let root = context.node_id();
        // The left zone gives up columns first; the right zone keeps what its
        // widgets measure.
        let left = context.add_child(root, Container::new(Layout::row()).with_name("left"))?;
        context.set_layout_override(
            left.into(),
            LayoutOverride {
                width: Some(Sizing::Flex(1)),
                min_width: Some(None),
                max_width: Some(None),
                ..LayoutOverride::new()
            },
        )?;
        let right = context.add_child(
            root,
            Container::new(Layout::row().gap(HINT_GAP)).with_name("right"),
        )?;
        context.set_layout_override(
            right.into(),
            LayoutOverride {
                width: Some(Sizing::Measure),
                min_width: Some(None),
                max_width: Some(None),
                ..LayoutOverride::new()
            },
        )?;
        for widget in mem::take(&mut self.left) {
            let child = context.create_detached_boxed(widget)?;
            context.attach(left.into(), child)?;
        }
        for widget in mem::take(&mut self.right) {
            let child = context.create_detached_boxed(widget)?;
            context.attach(right.into(), child)?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("status_bar")
    }
}

/// A key and what it does, drawn for a status bar.
///
/// The hint names an action, not a key: [`KeyHint::for_command`] and
/// [`KeyHint::for_intent`] resolve the key through binding discovery when the
/// hint mounts and whenever a binding or the mode stack changes, so the hint
/// follows the bindings. [`KeyHint::register`] installs that resync. It paints
/// the bare `key` and `text` parts of whatever layer holds it, so inside a
/// status bar they resolve `status_bar/key` and `status_bar/text`. A hint whose
/// action no key reaches measures nothing and draws nothing.
pub struct KeyHint {
    /// Action whose key the hint names.
    target: BindingTarget,
    /// Short action name, drawn after the key as `: label`.
    label: String,
    /// Label of the key last resolved, empty when no key reaches the action.
    key: String,
}

impl KeyHint {
    /// Construct a hint for the key that runs `call`.
    pub fn for_command(call: CommandCall, label: impl Into<String>) -> Self {
        Self::new(BindingTarget::Command(call), label)
    }

    /// Construct a hint for the key that offers the intent `name`.
    pub fn for_intent(name: IntentName, label: impl Into<String>) -> Self {
        Self::new(BindingTarget::Intent(name), label)
    }

    /// Construct a hint for `target`, with no key resolved yet.
    fn new(target: BindingTarget, label: impl Into<String>) -> Self {
        Self {
            target,
            label: label.into(),
            key: String::new(),
        }
    }

    /// Return the label of the key last resolved.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Return the label shown beside the key.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Resolve the key label from the current bindings and focus.
    fn resolve(&mut self, ctx: &dyn ViewContext) {
        self.key = ctx
            .key_for(&self.target)
            .map(|key| key.label())
            .unwrap_or_default();
    }

    /// Return the columns the hint takes, or 0 when it draws nothing.
    fn width(&self) -> u32 {
        if self.key.is_empty() {
            return 0;
        }
        let mut width = text_width(&self.key);
        if !self.label.is_empty() {
            width = width
                .saturating_add(text_width(": "))
                .saturating_add(text_width(&self.label));
        }
        width
    }
}

impl Widget for KeyHint {
    fn layout(&self) -> Layout {
        Layout::column().fixed_height(1)
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.clamp(Size::new(self.width(), 1))
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.resolve(ctx);
        Ok(())
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 || self.key.is_empty() {
            return Ok(());
        }
        let row = area.line(0)?;
        let key_width = text_width(&self.key).min(row.w);
        if key_width > 0 {
            rndr.text(
                roles::KEY,
                Line::new(row.tl.x, row.tl.y, key_width),
                &self.key,
            )?;
        }
        if self.label.is_empty() {
            return Ok(());
        }
        let x = row.tl.x.saturating_add(key_width);
        let width = text_width(": ").saturating_add(text_width(&self.label));
        let width = width.min(row.tl.x.saturating_add(row.w).saturating_sub(x));
        if width > 0 {
            rndr.text(
                roles::TEXT,
                Line::new(x, row.tl.y, width),
                &format!(": {}", self.label),
            )?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("key_hint")
    }
}

impl Register for KeyHint {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.register_binding_hook("key_hints", refresh_key_hints);
        Ok(())
    }
}

/// Resolve every mounted hint's key again after the bindings changed.
fn refresh_key_hints(ctx: &mut dyn Context) -> Result<()> {
    let hints: Vec<_> = ctx.descendants::<KeyHint>(ctx.root_id()).collect();
    for hint in hints {
        ctx.with_widget_mut(hint, |hint: &mut KeyHint, ctx| {
            hint.resolve(ctx);
            Ok(())
        })?;
    }
    Ok(())
}

/// Return the columns `text` occupies, saturating on absurd lengths.
fn text_width(text: &str) -> u32 {
    u32::try_from(UnicodeWidthStr::width(text)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use canopy::input::IntentName;

    use super::*;

    /// A hint for a placeholder intent, with `key` resolved.
    fn hint(key: &str, label: &str) -> KeyHint {
        let mut hint = KeyHint::for_intent(IntentName::new("app.help").unwrap(), label);
        hint.key = key.into();
        hint
    }

    #[test]
    fn a_hint_takes_its_key_label_and_separator() {
        assert_eq!(hint("ctrl+g", "help").width(), 12);
    }

    #[test]
    fn a_hint_with_no_label_takes_only_its_key() {
        assert_eq!(hint("x", "").width(), 1);
    }

    #[test]
    fn a_hint_with_no_key_takes_no_columns() {
        assert_eq!(hint("", "help").width(), 0);
    }
}
