//! Bounded, provider-ordered list state without I/O or a rendering runtime.
//!
//! A provider executes requests from [`Paged::take_request`] and returns a page
//! with the same token. Cursors describe page boundaries, independently of item
//! keys. Pages contain unique keys, preserve the provider's order, and contain
//! at most the request's limit. Adjacent pages must not overlap resident keys.
//! The controller never compares item values or sorts pages.
//!
//! Whole pages are evicted to preserve opaque boundary cursors. Resident items
//! never exceed capacity. One selected item remains pinned outside that budget,
//! even when independent viewport scrolling evicts its page. Providers that
//! support key seeking can restore that item before keyboard navigation.

use std::{
    collections::VecDeque,
    sync::atomic::{AtomicU64, Ordering},
};

/// Process-wide controller identity source, preventing foreign-token
/// acceptance.
static NEXT_OWNER: AtomicU64 = AtomicU64::new(0);

/// Optional provider operations. Loading the first page is always available.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// The provider can load the last page without enumerating earlier pages.
    pub last: bool,
    /// The provider can load a page containing a stable item key.
    pub seek: bool,
}

/// Direction through the provider's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Items preceding the resident window.
    Before,
    /// Items following the resident window.
    After,
}

/// An operation for the external provider. Cursors and keys are separate types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seek<K, C> {
    /// Load the start of the provider's order.
    First,
    /// Load the end of the provider's order.
    Last,
    /// Load a page containing this key, if the key still exists.
    Around(K),
    /// Load the page preceding this opaque boundary.
    Before(C),
    /// Load the page following this opaque boundary.
    After(C),
}

/// Opaque identity of one request in one controller generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// Controller identity that remains fixed across query resets.
    owner: u64,
    /// Query generation, incremented by reset.
    generation: u64,
    /// Monotonic request serial, including retries and canceled work.
    serial: u64,
}

/// Work to execute outside the controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request<K, C> {
    /// Return this token with either a page or a failure.
    pub token: Token,
    /// Provider operation, using opaque cursors or a stable key.
    pub seek: Seek<K, C>,
    /// Maximum number of items in the reply.
    pub limit: usize,
}

/// A contiguous slice in provider order, with cursors at its exact boundaries.
#[derive(Clone, Debug)]
pub struct Page<T, C> {
    /// Items in provider order. Do not sort them in the consumer.
    pub items: Vec<T>,
    /// Boundary for loading the preceding page, or `None` at the start.
    pub before: Option<C>,
    /// Boundary for loading the following page, or `None` at the end.
    pub after: Option<C>,
    /// Exact item count if known. `None` means unknown, not zero.
    pub total: Option<u64>,
}

/// Current provider request state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// No request is outstanding.
    #[default]
    Idle,
    /// A request is queued or executing.
    Loading,
    /// The last request failed. Only an explicit retry or command starts work.
    Failed,
    /// An empty page has a continuation. Automatic navigation has paused.
    Stalled,
}

/// A reply violated the bounded, unique-key page contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageError {
    /// A page contained more items than its request permitted.
    TooLarge,
    /// A key was repeated within the page or overlapped an adjacent page.
    DuplicateKey,
}

/// How the renderer should position the resident row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollKind {
    /// Preserve the top visible item's identity after resident indices change.
    Anchor,
    /// Reveal a selection made by the latest navigation command.
    Reveal,
}

/// Latest scroll effect. The renderer owns actual viewport geometry and scroll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrollIntent<K> {
    /// Stable identity of the resident item.
    pub key: K,
    /// Current resident index of the item.
    pub index: usize,
    /// Whether to anchor the viewport or reveal selection.
    pub kind: ScrollKind,
    /// User-action revision. Discard an effect superseded by a newer action.
    pub revision: u64,
}

/// Selection policy to apply when one page publishes.
#[derive(Clone)]
enum Selection<K> {
    /// Keep the pinned identity unless a complete result proves its removal.
    Preserve,
    /// Select the page's first item.
    First,
    /// Select the page's last item.
    Last,
    /// Select the requested key, falling back to the first returned item.
    Around(K),
}

/// Provider operation and its independent selection policy.
#[derive(Clone)]
struct Work<K, C> {
    /// Operation passed to the provider.
    seek: Seek<K, C>,
    /// Selection policy used upon publication.
    selection: Selection<K>,
}

/// Single queued or executing request whose token can publish.
struct Active<K, C> {
    /// Identity required by replies and failures.
    token: Token,
    /// Work retained for validation, publication, and explicit retry.
    work: Work<K, C>,
    /// Whether the external executor has already taken this request.
    dispatched: bool,
}

/// Exact cursors for one nonempty resident page, retained through eviction.
struct Boundary<C> {
    /// Number of resident items belonging to this page.
    len: usize,
    /// Cursor preceding this page's first item.
    before: Option<C>,
    /// Cursor following this page's last item.
    after: Option<C>,
}

/// Pure paging controller for a list with a bounded resident window.
pub struct Paged<T, K, C> {
    /// Maximum retained items after publication, excluding the pinned
    /// selection.
    capacity: usize,
    /// Maximum items requested or accepted in one page.
    page_size: usize,
    /// Key extractor called once per incoming item, before page acceptance.
    key: fn(&T) -> K,
    /// Provider operations available without scanning other pages.
    capabilities: Capabilities,
    /// Fixed controller identity included in request tokens.
    owner: u64,
    /// Current query generation, rejecting replies after reset.
    generation: u64,
    /// Monotonic serial distinguishing replacement requests and retries.
    serial: u64,
    /// Latest user action, shared by scroll effects and acknowledgments.
    revision: u64,
    /// Resident items in provider order.
    items: Vec<T>,
    /// Cached stable keys in the same order and length as resident items.
    keys: Vec<K>,
    /// Nonempty resident page boundaries in provider order.
    boundaries: VecDeque<Boundary<C>>,
    /// Cursor at the resident window's start, or the latest empty-page
    /// boundary.
    before: Option<C>,
    /// Cursor at the resident window's end, or the latest empty-page boundary.
    after: Option<C>,
    /// Latest provider-reported exact total, independently of resident length.
    total: Option<u64>,
    /// Selected item retained even when its resident page is evicted.
    selected: Option<T>,
    /// Cached stable key of the pinned selection.
    selected_key: Option<K>,
    /// Latest observed viewport anchor, independent of selection.
    viewport: Option<K>,
    /// Rebased resident index used when the viewport anchor is evicted.
    viewport_index: usize,
    /// Unfulfilled signed keyboard movement across page edges.
    pending_steps: i64,
    /// Single request allowed to publish, whether queued or dispatched.
    active: Option<Active<K, C>>,
    /// Failed work or an empty-page continuation awaiting explicit retry.
    retry: Option<Work<K, C>>,
    /// Current loading, idle, failure, or empty-continuation state.
    status: Status,
    /// Whether the latest navigation still takes priority over viewport
    /// anchoring.
    reveal: bool,
    /// Key of the latest taken reveal, checked before acknowledging its layout.
    revealed_key: Option<K>,
    /// Latest unconsumed scroll effect, superseding earlier effects.
    scroll: Option<ScrollIntent<K>>,
}

impl<T: Clone, K: Clone + Eq, C: Clone> Paged<T, K, C> {
    /// Create an idle controller. Call [`Self::first`] to request initial
    /// items.
    ///
    /// Capacity and page size must be positive, and page size must fit
    /// capacity. The key function must return a stable identity for each
    /// provider item.
    pub fn new(
        capacity: usize,
        page_size: usize,
        key: fn(&T) -> K,
        capabilities: Capabilities,
    ) -> Self {
        assert!(
            page_size > 0 && capacity >= page_size,
            "page size must fit positive capacity"
        );
        let owner = NEXT_OWNER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("paged controller identities exhausted");
        Self {
            capacity,
            page_size,
            key,
            capabilities,
            owner,
            generation: 0,
            serial: 0,
            revision: 0,
            items: Vec::new(),
            keys: Vec::new(),
            boundaries: VecDeque::new(),
            before: None,
            after: None,
            total: None,
            selected: None,
            selected_key: None,
            viewport: None,
            viewport_index: 0,
            pending_steps: 0,
            active: None,
            retry: None,
            status: Status::Idle,
            reveal: false,
            revealed_key: None,
            scroll: None,
        }
    }

    /// Resident items, in exact provider order.
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Selected item, including a pinned item whose page has been evicted.
    pub fn selected(&self) -> Option<&T> {
        self.selected.as_ref()
    }

    /// Stable identity of the selection, including an evicted selection.
    pub fn selected_key(&self) -> Option<K> {
        self.selected_key.clone()
    }

    /// Resident selection index, or `None` when the selected item is evicted.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected_key
            .as_ref()
            .and_then(|key| self.index_of(key))
    }

    /// Whether preceding items can be requested.
    pub fn has_before(&self) -> bool {
        self.before.is_some()
    }

    /// Whether following items can be requested.
    pub fn has_after(&self) -> bool {
        self.after.is_some()
    }

    /// Provider-reported exact total, or `None` when unknown.
    pub fn total(&self) -> Option<u64> {
        self.total
    }

    /// Current request state. Taking a request does not end `Loading`.
    pub fn status(&self) -> Status {
        self.status
    }

    /// Signed keyboard movement waiting for pages.
    pub fn pending_steps(&self) -> i64 {
        self.pending_steps
    }

    /// Latest user-action revision, also carried by scroll effects.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Take queued work once. Replies remain valid until canceled or reset.
    pub fn take_request(&mut self) -> Option<Request<K, C>> {
        let active = self.active.as_mut()?;
        if active.dispatched {
            return None;
        }
        active.dispatched = true;
        Some(Request {
            token: active.token,
            seek: active.work.seek.clone(),
            limit: self.page_size,
        })
    }

    /// Take the latest scroll effect, if its item remains resident.
    pub fn take_scroll_intent(&mut self) -> Option<ScrollIntent<K>> {
        let intent = self.scroll.take()?;
        if intent.kind == ScrollKind::Reveal {
            self.revealed_key = Some(intent.key.clone());
        }
        Some(intent)
    }

    /// Acknowledge a taken reveal after the renderer has applied it in layout.
    /// Observe the actual viewport with `anchor` first. Pending movement or a
    /// newer selection keeps reveal priority until its own layout completes.
    pub fn scroll_applied(&mut self, revision: u64) -> bool {
        if revision != self.revision
            || self.pending_steps != 0
            || self.revealed_key.is_none()
            || self.revealed_key != self.selected_key
        {
            return false;
        }
        self.reveal = false;
        self.revealed_key = None;
        if self
            .scroll
            .as_ref()
            .is_some_and(|intent| intent.kind == ScrollKind::Reveal)
        {
            self.scroll = None;
        }
        true
    }

    /// Forget the old query and reject every outstanding reply.
    pub fn reset(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("paged generations exhausted");
        self.user_action(false);
        self.cancel();
        self.items.clear();
        self.keys.clear();
        self.boundaries.clear();
        self.before = None;
        self.after = None;
        self.total = None;
        self.selected = None;
        self.selected_key = None;
        self.viewport = None;
        self.viewport_index = 0;
    }

    /// Request and select the first item, superseding previous work.
    pub fn first(&mut self) {
        self.replace(Seek::First, Selection::First, true);
    }

    /// Request and select the last item, if the provider supports it.
    pub fn last(&mut self) -> bool {
        if !self.capabilities.last {
            return false;
        }
        self.replace(Seek::Last, Selection::Last, true);
        true
    }

    /// Request a key's page and select that key, if the provider supports it.
    /// If the key no longer exists, select the returned page's first item.
    pub fn seek(&mut self, key: K) -> bool {
        if !self.capabilities.seek {
            return false;
        }
        self.replace(Seek::Around(key.clone()), Selection::Around(key), true);
        true
    }

    /// Reload the visible neighborhood while preserving selection and viewport.
    /// Returns false when a request is active or key seeking is required but
    /// unsupported.
    pub fn refresh(&mut self) -> bool {
        if self.active.is_some() {
            return false;
        }
        let seek = if self.before.is_none() {
            Seek::First
        } else if self.capabilities.seek {
            let Some(key) = self.viewport.clone().or_else(|| self.selected_key()) else {
                return false;
            };
            Seek::Around(key)
        } else {
            return false;
        };
        self.issue(Work {
            seek,
            selection: Selection::Preserve,
        });
        true
    }

    /// Select a resident item. Explicit selection supersedes queued navigation.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.items.len() {
            return false;
        }
        self.user_action(true);
        self.cancel();
        self.pending_steps = 0;
        self.pin(Some(index));
        self.emit_scroll();
        true
    }

    /// Observe renderer scroll without treating layout or reveals as user
    /// input.
    pub fn anchor(&mut self, index: usize) {
        self.viewport_index = index.min(self.items.len().saturating_sub(1));
        self.viewport = self.keys.get(self.viewport_index).cloned();
    }

    /// Observe user viewport movement, superseding pending keyboard navigation.
    /// An adjacent page may still publish, anchored to this newest viewport.
    pub fn set_viewport(&mut self, index: usize) {
        self.user_action(false);
        self.pending_steps = 0;
        self.anchor(index);
        if self
            .active
            .as_ref()
            .is_some_and(|active| !matches!(active.work.selection, Selection::Preserve))
        {
            self.cancel();
        }
    }

    /// Queue signed movement in provider order and consume resident movement
    /// now. Opposite input discards unfulfilled movement in the old
    /// direction. Returns false when an evicted selection requires
    /// unsupported key seeking.
    pub fn navigate(&mut self, delta: i64) -> bool {
        if delta == 0 {
            return true;
        }
        if self.selected.is_some() && self.selected_index().is_none() && !self.capabilities.seek {
            return false;
        }
        self.user_action(true);
        if self.pending_steps.signum() != 0 && self.pending_steps.signum() != delta.signum() {
            self.pending_steps = 0;
        }
        if self.active.as_ref().is_some_and(|active| {
            (self.selected_index().is_some()
                && !matches!(active.work.selection, Selection::Preserve))
                || match active.work.seek {
                    Seek::After(_) => delta < 0,
                    Seek::Before(_) => delta > 0,
                    _ => false,
                }
        }) {
            self.cancel();
        }
        self.retry = None;
        if self.active.is_none() {
            self.status = Status::Idle;
        }
        self.pending_steps = self.pending_steps.saturating_add(delta);
        self.advance();
        self.emit_scroll();
        true
    }

    /// Load an adjacent page without moving selection. A reversed direction
    /// cancels the old adjacent request. Call `set_viewport` for actual user
    /// scrolling.
    pub fn load(&mut self, direction: Direction) -> bool {
        if self.active.as_ref().is_some_and(|active| {
            matches!(
                (&active.work.seek, direction),
                (Seek::After(_), Direction::Before) | (Seek::Before(_), Direction::After)
            )
        }) {
            self.cancel();
        }
        if self.active.is_some() {
            return false;
        }
        let Some(seek) = self.adjacent(direction) else {
            return false;
        };
        self.issue(Work {
            seek,
            selection: Selection::Preserve,
        });
        true
    }

    /// Explicitly retry a failed request or continue past an empty page.
    pub fn retry(&mut self) -> bool {
        let Some(work) = self.retry.take() else {
            return false;
        };
        if self.active.is_some() {
            return false;
        }
        self.issue(work);
        true
    }

    /// Record a provider failure without changing visible items or selection.
    /// The caller owns the error message. Stale failures return false.
    pub fn fail(&mut self, token: Token) -> bool {
        if !self.matches(token) {
            return false;
        }
        self.retry = self.active.take().map(|active| active.work);
        self.status = Status::Failed;
        true
    }

    /// Accept a matching reply. Stale replies return `Ok(false)` unchanged.
    /// Invalid pages leave rows and selection intact and require explicit
    /// retry.
    pub fn apply(&mut self, token: Token, page: Page<T, C>) -> Result<bool, PageError> {
        if !self.matches(token) {
            return Ok(false);
        }
        let keys = match self.validate(&page) {
            Ok(keys) => keys,
            Err(error) => {
                self.fail(token);
                return Err(error);
            }
        };
        let active = self.active.take().expect("matching request");
        self.retry = None;
        self.status = Status::Idle;
        let empty = page.items.is_empty();
        self.total = page.total;
        self.install(page, keys, &active.work);
        self.choose(&active.work.selection);
        // Movement sees old and new rows together before whole-page eviction.
        if !empty {
            self.advance_resident();
        }
        self.evict(&active.work.seek);
        self.update_boundaries();
        self.emit_scroll();
        let direction = match active.work.seek {
            Seek::Before(_) | Seek::Last => Direction::Before,
            Seek::Around(_) if self.pending_steps < 0 => Direction::Before,
            _ => Direction::After,
        };
        if empty && self.adjacent(direction).is_some() {
            self.status = Status::Stalled;
            self.retry = self.adjacent(direction).map(|seek| Work {
                seek,
                selection: Selection::Preserve,
            });
        } else {
            if empty {
                self.pending_steps = 0;
            }
            self.advance();
            self.emit_scroll();
        }
        Ok(true)
    }

    /// Locate a stable identity without invoking the key extractor again.
    fn index_of(&self, key: &K) -> Option<usize> {
        self.keys.iter().position(|candidate| candidate == key)
    }

    /// Supersede old scroll effects and choose navigation or viewport priority.
    fn user_action(&mut self, reveal: bool) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("paged action revisions exhausted");
        self.reveal = reveal;
        self.revealed_key = None;
        self.scroll = None;
    }

    /// Invalidate active work and retries without dropping visible rows.
    fn cancel(&mut self) {
        self.active = None;
        self.retry = None;
        self.status = Status::Idle;
    }

    /// Supersede navigation with a page replacement and selection policy.
    fn replace(&mut self, seek: Seek<K, C>, selection: Selection<K>, reveal: bool) {
        self.user_action(reveal);
        self.pending_steps = 0;
        self.cancel();
        self.issue(Work { seek, selection });
    }

    /// Queue one operation under a fresh token.
    fn issue(&mut self, work: Work<K, C>) {
        self.serial = self
            .serial
            .checked_add(1)
            .expect("paged request identities exhausted");
        let token = Token {
            owner: self.owner,
            generation: self.generation,
            serial: self.serial,
        };
        self.active = Some(Active {
            token,
            work,
            dispatched: false,
        });
        self.retry = None;
        self.status = Status::Loading;
    }

    /// Check whether a reply still belongs to the active request.
    fn matches(&self, token: Token) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.token == token)
    }

    /// Build an adjacent operation from the resident boundary's opaque cursor.
    fn adjacent(&self, direction: Direction) -> Option<Seek<K, C>> {
        match direction {
            Direction::Before => self.before.clone().map(Seek::Before),
            Direction::After => self.after.clone().map(Seek::After),
        }
    }

    /// Consume movement within resident rows and clamp at known provider ends.
    fn advance_resident(&mut self) {
        let Some(index) = self.selected_index() else {
            return;
        };
        let last = self.items.len() - 1;
        let desired = if self.pending_steps < 0 {
            index.saturating_sub(
                usize::try_from(self.pending_steps.unsigned_abs()).unwrap_or(usize::MAX),
            )
        } else {
            index
                .saturating_add(usize::try_from(self.pending_steps).unwrap_or(usize::MAX))
                .min(last)
        };
        let moved = i64::try_from(desired).expect("resident index fits i64")
            - i64::try_from(index).expect("resident index fits i64");
        self.pending_steps = self.pending_steps.saturating_sub(moved);
        self.pin(Some(desired));
        if (desired == 0 && self.before.is_none()) || (desired == last && self.after.is_none()) {
            // Only clamp movement toward the unavailable side.
            if (self.pending_steps < 0 && desired == 0)
                || (self.pending_steps > 0 && desired == last)
            {
                self.pending_steps = 0;
            }
        }
    }

    /// Consume movement or request the next page or evicted selection's page.
    fn advance(&mut self) {
        if self.pending_steps == 0 {
            return;
        }
        if self.items.is_empty() {
            if self.active.is_none() {
                let direction = if self.pending_steps < 0 {
                    Direction::Before
                } else {
                    Direction::After
                };
                let seek = self.adjacent(direction).unwrap_or(Seek::First);
                self.issue(Work {
                    seek,
                    selection: Selection::First,
                });
            }
            return;
        }
        if self.selected.is_none() {
            self.pin(Some(0));
        }
        if self.selected_index().is_none() {
            if self.active.is_none()
                && let Some(key) = self.selected_key()
            {
                self.issue(Work {
                    seek: Seek::Around(key.clone()),
                    selection: Selection::Around(key),
                });
            }
            return;
        }
        self.advance_resident();
        if self.pending_steps != 0 && self.active.is_none() {
            let direction = if self.pending_steps < 0 {
                Direction::Before
            } else {
                Direction::After
            };
            if let Some(seek) = self.adjacent(direction) {
                self.issue(Work {
                    seek,
                    selection: Selection::Preserve,
                });
            } else {
                self.pending_steps = 0;
            }
        }
    }

    /// Validate bounded size and unique keys before mutating visible state.
    fn validate(&self, page: &Page<T, C>) -> Result<Vec<K>, PageError> {
        if page.items.len() > self.page_size {
            return Err(PageError::TooLarge);
        }
        let adjacent = self
            .active
            .as_ref()
            .is_some_and(|active| matches!(active.work.seek, Seek::Before(_) | Seek::After(_)));
        let keys: Vec<K> = page.items.iter().map(self.key).collect();
        for (index, key) in keys.iter().enumerate() {
            if keys[..index].contains(key) || (adjacent && self.index_of(key).is_some()) {
                return Err(PageError::DuplicateKey);
            }
        }
        Ok(keys)
    }

    /// Merge one page without reordering, retaining its exact boundary cursors.
    fn install(&mut self, page: Page<T, C>, keys: Vec<K>, work: &Work<K, C>) {
        let boundary = Boundary {
            len: page.items.len(),
            before: page.before.clone(),
            after: page.after.clone(),
        };
        match work.seek {
            Seek::After(_) => {
                self.after = page.after;
                if !page.items.is_empty() {
                    self.items.extend(page.items);
                    self.keys.extend(keys);
                    self.boundaries.push_back(boundary);
                } else if let Some(last) = self.boundaries.back_mut() {
                    last.after = self.after.clone();
                }
            }
            Seek::Before(_) => {
                self.before = page.before;
                if !page.items.is_empty() {
                    self.viewport_index = self.viewport_index.saturating_add(page.items.len());
                    let mut items = page.items;
                    items.append(&mut self.items);
                    self.items = items;
                    let mut keys = keys;
                    keys.append(&mut self.keys);
                    self.keys = keys;
                    self.boundaries.push_front(boundary);
                } else if let Some(first) = self.boundaries.front_mut() {
                    first.before = self.before.clone();
                }
            }
            _ => {
                self.items = page.items;
                self.keys = keys;
                self.boundaries.clear();
                self.before = page.before;
                self.after = page.after;
                if !self.items.is_empty() {
                    self.boundaries.push_back(boundary);
                }
            }
        }
    }

    /// Resolve selection policy, detecting removal only for a complete result.
    fn choose(&mut self, selection: &Selection<K>) {
        let first = (!self.items.is_empty()).then_some(0);
        let chosen = match selection {
            Selection::First => first,
            Selection::Last => self.items.len().checked_sub(1),
            Selection::Around(key) => self.index_of(key).or(first),
            Selection::Preserve => self.selected_index().or_else(|| {
                if self.selected.is_none() || (self.before.is_none() && self.after.is_none()) {
                    first
                } else {
                    None
                }
            }),
        };
        if chosen.is_some()
            || !matches!(selection, Selection::Preserve)
            || (self.before.is_none() && self.after.is_none())
        {
            self.pin(chosen);
        }
    }

    /// Retain the selected item and its cached identity outside the row window.
    fn pin(&mut self, index: Option<usize>) {
        self.selected = index.map(|index| self.items[index].clone());
        self.selected_key = index.map(|index| self.keys[index].clone());
    }

    /// Evict opposite-end whole pages and rebase the viewport's fallback index.
    fn evict(&mut self, seek: &Seek<K, C>) {
        while self.items.len() > self.capacity {
            if matches!(seek, Seek::Before(_)) {
                let boundary = self.boundaries.pop_back().expect("resident page boundary");
                self.items.truncate(self.items.len() - boundary.len);
                self.keys.truncate(self.keys.len() - boundary.len);
                self.viewport_index = self.viewport_index.min(self.items.len().saturating_sub(1));
            } else {
                let boundary = self.boundaries.pop_front().expect("resident page boundary");
                self.items.drain(..boundary.len);
                self.keys.drain(..boundary.len);
                self.viewport_index = self.viewport_index.saturating_sub(boundary.len);
            }
        }
    }

    /// Restore window cursors from the retained first and last page boundaries.
    fn update_boundaries(&mut self) {
        if let Some(first) = self.boundaries.front() {
            self.before.clone_from(&first.before);
        }
        if let Some(last) = self.boundaries.back() {
            self.after.clone_from(&last.after);
        }
    }

    /// Publish the latest reveal or viewport anchor in current resident
    /// indices.
    fn emit_scroll(&mut self) {
        let candidate = if self.reveal {
            self.selected_index()
                .map(|index| (index, ScrollKind::Reveal))
        } else {
            self.viewport
                .as_ref()
                .and_then(|key| self.index_of(key))
                .or_else(|| {
                    if self.items.is_empty() {
                        None
                    } else {
                        Some(self.viewport_index.min(self.items.len() - 1))
                    }
                })
                .map(|index| (index, ScrollKind::Anchor))
        };
        self.scroll = candidate.map(|(index, kind)| ScrollIntent {
            key: self.keys[index].clone(),
            index,
            kind,
            revision: self.revision,
        });
    }
}

#[cfg(test)]
#[path = "paged/tests.rs"]
mod tests;
