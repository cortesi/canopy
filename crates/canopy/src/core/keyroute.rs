//! Prospective key-route analysis.
//!
//! The analyzer walks the same route as key dispatch without running widget
//! effects, so a caller can explain where one key would go. Every widget
//! predicts its keys, so the analysis is exact for the current state. It is
//! still advisory: normal routing resolves and dispatches one node at a time,
//! because an ignored widget can change the tree, focus, or bindings before
//! the route reaches an ancestor.

use crate::{
    core::{
        Core, NodeId,
        inputmap::{
            BindingId, BindingPhase, BindingRecord, BindingTargetKind, InputSpec, RegistryStatus,
        },
    },
    error::Result,
    event::key::Key,
    path::Path,
    widget::EventOutcome,
};

/// One prospective key-route analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyRouteExplanation {
    /// Raw key being analyzed.
    pub key: Key,
    /// Node the route starts from.
    pub focus: NodeId,
    /// Path from the root to the focus.
    pub focus_path: Path,
    /// Nodes examined in focus-to-root order, stopping at the outcome.
    pub steps: Vec<KeyRouteStep>,
    /// What would act first.
    pub outcome: RouteOutcome,
}

/// One examined node on a prospective key route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyRouteStep {
    /// Examined node.
    pub node: NodeId,
    /// Route path at which the node was examined.
    pub path: Path,
    /// Binding selected at this node, when one exists.
    pub binding: Option<StepBinding>,
    /// Widget prediction for the key.
    pub widget: EventOutcome,
}

/// The binding selected at one examined route node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepBinding {
    /// Selected binding.
    pub id: BindingId,
    /// Kind of target the binding runs.
    pub kind: BindingTargetKind,
    /// Phase of the binding relative to the node's widget.
    pub phase: BindingPhase,
}

impl StepBinding {
    /// Describe one selected record.
    fn of(record: &BindingRecord) -> Self {
        Self {
            id: record.id,
            kind: BindingTargetKind::of(&record.target),
            phase: record.phase,
        }
    }
}

/// What would act first on a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteOutcome {
    /// A binding on the route runs: a before-widget binding, a widget action
    /// its node accepts, or an after-widget binding whose widget ignores the
    /// key.
    Binding(RouteWinner),
    /// A transient mode's binding runs before any widget sees the key.
    Transient(RouteWinner),
    /// A widget consumes the key.
    Widget {
        /// Consuming node.
        node: NodeId,
        /// Route path of the consuming node.
        path: Path,
    },
    /// A transient mode ends with no binding; the key is still consumed.
    TransientDismiss,
    /// No binding or widget handles the key.
    Unhandled,
}

impl RouteOutcome {
    /// Return the winning binding, when a binding acts.
    #[must_use]
    pub fn winner(&self) -> Option<&RouteWinner> {
        match self {
            Self::Binding(winner) | Self::Transient(winner) => Some(winner),
            Self::Widget { .. } | Self::TransientDismiss | Self::Unhandled => None,
        }
    }
}

/// The binding that acts on a key, and where it resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteWinner {
    /// Winning binding.
    pub binding: BindingId,
    /// Node the binding resolved at. For a widget action, this is the
    /// accepting node.
    pub node: NodeId,
    /// Route path of the winning binding.
    pub path: Path,
    /// Kind of target the binding runs.
    pub kind: BindingTargetKind,
    /// Phase of the binding relative to the node's widget.
    pub phase: BindingPhase,
}

impl RouteWinner {
    /// Describe one winning record at its route node.
    fn new(record: &BindingRecord, node: NodeId, path: Path) -> Self {
        Self {
            binding: record.id,
            node,
            path,
            kind: BindingTargetKind::of(&record.target),
            phase: record.phase,
        }
    }
}

impl Core {
    /// Explain where `key` would go if the route started at `requested`.
    ///
    /// The walk consults the same resolver and widget predictions as key
    /// dispatch, including transient-mode and modal admission.
    pub(crate) fn explain_key(
        &self,
        requested: Option<NodeId>,
        key: Key,
    ) -> Result<KeyRouteExplanation> {
        let focus = requested.or(self.focus).unwrap_or(self.root);
        self.validate_attached_node(focus)?;
        let focus_path = self.path_of(self.root, focus);
        if self.effective_transient_mode().is_some() {
            let outcome = match self.transient_winner(focus, key) {
                Some((node, path, record)) => {
                    RouteOutcome::Transient(RouteWinner::new(record, node, path))
                }
                None => RouteOutcome::TransientDismiss,
            };
            return Ok(KeyRouteExplanation {
                key,
                focus,
                focus_path,
                steps: Vec::new(),
                outcome,
            });
        }
        let mut steps = Vec::new();
        let mut outcome = RouteOutcome::Unhandled;
        for (node, path) in self.route(focus) {
            let widget = self.predict_key(node, key, focus);
            let selected = self.select_key_binding(node, &path, key, focus, &[]);
            steps.push(KeyRouteStep {
                node,
                path: path.clone(),
                binding: selected.map(StepBinding::of),
                widget,
            });
            // A before-widget binding runs whatever the widget would do; an
            // after-widget binding runs only when the widget ignores the key.
            if let Some(record) = selected.filter(|record| {
                record.phase == BindingPhase::BeforeWidget || widget == EventOutcome::Ignore
            }) {
                outcome = RouteOutcome::Binding(RouteWinner::new(record, node, path));
                break;
            }
            if widget == EventOutcome::Handle {
                outcome = RouteOutcome::Widget { node, path };
                break;
            }
        }
        Ok(KeyRouteExplanation {
            key,
            focus,
            focus_path,
            steps,
            outcome,
        })
    }
}

/// Combined registry and prospective-route verdict for one record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BindingVerdict {
    /// The record wins the key route.
    Exact,
    /// A widget consumes the key before the record.
    WidgetConsumes {
        /// Consuming node.
        node: NodeId,
    },
    /// A widget action with no accepting consumer on the inspected route.
    NoConsumer,
    /// The registry admits the record, but no key-route analysis applies.
    RegistryOnly,
    /// The registry rejects or shadows the record.
    Unavailable(RegistryStatus),
}

impl BindingVerdict {
    /// Return the stable human diagnostic label.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Exact => "route exact".to_string(),
            Self::NoConsumer => "no consumer on this route".to_string(),
            Self::RegistryOnly => "effective (context only)".to_string(),
            Self::WidgetConsumes { node } => format!("widget {node:?} consumes the key"),
            Self::Unavailable(status) => status.label(),
        }
    }
}

impl Core {
    /// Return the combined verdict for one binding record and route target.
    ///
    /// Mouse records have no point-aware analysis, so their verdict is the
    /// registry status alone.
    pub(crate) fn binding_verdict(&self, id: BindingId, target: NodeId) -> BindingVerdict {
        let route = self.diagnostic_route(target);
        let status = self.input_map.registry_status(id, &route);
        if status != RegistryStatus::Effective {
            return BindingVerdict::Unavailable(status);
        }
        let record = self
            .input_map
            .binding(id)
            .expect("an effective registry status must name a binding");
        let InputSpec::Key(key) = record.input else {
            return BindingVerdict::RegistryOnly;
        };
        if let Some(action) = record.target.widget_action() {
            // The route offers the action at every node whose path it matches
            // on the way up, so any accepting node keeps it reachable.
            let consumer = self.route(target).any(|(node, path)| {
                record.path_match(&path).is_some()
                    && self.node_accepts_action(node, action.as_str(), target)
            });
            if !consumer {
                return BindingVerdict::NoConsumer;
            }
        }
        let Ok(explanation) = self.explain_key(Some(target), key) else {
            return BindingVerdict::RegistryOnly;
        };
        match explanation.outcome {
            RouteOutcome::Widget { node, .. } => BindingVerdict::WidgetConsumes { node },
            outcome if outcome.winner().is_some_and(|winner| winner.binding == id) => {
                BindingVerdict::Exact
            }
            _ => BindingVerdict::Unavailable(RegistryStatus::NotEligible),
        }
    }

    /// Return the route paths from `target`, as dispatch would walk them.
    fn diagnostic_route(&self, target: NodeId) -> Vec<Path> {
        self.route(target).map(|(_, path)| path).collect()
    }
}

/// What a checked key dispatch expects to happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyExpectation {
    /// A specific widget consumes the key.
    Widget(NodeId),
    /// A specific binding runs.
    Binding(BindingId),
    /// A specific transient-mode binding runs.
    Transient(BindingId),
    /// A transient mode ends without running a binding.
    TransientDismiss,
    /// No binding or widget handles the key.
    Unhandled,
}

impl KeyExpectation {
    /// Return whether an analyzed outcome satisfies this expectation.
    pub(crate) fn matches(&self, outcome: &RouteOutcome) -> bool {
        match (self, outcome) {
            (Self::Widget(expected), RouteOutcome::Widget { node, .. }) => expected == node,
            (Self::Binding(expected), RouteOutcome::Binding(winner))
            | (Self::Transient(expected), RouteOutcome::Transient(winner)) => {
                *expected == winner.binding
            }
            (Self::TransientDismiss, RouteOutcome::TransientDismiss)
            | (Self::Unhandled, RouteOutcome::Unhandled) => true,
            _ => false,
        }
    }
}

/// A checked key dispatch diverged from its prospective analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyDispatchDivergence {
    /// Expectation the caller supplied.
    pub expected: KeyExpectation,
    /// Analysis step that was due, absent when the route ran past the analysis.
    pub analysis_step: Option<KeyRouteStep>,
    /// Widget outcome the route actually produced, when one was observed.
    pub actual_widget: Option<EventOutcome>,
    /// Binding the route actually resolved, when one was observed.
    pub actual_binding: Option<BindingId>,
}
