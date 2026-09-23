//! Declared application state contracts and execution identities.

use std::{
    process,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use canopy::{
    Canopy,
    error::{Error, Result as CanopyResult},
    geom::Size,
    render::RenderLimits,
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
    /// Each factory invocation owns independent domain state.
    Isolated,
}

/// Terminal dimensions in cells, serialized independently of internal geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScreenSize {
    /// Number of columns.
    pub width: u32,
    /// Number of rows.
    pub height: u32,
}

impl Default for ScreenSize {
    fn default() -> Self {
        Self {
            width: 120,
            height: 40,
        }
    }
}

impl ScreenSize {
    /// Reject empty or excessive headless dimensions before constructing an
    /// app.
    pub fn validate(self) -> crate::Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(Error::Invalid("screen dimensions must be nonzero".into()).into());
        }
        RenderLimits::default().cell_count(self.into())?;
        Ok(())
    }
}

impl From<ScreenSize> for Size {
    fn from(screen: ScreenSize) -> Self {
        Self::new(screen.width, screen.height)
    }
}

impl From<Size> for ScreenSize {
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
///
/// Factory operations are synchronous and may execute application scripts.
/// Async callers should construct, use, and drop each application on one
/// blocking worker. The headless MCP server supplies bounded workers itself.
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
        (self.factory)()
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
    AppFactory::new(AppMetadata::test(), factory)
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

/// Owned execution metadata returned even when an evaluation fails.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionMetadata {
    /// Stable application identity.
    pub app: String,
    /// UI lifetime used by this evaluation.
    pub execution: ExecutionMode,
    /// Opaque identity of the application session, not a durable node
    /// reference.
    pub instance_id: String,
    /// Evaluated or requested screen, absent when live metadata could not be
    /// observed.
    pub screen: Option<ScreenSize>,
    /// The application's declared domain reset contract.
    pub reset: ResetPolicy,
    /// Fixture applied before this evaluation, if any. A fixture resets the
    /// domain state it covers, whatever the reset policy.
    pub fixture: Option<String>,
    /// Generated API identity, absent only when preparation could not obtain
    /// it.
    pub api_digest: Option<String>,
}

/// Session sequence within this process.
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// Allocate an opaque session identity without constructing application state.
fn instance_id() -> String {
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
    pub(crate) fn fresh(app: &AppMetadata, screen: ScreenSize) -> Self {
        Self {
            app: app.app.clone(),
            execution: ExecutionMode::FreshAppPerEval,
            instance_id: instance_id(),
            screen: Some(screen),
            reset: app.reset,
            fixture: None,
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
    instance_id: String,
    /// The fixture this live session applied last, if any.
    fixture: Arc<Mutex<Option<String>>>,
}

impl LiveContext {
    /// Create one context per running app session, then clone it for requests.
    pub(crate) fn new(app: AppMetadata) -> Self {
        Self {
            app,
            instance_id: instance_id(),
            fixture: Arc::default(),
        }
    }

    /// Record a successful explicit domain fixture application.
    pub(crate) fn fixture_applied(&self, name: &str) {
        *self.fixture.lock().unwrap_or_else(PoisonError::into_inner) = Some(name.to_owned());
    }

    /// Metadata available even if the live application cannot be reached.
    pub(crate) fn unavailable_metadata(&self) -> ExecutionMetadata {
        ExecutionMetadata {
            app: self.app.app.clone(),
            execution: ExecutionMode::LiveSession,
            instance_id: self.instance_id.clone(),
            screen: None,
            // A live application's domain state outlives any one evaluation.
            reset: ResetPolicy::External,
            fixture: self
                .fixture
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            api_digest: None,
        }
    }

    /// Observe live identity and screen without preparing or mutating a
    /// frame.
    pub(crate) fn metadata(&self, canopy: &Canopy) -> CanopyResult<ExecutionMetadata> {
        let mut metadata = self.unavailable_metadata();
        if let Some(snapshot) = canopy.snapshot() {
            metadata.screen = Some(snapshot.size().into());
        }
        metadata.api_digest = Some(stable_digest(canopy.script_api()?));
        Ok(metadata)
    }
}
