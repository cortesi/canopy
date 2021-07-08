use crate::{error::Error, geom::Rect, EventOutcome};
use crossterm::event::{KeyCode, KeyEvent};

pub struct LineEditor {
    pub title: String,
    pub text: String,
    pub cursor: usize,
    pub area: Rect,
}

impl LineEditor {
    pub fn new(title: String, area: Rect) -> Self {
        LineEditor {
            title,
            area,
            text: "".into(),
            cursor: 0,
        }
    }
    pub fn resize(&mut self, area: Rect) {
        self.area = area;
    }

    pub fn key(&mut self, k: KeyEvent) -> Result<EventOutcome, Error> {
        Ok(match k.code {
            KeyCode::Char(c) => {
                self.text.insert(self.cursor, c);
                self.cursor += 1;
                EventOutcome::default()
            }
            _ => EventOutcome::default(),
        })
    }
}
