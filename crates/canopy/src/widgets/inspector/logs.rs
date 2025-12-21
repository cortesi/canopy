use std::{
    io::{Result as IoResult, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::core as canopy;
use crate::core::{
    Canopy, Loader, NodeState, derive_commands,
    geom::{Expanse, Rect},
    *,
};
use tracing_subscriber::fmt;

use crate::widgets::{Text, list::*};

#[derive(crate::core::StatefulNode)]
/// List item for a single log entry.
struct LogItem {
    /// Node state.
    state: NodeState,
    /// Selection state.
    selected: bool,
    /// Text display.
    child: Text,
}

#[derive_commands]
impl LogItem {
    /// Construct a log item from text.
    fn new(txt: &str) -> Self {
        Self {
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
    fn layout(&mut self, l: &Layout, target: Expanse) -> Result<()> {
        self.child.layout(
            l,
            Expanse {
                w: target.w - 2,
                h: target.h,
            },
        )?;
        let sz = self.child.vp().canvas();
        self.fit_size(sz, target);
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        let v = vp.view();
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

/// Log writer that appends to a shared buffer.
struct LogWriter {
    /// Shared log buffer.
    buf: Arc<Mutex<Vec<String>>>,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.buf
            .lock()
            .unwrap()
            .push(String::from_utf8_lossy(buf).to_string().trim().to_string());
        Ok(buf.len())
    }
    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

/// Inspector log panel.
#[derive(crate::core::StatefulNode)]
pub struct Logs {
    /// Node state.
    state: NodeState,
    /// List of log items.
    list: List<LogItem>,
    /// Whether logging is initialized.
    started: bool,
    /// Shared log buffer.
    buf: Arc<Mutex<Vec<String>>>,
}

impl Node for Logs {
    fn poll(&mut self, _c: &mut dyn Context) -> Option<Duration> {
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
            b.is_empty();
            let vals = b.drain(0..);
            for i in vals {
                self.list.append(LogItem::new(&i));
            }
        }
        Some(Duration::from_millis(100))
    }

    fn render(&mut self, _: &dyn Context, _: &mut Render) -> Result<()> {
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.list)?;
        Ok(())
    }
}

#[derive_commands]
impl Logs {
    /// Construct a log panel.
    pub fn new() -> Self {
        Self {
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
