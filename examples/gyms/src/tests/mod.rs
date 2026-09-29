mod chartgym;
mod focusgym;
mod framegym;
mod help;
mod listgym;
mod shell;
mod smoke;
mod stylegym;
mod termgym;

use canopy::{CanopyBuilder, Widget, error::Result, geom::Size, testing::harness::Harness};
use canopy_widgets::Root;

use crate::demo_canopy;

/// How a test harness mounts the app widget.
pub enum Mount {
    /// Replace the canopy root with the app widget.
    Replace,
    /// Install the app widget under a `Root`.
    Wrap,
}

/// Build a demo harness. `setup` adds the demo's registration and bindings to
/// the shared demo builder.
pub fn root_harness<W>(
    app: W,
    setup: impl FnOnce(CanopyBuilder) -> CanopyBuilder,
    size: Size,
    mount: Mount,
) -> Result<Harness>
where
    W: Widget + 'static,
{
    let canopy = setup(demo_canopy())
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
