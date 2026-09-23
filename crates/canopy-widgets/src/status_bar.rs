//! Single-line status bar container.

use std::mem;

use canopy::{
    Context, ContextExt, NodeName, Render, ViewContext, Widget,
    error::Result,
    geom::{Line, Size},
    layout::{Layout, LayoutOverride, MeasureConstraints, Measurement, Sizing},
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
        rndr.fill("status_bar", area, ' ')
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
        let root = context.node_id();
        // The left zone gives up columns first; the right zone keeps what its
        // widgets measure.
        let left = context.add_child_to(root, Container::new(Layout::row()).with_name("left"))?;
        context.set_layout_override_of(
            left.into(),
            LayoutOverride {
                width: Some(Sizing::Flex(1)),
                min_width: Some(None),
                max_width: Some(None),
                ..LayoutOverride::new()
            },
        )?;
        let right = context.add_child_to(
            root,
            Container::new(Layout::row().gap(HINT_GAP)).with_name("right"),
        )?;
        context.set_layout_override_of(
            right.into(),
            LayoutOverride {
                width: Some(Sizing::Measure),
                min_width: Some(None),
                max_width: Some(None),
                ..LayoutOverride::new()
            },
        )?;
        for widget in mem::take(&mut self.left) {
            context.add_child_to_boxed(left.into(), widget)?;
        }
        for widget in mem::take(&mut self.right) {
            context.add_child_to_boxed(right.into(), widget)?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("status_bar")
    }
}

/// A key and what it does, drawn for a status bar.
///
/// The key paints `status_bar/key` and its label paints `status_bar/text`, so
/// the hint takes the bar's ground from the `status_bar` rule. An empty key
/// measures nothing and draws nothing.
pub struct KeyHint {
    /// Key name, drawn with `status_bar/key`.
    key: String,
    /// Short action name, drawn after the key as `: label`.
    label: String,
}

impl KeyHint {
    /// Construct a hint for one key.
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
        }
    }

    /// Replace the key and its label.
    pub fn set(&mut self, key: impl Into<String>, label: impl Into<String>) {
        self.key = key.into();
        self.label = label.into();
    }

    /// Return the key name.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Return the action name.
    pub fn label(&self) -> &str {
        &self.label
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

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 || self.key.is_empty() {
            return Ok(());
        }
        let row = area.line(0)?;
        let key_width = text_width(&self.key).min(row.w);
        if key_width > 0 {
            rndr.text(
                "status_bar/key",
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
                "status_bar/text",
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

/// Return the columns `text` occupies, saturating on absurd lengths.
fn text_width(text: &str) -> u32 {
    u32::try_from(UnicodeWidthStr::width(text)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hint_takes_its_key_label_and_separator() {
        let hint = KeyHint::new("ctrl-g", "help");
        assert_eq!(hint.width(), 12);
    }

    #[test]
    fn a_hint_with_no_label_takes_only_its_key() {
        let hint = KeyHint::new("x", "");
        assert_eq!(hint.width(), 1);
    }

    #[test]
    fn a_hint_with_no_key_takes_no_columns() {
        let hint = KeyHint::new("", "help");
        assert_eq!(hint.width(), 0);
    }

    #[test]
    fn setting_a_hint_replaces_both_parts() {
        let mut hint = KeyHint::new("x", "old");
        hint.set("ctrl-g", "help");
        assert_eq!(hint.key(), "ctrl-g");
        assert_eq!(hint.label(), "help");
        assert_eq!(hint.width(), 12);
    }
}
