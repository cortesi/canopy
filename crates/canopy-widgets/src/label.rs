//! Display labels for list-style widgets.

/// An item that renders as one line of text.
pub trait Label {
    /// Return the display label for this item.
    fn label(&self) -> &str;
}

impl Label for String {
    fn label(&self) -> &str {
        self
    }
}

impl Label for &str {
    fn label(&self) -> &str {
        self
    }
}
