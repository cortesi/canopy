use crate::{event::EventSource, geom::Rect, Canopy, EventResult, Node};
use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::size,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use std::io::Write;

pub fn runloop<S>(app: &mut Canopy, root: &mut dyn Node<S>, s: &mut S) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;
    let events = EventSource::new(200);
    let size = size()?;
    app.resize(
        root,
        Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        },
    )?;
    'outer: loop {
        app.render(root, &mut stdout)?;
        loop {
            match app.event(root, s, events.next()?)? {
                EventResult::Ignore { .. } => {}
                EventResult::Exit => {
                    break 'outer;
                }
                EventResult::Handle { .. } => {
                    app.render(root, &mut stdout)?;
                }
            }
            stdout.flush()?;
        }
    }
    let mut stdout = std::io::stdout();
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture, Show)?;
    disable_raw_mode()?;
    Ok(())
}
