//! Exact, append-only accepted source. Snapshots keep accepted revisions
//! immutable.
use std::{
    error::Error as StdError,
    fmt,
    mem::size_of,
    ops::Range,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::{DocumentInput, FinishOutcome};

/// Process-wide source epoch allocator.
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);
/// Default maximum accepted source size, in bytes.
pub const DEFAULT_SOURCE_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Whether a producer can append to the current source epoch.
pub enum Lifecycle {
    /// A source epoch that still accepts appends.
    Receiving,
    /// A complete static document or closed source epoch.
    Complete(FinishOutcome),
}
impl Lifecycle {
    /// Return whether appending is closed for this epoch.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// An append or replacement rejected without accepted-state mutation.
pub enum StoreError {
    /// A complete static document or closed source epoch.
    Complete,
    /// The operation would exceed the accepted source byte limit.
    Limit {
        /// Configured maximum accepted source bytes.
        limit: usize,
        /// Total source bytes the rejected operation would have retained.
        attempted: usize,
    },
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Complete => f.write_str("the source is complete"),
            Self::Limit { limit, attempted } => {
                write!(f, "source size {attempted} exceeds the {limit} byte limit")
            }
        }
    }
}
impl StdError for StoreError {}

/// One immutable accepted append linked to the previous revision.
struct Chunk {
    /// The exact accepted UTF-8 append.
    text: Arc<str>,
    /// The immutable prior append chain.
    previous: Option<Arc<Self>>,
}
// Reclaim long, exclusively held histories without recursive Arc destruction.
impl Drop for Chunk {
    fn drop(&mut self) {
        let mut previous = self.previous.take();
        while let Some(chunk) = previous {
            match Arc::try_unwrap(chunk) {
                Ok(mut chunk) => previous = chunk.previous.take(),
                Err(_) => break,
            }
        }
    }
}

#[derive(Clone)]
/// An immutable accepted revision with shared append chunks.
pub struct SourceSnapshot {
    /// Source identity; replacement allocates a new epoch.
    pub epoch: u64,
    /// Monotonic accepted revision within this source epoch.
    pub revision: u64,
    /// The producer lifecycle attached to this exact revision.
    pub lifecycle: Lifecycle,
    /// Newest accepted chunk in the immutable chain.
    chunks: Option<Arc<Chunk>>,
    /// Number of nonempty accepted appends.
    chunk_count: usize,
    /// Exact accepted source byte count.
    len: usize,
    /// Revision-local flattened source cache.
    flattened: Arc<OnceLock<Arc<str>>>,
    /// Shared monotonic identity allocator for this epoch.
    ids: Arc<AtomicU64>,
}
impl fmt::Debug for SourceSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceSnapshot")
            .field("epoch", &self.epoch)
            .field("revision", &self.revision)
            .field("lifecycle", &self.lifecycle)
            .field("len", &self.len)
            .field("chunks", &self.chunk_count)
            .finish()
    }
}
impl SourceSnapshot {
    /// Flatten once and share the exact accepted UTF-8 bytes.
    pub fn text(&self) -> Arc<str> {
        self.flattened
            .get_or_init(|| {
                if let Some(chunk) = &self.chunks
                    && chunk.previous.is_none()
                {
                    return chunk.text.clone();
                }
                let mut pieces = Vec::with_capacity(self.chunk_count);
                let mut next = self.chunks.as_deref();
                while let Some(chunk) = next {
                    pieces.push(&*chunk.text);
                    next = chunk.previous.as_deref();
                }
                let mut out = String::with_capacity(self.len);
                for chunk in pieces.into_iter().rev() {
                    out.push_str(chunk);
                }
                out.into()
            })
            .clone()
    }
    /// Return the exact accepted byte count.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Return whether no bytes have been accepted.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Copy an exact source range, rejecting invalid UTF-8 boundaries.
    pub fn slice(&self, range: Range<usize>) -> Option<String> {
        self.text().get(range).map(str::to_owned)
    }
    /// Bytes in a separately allocated contiguous source cache, without
    /// creating that cache. Single-chunk text shares its accepted allocation.
    pub fn flattened_bytes(&self) -> usize {
        if self.chunk_count > 1 {
            self.flattened.get().map_or(0, |text| text.len())
        } else {
            0
        }
    }

    /// Return the number of nonempty accepted append chunks.
    pub fn chunk_count(&self) -> usize {
        self.chunk_count
    }
    /// Estimate retained payloads and Arc headers in constant time. Shared
    /// allocations are charged in full; allocator bookkeeping and rounding
    /// are excluded. The snapshot value itself belongs to its containing model.
    pub(crate) fn retained_bytes(&self) -> usize {
        let arc_header = 2 * size_of::<usize>();
        let chunk_headers = size_of::<Chunk>() + 2 * arc_header;
        let shared_state =
            size_of::<OnceLock<Arc<str>>>() + size_of::<AtomicU64>() + 2 * arc_header;
        let flattened = if self.chunk_count != 1 {
            self.flattened
                .get()
                .map_or(0, |text| text.len().saturating_add(arc_header))
        } else {
            // The one-chunk cache shares the chunk's text allocation.
            0
        };
        self.len
            .saturating_add(self.chunk_count.saturating_mul(chunk_headers))
            .saturating_add(shared_state)
            .saturating_add(flattened)
    }
    /// Allocate an opaque identity that cannot be reused in this epoch.
    pub(crate) fn allocate_id(&self) -> u64 {
        self.ids.fetch_add(1, Ordering::Relaxed)
    }
}

#[derive(Debug)]
/// The unique append authority for a source epoch.
pub struct SourceStore {
    /// The sole mutable accepted-source authority.
    snapshot: SourceSnapshot,
    /// Maximum bytes accepted in the current or replacement epoch.
    limit: usize,
}
impl SourceStore {
    /// Start an empty receiving epoch with the default source limit.
    pub fn receiving() -> Self {
        Self::receiving_with_limit(DEFAULT_SOURCE_LIMIT)
    }
    /// Start an empty receiving epoch with an explicit byte limit.
    pub fn receiving_with_limit(limit: usize) -> Self {
        Self {
            snapshot: SourceSnapshot {
                epoch: NEXT_EPOCH.fetch_add(1, Ordering::Relaxed),
                revision: 0,
                lifecycle: Lifecycle::Receiving,
                chunks: None,
                chunk_count: 0,
                len: 0,
                flattened: Arc::new(OnceLock::new()),
                ids: Arc::new(AtomicU64::new(1)),
            },
            limit,
        }
    }
    /// Adopt complete source with the default byte limit.
    pub fn complete(text: impl Into<Arc<str>>) -> Result<Self, StoreError> {
        Self::complete_with_limit(text, DEFAULT_SOURCE_LIMIT)
    }
    /// Adopt complete source without copying an existing Arc payload.
    pub fn complete_with_limit(
        text: impl Into<Arc<str>>,
        limit: usize,
    ) -> Result<Self, StoreError> {
        let text = text.into();
        if text.len() > limit {
            return Err(StoreError::Limit {
                limit,
                attempted: text.len(),
            });
        }
        let mut store = Self::receiving_with_limit(limit);
        if !text.is_empty() {
            store.snapshot.len = text.len();
            store.snapshot.chunk_count = 1;
            store.snapshot.revision = 1;
            store.snapshot.chunks = Some(Arc::new(Chunk {
                text,
                previous: None,
            }));
        }
        store.finish(FinishOutcome::Completed);
        Ok(store)
    }
    /// Capture the current immutable accepted revision.
    pub fn snapshot(&self) -> SourceSnapshot {
        self.snapshot.clone()
    }
    /// Accept all nonempty UTF-8 input or leave accepted state unchanged.
    pub fn append(&mut self, text: &str) -> Result<bool, StoreError> {
        if self.snapshot.lifecycle.is_complete() {
            return Err(StoreError::Complete);
        }
        if text.is_empty() {
            return Ok(false);
        }
        let attempted = self.snapshot.len.saturating_add(text.len());
        if attempted > self.limit {
            return Err(StoreError::Limit {
                limit: self.limit,
                attempted,
            });
        }
        self.snapshot.chunks = Some(Arc::new(Chunk {
            text: Arc::from(text),
            previous: self.snapshot.chunks.clone(),
        }));
        self.snapshot.chunk_count += 1;
        self.snapshot.len = attempted;
        self.snapshot.revision += 1;
        self.snapshot.flattened = Arc::new(OnceLock::new());
        Ok(true)
    }
    /// Complete the epoch once, advancing revision without adding bytes.
    pub fn finish(&mut self, outcome: FinishOutcome) -> bool {
        if self.snapshot.lifecycle.is_complete() {
            return false;
        }
        self.snapshot.lifecycle = Lifecycle::Complete(outcome);
        self.snapshot.revision += 1;
        true
    }
    /// Start a new epoch atomically, preserving the old one on rejection.
    pub fn replace(&mut self, input: DocumentInput) -> Result<(), StoreError> {
        let mut next = Self::receiving_with_limit(self.limit);
        match input {
            DocumentInput::Complete(text) => {
                next.append(&text)?;
                next.finish(FinishOutcome::Completed);
            }
            DocumentInput::Receiving(text) => {
                next.append(&text)?;
            }
        }
        *self = next;
        Ok(())
    }
}
impl Default for SourceStore {
    fn default() -> Self {
        Self::receiving()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_immutable_all_or_nothing() {
        let mut s = SourceStore::receiving_with_limit(8);
        s.append("é\r").unwrap();
        let old = s.snapshot();
        s.append("\n\t終").unwrap();
        assert_eq!(&*s.snapshot().text(), "é\r\n\t終");
        let revision = s.snapshot().revision;
        assert!(s.append("!").is_err());
        assert_eq!(s.snapshot().revision, revision);
        assert_eq!(&*old.text(), "é\r");
        assert!(!s.append("").unwrap());
        assert!(s.finish(FinishOutcome::Aborted("offline".into())));
        assert!(!s.finish(FinishOutcome::Completed));
        assert!(s.append("").is_err());
        assert_eq!(s.snapshot().slice(1..2), None);
    }
    #[test]
    fn replacement_changes_epoch_and_preserves_old_bytes() {
        let mut s = SourceStore::complete("old").unwrap();
        let old = s.snapshot();
        s.replace(DocumentInput::Receiving("新".into())).unwrap();
        assert_ne!(old.epoch, s.snapshot().epoch);
        assert_eq!(&*old.text(), "old");
        assert_eq!(s.snapshot().lifecycle, Lifecycle::Receiving);
        s.replace(DocumentInput::Complete("終".into())).unwrap();
        assert_eq!(&*s.snapshot().text(), "終");
        assert!(s.snapshot().lifecycle.is_complete());
    }
    #[test]
    fn many_chunks_flatten_and_reclaim_iteratively() {
        let mut source = SourceStore::receiving_with_limit(50_000);
        for _ in 0..50_000 {
            source.append("x").unwrap();
        }
        let snapshot = source.snapshot();
        assert_eq!(snapshot.chunk_count(), 50_000);
        assert_eq!(snapshot.text().len(), 50_000);
        drop(source);
        assert_eq!(snapshot.text().len(), 50_000);
        drop(snapshot);
    }
}
