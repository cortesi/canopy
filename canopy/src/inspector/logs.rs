use std::{
    io::Write,
    sync::{Arc, Mutex},
};
use tracing_subscriber::fmt;

use crate as canopy;
use crate::{
    event::key,
    fit,
    geom::{Expanse, Rect},
    taint_tree,
    widgets::{list::*, Text},
    BackendControl, Node, NodeState, Outcome, Render, Result, StatefulNode, ViewPort,
};
use std::time::Duration;

#[derive(StatefulNode)]
struct LogItem {
    state: NodeState,
    selected: bool,
    child: Text,
}

impl LogItem {
    fn new(txt: &str) -> Self {
        LogItem {
            state: NodeState::default(),
            selected: false,
            child: Text::new(txt),
        }
    }
}

impl ListItem for LogItem {
    fn set_selected(&mut self, state: bool) {
        self.selected = state
    }
}

impl Node for LogItem {
    fn fit(&mut self, target: Expanse) -> Result<Expanse> {
        self.child.fit(Expanse {
            w: target.w - 2,
            h: target.h,
        })
    }

    fn render(&mut self, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        let (_, screen) = vp.screen_rect().carve_hstart(2);
        let outer = self.child.fit(screen.into())?;
        let view = Rect {
            tl: vp.view_rect().tl,
            w: vp.view_rect().w.saturating_sub(2),
            h: vp.view_rect().h,
        };
        self.child
            .set_viewport(ViewPort::new(outer, view, screen.tl)?);

        let v = vp.view_rect();
        let status = Rect::new(v.tl.x, v.tl.y, 1, v.h);
        if self.selected {
            r.fill("blue", status, '\u{2588}')?;
        } else {
            r.fill("", status, ' ')?;
        }
        let buf = Rect::new(v.tl.x + 1, v.tl.y, 1, v.h);
        r.fill("", buf, ' ')?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }
}

struct LogWriter {
    buf: Arc<Mutex<Vec<String>>>,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf
            .lock()
            .unwrap()
            .push(String::from_utf8_lossy(buf).to_string().trim().to_string());
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(StatefulNode)]
pub struct Logs {
    state: NodeState,
    list: List<LogItem>,
    started: bool,
    buf: Arc<Mutex<Vec<String>>>,
}

impl Node for Logs {
    fn handle_key(&mut self, _ctrl: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        let lst = &mut self.list;
        match k {
            c if c == 'C' => {
                lst.clear();
            }
            c if c == 'd' => {
                lst.delete_selected();
            }
            c if c == 'g' => lst.select_first(),
            c if c == 'G' => lst.select_last(),
            c if c == 'J' => lst.scroll_down(),
            c if c == 'K' => lst.scroll_up(),
            c if c == 'j' || c == key::KeyCode::Down => lst.select_next(),
            c if c == 'k' || c == key::KeyCode::Up => lst.select_prev(),
            c if c == ' ' || c == key::KeyCode::PageDown => lst.page_down(),
            c if c == key::KeyCode::PageUp => lst.page_up(),
            _ => return Ok(Outcome::ignore()),
        };
        canopy::taint_tree(self);
        Ok(Outcome::handle())
    }

    fn poll(&mut self) -> Option<Duration> {
        if !self.started {
            // Configure a custom event formatter
            let format = fmt::format()
                .with_level(true)
                .with_line_number(true)
                .with_ansi(false)
                .without_time()
                .compact();

            let buf = self.buf.clone();
            tracing_subscriber::fmt()
                .with_writer(move || -> LogWriter { LogWriter { buf: buf.clone() } })
                .event_format(format)
                .init();
            self.started = true;
        }
        {
            let mut b = self.buf.lock().unwrap();
            for i in b.drain(0..) {
                self.list.append(LogItem::new(&i));
            }
        }
        taint_tree(self);
        Some(Duration::from_millis(100))
    }

    fn render(&mut self, _: &mut Render) -> Result<()> {
        let vp = self.vp();
        fit(&mut self.list, vp)?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.list)?;
        Ok(())
    }
}

impl Logs {
    pub fn new() -> Self {
        Logs {
            state: NodeState::default(),
            list: List::new(vec![]),
            started: false,
            buf: Arc::new(Mutex::new(vec![])),
        }
    }
}
