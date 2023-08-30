//! Utilities for working with a Canopy node tree.

use crate::{
    geom::Point,
    node::{postorder, preorder, Node, Walk},
    path::*,
    NodeId, Result,
};

/// Call a closure on the currently focused node and all its ancestors to the
/// root. If the closure returns Walk::Handle, traversal stops. Handle::Skip is
/// ignored.
pub fn walk_focus_path_e<R>(
    focus_gen: u64,
    root: &mut dyn Node,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<Walk<R>>,
) -> Result<Option<R>> {
    let mut focus_seen = false;
    Ok(postorder(root, &mut |x| -> Result<Walk<R>> {
        Ok(if focus_seen {
            f(x)?
        } else if x.is_hidden() {
            // Hidden nodes don't hold focus
            Walk::Continue
        } else if x.state().focus_gen == focus_gen {
            focus_seen = true;
            // Force skip on continue so we trigger skipping in the postorder
            // traversal.
            match f(x)? {
                Walk::Skip => Walk::Skip,
                Walk::Continue => Walk::Skip,
                Walk::Handle(t) => Walk::Handle(t),
            }
        } else {
            Walk::Continue
        })
    })?
    .value())
}

pub enum Locate<R> {
    // Note the match and continue traversal.
    Match(R),
    // Match and don't traverse children.
    Stop(R),
    // Continue looking.
    Continue,
}

/// Calls a closure on the root node under (x, y), then recurses up the tree to all children falling under the same
/// point. The function returns the last node that the closure returned a value for, either with Locate::Match
/// (continuing taversal) or Locate::Stop(stopping traversal). Hidden nodes and nodes that do not contain the location
/// point are skipped.
pub fn locate<R>(
    root: &mut dyn Node,
    p: impl Into<Point>,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<Locate<R>>,
) -> Result<Option<R>> {
    let p = p.into();
    let mut result = None;
    preorder(root, &mut |inner| -> Result<Walk<R>> {
        Ok(if !inner.is_hidden() {
            let a = inner.vp().screen_rect();
            if a.contains_point(p) {
                match f(inner)? {
                    Locate::Continue => Walk::Continue,
                    Locate::Stop(x) => {
                        result = Some(x);
                        Walk::Skip
                    }
                    Locate::Match(x) => {
                        result = Some(x);
                        Walk::Continue
                    }
                }
            } else {
                Walk::Skip
            }
        } else {
            Walk::Skip
        })
    })?;
    Ok(result)
}

/// Find the ID of the leaf node at a given point.
pub fn node_at(root: &mut dyn Node, p: impl Into<Point>) -> Result<Option<NodeId>> {
    locate(root, p, &mut |x| -> Result<Locate<NodeId>> {
        Ok(Locate::Match(x.id()))
    })
}

/// Call a closure on the node with the specified `id`, and all its ancestors to
/// the specified `root`.
pub fn walk_to_root<T>(
    root: &mut dyn Node,
    id: T,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<()>,
) -> Result<()>
where
    T: Into<NodeId>,
{
    let mut seen = false;
    let uid = id.into();
    postorder(root, &mut |x| -> Result<Walk<()>> {
        Ok(if seen {
            f(x)?;
            Walk::Continue
        } else if x.id() == uid {
            seen = true;
            f(x)?;
            Walk::Skip
        } else {
            Walk::Continue
        })
    })?;
    Ok(())
}

/// Return the node path for a specified node id, relative to the specified
///`root`.
pub fn node_path<T>(id: T, root: &mut dyn Node) -> Path
where
    T: Into<NodeId>,
{
    let mut path = Vec::new();
    walk_to_root(root, id, &mut |n| -> Result<()> {
        path.insert(0, n.name().to_string());
        Ok(())
    })
    .unwrap();
    path.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tutils::*;
    use crate::StatefulNode;

    #[test]
    fn tnode_path() -> Result<()> {
        run(|_c, _, mut root| {
            assert_eq!(node_path(root.id(), &mut root), Path::new(&["r"]));
            assert_eq!(
                node_path(root.a.a.id(), &mut root),
                Path::new(&["r", "ba", "ba_la"])
            );
            Ok(())
        })?;
        Ok(())
    }
}
