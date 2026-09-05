//! Core mutators schedule publication independently of facade entry points.

use super::Core;
use crate::{
    ChangeOutcome, ChangeSet, Context, Widget, core::context::CoreContext, error::Result,
    layout::LayoutOverride, style::StyleMap,
};

struct Leaf;
impl Widget for Leaf {}

#[test]
fn focus_visibility_and_layout_changes_mark_only_changed_setters() -> Result<()> {
    let mut core = Core::new();
    let leaf = core.create_detached(Leaf)?;
    core.attach(core.root, leaf)?;
    core.changes = ChangeSet::default();
    assert_eq!(core.set_focus(leaf)?, ChangeOutcome::Changed);
    assert!(core.changes.paint);
    core.changes = ChangeSet::default();
    assert_eq!(core.set_focus(leaf)?, ChangeOutcome::Unchanged);
    assert!(!core.changes.is_pending());
    assert_eq!(core.set_hidden(leaf, false)?, ChangeOutcome::Unchanged);
    assert!(!core.changes.is_pending());
    core.set_hidden(leaf, true)?;
    assert!(core.changes.layout);
    core.changes = ChangeSet::default();
    core.set_layout_override_of(leaf, LayoutOverride::new().fixed_height(2))?;
    assert!(core.changes.layout);
    core.changes = ChangeSet::default();
    core.set_layout_override_of(leaf, LayoutOverride::new().fixed_height(2))?;
    assert!(!core.changes.is_pending());
    Ok(())
}

#[test]
fn native_style_and_scroll_changes_mark_publication() -> Result<()> {
    let mut core = Core::new();
    let root = core.root;
    core.nodes[root].content_size = (1, 1).into();
    core.nodes[root].canvas = (10, 10).into();
    core.changes = ChangeSet::default();
    assert!(CoreContext::new(&mut core, root).scroll_to(2, 3));
    assert!(core.changes.layout);
    core.changes = ChangeSet::default();
    assert!(!CoreContext::new(&mut core, root).scroll_to(2, 3));
    assert!(!core.changes.is_pending());
    CoreContext::new(&mut core, root).set_style(StyleMap::default());
    assert!(core.changes.paint);
    Ok(())
}
