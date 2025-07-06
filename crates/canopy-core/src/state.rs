use crate::{Result, error, viewport::ViewPort};
use convert_case::{Case, Casing};
use std::sync::atomic::AtomicU64;

static CURRENT_ID: AtomicU64 = AtomicU64::new(0);

pub fn valid_nodename_char(c: char) -> bool {
    (c.is_ascii_lowercase() || c.is_ascii_digit()) || c == '_'
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
        write!(f, "{}#{}", self.name, self.id)
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
    pub id: u64,

    /// If this is equal to the global render_gen, we render during the current
    /// sweep.
    pub render_gen: u64,

    /// This node's focus generation. We increment the global focus counter when
    /// focus changes, invalidating the current focus generation without having
    /// to update all node states.
    pub focus_gen: u64,

    /// Set to be equal to the focus_gen during a pre-render sweep, if focus has
    /// changed.
    pub focus_path_gen: u64,

    // The last render sweep during which this node held focus.
    pub rendered_focus_gen: u64,

    /// Set to the `render_gen` during the pre-render sweep if focus has
    /// changed, and this node was either on the old focus path, or is on the
    /// new focus path.
    pub focus_path_render_gen: u64,

    /// The view for this node. The inner rectangle always has the same size as
    /// the screen_area.
    pub viewport: ViewPort,

    // Is this node hidden?
    pub hidden: bool,

    // Has this node been initialized? This is used to determine if we need to
    // call the poll function during the pre-render sweep.
    pub initialized: bool,

    // Set while inside `Node::layout` to detect recursive layout calls.
    pub in_layout: bool,
}

impl NodeState {
    /// Set the node's position within the parent canvas. This should only be called by the parent
    /// node.
    pub fn set_position(&mut self, p: crate::geom::Point) {
        self.viewport.set_position(p)
    }

    /// Set the size of the node's canvas.
    pub fn set_canvas(&mut self, sz: crate::geom::Expanse) {
        self.viewport.set_canvas(sz);
    }

    /// Set the node's view - that is the portion of the node that is displayed. The view rectangle
    /// is relative to the node's canvas, and must be fully contained within it. This method will
    /// clamp the view rectangle to fit within the canvas size if it's larger than the canvas.
    pub fn set_view(&mut self, view: crate::geom::Rect) {
        self.viewport.set_view(view);
    }

    /// Set the node size and the target view size at the same time.
    pub fn fit_size(&mut self, size: crate::geom::Expanse, view_size: crate::geom::Expanse) {
        self.viewport.fit_size(size, view_size);
    }

    /// Scroll the view to the specified position.
    pub fn scroll_to(&mut self, x: u32, y: u32) {
        self.viewport.scroll_to(x, y);
    }

    /// Scroll the view by the given offsets.
    pub fn scroll_by(&mut self, x: i32, y: i32) {
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
            in_layout: false,
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

    /// Set our canvas size.
    fn set_canvas(&mut self, sz: crate::geom::Expanse) {
        self.state_mut().set_canvas(sz);
    }

    /// Set our view position and size.
    fn set_view(&mut self, view: crate::geom::Rect) {
        self.state_mut().set_view(view);
    }

    /// Set both the canvas size and the view to fill the target size.
    fn fill(&mut self, sz: crate::geom::Expanse) -> Result<()> {
        self.state_mut().set_canvas(sz);
        self.state_mut().set_view(sz.rect());
        Ok(())
    }

    /// Wrap around a child by laying it out in our viewport, then seting our canvas size to match.
    fn wrap(&mut self, child: ViewPort) -> Result<()> {
        self.set_canvas(child.canvas());
        self.set_view(child.view());
        Ok(())
    }

    fn fit_size(&mut self, size: crate::geom::Expanse, view_size: crate::geom::Expanse) {
        self.state_mut().fit_size(size, view_size);
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
