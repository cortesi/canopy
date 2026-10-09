//! Display labels for list-style widgets.

/// An item that renders as one line of text.
pub trait ItemLabel {
    /// Return the display label for this item.
    fn label(&self) -> &str;

    /// Return whether the item shows muted, such as an item that cannot be
    /// used now. A muted item can still be selected.
    fn muted(&self) -> bool {
        false
    }

    /// Return the label as runs of text, each with a style role, or `None`
    /// to show it as one run. The runs join to [`ItemLabel::label`].
    ///
    /// A list paints each run with its role beneath the style of the row:
    /// `text/<role>`, `muted/<role>`, or the selection role and the run role.
    /// A role without a rule of its own takes the style of the row, so a
    /// muted or selected row reads as one unless a theme says otherwise.
    fn runs(&self) -> Option<Vec<(&str, &str)>> {
        None
    }

    /// Return the heading of the group the item belongs to, or `None` for an
    /// item in no group.
    ///
    /// A list that groups its items shows one heading above each run of
    /// adjacent items in the same group, and indents those items under it.
    /// Only the items take the selection, so the host orders the items to
    /// keep each group together.
    fn group(&self) -> Option<&str> {
        None
    }

    /// Return the text a filter searches for this item. The default is the
    /// label. An item can return more text, such as names or tags that its
    /// label leaves out.
    fn search_text(&self) -> &str {
        self.label()
    }

    /// Return when a list shows the item. The default shows it always.
    fn visibility(&self) -> ItemVisibility {
        ItemVisibility::Always
    }
}

/// When a list shows an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemVisibility {
    /// The item shows without a filter, and while a filter matches it.
    #[default]
    Always,
    /// The item shows only without a filter, such as a shortcut to an item
    /// that a filter finds in its own place.
    WithoutFilter,
    /// The item shows only while a filter matches it, such as an item that
    /// a search reaches from a list of summaries.
    WithFilter,
}

impl ItemLabel for String {
    fn label(&self) -> &str {
        self
    }
}

impl ItemLabel for &str {
    fn label(&self) -> &str {
        self
    }
}
