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
