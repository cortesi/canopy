use canopy::layout::Align;

/// Compute an offset for aligning content inside a span.
pub(crate) fn align_offset(content: u32, available: u32, align: Align) -> u32 {
    if available <= content {
        return 0;
    }
    match align {
        Align::Start => 0,
        Align::Center => (available - content) / 2,
        Align::End => available - content,
    }
}
