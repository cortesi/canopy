//! Utilities for working with a Canopy node tree.

use crate::{geom::Point, node::Node, path::*, NodeId, Result};

/// Walk is the return value from traversal closures.
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Walk<T> {
    /// Skip this node and continue walking. The meaning of Skip depends on the
    /// traversal function being used.
    Skip,
    /// Handle an event with a possible return value and stop walking.
    Handle(T),
    /// Continue walking, but don't mark the event as handled.
    Continue,
}

impl<T> Walk<T> {
    /// The handle value of the traversal, if any.
    pub fn value(self) -> Option<T> {
        match self {
            Walk::Handle(v) => Some(v),
            _ => None,
        }
    }
    /// Did the traversal return Handle?
    pub fn is_handled(&self) -> bool {
        match self {
            Walk::Handle(_) => true,
            _ => false,
        }
    }
    /// Did the traversal return Continue?
    pub fn is_continue(&self) -> bool {
        match self {
            Walk::Skip | Walk::Handle(_) => false,
            Walk::Continue => true,
        }
    }
}

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

/// A postorder traversal of the nodes under e.
///
/// - Walk::Skip causes stops further traversal of children, and all the nodes
/// in a path back to the root are visited.
/// - Walk::Handle stops the traversal and the contained value is returned.
/// - Any error return stops the traversal and the error is returned.
pub fn postorder<T>(
    e: &mut dyn Node,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<Walk<T>>,
) -> Result<Walk<T>> {
    let mut stop = None;
    e.children(&mut |x| {
        if stop.is_none() {
            let v = postorder(x, f)?;
            if !v.is_continue() {
                stop = Some(v)
            }
        }
        Ok(())
    })?;
    match stop {
        None => f(e),
        Some(v) => match v {
            Walk::Skip => {
                let v = f(e)?;
                if v.is_continue() {
                    Ok(Walk::Skip)
                } else {
                    Ok(v)
                }
            }
            Walk::Handle(t) => Ok(Walk::Handle(t)),
            _ => panic!("impossible"),
        },
    }
}

// A preorder traversal of the nodes under e.
///
/// - Walk::Skip prunes all children of the current node from the traversal.
/// - Walk::Handle stops the traversal and the contained value is returned.
/// - Any error return stops the traversal and the error is returned.
pub fn preorder<T>(
    e: &mut dyn Node,
    f: &mut dyn FnMut(&mut dyn Node) -> Result<Walk<T>>,
) -> Result<Walk<T>> {
    let mut res = f(e)?;
    if res.is_continue() {
        e.children(&mut |x| {
            if res.is_continue() {
                match preorder(x, f)? {
                    Walk::Skip => panic!("impossible"),
                    Walk::Continue => {}
                    Walk::Handle(t) => res = Walk::Handle(t),
                };
            }
            Ok(())
        })?;
    }
    // Skip is not propagated upwards, so we translate it to continue.
    Ok(match res {
        Walk::Skip => Walk::Continue,
        _ => res,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tutils::*;
    use crate::StatefulNode;
    use crate::*;

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

    /// Tiny helper to turn arrays into owned String vecs to ease comparison.
    fn vc(a: &[&str]) -> Vec<String> {
        a.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn tpreorder() -> Result<()> {
        fn trigger(name: &str, func: Result<Walk<()>>) -> (Vec<String>, Result<Walk<()>>) {
            let mut v: Vec<String> = vec![];
            let mut root = R::new();
            let res = preorder(&mut root, &mut |x| -> Result<Walk<()>> {
                v.push(x.name().to_string());
                if x.name() == name {
                    func.clone()
                } else {
                    Ok(Walk::Continue)
                }
            });
            (v, res)
        }

        assert_eq!(
            trigger("never", Ok(Walk::Skip)),
            (
                vc(&["r", "ba", "ba_la", "ba_lb", "bb", "bb_la", "bb_lb"]),
                Ok(Walk::Continue)
            )
        );

        // Skip
        assert_eq!(
            trigger("ba", Ok(Walk::Skip)),
            (vc(&["r", "ba", "bb", "bb_la", "bb_lb"]), Ok(Walk::Continue))
        );
        assert_eq!(
            trigger("r", Ok(Walk::Skip)),
            (vc(&["r"]), Ok(Walk::Continue))
        );

        // Handle
        assert_eq!(
            trigger("ba", Ok(Walk::Handle(()))),
            (vc(&["r", "ba"]), Ok(Walk::Handle(())))
        );
        assert_eq!(
            trigger("ba_la", Ok(Walk::Handle(()))),
            (vc(&["r", "ba", "ba_la"]), Ok(Walk::Handle(())))
        );

        // Error
        assert_eq!(
            trigger("ba_la".into(), Err(Error::NoResult)),
            (vc(&["r", "ba", "ba_la"]), Err(Error::NoResult))
        );
        assert_eq!(
            trigger("r".into(), Err(Error::NoResult)),
            (vc(&["r"]), Err(Error::NoResult))
        );

        Ok(())
    }

    #[test]
    fn tpostorder() -> Result<()> {
        fn trigger(name: &str, func: Result<Walk<()>>) -> (Vec<String>, Result<Walk<()>>) {
            let mut v: Vec<String> = vec![];
            let mut root = R::new();
            let res = postorder(&mut root, &mut |x| -> Result<Walk<()>> {
                v.push(x.name().to_string());
                if x.name() == name {
                    func.clone()
                } else {
                    Ok(Walk::Continue)
                }
            });
            (v, res)
        }

        // Skip
        assert_eq!(
            trigger("ba_la", Ok(Walk::Skip)),
            (vc(&["ba_la", "ba", "r"]), Ok(Walk::Skip))
        );

        assert_eq!(
            trigger("ba_lb", Ok(Walk::Skip)),
            (vc(&["ba_la", "ba_lb", "ba", "r"]), Ok(Walk::Skip))
        );
        assert_eq!(
            trigger("r", Ok(Walk::Skip)),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb", "r"]),
                Ok(Walk::Skip)
            )
        );
        assert_eq!(
            trigger("bb", Ok(Walk::Skip)),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb", "r"]),
                Ok(Walk::Skip)
            )
        );
        assert_eq!(
            trigger("ba", Ok(Walk::Skip)),
            (vc(&["ba_la", "ba_lb", "ba", "r"]), Ok(Walk::Skip))
        );

        // Handle
        assert_eq!(
            trigger("ba_la".into(), Ok(Walk::Handle(()))),
            (vc(&["ba_la"]), Ok(Walk::Handle(())))
        );
        assert_eq!(
            trigger("bb".into(), Ok(Walk::Handle(()))),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb"]),
                Ok(Walk::Handle(()))
            )
        );

        // Error
        assert_eq!(
            trigger("ba_la".into(), Err(Error::NoResult)),
            (vc(&["ba_la"]), Err(Error::NoResult))
        );
        assert_eq!(
            trigger("bb".into(), Err(Error::NoResult)),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb"]),
                Err(Error::NoResult)
            )
        );

        Ok(())
    }
}
