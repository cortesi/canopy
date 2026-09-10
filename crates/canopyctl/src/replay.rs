//! Replay journal types and their file IO.

use std::{fs, path::Path};

use anyhow::{Context, Result, anyhow, bail};
use canopy_mcp::{ExecutionMetadata, ExecutionMode, ResetPolicy, ScriptEvalOutcome, Viewport};
use serde::{Deserialize, Serialize};

/// Version identifier accepted by the strict replay parser.
const REPLAY_SCHEMA: &str = "canopy.replay/1";

/// Reproduction assumptions and ordered script expectations.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayEnvelope {
    /// Versioned wire-format identity.
    pub schema: String,
    /// Application identity declared by its factory or launcher.
    pub app: String,
    /// Digest of the API against which the source was recorded.
    pub api_digest: String,
    /// Whether each evaluation starts a new app or shares a live app.
    pub execution: ExecutionMode,
    /// Viewport used for the recorded evaluation.
    pub viewport: Viewport,
    /// Explicit reset fixture, if any.
    pub fixture: Option<String>,
    /// Declared domain-state reset contract.
    pub reset: ResetPolicy,
    /// Independent evals for fresh mode, ordered shared evals for live mode.
    pub steps: Vec<ReplayStep>,
}

/// One source and its expected completion state.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayStep {
    /// Luau source; durable references must use application identities.
    pub source: String,
    /// Recorded success or failure expectation.
    pub expect: ReplayExpectation,
}

/// Outcome fields compared during replay.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayExpectation {
    /// A selected expected failure passes when evaluation fails.
    pub success: bool,
}

impl ReplayEnvelope {
    /// Reject malformed or unusable envelopes before connection or mutation.
    pub fn validate(&self) -> Result<()> {
        if self.schema != REPLAY_SCHEMA {
            bail!("unsupported replay schema {:?}", self.schema);
        }
        if self.app.trim().is_empty() || self.api_digest.trim().is_empty() {
            bail!("replay app and api_digest must be nonempty");
        }
        self.viewport.validate()?;
        if self.steps.is_empty() {
            bail!("replay must contain at least one step");
        }
        if self
            .fixture
            .as_ref()
            .is_some_and(|name| name.trim().is_empty())
        {
            bail!("replay fixture name must be nonempty");
        }
        if self.reset == ResetPolicy::Fixture && self.fixture.is_none() {
            bail!("fixture reset requires an explicit fixture name");
        }
        Ok(())
    }

    /// List each compatible-shape assumption that differs from the target.
    pub fn mismatches(&self, target: &ExecutionMetadata) -> Result<Vec<String>> {
        let digest = target
            .api_digest
            .as_deref()
            .ok_or_else(|| anyhow!("target bootstrap has no API digest"))?;
        let viewport = target
            .viewport
            .ok_or_else(|| anyhow!("target bootstrap has no viewport"))?;
        let reset = if self.fixture.is_some() && target.reset != ResetPolicy::Isolated {
            ResetPolicy::Fixture
        } else {
            target.reset
        };
        let mut differences = Vec::new();
        for (field, recorded, actual) in [
            ("app", self.app.clone(), target.app.clone()),
            ("api_digest", self.api_digest.clone(), digest.to_owned()),
            (
                "execution",
                serde_json::to_string(&self.execution)?,
                serde_json::to_string(&target.execution)?,
            ),
            (
                "viewport",
                serde_json::to_string(&self.viewport)?,
                serde_json::to_string(&viewport)?,
            ),
            (
                "reset",
                serde_json::to_string(&self.reset)?,
                serde_json::to_string(&reset)?,
            ),
        ] {
            if recorded != actual {
                differences.push(format!("{field}: recorded {recorded}, target {actual}"));
            }
        }
        Ok(differences)
    }

    /// Record one evaluation using its actual execution metadata.
    pub fn record(
        source: String,
        fixture: Option<String>,
        outcome: &ScriptEvalOutcome,
    ) -> Result<Self> {
        let metadata = &outcome.metadata;
        let envelope = Self {
            schema: REPLAY_SCHEMA.into(),
            app: metadata.app.clone(),
            api_digest: metadata
                .api_digest
                .clone()
                .ok_or_else(|| anyhow!("cannot record replay without an API digest"))?,
            execution: metadata.execution,
            viewport: metadata
                .viewport
                .ok_or_else(|| anyhow!("cannot record replay without a viewport"))?,
            fixture,
            reset: metadata.reset,
            steps: vec![ReplayStep {
                source,
                expect: ReplayExpectation {
                    success: outcome.success,
                },
            }],
        };
        envelope.validate()?;
        Ok(envelope)
    }
}

/// Parse a versioned `canopy.replay/1` envelope.
pub fn parse_replay(contents: &str) -> Result<ReplayEnvelope> {
    let value: serde_json::Value = serde_json::from_str(contents)?;
    if value.get("schema").is_none() {
        bail!("expected canopy.replay/1 envelope");
    }
    let envelope: ReplayEnvelope = serde_json::from_value(value)?;
    envelope.validate()?;
    Ok(envelope)
}

/// Read and validate a replay before contacting its target.
pub fn load_replay(path: &Path) -> Result<ReplayEnvelope> {
    let contents = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    parse_replay(&contents).with_context(|| format!("parse {}", path.display()))
}

/// Write a complete versioned envelope.
pub fn write_replay(path: &Path, envelope: &ReplayEnvelope) -> Result<()> {
    envelope.validate()?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_string_pretty(envelope)?)
        .with_context(|| format!("write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    /// Minimal complete replay used to isolate validation errors.
    fn envelope_json() -> Value {
        json!({
            "schema": "canopy.replay/1", "app": "test", "api_digest": "digest",
            "execution": "fresh-app-per-eval", "viewport": {"width": 12, "height": 3},
            "fixture": null, "reset": "external",
            "steps": [{"source": "return true", "expect": {"success": true}}]
        })
    }

    #[test]
    fn malformed_versioned_files_are_rejected() {
        for malformed in [
            {
                let mut value = envelope_json();
                value["schema"] = json!("canopy.replay/2");
                value
            },
            {
                let mut value = envelope_json();
                value["viewport"]["width"] = json!(0);
                value
            },
            {
                let mut value = envelope_json();
                value["steps"][0]["expect"]["success"] = json!("yes");
                value
            },
            {
                let mut value = envelope_json();
                value["steps"][0].as_object_mut().unwrap().remove("source");
                value
            },
            {
                let mut value = envelope_json();
                value["reset"] = json!("fixture");
                value
            },
        ] {
            assert!(parse_replay(&malformed.to_string()).is_err());
        }
    }

    #[test]
    fn unversioned_input_is_rejected() {
        let source = "{\"journal\":[{\"source\":\"return true\",\"ok\":false}]}";
        assert!(parse_replay(source).is_err());
    }

    #[test]
    fn compatibility_lists_each_override_and_preserves_fixture_isolation() -> Result<()> {
        let envelope: ReplayEnvelope = serde_json::from_value(envelope_json())?;
        let mut metadata = ExecutionMetadata {
            app: "different".into(),
            api_digest: Some("different".into()),
            execution: ExecutionMode::LiveSession,
            session_id: "nondurable".into(),
            viewport: Some(Viewport {
                width: 14,
                height: 4,
            }),
            reset: ResetPolicy::Isolated,
        };
        let differences = envelope.mismatches(&metadata)?;
        assert_eq!(differences.len(), 5);
        for field in ["app:", "api_digest:", "execution:", "viewport:", "reset:"] {
            assert!(
                differences
                    .iter()
                    .any(|difference| difference.starts_with(field))
            );
        }
        let mut envelope = envelope;
        envelope.fixture = Some("seed".into());
        envelope.reset = ResetPolicy::Isolated;
        assert!(
            !envelope
                .mismatches(&metadata)?
                .iter()
                .any(|difference| difference.starts_with("reset:"))
        );
        metadata.reset = ResetPolicy::External;
        envelope.reset = ResetPolicy::Fixture;
        assert!(
            !envelope
                .mismatches(&metadata)?
                .iter()
                .any(|difference| difference.starts_with("reset:"))
        );
        Ok(())
    }
}
