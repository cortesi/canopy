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
