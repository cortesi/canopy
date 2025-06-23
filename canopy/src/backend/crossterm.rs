use std::{
    io::{self, Write},
    panic,
    process::exit,
    sync::mpsc,
    thread,
};

use color_backtrace::{default_output_stream, BacktracePrinter};
use scopeguard::defer;

use crate::{
    backend::BackendControl,
    cursor, error,
    event::{key, mouse, Event, EventSource},
    geom::{Expanse, Point},
    render::RenderBackend,
    style::{Color, Style},
    Canopy, Node, Result,
};
use crossterm::{
    self, cursor as ccursor, event as cevent, style, terminal, ExecutableCommand, QueueableCommand,
};

fn translate_color(c: Color) -> style::Color {
    match c {
        Color::Black => style::Color::Black,
        Color::DarkGrey => style::Color::DarkGrey,
        Color::Red => style::Color::Red,
        Color::DarkRed => style::Color::DarkRed,
        Color::Green => style::Color::Green,
        Color::DarkGreen => style::Color::DarkGreen,
        Color::Yellow => style::Color::Yellow,
        Color::DarkYellow => style::Color::DarkYellow,
        Color::Blue => style::Color::Blue,
        Color::DarkBlue => style::Color::DarkBlue,
        Color::Magenta => style::Color::Magenta,
        Color::DarkMagenta => style::Color::DarkMagenta,
        Color::Cyan => style::Color::Cyan,
        Color::DarkCyan => style::Color::DarkCyan,
        Color::White => style::Color::White,
        Color::Grey => style::Color::Grey,
        Color::Rgb { r, g, b } => style::Color::Rgb { r, g, b },
        Color::AnsiValue(a) => style::Color::AnsiValue(a),
    }
}

fn translate_result<T>(e: io::Result<T>) -> Result<T> {
    match e {
        Ok(t) => Ok(t),
        Err(e) => Err(error::Error::Render(e.to_string())),
    }
}

#[derive(Debug)]
pub struct CrosstermControl {
    fp: std::io::Stderr,
}

impl CrosstermControl {
    fn enter(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        self.fp.execute(terminal::EnterAlternateScreen)?;
        self.fp.execute(cevent::EnableMouseCapture)?;
        self.fp.execute(ccursor::Hide)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }
    fn exit(&mut self) -> io::Result<()> {
        self.fp.execute(terminal::LeaveAlternateScreen)?;
        self.fp.execute(cevent::DisableMouseCapture)?;
        self.fp.execute(ccursor::Show)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }
}

impl Default for CrosstermControl {
    fn default() -> CrosstermControl {
        CrosstermControl {
            fp: std::io::stderr(),
        }
    }
}

impl BackendControl for CrosstermControl {
    fn start(&mut self) -> Result<()> {
        translate_result(self.enter())
    }
    fn stop(&mut self) -> Result<()> {
        translate_result(self.exit())
    }
}

pub struct CrosstermRender {
    fp: std::io::Stderr,
}

impl CrosstermRender {
    fn flush(&mut self) -> io::Result<()> {
        self.fp.flush()?;
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.fp.queue(ccursor::Hide {})?;
        Ok(())
    }

    fn show_cursor(&mut self, c: cursor::Cursor) -> io::Result<()> {
        self.fp.queue(ccursor::MoveTo(c.location.x, c.location.y))?;
        if c.blink {
            self.fp.queue(ccursor::EnableBlinking)?;
        } else {
            self.fp.queue(ccursor::DisableBlinking)?;
        }
        self.fp.queue(match c.shape {
            cursor::CursorShape::Block => ccursor::SetCursorStyle::BlinkingBlock,
            cursor::CursorShape::Line => ccursor::SetCursorStyle::BlinkingBar,
            cursor::CursorShape::Underscore => ccursor::SetCursorStyle::BlinkingUnderScore,
        })?;
        self.fp.queue(ccursor::Show)?;
        Ok(())
    }

    fn style(&mut self, s: Style) -> io::Result<()> {
        // Order is important here - if we reset after setting foreground and
        // background colors they are lost.
        if s.attrs.is_empty() {
            self.fp
                .queue(style::SetAttribute(style::Attribute::Reset))?;
        } else {
            if s.attrs.bold {
                self.fp.queue(style::SetAttribute(style::Attribute::Bold))?;
            }
            if s.attrs.crossedout {
                self.fp
                    .queue(style::SetAttribute(style::Attribute::CrossedOut))?;
            }
            if s.attrs.dim {
                self.fp.queue(style::SetAttribute(style::Attribute::Dim))?;
            }
            if s.attrs.italic {
                self.fp
                    .queue(style::SetAttribute(style::Attribute::Italic))?;
            }
            if s.attrs.overline {
                self.fp
                    .queue(style::SetAttribute(style::Attribute::OverLined))?;
            }
            if s.attrs.underline {
                self.fp
                    .queue(style::SetAttribute(style::Attribute::Underlined))?;
            }
        }
        self.fp
            .queue(style::SetForegroundColor(translate_color(s.fg)))?;
        self.fp
            .queue(style::SetBackgroundColor(translate_color(s.bg)))?;
        Ok(())
    }

    fn text(&mut self, loc: Point, txt: &str) -> io::Result<()> {
        self.fp.queue(ccursor::MoveTo(loc.x, loc.y))?;
        self.fp.queue(style::Print(txt))?;
        Ok(())
    }
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
        translate_result(self.flush())
    }

    fn hide_cursor(&mut self) -> Result<()> {
        translate_result(self.hide_cursor())
    }

    fn show_cursor(&mut self, c: cursor::Cursor) -> Result<()> {
        translate_result(self.show_cursor(c))
    }

    fn style(&mut self, s: Style) -> Result<()> {
        translate_result(self.style(s))
    }

    fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
        translate_result(self.text(loc, txt))
    }

    #[allow(unused_must_use)]
    fn exit(&mut self, code: i32) -> ! {
        self.fp.execute(terminal::LeaveAlternateScreen);
        self.fp.execute(cevent::DisableMouseCapture);
        self.fp.execute(ccursor::Show);
        terminal::disable_raw_mode();
        exit(code)
    }

    fn reset(&mut self) -> Result<()> {
        Ok(())
    }
}

fn translate_key_modifiers(mods: cevent::KeyModifiers) -> key::Mods {
    key::Mods {
        shift: mods.contains(cevent::KeyModifiers::SHIFT),
        ctrl: mods.contains(cevent::KeyModifiers::CONTROL),
        alt: mods.contains(cevent::KeyModifiers::ALT),
    }
}

fn translate_button(b: cevent::MouseButton) -> mouse::Button {
    match b {
        cevent::MouseButton::Left => mouse::Button::Left,
        cevent::MouseButton::Right => mouse::Button::Right,
        cevent::MouseButton::Middle => mouse::Button::Middle,
    }
}

/// Translate a crossterm event into a canopy event
fn translate_event(e: cevent::Event) -> Event {
    match e {
        cevent::Event::Key(k) => Event::Key(key::Key {
            mods: translate_key_modifiers(k.modifiers),
            key: match k.code {
                cevent::KeyCode::Backspace => key::KeyCode::Backspace,
                cevent::KeyCode::Enter => key::KeyCode::Enter,
                cevent::KeyCode::Left => key::KeyCode::Left,
                cevent::KeyCode::Right => key::KeyCode::Right,
                cevent::KeyCode::Up => key::KeyCode::Up,
                cevent::KeyCode::Down => key::KeyCode::Down,
                cevent::KeyCode::Home => key::KeyCode::Home,
                cevent::KeyCode::End => key::KeyCode::End,
                cevent::KeyCode::PageUp => key::KeyCode::PageUp,
                cevent::KeyCode::PageDown => key::KeyCode::PageDown,
                cevent::KeyCode::Tab => key::KeyCode::Tab,
                cevent::KeyCode::BackTab => key::KeyCode::BackTab,
                cevent::KeyCode::Delete => key::KeyCode::Delete,
                cevent::KeyCode::Insert => key::KeyCode::Insert,
                cevent::KeyCode::F(x) => key::KeyCode::F(x),
                cevent::KeyCode::Char(c) => key::KeyCode::Char(c),
                cevent::KeyCode::Null => key::KeyCode::Null,
                cevent::KeyCode::Esc => key::KeyCode::Esc,
                cevent::KeyCode::CapsLock => key::KeyCode::CapsLock,
                cevent::KeyCode::ScrollLock => key::KeyCode::ScrollLock,
                cevent::KeyCode::NumLock => key::KeyCode::NumLock,
                cevent::KeyCode::PrintScreen => key::KeyCode::PrintScreen,
                cevent::KeyCode::Pause => key::KeyCode::Pause,
                cevent::KeyCode::Menu => key::KeyCode::Menu,
                cevent::KeyCode::KeypadBegin => key::KeyCode::KeypadBegin,
                cevent::KeyCode::Media(k) => key::KeyCode::Media(match k {
                    cevent::MediaKeyCode::Play => key::MediaKeyCode::Play,
                    cevent::MediaKeyCode::Pause => key::MediaKeyCode::Play,
                    cevent::MediaKeyCode::PlayPause => key::MediaKeyCode::PlayPause,
                    cevent::MediaKeyCode::Reverse => key::MediaKeyCode::Reverse,
                    cevent::MediaKeyCode::Stop => key::MediaKeyCode::Stop,
                    cevent::MediaKeyCode::FastForward => key::MediaKeyCode::FastForward,
                    cevent::MediaKeyCode::Rewind => key::MediaKeyCode::Rewind,
                    cevent::MediaKeyCode::TrackNext => key::MediaKeyCode::TrackNext,
                    cevent::MediaKeyCode::TrackPrevious => key::MediaKeyCode::TrackPrevious,
                    cevent::MediaKeyCode::Record => key::MediaKeyCode::Record,
                    cevent::MediaKeyCode::LowerVolume => key::MediaKeyCode::LowerVolume,
                    cevent::MediaKeyCode::RaiseVolume => key::MediaKeyCode::RaiseVolume,
                    cevent::MediaKeyCode::MuteVolume => key::MediaKeyCode::MuteVolume,
                }),
                cevent::KeyCode::Modifier(m) => key::KeyCode::Modifier(match m {
                    cevent::ModifierKeyCode::LeftShift => key::ModifierKeyCode::LeftShift,
                    cevent::ModifierKeyCode::LeftControl => key::ModifierKeyCode::LeftControl,
                    cevent::ModifierKeyCode::LeftAlt => key::ModifierKeyCode::LeftAlt,
                    cevent::ModifierKeyCode::LeftSuper => key::ModifierKeyCode::LeftSuper,
                    cevent::ModifierKeyCode::LeftHyper => key::ModifierKeyCode::LeftHyper,
                    cevent::ModifierKeyCode::LeftMeta => key::ModifierKeyCode::LeftMeta,
                    cevent::ModifierKeyCode::RightShift => key::ModifierKeyCode::RightShift,
                    cevent::ModifierKeyCode::RightControl => key::ModifierKeyCode::RightControl,
                    cevent::ModifierKeyCode::RightAlt => key::ModifierKeyCode::RightAlt,
                    cevent::ModifierKeyCode::RightSuper => key::ModifierKeyCode::RightSuper,
                    cevent::ModifierKeyCode::RightHyper => key::ModifierKeyCode::RightHyper,
                    cevent::ModifierKeyCode::RightMeta => key::ModifierKeyCode::RightMeta,
                    cevent::ModifierKeyCode::IsoLevel3Shift => key::ModifierKeyCode::IsoLevel3Shift,
                    cevent::ModifierKeyCode::IsoLevel5Shift => key::ModifierKeyCode::IsoLevel5Shift,
                }),
            },
        }),
        cevent::Event::Mouse(m) => {
            let mut button = mouse::Button::None;
            let action = match m.kind {
                cevent::MouseEventKind::Down(b) => {
                    button = translate_button(b);
                    mouse::Action::Down
                }
                cevent::MouseEventKind::Up(b) => {
                    button = translate_button(b);
                    mouse::Action::Up
                }
                cevent::MouseEventKind::Drag(b) => {
                    button = translate_button(b);
                    mouse::Action::Drag
                }
                cevent::MouseEventKind::Moved => mouse::Action::Moved,
                cevent::MouseEventKind::ScrollDown => mouse::Action::ScrollDown,
                cevent::MouseEventKind::ScrollUp => mouse::Action::ScrollUp,
                cevent::MouseEventKind::ScrollLeft => mouse::Action::ScrollLeft,
                cevent::MouseEventKind::ScrollRight => mouse::Action::ScrollRight,
            };
            Event::Mouse(mouse::MouseEvent {
                button,
                action,
                location: Point {
                    x: m.column,
                    y: m.row,
                },
                modifiers: translate_key_modifiers(m.modifiers),
            })
        }
        cevent::Event::Resize(x, y) => Event::Resize(Expanse::new(x, y)),
        cevent::Event::FocusGained => Event::FocusGained,
        cevent::Event::FocusLost => Event::FocusLost,
        cevent::Event::Paste(s) => Event::Paste(s),
    }
}

fn event_emitter(evt_tx: mpsc::Sender<Event>) {
    thread::spawn(move || loop {
        match cevent::read() {
            Ok(evt) => {
                let ret = evt_tx.send(translate_event(evt));
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

pub fn runloop<N>(mut cnpy: Canopy, mut root: N) -> Result<()>
where
    N: Node,
{
    let mut be = CrosstermRender::default();
    let ctrl = CrosstermControl::default();

    translate_result(terminal::enable_raw_mode())?;
    let mut w = std::io::stderr();

    translate_result(crossterm::execute!(
        w,
        terminal::EnterAlternateScreen,
        cevent::EnableMouseCapture,
        ccursor::Hide
    ))?;

    defer! {
        let mut stderr = std::io::stderr();
        #[allow(unused_must_use)]
        {
            crossterm::execute!(stderr, terminal::LeaveAlternateScreen, cevent::DisableMouseCapture, ccursor::Show);
            terminal::disable_raw_mode();
        }
    }

    panic::set_hook(Box::new(|pi| {
        let mut stderr = std::io::stderr();
        #[allow(unused_must_use)]
        {
            crossterm::execute!(
                stderr,
                terminal::LeaveAlternateScreen,
                cevent::DisableMouseCapture,
                ccursor::Show
            );
            terminal::disable_raw_mode();
            BacktracePrinter::new().print_panic_info(pi, &mut default_output_stream());
        }
    }));

    let rx = if let Some(x) = cnpy.event_rx.take() {
        x
    } else {
        panic!("core event loop already initialized")
    };

    let events = EventSource::new(rx);
    event_emitter(cnpy.event_tx.clone());
    let size = translate_result(terminal::size())?;
    cnpy.register_backend(ctrl);
    cnpy.set_root_size(Expanse::new(size.0, size.1), &mut root)?;
    cnpy.start_poller(cnpy.event_tx.clone());

    cnpy.render(&mut be, &mut root)?;
    translate_result(be.flush())?;

    loop {
        cnpy.event(&mut root, events.next()?)?;

        if cnpy.taint || cnpy.focus_changed() {
            cnpy.render(&mut be, &mut root)?;
            translate_result(be.flush())?;
            cnpy.taint = false;
        }
    }
}
