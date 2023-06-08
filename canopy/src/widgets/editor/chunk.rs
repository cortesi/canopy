/// Split the input text into lines of the given width, and return the start and end offsets for each line.
fn wrap_offsets(s: &str, width: usize) -> Vec<(usize, usize)> {
    let mut offsets = Vec::new();
    let words = textwrap::core::break_words(
        textwrap::WordSeparator::UnicodeBreakProperties.find_words(s),
        width,
    );
    if words.is_empty() {
        return vec![(0, 0)];
    }
    let lines = textwrap::wrap_algorithms::wrap_first_fit(&words, &[width as f64]);
    for l in lines {
        let start = unsafe { l[0].word.as_ptr().offset_from(s.as_ptr()) };
        let last = l[l.len() - 1];
        let end = unsafe { last.word.as_ptr().offset_from(s.as_ptr()) as usize + last.word.len() };
        offsets.push((start as usize, end));
    }
    offsets
}

/// A chunk is a single piece of text containing no newlines, including any terminating line break characters. A Chunk
/// may be wrapped into multiple lines for display.
#[derive(Debug, Clone, Eq, Hash)]
pub struct Chunk {
    /// The raw text of the line.
    text: String,
    /// The start and end offsets of each wrapped line in the chunk.
    pub wraps: Vec<(usize, usize)>,
    /// The width to which this chunk was wrapped
    // FIXME: This should not be stored in every line
    pub wrap_width: usize,
}

impl PartialEq for Chunk {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Chunk {
    pub fn new(s: &str, wrap: usize) -> Chunk {
        let mut l = Chunk {
            text: s.into(),
            wraps: vec![],
            wrap_width: wrap,
        };
        l.wrap(wrap);
        l
    }

    pub fn replace_range<R: std::ops::RangeBounds<usize>>(&mut self, range: R, s: &str) {
        self.text.replace_range(range, s);
        self.wrap(self.wrap_width);
    }

    pub fn push_str(&mut self, s: &str) {
        self.text.push_str(s);
        self.wrap(self.wrap_width);
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Insert a string at the given offset
    pub fn insert(&mut self, offset: usize, s: &str) {
        self.text.insert_str(offset, s);
        self.wrap(self.wrap_width);
    }

    /// Wrap the chunk into lines of the given width, and return the number of wrapped lines that resulted.
    pub fn wrap(&mut self, width: usize) -> usize {
        self.wraps = wrap_offsets(&self.text, width);
        self.wrap_width = width;
        self.wraps.len()
    }

    /// Return a wrapped line, by offset within this chunk. The offset must be within range, or this function will panic.
    pub fn wrapped_line(&self, off: usize) -> &str {
        let (start, end) = self.wraps[off];
        &self.text[start..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn twrap(s: &str, width: usize, expected: Vec<String>) {
        let offsets = wrap_offsets(s, width);
        assert_eq!(offsets.len(), expected.len());
        for i in 0..offsets.len() {
            let (start, end) = offsets[i];
            let line = &s[start..end];
            assert_eq!(line, expected[i]);
        }
    }

    #[test]
    fn test_wrap_offsets() {
        twrap("", 3, vec!["".into()]);
        twrap("one two three four", 100, vec!["one two three four".into()]);
        twrap("one two", 3, vec!["one".into(), "two".into()]);
        twrap(
            "one two three four",
            10,
            vec!["one two".into(), "three four".into()],
        );
    }
}
