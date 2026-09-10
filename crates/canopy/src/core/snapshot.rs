//! Owned observations of one successfully prepared frame.

use super::{
    context::CoreViewContext,
    termbuf::TermBuf,
    view::View,
    world::{Core, WidgetOperation},
};
use crate::{
    Cell, FrameId, NodeId, SemanticIdentity,
    commands::{ArgValue, CommandStatus},
    error::Result,
    geom::{Rect, Size},
    layout::Display,
};

/// Optional application observations, without arbitrary widget serialization.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WidgetSemantics {
    /// Application role independent of structural wrappers.
    pub role: Option<String>,
    /// Human-readable accessible label.
    pub label: Option<String>,
    /// Explicitly exposed value; sensitive values must be omitted.
    pub value: Option<String>,
    /// Whether this widget is selected, when applicable.
    pub selected: Option<bool>,
    /// Stable application keys selected by a collection.
    pub selected_keys: Vec<ArgValue>,
    /// Availability of the widget's primary action.
    pub action_status: Option<CommandStatus>,
}

/// Owned identity, geometry, and semantics for one live arena node.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeSnapshot {
    /// Arena identity at publication, retained even after removal.
    pub id: NodeId,
    /// Structural parent at publication.
    pub parent: Option<NodeId>,
    /// Children in source order.
    pub children: Vec<NodeId>,
    /// Widget path name.
    pub name: String,
    /// Explicit application identity.
    pub semantic_identity: Option<SemanticIdentity>,
    /// Whether the root tree contains this node.
    pub attached: bool,
    /// Attached with no hidden or display-suppressed ancestor.
    pub displayed: bool,
    /// Intersection with the viewport and all ancestor content clips.
    pub intersects_viewport: bool,
    /// Current view geometry, absent when not displayed.
    pub view: Option<View>,
    /// Whether this node owns focus.
    pub focused: bool,
    /// Widget-provided observations captured after paint.
    pub semantics: WidgetSemantics,
}

/// Immutable data from one successfully prepared frame.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameSnapshot {
    /// Publication generation shared with turn outcomes.
    pub frame_id: FrameId,
    /// Dimensions of the rendered cells.
    pub viewport: Size,
    /// All live arena nodes, including detached trees.
    pub nodes: Vec<NodeSnapshot>,
    /// Styled terminal cells in row-major order.
    pub cells: Vec<Cell>,
    /// Focus owner at publication.
    pub focus: Option<NodeId>,
}

/// Capture fallible widget observations before replacing any published state.
pub(super) fn capture(core: &Core, frame_id: FrameId, buffer: &TermBuf) -> Result<FrameSnapshot> {
    let viewport = buffer.size();
    let mut nodes = Vec::with_capacity(core.nodes.len());
    let screen = Rect::new(0, 0, viewport.w, viewport.h);
    let mut stack: Vec<_> = core
        .nodes
        .iter()
        .filter(|(id, node)| *id != core.root && node.parent.is_none())
        .map(|(id, _)| (id, false, false, None))
        .collect();
    stack.reverse();
    stack.push((core.root, true, true, Some(screen)));
    while let Some((id, attached, parent_displayed, clip)) = stack.pop() {
        let node = &core.nodes[id];
        let displayed = parent_displayed && !node.hidden && node.layout.display != Display::None;
        let intersects_viewport = displayed
            && clip
                .and_then(|clip| node.view.outer.intersect_rect(clip))
                .is_some();
        let child_clip = clip.and_then(|clip| node.view.content.intersect_rect(clip));
        let semantics =
            core.with_widget(id, WidgetOperation::access("semantics"), |widget, core| {
                widget.semantics(&CoreViewContext::new(core, id))
            })??;
        nodes.push(NodeSnapshot {
            id,
            parent: node.parent,
            children: node.children.clone(),
            name: node.name.to_string(),
            semantic_identity: node.semantic_identity.clone(),
            attached,
            displayed,
            intersects_viewport,
            view: displayed.then_some(node.view),
            focused: core.focus == Some(id),
            semantics,
        });
        stack.extend(
            node.children
                .iter()
                .rev()
                .map(|child| (*child, attached, displayed, child_clip)),
        );
    }
    let cells = buffer.cells.clone();
    Ok(FrameSnapshot {
        frame_id,
        viewport,
        nodes,
        cells,
        focus: core.focus,
    })
}
