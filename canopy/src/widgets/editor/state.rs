use super::primitives::*;

use crate::geom::Point;

const DEFAULT_WRAP: usize = 80;

/// The current state of the editor
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct State {
    /// The underlying raw text being edited.
    pub chunks: Vec<Chunk>,
    /// The current cursor position.
    pub cursor: Cursor,
    /// The current wrap width
    pub width: usize,
}

impl State {
    /// Create a new State from the specified text. The cursor begins at the start of the text, in visual mode.
    pub fn new(text: &str) -> Self {
        let cursor = Cursor::Char((0, 0).into());
        let mut t: Vec<Chunk> = text
            .split("\n")
            .map(|x| Chunk::new(x, DEFAULT_WRAP))
            .collect();
        if t.is_empty() {
            t.push(Chunk::new("", DEFAULT_WRAP))
        }
        State {
            chunks: t,
            cursor,
            width: DEFAULT_WRAP,
        }
    }

    #[cfg(test)]
    /// Create a new State from a text specification. An Insert cursor position is indicated by an underscore "_"
    /// character. A Character cursor position is indicated by a "<" character, which "points at" the character at the
    /// offset. The cursor position indicator is removed from the final string.
    pub(crate) fn from_spec(spec: &str) -> Self {
        let mut txt = vec![];
        let mut cursor = None;
        for (cnt, i) in spec.lines().enumerate() {
            if let Some(x) = i.find("_") {
                cursor = Some(Cursor::Insert((cnt, x).into()));
                txt.push(i.replace("_", ""))
            } else if let Some(x) = i.find("<") {
                cursor = Some(Cursor::Char((cnt, x.saturating_sub(1)).into()));
                txt.push(i.replace("<", ""))
            } else {
                txt.push(i.into());
            }
        }
        let mut n = State::new(&txt.join("\n"));
        if let Some(x) = cursor {
            n.cursor = x;
        }
        n
    }

    /// Insert a set of lines at the cursor, then update the cursor to point just beyond the last inserted character.
    pub fn insert_lines<T, S, I>(&mut self, pos: T, s: S)
    where
        S: AsRef<[I]>,
        I: ToString,
        T: Into<InsertPos>,
    {
        let pos = pos.into();
        let s = s.as_ref();
        if s.len() > 1 {
            // Start by snipping the line at the insert point into start and end chunks.
            let start = &self.chunks[pos.chunk].as_str()[..pos.offset];
            let end = &self.chunks[pos.chunk].as_str()[pos.offset..].to_string();

            self.chunks[pos.chunk] =
                Chunk::new(&format!("{}{}", start, s[0].to_string()), self.width);

            let mut trailer = s[1..].iter().map(|x| x.to_string()).collect::<Vec<_>>();
            let last = trailer.pop().unwrap();
            trailer.push(format!("{}{}", last, end));

            self.chunks.splice(
                pos.chunk + 1..pos.chunk + 1,
                trailer.iter().map(|x| Chunk::new(x, self.width)),
            );
            match self.cursor {
                Cursor::Char(_) => {
                    self.cursor = Cursor::Char(CharPos {
                        chunk: pos.chunk + s.len() - 1,
                        offset: last.len(),
                    });
                }
                Cursor::Insert(_) => {
                    self.cursor = Cursor::Insert(InsertPos {
                        chunk: pos.chunk + s.len() - 1,
                        offset: last.len(),
                    });
                }
            }
        } else {
            // If there are no newlines, we just insert the text in-place.
            let s = &s[0].to_string();
            self.chunks[pos.chunk].insert(pos.offset as usize, s);
            self.cursor = self.cursor.shift(self, s.len() as isize);
            match self.cursor {
                Cursor::Char(_) => {
                    self.cursor = Cursor::Char(CharPos {
                        chunk: pos.chunk,
                        offset: (pos.offset + s.len()).saturating_sub(1),
                    });
                }
                Cursor::Insert(_) => {
                    self.cursor = Cursor::Insert(InsertPos {
                        chunk: pos.chunk,
                        offset: pos.offset + s.len(),
                    });
                }
            }
        }
    }

    /// Insert the given text at the given position, and update the cursor.
    pub fn insert<T>(&mut self, pos: T, s: &str)
    where
        T: Into<InsertPos>,
    {
        self.insert_lines(pos, s.split("\n").collect::<Vec<&str>>())
    }

    /// Insert the given text at the given position, and update the cursor if necessary.
    pub fn delete<T>(&mut self, start: T, end: T)
    where
        T: Into<InsertPos>,
    {
        let start: InsertPos = start.into();
        let end: InsertPos = end.into();

        if start.chunk > self.chunks.len() || end == start {
            // Out of bounds, so this is a no-op
            return;
        } else if start.chunk == end.chunk {
            // We're doing a delete that doesn't cross chunk boundaries.
            self.chunks[start.chunk].replace_range(start.offset..end.offset, "");
            let ip = self.cursor.insert();
            // We only need to adjust the cursor if it was beyond the deletion point
            if ip > start {
                if ip <= end {
                    // If it was within the deleted text, the new cursor position is at the start of the deleted chunk.
                    self.cursor = Cursor::Insert(start);
                } else if ip.chunk == start.chunk {
                    // If it was beyond the deleted text, we shift the cursor back by the number of chars deleted.
                    self.cursor = Cursor::Insert(InsertPos {
                        chunk: ip.chunk,
                        offset: ip.offset.saturating_sub(end.offset - start.offset),
                    });
                }
            }
        } else {
            // We're doing a delete that crosses chunk boundaries.
            // We begin by chopping off the trailer of the first chunk.
            let mut m = self.chunks.remove(start.chunk);
            m.replace_range(start.offset.., "");

            // If our deletion range doesn't exceed the number of chunks we have (meaning we are deleting to the end of
            // the text), we need to splice in the trailer of the last chunk.
            if self.chunks.len() > end.chunk - 1 {
                // Remove the last chunk, exract its trailer, and push it onto the end of the first chunk.
                let mut n = self.chunks.remove(end.chunk - 1);
                n.replace_range(..end.offset.min(n.len()), "");
                m.push_str(n.as_str());
                // Now remove all intermediate chunks - these are chunks that are deleted completely.
                self.chunks.drain(start.chunk..end.chunk - 1);
            }
            self.chunks.insert(start.chunk, m);

            // Now we need to adjust the cursor.
            let cursor = self.cursor.insert();
            // If the cursor was before the deleted section, just leave it.
            if cursor > start {
                if cursor <= end {
                    // The cursor was within the deleted chunk, so the new position is just at deletion point.
                    self.cursor = Cursor::Insert(start);
                } else if cursor.chunk == end.chunk {
                    // The cursor was within the trailer of the last chunk. Maintain the character position.
                    self.cursor = Cursor::Insert(InsertPos {
                        chunk: start.chunk,
                        offset: start.offset + cursor.offset.saturating_sub(end.offset),
                    });
                } else {
                    // The cursor was beyond the deleted chunk. We only need to adjust the chunk offset.
                    self.cursor = Cursor::Insert(InsertPos {
                        chunk: cursor.chunk.saturating_sub(cursor.chunk - start.chunk),
                        offset: cursor.offset.saturating_sub(cursor.offset),
                    });
                }
            }
        }
    }

    /// What's the last insert position in the text?
    pub(super) fn last(&self) -> InsertPos {
        (
            self.chunks.len() - 1,
            self.chunks[self.chunks.len() - 1].len() - 1,
        )
            .into()
    }

    /// Retrieve lines of text from inclusive start to exclusive end. The first and last line returned may be partial if
    /// the offsets are not on line boundaries.
    pub fn line_range<T>(&self, start: T, end: T) -> Vec<String>
    where
        T: Into<InsertPos>,
    {
        let start = start.into().cap(self);
        let end = end.into().cap(self);

        let mut buf = vec![];
        if start.chunk == end.chunk {
            buf.push(self.chunks[start.chunk].as_str()[start.offset..end.offset].to_string());
        } else {
            buf.push(self.chunks[start.chunk].as_str()[start.offset..].to_string());
            if end.chunk - start.chunk > 1 {
                for l in &self.chunks[(start.chunk + 1)..(end.chunk - 1)] {
                    buf.push(l.as_str().into());
                }
            }
            buf.push(self.chunks[end.chunk].as_str()[..end.offset].to_string());
        }
        buf
    }

    /// The complete text of this editor, with chunks separated by newlines.
    pub fn text(&self) -> String {
        self.chunks
            .iter()
            .map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Retrieve the text from inclusive start to exclusive end. The first and last line returned may be partial if the
    /// offsets are not on line boundaries.
    pub fn text_range<T>(&self, start: T, end: T) -> String
    where
        T: Into<InsertPos>,
    {
        self.line_range(start, end).join("\n")
    }

    /// Get a Line from a given wrapped line offset. The return value is a tuple (chunk offset, wrapped line offset),
    /// where the wrapped line offset is the offset within the returned chunk. If the specified offset is out of range,
    /// the last line is returned.
    pub fn line_from_offset(&self, offset: usize) -> Line {
        let mut wrapped_offset = 0;
        for (i, c) in self.chunks.iter().enumerate() {
            if wrapped_offset + c.wraps.len() > offset {
                return (i, offset - wrapped_offset).into();
            }
            wrapped_offset += c.wraps.len();
        }
        (
            self.chunks.len() - 1,
            self.chunks[self.chunks.len() - 1].wraps.len() - 1,
        )
            .into()
    }

    /// Calulate the (x, y) co-ordinates of a Position within a wrapped window. If the position is not in the
    /// window, None is returned.
    pub fn coords_in_window(&self, win: Window, pos: InsertPos) -> Option<Point> {
        for (y, l) in win.lines(self).iter().enumerate() {
            if let Some(l) = l {
                if l.chunk == pos.chunk
                    && l.offset <= pos.offset
                    && l.offset + self.width > pos.offset
                {
                    return Some(((l.offset - pos.offset) as u16, y as u16).into());
                }
            }
        }
        None
    }

    /// Return the wrapped lines in a given window. The start offset is in terms of the wrapped text. The returned Vec
    /// may be shorter than length if the end of the text is reached.
    pub fn wrapped_text(&self, w: Window) -> Vec<Option<&str>> {
        let mut buf = vec![];
        for l in w.lines(self) {
            if let Some(l) = l {
                buf.push(Some(self.chunks[l.chunk].wrapped_line(l.offset)));
            } else {
                buf.push(None);
            }
        }
        buf
    }

    pub fn wrapped_height(&self) -> usize {
        self.chunks.iter().map(|x| x.wraps.len()).sum()
    }

    /// Set the width of the editor for wrapping, and return the total number of wrapped lines that resulted.
    pub fn set_width(&mut self, width: usize) -> usize {
        // FIXME: This needs to be a as close to a nop as possible if the width hasn't changed.
        self.width = width;
        self.chunks.iter_mut().map(|x| x.wrap(width)).sum()
    }

    /// Move the cursor right within the current chunk, moving to the next wrapped line if needed. Won't move to the
    /// next chunk.
    pub fn cursor_right(&mut self, n: usize) {}

    /// Move the cursor leftight within the current chunk, moving to the previous wrapped line if needed. Won't move to
    /// the previous chunk.
    pub fn cursor_left(&mut self, n: usize) {}

    /// Move the cursor down, shifting to the next chunk if needed.
    pub fn cursor_down(&mut self, n: usize) {}

    /// Move the cursor up, shifting to the previous chunk if needed.
    pub fn cursor_up(&mut self, n: usize) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Check if a specification given as a string containing newlines is equal to a Vec<Option<&str>>. None is ignored.
    fn win_str_eq(b: Vec<Option<&str>>, a: &str) {
        let mut m = vec![];
        for i in b.iter() {
            if let Some(s) = i {
                m.push(*s)
            } else {
                break;
            }
        }
        if a.is_empty() {
            assert!(m.is_empty());
            return;
        }
        assert_eq!(m.join("\n"), a)
    }

    /// Take a state specification a, turn it into a State object, apply the transformation f, then check if the result
    /// is equal to the state specification b.
    fn seq<F>(a: &str, f: F, b: &str)
    where
        F: FnOnce(&mut State) -> (),
    {
        let mut a = State::from_spec(a);
        let b = State::from_spec(b);
        f(&mut a);
        assert_eq!(a, b);
    }

    #[test]
    fn insert() {
        seq("_", |x| x.insert((0, 0), "a"), "a_");
        seq("_", |x| x.insert((0, 0), "a\nb"), "a\nb_");
        seq("abc\ndef_", |x| x.insert((0, 2), "x\ny"), "abx\ny_c\ndef");
    }

    #[test]
    fn delete() {
        // Nop, empty
        seq("a_", |x| x.delete((0, 0), (0, 0)), "a_");

        // Nop, beyond bounds
        seq("a_", |x| x.delete((10, 0), (10, 0)), "a_");
        seq("a_", |x| x.delete((1, 0), (1, 0)), "a_");

        // Single line deletes
        seq("a_", |x| x.delete((0, 0), (0, 1)), "_");
        seq("abc_", |x| x.delete((0, 0), (0, 1)), "bc_");
        seq("abc_", |x| x.delete((0, 1), (0, 2)), "ac_");
        seq("abc_", |x| x.delete((0, 2), (0, 3)), "ab_");
        seq("_abc", |x| x.delete((0, 2), (0, 3)), "_ab");
        seq("ab_c", |x| x.delete((0, 1), (0, 3)), "a_");
        seq("ab_c\nfoo", |x| x.delete((0, 1), (0, 3)), "a_\nfoo");
        seq(
            "foo\nab_c\nfoo",
            |x| x.delete((1, 1), (1, 3)),
            "foo\na_\nfoo",
        );
        seq(
            "foo\nab_c\nfoo",
            |x| x.delete((1, 0), (1, 3)),
            "foo\n_\nfoo",
        );

        // Multi line deletes
        seq(
            "one_\ntwo\nthree",
            |x| x.delete((1, 0), (2, 1)),
            "one_\nhree",
        );
        seq(
            "one\ntw_o\nthree",
            |x| x.delete((1, 0), (2, 1)),
            "one\n_hree",
        );
        seq(
            "one\ntwo\nthre_e",
            |x| x.delete((1, 0), (2, 1)),
            "one\nhre_e",
        );
        seq("one\ntwo\nthre_e", |x| x.delete((0, 1), (2, 4)), "o_e");
        seq("one\ntwo\nthre_e", |x| x.delete((0, 3), (2, 2)), "onere_e");
        seq("one\ntwo\nthre_e", |x| x.delete((0, 3), (2, 3)), "onee_e");
        seq("one\ntwo\nthre_e", |x| x.delete((0, 3), (2, 4)), "one_e");
        seq("one\ntwo\nthre_e", |x| x.delete((0, 3), (2, 5)), "one_");
    }

    #[test]
    fn text_range() {
        let s = State::new("one two\nthree four\nx");
        assert_eq!(s.chunks.len(), 3);
        assert_eq!(s.text_range((0, 0), (0, 3)), "one");
        assert_eq!(s.text_range((0, 4), (0, 7)), "two");
        assert_eq!(s.text_range((0, 1), (0, 2)), "n");
        assert_eq!(s.text_range((0, 0), (1, 0)), "one two\n");
        // // Beyond bounds
        assert_eq!(s.text_range((10, 0), (11, 0)), "");
        assert_eq!(s.text_range((1, 6), (11, 0)), "four\nx");
    }

    #[test]
    fn coords_in_window() {
        let mut s = State::new("one two\nthree four\nx");
        assert_eq!(s.set_width(3), 7);
        assert_eq!(
            s.coords_in_window(Window::from_offset(&s, 0, 3), InsertPos::new(&s, 0, 0)),
            Some(Point { x: 0, y: 0 })
        );
        assert_eq!(
            s.coords_in_window(Window::from_offset(&s, 0, 3), InsertPos::new(&s, 100, 0)),
            None
        );
    }

    #[test]
    fn text_width() {
        let mut s = State::new("one two\nthree four\nx");
        assert_eq!(s.set_width(3), 7);
    }

    #[test]
    fn wrapped_line_offset() {
        let mut s = State::new("one two\nthree four\nx");
        assert_eq!(s.set_width(3), 7);
        assert_eq!(s.line_from_offset(0), (0, 0).into());
        assert_eq!(s.line_from_offset(1), (0, 1).into());
        assert_eq!(s.line_from_offset(2), (1, 0).into());
        assert_eq!(s.line_from_offset(100), (2, 0).into());
    }

    #[test]
    fn wrapped_text() {
        let mut s = State::new("one two\nthree four\nx");
        assert_eq!(s.chunks.len(), 3);
        assert_eq!(s.set_width(3), 7);
        assert_eq!(s.wrapped_text(Window::from_offset(&s, 0, 0)), vec![]);
        assert_eq!(
            s.wrapped_text(Window::from_offset(&s, 0, 1)),
            vec![Some("one")]
        );
        assert_eq!(
            s.wrapped_text(Window::from_offset(&s, 0, 2)),
            vec![Some("one"), Some("two")]
        );
        assert_eq!(
            s.wrapped_text(Window::from_offset(&s, 0, 3)),
            vec![Some("one"), Some("two"), Some("thr")]
        );

        assert_eq!(
            s.wrapped_text(Window::from_offset(&s, 1, 1)),
            vec![Some("two")]
        );
        assert_eq!(
            s.wrapped_text(Window::from_offset(&s, 1, 2)),
            vec![Some("two"), Some("thr")]
        );

        assert_eq!(
            s.wrapped_text(Window::from_offset(&s, 2, 1)),
            vec![Some("thr")]
        );
        assert_eq!(
            s.wrapped_text(Window::from_offset(&s, 2, 2)),
            vec![Some("thr"), Some("ee")]
        );
        assert_eq!(
            s.wrapped_text(Window::from_offset(&s, 2, 3)),
            vec![Some("thr"), Some("ee"), Some("fou")]
        );
        assert_eq!(
            s.wrapped_text(Window::from_offset(&s, 2, 4)),
            vec![Some("thr"), Some("ee"), Some("fou"), Some("r")]
        );
        assert_eq!(
            s.wrapped_text(Window::from_offset(&s, 2, 5)),
            vec![Some("thr"), Some("ee"), Some("fou"), Some("r"), Some("x")]
        );
        assert_eq!(
            s.wrapped_text(Window::from_offset(&s, 2, 6)),
            vec![
                Some("thr"),
                Some("ee"),
                Some("fou"),
                Some("r"),
                Some("x"),
                None
            ]
        );
    }

    #[test]
    fn whitespace() {
        let mut s = State::new("one two\n\nthree four\n\n\nx");
        assert_eq!(s.set_width(3), 10);
        win_str_eq(s.wrapped_text(Window::from_offset(&s, 0, 3)), "one\ntwo\n");
        win_str_eq(
            s.wrapped_text(Window::from_offset(&s, 0, 4)),
            "one\ntwo\n\nthr",
        );
    }
}
