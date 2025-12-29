use canopy::{error::Result, geom::Expanse, testing::harness::Harness};

use crate::stylegym::{Stylegym, setup_bindings};

fn setup_harness(size: Expanse) -> Result<Harness> {
    let mut harness = Harness::builder(Stylegym::new())
        .size(size.w, size.h)
        .build()?;
    setup_bindings(&mut harness.canopy)?;
    harness.render()?;
    Ok(harness)
}

#[test]
fn test_stylegym_creates() -> Result<()> {
    let _harness = setup_harness(Expanse::new(80, 24))?;
    Ok(())
}

#[test]
fn test_stylegym_renders() -> Result<()> {
    let harness = setup_harness(Expanse::new(80, 24))?;
    // Just check it rendered without panicking.
    let _buf = harness.buf();
    Ok(())
}
