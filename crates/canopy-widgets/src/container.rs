//! A container that supplies a layout and nothing else.

use canopy::{
    NodeName, Widget,
    layout::{Align, Direction, Edges, Layout},
};

/// A widget that lays out its children and has no other behavior.
///
/// Parents adjust how each child sizes through layout overrides. Use
/// [`Container::with_name`] to keep a path segment that scripts or bindings
/// match.
pub struct Container {
    /// Layout applied to the children.
    layout: Layout,
    /// Path segment for this node.
    name: NodeName,
}

impl Container {
    /// Construct a container with any layout.
    pub fn new(layout: Layout) -> Self {
        Self {
            layout,
            name: NodeName::convert("container"),
        }
    }

    /// Fill the available space and place children in a row.
    pub fn row() -> Self {
        Self::new(Layout::fill().direction(Direction::Row))
    }

    /// Fill the available space and stack children in a column.
    pub fn column() -> Self {
        Self::new(Layout::fill().direction(Direction::Column))
    }

    /// Fill the available space and overlap children, the last on top.
    pub fn stack() -> Self {
        Self::new(Layout::fill().direction(Direction::Stack))
    }

    /// Fill the available space and center each child on both axes, overlapped.
    ///
    /// For a dimmed overlay, push an effect on the background content with
    /// `c.push_effect(background_id, effects::brightness(0.5))`. The centered
    /// content stays at full brightness because it is a sibling of the dimmed
    /// content, not a descendant. Insert it inside a parent that uses `Stack`
    /// layout so it can overlay the existing view.
    pub fn center() -> Self {
        Self::new(
            Layout::fill()
                .direction(Direction::Stack)
                .align_horizontal(Align::Center)
                .align_vertical(Align::Center),
        )
        .with_name("center")
    }

    /// Fill the available space and inset the children by `padding`.
    pub fn padded(padding: Edges) -> Self {
        Self::new(Layout::fill().padding(padding)).with_name("pad")
    }

    /// Name this node's path segment.
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = NodeName::convert(name);
        self
    }
}

impl Widget for Container {
    fn layout(&self) -> Layout {
        self.layout
    }

    fn name(&self) -> NodeName {
        self.name.clone()
    }
}
