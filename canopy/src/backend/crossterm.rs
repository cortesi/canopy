use std::io::Write;
use std::panic;
use std::process::exit;
use std::thread;

use color_backtrace::{default_output_stream, BacktracePrinter};
use scopeguard::defer;

use crate::{
    control::ControlBackend,
    cursor,
    event::{Event, EventSource},
    geom::{Point, Size},
    render::RenderBackend,
    style::{Color, Style, StyleManager},
    Actions, Canopy, Node, Outcome, Render, Result,
};
use crossterm::{
    cursor::{CursorShape, DisableBlinking, EnableBlinking, Hide, MoveTo, SetCursorShape, Show},
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    style::Print,
    style::{Attribute, Color as CColor, SetAttribute, SetBackgroundColor, SetForegroundColor},
    terminal::size,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand, QueueableCommand,
};

fn translate_color(c: Color) -> CColor {
    match c {
        Color::Black => CColor::Black,
        Color::DarkGrey => CColor::DarkGrey,
        Color::Red => CColor::Red,
        Color::DarkRed => CColor::DarkRed,
        Color::Green => CColor::Green,
        Color::DarkGreen => CColor::DarkGreen,
        Color::Yellow => CColor::Yellow,
        Color::DarkYellow => CColor::DarkYellow,
        Color::Blue => CColor::Blue,
        Color::DarkBlue => CColor::DarkBlue,
        Color::Magenta => CColor::Magenta,
        Color::DarkMagenta => CColor::DarkMagenta,
        Color::Cyan => CColor::Cyan,
        Color::DarkCyan => CColor::DarkCyan,
        Color::White => CColor::White,
        Color::Grey => CColor::Grey,
        Color::Rgb { r, g, b } => CColor::Rgb { r, g, b },
        Color::AnsiValue(a) => CColor::AnsiValue(a),
    }
}

pub struct CrosstermControl {
    fp: std::io::Stderr,
}

impl Default for CrosstermControl {
    fn default() -> CrosstermControl {
        CrosstermControl {
            fp: std::io::stderr(),
        }
    }
}

impl ControlBackend for CrosstermControl {
    fn enter(&mut self) -> Result<()> {
        enable_raw_mode()?;
        self.fp.execute(EnterAlternateScreen)?;
        self.fp.execute(EnableMouseCapture)?;
        self.fp.execute(Hide)?;
        disable_raw_mode()?;
        Ok(())
    }
    fn exit(&mut self) -> Result<()> {
        self.fp.execute(LeaveAlternateScreen)?;
        self.fp.execute(DisableMouseCapture)?;
        self.fp.execute(Show)?;
        disable_raw_mode()?;
        Ok(())
    }
}

pub struct CrosstermRender {
    fp: std::io::Stderr,
}

impl Default for CrosstermRender {
    fn default() -> CrosstermRender {
        CrosstermRender {
            fp: std::io::stderr(),
        }
    }
}

impl RenderBackend for CrosstermRender {
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

    fn style(&mut self, s: Style) -> Result<()> {
        // Order is important here - if we reset after setting foreground and
        // background colors they are lost.
        if s.attrs.is_empty() {
            self.fp.queue(SetAttribute(Attribute::Reset))?;
        } else {
            if s.attrs.bold {
                self.fp.queue(SetAttribute(Attribute::Bold))?;
            }
            if s.attrs.crossedout {
                self.fp.queue(SetAttribute(Attribute::CrossedOut))?;
            }
            if s.attrs.dim {
                self.fp.queue(SetAttribute(Attribute::Dim))?;
            }
            if s.attrs.italic {
                self.fp.queue(SetAttribute(Attribute::Italic))?;
            }
            if s.attrs.overline {
                self.fp.queue(SetAttribute(Attribute::OverLined))?;
            }
            if s.attrs.underline {
                self.fp.queue(SetAttribute(Attribute::Underlined))?;
            }
        }
        self.fp.queue(SetForegroundColor(translate_color(s.fg)))?;
        self.fp.queue(SetBackgroundColor(translate_color(s.bg)))?;
        Ok(())
    }

    fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
        self.fp.queue(MoveTo(loc.x, loc.y))?;
        self.fp.queue(Print(txt))?;
        Ok(())
    }

    #[allow(unused_must_use)]
    fn exit(&mut self, code: i32) -> ! {
        self.fp.execute(LeaveAlternateScreen);
        self.fp.execute(DisableMouseCapture);
        self.fp.execute(Show);
        disable_raw_mode();
        exit(code)
    }

    fn reset(&mut self) -> Result<()> {
        Ok(())
    }
}

fn event_emitter<A>(e: &EventSource<A>)
where
    A: 'static + Actions,
{
    let evt_tx = e.tx();
    thread::spawn(move || loop {
        match event::read() {
            Ok(evt) => {
                let oevt = match evt {
                    event::Event::Key(e) => Event::Key(e.into()),
                    event::Event::Mouse(e) => Event::Mouse(e.into()),
                    event::Event::Resize(x, y) => Event::Resize(Size::new(x, y)),
                };
                let ret = evt_tx.send(oevt);
                if ret.is_err() {
                    // FIXME: Do a bit more work here. Restore context,
                    // exit.
                    return;
                }
            }
            Err(_) => {
                // FIXME: Do a bit more work here. Restore context,
                // exit.
                return;
            }
        }
    });
}

pub fn runloop<S, A: 'static + Actions, N>(
    style: StyleManager,
    root: &mut N,
    s: &mut S,
) -> Result<()>
where
    N: Node<S, A>,
{
    let mut be = CrosstermRender::default();
    let mut ctrl = CrosstermControl::default();
    let mut render = Render::new(&mut be, style);

    let mut app = Canopy::new();

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
            BacktracePrinter::new().print_panic_info(pi, &mut default_output_stream());
        }
    }));

    let events = EventSource::default();
    event_emitter(&events);
    let size = size()?;
    app.set_root_size(Size::new(size.0, size.1), root)?;

    loop {
        let mut ignore = false;
        loop {
            if !ignore {
                app.pre_render(&mut render, root)?;
                app.render(&mut render, root)?;
                app.post_render(&mut render, root)?;
                render.flush()?;
            }
            match app.event(&mut ctrl, root, s, events.next()?)? {
                Outcome::Ignore { .. } => {
                    ignore = true;
                }
                Outcome::Handle { .. } => {
                    ignore = false;
                }
            }
        }
    }
}
