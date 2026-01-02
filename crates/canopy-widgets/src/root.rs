use canopy::{
    Binder, Canopy, ChildKey, Context, Core, DefaultBindings, Loader, NodeId, ReadContext, Widget,
    command,
    commands::FocusDirection,
    derive_commands,
    error::{Error, Result},
    event::key::*,
    key,
    layout::Layout,
    state::NodeName,
};

use crate::inspector::Inspector;

// Typed key for the inspector slot
key!(InspectorSlot: Inspector);

/// Key for the application subtree under root (widget type varies).
const KEY_APP: &str = "AppSlot";

/// A Root widget that lives at the base of a Canopy app.
pub struct Root {
    /// Whether the inspector is visible.
    inspector_active: bool,
}

#[derive_commands]
impl Root {
    /// Construct a root widget wrapping the application and inspector nodes.
    pub fn new() -> Self {
        Self {
            inspector_active: false,
        }
    }

    /// Start with the inspector open.
    pub fn with_inspector(mut self, state: bool) -> Self {
        self.inspector_active = state;
        self
    }

    /// Synchronize the root layout based on inspector visibility.
    fn sync_layout(&self, c: &mut dyn Context) -> Result<()> {
        let app = self.app_id(c)?;
        let inspector = self.inspector_id(c)?;

        c.set_hidden_of(inspector, !self.inspector_active);

        c.set_layout(Layout::row().flex_horizontal(1).flex_vertical(1))?;

        c.set_layout_of(app, Layout::fill())?;
        c.set_layout_of(inspector, Layout::fill())?;

        Ok(())
    }

    /// Application node id.
    fn app_id(&self, c: &dyn Context) -> Result<NodeId> {
        c.child_keyed(KEY_APP)
            .ok_or_else(|| Error::NotFound("app".into()))
    }

    /// Inspector node id.
    fn inspector_id(&self, c: &dyn Context) -> Result<NodeId> {
        c.get_child::<InspectorSlot>()
            .ok_or_else(|| Error::NotFound("inspector".into()))
    }

    #[command]
    /// Exit from the program, restoring terminal state. If the inspector is
    /// open, exit the inspector instead.
    pub fn quit(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.inspector_active {
            self.hide_inspector(c)?;
        } else {
            c.exit(0)
        }
        Ok(())
    }

    /// Move focus in the specified direction.
    pub fn focus(&mut self, c: &mut dyn Context, direction: FocusDirection) -> Result<()> {
        match direction {
            FocusDirection::Next => c.focus_next_global(),
            FocusDirection::Prev => c.focus_prev_global(),
            FocusDirection::Up => c.focus_up_global(),
            FocusDirection::Down => c.focus_down_global(),
            FocusDirection::Left => c.focus_left_global(),
            FocusDirection::Right => c.focus_right_global(),
        }
        Ok(())
    }

    #[command]
    /// Move focus to the next node.
    pub fn focus_next(&mut self, c: &mut dyn Context) -> Result<()> {
        self.focus(c, FocusDirection::Next)
    }

    #[command]
    /// Move focus to the previous node.
    pub fn focus_prev(&mut self, c: &mut dyn Context) -> Result<()> {
        self.focus(c, FocusDirection::Prev)
    }

    #[command]
    /// Move focus up.
    pub fn focus_up(&mut self, c: &mut dyn Context) -> Result<()> {
        self.focus(c, FocusDirection::Up)
    }

    #[command]
    /// Move focus down.
    pub fn focus_down(&mut self, c: &mut dyn Context) -> Result<()> {
        self.focus(c, FocusDirection::Down)
    }

    #[command]
    /// Move focus left.
    pub fn focus_left(&mut self, c: &mut dyn Context) -> Result<()> {
        self.focus(c, FocusDirection::Left)
    }

    #[command]
    /// Move focus right.
    pub fn focus_right(&mut self, c: &mut dyn Context) -> Result<()> {
        self.focus(c, FocusDirection::Right)
    }

    #[command]
    /// Hide the inspector.
    pub fn hide_inspector(&mut self, c: &mut dyn Context) -> Result<()> {
        self.inspector_active = false;
        self.sync_layout(c)?;
        let app = self.app_id(c)?;
        c.focus_first_in(app);
        Ok(())
    }

    #[command]
    /// Show the inspector.
    pub fn activate_inspector(&mut self, c: &mut dyn Context) -> Result<()> {
        self.inspector_active = true;
        self.sync_layout(c)?;
        let inspector = self.inspector_id(c)?;
        c.focus_first_in(inspector);
        Ok(())
    }

    #[command]
    /// Toggle inspector visibility.
    pub fn toggle_inspector(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.inspector_active {
            self.hide_inspector(c)
        } else {
            self.activate_inspector(c)
        }
    }

    #[command]
    /// If we're currently focused in the inspector, shift focus into the app pane instead.
    pub fn focus_app(&mut self, c: &mut dyn Context) -> Result<()> {
        let inspector = self.inspector_id(c)?;
        let app = self.app_id(c)?;
        if c.node_is_on_focus_path(inspector) {
            c.focus_first_in(app);
        }
        Ok(())
    }

    /// Helper to install a root widget into the core and configure children.
    pub fn install(core: &mut Core, app: NodeId) -> Result<NodeId> {
        Self::install_with_inspector(core, app, false)
    }

    /// Helper to install a root widget into the core with an optional inspector pane.
    pub fn install_with_inspector(
        core: &mut Core,
        app: NodeId,
        inspector_active: bool,
    ) -> Result<NodeId> {
        let inspector = Inspector::install(core)?;
        let root = Self::new().with_inspector(inspector_active);
        core.set_widget(core.root_id(), root);
        core.attach_keyed(core.root_id(), InspectorSlot::KEY, inspector)?;
        core.attach_keyed(core.root_id(), KEY_APP, app)?;
        core.set_hidden(inspector, !inspector_active);
        core.with_layout_of(core.root_id(), |layout| {
            *layout = Layout::row().flex_horizontal(1).flex_vertical(1);
        })?;
        core.with_layout_of(app, |layout| {
            *layout = (*layout).flex_horizontal(1).flex_vertical(1);
        })?;
        core.with_layout_of(inspector, |layout| {
            *layout = (*layout).flex_horizontal(1).flex_vertical(1);
        })?;
        Ok(core.root_id())
    }
}

impl Widget for Root {
    fn render(&mut self, _rndr: &mut canopy::render::Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn name(&self) -> NodeName {
        NodeName::convert("root")
    }
}

impl Default for Root {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultBindings for Root {
    fn defaults(b: Binder) -> Binder {
        b.defaults::<Inspector>()
            .with_path("root")
            .key(Ctrl + KeyCode::Right, "root::toggle_inspector()")
            .key('q', "root::quit()")
            .with_path("inspector")
            .key('a', "root::focus_app()")
    }
}

impl Loader for Root {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        Inspector::load(c);
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        ReadContext, Widget,
        commands::{CommandNode, CommandSpec},
        error::Result,
        geom::Expanse,
        layout::Layout,
        render::Render,
        state::NodeName,
        testing::render::NopBackend,
    };

    use super::*;

    struct App;

    impl CommandNode for App {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for App {
        fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("app")
        }
    }

    struct FocusLeaf {
        name: &'static str,
    }

    impl FocusLeaf {
        fn new(name: &'static str) -> Self {
            Self { name }
        }
    }

    impl CommandNode for FocusLeaf {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for FocusLeaf {
        fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
            true
        }

        fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert(self.name)
        }
    }

    fn setup_root_tree() -> Result<(Canopy, NopBackend, NodeId, NodeId)> {
        let mut canopy = Canopy::new();
        Root::load(&mut canopy);

        let app_id = canopy.core.create_detached(App);
        let left = canopy.core.create_detached(FocusLeaf::new("left"));
        let right = canopy.core.create_detached(FocusLeaf::new("right"));
        canopy.core.set_children(app_id, vec![left, right])?;

        canopy
            .core
            .set_layout_of(app_id, Layout::row().flex_horizontal(1).flex_vertical(1))?;

        canopy.core.set_layout_of(left, Layout::fill())?;
        canopy.core.set_layout_of(right, Layout::fill())?;

        Root::install(&mut canopy.core, app_id)?;
        canopy.set_root_size(Expanse::new(20, 6))?;

        let mut backend = NopBackend::new();
        canopy.render(&mut backend)?;

        Ok((canopy, backend, left, right))
    }

    fn run_script(canopy: &mut Canopy, script: &str) -> Result<()> {
        let script_id = canopy.compile_script(script)?;
        canopy.run_script(canopy.core.root_id(), script_id)?;
        Ok(())
    }

    #[test]
    fn test_root_focus_dir_commands_via_script() -> Result<()> {
        let (mut canopy, mut backend, left, right) = setup_root_tree()?;

        assert_eq!(canopy.core.focus_id(), Some(left));

        run_script(&mut canopy, "root::focus_right()")?;
        assert_eq!(canopy.core.focus_id(), Some(right));

        run_script(&mut canopy, "root::focus_left()")?;
        assert_eq!(canopy.core.focus_id(), Some(left));

        run_script(&mut canopy, "root::focus_up()")?;
        run_script(&mut canopy, "root::focus_down()")?;

        canopy.render(&mut backend)?;
        assert!(canopy.core.focus_id().is_some());

        Ok(())
    }

    #[test]
    fn test_root_focus_next_prev_commands_via_script() -> Result<()> {
        let (mut canopy, mut backend, left, right) = setup_root_tree()?;

        assert_eq!(canopy.core.focus_id(), Some(left));

        run_script(&mut canopy, "root::focus_next()")?;
        assert_eq!(canopy.core.focus_id(), Some(right));

        run_script(&mut canopy, "root::focus_prev()")?;
        assert_eq!(canopy.core.focus_id(), Some(left));

        canopy.render(&mut backend)?;
        assert!(canopy.core.focus_id().is_some());

        Ok(())
    }
}
