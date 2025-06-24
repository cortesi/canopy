use crate::{error, viewport::ViewPort, Result};
use convert_case::{Case, Casing};
use std::sync::atomic::AtomicU64;

pub use canopy_derive::StatefulNode;

static CURRENT_ID: AtomicU64 = AtomicU64::new(0);

pub fn valid_nodename_char(c: char) -> bool {
    (c.is_ascii_alphanumeric() && c.is_lowercase()) || c == '_'
}

pub fn valid_nodename(name: &str) -> bool {
    name.chars().all(valid_nodename_char)
}

/// A unique ID for a node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId {
    id: u64,
    name: NodeName,
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.id)
    }
}

impl PartialEq<u64> for NodeId {
    fn eq(&self, other: &u64) -> bool {
        self.id == *other
    }
}

/// A node name, which consists of lowercase ASCII alphanumeric characters, plus
/// underscores.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeName {
    name: String,
}

impl NodeName {
    /// Create a new NodeName, returning an error if the string contains invalid
    /// characters.
    fn new(name: &str) -> Result<Self> {
        if !valid_nodename(name) {
            return Err(error::Error::Invalid(name.into()));
        }
        Ok(Self {
            name: name.to_string(),
        })
    }

    /// Takes a string and munges it into a valid node name. It does this by
    /// first converting the string to snake case, then removing all invalid
    /// characters.
    pub fn convert(name: &str) -> Self {
        let name = name.to_case(Case::Snake);
        NodeName {
            name: name.chars().filter(|x| valid_nodename_char(*x)).collect(),
        }
    }
}

impl std::fmt::Display for NodeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl PartialEq<&str> for NodeName {
    fn eq(&self, other: &&str) -> bool {
        self.name == *other
    }
}

impl PartialEq<String> for NodeName {
    fn eq(&self, other: &String) -> bool {
        self.name == *other
    }
}

/// Converts a string into the standard node name format, and errors if it
/// doesn't comply to the node name standard.
impl TryFrom<&str> for NodeName {
    type Error = error::Error;
    fn try_from(name: &str) -> Result<Self> {
        Self::new(name)
    }
}

/// An opaque structure that Canopy uses to track node state. Each Node has to
/// keep a NodeState structure, and offer it up through the `Node::state()`
/// method on request.
#[derive(Debug, PartialEq, Eq)]
pub struct NodeState {
    // Unique node ID
    id: u64,

    /// If this is equal to the global render_gen, we render during the current
    /// sweep.
    pub(crate) render_gen: u64,
    /// This node's focus generation. We increment the global focus counter when
    /// focus changes, invalidating the current focus generation without having
    /// to update all node states.
    pub(crate) focus_gen: u64,
    /// Set to be equal to the focus_gen during a pre-render sweep, if focus has
    /// changed.
    pub(crate) focus_path_gen: u64,
    // The last render sweep during which this node held focus.
    pub(crate) rendered_focus_gen: u64,
    /// Set to the `render_gen` during the pre-render sweep if focus has
    /// changed, and this node was either on the old focus path, or is on the
    /// new focus path.
    pub(crate) focus_path_render_gen: u64,

    /// The view for this node. The inner rectangle always has the same size as
    /// the screen_area.
    pub(crate) viewport: ViewPort,
    // Is this node hidden?
    pub(crate) hidden: bool,
    // Has this node been initialized? This is used to determine if we need to
    // call the poll function during the pre-render sweep.
    pub(crate) initialized: bool,
}

impl NodeState {
    /// Set the node's position within the parent canvas.
    pub fn set_position(&mut self, p: crate::geom::Point, parent: crate::geom::Expanse) -> crate::Result<()> {
        self.viewport.set_position(p, parent)
    }

    /// Set the size of the node's canvas.
    pub fn set_canvas(&mut self, sz: crate::geom::Expanse) {
        self.viewport.set_canvas(sz);
    }

    /// Set the portion of the node that is displayed.
    pub fn set_view(&mut self, view: crate::geom::Rect) -> crate::Result<()> {
        self.viewport.set_view(view)
    }

    /// Constrain this viewport so that its screen rectangle falls within the
    /// specified parent viewport.
    pub fn constrain(&mut self, parent: ViewPort) {
        self.viewport.constrain(parent);
    }

    /// Set the node size and the target view size at the same time.
    pub fn fit_size(&mut self, size: crate::geom::Expanse, view_size: crate::geom::Expanse) {
        self.viewport.fit_size(size, view_size);
    }

    /// Scroll the view to the specified position.
    pub fn scroll_to(&mut self, x: u16, y: u16) {
        self.viewport.scroll_to(x, y);
    }

    /// Scroll the view by the given offsets.
    pub fn scroll_by(&mut self, x: i16, y: i16) {
        self.viewport.scroll_by(x, y);
    }

    /// Scroll the view up by the height of the view rectangle.
    pub fn page_up(&mut self) {
        self.viewport.page_up();
    }

    /// Scroll the view down by the height of the view rectangle.
    pub fn page_down(&mut self) {
        self.viewport.page_down();
    }

    /// Scroll the view up by one line.
    pub fn scroll_up(&mut self) {
        self.viewport.scroll_up();
    }

    /// Scroll the view down by one line.
    pub fn scroll_down(&mut self) {
        self.viewport.scroll_down();
    }

    /// Scroll the view left by one line.
    pub fn scroll_left(&mut self) {
        self.viewport.scroll_left();
    }

    /// Scroll the view right by one line.
    pub fn scroll_right(&mut self) {
        self.viewport.scroll_right();
    }
}

/// The node state object - each node needs to keep one of these, and offer it
/// up by implementing the StatefulNode trait.
impl Default for NodeState {
    fn default() -> Self {
        let id = CURRENT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        NodeState {
            id,
            render_gen: 0,
            focus_gen: 0,
            focus_path_gen: 0,
            focus_path_render_gen: 0,
            rendered_focus_gen: 0,
            hidden: false,
            viewport: ViewPort::default(),
            initialized: false,
        }
    }
}

/// The interface implemented by all nodes that track state.
pub trait StatefulNode {
    /// The name of this node, used for debugging and command dispatch.
    fn name(&self) -> NodeName;

    /// Get a reference to the node's state object.
    fn state(&self) -> &NodeState;

    /// Get a mutable reference to the node's state object.
    fn state_mut(&mut self) -> &mut NodeState;

    /// Hides the element and all its descendants from rendering. The nodes are
    /// still included in the tree.
    fn hide(&mut self) {
        self.state_mut().hidden = true;
    }

    /// Hides the element
    fn unhide(&mut self) {
        self.state_mut().hidden = false;
    }

    /// Is this element hidden?
    fn is_hidden(&self) -> bool {
        self.state().hidden
    }

    /// Get the node's `ViewPort`.
    fn vp(&self) -> ViewPort {
        self.state().viewport
    }

    /// A unique ID for this node.
    fn id(&self) -> NodeId {
        NodeId {
            id: self.state().id,
            name: self.name(),
        }
    }

    /// Has this node been initialized? That is, has its poll function been
    /// called for the first time to schedule future polls.
    fn is_initialized(&self) -> bool {
        self.state().initialized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;

    #[test]
    fn nodename() -> Result<()> {
        assert_eq!(NodeName::try_from("foo").unwrap(), "foo");
        assert!(NodeName::try_from("Foo").is_err());
        assert_eq!(NodeName::convert("Foo"), "foo");
        assert_eq!(NodeName::convert("FooBar"), "foo_bar");
        assert_eq!(NodeName::convert("FooBar Voing"), "foo_bar_voing");

        Ok(())
    }
}
