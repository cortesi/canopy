use std::mem;

use canopy::{
    Canopy, ChildKey, Context, ExclusiveFrameToken, FocusScope, FrameworkBindingGroup, InputSpec,
    Loader, NodeId, TypedId, ViewContext, Widget, command,
    commands::{
        CommandArgs, CommandId, CommandInvocation, CommandNode, CommandSpec, FocusDirection,
    },
    derive_commands,
    error::{Error, Result},
    event::key::Key,
    geom,
    layout::{Direction, Layout, Sizing},
    render::Render,
    state::NodeName,
    style::effects,
};

use crate::{help::Help, inspector::Inspector};

/// Default root bindings exposed through `root.default_bindings()`.
const DEFAULT_BINDINGS: &str = r#"
inspector.default_bindings()

canopy.bind("ctrl-Right", { path = "root", description = "Toggle inspector" }, function()
    root.toggle_inspector()
end)
canopy.bind("q", { path = "root", description = "Quit" }, function()
    root.quit()
end)
canopy.bind("a", { path = "inspector", description = "Focus app" }, function()
    root.focus_app()
end)
"#;

/// Framework binding group used while contextual help is open.
const HELP_BINDINGS: FrameworkBindingGroup = FrameworkBindingGroup::new("root.help");

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
    /// Context saved while the help modal is open.
    help_state: HelpState,
}

/// Root-owned contextual help state.
enum HelpState {
    /// Help is not visible.
    Closed,
    /// Help is visible and owns one exclusive binding frame.
    Open {
        /// Focus node to restore when it remains live.
        origin_focus: Option<NodeId>,
        /// Application or inspector pane used for fallback focus.
        origin_pane: Option<NodeId>,
        /// Exact exclusive frame to remove when help closes.
        exclusive_token: ExclusiveFrameToken,
    },
}

impl HelpState {
    /// Return true when help is open.
    fn is_open(&self) -> bool {
        matches!(self, Self::Open { .. })
    }
}

#[derive_commands]
impl Root {
    /// Construct a root widget wrapping the application and inspector nodes.
    pub fn new() -> Self {
        Self {
            inspector_active: false,
            help_state: HelpState::Closed,
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

        c.set_hidden_of(inspector, !self.inspector_active)?;
        c.with_layout_of(app, &mut |layout| {
            *layout = layout.width(Sizing::Flex(1)).height(Sizing::Flex(1));
        })?;
        c.with_layout_of(inspector, &mut |layout| {
            *layout = layout.width(Sizing::Flex(1)).height(Sizing::Flex(1));
        })?;

        // Help overlay
        c.set_hidden_of(help, !self.help_state.is_open())?;
        c.set_layout_of(help, Layout::fill())?;

        // Dim effect on main pane when help is visible
        if self.help_state.is_open() {
            c.push_effect(main_pane, effects::brightness(0.5))?;
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
        c.get_child_in::<InspectorSlot>(main_pane)?
            .map(Into::into)
            .ok_or_else(|| Error::NotFound("inspector".into()))
    }

    /// Help node id.
    fn help_id(&self, c: &dyn Context) -> Result<NodeId> {
        c.get_child::<HelpSlot>()?
            .map(Into::into)
            .ok_or_else(|| Error::NotFound("help".into()))
    }

    #[command]
    /// Exit from the program, restoring terminal state. If help or inspector is
    /// open, close them first.
    pub fn quit(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.help_state.is_open() {
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
            FocusDirection::Next => c.focus_next(FocusScope::Root),
            FocusDirection::Prev => c.focus_prev(FocusScope::Root),
            FocusDirection::Up => c.focus_dir(FocusScope::Root, geom::Direction::Up),
            FocusDirection::Down => c.focus_dir(FocusScope::Root, geom::Direction::Down),
            FocusDirection::Left => c.focus_dir(FocusScope::Root, geom::Direction::Left),
            FocusDirection::Right => c.focus_dir(FocusScope::Root, geom::Direction::Right),
        }?;
        Ok(())
    }

    #[command]
    /// Hide the inspector.
    pub fn hide_inspector(&mut self, c: &mut dyn Context) -> Result<()> {
        self.inspector_active = false;
        self.sync_layout(c)?;
        let app = self.app_id(c)?;
        c.focus_first(FocusScope::Node(app))?;
        Ok(())
    }

    #[command]
    /// Show the inspector.
    pub fn activate_inspector(&mut self, c: &mut dyn Context) -> Result<()> {
        self.inspector_active = true;
        self.sync_layout(c)?;
        let inspector = self.inspector_id(c)?;
        c.focus_first(FocusScope::Node(inspector))?;
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
            c.focus_first(FocusScope::Node(app))?;
        }
        Ok(())
    }

    #[command]
    /// Show the help modal with contextual bindings and commands.
    pub fn show_help(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.help_state.is_open() {
            return Ok(());
        }

        let help = self.help_id(c)?;
        let list = Help::binding_list_id(c, help)?;
        let origin_focus = c.focused_node();
        let app = self.app_id(c)?;
        let inspector = self.inspector_id(c)?;
        let origin_pane = if c.node_is_on_focus_path(app) {
            Some(app)
        } else if c.node_is_on_focus_path(inspector) {
            Some(inspector)
        } else {
            None
        };
        let snapshot = c.available_bindings(origin_focus)?;
        let (previous_snapshot, previous_scroll) = c.with_widget(list, |list, context| {
            let previous = list.replace_snapshot(Some(snapshot));
            let scroll = context.view().tl;
            context.scroll_to(0, 0);
            Ok((previous, scroll))
        })?;

        let mut token = None;
        let mut prior_capture = None;
        let opened = (|| {
            let exclusive = c.push_exclusive_bindings(HELP_BINDINGS)?;
            token = Some(exclusive);
            prior_capture = c.take_mouse_capture()?;
            self.help_state = HelpState::Open {
                origin_focus,
                origin_pane,
                exclusive_token: exclusive,
            };
            self.sync_layout(c)?;
            c.set_focus(NodeId::from(list))?;
            Ok(())
        })();
        if let Err(error) = opened {
            self.help_state = HelpState::Closed;
            drop(self.sync_layout(c));
            if let Some(token) = token {
                drop(c.pop_exclusive_bindings(token));
            }
            if let Some(capture) = prior_capture {
                drop(c.restore_mouse_capture(capture));
            }
            drop(c.with_widget(list, |list, context| {
                list.replace_snapshot(previous_snapshot);
                context.scroll_to(previous_scroll.x, previous_scroll.y);
                Ok(())
            }));
            if let Some(origin) = origin_focus {
                drop(c.set_focus(origin));
            }
            return Err(error);
        }
        Ok(())
    }

    #[command]
    /// Hide the help modal.
    pub fn hide_help(&mut self, c: &mut dyn Context) -> Result<()> {
        let HelpState::Open {
            origin_focus,
            origin_pane,
            exclusive_token,
        } = mem::replace(&mut self.help_state, HelpState::Closed)
        else {
            return Ok(());
        };
        let help = self.help_id(c)?;
        let list = Help::binding_list_id(c, help)?;
        self.sync_layout(c)?;
        c.with_widget(list, |list, context| {
            list.replace_snapshot(None);
            context.scroll_to(0, 0);
            Ok(())
        })?;
        c.pop_exclusive_bindings(exclusive_token)?;

        let focusable = c.focusable_leaves(c.root_id());
        if let Some(origin) = origin_focus
            && focusable.contains(&origin)
        {
            c.set_focus(origin)?;
            return Ok(());
        }
        if let Some(pane) = origin_pane
            && c.node_is_attached(pane)
            && c.focus_first(FocusScope::Node(pane))?.changed()
        {
            return Ok(());
        }
        let main_pane = self.main_pane_id(c)?;
        c.focus_first(FocusScope::Node(main_pane))?;
        Ok(())
    }

    #[command]
    /// Toggle help modal visibility.
    pub fn toggle_help(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.help_state.is_open() {
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
        let app_id = canopy.create_detached(app)?;
        let app_node = NodeId::from(app_id);
        let root = Self::new().with_inspector(inspector_active);
        let root_id: NodeId = canopy.replace_root(root)?.into();
        canopy.with_root_context(|context| {
            // Main pane holds the app beside the inspector.
            let main_pane: NodeId = context.create_detached(MainPane)?.into();
            let inspector = Inspector::install(context)?;
            context.attach_keyed(main_pane, KEY_APP, app_node)?;
            context.attach_keyed(main_pane, InspectorSlot::KEY, inspector)?;

            // The help modal overlays the main pane.
            let help = Help::install(context)?;
            context.attach_keyed(root_id, KEY_MAIN_PANE, main_pane)?;
            context.attach_keyed(root_id, HelpSlot::KEY, help)?;
            Ok(())
        })?;
        canopy.with_root_context(|context| {
            let root_id = context.node_id();
            context.with_node(root_id, |root: &mut Self, context| {
                root.sync_layout(context)
            })
        })?;
        Ok(app_id)
    }
}

/// Simple container widget for the main pane (app + inspector).
struct MainPane;

impl Widget for MainPane {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Row)
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
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
    fn render(&mut self, _rndr: &mut canopy::render::Render, _ctx: &dyn ViewContext) -> Result<()> {
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
        register_help_bindings(c)?;
        Ok(())
    }
}

/// Register the Root-owned controls admitted by the help exclusive frame.
fn register_help_bindings(canopy: &mut Canopy) -> Result<()> {
    let bindings = [
        ("Up", "Scroll up", "binding_list::scroll_up"),
        ("k", "Scroll up", "binding_list::scroll_up"),
        ("Down", "Scroll down", "binding_list::scroll_down"),
        ("j", "Scroll down", "binding_list::scroll_down"),
        ("PageUp", "Page up", "binding_list::page_up"),
        ("PageDown", "Page down", "binding_list::page_down"),
        ("Space", "Page down", "binding_list::page_down"),
        ("Home", "First binding", "binding_list::scroll_to_top"),
        ("g", "First binding", "binding_list::scroll_to_top"),
        ("End", "Last binding", "binding_list::scroll_to_bottom"),
        ("G", "Last binding", "binding_list::scroll_to_bottom"),
        ("Esc", "Close help", "root::hide_help"),
        ("?", "Close help", "root::toggle_help"),
    ];
    for (key, description, command) in bindings {
        canopy.bind_framework(
            HELP_BINDINGS,
            InputSpec::Key(Key::parse_spec(key).map_err(Error::Invalid)?),
            "/root/help/**/",
            description,
            CommandInvocation {
                id: CommandId(command),
                args: CommandArgs::default(),
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use canopy::{
        BindingScope, Context, EventOutcome, ViewContext, Widget,
        commands::{CommandNode, CommandSpec},
        error::Result,
        event::Event,
        geom::Size,
        help::BindingSnapshot,
        layout::Layout,
        render::{NopBackend, Render},
        state::NodeName,
    };

    use super::*;
    use crate::help::BindingList;

    static APP_EVENTS: AtomicUsize = AtomicUsize::new(0);

    struct App;

    impl CommandNode for App {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for App {
        fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("app")
        }
    }

    struct RowApp;

    impl CommandNode for RowApp {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for RowApp {
        fn layout(&self) -> Layout {
            Layout::row()
        }

        fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
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
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }

        fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }

        fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
            if matches!(event, Event::Key(_) | Event::Mouse(_)) {
                APP_EVENTS.fetch_add(1, Ordering::Relaxed);
            }
            Ok(EventOutcome::Ignore)
        }

        fn name(&self) -> NodeName {
            NodeName::convert(self.name)
        }
    }

    fn setup_root_tree() -> Result<(Canopy, NopBackend, NodeId, NodeId)> {
        let mut canopy = Canopy::new();
        Root::load(&mut canopy)?;

        let app_id = Root::install_app(&mut canopy, App)?;
        let left = canopy.create_detached(FocusLeaf::new("left"))?;
        let right = canopy.create_detached(FocusLeaf::new("right"))?;
        canopy.with_root_context(|context| {
            context.set_children_of(app_id.into(), vec![left.into(), right.into()])?;
            context.set_layout_of(app_id, Layout::fill().direction(Direction::Row))?;
            context.set_layout_of(left, Layout::fill())?;
            context.set_layout_of(right, Layout::fill())
        })?;
        canopy.finalize_api()?;
        canopy.set_root_size(Size::new(60, 14))?;

        let mut backend = NopBackend::new();
        canopy.render(&mut backend)?;

        Ok((canopy, backend, left.into(), right.into()))
    }

    fn install_help_trigger(canopy: &mut Canopy) -> Result<()> {
        canopy.eval_script(
            r#"
            canopy.bind("?", {
                description = "Show key bindings",
                path = "/root/**/",
                tier = "global",
            }, function()
                root.toggle_help()
            end)
            "#,
        )
    }

    fn binding_list_id(canopy: &Canopy) -> NodeId {
        let matches =
            canopy.with_root_view(|context| context.find_nodes("root/help/**/binding_list"));
        assert_eq!(matches.len(), 1);
        matches[0]
    }

    fn modal_snapshot(canopy: &mut Canopy) -> Result<BindingSnapshot> {
        let list = binding_list_id(canopy);
        canopy.with_root_context(|context| {
            context.with_node(list, |list: &mut BindingList, _context| {
                list.snapshot()
                    .cloned()
                    .ok_or_else(|| Error::NotFound("help snapshot".to_string()))
            })
        })
    }

    fn send_key(canopy: &mut Canopy, key: &str) -> Result<()> {
        canopy.eval_script(&format!("canopy.send_key({key:?})"))
    }

    fn run_script(canopy: &mut Canopy, script: &str) -> Result<()> {
        let script_id = canopy.compile_script(script)?;
        canopy.run_script(canopy.root_id(), script_id)?;
        Ok(())
    }

    #[test]
    fn install_app_preserves_app_layout_direction() -> Result<()> {
        let mut canopy = Canopy::new();
        Root::load(&mut canopy)?;

        let app = Root::install_app(&mut canopy, RowApp)?;
        let layout = canopy.with_root_view(|context| context.node_layout(app.into()));

        assert_eq!(layout.map(|layout| layout.direction), Some(Direction::Row));
        Ok(())
    }

    #[test]
    fn test_root_focus_dir_commands_via_script() -> Result<()> {
        let (mut canopy, mut backend, left, _right) = setup_root_tree()?;

        assert_eq!(
            canopy.with_root_view(|context| context.focused_leaf(context.root_id())),
            Some(left)
        );

        run_script(
            &mut canopy,
            include_str!("../tests/luau/root_focus_dir.luau"),
        )?;
        assert_eq!(
            canopy.with_root_view(|context| context.focused_leaf(context.root_id())),
            Some(left)
        );

        canopy.render(&mut backend)?;
        assert!(
            canopy
                .with_root_view(|context| context.focused_leaf(context.root_id()))
                .is_some()
        );

        Ok(())
    }

    #[test]
    fn test_root_focus_next_prev_commands_via_script() -> Result<()> {
        let (mut canopy, mut backend, left, _right) = setup_root_tree()?;

        assert_eq!(
            canopy.with_root_view(|context| context.focused_leaf(context.root_id())),
            Some(left)
        );

        run_script(
            &mut canopy,
            include_str!("../tests/luau/root_focus_order.luau"),
        )?;
        assert_eq!(
            canopy.with_root_view(|context| context.focused_leaf(context.root_id())),
            Some(left)
        );

        canopy.render(&mut backend)?;
        assert!(
            canopy
                .with_root_view(|context| context.focused_leaf(context.root_id()))
                .is_some()
        );

        Ok(())
    }

    #[test]
    fn help_opens_synchronously_from_a_command_and_restores_exact_focus() -> Result<()> {
        let (mut canopy, mut backend, left, _right) = setup_root_tree()?;
        install_help_trigger(&mut canopy)?;
        let before = canopy.available_bindings(Some(left))?;

        canopy.eval_script("root.show_help()")?;

        let list = binding_list_id(&canopy);
        assert_eq!(
            canopy.with_root_view(|context| context.focused_node()),
            Some(list)
        );
        let live = canopy.available_bindings(None)?;
        assert_eq!(live.exclusive_group, Some(HELP_BINDINGS));
        assert!(canopy.available_bindings(Some(left))?.bindings.is_empty());
        let installed = modal_snapshot(&mut canopy)?;
        assert_eq!(
            installed
                .bindings
                .iter()
                .map(|binding| binding.id)
                .collect::<Vec<_>>(),
            before
                .bindings
                .iter()
                .map(|binding| binding.id)
                .collect::<Vec<_>>()
        );

        send_key(&mut canopy, "?")?;
        canopy.render(&mut backend)?;
        assert_eq!(
            canopy.with_root_view(|context| context.focused_leaf(context.root_id())),
            Some(left)
        );
        assert_eq!(canopy.available_bindings(None)?.exclusive_group, None);
        Ok(())
    }

    #[test]
    fn repeated_show_keeps_the_owned_snapshot_and_scroll_position() -> Result<()> {
        let (mut canopy, mut backend, _left, _right) = setup_root_tree()?;
        install_help_trigger(&mut canopy)?;
        canopy.eval_script(
            r#"
            local keys: {string} = { "b", "c", "d", "e", "f", "g", "h", "i", "j", "k",
                "l", "m", "n", "o", "p", "r", "s", "t", "u", "v", "w", "x", "y", "z" }
            for _, key: string in keys do
                canopy.bind(key, { description = "Extra binding" }, function() end)
            end
            "#,
        )?;
        send_key(&mut canopy, "?")?;
        canopy.render(&mut backend)?;
        let list = binding_list_id(&canopy);
        let before = modal_snapshot(&mut canopy)?;
        send_key(&mut canopy, "Down")?;
        canopy.render(&mut backend)?;
        let scroll =
            canopy.with_root_view(|context| context.node_view(list).expect("binding-list view").tl);

        canopy.eval_script("root.show_help()")?;

        let after = modal_snapshot(&mut canopy)?;
        assert_eq!(
            after
                .bindings
                .iter()
                .map(|binding| binding.id)
                .collect::<Vec<_>>(),
            before
                .bindings
                .iter()
                .map(|binding| binding.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            canopy.with_root_view(|context| context.node_view(list).expect("binding-list view").tl),
            scroll
        );
        send_key(&mut canopy, "?")?;
        Ok(())
    }

    #[test]
    fn help_isolates_application_bindings_widgets_and_mouse_capture() -> Result<()> {
        APP_EVENTS.store(0, Ordering::Relaxed);
        let (mut canopy, mut backend, left, _right) = setup_root_tree()?;
        install_help_trigger(&mut canopy)?;
        canopy.eval_script(
            r#"
            canopy.bind("x", { description = "Leak sentinel" }, function()
                canopy.set_mode("leaked")
            end)
            local keys: {string} = { "b", "c", "d", "e", "f", "g", "h", "i", "j", "k",
                "l", "m", "n", "o", "p", "r", "s", "t", "u", "v", "w", "y", "z" }
            for _, key: string in keys do
                canopy.bind(key, { description = "Extra binding" }, function() end)
            end
            "#,
        )?;
        canopy.with_context(left, |context| {
            context.capture_mouse()?;
            Ok(())
        })?;

        send_key(&mut canopy, "?")?;
        canopy.render(&mut backend)?;
        let capture = canopy.with_root_context(|context| context.take_mouse_capture())?;
        assert_eq!(capture, None);
        send_key(&mut canopy, "x")?;
        let list = binding_list_id(&canopy);
        let view =
            canopy.with_root_view(|context| context.node_view(list).expect("binding-list view"));
        let x = view.content.tl.x + i32::try_from(view.content.w.saturating_sub(1)).unwrap();
        let y = view.content.tl.y + i32::try_from(view.content.h.saturating_sub(1)).unwrap();
        canopy.eval_script(&format!(
            "canopy.send_scroll(\"Down\", {x}, {y}); canopy.send_click({x}, {y})"
        ))?;
        canopy.eval_script("canopy.send_click(1, 1)")?;
        canopy.render(&mut backend)?;
        assert_eq!(canopy.input_mode(), "");
        assert_eq!(APP_EVENTS.load(Ordering::Relaxed), 0);

        send_key(&mut canopy, "?")?;
        send_key(&mut canopy, "x")?;
        assert_eq!(canopy.input_mode(), "leaked");
        assert_eq!(APP_EVENTS.load(Ordering::Relaxed), 1);
        Ok(())
    }

    #[test]
    fn stale_origin_falls_back_inside_the_saved_pane() -> Result<()> {
        let (mut canopy, _backend, left, right) = setup_root_tree()?;
        install_help_trigger(&mut canopy)?;
        send_key(&mut canopy, "?")?;
        canopy.with_root_context(|context| context.remove_subtree(left))?;

        send_key(&mut canopy, "?")?;

        assert_eq!(
            canopy.with_root_view(|context| context.focused_leaf(context.root_id())),
            Some(right)
        );
        Ok(())
    }

    #[test]
    fn failed_open_preserves_focus_capture_and_token_balance() -> Result<()> {
        let (mut canopy, _backend, left, _right) = setup_root_tree()?;
        let help = canopy
            .with_root_view(|context| context.find_nodes("root/help"))
            .into_iter()
            .next()
            .expect("help node");
        canopy.with_context(left, |context| {
            context.capture_mouse()?;
            Ok(())
        })?;
        canopy.with_root_context(|context| context.remove_subtree(help))?;

        assert!(canopy.eval_script("root.show_help()").is_err());

        assert_eq!(
            canopy.with_root_view(|context| context.focused_leaf(context.root_id())),
            Some(left)
        );
        assert_eq!(canopy.available_bindings(None)?.exclusive_group, None);
        let capture = canopy.with_root_context(|context| context.take_mouse_capture())?;
        assert_eq!(capture, Some(left));
        Ok(())
    }

    #[test]
    fn replacing_an_open_root_retires_its_exclusive_frame() -> Result<()> {
        let (mut canopy, _backend, _left, _right) = setup_root_tree()?;
        install_help_trigger(&mut canopy)?;
        send_key(&mut canopy, "?")?;
        assert_eq!(
            canopy.available_bindings(None)?.exclusive_group,
            Some(HELP_BINDINGS)
        );

        canopy.replace_root(App)?;

        assert_eq!(canopy.available_bindings(None)?.exclusive_group, None);
        Ok(())
    }

    #[test]
    fn reopening_captures_new_application_bindings() -> Result<()> {
        let (mut canopy, _backend, _left, _right) = setup_root_tree()?;
        install_help_trigger(&mut canopy)?;
        send_key(&mut canopy, "?")?;
        let first = modal_snapshot(&mut canopy)?;
        send_key(&mut canopy, "?")?;
        canopy
            .eval_script(r#"canopy.bind("z", { description = "Added later" }, function() end)"#)?;

        send_key(&mut canopy, "?")?;
        let second = modal_snapshot(&mut canopy)?;

        assert!(!first.bindings.iter().any(|binding| binding.key == 'z'));
        assert!(second.bindings.iter().any(|binding| {
            binding.key == 'z'
                && binding.scope == BindingScope::Default
                && binding.description == "Added later"
        }));
        send_key(&mut canopy, "?")?;
        Ok(())
    }
}
