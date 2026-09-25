//! Contextual binding discovery.

use crate::{
    NodeId,
    commands::{CommandAvailability, CommandCall, CommandResolver},
    core::Core,
    error::Result,
    input::{
        BindingAction, BindingActionKind, BindingId, BindingPhase, BindingTier,
        FrameworkBindingGroup, InputSpec, IntentName, key::Key, mouse::Mouse,
    },
    path::Path,
};

/// Owned snapshot of the effective bindings for one focus context.
///
/// The snapshot answers what one context would do with an input, not what the
/// next event will do. A route is only hypothetical here: hit testing and mouse
/// capture pick the real mouse target. Key discovery asks each widget along the
/// route through [`crate::Widget::key_outcome`], so a key a widget consumes
/// hides the after-widget bindings it would shadow.
#[derive(Clone, Debug)]
pub struct BindingSnapshot {
    /// Node used as the discovery focus.
    pub focus: NodeId,
    /// Path from the root to the focus.
    pub focus_path: Path,
    /// Active non-default modes in resolution order.
    pub active_modes: Vec<String>,
    /// Transient mode that takes the next key.
    ///
    /// This is the newest active mode when it is transient. It is absent
    /// while a framework-group modal suspends transient modes.
    pub transient_mode: Option<String>,
    /// Framework group the open modal admits.
    pub framework_group: Option<FrameworkBindingGroup>,
    /// Effective key bindings, each with a route to the node where it acts.
    ///
    /// An intent binding appears only when a widget on the route accepts it.
    pub bindings: Vec<AvailableBinding<Key>>,
    /// Effective mouse bindings, with one winner per normalized mouse input.
    ///
    /// The route starts at the requested node, as a click on it would. The
    /// pointer's own position plays no part.
    pub mouse_bindings: Vec<AvailableBinding<Mouse>>,
}

/// One effective binding in a contextual snapshot.
///
/// `I` is the input kind: [`Key`] for a key binding and [`Mouse`] for a mouse
/// binding. Every other field means the same thing for both.
#[derive(Clone, Debug)]
pub struct AvailableBinding<I> {
    /// Stable binding identifier.
    pub id: BindingId,
    /// Normalized input.
    pub input: I,
    /// Required user-facing description.
    pub description: String,
    /// Resolution tier.
    pub tier: BindingTier,
    /// Original path filter.
    pub path_filter: String,
    /// Route path at which this binding wins.
    pub route_path: Path,
    /// Kind of action this binding runs.
    pub action: BindingActionKind,
    /// Intent name, present when the action is an intent.
    pub intent: Option<IntentName>,
    /// Phase relative to widget input handling.
    pub phase: BindingPhase,
    /// Declarative command details, absent for opaque script callbacks and
    /// intents.
    pub command: Option<BindingCommand>,
    /// Optional diagnostic source.
    pub source: Option<String>,
}

/// Owned command details captured with an effective key binding.
#[derive(Clone, Debug)]
pub struct BindingCommand {
    /// Stored command call, with its arguments and target policy.
    pub call: CommandCall,
    /// Availability at capture time, absent when the command is not
    /// registered.
    pub availability: Option<CommandAvailability>,
}

/// The action a key hint names: a command call or a registered intent.
#[derive(Clone, Debug, PartialEq)]
pub enum BindingTarget {
    /// A command call, matched by command and arguments.
    Command(CommandCall),
    /// A registered intent, matched by name.
    Intent(IntentName),
}

impl BindingTarget {
    /// Return whether a binding running `action` reaches this target.
    fn matches(&self, action: &BindingAction) -> bool {
        match (self, action) {
            (Self::Command(call), BindingAction::Command(bound)) => {
                call.id == bound.id && call.args == bound.args
            }
            (Self::Intent(name), _) => action.intent() == Some(name),
            _ => false,
        }
    }
}

impl From<CommandCall> for BindingTarget {
    fn from(call: CommandCall) -> Self {
        Self::Command(call)
    }
}

impl From<IntentName> for BindingTarget {
    fn from(name: IntentName) -> Self {
        Self::Intent(name)
    }
}

impl Core {
    /// Return a key that reaches `target` from the current focus.
    ///
    /// Discovery explains each candidate key as [`Core::available_bindings`]
    /// does, so a key another binding or a widget shadows is not offered.
    pub(crate) fn key_for(&self, target: &BindingTarget) -> Result<Option<Key>> {
        for key in self.input_map.candidate_keys() {
            let explanation = self.explain_key(None, key)?;
            let Some(winner) = explanation.outcome.winner() else {
                continue;
            };
            if self
                .input_map
                .binding(winner.binding)
                .is_some_and(|record| target.matches(&record.action))
            {
                return Ok(Some(key));
            }
        }
        Ok(None)
    }

    /// Return the effective bindings for a node or the current focus.
    ///
    /// Key discovery explains each candidate key, so it cannot disagree with
    /// [`Core::explain_key`] about which binding a key reaches.
    pub(crate) fn available_bindings(&self, requested: Option<NodeId>) -> Result<BindingSnapshot> {
        let focus = requested.or(self.focus).unwrap_or(self.root);
        self.validate_attached_node(focus)?;
        let focus_path = self.path_of(self.root, focus);

        let mut bindings = Vec::new();
        for key in self.input_map.candidate_keys() {
            let explanation = self.explain_key(Some(focus), key)?;
            if let Some(winner) = explanation.outcome.winner() {
                bindings.push(self.available_binding(
                    winner.node,
                    key,
                    winner.path.clone(),
                    winner.binding,
                )?);
            }
        }
        let mut mouse_bindings = Vec::new();
        for mouse in self.input_map.eligible_mouse_inputs() {
            mouse_bindings.extend(self.winner_along_route(
                focus,
                mouse,
                InputSpec::Mouse(mouse),
            )?);
        }

        Ok(BindingSnapshot {
            focus,
            focus_path,
            active_modes: self
                .input_map
                .active_modes()
                .into_iter()
                .map(str::to_string)
                .collect(),
            transient_mode: self.effective_transient_mode().map(str::to_string),
            framework_group: self.input_map.active_framework_group(),
            bindings,
            mouse_bindings,
        })
    }

    /// Return the binding that wins `spec` along the route from `focus`.
    ///
    /// Mouse discovery has no widget prediction, so this walk selects at each
    /// route node as mouse routing does. Key discovery uses
    /// [`Core::explain_key`].
    fn winner_along_route<I>(
        &self,
        focus: NodeId,
        input: I,
        spec: InputSpec,
    ) -> Result<Option<AvailableBinding<I>>> {
        let winner = self.route(focus).find_map(|(node, path)| {
            self.select_binding(node, &path, spec, focus, &[])
                .map(|record| (node, path, record.id))
        });
        winner
            .map(|(node, path, id)| self.available_binding(node, input, path, id))
            .transpose()
    }

    /// Build one effective binding record from a resolved winner.
    fn available_binding<I>(
        &self,
        node: NodeId,
        input: I,
        route_path: Path,
        id: BindingId,
    ) -> Result<AvailableBinding<I>> {
        let record = self
            .input_map
            .binding(id)
            .expect("resolved binding record must remain registered");
        let command = match &record.action {
            BindingAction::Script(_) | BindingAction::Intent(_) | BindingAction::Menu(_) => None,
            BindingAction::Command(call) => {
                let availability = self
                    .commands
                    .get(call.id.0)
                    .map(|spec| {
                        CommandResolver::for_target(self, call.target_or(node))
                            .availability_for(spec)
                    })
                    .transpose()?;
                Some(BindingCommand {
                    call: call.clone(),
                    availability,
                })
            }
        };
        Ok(AvailableBinding {
            id: record.id,
            input,
            description: record.description.clone(),
            tier: record.tier.clone(),
            path_filter: record.path_filter().to_string(),
            route_path,
            action: BindingActionKind::of(&record.action),
            intent: record.action.intent().cloned(),
            phase: record.phase,
            command,
            source: record.source.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;
    use crate::{
        NodeName, ViewContext,
        commands::{
            CommandArgs, CommandCall, CommandId, CommandNode, CommandResolution, CommandStatus,
            CommandTarget,
        },
        error::Error,
        input::{BindingAction, BindingOptions, InputSpec, ModalBindings, key::KeyCode},
        script::LuauFunctionId,
        widget::{EventOutcome, Widget},
    };

    struct Leaf;

    impl Widget for Leaf {
        fn name(&self) -> NodeName {
            NodeName::convert("leaf")
        }
    }

    /// A leaf that consumes one key and ignores every other, as a terminal or
    /// text field would.
    struct CapturingLeaf;

    impl Widget for CapturingLeaf {
        fn key_outcome(&self, key: Key, _context: &dyn ViewContext) -> EventOutcome {
            if key.key == KeyCode::Char('x') {
                EventOutcome::Handle
            } else {
                EventOutcome::Ignore
            }
        }

        fn name(&self) -> NodeName {
            NodeName::convert("capturing_leaf")
        }
    }

    fn bind(
        core: &mut Core,
        tier: BindingTier,
        key: char,
        path: &str,
        description: &str,
        target: u64,
    ) -> Result<()> {
        bind_phase(
            core,
            tier,
            key,
            path,
            description,
            target,
            BindingPhase::AfterWidget,
        )
    }

    /// Bind one key with an explicit phase.
    fn bind_phase(
        core: &mut Core,
        tier: BindingTier,
        key: char,
        path: &str,
        description: &str,
        target: u64,
        phase: BindingPhase,
    ) -> Result<()> {
        core.input_map.bind(
            InputSpec::Key(key.into()),
            BindingOptions {
                tier,
                path: Some(path.parse()?),
                description: description.into(),
                source: Some("test".to_string()),
                phase: Some(phase),
            },
            BindingAction::Script(LuauFunctionId::for_test(target)),
        )?;
        Ok(())
    }

    /// Bind one mouse spec, returning its normalized input.
    fn bind_mouse(
        core: &mut Core,
        tier: BindingTier,
        spec: &str,
        path: &str,
        description: &str,
        target: u64,
    ) -> Result<Mouse> {
        let mouse = Mouse::parse_spec(spec)?;
        core.input_map.bind(
            InputSpec::Mouse(mouse),
            BindingOptions {
                tier,
                path: Some(path.parse()?),
                description: description.into(),
                source: Some("test".to_string()),
                phase: Some(BindingPhase::AfterWidget),
            },
            BindingAction::Script(LuauFunctionId::for_test(target)),
        )?;
        Ok(mouse)
    }

    #[test]
    fn mouse_availability_reports_one_winner_per_input_in_a_stable_order() -> Result<()> {
        let mut core = Core::new();
        let leaf = core.create_detached(Leaf)?;
        core.attach(core.root, leaf)?;
        core.set_focus(leaf)?;

        // Two records for one input: the route reports the winner alone.
        bind_mouse(
            &mut core,
            BindingTier::Default,
            "LeftDown",
            "",
            "Anywhere",
            1,
        )?;
        bind_mouse(
            &mut core,
            BindingTier::Default,
            "LeftDown",
            "leaf/",
            "On the leaf",
            2,
        )?;
        bind_mouse(
            &mut core,
            BindingTier::Default,
            "ScrollUp",
            "leaf/",
            "Scroll up",
            3,
        )?;
        bind_mouse(
            &mut core,
            BindingTier::Mode("insert".to_string()),
            "ctrl-RightDown",
            "leaf/",
            "Only in insert",
            4,
        )?;
        bind(&mut core, BindingTier::Default, 'a', "leaf/", "A key", 5)?;

        let snapshot = core.available_bindings(None)?;
        assert_eq!(
            snapshot
                .mouse_bindings
                .iter()
                .map(|binding| (binding.input.to_string(), binding.description.clone()))
                .collect::<Vec<_>>(),
            [
                ("LeftDown".to_string(), "On the leaf".to_string()),
                ("ScrollUp".to_string(), "Scroll up".to_string()),
            ],
            "the more specific path wins, and an inactive mode contributes nothing"
        );
        assert_eq!(
            snapshot
                .bindings
                .iter()
                .map(|binding| binding.input.to_string())
                .collect::<Vec<_>>(),
            ["a"],
            "the key list stays key-only"
        );

        // Every other field means the same as it does for a key.
        let winner = &snapshot.mouse_bindings[0];
        assert_eq!(winner.path_filter, "leaf/");
        assert_eq!(winner.route_path, Path::from("/root/leaf"));
        assert_eq!(winner.phase, BindingPhase::AfterWidget);
        assert_eq!(winner.tier, BindingTier::Default);
        assert_eq!(winner.source.as_deref(), Some("test"));
        assert!(winner.command.is_none(), "a script callback stays opaque");

        // A mode contributes its own winner once it is active. Order follows
        // the label, so presentation is stable whatever the insertion order.
        core.input_map.push_mode("insert");
        let snapshot = core.available_bindings(None)?;
        assert_eq!(
            snapshot
                .mouse_bindings
                .iter()
                .map(|binding| binding.input.to_string())
                .collect::<Vec<_>>(),
            ["Ctrl+RightDown", "LeftDown", "ScrollUp"]
        );
        Ok(())
    }

    #[test]
    fn a_framework_group_admits_only_its_own_mouse_records() -> Result<()> {
        const GROUP: FrameworkBindingGroup = FrameworkBindingGroup::new("root.help");
        let mut core = Core::new();
        let leaf = core.create_detached(Leaf)?;
        core.attach(core.root, leaf)?;
        bind_mouse(
            &mut core,
            BindingTier::Default,
            "LeftDown",
            "",
            "Application click",
            1,
        )?;
        let group = GROUP;
        core.input_map.bind(
            Mouse::parse_spec("LeftDown")?.into(),
            BindingOptions {
                path: Some("/root/**/".parse()?),
                tier: BindingTier::Framework(group),
                description: "Dialog click".to_string(),
                source: None,
                phase: Some(BindingPhase::AfterWidget),
            },
            BindingAction::Command(CommandCall {
                id: CommandId("binding_list::scroll_down"),
                args: CommandArgs::default(),
                target: None,
            }),
        )?;
        core.input_map
            .set_modal_bindings(Some(ModalBindings::Framework {
                groups: &[GROUP],
                intents: &[],
            }));

        let snapshot = core.available_bindings(Some(leaf))?;
        assert_eq!(snapshot.mouse_bindings.len(), 1);
        assert_eq!(snapshot.mouse_bindings[0].description, "Dialog click");
        Ok(())
    }

    #[test]
    fn a_mouse_binding_reports_its_command_status_like_a_key() -> Result<()> {
        let mut core = Core::new();
        let enabled = Rc::new(Cell::new(false));
        let leaf = core.create_detached(EligibleLeaf {
            enabled: enabled.clone(),
        })?;
        core.attach(core.root, leaf)?;
        core.commands.add(EligibleLeaf::commands())?;
        let call = EligibleLeaf::call_update(7).with_target(CommandTarget::Exact(leaf));
        core.input_map.bind(
            InputSpec::Mouse(Mouse::parse_spec("LeftDown")?),
            BindingOptions {
                path: Some("eligible_leaf/".parse()?),
                tier: BindingTier::Default,
                description: "Update selection".into(),
                source: None,
                phase: Some(BindingPhase::AfterWidget),
            },
            BindingAction::Command(call),
        )?;
        let status = |core: &Core| {
            core.available_bindings(Some(leaf))
                .expect("snapshot")
                .mouse_bindings
                .first()
                .and_then(|binding| {
                    binding
                        .command
                        .as_ref()?
                        .availability
                        .as_ref()?
                        .status
                        .clone()
                })
        };
        assert_eq!(
            status(&core),
            Some(CommandStatus::Disabled("no selection".into()))
        );
        enabled.set(true);
        assert_eq!(status(&core), Some(CommandStatus::Enabled));
        Ok(())
    }

    #[test]
    fn availability_uses_route_tiers_and_reports_fallback_phase() -> Result<()> {
        let mut core = Core::new();
        let leaf = core.create_detached(Leaf)?;
        core.attach(core.root, leaf)?;
        core.set_focus(leaf)?;
        bind(&mut core, BindingTier::Default, 'a', "root", "Fallback", 1)?;
        bind(
            &mut core,
            BindingTier::Mode("insert".to_string()),
            'b',
            "leaf/",
            "Mode",
            2,
        )?;
        bind(
            &mut core,
            BindingTier::Global,
            'b',
            "/root/**/",
            "Global",
            3,
        )?;
        core.input_map.push_mode("insert");

        let snapshot = core.available_bindings(None)?;

        assert_eq!(snapshot.focus, leaf);
        assert_eq!(snapshot.focus_path, Path::from("/root/leaf"));
        assert_eq!(snapshot.active_modes, ["insert"]);
        assert_eq!(snapshot.bindings.len(), 2);
        let fallback = snapshot
            .bindings
            .iter()
            .find(|binding| binding.input == 'a')
            .expect("fallback binding");
        assert_eq!(fallback.phase, BindingPhase::AfterWidget);
        let global = snapshot
            .bindings
            .iter()
            .find(|binding| binding.input == 'b')
            .expect("global binding");
        assert_eq!(global.description, "Global");
        assert_eq!(global.tier, BindingTier::Global);
        Ok(())
    }

    struct EligibleLeaf {
        enabled: Rc<Cell<bool>>,
    }

    impl Widget for EligibleLeaf {
        fn name(&self) -> NodeName {
            NodeName::convert("eligible_leaf")
        }
    }

    #[crate::derive_commands]
    impl EligibleLeaf {
        fn can_update(&self, _ctx: &dyn crate::ViewContext) -> Result<CommandStatus> {
            Ok(if self.enabled.get() {
                CommandStatus::Enabled
            } else {
                CommandStatus::Disabled("no selection".into())
            })
        }

        #[command(enabled = "can_update")]
        fn update(&self, value: i64) {
            let _ = value;
        }
    }

    #[test]
    fn command_binding_snapshot_captures_intent_and_current_status() -> Result<()> {
        use crate::{commands::CommandNode, input::BindingOptions};

        let mut core = Core::new();
        let enabled = Rc::new(Cell::new(false));
        let leaf = core.create_detached(EligibleLeaf {
            enabled: enabled.clone(),
        })?;
        core.attach(core.root, leaf)?;
        core.commands.add(EligibleLeaf::commands())?;
        let call = EligibleLeaf::call_update(7).with_target(CommandTarget::Exact(leaf));
        core.input_map.bind(
            InputSpec::Key('u'.into()),
            BindingOptions {
                path: Some("eligible_leaf/".parse()?),
                tier: BindingTier::Default,
                description: "Update selection".into(),
                source: None,
                phase: Some(BindingPhase::AfterWidget),
            },
            BindingAction::Command(call.clone()),
        )?;
        let snapshot = core.available_bindings(Some(leaf))?;
        let binding = &snapshot.bindings[0];
        assert_eq!(binding.phase, BindingPhase::AfterWidget);
        let command = binding.command.as_ref().expect("command details");
        assert_eq!(command.call, call);
        let availability = command.availability.as_ref().expect("registered command");
        assert_eq!(
            availability.resolution,
            Some(CommandResolution::Exact { target: leaf })
        );
        assert_eq!(
            availability.status,
            Some(CommandStatus::Disabled("no selection".into()))
        );
        enabled.set(true);
        let refreshed = core.available_bindings(Some(leaf))?;
        let refreshed = refreshed.bindings[0]
            .command
            .as_ref()
            .and_then(|command| command.availability.as_ref())
            .expect("registered command");
        assert_eq!(refreshed.status, Some(CommandStatus::Enabled));
        assert_eq!(
            availability.status,
            Some(CommandStatus::Disabled("no selection".into()))
        );
        Ok(())
    }

    #[test]
    fn explicit_detached_or_missing_nodes_are_rejected() -> Result<()> {
        let mut core = Core::new();
        let detached = core.create_detached(Leaf)?;
        assert!(matches!(
            core.available_bindings(Some(detached)),
            Err(Error::NodeDetached(id)) if id == detached
        ));
        core.remove_subtree(detached)?;
        assert!(matches!(
            core.available_bindings(Some(detached)),
            Err(Error::NodeNotFound(id)) if id == detached
        ));
        Ok(())
    }

    #[test]
    fn a_widget_that_handles_a_key_hides_after_widget_bindings() -> Result<()> {
        let mut core = Core::new();
        let leaf = core.create_detached(CapturingLeaf)?;
        core.attach(core.root, leaf)?;
        core.set_focus(leaf)?;
        bind(
            &mut core,
            BindingTier::Default,
            'x',
            "capturing_leaf/",
            "Shadowed",
            1,
        )?;
        bind(
            &mut core,
            BindingTier::Default,
            'y',
            "capturing_leaf/",
            "Available",
            2,
        )?;
        bind_phase(
            &mut core,
            BindingTier::Default,
            'z',
            "capturing_leaf/",
            "Before widget",
            3,
            BindingPhase::BeforeWidget,
        )?;
        bind(
            &mut core,
            BindingTier::Default,
            'x',
            "root",
            "Ancestor shadowed",
            4,
        )?;

        let snapshot = core.available_bindings(None)?;
        let mut rows = snapshot
            .bindings
            .iter()
            .map(|binding| (binding.input.to_string(), binding.description.clone()))
            .collect::<Vec<_>>();
        rows.sort();
        assert_eq!(
            rows,
            [
                ("y".to_string(), "Available".to_string()),
                ("z".to_string(), "Before widget".to_string()),
            ],
            "a handled key hides after-widget bindings at the widget and above, \
             and leaves before-widget bindings alone"
        );
        Ok(())
    }

    #[test]
    fn a_transient_mode_keeps_its_bindings_despite_widget_capture() -> Result<()> {
        let mut core = Core::new();
        let leaf = core.create_detached(CapturingLeaf)?;
        core.attach(core.root, leaf)?;
        core.set_focus(leaf)?;
        bind(
            &mut core,
            BindingTier::Mode("go".to_string()),
            'x',
            "capturing_leaf/",
            "Mode key",
            1,
        )?;
        core.input_map.push_transient_mode("go");

        let snapshot = core.available_bindings(None)?;
        assert_eq!(
            snapshot
                .bindings
                .iter()
                .map(|binding| binding.description.clone())
                .collect::<Vec<_>>(),
            ["Mode key"],
            "a transient mode takes the key before the widget sees it"
        );
        Ok(())
    }

    #[test]
    fn discovery_probes_canonical_keys_while_dispatch_uses_raw_keys() -> Result<()> {
        /// Consumes the raw Ctrl+A control code as text.
        struct ControlCodeLeaf;

        impl Widget for ControlCodeLeaf {
            fn key_outcome(&self, key: Key, _context: &dyn ViewContext) -> EventOutcome {
                if key.key == KeyCode::Char('\u{1}') {
                    EventOutcome::Handle
                } else {
                    EventOutcome::Ignore
                }
            }

            fn name(&self) -> NodeName {
                NodeName::convert("control_code_leaf")
            }
        }

        let mut core = Core::new();
        let leaf = core.create_detached(ControlCodeLeaf)?;
        core.attach(core.root, leaf)?;
        core.set_focus(leaf)?;
        core.input_map.bind(
            InputSpec::Key(Key::parse_spec("ctrl-a")?),
            BindingOptions {
                path: Some("control_code_leaf/".parse()?),
                tier: BindingTier::Default,
                description: "Canonical binding".to_string(),
                source: None,
                phase: Some(BindingPhase::AfterWidget),
            },
            BindingAction::Script(LuauFunctionId::for_test(1)),
        )?;

        let raw = Key::from('\u{1}');
        assert_eq!(
            core.node_key_outcome(leaf, raw, leaf)?,
            EventOutcome::Handle,
            "dispatch passes the raw control code"
        );
        assert_eq!(
            core.node_key_outcome(leaf, Key::parse_spec("ctrl-a")?, leaf)?,
            EventOutcome::Ignore,
            "discovery probes the canonical key"
        );
        let snapshot = core.available_bindings(None)?;
        assert_eq!(snapshot.bindings.len(), 1);
        assert_eq!(snapshot.bindings[0].input.to_string(), "Ctrl+a");
        Ok(())
    }

    #[test]
    fn a_framework_group_blocks_application_tiers() -> Result<()> {
        const GROUP: FrameworkBindingGroup = FrameworkBindingGroup::new("root.help");
        let mut core = Core::new();
        let leaf = core.create_detached(Leaf)?;
        core.attach(core.root, leaf)?;
        bind(&mut core, BindingTier::Default, 'a', "", "Application", 1)?;
        let group = GROUP;
        core.input_map.bind(
            'j'.into(),
            BindingOptions {
                path: Some("/root/help/**/".parse()?),
                tier: BindingTier::Framework(group),
                description: "Scroll down".to_string(),
                source: None,
                phase: Some(BindingPhase::AfterWidget),
            },
            BindingAction::Command(CommandCall {
                id: CommandId("binding_list::scroll_down"),
                args: CommandArgs::default(),
                target: None,
            }),
        )?;
        core.input_map
            .set_modal_bindings(Some(ModalBindings::Framework {
                groups: &[GROUP],
                intents: &[],
            }));

        let snapshot = core.available_bindings(Some(leaf))?;

        assert_eq!(snapshot.framework_group, Some(group));
        assert!(snapshot.bindings.is_empty());
        assert!(
            core.input_map
                .bindings()
                .iter()
                .any(|record| { matches!(record.action, BindingAction::Command(_)) })
        );
        Ok(())
    }
}
