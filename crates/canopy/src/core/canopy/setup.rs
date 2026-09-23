//! Registration before the application API is finalized.

use std::path::PathBuf;

use super::{Canopy, ModeHook};
use crate::{
    Fixture, RenderLimits, commands,
    core::inputmap,
    error::{Error, Result},
    style::StyleMap,
};

/// Per-type registration that runs before the application API is finalized.
///
/// An implementation registers what its type needs at runtime: commands,
/// default bindings, framework bindings, widget actions, fixtures, and mode
/// hooks. It never touches the widget tree; widgets are built during assembly.
/// A composite type registers the types it mounts by calling their
/// implementations.
pub trait Register {
    /// Register this type's commands, bindings, and resources.
    fn register(setup: &mut Setup) -> Result<()>;
}

/// Registration handle for an application that is not yet finalized.
///
/// [`CanopyBuilder::configure`](super::CanopyBuilder::configure) passes this
/// handle. It owns every operation that extends the command, binding, and
/// script surface. The builder finalizes the API after the configure callbacks
/// return, so none of this can change at runtime. The style map set here is
/// the initial theme; the running application can replace it.
pub struct Setup {
    /// Application under registration.
    canopy: Canopy,
}

impl Setup {
    /// Start registration for an empty application.
    pub(super) fn new() -> Self {
        Self {
            canopy: Canopy::empty(),
        }
    }

    /// Mount trusted script roots, finalize the API, and return the
    /// application.
    pub(super) fn finalize(
        mut self,
        user_root: Option<PathBuf>,
        project_root: Option<PathBuf>,
    ) -> Result<Canopy> {
        if let Some(root) = user_root {
            self.canopy.script.module_roots.set_user_root(root);
        }
        if let Some(root) = project_root {
            self.canopy.script.module_roots.set_project_root(root);
        }
        self.canopy.finalize_api()?;
        Ok(self.canopy)
    }

    /// Register the commands of a command node under its owner name.
    ///
    /// Registering an equivalent definition again is a no-op. A conflicting
    /// definition is an error and registers nothing from the batch.
    pub fn add_commands<T: commands::CommandNode>(&mut self) -> Result<()> {
        Ok(self.canopy.core.commands.add(T::commands())?)
    }

    /// Register a Luau script as the default bindings for a command owner.
    ///
    /// Scripts reach it as `<owner>.default_bindings()`. Registering the same
    /// source again is a no-op; different source for the same owner is an
    /// error.
    pub fn register_default_bindings(&mut self, owner: &str, script: &str) -> Result<()> {
        if self
            .canopy
            .core
            .commands
            .iter()
            .any(|(_, spec)| spec.owner == owner && spec.name == "default_bindings")
        {
            return Err(Error::Invalid(format!(
                "owner {owner} already defines a command named default_bindings"
            )));
        }
        self.canopy.script.register_default_bindings(owner, script)
    }

    /// Register an application startup script.
    ///
    /// Startup scripts define a `setup()` global and run during the first
    /// frame preparation, before the user and project `init.luau` modules.
    pub fn register_startup_script(&mut self, name: &str, source: &str) -> Result<()> {
        self.canopy.script.register_startup_script(name, source)
    }

    /// Register a named fixture available to headless and live automation.
    pub fn register_fixture(&mut self, fixture: Fixture) -> Result<()> {
        self.canopy.script.register_fixture(fixture)
    }

    /// Register one bindable widget action.
    ///
    /// The catalog carries names and descriptions; widgets decide whether
    /// they consume an action.
    pub fn register_widget_action(&mut self, spec: inputmap::WidgetActionSpec) -> Result<()> {
        self.canopy.core.input_map.register_widget_action(spec)
    }

    /// Register a hook that runs against the root context before the next
    /// frame whenever the mode stack has changed.
    ///
    /// Registering a name again replaces its hook. Hooks run in name order.
    pub fn register_mode_hook(&mut self, name: &'static str, hook: ModeHook) {
        self.canopy.mode_hooks.insert(name, hook);
    }

    /// Install an idempotent framework-owned command binding.
    ///
    /// `options.tier` must be [`inputmap::BindingTier::Framework`], and names
    /// the group the binding joins.
    pub fn bind_framework(
        &mut self,
        input: impl Into<inputmap::InputSpec>,
        options: inputmap::BindingOptions,
        command: commands::CommandCall,
    ) -> Result<inputmap::BindingId> {
        self.canopy
            .core
            .input_map
            .bind_framework(input, options, command)
    }

    /// Install or replace an application command binding.
    ///
    /// An omitted command target resolves from the node where the binding wins.
    pub fn bind_command(
        &mut self,
        input: impl Into<inputmap::InputSpec>,
        options: inputmap::BindingOptions,
        command: commands::CommandCall,
    ) -> Result<inputmap::BindingId> {
        self.bind(input, options, inputmap::BindingTarget::Command(command))
    }

    /// Install or replace an application widget action binding.
    ///
    /// The action must already be registered with
    /// [`Self::register_widget_action`].
    pub fn bind_widget_action(
        &mut self,
        input: impl Into<inputmap::InputSpec>,
        options: inputmap::BindingOptions,
        action: inputmap::WidgetActionName,
    ) -> Result<inputmap::BindingId> {
        self.bind(
            input,
            options,
            inputmap::BindingTarget::WidgetAction(action),
        )
    }

    /// Install or replace one application binding.
    fn bind(
        &mut self,
        input: impl Into<inputmap::InputSpec>,
        options: inputmap::BindingOptions,
        target: inputmap::BindingTarget,
    ) -> Result<inputmap::BindingId> {
        let (id, removed) = self.canopy.core.input_map.replace_application_binding(
            input.into(),
            options,
            target,
        )?;
        self.canopy.release_removed_bindings(removed);
        Ok(id)
    }

    /// Replace the limits on the visible render target.
    pub fn set_render_limits(&mut self, limits: RenderLimits) {
        self.canopy.frame.render_limits = limits;
    }

    /// Return the initial style map for mutation.
    pub fn style_mut(&mut self) -> &mut StyleMap {
        &mut self.canopy.style
    }
}
