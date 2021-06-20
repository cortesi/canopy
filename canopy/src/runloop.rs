use crate::{
    event::EventSource,
    geom::{Point, Rect},
    layout::FixedLayout,
    Canopy, EventResult, Node,
};
use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::size,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use color_backtrace::{default_output_stream, BacktracePrinter};
use scopeguard::defer;

use std::io::Write;
use std::panic;

pub fn runloop<S, N>(app: &mut Canopy<S>, root: &mut N, s: &mut S) -> Result<()>
where
    N: Node<S> + FixedLayout<S>,
{
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;
    defer! {
        let mut stdout = std::io::stdout();
        #[allow(unused_must_use)]
        {
            execute!(stdout, LeaveAlternateScreen, DisableMouseCapture, Show);
            disable_raw_mode();
        }
    }

    panic::set_hook(Box::new(|pi| {
        let mut stdout = std::io::stdout();
        #[allow(unused_must_use)]
        {
            execute!(stdout, LeaveAlternateScreen, DisableMouseCapture, Show);
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
    let _ = panic::take_hook();

    let mut stdout = std::io::stdout();
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture, Show)?;
    disable_raw_mode()?;
    Ok(())
}
