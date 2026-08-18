use canopy::{
    prelude::*,
    style::{Attr, AttrSet, PartialStyle},
    testing::harness::Harness,
};
use canopy_widgets::{Root, Selector};

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
fn installed_stylegym_keeps_controls_beside_demo() -> Result<()> {
    let mut canopy = Canopy::new();
    Stylegym::load(&mut canopy)?;
    let app = Root::install_app(&mut canopy, Stylegym::new())?;
    let mut harness = Harness::from_canopy(canopy, Size::new(80, 24))?;
    harness.render()?;

    harness.canopy.with_root_view(|context| {
        let children = context.children_of(app.into());
        let [controls, demo] = children.as_slice() else {
            panic!("stylegym must have controls and demo children");
        };
        let controls = context.node_view(*controls).expect("controls view").outer;
        let demo = context.node_view(*demo).expect("demo view").outer;

        assert_eq!(controls.top(), demo.top());
        assert_eq!(controls.h, demo.h);
        assert_eq!(controls.right(), demo.left());
    });

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
