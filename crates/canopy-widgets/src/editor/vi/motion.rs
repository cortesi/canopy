//! Vim's word, sentence, and paragraph motions and text objects.
//!
//! The functions port those of Neovim's `textobject.c`, so vi mode moves and
//! selects as Neovim does. Each one keeps the name of its original, and the
//! control flow of the original, so the two can be compared line by line.
//!
//! A position steps by grapheme. Its column can equal the length of its
//! line: Vim's place on the line break, where visual mode can put the
//! cursor. The last line has this place too, although no break follows it.

use canopy::text;

use super::class::char_class;
use crate::text_buffer::{TextBuffer, TextPosition};

/// The direction of a sentence search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// Toward the end of the text.
    Forward,
    /// Toward the start of the text.
    Backward,
}

/// The ends and the kind of a visual selection, as text objects read and
/// change them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VisualArea {
    /// The end that stays when the cursor moves: Vim's `VIsual`.
    pub anchor: TextPosition,
    /// The cursor, at the free end.
    pub cursor: TextPosition,
    /// Whether the selection holds whole lines.
    pub linewise: bool,
}

/// The graphemes of one line.
#[derive(Debug)]
struct LineMap {
    /// Index of the line.
    index: usize,
    /// Char column of each grapheme, then the length of the line.
    starts: Vec<usize>,
    /// First char of each grapheme.
    firsts: Vec<char>,
}

impl LineMap {
    /// Map the graphemes of `line` in `buffer`.
    fn new(buffer: &TextBuffer, line: usize) -> Self {
        let text = buffer.line_text(line);
        let mut starts = Vec::new();
        let mut firsts = Vec::new();
        let mut column = 0usize;
        for grapheme in text::graphemes(&text) {
            starts.push(column);
            firsts.push(grapheme.chars().next().unwrap_or(' '));
            column = column.saturating_add(grapheme.chars().count());
        }
        starts.push(column);
        Self {
            index: line,
            starts,
            firsts,
        }
    }

    /// Return the length of the line in chars.
    fn len(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    /// Return the index of the grapheme that holds `column`, or `None` at
    /// the end of the line.
    fn grapheme_at(&self, column: usize) -> Option<usize> {
        if column >= self.len() {
            return None;
        }
        Some(match self.starts.binary_search(&column) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        })
    }

    /// Return the column of the grapheme after the one at `column`, or the
    /// length of the line.
    fn next(&self, column: usize) -> usize {
        self.grapheme_at(column)
            .map_or(self.len(), |index| self.starts[index + 1])
    }

    /// Return the column of the grapheme before `column`, or 0.
    fn prev(&self, column: usize) -> usize {
        let index = match self.starts.binary_search(&column) {
            Ok(index) | Err(index) => index,
        };
        index.checked_sub(1).map_or(0, |index| self.starts[index])
    }
}

/// Return whether `c` is a blank that sentences and indents skip: a space
/// or a tab.
fn is_white(c: Option<char>) -> bool {
    matches!(c, Some(' ' | '\t'))
}

/// Return whether `c` is one of the chars of `set`.
fn one_of(c: Option<char>, set: &str) -> bool {
    c.is_some_and(|c| set.contains(c))
}

/// Return the position of the last grapheme of `line` that starts at or
/// before `column`, so a normal-mode cursor never rests on a line break.
pub(super) fn clamp_to_char(buffer: &TextBuffer, position: TextPosition) -> TextPosition {
    let map = LineMap::new(buffer, position.line);
    if position.column == 0 || position.column < map.len() {
        return position;
    }
    TextPosition::new(position.line, map.prev(map.len()))
}

/// Return `position` moved back to the start of the grapheme that holds it,
/// or to the end of its line from past the end.
pub(super) fn grapheme_start(buffer: &TextBuffer, position: TextPosition) -> TextPosition {
    let map = LineMap::new(buffer, position.line);
    let column = map
        .grapheme_at(position.column)
        .map_or(map.len(), |index| map.starts[index]);
    TextPosition::new(position.line, column)
}

/// Return the column of the grapheme after `position`, or the length of its
/// line.
pub(super) fn next_column(buffer: &TextBuffer, position: TextPosition) -> usize {
    LineMap::new(buffer, position.line).next(position.column)
}

/// Return the column of the grapheme before `position`, or 0.
pub(super) fn prev_column(buffer: &TextBuffer, position: TextPosition) -> usize {
    LineMap::new(buffer, position.line).prev(position.column)
}

/// Return the column of Vim's `^`: the first char of `line` that is not a
/// blank, or the last char when every char is a blank.
pub(super) fn first_non_blank(buffer: &TextBuffer, line: usize) -> usize {
    let map = LineMap::new(buffer, line);
    let mut index = 0usize;
    while index + 1 < map.firsts.len() && is_white(Some(map.firsts[index])) {
        index += 1;
    }
    // The index stays below the count of graphemes, or 0 on an empty line,
    // so it names a start.
    map.starts[index]
}

/// Vim's view of a buffer: a cursor, and the steps and character classes
/// that its motions use.
pub(super) struct Vim<'a> {
    /// The text.
    buffer: &'a TextBuffer,
    /// The graphemes of the line read last.
    line: Option<LineMap>,
    /// The cursor: Vim's `curwin->w_cursor`.
    cursor: TextPosition,
}

impl<'a> Vim<'a> {
    /// Start at `cursor` in `buffer`.
    pub(super) fn new(buffer: &'a TextBuffer, cursor: TextPosition) -> Self {
        Self {
            buffer,
            line: None,
            cursor,
        }
    }

    /// Return the cursor.
    pub(super) fn cursor(&self) -> TextPosition {
        self.cursor
    }

    /// Return the number of lines, at least one.
    fn line_count(&self) -> usize {
        self.buffer.line_count().max(1)
    }

    /// Return the index of the last line.
    fn last_line(&self) -> usize {
        self.line_count() - 1
    }

    /// Return the graphemes of `line`, reading them once for consecutive
    /// calls.
    fn map(&mut self, line: usize) -> &LineMap {
        let buffer = self.buffer;
        let cached = self.line.take().filter(|map| map.index == line);
        self.line
            .insert(cached.unwrap_or_else(|| LineMap::new(buffer, line)))
    }

    /// Return the length of `line` in chars.
    fn len(&mut self, line: usize) -> usize {
        self.map(line).len()
    }

    /// Return whether `line` is empty: Vim's `LINEEMPTY`.
    fn line_empty(&mut self, line: usize) -> bool {
        self.len(line) == 0
    }

    /// Return whether `line` holds blanks alone: Vim's `linewhite`.
    fn line_white(&mut self, line: usize) -> bool {
        self.map(line).firsts.iter().all(|&c| is_white(Some(c)))
    }

    /// Return whether `line` starts a paragraph or a section: Vim's
    /// `startPS`, for an empty line or a form feed. The nroff macros of
    /// 'paragraphs' and 'sections' do not count.
    fn start_ps(&mut self, line: usize) -> bool {
        self.map(line).firsts.first().is_none_or(|&c| c == '\u{c}')
    }

    /// Return the first char of the grapheme at `position`, or `None` on the
    /// end of the line: Vim's `gchar_pos`.
    fn gchar(&mut self, position: TextPosition) -> Option<char> {
        let map = self.map(position.line);
        map.grapheme_at(position.column)
            .map(|index| map.firsts[index])
    }

    /// Return the class of the char at `position`: 0 for a blank or the end
    /// of a line.
    fn cls_at(&mut self, position: TextPosition) -> u32 {
        match self.gchar(position) {
            None | Some(' ' | '\t') => 0,
            Some(c) => char_class(c),
        }
    }

    /// Return the class of the char at the cursor: Vim's `cls`.
    fn cls(&mut self) -> u32 {
        self.cls_at(self.cursor)
    }

    /// Step `position` forward: Vim's `inc`. Return 1 on a move to the next
    /// line, 2 on a move onto the end of a line, -1 at the end of the text
    /// with no move, and 0 otherwise.
    fn inc(&mut self, position: &mut TextPosition) -> i32 {
        let map = self.map(position.line);
        if position.column < map.len() {
            position.column = map.next(position.column);
            return if position.column < map.len() { 0 } else { 2 };
        }
        if position.line + 1 < self.line_count() {
            *position = TextPosition::new(position.line + 1, 0);
            return 1;
        }
        -1
    }

    /// Step `position` back: Vim's `dec`. Return 1 on a move to the end of
    /// the previous line, -1 at the start of the text with no move, and 0
    /// otherwise.
    fn dec(&mut self, position: &mut TextPosition) -> i32 {
        if position.column > 0 {
            position.column = self.map(position.line).prev(position.column);
            return 0;
        }
        if position.line > 0 {
            let line = position.line - 1;
            *position = TextPosition::new(line, self.len(line));
            return 1;
        }
        -1
    }

    /// Step `position` forward past the end of a line that is not empty:
    /// Vim's `incl`.
    fn incl(&mut self, position: &mut TextPosition) -> i32 {
        let result = self.inc(position);
        if result >= 1 && position.column > 0 {
            return self.inc(position);
        }
        result
    }

    /// Step `position` back past the end of a line that is not empty: Vim's
    /// `decl`.
    fn decl(&mut self, position: &mut TextPosition) -> i32 {
        let result = self.dec(position);
        if result == 1 && position.column > 0 {
            return self.dec(position);
        }
        result
    }

    /// Run a step on the cursor.
    fn step_cursor(&mut self, step: fn(&mut Self, &mut TextPosition) -> i32) -> i32 {
        let mut position = self.cursor;
        let result = step(self, &mut position);
        self.cursor = position;
        result
    }

    /// Step the cursor forward: Vim's `inc_cursor`.
    fn inc_cursor(&mut self) -> i32 {
        self.step_cursor(Self::inc)
    }

    /// Step the cursor back: Vim's `dec_cursor`.
    fn dec_cursor(&mut self) -> i32 {
        self.step_cursor(Self::dec)
    }

    /// Step the cursor forward past line ends: Vim's `incl` on the cursor.
    fn incl_cursor(&mut self) -> i32 {
        self.step_cursor(Self::incl)
    }

    /// Step the cursor back past line ends: Vim's `decl` on the cursor.
    fn decl_cursor(&mut self) -> i32 {
        self.step_cursor(Self::decl)
    }

    /// Move the cursor one grapheme left in its line: Vim's `oneleft`.
    /// Return false at the start of the line.
    fn oneleft(&mut self) -> bool {
        if self.cursor.column == 0 {
            return false;
        }
        let TextPosition { line, column } = self.cursor;
        self.cursor.column = self.map(line).prev(column);
        true
    }

    /// Move to the start of the next word: Vim's `fwd_word`, with a count of
    /// one. With `eol`, the last word of a line stops at its end. Return
    /// false when the cursor started on the last char of the text.
    pub(super) fn fwd_word(&mut self, eol: bool) -> bool {
        let start_class = self.cls();
        let last_line = self.cursor.line == self.last_line();
        let step = self.inc_cursor();
        if step == -1 || (step >= 1 && last_line) {
            return false;
        }
        if step >= 1 && eol {
            return true;
        }
        // Go one char past the end of the current word, if any.
        if start_class != 0 {
            while self.cls() == start_class {
                let step = self.inc_cursor();
                if step == -1 || (step >= 1 && eol) {
                    return true;
                }
            }
        }
        // Go to the next char that is not a blank, and stop on an empty line.
        while self.cls() == 0 {
            if self.cursor.column == 0 && self.line_empty(self.cursor.line) {
                break;
            }
            let step = self.inc_cursor();
            if step == -1 || (step >= 1 && eol) {
                return true;
            }
        }
        true
    }

    /// Move to the start of the word before: Vim's `bck_word`, with a count
    /// of one. With `stop`, a cursor at the start of a word stays. Return
    /// false at the start of the text.
    pub(super) fn bck_word(&mut self, stop: bool) -> bool {
        let start_class = self.cls();
        if self.dec_cursor() == -1 {
            return false;
        }
        if !stop || start_class == self.cls() || start_class == 0 {
            // Skip the blanks before the word, and stop on an empty line.
            while self.cls() == 0 {
                if self.cursor.column == 0 && self.line_empty(self.cursor.line) {
                    return true;
                }
                if self.dec_cursor() == -1 {
                    return true;
                }
            }
            // Move back to the start of this word.
            let class = self.cls();
            if self.skip_chars(class, Direction::Backward) {
                return true;
            }
        }
        // One step too far.
        self.inc_cursor();
        true
    }

    /// Move to the end of the word: Vim's `end_word`, with a count of one.
    /// With `stop`, a cursor at the end of a word stays. With `empty`, stop
    /// on an empty line. Return false at the end of the text.
    pub(super) fn end_word(&mut self, stop: bool, empty: bool) -> bool {
        let start_class = self.cls();
        if self.inc_cursor() == -1 {
            return false;
        }
        if self.cls() == start_class && start_class != 0 {
            // In the middle of a word: move to its end.
            if self.skip_chars(start_class, Direction::Forward) {
                return false;
            }
        } else if !stop || start_class == 0 {
            // At the end of a word: skip the blanks, then move to the end of
            // the next word.
            while self.cls() == 0 {
                if empty && self.cursor.column == 0 && self.line_empty(self.cursor.line) {
                    return true;
                }
                if self.inc_cursor() == -1 {
                    return false;
                }
            }
            let class = self.cls();
            if self.skip_chars(class, Direction::Forward) {
                return false;
            }
        }
        // One step too far.
        self.dec_cursor();
        true
    }

    /// Move back to the end of the word before: Vim's `bckend_word`, with a
    /// count of one. With `eol`, stop at the end of a line. Return false at
    /// the start of the text.
    fn bckend_word(&mut self, eol: bool) -> bool {
        let start_class = self.cls();
        let step = self.dec_cursor();
        if step == -1 {
            return false;
        }
        if eol && step == 1 {
            return true;
        }
        if start_class != 0 {
            while self.cls() == start_class {
                let step = self.dec_cursor();
                if step == -1 || (eol && step == 1) {
                    return true;
                }
            }
        }
        while self.cls() == 0 {
            if self.cursor.column == 0 && self.line_empty(self.cursor.line) {
                break;
            }
            let step = self.dec_cursor();
            if step == -1 || (eol && step == 1) {
                return true;
            }
        }
        true
    }

    /// Skip a run of chars of `class`: Vim's `skip_chars`. Return true at
    /// an end of the text.
    fn skip_chars(&mut self, class: u32, direction: Direction) -> bool {
        while self.cls() == class {
            let step = match direction {
                Direction::Forward => self.inc_cursor(),
                Direction::Backward => self.dec_cursor(),
            };
            if step == -1 {
                return true;
            }
        }
        false
    }

    /// Move back to the start of the word or the blanks at the cursor, in
    /// its line: Vim's `back_in_line`.
    fn back_in_line(&mut self) {
        let start_class = self.cls();
        while self.cursor.column > 0 {
            self.dec_cursor();
            if self.cls() != start_class {
                self.inc_cursor();
                break;
            }
        }
    }

    /// Move `position` back to the first of the blanks before it: Vim's
    /// `find_first_blank`.
    fn find_first_blank(&mut self, position: &mut TextPosition) {
        while self.decl(position) != -1 {
            if !is_white(self.gchar(*position)) {
                self.incl(position);
                break;
            }
        }
    }

    /// Run [`Self::find_first_blank`] on the cursor.
    fn cursor_to_first_blank(&mut self) {
        let mut position = self.cursor;
        self.find_first_blank(&mut position);
        self.cursor = position;
    }

    /// Skip `count / 2` sentences and `count / 2` runs of blanks between
    /// them: Vim's `findsent_forward`.
    fn findsent_forward(&mut self, count: usize, at_start_sent: bool) {
        let mut at_start_sent = at_start_sent;
        for remaining in (0..count).rev() {
            self.findsent(Direction::Forward, 1);
            if at_start_sent {
                self.cursor_to_first_blank();
            }
            if remaining == 0 || at_start_sent {
                self.decl_cursor();
            }
            at_start_sent = !at_start_sent;
        }
    }

    /// Step `position` in `direction`, past the ends of lines.
    fn step(&mut self, direction: Direction, position: &mut TextPosition) -> i32 {
        match direction {
            Direction::Forward => self.incl(position),
            Direction::Backward => self.decl(position),
        }
    }

    /// Move the cursor to the start of a sentence in `direction`: Vim's
    /// `findsent`. Return false when no sentence follows.
    fn findsent(&mut self, direction: Direction, count: usize) -> bool {
        let mut count = count;
        let mut noskip = false;
        let mut position = self.cursor;
        while count > 0 {
            count -= 1;
            let prev = position;
            'found: {
                if self.gchar(position).is_none() {
                    // On an empty line, skip to a line that is not empty.
                    while self.step(direction, &mut position) != -1
                        && self.gchar(position).is_none()
                    {}
                    if direction == Direction::Forward {
                        break 'found;
                    }
                } else if direction == Direction::Forward
                    && position.column == 0
                    && self.start_ps(position.line)
                {
                    // At the start of a paragraph, go to the next line.
                    if position.line == self.last_line() {
                        return false;
                    }
                    position.line += 1;
                    break 'found;
                } else if direction == Direction::Backward {
                    self.decl(&mut position);
                }

                // Go back to the char before the blanks and closing
                // punctuation.
                let mut found_dot = false;
                loop {
                    let c = self.gchar(position);
                    if !(is_white(c) || one_of(c, ".!?)]\"'")) {
                        break;
                    }
                    let mut before = position;
                    if self.decl(&mut before) == -1
                        || (self.line_empty(before.line) && direction == Direction::Forward)
                    {
                        break;
                    }
                    if found_dot {
                        break;
                    }
                    if one_of(c, ".!?") {
                        found_dot = true;
                    }
                    if one_of(c, ")]\"'") && !one_of(self.gchar(before), ".!?)]\"'") {
                        break;
                    }
                    self.decl(&mut position);
                }

                // Find the end of the sentence.
                let start_line = position.line;
                loop {
                    let c = self.gchar(position);
                    if c.is_none() || (position.column == 0 && self.start_ps(position.line)) {
                        if direction == Direction::Backward && position.line != start_line {
                            position.line += 1;
                        }
                        break;
                    }
                    if one_of(c, ".!?") && self.ends_sentence(&mut position) {
                        break;
                    }
                    if self.step(direction, &mut position) == -1 {
                        if count > 0 {
                            return false;
                        }
                        noskip = true;
                        break;
                    }
                }
            }
            // Skip the blanks.
            while !noskip && is_white(self.gchar(position)) {
                if self.incl(&mut position) == -1 {
                    break;
                }
            }
            if prev == position {
                // No move: step one char and try again.
                if self.step(direction, &mut position) == -1 {
                    if count > 0 {
                        return false;
                    }
                    break;
                }
                count += 1;
            }
        }
        self.cursor = position;
        true
    }

    /// Check whether the `.`, `!`, or `?` at `position` ends a sentence:
    /// closing punctuation may follow it, and then a blank, the end of the
    /// line, or the end of the text. If so, move `position` to the char
    /// after, past the end of a line, and return true.
    fn ends_sentence(&mut self, position: &mut TextPosition) -> bool {
        let mut after = *position;
        let mut at_end = false;
        loop {
            if self.inc(&mut after) == -1 {
                at_end = true;
                break;
            }
            if !one_of(self.gchar(after), ")]\"'") {
                break;
            }
        }
        let c = self.gchar(after);
        if !(at_end || is_white(c) || c.is_none()) {
            return false;
        }
        *position = after;
        if self.gchar(*position).is_none() {
            self.inc(position);
        }
        true
    }

    /// Select or extend by a word: Vim's `current_word` in visual mode, for
    /// `iw` with `include` false and `aw` with it true. Return false when
    /// the selection cannot grow; the area may still have changed.
    pub(super) fn current_word(&mut self, area: &mut VisualArea, include: bool) -> bool {
        self.cursor = area.cursor;
        let result = self.current_word_inner(area, include);
        area.cursor = self.cursor;
        result
    }

    /// The body of [`Self::current_word`], which leaves the cursor in
    /// `self`.
    ///
    /// With a count of one, a selection either starts on one char or grows
    /// by one word, never both. So the end that Vim marks exclusive after a
    /// grown selection never meets the blanks that a new `aw` takes, and the
    /// port leaves that flag out.
    fn current_word_inner(&mut self, area: &mut VisualArea, include: bool) -> bool {
        let mut count = 1;
        let mut start = TextPosition::new(0, 0);
        let mut include_white = false;

        // A selection of one char selects the word or the blanks at the
        // cursor.
        if self.cursor == area.anchor {
            self.back_in_line();
            start = self.cursor;
            if (self.cls() == 0) == include {
                if !self.end_word(true, true) {
                    return false;
                }
            } else {
                self.fwd_word(true);
                if self.cursor.column == 0 {
                    self.decl_cursor();
                } else {
                    self.oneleft();
                }
                if include {
                    include_white = true;
                }
            }
            area.anchor = start;
            count -= 1;
        }

        // Extend by more words.
        while count > 0 {
            if self.cursor < area.anchor {
                // The cursor is at the start: move it back.
                if self.decl_cursor() == -1 {
                    return false;
                }
                if include != (self.cls() != 0) {
                    if !self.bck_word(true) {
                        return false;
                    }
                } else {
                    if !self.bckend_word(true) {
                        return false;
                    }
                    self.incl_cursor();
                }
            } else {
                // Move the cursor forward over one word or run of blanks.
                if self.incl_cursor() == -1 {
                    return false;
                }
                if include != (self.cls() == 0) {
                    self.fwd_word(true);
                    // Stop on the last blank, not on the first char of the
                    // next line.
                    self.oneleft();
                } else if !self.end_word(true, true) {
                    return false;
                }
            }
            count -= 1;
        }

        if include_white && self.cls() != 0 {
            // No blanks follow the word, so take the blanks before it, but
            // not an indent.
            let end = self.cursor;
            self.cursor = start;
            if self.oneleft() {
                self.back_in_line();
                if self.cls() == 0 && self.cursor.column > 0 {
                    area.anchor = self.cursor;
                }
            }
            self.cursor = end;
        }
        area.linewise = false;
        true
    }

    /// Select or extend by a sentence: Vim's `current_sent` in visual mode,
    /// for `is` with `include` false and `as` with it true.
    pub(super) fn current_sent(&mut self, area: &mut VisualArea, include: bool) -> bool {
        self.cursor = area.cursor;
        let result = self.current_sent_inner(area, include);
        area.cursor = self.cursor;
        result
    }

    /// The body of [`Self::current_sent`], which leaves the cursor in
    /// `self`.
    fn current_sent_inner(&mut self, area: &mut VisualArea, include: bool) -> bool {
        let count = 1usize;
        let mut start = self.cursor;
        let mut position = start;
        self.findsent(Direction::Forward, 1);

        // A selection of more than one char grows.
        if start != area.anchor {
            self.extend_sent(area, include, count, start, position);
            return true;
        }

        // A cursor on a blank may be just before the next sentence.
        while is_white(self.gchar(position)) {
            self.incl(&mut position);
        }
        let start_blank = position == self.cursor;
        if start_blank {
            self.find_first_blank(&mut start);
        } else {
            self.findsent(Direction::Backward, 1);
            start = self.cursor;
        }
        let ncount = if include {
            count * 2
        } else {
            count - usize::from(start_blank)
        };
        if ncount > 0 {
            self.findsent_forward(ncount, true);
        } else {
            self.decl_cursor();
        }

        if include {
            // With the blanks before the sentence, leave out the blanks
            // after it. With no blanks after it, take the blanks before.
            if start_blank {
                self.cursor_to_first_blank();
                if is_white(self.gchar(self.cursor)) {
                    self.decl_cursor();
                }
            } else if !is_white(self.gchar(self.cursor)) {
                self.find_first_blank(&mut start);
            }
        }

        // Do not get stuck on a single blank before a sentence.
        if start == self.cursor {
            self.extend_sent(area, include, count, start, position);
            return true;
        }
        area.anchor = start;
        area.linewise = false;
        true
    }

    /// Grow a selection by `count` sentences: the extend branch of Vim's
    /// `current_sent`, which reads the start of the operation and a scan
    /// position.
    fn extend_sent(
        &mut self,
        area: &VisualArea,
        include: bool,
        count: usize,
        start: TextPosition,
        position: TextPosition,
    ) {
        let mut position = position;
        let count = if include { count * 2 } else { count };
        if start < area.anchor {
            // The cursor is at the start of the selection: in the blanks
            // before a sentence, in a sentence or just after one, or at its
            // start.
            let mut at_start_sent = true;
            self.decl(&mut position);
            while position < self.cursor {
                if !is_white(self.gchar(position)) {
                    at_start_sent = false;
                    break;
                }
                self.incl(&mut position);
            }
            if !at_start_sent {
                self.findsent(Direction::Backward, 1);
                if self.cursor == start {
                    at_start_sent = true;
                } else {
                    // Inside a sentence: go to its end.
                    self.findsent(Direction::Forward, 1);
                }
            }
            for _ in 0..count {
                if at_start_sent {
                    self.cursor_to_first_blank();
                }
                let c = self.gchar(self.cursor);
                if !at_start_sent || (!include && !is_white(c)) {
                    self.findsent(Direction::Backward, 1);
                }
                at_start_sent = !at_start_sent;
            }
        } else {
            // The cursor is at the end of the selection: just before a
            // sentence, in the blanks before one, or in a sentence.
            self.incl(&mut position);
            let mut at_start_sent = true;
            if position != self.cursor {
                at_start_sent = false;
                while position < self.cursor {
                    if !is_white(self.gchar(position)) {
                        at_start_sent = true;
                        break;
                    }
                    self.incl(&mut position);
                }
                if at_start_sent {
                    self.findsent(Direction::Backward, 1);
                } else {
                    self.cursor = start;
                }
            }
            self.findsent_forward(count, at_start_sent);
        }
    }

    /// Select or extend by a paragraph: Vim's `current_par` in visual mode,
    /// for `ip` with `include` false and `ap` with it true. The selection
    /// becomes linewise.
    pub(super) fn current_par(&mut self, area: &mut VisualArea, include: bool) -> bool {
        self.cursor = area.cursor;
        let result = self.current_par_inner(area, include);
        area.cursor = self.cursor;
        result
    }

    /// The body of [`Self::current_par`]. Line numbers count from one, as
    /// in Vim.
    fn current_par_inner(&mut self, area: &mut VisualArea, include: bool) -> bool {
        let count = 1usize;
        let line_count = self.line_count();
        let mut start_line = self.cursor.line + 1;

        // A selection of more than one line grows.
        if start_line != area.anchor.line + 1 {
            return self.extend_par(area, include, count, start_line);
        }

        // Move back to the first line of the paragraph or of the blank lines.
        let white_in_front = self.line_white(start_line - 1);
        while start_line > 1 {
            let before_white = self.line_white(start_line - 2);
            if white_in_front {
                if !before_white {
                    break;
                }
            } else if before_white || self.start_ps(start_line - 1) {
                break;
            }
            start_line -= 1;
        }

        // Move past the blank lines.
        let mut end_line = start_line;
        while end_line <= line_count && self.line_white(end_line - 1) {
            end_line += 1;
        }
        end_line -= 1;

        let mut remaining = count;
        if !include && white_in_front {
            remaining -= 1;
        }
        while remaining > 0 {
            remaining -= 1;
            if end_line == line_count {
                return false;
            }
            let do_white = !include && self.line_white(end_line);
            if include || !do_white {
                end_line += 1;
                // Skip to the end of the paragraph.
                while end_line < line_count
                    && !self.line_white(end_line)
                    && !self.start_ps(end_line)
                {
                    end_line += 1;
                }
            }
            if remaining == 0 && white_in_front && include {
                break;
            }
            // Skip to the end of the blank lines after the paragraph.
            if include || do_white {
                while end_line < line_count && self.line_white(end_line) {
                    end_line += 1;
                }
            }
        }

        // With no blank lines at the end, take the blank lines before.
        if !white_in_front && end_line >= 1 && !self.line_white(end_line - 1) && include {
            while start_line > 1 && self.line_white(start_line - 2) {
                start_line -= 1;
            }
        }

        // Do not get stuck on a single blank line with `Vipipip`.
        if area.linewise && start_line == self.cursor.line + 1 {
            return self.extend_par(area, include, count, start_line);
        }
        if area.anchor.line + 1 != start_line {
            area.anchor = TextPosition::new(start_line - 1, 0);
        }
        area.linewise = true;
        self.cursor = TextPosition::new(end_line.saturating_sub(1), 0);
        true
    }

    /// Grow a selection of lines by `count` paragraphs: the extend branch
    /// of Vim's `current_par`. `start_line` counts from one.
    fn extend_par(
        &mut self,
        area: &VisualArea,
        include: bool,
        count: usize,
        start_line: usize,
    ) -> bool {
        let backward = start_line < area.anchor.line + 1;
        let limit = if backward { 1 } else { self.line_count() };
        let advance = |line: usize| if backward { line - 1 } else { line + 1 };
        let retreat = |line: usize| if backward { line + 1 } else { line - 1 };
        let mut start_line = start_line;
        let mut result = true;
        for _ in 0..count {
            if start_line == limit {
                result = false;
                break;
            }
            let mut prev_white = None;
            for _ in 0..2 {
                start_line = advance(start_line);
                let start_white = self.line_white(start_line - 1);
                if prev_white == Some(start_white) {
                    start_line = retreat(start_line);
                    break;
                }
                while start_line != limit {
                    let next = advance(start_line);
                    let boundary = if backward { start_line } else { next };
                    if start_white != self.line_white(next - 1)
                        || (!start_white && self.start_ps(boundary - 1))
                    {
                        break;
                    }
                    start_line = next;
                }
                if !include || start_line == limit {
                    break;
                }
                prev_white = Some(start_white);
            }
        }
        self.cursor = TextPosition::new(start_line - 1, 0);
        result
    }
}
