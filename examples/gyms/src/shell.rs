//! Common viewport selector and footer for every gym.

use canopy::{
    Context, ContextExt, NodeName, Register, Setup, TypedId, Widget, derive_commands,
    error::{Error, Result},
    input::{
        BindingAction, BindingOptions, BindingPhase, BindingTier, FrameworkBindingGroup,
        IntentName, ModalBindings, ModalOptions, ModalToken, NavIntent, key::Key,
    },
    layout::{Direction, Layout, LayoutOverride},
    widget::Mount,
};
use canopy_widgets::{Container, Dialog, KeyHint, Root, Selector, StatusBar, Text};

use crate::{HELP_LABEL, flex_row};

/// Keys admitted only while the view selector is open.
const VIEW_BINDINGS: FrameworkBindingGroup = FrameworkBindingGroup::new("gyms.view");
/// The presets cap the whole tree, as a smaller terminal would.
const VIEW_LABELS: [&str; 3] = [
    "Full · terminal size",
    "Narrow · up to 40 columns",
    "Short · up to 12 rows",
];

/// A demo and its footer, constrained together by the chosen view preset.
pub struct DemoShell {
    /// The demo, until its first mount takes it, and the mounted nodes.
    mount: Mount<Option<Box<dyn Widget>>, Parts>,
    /// Name displayed in the footer.
    status: String,
    /// Index of the current preset in the selector.
    view: usize,
    /// Scope that restores focus when the selector closes.
    token: Option<ModalToken>,
}

/// The nodes that the shell builds.
struct Parts {
    /// Demo and footer, dimmed together behind the selector.
    content: TypedId<Container>,
    /// The selector dialog and its body, built when the selector first
    /// opens.
    selector: Option<(TypedId<Dialog>, TypedId<Selector<&'static str>>)>,
}

#[derive_commands]
impl DemoShell {
    /// Wrap any gym in the shared controls.
    pub(crate) fn new(app: impl Widget + 'static) -> Self {
        Self {
            status: app.name().to_string(),
            mount: Mount::new(Some(Box::new(app))),
            view: 0,
            token: None,
        }
    }

    /// Choose the dimensions of the entire gym.
    #[command]
    pub fn choose_view(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.token.is_some_and(|token| c.modal_is_open(token)) {
            return self.close_view(c);
        }
        // An inspector or help panel retains its own modal controls.
        if !c.modal_admits(c.node_id()) {
            return Ok(());
        }
        let parts = self.mount.mounted_mut()?;
        let content = parts.content;
        let (dialog, selector) = match parts.selector {
            Some(nodes) => nodes,
            None => {
                let parent = c.node_id();
                let nodes = Dialog::new().with_title("Gym view").add(
                    c,
                    parent,
                    Selector::new(VIEW_LABELS.to_vec()).with_label("Gym view"),
                )?;
                c.set_hidden(nodes.0.into(), true)?;
                parts.selector = Some(nodes);
                nodes
            }
        };
        c.with_widget_mut(selector, |selector: &mut Selector<&'static str>, _| {
            selector.set_items(VIEW_LABELS.to_vec(), Some(self.view));
            Ok(())
        })?;
        self.token = Some(c.open_modal(ModalOptions {
            owner: c.node_id(),
            modal: dialog.into(),
            initial_focus: selector.into(),
            dim_target: Some(content.into()),
            bindings: ModalBindings::Framework {
                groups: &[VIEW_BINDINGS],
                intents: &[],
            },
        })?);
        Ok(())
    }

    /// Apply the selected preset and restore the gym's previous focus.
    #[command]
    pub fn accept_view(&mut self, c: &mut dyn Context) -> Result<()> {
        let Some((_, selector)) = self.mount.mounted()?.selector else {
            return Ok(());
        };
        self.view = c.with_widget_mut(selector, |selector: &mut Selector<&'static str>, _| {
            Ok(selector.selected_index())
        })?;
        // The cap sits on the root, so help, notices, and modals such as
        // inspectors share the simulated terminal with the gym.
        let root = c.root_id();
        c.set_layout_override(
            root,
            LayoutOverride {
                max_width: Some((self.view == 1).then_some(40)),
                max_height: Some((self.view == 2).then_some(12)),
                ..LayoutOverride::new()
            },
        )?;
        self.close_view(c)
    }

    /// Dismiss the selector without changing the view.
    #[command]
    pub fn close_view(&mut self, c: &mut dyn Context) -> Result<()> {
        if let Some(token) = self.token.take() {
            c.close_modal(token)?;
        }
        Ok(())
    }
}

impl Widget for DemoShell {
    fn name(&self) -> NodeName {
        NodeName::convert("demo_shell")
    }

    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Stack)
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let status = self.status.clone();
        self.mount.mount(|app| {
            let app = app
                .take()
                .ok_or_else(|| Error::Internal("the demo mounted once already".into()))?;
            let content = c.add_child(c.node_id(), Container::column().with_name("gym_content"))?;
            let node = c.create_detached_boxed(app)?;
            c.attach(content.into(), node)?;
            c.set_layout_override(
                node,
                LayoutOverride {
                    min_width: Some(None),
                    max_width: Some(None),
                    ..flex_row(1)
                },
            )?;
            c.add_child(
                content,
                StatusBar::new()
                    .with_left(Text::new(status))
                    .with_right(KeyHint::for_command(Root::call_toggle_help(), HELP_LABEL)),
            )?;
            Ok(Parts {
                content,
                selector: None,
            })
        })
    }

    fn on_unmount(&mut self, _c: &mut dyn Context) {
        self.mount.unmount();
        self.token = None;
    }
}

impl Register for DemoShell {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        setup.bind(
            Key::parse_spec("Ctrl+l")?,
            BindingOptions {
                path: Some("/root/**/".parse()?),
                phase: Some(BindingPhase::BeforeWidget),
                tier: BindingTier::Global,
                description: "Choose gym view".into(),
                source: None,
                show_in_help: true,
            },
            BindingAction::Command(Self::call_choose_view()),
        )?;
        let nav = |intent: NavIntent| IntentName::new(intent.name()).map(BindingAction::Intent);
        let keys = [
            ("Up", "Previous view", nav(NavIntent::Up)?),
            ("k", "Previous view", nav(NavIntent::Up)?),
            ("Down", "Next view", nav(NavIntent::Down)?),
            ("j", "Next view", nav(NavIntent::Down)?),
            ("Home", "First view", nav(NavIntent::First)?),
            ("End", "Last view", nav(NavIntent::Last)?),
            (
                "Enter",
                "Use view",
                BindingAction::Command(Self::call_accept_view()),
            ),
            (
                "Esc",
                "Cancel",
                BindingAction::Command(Self::call_close_view()),
            ),
            (
                "Ctrl+l",
                "Close view selector",
                BindingAction::Command(Self::call_close_view()),
            ),
        ];
        for (key, description, action) in keys {
            setup.bind(
                Key::parse_spec(key)?,
                BindingOptions {
                    path: Some("**/".parse()?),
                    phase: Some(BindingPhase::BeforeWidget),
                    tier: BindingTier::Framework(VIEW_BINDINGS),
                    description: description.into(),
                    source: None,
                    show_in_help: true,
                },
                action,
            )?;
        }
        Ok(())
    }
}
