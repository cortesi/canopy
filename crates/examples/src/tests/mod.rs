mod focusgym;
mod framegym;
mod help;
mod listgym;
mod stylegym;
mod termgym;

use canopy::{CanopyBuilder, Loader, Widget, error::Result, geom::Size, testing::harness::Harness};

use crate::demo_canopy;

fn root_harness<W>(app: W, setup: fn(CanopyBuilder) -> CanopyBuilder, size: Size) -> Result<Harness>
where
    W: Widget + Loader + 'static,
{
    let canopy = setup(demo_canopy().configure(W::load))
        .assemble(move |canopy| {
            canopy.replace_root(app)?;
            Ok(())
        })
        .build()?;
    let mut harness = Harness::from_canopy(canopy, size)?;
    harness.render()?;
    Ok(harness)
}
