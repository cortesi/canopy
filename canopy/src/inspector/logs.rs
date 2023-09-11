use std::{
    io::Write,
    sync::{Arc, Mutex},
};
use tracing_subscriber::fmt;

use crate as canopy;
use crate::{
    geom::{Expanse, Rect},
    layout,
    widgets::{list::*, Text},
    *,
};
use std::time::Duration;

#[derive(StatefulNode)]
struct LogItem {
    state: NodeState,
    selected: bool,
    child: Text,
}

#[derive_commands]
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
    fn fit(&mut self, target: Expanse) -> Result<()> {
        self.child.fit(Expanse {
            w: target.w - 2,
            h: target.h,
        })?;
        let sz = self.child.vp().canvas;
        self.vp_mut().fit_size(sz, target);
        Ok(())
    }

    fn render(&mut self, _c: &dyn Core, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        let (_, screen) = vp.screen_rect().carve_hstart(2);
        self.child.fit(screen.into())?;
        let view = Rect {
            tl: vp.view.tl,
            w: vp.view.w.saturating_sub(2),
            h: vp.view.h,
        };
        self.child
            .set_viewport(ViewPort::new(self.child.vp().canvas, view, screen.tl)?);

        let v = vp.view;
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
    fn poll(&mut self, c: &mut dyn Core) -> Option<Duration> {
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
            let buf = self.buf.clone();
            let mut b = buf.lock().unwrap();
            if !b.is_empty() {
                c.taint_tree(self);
            }
            let vals = b.drain(0..);
            for i in vals {
                self.list.append(LogItem::new(&i));
            }
        }
        Some(Duration::from_millis(100))
    }

    fn render(&mut self, _: &dyn Core, _: &mut Render) -> Result<()> {
        let vp = self.vp();
        layout::fit(&mut self.list, vp)?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.list)?;
        Ok(())
    }
}

#[derive_commands]
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

impl Loader for Logs {
    fn load(c: &mut Canopy) {
        c.add_commands::<List<LogItem>>();
    }
}
