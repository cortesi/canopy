//! Named intents and the per-application action catalog.
//!
//! An intent is a binding target. The application registers each
//! bindable name before the script API is finalized, so a configuration typo
//! fails at binding registration. The catalog holds names and descriptions
//! only. Widgets decide whether they consume an action on the current route.

use std::{collections::BTreeMap, fmt};

use crate::error::{Error, Result};

/// Validated name of a bindable intent.
///
/// A name is nonempty and dotted, such as `canopy.clear`. The dotted
/// form names the namespace that owns the behavior.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IntentName(String);

impl IntentName {
    /// Validate `name` and return it.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(Error::Invalid("intent name cannot be empty".to_string()));
        }
        if name.split('.').any(str::is_empty) {
            return Err(Error::Invalid(format!(
                "intent name {name:?} must be dotted with nonempty segments"
            )));
        }
        if !name.contains('.') {
            return Err(Error::Invalid(format!(
                "intent name {name:?} must be dotted, such as canopy.clear"
            )));
        }
        if name
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(Error::Invalid(format!(
                "intent name {name:?} cannot contain whitespace or control characters"
            )));
        }
        Ok(Self(name))
    }

    /// Return the intent name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IntentName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One intent an application registers as bindable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentSpec {
    /// Validated action name.
    name: IntentName,
    /// Description that names the widget states that consume the action.
    description: String,
}

impl IntentSpec {
    /// Build a spec from a name and a description.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Result<Self> {
        let description = description.into();
        if description.trim().is_empty() {
            return Err(Error::Invalid(
                "intent description cannot be empty".to_string(),
            ));
        }
        Ok(Self {
            name: IntentName::new(name)?,
            description,
        })
    }
}

/// A built-in navigation intent.
///
/// Core registers these in every application. At each route node, a widget
/// that accepts one handles it: a cursor widget moves its selection.
/// Otherwise the runtime scrolls that node's view when it can move, as the
/// wheel does, and a view that cannot move declines so the route continues.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavIntent {
    /// Move up one row.
    Up,
    /// Move down one row.
    Down,
    /// Move left one column.
    Left,
    /// Move right one column.
    Right,
    /// Move up one page.
    PageUp,
    /// Move down one page.
    PageDown,
    /// Move to the first row.
    First,
    /// Move to the last row.
    Last,
}

impl NavIntent {
    /// Every navigation intent.
    pub const ALL: [Self; 8] = [
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::PageUp,
        Self::PageDown,
        Self::First,
        Self::Last,
    ];

    /// Return the intent name, such as `canopy.nav.up`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Up => "canopy.nav.up",
            Self::Down => "canopy.nav.down",
            Self::Left => "canopy.nav.left",
            Self::Right => "canopy.nav.right",
            Self::PageUp => "canopy.nav.page_up",
            Self::PageDown => "canopy.nav.page_down",
            Self::First => "canopy.nav.first",
            Self::Last => "canopy.nav.last",
        }
    }

    /// Return the navigation intent named `name`, if it is one.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|nav| nav.name() == name)
    }

    /// Return the catalog description.
    const fn description(self) -> &'static str {
        match self {
            Self::Up => "Move the selection or view up one row",
            Self::Down => "Move the selection or view down one row",
            Self::Left => "Move the view left one column",
            Self::Right => "Move the view right one column",
            Self::PageUp => "Move the selection or view up one page",
            Self::PageDown => "Move the selection or view down one page",
            Self::First => "Move the selection or view to the start",
            Self::Last => "Move the selection or view to the end",
        }
    }
}

/// Bindable intents for one application, with their descriptions.
///
/// Actions are registered during setup, before the script API finalizes.
/// Every catalog starts with the [`NavIntent`]s.
#[derive(Clone, Debug)]
pub struct IntentCatalog {
    /// Registered actions in name order.
    actions: BTreeMap<IntentName, String>,
}

impl Default for IntentCatalog {
    fn default() -> Self {
        let actions = NavIntent::ALL
            .into_iter()
            .map(|nav| {
                (
                    IntentName(nav.name().to_string()),
                    nav.description().to_string(),
                )
            })
            .collect();
        Self { actions }
    }
}

impl IntentCatalog {
    /// Register one action. An identical spec is idempotent.
    pub(crate) fn register(&mut self, spec: IntentSpec) -> Result<()> {
        match self.actions.get(&spec.name) {
            Some(existing) if *existing == spec.description => Ok(()),
            Some(_) => Err(Error::Invalid(format!(
                "conflicting intent already registered for {}",
                spec.name
            ))),
            None => {
                self.actions.insert(spec.name, spec.description);
                Ok(())
            }
        }
    }

    /// Return whether the catalog holds no actions.
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Return whether `name` is registered.
    #[must_use]
    pub(crate) fn contains(&self, name: &IntentName) -> bool {
        self.actions.contains_key(name)
    }

    /// Return every registered action in name order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&IntentName, &str)> {
        self.actions
            .iter()
            .map(|(name, description)| (name, description.as_str()))
    }
}
