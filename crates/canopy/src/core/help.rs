//! Contextual binding discovery.

use crate::{
    commands::{
        CommandAction, CommandRequirement, CommandResolution, CommandResolver, CommandStatus,
        CommandTarget,
    },
    core::{
        Core, NodeId,
        context::CoreViewContext,
        inputmap::{
            BindingId, BindingOwner, BindingPhase, BindingScope, BindingTarget, BindingTargetKind,
            FrameworkBindingGroup, InputSpec, WidgetActionName,
        },
        world::WidgetOperation,
    },
    error::Result,
    event::{key::Key, mouse::Mouse},
    path::Path,
    widget::EventOutcome,
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
    /// Newest active mode when it is transient.
    pub transient_mode: Option<String>,
    /// Newest active exclusive binding group.
    pub exclusive_group: Option<FrameworkBindingGroup>,
    /// Effective key bindings with an exact route to a consumer.
    ///
    /// An action binding appears only when a widget on the route accepts it.
    /// An action without a consumer appears in neither this list nor
    /// `provisional_bindings`.
    pub bindings: Vec<AvailableBinding<Key>>,
    /// Key bindings whose reachability depends on an unknown widget.
    ///
    /// A widget that returns no prediction (`None`) can consume a key before a
    /// binding that discovery otherwise includes. These rows are diagnostic
    /// and do not belong in executable help.
    pub provisional_bindings: Vec<AvailableBinding<Key>>,
    /// Included key bindings whose reachability depends on an unknown widget.
    ///
    /// Each gap names the canonical key, the provisional binding, and the
    /// unknown widget that precedes it. An empty list means the returned
    /// key-binding set is exact.
    pub key_prediction_gaps: Vec<KeyPredictionGap>,
    /// Effective mouse bindings, with one winner per normalized mouse input.
    ///
    /// The route starts at the requested node, as a click on it would. The
    /// pointer's own position plays no part.
    pub mouse_bindings: Vec<AvailableBinding<Mouse>>,
}

/// One included binding made provisional by an unknown widget.
///
/// `node` and `path` identify the widget that returned no prediction, and
/// `binding` is the included binding it can hide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyPredictionGap {
    /// Canonical binding key that stays provisional.
    pub input: Key,
    /// Included binding that an unknown widget can hide.
    pub binding: BindingId,
    /// Unknown widget that precedes the binding.
    pub node: NodeId,
    /// Route path of the unknown widget.
    pub path: Path,
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
    /// Binding owner.
    pub owner: BindingOwner,
    /// Resolution scope.
    pub scope: BindingScope,
    /// Original path filter.
    pub path_filter: String,
    /// Route path at which this binding wins.
    pub route_path: Path,
    /// Kind of target this binding owns.
    pub target: BindingTargetKind,
    /// Widget action name, present for an action target.
    pub action: Option<WidgetActionName>,
    /// Phase relative to widget input handling, absent for a widget action.
    pub phase: Option<BindingPhase>,
    /// Declarative command details, absent for opaque script callbacks and
    /// widget actions.
    pub command: Option<BindingCommand>,
    /// Optional diagnostic source.
    pub source: Option<String>,
}

/// Owned command details captured with an effective key binding.
#[derive(Clone, Debug)]
pub struct BindingCommand {
    /// Stored invocation, arguments, and target policy.
    pub action: CommandAction,
    /// Resolved owner at capture time, absent for an unavailable command.
    pub resolution: Option<CommandResolution>,
    /// Eligibility at capture time, separate from target resolution.
    pub status: Option<CommandStatus>,
    /// Required context absent at capture time.
    pub missing_requirements: Vec<CommandRequirement>,
}

impl Core {
    /// Return the effective bindings for a node or the current focus.
    pub(crate) fn available_bindings(&self, requested: Option<NodeId>) -> Result<BindingSnapshot> {
        let focus = requested.or(self.focus).unwrap_or(self.root);
        self.validate_attached_node(focus)?;
        let focus_path = self.path_of(self.root, focus);

        let mut bindings = Vec::new();
        let mut provisional_bindings = Vec::new();
        let mut key_prediction_gaps = Vec::new();
        for key in self.input_map.candidate_keys() {
            let explanation = self.explain_key(Some(focus), key)?;
            let projection = self.key_projection(&explanation)?;
            bindings.extend(projection.exact);
            provisional_bindings.extend(projection.provisional);
            key_prediction_gaps.extend(projection.gaps);
        }
        let mut mouse_bindings = Vec::new();
        for mouse in self.input_map.eligible_mouse_inputs() {
            mouse_bindings.extend(self.winner_along_route(
                focus,
                &focus_path,
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
            transient_mode: self.input_map.transient_mode().map(str::to_string),
            exclusive_group: self.input_map.active_exclusive_group(),
            bindings,
            provisional_bindings,
            key_prediction_gaps,
            mouse_bindings,
        })
    }

    /// Return the binding that wins `spec` along the route from `focus`.
    ///
    /// Mouse discovery has no widget prediction, so this walk is the plain
    /// resolver route. Key discovery uses [`Core::explain_key`].
    fn winner_along_route<I>(
        &self,
        focus: NodeId,
        focus_path: &Path,
        input: I,
        spec: InputSpec,
    ) -> Result<Option<AvailableBinding<I>>> {
        let mut route_node = self.interaction_admits(focus).then_some(focus);
        let mut route_path = focus_path.clone();
        while let Some(node) = route_node {
            let Some(resolved) = self.input_map.resolve_match(&route_path, spec) else {
                route_node = if self.modal_owner() == Some(node) {
                    None
                } else {
                    self.nodes.get(node).and_then(|entry| entry.parent)
                };
                route_path.pop();
                continue;
            };
            return Ok(Some(self.available_binding(
                node,
                input,
                route_path,
                resolved.id,
            )?));
        }
        Ok(None)
    }

    /// Build one effective binding record from a resolved winner.
    pub(crate) fn available_binding<I>(
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
        let command = match &record.target {
            BindingTarget::Script(_) | BindingTarget::WidgetAction(_) => None,
            BindingTarget::Command(action) => {
                let availability = self
                    .commands
                    .get(action.invocation.id.0)
                    .map(|spec| {
                        CommandResolver::for_target(
                            self,
                            action.target.unwrap_or(CommandTarget::From(node)),
                        )
                        .availability_for(spec)
                    })
                    .transpose()?;
                Some(BindingCommand {
                    action: action.clone(),
                    resolution: availability.as_ref().and_then(|item| item.resolution),
                    status: availability.as_ref().and_then(|item| item.status.clone()),
                    missing_requirements: availability
                        .map_or_else(Vec::new, |item| item.missing_requirements),
                })
            }
        };
        Ok(AvailableBinding {
            id: record.id,
            input,
            description: record.description.clone(),
            owner: record.owner,
            scope: record.scope.clone(),
            path_filter: record.path_filter().to_string(),
            route_path,
            target: BindingTargetKind::of(&record.target),
            action: record.target.widget_action().cloned(),
            phase: record.phase,
            command,
            source: record.source.clone(),
        })
    }

    /// Return the widget's key prediction for one node, or `None` when it
    /// offers none or cannot be borrowed.
    pub(crate) fn node_key_outcome(
        &self,
        node: NodeId,
        key: Key,
        focus: NodeId,
    ) -> Option<EventOutcome> {
        let context = CoreViewContext::with_focus(self, node, focus);
        self.with_widget(
            node,
            WidgetOperation::access("binding discovery"),
            |widget, _| widget.key_outcome(key, &context),
        )
        .ok()
        .flatten()
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;
    use crate::{
        ViewContext,
        commands::{CommandAction, CommandArgs, CommandId, CommandInvocation, CommandNode},
        core::inputmap::{BindingOptions, BindingTarget, InputSpec},
        error::Error,
        event::key::KeyCode,
        script::LuauFunctionId,
        state::NodeName,
        widget::Widget,
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
        fn key_outcome(&self, key: Key, _context: &dyn ViewContext) -> Option<EventOutcome> {
            Some(if key.key == KeyCode::Char('x') {
                EventOutcome::Handle
            } else {
                EventOutcome::Ignore
            })
        }

        fn name(&self) -> NodeName {
            NodeName::convert("capturing_leaf")
        }
    }

    /// A leaf that offers no key prediction.
    struct UnknownLeaf;

    impl Widget for UnknownLeaf {
        fn name(&self) -> NodeName {
            NodeName::convert("unknown_leaf")
        }
    }

    fn bind(
        core: &mut Core,
        scope: BindingScope,
        key: char,
        path: &str,
        description: &str,
        target: u64,
    ) -> Result<()> {
        bind_phase(
            core,
            scope,
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
        scope: BindingScope,
        key: char,
        path: &str,
        description: &str,
        target: u64,
        phase: BindingPhase,
    ) -> Result<()> {
        core.input_map.replace_application_binding(
            InputSpec::Key(key.into()),
            crate::BindingOptions {
                scope,
                path: Some(path.parse()?),
                description: description.into(),
                source: Some("test".to_string()),
                phase: Some(phase),
            },
            BindingTarget::Script(LuauFunctionId::for_test(target)),
        )?;
        Ok(())
    }

    /// Bind one mouse spec, returning its normalized input.
    fn bind_mouse(
        core: &mut Core,
        scope: BindingScope,
        spec: &str,
        path: &str,
        description: &str,
        target: u64,
    ) -> Result<Mouse> {
        let mouse = Mouse::parse_spec(spec)?;
        core.input_map.replace_application_binding(
            InputSpec::Mouse(mouse),
            crate::BindingOptions {
                scope,
                path: Some(path.parse()?),
                description: description.into(),
                source: Some("test".to_string()),
                phase: Some(BindingPhase::AfterWidget),
            },
            BindingTarget::Script(LuauFunctionId::for_test(target)),
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
            BindingScope::Default,
            "LeftDown",
            "",
            "Anywhere",
            1,
        )?;
        bind_mouse(
            &mut core,
            BindingScope::Default,
            "LeftDown",
            "leaf/",
            "On the leaf",
            2,
        )?;
        bind_mouse(
            &mut core,
            BindingScope::Default,
            "ScrollUp",
            "leaf/",
            "Scroll up",
            3,
        )?;
        bind_mouse(
            &mut core,
            BindingScope::Mode("insert".to_string()),
            "ctrl-RightDown",
            "leaf/",
            "Only in insert",
            4,
        )?;
        bind(&mut core, BindingScope::Default, 'a', "leaf/", "A key", 5)?;

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
        assert!(
            snapshot.bindings.is_empty(),
            "an unknown leaf leaves no exact key row"
        );
        assert_eq!(
            snapshot
                .provisional_bindings
                .iter()
                .map(|binding| binding.input.to_string())
                .collect::<Vec<_>>(),
            ["a"],
            "the provisional list stays key-only"
        );

        // Every other field means the same as it does for a key.
        let winner = &snapshot.mouse_bindings[0];
        assert_eq!(winner.path_filter, "leaf/");
        assert_eq!(winner.route_path, Path::from("/root/leaf"));
        assert_eq!(winner.phase, Some(BindingPhase::AfterWidget));
        assert_eq!(winner.owner, BindingOwner::Application);
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
    fn an_exclusive_group_admits_only_its_own_mouse_records() -> Result<()> {
        let mut core = Core::new();
        let leaf = core.create_detached(Leaf)?;
        core.attach(core.root, leaf)?;
        bind_mouse(
            &mut core,
            BindingScope::Default,
            "LeftDown",
            "",
            "Application click",
            1,
        )?;
        let group = FrameworkBindingGroup::new("root.help");
        core.input_map.bind_framework(
            group,
            Mouse::parse_spec("LeftDown")?,
            BindingOptions {
                path: Some("/root/**/".parse()?),
                scope: BindingScope::Exclusive(group),
                description: "Dialog click".to_string(),
                source: None,
                phase: Some(BindingPhase::AfterWidget),
            },
            CommandAction {
                invocation: CommandInvocation {
                    id: CommandId("binding_list::scroll_down"),
                    args: CommandArgs::default(),
                },
                target: None,
            },
        )?;
        core.input_map
            .set_modal_bindings(Some(crate::ModalBindings::Framework(group)));

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
        let action = EligibleLeaf::call_update(7)
            .with_target(CommandTarget::Exact(leaf))
            .action();
        core.input_map.replace_application_binding(
            InputSpec::Mouse(Mouse::parse_spec("LeftDown")?),
            BindingOptions {
                path: Some("eligible_leaf/".parse()?),
                scope: BindingScope::Default,
                description: "Update selection".into(),
                source: None,
                phase: Some(BindingPhase::AfterWidget),
            },
            BindingTarget::Command(action),
        )?;
        let status = |core: &Core| {
            core.available_bindings(Some(leaf))
                .expect("snapshot")
                .mouse_bindings
                .first()
                .and_then(|binding| binding.command.as_ref()?.status.clone())
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
        bind(&mut core, BindingScope::Default, 'a', "root", "Fallback", 1)?;
        bind(
            &mut core,
            BindingScope::Mode("insert".to_string()),
            'b',
            "leaf/",
            "Mode",
            2,
        )?;
        bind(
            &mut core,
            BindingScope::Global,
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
        assert_eq!(snapshot.bindings.len(), 0);
        assert_eq!(snapshot.provisional_bindings.len(), 2);
        let fallback = snapshot
            .provisional_bindings
            .iter()
            .find(|binding| binding.input == 'a')
            .expect("fallback binding");
        assert_eq!(fallback.phase, Some(BindingPhase::AfterWidget));
        let global = snapshot
            .provisional_bindings
            .iter()
            .find(|binding| binding.input == 'b')
            .expect("global binding");
        assert_eq!(global.description, "Global");
        assert_eq!(global.scope, BindingScope::Global);
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
        use crate::{commands::CommandNode, core::inputmap::BindingOptions};

        let mut core = Core::new();
        let enabled = Rc::new(Cell::new(false));
        let leaf = core.create_detached(EligibleLeaf {
            enabled: enabled.clone(),
        })?;
        core.attach(core.root, leaf)?;
        core.commands.add(EligibleLeaf::commands())?;
        let action = EligibleLeaf::call_update(7)
            .with_target(CommandTarget::Exact(leaf))
            .action();
        core.input_map.replace_application_binding(
            InputSpec::Key('u'.into()),
            BindingOptions {
                path: Some("eligible_leaf/".parse()?),
                scope: BindingScope::Default,
                description: "Update selection".into(),
                source: None,
                phase: Some(BindingPhase::AfterWidget),
            },
            BindingTarget::Command(action.clone()),
        )?;
        let snapshot = core.available_bindings(Some(leaf))?;
        assert!(snapshot.bindings.is_empty());
        let binding = &snapshot.provisional_bindings[0];
        assert_eq!(binding.phase, Some(BindingPhase::AfterWidget));
        let command = binding.command.as_ref().expect("command details");
        assert_eq!(command.action, action);
        assert_eq!(
            command.resolution,
            Some(CommandResolution::Exact { target: leaf })
        );
        assert_eq!(
            command.status,
            Some(CommandStatus::Disabled("no selection".into()))
        );
        enabled.set(true);
        assert_eq!(
            core.available_bindings(Some(leaf))?.provisional_bindings[0]
                .command
                .as_ref()
                .unwrap()
                .status,
            Some(CommandStatus::Enabled)
        );
        assert_eq!(
            command.status,
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
            BindingScope::Default,
            'x',
            "capturing_leaf/",
            "Shadowed",
            1,
        )?;
        bind(
            &mut core,
            BindingScope::Default,
            'y',
            "capturing_leaf/",
            "Available",
            2,
        )?;
        bind_phase(
            &mut core,
            BindingScope::Default,
            'z',
            "capturing_leaf/",
            "Before widget",
            3,
            BindingPhase::BeforeWidget,
        )?;
        bind(
            &mut core,
            BindingScope::Default,
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
        assert!(
            snapshot.key_prediction_gaps.is_empty(),
            "a complete predictor leaves no gaps"
        );
        Ok(())
    }

    #[test]
    fn an_unknown_widget_keeps_an_after_widget_binding_with_a_gap() -> Result<()> {
        let mut core = Core::new();
        let leaf = core.create_detached(UnknownLeaf)?;
        core.attach(core.root, leaf)?;
        core.set_focus(leaf)?;
        bind(
            &mut core,
            BindingScope::Default,
            'a',
            "unknown_leaf/",
            "Provisional",
            1,
        )?;

        let snapshot = core.available_bindings(None)?;
        assert!(
            snapshot.bindings.is_empty(),
            "an unknown widget keeps the row out of executable help"
        );
        assert_eq!(snapshot.provisional_bindings.len(), 1);
        let binding = &snapshot.provisional_bindings[0];
        assert_eq!(binding.description, "Provisional");
        assert_eq!(snapshot.key_prediction_gaps.len(), 1);
        let gap = &snapshot.key_prediction_gaps[0];
        assert_eq!(gap.input, Key::from('a'));
        assert_eq!(gap.binding, binding.id);
        assert_eq!(gap.node, leaf);
        assert_eq!(gap.path, Path::from("/root/unknown_leaf"));
        Ok(())
    }

    #[test]
    fn an_unknown_widget_leaves_a_same_node_before_widget_binding_exact() -> Result<()> {
        let mut core = Core::new();
        let leaf = core.create_detached(UnknownLeaf)?;
        core.attach(core.root, leaf)?;
        core.set_focus(leaf)?;
        bind_phase(
            &mut core,
            BindingScope::Default,
            'a',
            "unknown_leaf/",
            "Early",
            1,
            BindingPhase::BeforeWidget,
        )?;

        let snapshot = core.available_bindings(None)?;
        assert_eq!(snapshot.bindings.len(), 1);
        assert!(
            snapshot.key_prediction_gaps.is_empty(),
            "a before-widget binding runs before its own widget"
        );
        Ok(())
    }

    #[test]
    fn an_unknown_widget_makes_an_ancestor_before_widget_binding_provisional() -> Result<()> {
        let mut core = Core::new();
        let leaf = core.create_detached(UnknownLeaf)?;
        core.attach(core.root, leaf)?;
        core.set_focus(leaf)?;
        bind_phase(
            &mut core,
            BindingScope::Default,
            'a',
            "/root/",
            "Ancestor early",
            1,
            BindingPhase::BeforeWidget,
        )?;

        let snapshot = core.available_bindings(None)?;
        assert!(snapshot.bindings.is_empty());
        assert_eq!(snapshot.provisional_bindings.len(), 1);
        assert_eq!(snapshot.key_prediction_gaps.len(), 1);
        assert_eq!(snapshot.key_prediction_gaps[0].node, leaf);
        Ok(())
    }

    #[test]
    fn a_definite_handle_after_an_unknown_leaves_no_gap() -> Result<()> {
        let mut core = Core::new();
        let parent = core.create_detached(CapturingLeaf)?;
        core.attach(core.root, parent)?;
        let leaf = core.create_detached(UnknownLeaf)?;
        core.attach(parent, leaf)?;
        core.set_focus(leaf)?;
        bind(&mut core, BindingScope::Default, 'x', "/root/", "Hidden", 1)?;

        let snapshot = core.available_bindings(None)?;
        assert!(snapshot.bindings.is_empty());
        assert!(
            snapshot.key_prediction_gaps.is_empty(),
            "a definite handle discards provisional state"
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
            BindingScope::Mode("go".to_string()),
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
        assert!(snapshot.key_prediction_gaps.is_empty());
        Ok(())
    }

    #[test]
    fn discovery_probes_canonical_keys_while_dispatch_uses_raw_keys() -> Result<()> {
        /// Consumes the raw Ctrl+A control code as text.
        struct ControlCodeLeaf;

        impl Widget for ControlCodeLeaf {
            fn key_outcome(&self, key: Key, _context: &dyn ViewContext) -> Option<EventOutcome> {
                Some(if key.key == KeyCode::Char('\u{1}') {
                    EventOutcome::Handle
                } else {
                    EventOutcome::Ignore
                })
            }

            fn name(&self) -> NodeName {
                NodeName::convert("control_code_leaf")
            }
        }

        let mut core = Core::new();
        let leaf = core.create_detached(ControlCodeLeaf)?;
        core.attach(core.root, leaf)?;
        core.set_focus(leaf)?;
        core.input_map.replace_application_binding(
            InputSpec::Key(Key::parse_spec("ctrl-a")?),
            BindingOptions {
                path: Some("control_code_leaf/".parse()?),
                scope: BindingScope::Default,
                description: "Canonical binding".to_string(),
                source: None,
                phase: Some(BindingPhase::AfterWidget),
            },
            BindingTarget::Script(LuauFunctionId::for_test(1)),
        )?;

        let raw = Key::from('\u{1}');
        assert_eq!(
            core.node_key_outcome(leaf, raw, leaf),
            Some(EventOutcome::Handle),
            "dispatch passes the raw control code"
        );
        assert_eq!(
            core.node_key_outcome(leaf, Key::parse_spec("ctrl-a")?, leaf),
            Some(EventOutcome::Ignore),
            "discovery probes the canonical key"
        );
        let snapshot = core.available_bindings(None)?;
        assert_eq!(snapshot.bindings.len(), 1);
        assert_eq!(snapshot.bindings[0].input.to_string(), "Ctrl+a");
        assert!(snapshot.key_prediction_gaps.is_empty());
        Ok(())
    }

    #[test]
    fn exclusive_context_blocks_application_tiers() -> Result<()> {
        let mut core = Core::new();
        let leaf = core.create_detached(Leaf)?;
        core.attach(core.root, leaf)?;
        bind(&mut core, BindingScope::Default, 'a', "", "Application", 1)?;
        let group = FrameworkBindingGroup::new("root.help");
        core.input_map.bind_framework(
            group,
            'j',
            BindingOptions {
                path: Some("/root/help/**/".parse()?),
                scope: BindingScope::Exclusive(group),
                description: "Scroll down".to_string(),
                source: None,
                phase: Some(BindingPhase::AfterWidget),
            },
            CommandAction {
                invocation: CommandInvocation {
                    id: CommandId("binding_list::scroll_down"),
                    args: CommandArgs::default(),
                },
                target: None,
            },
        )?;
        core.input_map
            .set_modal_bindings(Some(crate::ModalBindings::Framework(group)));

        let snapshot = core.available_bindings(Some(leaf))?;

        assert_eq!(snapshot.exclusive_group, Some(group));
        assert!(snapshot.bindings.is_empty());
        assert!(
            core.input_map
                .bindings()
                .iter()
                .any(|record| { matches!(record.target, BindingTarget::Command(_)) })
        );
        Ok(())
    }
}
