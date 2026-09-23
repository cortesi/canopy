//! Display labels for list-style widgets.

/// An item that renders as one line of text.
pub trait ItemLabel {
    /// Return the display label for this item.
    fn label(&self) -> &str;
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
