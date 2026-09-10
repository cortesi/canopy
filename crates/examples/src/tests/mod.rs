mod focusgym;
mod framegym;
mod help;
mod listgym;
mod stylegym;
mod termgym;

use canopy::{CanopyBuilder, Loader, Widget, error::Result, geom::Size, testing::harness::Harness};
use canopy_widgets::Root;

use crate::demo_canopy;

/// How a test harness mounts the app widget.
pub enum Mount {
    /// Replace the canopy root with the app widget.
    Replace,
    /// Install the app widget under a `Root`.
    Wrap,
}

pub fn root_harness<W>(
    app: W,
    setup: fn(CanopyBuilder) -> CanopyBuilder,
    size: Size,
    mount: Mount,
) -> Result<Harness>
where
    W: Widget + Loader + 'static,
{
    let canopy = setup(demo_canopy().configure(W::load))
        .assemble(move |canopy| {
            match mount {
                Mount::Replace => {
                    canopy.replace_root(app)?;
                }
                Mount::Wrap => {
                    Root::new().install(canopy, app)?;
                }
            }
            Ok(())
        })
        .build()?;
    let mut harness = Harness::from_canopy(canopy, size)?;
    harness.render()?;
    Ok(harness)
}
