use std::io::Write;
use std::marker::PhantomData;

use anyhow::Result;

use crate as canopy;
use crate::{
    event::key,
    geom::{Point, Rect},
    layout::FixedLayout,
    widgets::frame,
    Canopy, EventResult, Node,
};

use crossterm::{
    cursor::MoveTo,
    style::{Color, Print, SetForegroundColor},
    QueueableCommand,
};

pub struct Input<S> {
    pub rect: Option<Rect>,
    pub state: canopy::NodeState,
    pub width: u16,
    pub value: String,
    _marker: PhantomData<S>,
}

impl<S> Input<S> {
    pub fn new(width: u16) -> Self {
        Input {
            state: canopy::NodeState::default(),
            rect: None,
            _marker: PhantomData,
            value: String::new(),
            width,
        }
    }
}

impl<S> FixedLayout for Input<S> {
    fn layout(&mut self, _app: &mut Canopy, rect: Option<Rect>) -> Result<()> {
        self.rect = rect;
        Ok(())
    }
}

impl<S> frame::FrameContent for Input<S> {
    fn bounds(&self) -> Option<(Rect, Rect)> {
        if let Some(r) = self.rect {
            let vr = Rect {
                tl: Point { x: 0, y: 0 },
                w: r.w,
                h: r.h,
            };
            Some((vr, vr))
        } else {
            None
        }
    }
}

impl<'a, S> Node<S> for Input<S> {
    fn can_focus(&self) -> bool {
        true
    }
    fn state(&mut self) -> &mut canopy::NodeState {
        &mut self.state
    }
    fn rect(&self) -> Option<Rect> {
        self.rect
    }
    fn render(&mut self, _app: &mut Canopy, w: &mut dyn Write) -> Result<()> {
        if let Some(r) = self.rect {
            w.queue(MoveTo(r.tl.x, r.tl.y))?;
            w.queue(Print(&self.value))?;
        }
        Ok(())
    }
    fn handle_key(&mut self, app: &mut Canopy, _: &mut S, k: key::Key) -> Result<EventResult> {
        Ok(match k {
            key::Key(_, key::KeyCode::Char(c)) => {
                self.value.push(c);
                app.taint(self);
                EventResult::Handle { skip: false }
            }
            _ => EventResult::Ignore { skip: false },
        })
    }
}
