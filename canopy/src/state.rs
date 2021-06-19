use crate::geom::Rect;
use crate::NodeState;

pub use canopy_derive::StatefulNode;

pub trait StateFulNode {
    fn state(&self) -> &NodeState;

    fn state_mut(&mut self) -> &mut NodeState;

    fn rect(&self) -> Option<Rect>;
}
