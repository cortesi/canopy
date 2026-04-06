use std::sync::Arc;

use super::canopy::Canopy;
use crate::error::Result;

/// Shared setup closure used to realize a named fixture.
pub type FixtureSetup = Arc<dyn Fn(&mut Canopy) -> Result<()> + Send + Sync>;

/// A named, reproducible application state.
#[derive(Clone)]
pub struct Fixture {
    /// Fixture name.
    pub name: String,
    /// Human-readable fixture description.
    pub description: String,
    /// Setup closure applied to the current canopy instance.
    pub setup: FixtureSetup,
}

impl Fixture {
    /// Construct a fixture from owned name/description values.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        setup: impl Fn(&mut Canopy) -> Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            setup: Arc::new(setup),
        }
    }

    /// Return fixture metadata without the setup closure.
    pub fn info(&self) -> FixtureInfo {
        FixtureInfo {
            name: self.name.clone(),
            description: self.description.clone(),
        }
    }
}

/// Serializable metadata about a registered fixture.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct FixtureInfo {
    /// Fixture name.
    pub name: String,
    /// Human-readable fixture description.
    pub description: String,
}
