use crate::{
    Binder, Canopy, Context, DefaultBindings, Loader, NodeId, ViewContext, command,
    core::Core,
    derive_commands,
    error::Result,
    event::key::*,
    geom::Rect,
    layout::{Dimension, Display, FlexDirection, Style},
    state::NodeName,
    widget::Widget,
    widgets::inspector::Inspector,
};

/// A Root widget that lives at the base of a Canopy app.
pub struct Root {
    /// Application root node.
    app: NodeId,
    /// Inspector overlay node.
    inspector: NodeId,
    /// Whether the inspector is visible.
    inspector_active: bool,
}

#[derive_commands]
impl Root {
    /// Construct a root widget wrapping the application and inspector nodes.
    pub fn new(app: NodeId, inspector: NodeId) -> Self {
        Self {
            app,
            inspector,
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
        if self.inspector_active {
            c.set_children(vec![self.inspector, self.app])?;
        } else {
            c.set_children(vec![self.app])?;
        }

        let mut update_root = |style: &mut Style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Row;
        };
        c.with_style(&mut update_root)?;

        let mut update_child = |style: &mut Style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        };
        c.with_style_of(self.app, &mut update_child)?;
        if self.inspector_active {
            c.with_style_of(self.inspector, &mut update_child)?;
        }

        Ok(())
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

    #[command]
    /// Focus the next node in a pre-order traversal of the app.
    pub fn focus_next(&mut self, c: &mut dyn Context) -> Result<()> {
        c.focus_next_global();
        Ok(())
    }

    #[command]
    /// Focus the previous node in a pre-order traversal of the app.
    pub fn focus_prev(&mut self, c: &mut dyn Context) -> Result<()> {
        c.focus_prev_global();
        Ok(())
    }

    #[command]
    /// Shift focus right.
    pub fn focus_right(&mut self, c: &mut dyn Context) -> Result<()> {
        c.focus_right_global();
        Ok(())
    }

    #[command]
    /// Shift focus left.
    pub fn focus_left(&mut self, c: &mut dyn Context) -> Result<()> {
        c.focus_left_global();
        Ok(())
    }

    #[command]
    /// Shift focus up.
    pub fn focus_up(&mut self, c: &mut dyn Context) -> Result<()> {
        c.focus_up_global();
        Ok(())
    }

    #[command]
    /// Shift focus down.
    pub fn focus_down(&mut self, c: &mut dyn Context) -> Result<()> {
        c.focus_down_global();
        Ok(())
    }

    #[command]
    /// Hide the inspector.
    pub fn hide_inspector(&mut self, c: &mut dyn Context) -> Result<()> {
        self.inspector_active = false;
        self.sync_layout(c)?;
        c.focus_first_in(self.app);
        Ok(())
    }

    #[command]
    /// Show the inspector.
    pub fn activate_inspector(&mut self, c: &mut dyn Context) -> Result<()> {
        self.inspector_active = true;
        self.sync_layout(c)?;
        c.focus_first_in(self.inspector);
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
        if c.node_is_on_focus_path(self.inspector) {
            c.focus_first_in(self.app);
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
        let root = Self::new(app, inspector).with_inspector(inspector_active);
        core.set_widget(core.root, root);
        if inspector_active {
            core.set_children(core.root, vec![inspector, app])?;
        } else {
            core.set_children(core.root, vec![app])?;
        }
        core.build(core.root).flex_row().w_full();
        core.build(app).style(|style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        });
        if inspector_active {
            core.build(inspector).style(|style| {
                style.flex_grow = 1.0;
                style.flex_shrink = 1.0;
                style.flex_basis = Dimension::Auto;
            });
        }
        Ok(core.root)
    }
}

impl Widget for Root {
    fn render(
        &mut self,
        _rndr: &mut crate::render::Render,
        _area: Rect,
        _ctx: &dyn ViewContext,
    ) -> Result<()> {
        Ok(())
    }

    fn configure_style(&self, style: &mut Style) {
        style.size.width = Dimension::Percent(1.0);
        style.size.height = Dimension::Percent(1.0);
    }

    fn name(&self) -> NodeName {
        NodeName::convert("root")
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
    use super::*;
    use crate::{
        Context, ViewContext,
        commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
        error::{Error, Result},
        geom::Expanse,
        layout::{Dimension, Display, FlexDirection, Style},
        render::Render,
        state::NodeName,
        testing::render::NopBackend,
        widget::Widget,
    };

    struct App;

    impl CommandNode for App {
        fn commands() -> Vec<CommandSpec> {
            Vec::new()
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn Context,
            cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Err(Error::UnknownCommand(cmd.command.clone()))
        }
    }

    impl Widget for App {
        fn render(
            &mut self,
            _rndr: &mut Render,
            _area: Rect,
            _ctx: &dyn ViewContext,
        ) -> Result<()> {
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
        fn commands() -> Vec<CommandSpec> {
            Vec::new()
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn Context,
            cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Err(Error::UnknownCommand(cmd.command.clone()))
        }
    }

    impl Widget for FocusLeaf {
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }

        fn render(
            &mut self,
            _rndr: &mut Render,
            _area: Rect,
            _ctx: &dyn ViewContext,
        ) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert(self.name)
        }
    }

    fn setup_root_tree() -> Result<(Canopy, NopBackend, NodeId, NodeId)> {
        let mut canopy = Canopy::new();
        Root::load(&mut canopy);

        let app_id = canopy.core.add(App);
        let left = canopy.core.add(FocusLeaf::new("left"));
        let right = canopy.core.add(FocusLeaf::new("right"));
        canopy.core.set_children(app_id, vec![left, right])?;

        canopy.core.build(app_id).style(|style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Row;
        });

        let grow = |style: &mut Style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        };
        canopy.core.build(left).style(grow);
        canopy.core.build(right).style(grow);

        Root::install(&mut canopy.core, app_id)?;
        canopy.set_root_size(Expanse::new(20, 6))?;

        let mut backend = NopBackend::new();
        canopy.render(&mut backend)?;

        Ok((canopy, backend, left, right))
    }

    fn run_script(canopy: &mut Canopy, script: &str) -> Result<()> {
        let script_id = canopy.script_host.compile(script)?;
        canopy.run_script(canopy.core.root, script_id)?;
        Ok(())
    }

    #[test]
    fn test_root_focus_dir_commands_via_script() -> Result<()> {
        let (mut canopy, mut backend, left, right) = setup_root_tree()?;

        assert_eq!(canopy.core.focus, Some(left));

        run_script(&mut canopy, "root::focus_right()")?;
        assert_eq!(canopy.core.focus, Some(right));

        run_script(&mut canopy, "root::focus_left()")?;
        assert_eq!(canopy.core.focus, Some(left));

        run_script(&mut canopy, "root::focus_up()")?;
        run_script(&mut canopy, "root::focus_down()")?;

        canopy.render(&mut backend)?;
        assert!(canopy.core.focus.is_some());

        Ok(())
    }

    #[test]
    fn test_root_focus_next_prev_commands_via_script() -> Result<()> {
        let (mut canopy, mut backend, left, right) = setup_root_tree()?;

        assert_eq!(canopy.core.focus, Some(left));

        run_script(&mut canopy, "root::focus_next()")?;
        assert_eq!(canopy.core.focus, Some(right));

        run_script(&mut canopy, "root::focus_prev()")?;
        assert_eq!(canopy.core.focus, Some(left));

        canopy.render(&mut backend)?;
        assert!(canopy.core.focus.is_some());

        Ok(())
    }
}
