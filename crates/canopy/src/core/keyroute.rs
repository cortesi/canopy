//! Prospective key-route analysis.
//!
//! The analyzer walks the same route as key dispatch without running widget
//! effects, so a caller can explain where one key would go. The result is
//! advisory: normal routing still resolves and dispatches one node at a time,
//! because an ignored widget can change the tree, focus, or bindings before
//! the route reaches an ancestor.

use crate::{
    core::{
        Core, NodeId,
        help::{AvailableBinding, KeyPredictionGap},
        inputmap::{
            BindingId, BindingPhase, BindingTarget, BindingTargetKind, InputSpec, RegistryStatus,
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
    /// Whether every step that affects the outcome is predicted.
    pub certainty: RouteCertainty,
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
    /// Resolved binding at this node, when one exists.
    pub binding: Option<BindingId>,
    /// Phase of an ordinary binding. Absent for a widget action, which has no
    /// phase, and for a node with no binding.
    pub phase: Option<BindingPhase>,
    /// Kind of the resolved target, present exactly when `binding` is.
    pub target: Option<BindingTargetKind>,
    /// Widget prediction, absent when the widget offers none.
    pub widget: Option<EventOutcome>,
}

/// How exact a prospective key route is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteCertainty {
    /// Every step that affects the outcome is predicted.
    Exact,
    /// An unknown widget precedes the outcome, so the outcome is provisional.
    Partial,
}

/// What would act first on a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteOutcome {
    /// A transient mode's own binding runs.
    Transient {
        /// Winning binding.
        binding: BindingId,
        /// Node the binding resolved at.
        node: NodeId,
        /// Route path of the winning binding.
        path: Path,
    },
    /// A transient mode's widget action runs, with the consumer known.
    TransientWidgetAction {
        /// Winning binding.
        binding: BindingId,
        /// Node the action resolved at.
        node: NodeId,
        /// Route path of the winning binding.
        path: Path,
    },
    /// A transient mode ends with no binding; the key is still consumed.
    TransientDismiss,
    /// A widget action runs before the node's raw key handler.
    WidgetAction {
        /// Winning binding.
        binding: BindingId,
        /// Accepting node.
        node: NodeId,
        /// Route path of the winning binding.
        path: Path,
    },
    /// A `before_widget` binding runs.
    BeforeWidget {
        /// Winning binding.
        binding: BindingId,
        /// Node the binding resolved at.
        node: NodeId,
        /// Route path of the winning binding.
        path: Path,
    },
    /// A widget consumes the key.
    Widget {
        /// Consuming node.
        node: NodeId,
        /// Route path of the consuming node.
        path: Path,
    },
    /// An `after_widget` binding runs.
    AfterWidget {
        /// Winning binding.
        binding: BindingId,
        /// Node the binding resolved at.
        node: NodeId,
        /// Route path of the winning binding.
        path: Path,
    },
    /// No binding or widget handles the key.
    Unhandled,
}

impl Core {
    /// Explain where `key` would go if the route started at `requested`.
    ///
    /// The walk consults the same resolver and widget predictions as key
    /// dispatch, including transient-mode and modal admission. A step whose
    /// widget offers no prediction is unknown; certainty is `Partial` when
    /// such a step precedes the outcome.
    pub(crate) fn explain_key(
        &self,
        requested: Option<NodeId>,
        key: Key,
    ) -> Result<KeyRouteExplanation> {
        let focus = requested.or(self.focus).unwrap_or(self.root);
        self.validate_attached_node(focus)?;
        let focus_path = self.path_of(self.root, focus);
        let transient = self.input_map.transient_mode().is_some() && self.modal_region().is_none();
        if transient {
            return Ok(self.explain_transient_key(focus, focus_path, key));
        }
        let mut route_node = self.interaction_admits(focus).then_some(focus);
        let mut route_path = focus_path.clone();
        let mut steps = Vec::new();
        let mut unknown = false;
        while let Some(node) = route_node {
            let prior_unknown = unknown;
            let prediction = self.node_key_outcome(node, key, focus);
            let selected = self.select_key_binding(node, &route_path, key, focus, &[]);
            steps.push(KeyRouteStep {
                node,
                path: route_path.clone(),
                binding: selected.as_ref().map(|selected| selected.id),
                phase: selected.as_ref().and_then(|selected| selected.phase),
                target: selected
                    .as_ref()
                    .map(|selected| BindingTargetKind::of(&selected.target)),
                widget: prediction.clone(),
            });
            let Some(selected) = selected else {
                if prediction == Some(EventOutcome::Handle) {
                    return Ok(explanation(
                        key,
                        focus,
                        focus_path,
                        steps,
                        prior_unknown,
                        RouteOutcome::Widget {
                            node,
                            path: route_path,
                        },
                    ));
                }
                unknown = prior_unknown || prediction.is_none();
                route_node = if self.modal_owner() == Some(node) {
                    None
                } else {
                    self.nodes.get(node).and_then(|entry| entry.parent)
                };
                route_path.pop();
                continue;
            };
            let outcome = match (&selected.target, selected.phase) {
                (BindingTarget::WidgetAction(_), _) => RouteOutcome::WidgetAction {
                    binding: selected.id,
                    node,
                    path: route_path,
                },
                (_, Some(BindingPhase::BeforeWidget)) => RouteOutcome::BeforeWidget {
                    binding: selected.id,
                    node,
                    path: route_path,
                },
                (_, Some(BindingPhase::AfterWidget))
                    if prediction == Some(EventOutcome::Handle) =>
                {
                    RouteOutcome::Widget {
                        node,
                        path: route_path,
                    }
                }
                (_, Some(BindingPhase::AfterWidget)) => RouteOutcome::AfterWidget {
                    binding: selected.id,
                    node,
                    path: route_path,
                },
                (_, None) => {
                    debug_assert!(false, "only a widget action has no phase");
                    return Ok(explanation(
                        key,
                        focus,
                        focus_path,
                        steps,
                        prior_unknown,
                        RouteOutcome::Unhandled,
                    ));
                }
            };
            // A widget action is selected only when this node accepts it, so
            // its consumer is exact. A deeper unknown widget can still consume
            // the key first, which keeps the outcome provisional.
            let outcome_unknown = prior_unknown
                || (matches!(&outcome, RouteOutcome::AfterWidget { .. }) && prediction.is_none());
            return Ok(explanation(
                key,
                focus,
                focus_path,
                steps,
                outcome_unknown,
                outcome,
            ));
        }
        Ok(explanation(
            key,
            focus,
            focus_path,
            steps,
            unknown,
            RouteOutcome::Unhandled,
        ))
    }

    /// Analyze a key that a transient mode would take before any widget.
    fn explain_transient_key(
        &self,
        focus: NodeId,
        focus_path: Path,
        key: Key,
    ) -> KeyRouteExplanation {
        let mut route_node = self.interaction_admits(focus).then_some(focus);
        let mut route_path = focus_path.clone();
        while let Some(node) = route_node {
            if let Some(selected) = self.select_key_binding(node, &route_path, key, focus, &[]) {
                let outcome = match selected.target {
                    BindingTarget::WidgetAction(_) => RouteOutcome::TransientWidgetAction {
                        binding: selected.id,
                        node,
                        path: route_path,
                    },
                    BindingTarget::Script(_) | BindingTarget::Command(_) => {
                        RouteOutcome::Transient {
                            binding: selected.id,
                            node,
                            path: route_path,
                        }
                    }
                };
                return explanation(key, focus, focus_path, Vec::new(), false, outcome);
            }
            route_node = if self.modal_owner() == Some(node) {
                None
            } else {
                self.nodes.get(node).and_then(|entry| entry.parent)
            };
            route_path.pop();
        }
        explanation(
            key,
            focus,
            focus_path,
            Vec::new(),
            false,
            RouteOutcome::TransientDismiss,
        )
    }

    /// Project the included binding and gaps for one explained key.
    ///
    /// `available_bindings` uses this so key discovery and `explain_key`
    /// cannot disagree about reachability.
    pub(crate) fn key_projection(
        &self,
        explanation: &KeyRouteExplanation,
    ) -> Result<KeyProjection> {
        let (binding, node, path) = match &explanation.outcome {
            RouteOutcome::Transient {
                binding,
                node,
                path,
            }
            | RouteOutcome::TransientWidgetAction {
                binding,
                node,
                path,
            }
            | RouteOutcome::WidgetAction {
                binding,
                node,
                path,
            }
            | RouteOutcome::BeforeWidget {
                binding,
                node,
                path,
            }
            | RouteOutcome::AfterWidget {
                binding,
                node,
                path,
            } => (*binding, *node, path.clone()),
            RouteOutcome::TransientDismiss
            | RouteOutcome::Widget { .. }
            | RouteOutcome::Unhandled => {
                return Ok(KeyProjection {
                    exact: None,
                    provisional: None,
                    gaps: Vec::new(),
                });
            }
        };
        // A binding that runs before the raw key handler makes its own
        // node's prediction irrelevant, so that step never adds a gap.
        let runs_before_raw = matches!(
            explanation.outcome,
            RouteOutcome::BeforeWidget { .. }
                | RouteOutcome::WidgetAction { .. }
                | RouteOutcome::TransientWidgetAction { .. }
        );
        let gaps: Vec<KeyPredictionGap> = explanation
            .steps
            .iter()
            .filter(|step| step.widget.is_none())
            .filter(|step| !(runs_before_raw && step.node == node))
            .map(|step| KeyPredictionGap {
                input: explanation.key,
                binding,
                node: step.node,
                path: step.path.clone(),
            })
            .collect();
        let binding = self.available_binding(node, explanation.key, path, binding)?;
        let (exact, provisional) = if gaps.is_empty() {
            (Some(binding), None)
        } else {
            (None, Some(binding))
        };
        Ok(KeyProjection {
            exact,
            provisional,
            gaps,
        })
    }
}

/// Binding and gaps projected from one explained key.
pub(crate) struct KeyProjection {
    /// Binding with an exact route to a consumer.
    pub exact: Option<AvailableBinding<Key>>,
    /// Binding behind an unknown widget, for diagnostics only.
    pub provisional: Option<AvailableBinding<Key>>,
    /// Unknown widgets that make the binding provisional.
    pub gaps: Vec<KeyPredictionGap>,
}

/// Combined registry and prospective-route verdict for one record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BindingVerdict {
    /// The record wins and the key route is exact.
    Exact,
    /// A widget consumes the key before the record.
    WidgetConsumes {
        /// Consuming node.
        node: NodeId,
    },
    /// An unknown widget makes the record provisional.
    Provisional,
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
            Self::Provisional => "provisional behind an unknown widget".to_string(),
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
            let mut node = Some(target);
            let mut path = self.path_of(self.root, target);
            let mut consumer = false;
            while let Some(current) = node {
                if record.path_match(&path).is_some()
                    && self.node_accepts_action(current, action.as_str(), target)
                {
                    consumer = true;
                    break;
                }
                node = self.nodes.get(current).and_then(|entry| entry.parent);
                path.pop();
            }
            if !consumer {
                return BindingVerdict::NoConsumer;
            }
        }
        let Ok(explanation) = self.explain_key(Some(target), key) else {
            return BindingVerdict::RegistryOnly;
        };
        match explanation.certainty {
            RouteCertainty::Partial => BindingVerdict::Provisional,
            RouteCertainty::Exact => match explanation.outcome {
                RouteOutcome::Transient { binding, .. }
                | RouteOutcome::TransientWidgetAction { binding, .. }
                | RouteOutcome::WidgetAction { binding, .. }
                | RouteOutcome::BeforeWidget { binding, .. }
                | RouteOutcome::AfterWidget { binding, .. }
                    if binding == id =>
                {
                    BindingVerdict::Exact
                }
                RouteOutcome::Widget { node, .. } => BindingVerdict::WidgetConsumes { node },
                _ => BindingVerdict::Unavailable(RegistryStatus::NotEligible),
            },
        }
    }

    /// Return the diagnostic target-to-root route.
    fn diagnostic_route(&self, target: NodeId) -> Vec<Path> {
        let mut route = Vec::new();
        let mut route_path = self.path_of(self.root, target);
        let mut route_node = Some(target);
        while let Some(node) = route_node {
            route.push(route_path.clone());
            route_node = self.nodes.get(node).and_then(|entry| entry.parent);
            route_path.pop();
        }
        route
    }
}

/// Assemble one explanation with certainty derived from its unknowns.
fn explanation(
    key: Key,
    focus: NodeId,
    focus_path: Path,
    steps: Vec<KeyRouteStep>,
    unknown: bool,
    outcome: RouteOutcome,
) -> KeyRouteExplanation {
    KeyRouteExplanation {
        key,
        focus,
        focus_path,
        steps,
        certainty: if unknown {
            RouteCertainty::Partial
        } else {
            RouteCertainty::Exact
        },
        outcome,
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
            (
                Self::Binding(expected),
                RouteOutcome::WidgetAction { binding, .. }
                | RouteOutcome::BeforeWidget { binding, .. }
                | RouteOutcome::AfterWidget { binding, .. },
            ) => expected == binding,
            (
                Self::Transient(expected),
                RouteOutcome::Transient { binding, .. }
                | RouteOutcome::TransientWidgetAction { binding, .. },
            ) => expected == binding,
            (Self::TransientDismiss, RouteOutcome::TransientDismiss) => true,
            (Self::Unhandled, RouteOutcome::Unhandled) => true,
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
