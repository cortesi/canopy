//! Status items: the styled runs a widget exposes for a status line.
//!
//! A widget describes its status with
//! [`Widget::status`](crate::Widget::status), and a status line asks its
//! sources for their items as it renders, so the line follows their state with
//! no update calls. Each run names a style path, which resolves under the layer
//! of the line that draws it, so a host styles status through its ordinary
//! rules.

use std::borrow::Cow;

use crate::text;

/// One styled run of status text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusSpan {
    /// Style path the run resolves, under the drawing line's layer.
    pub style: Cow<'static, str>,
    /// Text the run shows.
    pub text: String,
}

impl StatusSpan {
    /// A run that resolves `style`.
    pub fn new(style: impl Into<Cow<'static, str>>, text: impl Into<String>) -> Self {
        Self {
            style: style.into(),
            text: text.into(),
        }
    }

    /// A run in the line's own text style.
    pub fn plain(text: impl Into<String>) -> Self {
        Self::new("text", text)
    }

    /// Return the columns the run takes.
    pub fn width(&self) -> u32 {
        text::width(&self.text)
    }
}

/// The end of a status line an item gathers at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusSide {
    /// The start of the line, after the items before it.
    #[default]
    Start,
    /// The end of the line, before the items after it.
    End,
}

/// One unit of status: a group of runs that shows, shortens, or hides as a
/// whole.
///
/// When a line cannot fit every item, it shortens the item of lowest priority
/// that has a compact form, and otherwise hides it, until the rest fit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusItem {
    /// Stable identity, for scripts and tests.
    pub id: Cow<'static, str>,
    /// The end of the line the item gathers at.
    pub side: StatusSide,
    /// How long the item keeps its place: a line short of room shortens and
    /// hides items of lower priority first.
    pub priority: i32,
    /// The runs the item shows.
    pub spans: Vec<StatusSpan>,
    /// Shorter runs the item shows before it hides, when the full ones do not
    /// fit.
    pub compact: Option<Vec<StatusSpan>>,
}

impl StatusItem {
    /// An empty item at the start of the line, of priority zero.
    pub fn new(id: impl Into<Cow<'static, str>>) -> Self {
        Self {
            id: id.into(),
            side: StatusSide::Start,
            priority: 0,
            spans: Vec::new(),
            compact: None,
        }
    }

    /// Gather the item at the end of the line.
    #[must_use]
    pub fn at_end(mut self) -> Self {
        self.side = StatusSide::End;
        self
    }

    /// Set the item's priority.
    #[must_use]
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Append a run that resolves `style`.
    #[must_use]
    pub fn span(mut self, style: impl Into<Cow<'static, str>>, text: impl Into<String>) -> Self {
        self.spans.push(StatusSpan::new(style, text));
        self
    }

    /// Append runs.
    #[must_use]
    pub fn spans(mut self, spans: impl IntoIterator<Item = StatusSpan>) -> Self {
        self.spans.extend(spans);
        self
    }

    /// Set the shorter runs the item shows before it hides.
    #[must_use]
    pub fn compact(mut self, spans: impl IntoIterator<Item = StatusSpan>) -> Self {
        self.compact = Some(spans.into_iter().collect());
        self
    }

    /// Return whether the item shows nothing.
    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(|span| span.text.is_empty())
    }
}

/// Return the columns `spans` take.
pub fn spans_width(spans: &[StatusSpan]) -> u32 {
    spans
        .iter()
        .map(StatusSpan::width)
        .fold(0, u32::saturating_add)
}
