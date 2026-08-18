//! Contextual binding discovery.

use crate::{
    core::{
        Core, NodeId,
        inputmap::{
            BindingId, BindingOwner, BindingPhase, BindingScope, FrameworkBindingGroup, InputSpec,
        },
    },
    error::Result,
    event::key::Key,
    path::Path,
};

/// Owned snapshot of the effective key bindings for one focus context.
#[derive(Clone, Debug)]
pub struct BindingSnapshot {
    /// Node used as the discovery focus.
    pub focus: NodeId,
    /// Path from the root to the focus.
    pub focus_path: Path,
    /// Active non-default modes in resolution order.
    pub active_modes: Vec<String>,
    /// Newest active exclusive binding group.
    pub exclusive_group: Option<FrameworkBindingGroup>,
    /// Effective key bindings, with one winner per normalized key.
    pub bindings: Vec<AvailableBinding>,
}

/// One effective key binding in a contextual snapshot.
#[derive(Clone, Debug)]
pub struct AvailableBinding {
    /// Stable binding identifier.
    pub id: BindingId,
    /// Normalized key.
    pub key: Key,
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
    /// Phase relative to widget input handling.
    pub phase: BindingPhase,
    /// Optional diagnostic source.
    pub source: Option<String>,
}

impl Core {
    /// Return the effective key bindings for a node or the current focus.
    pub(crate) fn available_bindings(&self, requested: Option<NodeId>) -> Result<BindingSnapshot> {
        let focus = requested.or(self.focus).unwrap_or(self.root);
        self.validate_attached_node(focus)?;
        let focus_path = self.node_path(self.root, focus);
        let mut bindings = Vec::new();

        for key in self.input_map.eligible_keys() {
            let mut route_node = Some(focus);
            let mut route_path = focus_path.clone();
            while let Some(node) = route_node {
                let Some(resolved) = self
                    .input_map
                    .resolve_match(&route_path, InputSpec::Key(key))
                else {
                    route_node = self.nodes.get(node).and_then(|entry| entry.parent);
                    route_path.pop();
                    continue;
                };
                let record = self
                    .input_map
                    .binding(resolved.id)
                    .expect("resolved binding record must remain registered");
                bindings.push(AvailableBinding {
                    id: record.id,
                    key,
                    description: record.description.clone(),
                    owner: record.owner,
                    scope: record.scope.clone(),
                    path_filter: record.path_filter().to_string(),
                    route_path: route_path.clone(),
                    phase: resolved.phase,
                    source: record.source.clone(),
                });
                break;
            }
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
            exclusive_group: self.input_map.active_exclusive_group(),
            bindings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commands::{CommandArgs, CommandId, CommandInvocation},
        core::inputmap::{BindingTarget, InputSpec},
        error::Error,
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

    fn bind(
        core: &mut Core,
        scope: BindingScope,
        key: char,
        path: &str,
        description: &str,
        target: u64,
    ) -> Result<()> {
        core.input_map.replace_application_binding(
            scope,
            InputSpec::Key(key.into()),
            path,
            description,
            Some("test".to_string()),
            LuauFunctionId::for_test(target),
        )?;
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
        core.input_map.push_mode("insert")?;

        let snapshot = core.available_bindings(None)?;

        assert_eq!(snapshot.focus, leaf);
        assert_eq!(snapshot.focus_path, Path::from("/root/leaf"));
        assert_eq!(snapshot.active_modes, ["insert"]);
        assert_eq!(snapshot.bindings.len(), 2);
        let fallback = snapshot
            .bindings
            .iter()
            .find(|binding| binding.key == 'a')
            .expect("fallback binding");
        assert_eq!(fallback.phase, BindingPhase::AfterIgnore);
        let global = snapshot
            .bindings
            .iter()
            .find(|binding| binding.key == 'b')
            .expect("global binding");
        assert_eq!(global.description, "Global");
        assert_eq!(global.scope, BindingScope::Global);
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
    fn exclusive_context_blocks_application_tiers() -> Result<()> {
        let mut core = Core::new();
        let leaf = core.create_detached(Leaf)?;
        core.attach(core.root, leaf)?;
        bind(&mut core, BindingScope::Default, 'a', "", "Application", 1)?;
        let group = FrameworkBindingGroup::new("root.help");
        core.input_map.bind_framework(
            group,
            InputSpec::Key('j'.into()),
            "/root/help/**/",
            "Scroll down",
            CommandInvocation {
                id: CommandId("binding_list::scroll_down"),
                args: CommandArgs::default(),
            },
        )?;
        let token = core.input_map.push_exclusive_bindings(group, core.root)?;

        let snapshot = core.available_bindings(Some(leaf))?;

        assert_eq!(snapshot.exclusive_group, Some(group));
        assert!(snapshot.bindings.is_empty());
        assert!(
            core.input_map
                .bindings()
                .iter()
                .any(|record| { matches!(record.target, BindingTarget::Command(_)) })
        );
        core.input_map.pop_exclusive_bindings(token)?;
        Ok(())
    }
}
