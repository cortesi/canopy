use super::primitives::*;
use canopy_core as canopy;

use canopy_core::geom::Point;

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
    pub window: Window,
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
            window: Window {
                line: Line {
                    chunk: 0,
                    wrap_idx: 0,
                },
                height: 0,
            },
        }
    }

    /// Create a new State from a text specification. An Insert cursor position is indicated by an underscore "_"
    /// character. A Character cursor position is indicated by a "<" character, which "points at" the character at the
    /// offset. The cursor position indicator is removed from the final string.
    #[cfg(test)]
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

    /// Turns a state into a text specification.
    #[cfg(test)]
    pub(crate) fn to_spec(&self) -> String {
        let mut buf = vec![];
        let char = match self.cursor {
            Cursor::Char(_) => '<',
            Cursor::Insert(_) => '_',
        };
        let (chunk, offset) = match self.cursor {
            Cursor::Char(x) => x.chunk_offset(),
            Cursor::Insert(x) => x.chunk_offset(),
        };
        for (i, c) in self.chunks.iter().enumerate() {
            let mut s = c.as_str().to_string();
            if i == chunk {
                if char == '<' {
                    if s.is_empty() {
                        s = "<".into();
                    } else {
                        s.insert(offset + 1, char);
                    }
                } else {
                    s.insert(offset, char);
                }
            }
            buf.push(s);
        }
        buf.join("\n")
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
        if s.len() == 1 {
            // Simple case - there are no newlines, we just insert the text in-place.
            let s = &s[0].to_string();
            self.chunks[pos.chunk].insert(pos.offset, s);

            let cursor = self.cursor.insert(self);
            if cursor >= pos {
                // Adjust the cursor if it was after the insert point.
                self.cursor = self.cursor.shift(self, s.len() as isize);
            }
        } else {
            // We have a multi-line insert. Start by snipping the line at the insert point into start and end chunks.
            let start = &self.chunks[pos.chunk].as_str()[..pos.offset];
            let end = &self.chunks[pos.chunk].as_str()[pos.offset..].to_string();

            // Now modify the start chunk to include the trailer
            self.chunks[pos.chunk] =
                Chunk::new(&format!("{}{}", start, s[0].to_string()), self.width);

            // And generate and insert our trailing lines
            let mut trailer = s[1..].iter().map(|x| x.to_string()).collect::<Vec<_>>();
            let last = trailer.pop().unwrap();
            trailer.push(format!("{last}{end}"));
            self.chunks.splice(
                pos.chunk + 1..pos.chunk + 1,
                trailer.iter().map(|x| Chunk::new(x, self.width)),
            );

            let cursor = self.cursor.insert(self);
            if cursor >= pos {
                // The cursor was at or beyond the insert position, so we have to adjust it.
                self.cursor = self
                    .cursor
                    .shift_chunk(self, s.len().saturating_sub(1) as isize);
                if self.cursor.insert(self).chunk == pos.chunk + trailer.len() {
                    self.cursor = self.cursor.shift(self, last.len() as isize);
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
        let cursor = self.cursor.insert(self);

        if start.chunk > self.chunks.len() || end == start {
            // Out of bounds, so this is a no-op
        } else if start.chunk == end.chunk {
            // We're doing a delete that doesn't cross chunk boundaries.
            //
            self.chunks[start.chunk].replace_range(start.offset..end.offset, "");
            let ip = self.cursor.insert(self);
            // We only need to adjust the cursor if it was beyond the deletion point
            if ip > start && ip < end {
                // If it was within the deleted text, the new cursor position is at the start of the deleted chunk.
                self.cursor = self.cursor.at(self, start.chunk, start.offset);
            } else if ip > start && ip.chunk == start.chunk {
                // If it was beyond the deleted text, we shift the cursor back by the number of chars deleted.
                self.cursor = self.cursor.at(
                    self,
                    ip.chunk,
                    ip.offset.saturating_sub(end.offset - start.offset - 1),
                );
            } else {
                self.cursor = self.cursor.constrain(self);
            }
        } else {
            // We're doing a delete that crosses chunk boundaries.
            //
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
            if cursor > start && cursor <= end {
                // The cursor was within the deleted chunk, so the new position is just at deletion point.
                self.cursor = self.cursor.at(self, start.chunk, start.offset);
            } else if cursor > start && cursor.chunk == end.chunk {
                // The cursor was within the trailer of the last chunk. Maintain the character position.
                self.cursor = self.cursor.at(
                    self,
                    start.chunk,
                    start.offset + cursor.offset.saturating_sub(end.offset),
                );
            } else {
                // The cursor was beyond the deleted chunk. We only need to adjust the chunk offset.
                self.cursor = self.cursor.at(
                    self,
                    cursor.chunk.saturating_sub(end.chunk - start.chunk),
                    cursor.offset,
                );
            }
        }
    }

    /// What's the last insert position in the text?
    pub(super) fn last(&self) -> InsertPos {
        let chunk = self.chunks.len().saturating_sub(1);
        if self.chunks.is_empty() {
            (0, 0)
        } else {
            (chunk, self.chunks[chunk].len().saturating_sub(1))
        }
        .into()
    }

    /// Retrieve lines of text from inclusive start to exclusive end. The first and last line returned may be partial if
    /// the offsets are not on line boundaries.
    pub fn line_range<T>(&self, start: T, end: T) -> Vec<String>
    where
        T: Into<InsertPos>,
    {
        let start = start.into().constrain(self);
        let end = end.into().constrain(self);

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

    /// Calculate the (x, y) co-ordinates of a cursor within a wrapped window. If the position is not in the window,
    /// None is returned. Empty chunks are handled specially, with the
    pub fn cursor_position(&self) -> Option<Point> {
        let pos = self.cursor.insert(self);
        let c = &self.chunks[pos.chunk];
        for (y, l) in self.window.lines(self).iter().enumerate() {
            if let Some(l) = l {
                let (lstart, lend) = self.chunks[l.chunk].wraps[l.wrap_idx];
                if c.len() == 0 && l.chunk == pos.chunk {
                    // We're at the first character of an empty chunk.
                    return Some((0, y as u16).into());
                } else if pos.offset >= c.len() && l.chunk > pos.chunk {
                    // We're beyond the end of the chunk, which means we must be an insertion cursor. Place the cursor
                    // position at the first character of the next line.
                    return Some((0, y as u16).into());
                } else if l.chunk == pos.chunk && lstart <= pos.offset && lend > pos.offset {
                    return Some(((pos.offset - lstart) as u16, y as u16).into());
                }
            }
        }
        None
    }

    /// Return the wrapped lines in the window. The start offset is in terms of the wrapped text. The returned Vec
    /// may be shorter than length if the end of the text is reached.
    pub fn window_text(&self) -> Vec<Option<&str>> {
        let mut buf = vec![];
        for l in self.window.lines(self) {
            if let Some(l) = l {
                buf.push(Some(self.chunks[l.chunk].wrapped_line(l.wrap_idx)));
            } else {
                buf.push(None);
            }
        }
        buf
    }

    pub fn line_height(&self) -> usize {
        self.chunks.iter().map(|x| x.wraps.len()).sum()
    }

    /// Set the width of the editor for wrapping, and return the total number of wrapped lines that resulted.
    pub fn resize_window(&mut self, width: usize, height: usize) -> usize {
        // This needs to be as cheap as possible if the width hasn't changed.
        if self.width == width && self.window.height == height {
            return self.line_height();
        }
        self.width = width;
        self.window = self.window.with_height(height);
        self.chunks.iter_mut().map(|x| x.wrap(width)).sum()
    }

    /// Move the cursor left or right within the current chunk, moving to the next or previous wrapped line if needed.
    /// Won't move to the next chunk. Adjust the window to include the cursor if needed.
    pub fn cursor_shift(&mut self, n: isize) {
        self.cursor = self.cursor.shift(self, n);
        self.window = self.window.adjust(self);
    }

    /// Move the cursor up or down in wrapped lines, moving to the next or previous chunk if needed. Adjust the window
    /// to include the cursor if needed.
    pub fn cursor_shift_line(&mut self, n: isize) {
        self.cursor = self.cursor.shift_line(self, n);
        self.window = self.window.adjust(self);
    }

    /// Move the up or down in the chunk list. Adjust the window to include the cursor if needed.
    pub fn cursor_shift_chunk(&mut self, n: isize) {
        self.cursor = self.cursor.shift_chunk(self, n);
        self.window = self.window.adjust(self);
    }
}
