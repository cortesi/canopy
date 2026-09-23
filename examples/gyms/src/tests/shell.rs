//! Launcher shell render checks.

use canopy::{Register, error::Result, geom::Size, testing::harness::Harness};
use canopy_widgets::Root;

use crate::{DemoShell, chargym, demo_canopy};

#[test]
fn the_launcher_shell_adds_a_footer_naming_the_demo_and_the_help_key() -> Result<()> {
    let canopy = chargym::binding_setup(demo_canopy().configure(chargym::CharGym::register))
        .assemble(|canopy| {
            Root::new().install(canopy, DemoShell::new(chargym::CharGym::new()))?;
            Ok(())
        })
        .build()?;
    let mut harness = Harness::from_canopy(canopy, Size::new(60, 12))?;
    harness.render()?;
    assert!(harness.tbuf().contains_text("char_gym"));
    assert!(harness.tbuf().contains_text("ctrl+g: help"));
    Ok(())
}
