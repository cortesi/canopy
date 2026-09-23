//! Registration before the application API is finalized.

use std::path::PathBuf;

use super::{Canopy, Hook};
use crate::{
    commands,
    core::inputmap,
    error::{Error, Result},
    render::RenderLimits,
    script::Fixture,
    style::StyleMap,
};

/// Per-type registration that runs before the application API is finalized.
///
/// An implementation registers what its type needs at runtime: commands,
/// default bindings, framework bindings, intents, fixtures, and mode
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

    /// Register one bindable intent.
    ///
    /// The catalog carries names and descriptions; widgets decide whether
    /// they consume an intent. A widget type registers each intent it
    /// implements from its `Register` impl. Registering the same intent again
    /// is harmless; a different description for the same name is an error.
    pub fn register_intent(&mut self, spec: inputmap::IntentSpec) -> Result<()> {
        self.canopy.core.input_map.register_intent(spec)
    }

    /// Register a hook that runs against the root context before the next
    /// frame whenever the mode stack has changed.
    ///
    /// Registering a name again replaces its hook. Hooks run in name order.
    pub fn register_mode_hook(&mut self, name: &'static str, hook: Hook) {
        self.canopy.mode_hooks.insert(name, hook);
    }

    /// Register a hook that runs against the root context whenever the shown
    /// notice changes: after a notice is recorded, before the next frame, and
    /// when input dismisses it, before that input routes.
    ///
    /// A hook reads the shown notice with
    /// [`ViewContext::notice`](crate::ViewContext::notice). Registering a name
    /// again replaces its hook. Hooks run in name order.
    pub fn register_notice_hook(&mut self, name: &'static str, hook: Hook) {
        self.canopy.notice_hooks.insert(name, hook);
    }

    /// Install one binding for `input`.
    ///
    /// The tier in `options` decides the semantics. A framework tier
    /// ([`BindingTier::Framework`](inputmap::BindingTier::Framework)) names the
    /// group the binding joins; registering the same binding again is a no-op,
    /// and a different binding for the same group, input, and path is an
    /// error. An application tier replaces any binding with the same tier,
    /// input, and path. A command call without a target resolves from the node
    /// where the binding wins. An intent must already be registered with
    /// [`Self::register_intent`].
    pub fn bind(
        &mut self,
        input: impl Into<inputmap::InputSpec>,
        options: inputmap::BindingOptions,
        target: inputmap::BindingAction,
    ) -> Result<inputmap::BindingId> {
        let (id, removed) = self
            .canopy
            .core
            .input_map
            .bind(input.into(), options, target)?;
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
