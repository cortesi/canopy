//! Named widget actions and the per-application action catalog.
//!
//! A widget action is a binding target. The application registers each
//! bindable name before the script API is finalized, so a configuration typo
//! fails at binding registration. The catalog holds names and descriptions
//! only. Widgets decide whether they consume an action on the current route.

use std::{collections::BTreeMap, fmt};

use crate::error::{Error, Result};

/// Validated name of a bindable widget action.
///
/// A name is nonempty and dotted, such as `canopy.text.clear`. The dotted
/// form names the namespace that owns the behavior.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WidgetActionName(String);

impl WidgetActionName {
    /// Validate `name` and return it.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(Error::Invalid(
                "widget action name cannot be empty".to_string(),
            ));
        }
        if name.split('.').any(str::is_empty) {
            return Err(Error::Invalid(format!(
                "widget action name {name:?} must be dotted with nonempty segments"
            )));
        }
        if !name.contains('.') {
            return Err(Error::Invalid(format!(
                "widget action name {name:?} must be dotted, such as canopy.text.clear"
            )));
        }
        if name
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(Error::Invalid(format!(
                "widget action name {name:?} cannot contain whitespace or control characters"
            )));
        }
        Ok(Self(name))
    }

    /// Return the action name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WidgetActionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One widget action an application registers as bindable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WidgetActionSpec {
    /// Validated action name.
    name: WidgetActionName,
    /// Description that names the widget states that consume the action.
    description: String,
}

impl WidgetActionSpec {
    /// Build a spec from a name and a description.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Result<Self> {
        let description = description.into();
        if description.trim().is_empty() {
            return Err(Error::Invalid(
                "widget action description cannot be empty".to_string(),
            ));
        }
        Ok(Self {
            name: WidgetActionName::new(name)?,
            description,
        })
    }
}

/// Bindable widget actions for one application, with their descriptions.
///
/// The catalog freezes when the script API finalizes. Registration after the
/// freeze fails with the same error as other late API registration.
#[derive(Clone, Debug, Default)]
pub struct WidgetActionCatalog {
    /// Registered actions in name order.
    actions: BTreeMap<WidgetActionName, String>,
    /// Whether the catalog rejects further registration.
    frozen: bool,
}

impl WidgetActionCatalog {
    /// Register one action. An identical spec is idempotent.
    pub(crate) fn register(&mut self, spec: WidgetActionSpec) -> Result<()> {
        if self.frozen {
            return Err(Error::Invalid(
                "widget action registration is sealed after finalize_api()".to_string(),
            ));
        }
        match self.actions.get(&spec.name) {
            Some(existing) if *existing == spec.description => Ok(()),
            Some(_) => Err(Error::Invalid(format!(
                "conflicting widget action already registered for {}",
                spec.name
            ))),
            None => {
                self.actions.insert(spec.name, spec.description);
                Ok(())
            }
        }
    }

    /// Freeze the catalog after the script API finalizes.
    pub(crate) fn freeze(&mut self) {
        self.frozen = true;
    }

    /// Return whether the catalog holds no actions.
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Return whether `name` is registered.
    #[must_use]
    pub(crate) fn contains(&self, name: &WidgetActionName) -> bool {
        self.actions.contains_key(name)
    }

    /// Return every registered action in name order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&WidgetActionName, &str)> {
        self.actions
            .iter()
            .map(|(name, description)| (name, description.as_str()))
    }
}
