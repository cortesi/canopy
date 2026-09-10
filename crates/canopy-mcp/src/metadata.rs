//! Declared application state contracts and execution identities.

use std::{
    process,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use canopy::{
    Canopy, RenderLimits,
    error::{Error, Result as CanopyResult},
    geom::Size,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::script::stable_digest;

/// Whether evaluation constructs a new UI or uses the running application.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionMode {
    /// Each evaluation constructs a fresh application UI.
    #[default]
    FreshAppPerEval,
    /// Evaluations share one running application.
    LiveSession,
}

/// Application-declared reset behavior for domain data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResetPolicy {
    /// Domain state can persist outside the UI instance.
    #[default]
    External,
    /// An explicitly applied fixture resets the relevant domain state.
    Fixture,
    /// Each factory invocation owns independent domain state.
    Isolated,
}

/// Terminal dimensions in cells, serialized independently of internal geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Viewport {
    /// Number of columns.
    pub width: u32,
    /// Number of rows.
    pub height: u32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 120,
            height: 40,
        }
    }
}

impl Viewport {
    /// Reject empty or excessive headless dimensions before constructing an
    /// app.
    pub fn validate(self) -> crate::Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(Error::Invalid("viewport dimensions must be nonzero".into()).into());
        }
        RenderLimits::default().cell_count(self.into())?;
        Ok(())
    }
}

impl From<Viewport> for Size {
    fn from(viewport: Viewport) -> Self {
        Self::new(viewport.width, viewport.height)
    }
}

impl From<Size> for Viewport {
    fn from(size: Size) -> Self {
        Self {
            width: size.w,
            height: size.h,
        }
    }
}

/// Identity and domain reset behavior declared by an application factory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppMetadata {
    /// Stable application name used by replay compatibility checks.
    pub app: String,
    /// Domain reset behavior, independent of UI reconstruction.
    pub reset: ResetPolicy,
}

/// Shared application constructor with an explicit domain-state declaration.
#[derive(Clone)]
pub struct AppFactory {
    /// Application constructor.
    factory: Arc<dyn Fn() -> crate::Result<Canopy> + Send + Sync>,
    /// Stable metadata supplied by the application.
    metadata: AppMetadata,
}

impl AppFactory {
    /// Declare application metadata and the constructor for built instances.
    pub fn new<F>(metadata: AppMetadata, factory: F) -> Self
    where
        F: Fn() -> crate::Result<Canopy> + Send + Sync + 'static,
    {
        Self {
            factory: Arc::new(factory),
            metadata,
        }
    }

    /// Build one fully configured application instance.
    pub fn build(&self) -> crate::Result<Canopy> {
        let canopy = (self.factory)()?;
        if !canopy.is_api_finalized() {
            return Err(Error::InvalidOperation(
                "AppFactory must return an application built by CanopyBuilder".into(),
            )
            .into());
        }
        Ok(canopy)
    }

    /// Read the application declaration without constructing its UI.
    pub fn metadata(&self) -> &AppMetadata {
        &self.metadata
    }
}

/// Construct an isolated factory with stable metadata for crate tests.
#[cfg(test)]
pub fn test_app_factory<F>(factory: F) -> AppFactory
where
    F: Fn() -> crate::Result<Canopy> + Send + Sync + 'static,
{
    AppFactory::new(
        AppMetadata {
            app: "canopy-test".into(),
            reset: ResetPolicy::Isolated,
        },
        factory,
    )
}

#[cfg(test)]
impl AppMetadata {
    pub(crate) fn test() -> Self {
        Self {
            app: "canopy-test".into(),
            reset: ResetPolicy::Isolated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_rejects_an_application_that_bypasses_the_builder() {
        let factory = AppFactory::new(
            AppMetadata {
                app: "test".into(),
                reset: ResetPolicy::Isolated,
            },
            || Ok(Canopy::new()),
        );
        let error = match factory.build() {
            Ok(_) => panic!("raw Canopy should be rejected"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("application built by CanopyBuilder")
        );
    }
}

/// Owned execution metadata returned even when an evaluation fails.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionMetadata {
    /// Stable application identity.
    pub app: String,
    /// UI lifetime used by this evaluation.
    pub execution: ExecutionMode,
    /// Opaque identity of the application session, not a durable node
    /// reference.
    pub session_id: String,
    /// Evaluated or requested viewport, absent when live metadata could not be
    /// observed.
    pub viewport: Option<Viewport>,
    /// Effective domain reset contract.
    pub reset: ResetPolicy,
    /// Generated API identity, absent only when preparation could not obtain
    /// it.
    pub api_digest: Option<String>,
}

/// Session sequence within this process.
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// Allocate an opaque session identity without constructing application state.
fn session_id() -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{}-{epoch}-{}",
        process::id(),
        NEXT_SESSION.fetch_add(1, Ordering::Relaxed)
    )
}

impl ExecutionMetadata {
    /// Metadata for a fresh request before construction begins.
    pub(crate) fn fresh(app: &AppMetadata, viewport: Viewport) -> Self {
        Self {
            app: app.app.clone(),
            execution: ExecutionMode::FreshAppPerEval,
            session_id: session_id(),
            viewport: Some(viewport),
            reset: app.reset,
            api_digest: None,
        }
    }
}

/// Shared identity and fixture state for one live listener or direct session.
#[derive(Clone, Debug)]
pub struct LiveContext {
    /// Application declaration, shared across requests.
    app: AppMetadata,
    /// Session identity retained across client reconnects.
    session_id: String,
    /// Whether this live session has explicitly applied a fixture.
    fixture_applied: Arc<AtomicBool>,
}

impl LiveContext {
    /// Create one context per running app session, then clone it for requests.
    pub(crate) fn new(app: AppMetadata) -> Self {
        Self {
            app,
            session_id: session_id(),
            fixture_applied: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Record a successful explicit domain fixture application.
    pub(crate) fn fixture_applied(&self) {
        self.fixture_applied.store(true, Ordering::Relaxed);
    }

    /// Metadata available even if the live application cannot be reached.
    pub(crate) fn unavailable_metadata(&self) -> ExecutionMetadata {
        ExecutionMetadata {
            app: self.app.app.clone(),
            execution: ExecutionMode::LiveSession,
            session_id: self.session_id.clone(),
            viewport: None,
            reset: if self.fixture_applied.load(Ordering::Relaxed) {
                ResetPolicy::Fixture
            } else {
                ResetPolicy::External
            },
            api_digest: None,
        }
    }

    /// Observe live identity and viewport without preparing or mutating a
    /// frame.
    pub(crate) fn metadata(&self, canopy: &Canopy) -> CanopyResult<ExecutionMetadata> {
        let mut metadata = self.unavailable_metadata();
        if let Some(snapshot) = canopy.snapshot() {
            metadata.viewport = Some(snapshot.viewport.into());
        }
        metadata.api_digest = Some(stable_digest(canopy.script_api()?));
        Ok(metadata)
    }
}
