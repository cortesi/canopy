use canopy::{
    prelude::*,
    style::{Attr, AttrSet, PartialStyle},
    testing::harness::Harness,
};
use canopy_widgets::Selector;

use crate::stylegym::{EffectOption, Stylegym, setup_bindings};

fn setup_harness(size: Size) -> Result<Harness> {
    let mut harness = Harness::builder(Stylegym::new())
        .size(size.w, size.h)
        .build()?;
    setup_bindings(&mut harness.canopy)?;
    harness.render()?;
    Ok(harness)
}

#[test]
fn test_stylegym_creates() -> Result<()> {
    let _harness = setup_harness(Size::new(80, 24))?;
    Ok(())
}

#[test]
fn test_stylegym_renders() -> Result<()> {
    let harness = setup_harness(Size::new(80, 24))?;
    // Just check it rendered without panicking.
    let _buf = harness.buf();
    Ok(())
}

#[test]
fn italic_effect_excludes_demo_frame() -> Result<()> {
    let mut harness = setup_harness(Size::new(80, 24))?;

    harness.with_root_context(|stylegym: &mut Stylegym, ctx| {
        ctx.with_unique_descendant::<Selector<EffectOption>, _>(|selector, selector_ctx| {
            selector.select_by(selector_ctx, 6)?;
            selector.toggle(selector_ctx)
        })?;
        stylegym.apply_effects(ctx)
    })?;
    harness.render()?;

    let italic = PartialStyle::attrs(AttrSet::new(Attr::Italic));
    assert!(
        harness
            .tbuf()
            .contains_text_style("Normal text sample", &italic)
    );
    assert!(!harness.tbuf().contains_text_style("│", &italic));

    Ok(())
}
