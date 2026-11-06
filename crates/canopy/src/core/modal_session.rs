//! An owner's record of the one modal it has open.
//!
//! | State | Event | Next state |
//! |---|---|---|
//! | `Closed` | `opened(token, kind)` | `Open` |
//! | `Open` | `close` succeeds | `Closing` |
//! | `Open` | `closed_by_widget` | `Closing` |
//! | `Open`, `Closing` | `reconcile`, scope gone | `Closed` |
//!
//! A close is queued, and Canopy reports the modal open until the scope is
//! gone, so a closed modal passes through `Closing`. Reconcile on command
//! entry and in queries, not only in a poll: a modal can close by a route
//! other than the owner's own commands, such as the close of a modal below
//! it.
//!
//! An owner opens one modal at a time. [`ModalSession::can_open`] is false
//! while one is open or closing, and the owner refuses the request with a
//! notice. A confirmation nested inside the open modal belongs to that
//! session and is not a second modal.

use std::mem;

use crate::{Context, ViewContext, error::Result, input::ModalToken};

/// An owner's record of its modal, of kind `K`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ModalSession<K> {
    /// No modal is open.
    #[default]
    Closed,
    /// A modal is open.
    Open {
        /// The modal's scope.
        token: ModalToken,
        /// What the modal is for.
        kind: K,
    },
    /// A close was queued, and the scope is not gone yet.
    Closing {
        /// The modal's scope.
        token: ModalToken,
    },
}

impl<K> ModalSession<K> {
    /// Return to `Closed` when the modal's scope is gone.
    pub fn reconcile(&mut self, ctx: &(impl ViewContext + ?Sized)) {
        let token = match self {
            Self::Closed => return,
            Self::Open { token, .. } | Self::Closing { token } => *token,
        };
        if !ctx.modal_is_open(token) {
            *self = Self::Closed;
        }
    }

    /// Reconcile, then return whether a new modal may open.
    pub fn can_open(&mut self, ctx: &(impl ViewContext + ?Sized)) -> bool {
        self.reconcile(ctx);
        matches!(self, Self::Closed)
    }

    /// Record a modal that opened.
    pub fn opened(&mut self, token: ModalToken, kind: K) {
        *self = Self::Open { token, kind };
    }

    /// Record that the modal's widget closed it, as a dialog does on an
    /// answer. Return the modal's kind.
    pub fn closed_by_widget(&mut self) -> Option<K> {
        match mem::replace(self, Self::Closed) {
            Self::Open { token, kind } => {
                *self = Self::Closing { token };
                Some(kind)
            }
            other => {
                *self = other;
                None
            }
        }
    }

    /// The open modal's token and kind.
    pub fn open(&self) -> Option<(ModalToken, &K)> {
        match self {
            Self::Open { token, kind } => Some((*token, kind)),
            Self::Closed | Self::Closing { .. } => None,
        }
    }

    /// The open modal's kind.
    pub fn kind(&self) -> Option<&K> {
        self.open().map(|(_, kind)| kind)
    }

    /// The token of a modal that is open or closing.
    pub fn token(&self) -> Option<ModalToken> {
        match self {
            Self::Open { token, .. } | Self::Closing { token } => Some(*token),
            Self::Closed => None,
        }
    }

    /// Whether a modal is open, and not closing.
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open { .. })
    }

    /// Queue the close of the open modal. On success the session is
    /// `Closing`, and the kind is returned; on failure it stays `Open`.
    pub fn close(&mut self, ctx: &mut (impl Context + ?Sized)) -> Result<Option<K>> {
        let Self::Open { token, .. } = self else {
            return Ok(None);
        };
        let token = *token;
        ctx.close_modal(token)?;
        Ok(self.closed_by_widget())
    }
}
