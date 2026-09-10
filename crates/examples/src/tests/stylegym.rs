use canopy::{
    error::Result,
    prelude::*,
    style::{Attr, AttrSet, PartialStyle, ResolvedStyle},
    testing::harness::Harness,
};
use canopy_widgets::{Root, Selector};

use super::root_harness;
use crate::stylegym::{EffectOption, Stylegym, binding_setup};

fn setup_harness(size: Size) -> Result<Harness> {
    root_harness(Stylegym::new(), binding_setup, size)
}

#[test]
fn installed_stylegym_keeps_controls_beside_demo() -> Result<()> {
    let mut canopy = Canopy::new();
    Stylegym::load(&mut canopy)?;
    let app = Root::new().install(&mut canopy, Stylegym::new())?;
    let mut harness = Harness::from_canopy(canopy, Size::new(80, 24))?;
    harness.render()?;

    harness.canopy.with_root_view(|context| {
        let children = context.children_of(app.into());
        let [controls, demo] = children.as_slice() else {
            panic!("stylegym must have controls and demo children");
        };
        let controls = context.view_of(*controls).expect("controls view").outer;
        let demo = context.view_of(*demo).expect("demo view").outer;

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

fn demo_color_and_frame_style(harness: &Harness) -> (ResolvedStyle, ResolvedStyle) {
    let frame = harness.canopy.with_root_view(|ctx| {
        let right = ctx.children()[1];
        let frame = ctx.children_of(right)[0];
        ctx.view_of(frame).expect("demo frame view").outer
    });
    let content = Point {
        x: (frame.left() + 1) as u32,
        y: (frame.top() + 2) as u32,
    };
    let border = Point {
        x: (frame.right() - 1) as u32,
        y: content.y,
    };
    let content = harness.buf().get(content).expect("red sample cell");
    let border = harness.buf().get(border).expect("demo border cell");
    assert_eq!(content.ch, '█');
    assert_eq!(border.ch, '│');
    (content.style, border.style)
}

fn select_invert_and_apply(harness: &mut Harness) -> Result<()> {
    harness.with_root_context(|stylegym: &mut Stylegym, ctx| {
        ctx.with_unique_descendant::<Selector<EffectOption>, _>(|selector, selector_ctx| {
            selector.select_by(selector_ctx, 3)?;
            selector.toggle(selector_ctx)
        })?;
        stylegym.apply_effects(ctx)
    })?;
    harness.render()
}

#[test]
fn modal_dimming_survives_effect_changes_without_accumulating() -> Result<()> {
    let mut effects_first = setup_harness(Size::new(80, 24))?;
    let baseline = demo_color_and_frame_style(&effects_first);
    select_invert_and_apply(&mut effects_first)?;
    let undimmed_invert = demo_color_and_frame_style(&effects_first);
    effects_first.with_root_context(|stylegym: &mut Stylegym, ctx| stylegym.show_modal(ctx))?;
    effects_first.render()?;
    let expected = demo_color_and_frame_style(&effects_first);
    assert_ne!(expected.0, undimmed_invert.0);
    assert_eq!(expected.1, baseline.1);

    let mut modal_first = setup_harness(Size::new(80, 24))?;
    modal_first.with_root_context(|stylegym: &mut Stylegym, ctx| stylegym.show_modal(ctx))?;
    select_invert_and_apply(&mut modal_first)?;
    assert_eq!(demo_color_and_frame_style(&modal_first), expected);
    for _ in 0..2 {
        modal_first
            .with_root_context(|stylegym: &mut Stylegym, ctx| stylegym.apply_effects(ctx))?;
        modal_first.render()?;
        assert_eq!(demo_color_and_frame_style(&modal_first), expected);
    }
    modal_first.with_root_context(|stylegym: &mut Stylegym, ctx| stylegym.hide_modal(ctx))?;
    modal_first.render()?;
    assert_eq!(demo_color_and_frame_style(&modal_first), undimmed_invert);
    Ok(())
}
