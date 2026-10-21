//! Notices: the messages the application shows the user, kept in one log.
//!
//! Every notice has a level, and shows as a toast for its duration, and then
//! goes. A worse level shows longer: an error reports a failure. The log
//! keeps the newest notices of every level, so a user can read them again
//! after they have gone.
//!
//! The runtime records a failure from an input binding, a widget handler, or
//! a poll as an error when [`Error::is_notice`] says so. A widget posts any
//! notice with [`Context::notify`](crate::Context::notify).

use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use crate::{
    NodeId,
    commands::CommandError,
    error::{Error, ScriptErrorKind},
};

/// Maximum retained notices. The oldest is dropped first.
const NOTICE_LIMIT: usize = 100;

/// Maximum toasts that show at once. A newer toast ends the oldest early.
pub const TOAST_LIMIT: usize = 4;

/// Where a failure arose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeSource {
    /// A binding's command or callback failed.
    Binding,
    /// A widget's event or intent handler failed, or a call a widget posted.
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

/// How a notice reads, and how it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NoticeLevel {
    /// An outcome, such as an entry moved to the trash.
    Info,
    /// A refusal that needs no action, such as a key with nothing to act on.
    Warning,
    /// A failure.
    Error,
}

impl NoticeLevel {
    /// Every level, in declaration order.
    pub(crate) const ALL: [Self; 3] = [Self::Info, Self::Warning, Self::Error];

    /// Return a stable scripting label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// Return the level a scripting label names.
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.label() == label)
    }

    /// Return how long a toast of this level shows unless its post names
    /// another duration: longer for worse news.
    pub const fn default_duration(self) -> Duration {
        match self {
            Self::Info => Duration::from_secs(2),
            Self::Warning => Duration::from_secs(4),
            Self::Error => Duration::from_secs(6),
        }
    }
}

/// Identity of one entry in the notice log, unique for the life of the
/// runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoticeId(u64);

impl NoticeId {
    /// Return the number scripts see.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// What the runtime knows about a failure it caught.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    /// Stable failure category, as script error payloads report it.
    pub kind: ScriptErrorKind,
    /// Where the failure arose.
    pub source: NoticeSource,
    /// Node the binding, handler, or poll ran on, when known.
    pub node: Option<NodeId>,
}

/// One entry in the notice log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// Identity of this entry.
    pub id: NoticeId,
    /// Human-readable message.
    pub message: String,
    /// How the notice reads and shows.
    pub level: NoticeLevel,
    /// Driver clock time of the latest post this entry holds.
    pub at: Instant,
    /// How long the notice shows as a toast after `at`.
    pub duration: Duration,
    /// How many posts this entry holds: 1 for a notice posted once.
    pub repeats: u32,
    /// The caught failure, for a failure the runtime recorded.
    pub failure: Option<Failure>,
}

impl Notice {
    /// Return the count a repeated notice shows after its message, such as
    /// "×3", or `None` for a notice posted once. A view styles the count apart
    /// from the message.
    pub fn count_label(&self) -> Option<String> {
        (self.repeats > 1).then(|| format!("×{}", self.repeats))
    }

    /// Return when the notice's toast time ends.
    pub fn ends(&self) -> Instant {
        self.at + self.duration
    }
}

/// A notice that a widget or script asks to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticePost {
    /// Human-readable message.
    message: String,
    /// How the notice reads and shows.
    level: NoticeLevel,
    /// How long it shows as a toast.
    duration: Duration,
}

impl NoticePost {
    /// Post an outcome, such as an entry moved to the trash.
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Info, message)
    }

    /// Post a refusal that needs no action.
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Warning, message)
    }

    /// Post a failure.
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Error, message)
    }

    /// Post a notice of `level`.
    pub fn new(level: NoticeLevel, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level,
            duration: level.default_duration(),
        }
    }

    /// Show the toast for `duration` in place of its level's default.
    #[must_use]
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Return the message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Return the level.
    pub fn level(&self) -> NoticeLevel {
        self.level
    }

    /// Return how long the notice shows as a toast.
    pub fn duration(&self) -> Duration {
        self.duration
    }
}

/// The notice log, and which of its notices show.
#[derive(Debug, Default)]
pub struct NoticeLog {
    /// Retained notices, oldest first.
    entries: Vec<Notice>,
    /// Identity the next new entry takes.
    next_id: u64,
    /// Toasts that a click or the toast limit ended before their time.
    ended: HashSet<NoticeId>,
    /// Number of changes to the log or to what shows, so hooks know when to
    /// resync.
    generation: u64,
}

impl NoticeLog {
    /// Record `post` at `now`, and return the entry that holds it.
    ///
    /// A post that matches the newest entry, by level, message, and failure,
    /// joins that entry and restarts its time, in place of a new entry.
    pub(crate) fn post(
        &mut self,
        post: NoticePost,
        failure: Option<Failure>,
        now: Instant,
    ) -> NoticeId {
        let NoticePost {
            message,
            level,
            duration,
        } = post;
        let id = match self.entries.last_mut() {
            Some(newest)
                if newest.level == level
                    && newest.message == message
                    && newest.failure == failure =>
            {
                newest.repeats = newest.repeats.saturating_add(1);
                newest.at = now;
                newest.duration = duration;
                self.ended.remove(&newest.id);
                newest.id
            }
            _ => {
                let id = NoticeId(self.next_id);
                self.next_id += 1;
                if self.entries.len() == NOTICE_LIMIT {
                    let evicted = self.entries.remove(0);
                    self.ended.remove(&evicted.id);
                }
                self.entries.push(Notice {
                    id,
                    message,
                    level,
                    at: now,
                    duration,
                    repeats: 1,
                    failure,
                });
                id
            }
        };
        self.limit_toasts(now);
        self.generation = self.generation.wrapping_add(1);
        id
    }

    /// Record a notice-class failure that the runtime caught.
    pub(crate) fn record_failure(
        &mut self,
        error: &Error,
        source: NoticeSource,
        node: Option<NodeId>,
        now: Instant,
    ) -> NoticeId {
        // A command implementation's error is reported by its own message,
        // without the dispatch wrapper.
        let message = match error {
            Error::Command(CommandError::Exec(source)) => source.to_string(),
            other => other.to_string(),
        };
        let failure = Failure {
            kind: error.script_kind(),
            source,
            node,
        };
        self.post(NoticePost::error(message), Some(failure), now)
    }

    /// End the oldest live toasts until at most [`TOAST_LIMIT`] show.
    fn limit_toasts(&mut self, now: Instant) {
        let live = self.live_toasts(now);
        let excess = live.len().saturating_sub(TOAST_LIMIT);
        let ended = live
            .iter()
            .take(excess)
            .map(|notice| notice.id)
            .collect::<Vec<_>>();
        self.ended.extend(ended);
    }

    /// End the toast of `id` before its time. Returns whether it was live.
    pub(crate) fn end_toast(&mut self, id: NoticeId, now: Instant) -> bool {
        let live = self
            .entries
            .iter()
            .any(|notice| notice.id == id && self.toast_live(notice, now));
        if !live {
            return false;
        }
        self.ended.insert(id);
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// Return the retained notices, oldest first.
    pub fn entries(&self) -> &[Notice] {
        &self.entries
    }

    /// Return the toasts that show at `now`, oldest first.
    pub fn live_toasts(&self, now: Instant) -> Vec<&Notice> {
        self.entries
            .iter()
            .filter(|notice| self.toast_live(notice, now))
            .collect()
    }

    /// Return whether `notice` shows at `now`, as a toast in its time.
    pub fn is_live(&self, notice: &Notice, now: Instant) -> bool {
        self.toast_live(notice, now)
    }

    /// Return when the next live toast ends, if one shows at `now`.
    pub fn next_toast_end(&self, now: Instant) -> Option<Instant> {
        self.live_toasts(now)
            .iter()
            .map(|notice| notice.ends())
            .min()
    }

    /// Return the number of changes so far.
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Return whether `notice` is a toast that shows at `now`.
    fn toast_live(&self, notice: &Notice, now: Instant) -> bool {
        !self.ended.contains(&notice.id) && notice.at <= now && now < notice.ends()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::id::NodeArena;

    /// A log, and a clock time to post at.
    fn log() -> (NoticeLog, Instant) {
        (NoticeLog::default(), Instant::now())
    }

    #[test]
    fn the_ring_keeps_the_newest_notices() {
        let (mut log, now) = log();
        for index in 0..NOTICE_LIMIT + 3 {
            log.post(NoticePost::info(index.to_string()), None, now);
        }
        assert_eq!(log.entries().len(), NOTICE_LIMIT);
        assert_eq!(log.entries()[0].message, "3", "the oldest drop first");
    }

    #[test]
    fn a_repeat_joins_the_newest_entry_and_restarts_its_time() {
        let (mut log, now) = log();
        let first = log.post(NoticePost::warning("top of repo"), None, now);
        let later = now + Duration::from_millis(1500);
        let again = log.post(NoticePost::warning("top of repo"), None, later);
        assert_eq!(first, again);
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].repeats, 2);
        assert_eq!(log.entries()[0].count_label().as_deref(), Some("×2"));
        let past_first = now + Duration::from_millis(2500);
        assert_eq!(
            log.live_toasts(past_first).len(),
            1,
            "the repeat restarts the time"
        );
        log.post(NoticePost::info("top of repo"), None, later);
        assert_eq!(log.entries().len(), 2, "another level is another entry");
    }

    #[test]
    fn identical_failures_from_two_places_stay_apart() {
        let (mut log, now) = log();
        let error = Error::App("disk full".into());
        let mut arena = NodeArena::new();
        let (one, two) = (arena.insert(()), arena.insert(()));
        log.record_failure(&error, NoticeSource::Binding, Some(one), now);
        log.record_failure(&error, NoticeSource::Binding, Some(two), now);
        log.record_failure(&error, NoticeSource::Poll, Some(two), now);
        assert_eq!(log.entries().len(), 3, "node and source keep them apart");
        log.record_failure(&error, NoticeSource::Poll, Some(two), now);
        assert_eq!(log.entries().len(), 3);
        assert_eq!(log.entries()[2].repeats, 2, "the same failure repeats");
    }

    #[test]
    fn every_level_is_a_toast_and_worse_news_shows_longer() {
        let (mut log, now) = log();
        log.post(NoticePost::error("broken"), None, now);
        log.post(NoticePost::warning("refused"), None, now);
        log.post(NoticePost::info("saved"), None, now);
        let live = |log: &NoticeLog, at: Duration| {
            log.live_toasts(now + at)
                .into_iter()
                .map(|notice| notice.message.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(live(&log, Duration::ZERO), ["broken", "refused", "saved"]);
        assert_eq!(live(&log, Duration::from_secs(2)), ["broken", "refused"]);
        assert_eq!(live(&log, Duration::from_secs(4)), ["broken"]);
        assert!(
            live(&log, Duration::from_secs(6)).is_empty(),
            "an error ends too"
        );
        let error = NoticePost::error("slow").with_duration(Duration::from_secs(1));
        assert_eq!(
            error.duration(),
            Duration::from_secs(1),
            "a post names its own"
        );
    }

    #[test]
    fn toasts_end_by_their_own_time_and_the_limit_ends_the_oldest() {
        let (mut log, now) = log();
        let long = NoticePost::info("long").with_duration(Duration::from_secs(10));
        log.post(long, None, now);
        log.post(NoticePost::info("short"), None, now);
        let later = now + Duration::from_secs(3);
        let live = log
            .live_toasts(later)
            .into_iter()
            .map(|notice| notice.message.as_str())
            .collect::<Vec<_>>();
        assert_eq!(live, ["long"], "each toast keeps its own time");
        assert_eq!(
            log.next_toast_end(now),
            Some(now + NoticeLevel::Info.default_duration())
        );

        for index in 0..TOAST_LIMIT {
            log.post(NoticePost::info(index.to_string()), None, later);
        }
        let live = log
            .live_toasts(later)
            .into_iter()
            .map(|notice| notice.message.as_str())
            .collect::<Vec<_>>();
        assert_eq!(live, ["0", "1", "2", "3"], "the limit ends the oldest");
        assert_eq!(log.entries().len(), TOAST_LIMIT + 2, "the log keeps it");
    }

    #[test]
    fn a_click_ends_one_toast() {
        let (mut log, now) = log();
        let first = log.post(NoticePost::info("one"), None, now);
        log.post(NoticePost::info("two"), None, now);
        assert!(log.end_toast(first, now));
        assert!(!log.end_toast(first, now), "an ended toast stays ended");
        assert_eq!(log.live_toasts(now).len(), 1);
    }

    #[test]
    fn an_execution_failure_reports_its_own_message() {
        let (mut log, now) = log();
        let error = Error::Command(CommandError::execution(Error::App("disk full".into())));
        log.record_failure(&error, NoticeSource::Binding, None, now);
        let notice = &log.entries()[0];
        assert_eq!(notice.message, "disk full");
        assert_eq!(notice.level, NoticeLevel::Error);
        assert_eq!(
            notice.failure.map(|failure| failure.kind),
            Some(ScriptErrorKind::CommandExecution)
        );
    }
}
