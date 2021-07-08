use crate::{
    colorscheme::ColorScheme,
    error::CanopyError,
    event::EventSource,
    geom::{Point, Rect},
    layout::FixedLayout,
    Canopy, EventResult, Node,
};
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

pub fn runloop<S, N>(
    app: &mut Canopy<S>,
    colors: &mut ColorScheme,
    root: &mut N,
    s: &mut S,
) -> Result<(), CanopyError>
where
    N: Node<S> + FixedLayout<S>,
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
    'outer: loop {
        let mut ignore = false;
        loop {
            if !ignore {
                app.pre_render(root, &mut w)?;
                app.render(root, colors, &mut w)?;
                app.post_render(root, &mut w)?;
                w.flush()?;
            }
            match app.event(root, s, events.next()?)? {
                EventResult::Ignore { .. } => {
                    ignore = true;
                }
                EventResult::Exit => {
                    break 'outer;
                }
                EventResult::Handle { .. } => {
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
