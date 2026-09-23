//! Notices: recoverable failures the runtime reports while the application
//! keeps running.
//!
//! A failure from an input binding, a widget handler, or a poll is a notice
//! when [`Error::is_notice`] says so. The runtime records it here, and the
//! application shows the newest one until the next input event.

use crate::{
    NodeId,
    commands::CommandError,
    error::{Error, ScriptErrorKind},
};

/// Maximum retained notices. The oldest is dropped first.
const NOTICE_LIMIT: usize = 32;

/// Where a notice arose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeSource {
    /// A binding's command or callback failed.
    Binding,
    /// A widget's event or intent handler failed.
    Widget,
    /// A widget's poll failed.
    Poll,
}

impl NoticeSource {
    /// Every notice source, in declaration order.
    pub(crate) const ALL: [Self; 3] = [Self::Binding, Self::Widget, Self::Poll];

    /// Return a stable scripting and diagnostic label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Binding => "binding",
            Self::Widget => "widget",
            Self::Poll => "poll",
        }
    }
}

/// A recoverable failure, reported to the user and to scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// Human-readable failure message.
    pub message: String,
    /// Stable failure category, as script error payloads report it.
    pub kind: ScriptErrorKind,
    /// Where the failure arose.
    pub source: NoticeSource,
    /// Node the binding, handler, or poll ran on, when known.
    pub node: Option<NodeId>,
}

impl Notice {
    /// Describe one notice-class failure.
    ///
    /// A command implementation's error is reported by its own message,
    /// without the dispatch wrapper.
    pub(crate) fn new(error: &Error, source: NoticeSource, node: Option<NodeId>) -> Self {
        let message = match error {
            Error::Command(CommandError::Exec(source)) => source.to_string(),
            other => other.to_string(),
        };
        Self {
            message,
            kind: error.script_kind(),
            source,
            node,
        }
    }
}

/// The retained notices, and whether the newest one is shown.
#[derive(Debug, Default)]
pub struct Notices {
    /// Retained notices, oldest first.
    entries: Vec<Notice>,
    /// Whether the newest notice is shown: from its record until the next
    /// input event.
    shown: bool,
    /// Number of changes to the shown notice, so hooks know when to resync.
    generation: u64,
}

impl Notices {
    /// Record a notice and show it.
    pub(crate) fn record(&mut self, notice: Notice) {
        if self.entries.len() == NOTICE_LIMIT {
            self.entries.remove(0);
        }
        self.entries.push(notice);
        self.shown = true;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Stop showing the newest notice. Returns whether one was shown.
    pub(crate) fn dismiss(&mut self) -> bool {
        if !self.shown {
            return false;
        }
        self.shown = false;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// Return the retained notices, oldest first.
    pub(crate) fn entries(&self) -> &[Notice] {
        &self.entries
    }

    /// Return the shown notice, if one is shown.
    pub(crate) fn shown(&self) -> Option<&Notice> {
        self.entries.last().filter(|_| self.shown)
    }

    /// Return the number of changes to the shown notice so far.
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(message: &str) -> Notice {
        Notice {
            message: message.to_string(),
            kind: ScriptErrorKind::App,
            source: NoticeSource::Binding,
            node: None,
        }
    }

    #[test]
    fn the_queue_keeps_the_newest_notices_and_shows_until_dismissed() {
        let mut notices = Notices::default();
        for index in 0..NOTICE_LIMIT + 3 {
            notices.record(notice(&index.to_string()));
        }
        assert_eq!(notices.entries().len(), NOTICE_LIMIT);
        assert_eq!(notices.entries()[0].message, "3", "the oldest drop first");
        let newest = (NOTICE_LIMIT + 2).to_string();
        assert_eq!(
            notices.shown().map(|n| n.message.as_str()),
            Some(newest.as_str())
        );

        let generation = notices.generation();
        assert!(notices.dismiss());
        assert!(notices.shown().is_none());
        assert!(!notices.dismiss(), "a second dismissal changes nothing");
        assert_eq!(notices.generation(), generation + 1);
        assert_eq!(
            notices.entries().len(),
            NOTICE_LIMIT,
            "dismissal keeps the record"
        );
    }

    #[test]
    fn an_execution_failure_reports_its_own_message() {
        let error = Error::Command(CommandError::execution(Error::App("disk full".into())));
        let notice = Notice::new(&error, NoticeSource::Binding, None);
        assert_eq!(notice.message, "disk full");
        assert_eq!(notice.kind, ScriptErrorKind::CommandExecution);
    }
}
