use canopy::{
    Canopy, ChildKey, Context, FocusScope, FrameworkBindingGroup, InputSpec, InteractionToken,
    Loader, ModalBindings, ModalOptions, NodeId, TypedId, ViewContext, Widget, command,
    commands::{
        CommandArgs, CommandId, CommandInvocation, CommandNode, CommandSpec, FocusDirection,
    },
    derive_commands,
    error::{Error, Result},
    event::key::Key,
    geom,
    layout::{Direction, Layout, Sizing},
    state::NodeName,
};

use crate::help::Help;
#[cfg(feature = "devtools")]
use crate::inspector::Inspector;

/// Default root bindings exposed through `root.default_bindings()`.
const DEFAULT_BINDINGS: &str = r#"
canopy.bind_command("q", { phase = "after_widget", path = "root", description = "Quit" }, "root::quit")
"#;

/// Additional root bindings installed with developer tools.
#[cfg(feature = "devtools")]
const DEVTOOLS_BINDINGS: &str = r#"
inspector.default_bindings()

canopy.bind_command("ctrl-Right", { phase = "after_widget", path = "root", description = "Toggle inspector" }, "root::toggle_inspector")
canopy.bind_command("a", { phase = "after_widget", path = "inspector", description = "Focus app" }, "root::focus_app")
"#;

/// Framework binding group used while contextual help is open.
const HELP_BINDINGS: FrameworkBindingGroup = FrameworkBindingGroup::new("root.help");

// Typed key for the inspector slot
#[cfg(feature = "devtools")]
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
    #[cfg(feature = "devtools")]
    inspector_active: bool,
    /// Context saved while the help modal is open.
    help_state: HelpState,
}

/// Root-owned contextual help state.
enum HelpState {
    /// Help is not visible.
    Closed,
    /// Last opened scope; Core determines whether it remains active.
    Open {
        /// Retained after deferred close so a failed outer dispatch can retry.
        token: InteractionToken,
    },
}

impl HelpState {
    /// Return true when help is open.
    fn is_open(&self, context: &dyn ViewContext) -> bool {
        matches!(self, Self::Open { token } if context.modal_is_open(*token))
    }
}

#[derive_commands]
impl Root {
    /// Construct a root widget wrapping the application and inspector nodes.
    fn new() -> Self {
        Self {
            #[cfg(feature = "devtools")]
            inspector_active: false,
            help_state: HelpState::Closed,
        }
    }

    /// Start with the inspector open.
    #[cfg(feature = "devtools")]
    fn with_inspector(mut self, state: bool) -> Self {
        self.inspector_active = state;
        self
    }

    /// Synchronize the root layout based on inspector and help visibility.
    fn sync_layout(&self, c: &mut dyn Context) -> Result<()> {
        let app = self.app_id(c)?;
        #[cfg(feature = "devtools")]
        {
            let inspector = self.inspector_id(c)?;
            c.set_hidden_of(inspector, !self.inspector_active)?;
        }
        c.with_layout_of(app, &mut |layout| {
            *layout = layout.width(Sizing::Flex(1)).height(Sizing::Flex(1));
        })?;
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
    #[cfg(feature = "devtools")]
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
        if self.help_state.is_open(c) {
            self.hide_help(c)?;
            return Ok(());
        }
        #[cfg(feature = "devtools")]
        if self.inspector_active {
            self.hide_inspector(c)?;
            return Ok(());
        }
        c.exit(0);
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
    #[cfg(feature = "devtools")]
    pub fn hide_inspector(&mut self, c: &mut dyn Context) -> Result<()> {
        self.inspector_active = false;
        self.sync_layout(c)?;
        let app = self.app_id(c)?;
        c.focus_first(FocusScope::Node(app))?;
        Ok(())
    }

    #[command]
    /// Show the inspector.
    #[cfg(feature = "devtools")]
    pub fn activate_inspector(&mut self, c: &mut dyn Context) -> Result<()> {
        self.inspector_active = true;
        self.sync_layout(c)?;
        let inspector = self.inspector_id(c)?;
        c.focus_first(FocusScope::Node(inspector))?;
        Ok(())
    }

    #[command]
    /// Toggle inspector visibility.
    #[cfg(feature = "devtools")]
    pub fn toggle_inspector(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.inspector_active {
            self.hide_inspector(c)
        } else {
            self.activate_inspector(c)
        }
    }

    #[command]
    /// If we're currently focused in the inspector, shift focus into the app
    /// pane instead.
    #[cfg(feature = "devtools")]
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
        if self.help_state.is_open(c) {
            return Ok(());
        }

        let help = self.help_id(c)?;
        let list = Help::binding_list_id(c, help)?;
        let origin_focus = c.focused_node();
        let snapshot = c.available_bindings(origin_focus)?;
        let (previous_snapshot, previous_scroll) = c.with_widget(list, |list, context| {
            let previous = list.replace_snapshot(Some(snapshot));
            let scroll = context.view().tl;
            context.scroll_to(0, 0);
            Ok((previous, scroll))
        })?;

        let opened = c.open_modal(ModalOptions {
            owner: c.node_id(),
            modal: help,
            initial_focus: list.into(),
            dim_target: Some(self.main_pane_id(c)?),
            bindings: ModalBindings::Framework(HELP_BINDINGS),
        });
        match opened {
            Ok(token) => self.help_state = HelpState::Open { token },
            Err(error) => {
                c.with_widget(list, |list, context| {
                    list.replace_snapshot(previous_snapshot);
                    context.scroll_to(previous_scroll.x, previous_scroll.y);
                    Ok(())
                })?;
                return Err(error);
            }
        }
        Ok(())
    }

    #[command]
    /// Hide the help modal.
    pub fn hide_help(&mut self, c: &mut dyn Context) -> Result<()> {
        let HelpState::Open { token } = self.help_state else {
            return Ok(());
        };
        c.close_modal(token)
    }

    #[command]
    /// Toggle help modal visibility.
    pub fn toggle_help(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.help_state.is_open(c) {
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
        Self::new().install(canopy, app)
    }

    /// Helper to install a root widget into the canopy with an optional
    /// inspector pane.
    #[cfg(feature = "devtools")]
    pub fn install_app_with_inspector<W>(
        canopy: &mut Canopy,
        app: W,
        inspector_active: bool,
    ) -> Result<TypedId<W>>
    where
        W: Widget + 'static,
    {
        Self::new()
            .with_inspector(inspector_active)
            .install(canopy, app)
    }

    /// Install the application, shared help, and enabled developer tools.
    fn install<W>(self, canopy: &mut Canopy, app: W) -> Result<TypedId<W>>
    where
        W: Widget + 'static,
    {
        let app_id = canopy.create_detached(app)?;
        let app_node = NodeId::from(app_id);
        let root_id: NodeId = canopy.replace_root(self)?.into();
        canopy.with_root_context(|context| {
            // Main pane holds the app beside the inspector.
            let main_pane: NodeId = context.create_detached(MainPane)?.into();
            context.attach_keyed(main_pane, KEY_APP, app_node)?;
            #[cfg(feature = "devtools")]
            {
                let inspector = Inspector::install(context)?;
                context.attach_keyed(main_pane, InspectorSlot::KEY, inspector)?;
            }

            // The help modal overlays the main pane.
            let help = Help::install(context)?;
            context.attach_keyed(root_id, KEY_MAIN_PANE, main_pane)?;
            context.attach_keyed(root_id, HelpSlot::KEY, help)?;
            context.set_hidden_of(help, true)?;
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
    fn layout(&self) -> Layout {
        // Stack layout so the help modal overlays the main pane.
        Layout::fill().direction(Direction::Stack)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("root")
    }
}

impl Loader for Root {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        #[cfg(feature = "devtools")]
        {
            c.register_default_bindings(
                "root",
                &format!("{DEFAULT_BINDINGS}\n{DEVTOOLS_BINDINGS}"),
            )?;
            Inspector::load(c)?;
        }
        #[cfg(not(feature = "devtools"))]
        c.register_default_bindings("root", DEFAULT_BINDINGS)?;
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
        canopy.bind_framework_with_options(
            HELP_BINDINGS,
            InputSpec::Key(Key::parse_spec(key).map_err(Error::Invalid)?),
            canopy::BindingOptions {
                path: "/root/help/**/".to_string(),
                scope: canopy::BindingScope::Exclusive(HELP_BINDINGS),
                description: description.to_string(),
                source: None,
                phase: Some(canopy::BindingPhase::BeforeWidget),
            },
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

    #[cfg(feature = "devtools")]
    use canopy::testing::harness::Harness;
    use canopy::{
        BindingScope, Context, EventOutcome, ViewContext, Widget,
        commands::{CommandNode, CommandSpec},
        error::Result,
        event::Event,
        geom::Size,
        help::BindingSnapshot,
        layout::Layout,
        render::NopBackend,
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
            canopy.bind_command("?", { phase = "before_widget",
                description = "Show key bindings",
                path = "/root/**/",
                tier = "global",
            }, "root::toggle_help")
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
        canopy.eval_script(script)
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
    #[cfg(feature = "devtools")]
    fn inspector_pane_draws_its_frame() -> Result<()> {
        let mut canopy = Canopy::new();
        Root::load(&mut canopy)?;
        Root::install_app_with_inspector(&mut canopy, App, true)?;
        canopy.finalize_api()?;

        let mut harness = Harness::from_canopy(canopy, Size::new(40, 8))?;
        harness.render()?;

        let lines = harness.tbuf().lines();
        assert!(
            lines[0].contains('\u{256d}'),
            "inspector frame top corner missing: {lines:?}"
        );
        assert!(
            lines[7].contains('\u{2570}'),
            "inspector frame bottom corner missing: {lines:?}"
        );
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
                canopy.bind(key, { phase = "after_widget", description = "Extra binding" }, function() end)
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
            canopy.bind("x", { phase = "after_widget", description = "Leak sentinel" }, function()
                canopy.set_mode("leaked")
            end)
            local keys: {string} = { "b", "c", "d", "e", "f", "g", "h", "i", "j", "k",
                "l", "m", "n", "o", "p", "r", "s", "t", "u", "v", "w", "y", "z" }
            for _, key: string in keys do
                canopy.bind(key, { phase = "after_widget", description = "Extra binding" }, function() end)
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
    fn failed_outer_dispatch_keeps_help_close_retryable() -> Result<()> {
        let (mut canopy, _backend, left, _right) = setup_root_tree()?;
        canopy.eval_script("root.show_help()")?;
        let before = modal_snapshot(&mut canopy)?;
        let result = canopy.with_root_context(|context| {
            context.dispatch_exact(context.node_id(), &Root::call_hide_help().invocation())?;
            Err::<(), _>(Error::Invalid("outer dispatch failed".into()))
        });
        assert!(result.is_err());
        assert_eq!(
            canopy.available_bindings(None)?.exclusive_group,
            Some(HELP_BINDINGS)
        );
        assert_eq!(
            modal_snapshot(&mut canopy)?.bindings.len(),
            before.bindings.len()
        );
        canopy.eval_script("root.hide_help()")?;
        assert_eq!(canopy.available_bindings(None)?.exclusive_group, None);
        assert_eq!(
            canopy.with_root_view(|context| context.focused_node()),
            Some(left)
        );
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
            .eval_script(r#"canopy.bind("z", { phase = "after_widget", description = "Added later" }, function() end)"#)?;

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
