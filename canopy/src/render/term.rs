use std::io::Write;
use std::panic;

use color_backtrace::{default_output_stream, BacktracePrinter};
use scopeguard::defer;

use super::Backend;
use crate::{
    cursor,
    event::EventSource,
    geom::{Point, Rect},
    layout::FillLayout,
    render::Render,
    style::Style,
    Canopy, EventOutcome, Node, Result,
};
use crossterm::{
    cursor::{CursorShape, DisableBlinking, EnableBlinking, Hide, MoveTo, SetCursorShape, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    style::Print,
    style::{Color, SetBackgroundColor, SetForegroundColor},
    terminal::size,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    QueueableCommand,
};

pub struct Term<'a> {
    fp: &'a mut dyn Write,
}

impl<'a> Backend for Term<'a> {
    fn flush(&mut self) -> Result<()> {
        self.fp.flush()?;
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<()> {
        self.fp.queue(Hide {})?;
        Ok(())
    }

    fn show_cursor(&mut self, c: cursor::Cursor) -> Result<()> {
        self.fp.queue(MoveTo(c.location.x, c.location.y))?;
        if c.blink {
            self.fp.queue(EnableBlinking)?;
        } else {
            self.fp.queue(DisableBlinking)?;
        }
        self.fp.queue(SetCursorShape(match c.shape {
            cursor::CursorShape::Block => CursorShape::Block,
            cursor::CursorShape::Line => CursorShape::Line,
            cursor::CursorShape::Underscore => CursorShape::UnderScore,
        }))?;
        self.fp.queue(Show)?;
        Ok(())
    }

    fn fg(&mut self, c: Color) -> Result<()> {
        self.fp.queue(SetForegroundColor(c))?;
        Ok(())
    }

    fn bg(&mut self, c: Color) -> Result<()> {
        self.fp.queue(SetBackgroundColor(c))?;
        Ok(())
    }

    fn fill(&mut self, r: Rect, c: char) -> Result<()> {
        let line = c.to_string().repeat(r.w as usize);
        for n in 0..r.h {
            self.fp.queue(MoveTo(r.tl.x, r.tl.y + n))?;
            self.fp.queue(Print(&line))?;
        }
        Ok(())
    }

    fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
        self.fp.queue(MoveTo(loc.x, loc.y))?;
        self.fp.queue(Print(txt))?;
        Ok(())
    }
}

pub fn runloop<S, N>(app: &mut Canopy<S>, style: Style, root: &mut N, s: &mut S) -> Result<()>
where
    N: Node<S> + FillLayout<S>,
{
    enable_raw_mode()?;
    let mut w = std::io::stderr();

    execute!(w, EnterAlternateScreen, EnableMouseCapture, Hide)?;
    defer! {
        let mut stderr = std::io::stderr();
        #[allow(unused_must_use)]
        {
            execute!(stderr, LeaveAlternateScreen, DisableMouseCapture, Show);
            disable_raw_mode();
        }
    }

    panic::set_hook(Box::new(|pi| {
        let mut stderr = std::io::stderr();
        #[allow(unused_must_use)]
        {
            execute!(stderr, LeaveAlternateScreen, DisableMouseCapture, Show);
            disable_raw_mode();
            BacktracePrinter::new().print_panic_info(&pi, &mut default_output_stream());
        }
    }));

    let events = EventSource::new(200);
    let size = size()?;
    app.resize(
        root,
        Rect {
            tl: Point { x: 0, y: 0 },
            w: size.0,
            h: size.1,
        },
    )?;

    let mut be = Term { fp: &mut w };
    let mut rndr = Render::new(&mut be, style);

    'outer: loop {
        let mut ignore = false;
        loop {
            if !ignore {
                app.pre_render(root, &mut rndr)?;
                app.render(&mut rndr, root)?;
                app.post_render(root, &mut rndr)?;
                rndr.flush()?;
            }
            match app.event(root, s, events.next()?)? {
                EventOutcome::Ignore { .. } => {
                    ignore = true;
                }
                EventOutcome::Exit => {
                    break 'outer;
                }
                EventOutcome::Handle { .. } => {
                    ignore = false;
                }
            }
        }
    }
    let _ = panic::take_hook();

    let mut stderr = std::io::stderr();
    execute!(stderr, LeaveAlternateScreen, DisableMouseCapture, Show)?;
    disable_raw_mode()?;
    Ok(())
}
