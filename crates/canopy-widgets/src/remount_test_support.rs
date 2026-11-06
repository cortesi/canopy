//! A tree edit that a failing sibling rolls back, for tests of widgets that
//! mount again.

use canopy::{
    Context, ContextExt, NodeId, Widget,
    error::{Error, Result},
};

/// A widget whose mount fails, which fails the tree edit that adds it.
struct MountFails;

impl Widget for MountFails {
    fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        Err(Error::Invalid("the sibling's mount fails".into()))
    }
}

/// Attach the detached `node` under the node of `ctx` in a tree edit that a
/// failing sibling rolls back, so `node` mounts and then unmounts.
pub fn attach_and_roll_back(ctx: &mut dyn Context, node: NodeId) {
    let parent = ctx.node_id();
    let failed = ctx.edit_structure(&mut |ctx| {
        ctx.attach(parent, node)?;
        ctx.add_child(parent, MountFails)?;
        Ok(())
    });
    assert!(failed.is_err(), "the sibling's mount fails the edit");
}

/// Attach the detached `node` as [`attach_and_roll_back`] does, and then
/// attach it again, so it mounts a second time.
pub fn attach_after_rollback(ctx: &mut dyn Context, node: NodeId) -> Result<()> {
    attach_and_roll_back(ctx, node);
    let parent = ctx.node_id();
    ctx.attach(parent, node)
}
