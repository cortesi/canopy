use canopy::{
    Canopy, ChildKey, Context, Core, Loader, NodeId, ReadContext, TypedId, Widget, command,
    commands::{CommandNode, CommandSpec, FocusDirection},
    derive_commands,
    error::{Error, Result},
    layout::{Direction, Layout, Sizing},
    render::Render,
    state::NodeName,
    style::effects,
};

use crate::{help::Help, inspector::Inspector};

/// Default root bindings exposed through `root.default_bindings()`.
const DEFAULT_BINDINGS: &str = r#"
inspector.default_bindings()
help.default_bindings()

canopy.bind_with("ctrl-Right", { path = "root", desc = "Toggle inspector" }, function()
    root.toggle_inspector()
end)
canopy.bind_with("ctrl-/", { path = "root", desc = "Toggle help" }, function()
    root.toggle_help()
end)
canopy.bind_with("q", { path = "root", desc = "Quit" }, function()
    root.quit()
end)
canopy.bind_with("a", { path = "inspector", desc = "Focus app" }, function()
    root.focus_app()
end)
"#;

// Typed key for the inspector slot
canopy::key!(InspectorSlot: Inspector);

// Typed key for the help slot
canopy::key!(HelpSlot: Help);

/// Key for the application subtree under root (widget type varies).
const KEY_APP: &str = "AppSlot";

/// Key for the main pane container (app + inspector).
const KEY_MAIN_PANE: &str = "MainPane";

/// A Root widget that lives at the base of a Canopy app.
pub struct Root {
    /// Whether the inspector is visible.
    inspector_active: bool,
    /// Whether the help modal is visible.
    help_active: bool,
}

#[derive_commands]
impl Root {
    /// Construct a root widget wrapping the application and inspector nodes.
    pub fn new() -> Self {
        Self {
            inspector_active: false,
            help_active: false,
        }
    }

    /// Start with the inspector open.
    pub fn with_inspector(mut self, state: bool) -> Self {
        self.inspector_active = state;
        self
    }

    /// Synchronize the root layout based on inspector and help visibility.
    fn sync_layout(&self, c: &mut dyn Context) -> Result<()> {
        let main_pane = self.main_pane_id(c)?;
        let app = self.app_id(c)?;
        let inspector = self.inspector_id(c)?;
        let help = self.help_id(c)?;

        // Main pane uses Row for app + inspector
        c.set_hidden_of(inspector, !self.inspector_active);
        c.set_layout_of(main_pane, Layout::fill().direction(Direction::Row))?;
        c.set_layout_of(app, Layout::fill())?;
        c.set_layout_of(inspector, Layout::fill())?;

        // Help overlay
        c.set_hidden_of(help, !self.help_active);
        c.set_layout_of(help, Layout::fill())?;

        // Dim effect on main pane when help is visible
        if self.help_active {
            c.push_effect(main_pane, effects::dim(0.5))?;
        } else {
            c.clear_effects(main_pane)?;
        }

        // Root uses Stack layout so help overlays main pane
        c.set_layout(Layout::fill().direction(Direction::Stack))?;

        Ok(())
    }

    /// Main pane (app + inspector container) node id.
    fn main_pane_id(&self, c: &dyn Context) -> Result<NodeId> {
        c.child_keyed(KEY_MAIN_PANE)
            .ok_or_else(|| Error::NotFound("main_pane".into()))
    }

    /// Application node id (inside main pane).
    fn app_id(&self, c: &dyn Context) -> Result<NodeId> {
        let main_pane = self.main_pane_id(c)?;
        c.child_keyed_in(main_pane, KEY_APP)
            .ok_or_else(|| Error::NotFound("app".into()))
    }

    /// Inspector node id (inside main pane).
    fn inspector_id(&self, c: &dyn Context) -> Result<NodeId> {
        let main_pane = self.main_pane_id(c)?;
        c.get_child_in::<InspectorSlot>(main_pane)
            .map(Into::into)
            .ok_or_else(|| Error::NotFound("inspector".into()))
    }

    /// Help node id.
    fn help_id(&self, c: &dyn Context) -> Result<NodeId> {
        c.get_child::<HelpSlot>()
            .map(Into::into)
            .ok_or_else(|| Error::NotFound("help".into()))
    }

    #[command]
    /// Exit from the program, restoring terminal state. If help or inspector is
    /// open, close them first.
    pub fn quit(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.help_active {
            self.hide_help(c)?;
        } else if self.inspector_active {
            self.hide_inspector(c)?;
        } else {
            c.exit(0);
        }
        Ok(())
    }

    #[command]
    /// Dump diagnostic information about the tree, focus, and bindings.
    pub fn dump_diagnostics(&mut self, c: &mut dyn Context) -> Result<()> {
        let target = c.focused_leaf(c.root_id()).unwrap_or_else(|| c.node_id());
        c.request_diagnostic_dump(target);
        Ok(())
    }

    /// Move focus in the specified direction.
    /// @param direction The direction to move focus.
    #[command]
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

    #[command]
    /// Show the help modal with contextual bindings and commands.
    pub fn show_help(&mut self, c: &mut dyn Context) -> Result<()> {
        // Request snapshot BEFORE changing focus, so we capture the pre-help context
        let help = self.help_id(c)?;
        c.request_help_snapshot(help);

        self.help_active = true;
        self.sync_layout(c)?;
        c.focus_first_in(help);
        Ok(())
    }

    #[command]
    /// Hide the help modal.
    pub fn hide_help(&mut self, c: &mut dyn Context) -> Result<()> {
        self.help_active = false;
        self.sync_layout(c)?;
        let app = self.app_id(c)?;
        c.focus_first_in(app);
        Ok(())
    }

    #[command]
    /// Toggle help modal visibility.
    pub fn toggle_help(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.help_active {
            self.hide_help(c)
        } else {
            self.show_help(c)
        }
    }

    /// Helper to install a root widget into a canopy app.
    pub fn install_app<W>(canopy: &mut Canopy, app: W) -> Result<TypedId<W>>
    where
        W: Widget + 'static,
    {
        Self::install_app_with_inspector(canopy, app, false)
    }

    /// Helper to install a root widget into the canopy with an optional inspector pane.
    pub fn install_app_with_inspector<W>(
        canopy: &mut Canopy,
        app: W,
        inspector_active: bool,
    ) -> Result<TypedId<W>>
    where
        W: Widget + 'static,
    {
        let app_id = canopy.create_detached(app);
        Self::install_with_inspector(canopy.core_mut(), app_id, inspector_active)?;
        Ok(app_id)
    }

    /// Helper to install a root widget into the core and configure children.
    pub fn install(core: &mut Core, app: impl Into<NodeId>) -> Result<NodeId> {
        Self::install_with_inspector(core, app, false)
    }

    /// Helper to install a root widget into the core with an optional inspector pane.
    pub fn install_with_inspector(
        core: &mut Core,
        app: impl Into<NodeId>,
        inspector_active: bool,
    ) -> Result<NodeId> {
        let app = app.into();
        // Create main pane container for app + inspector
        let main_pane = core.create_detached(MainPane);
        let inspector = Inspector::install(core)?;

        // Attach app and inspector to main pane
        core.attach_keyed(main_pane, KEY_APP, app)?;
        core.attach_keyed(main_pane, InspectorSlot::KEY, inspector)?;
        core.set_layout_of(main_pane, Layout::fill().direction(Direction::Row))?;

        // Create help modal (hidden by default)
        let help = Help::install(core)?;
        core.set_hidden(help, true);

        // Set up root with main pane and help as children
        let root = Self::new().with_inspector(inspector_active);
        core.replace_subtree(core.root_id(), root)?;
        core.attach_keyed(core.root_id(), KEY_MAIN_PANE, main_pane)?;
        core.attach_keyed(core.root_id(), HelpSlot::KEY, help)?;

        // Configure layout
        core.set_hidden(inspector, !inspector_active);
        core.set_layout_of(core.root_id(), Layout::fill().direction(Direction::Stack))?;
        core.with_layout_of(app, |layout| {
            *layout = layout.width(Sizing::Flex(1)).height(Sizing::Flex(1));
        })?;
        core.with_layout_of(inspector, |layout| {
            *layout = layout.width(Sizing::Flex(1)).height(Sizing::Flex(1));
        })?;
        core.set_layout_of(help, Layout::fill())?;

        Ok(core.root_id())
    }
}

/// Simple container widget for the main pane (app + inspector).
struct MainPane;

impl Widget for MainPane {
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("main_pane")
    }
}

impl CommandNode for MainPane {
    fn commands() -> &'static [&'static CommandSpec] {
        &[]
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

impl Loader for Root {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        c.register_default_bindings("root", DEFAULT_BINDINGS)?;
        Inspector::load(c)?;
        Help::load(c)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        ReadContext, Widget,
        commands::{CommandNode, CommandSpec},
        error::Result,
        geom::Size,
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
        Root::load(&mut canopy)?;

        let app_id = canopy.create_detached(App);
        let left = canopy.create_detached(FocusLeaf::new("left"));
        let right = canopy.create_detached(FocusLeaf::new("right"));
        canopy
            .core_mut()
            .set_children(app_id, vec![left.into(), right.into()])?;

        canopy
            .core_mut()
            .set_layout_of(app_id, Layout::fill().direction(Direction::Row))?;

        canopy.core_mut().set_layout_of(left, Layout::fill())?;
        canopy.core_mut().set_layout_of(right, Layout::fill())?;

        Root::install(canopy.core_mut(), app_id)?;
        canopy.set_root_size(Size::new(20, 6))?;

        let mut backend = NopBackend::new();
        canopy.render(&mut backend)?;

        Ok((canopy, backend, left.into(), right.into()))
    }

    fn run_script(canopy: &mut Canopy, script: &str) -> Result<()> {
        let script_id = canopy.compile_script(script)?;
        canopy.run_script(canopy.root_id(), script_id)?;
        Ok(())
    }

    #[test]
    fn test_root_focus_dir_commands_via_script() -> Result<()> {
        let (mut canopy, mut backend, left, _right) = setup_root_tree()?;

        assert_eq!(canopy.core().focus_id(), Some(left));

        run_script(
            &mut canopy,
            include_str!("../tests/luau/root_focus_dir.luau"),
        )?;
        assert_eq!(canopy.core().focus_id(), Some(left));

        canopy.render(&mut backend)?;
        assert!(canopy.core().focus_id().is_some());

        Ok(())
    }

    #[test]
    fn test_root_focus_next_prev_commands_via_script() -> Result<()> {
        let (mut canopy, mut backend, left, _right) = setup_root_tree()?;

        assert_eq!(canopy.core().focus_id(), Some(left));

        run_script(
            &mut canopy,
            include_str!("../tests/luau/root_focus_order.luau"),
        )?;
        assert_eq!(canopy.core().focus_id(), Some(left));

        canopy.render(&mut backend)?;
        assert!(canopy.core().focus_id().is_some());

        Ok(())
    }
}
