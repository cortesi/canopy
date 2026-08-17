//! Replay journal types and their file IO.

use std::{fs, path::Path};

use anyhow::{Context, Result, anyhow};
use canopy_mcp::{ScriptAssertion, ScriptEvalOutcome};
use serde::{Deserialize, Serialize};

/// JSON replay journal accepted by `canopyctl replay`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReplayJournal {
    /// Recorded script evaluations.
    journal: Vec<ReplayEntry>,
}

/// One replayable script evaluation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReplayEntry {
    /// Optional monotonic source journal id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<u64>,
    /// Script origin such as `eval` or `startup:app`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<String>,
    /// Evaluated Luau source text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    /// Alternate source field accepted for hand-written replay files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    script: Option<String>,
    /// Whether the original evaluation completed successfully.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ok: Option<bool>,
    /// Error message from the original evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Logs emitted by the original evaluation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    logs: Vec<String>,
    /// Assertions emitted by the original evaluation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    assertions: Vec<ScriptAssertion>,
    /// Original wall-clock duration in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
}

impl ReplayEntry {
    /// Return true when the recorded evaluation failed.
    pub fn originally_failed(&self) -> bool {
        self.ok == Some(false)
    }

    /// Return a stable human-readable origin.
    pub fn origin(&self) -> &str {
        self.origin.as_deref().unwrap_or("journal")
    }

    /// Return the script source for this replay entry.
    pub fn source(&self) -> Result<&str> {
        self.source
            .as_deref()
            .or(self.script.as_deref())
            .ok_or_else(|| {
                anyhow!(
                    "replay entry from {} has no source/script field",
                    self.origin()
                )
            })
    }
}

/// Accepted top-level JSON shapes for replay journals.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ReplayInput {
    /// Object form emitted by `canopyctl eval --journal-out`.
    Object(ReplayJournal),
    /// Bare array accepted for simple hand-authored replays.
    Entries(Vec<ReplayEntry>),
}

impl ReplayInput {
    /// Convert into replay entries.
    pub fn into_entries(self) -> Vec<ReplayEntry> {
        match self {
            Self::Object(journal) => journal.journal,
            Self::Entries(entries) => entries,
        }
    }
}

/// Load replay entries from a journal file.
pub fn load_replay_journal(path: &Path) -> Result<Vec<ReplayEntry>> {
    let contents = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_str::<ReplayInput>(&contents)
        .with_context(|| format!("parse {}", path.display()))?
        .into_entries())
}

/// Write a single-entry replay journal.
pub fn write_replay_journal(path: &Path, entry: ReplayEntry) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let journal = ReplayJournal {
        journal: vec![entry],
    };
    let encoded = serde_json::to_string_pretty(&journal)?;
    fs::write(path, encoded).with_context(|| format!("write {}", path.display()))
}

/// Build a replay entry from an eval outcome and original source.
pub fn replay_entry_from_eval(source: String, outcome: &ScriptEvalOutcome) -> ReplayEntry {
    ReplayEntry {
        id: None,
        origin: Some("canopyctl eval".to_string()),
        source: Some(source),
        script: None,
        ok: Some(outcome.success),
        error: outcome.error.as_ref().map(|error| error.message.clone()),
        logs: outcome.logs.clone(),
        assertions: outcome.assertions.clone(),
        duration_ms: Some(outcome.timing.total_ms),
    }
}
