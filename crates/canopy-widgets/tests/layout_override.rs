//! Parent layout overrides through the production Root composition.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Loader, Widget,
        error::Result,
        geom::{Point, Size},
        layout::LayoutOverride,
        testing::harness::Harness,
    };
    use canopy_widgets::{Frame, Root, Text};

    #[test]
    fn frame_override_survives_refresh_and_clears_to_widget_defaults() -> Result<()> {
        let mut canopy = Canopy::new();
        Root::load(&mut canopy)?;
        let frame = Root::new().install(&mut canopy, Frame::new())?;
        canopy.with_context(frame, |ctx| {
            let text = ctx.create_detached(Text::new("retained"))?;
            ctx.attach(frame.into(), text.into())?;
            ctx.set_layout_override_of(frame.into(), LayoutOverride::new().fixed_height(3))
        })?;
        canopy.finalize_api()?;
        let mut harness = Harness::from_canopy(canopy, Size::new(20, 8))?;
        harness.render()?;

        for _ in 0..2 {
            harness.canopy.with_context(frame, |ctx| {
                ctx.invalidate_layout();
                Ok(())
            })?;
            harness.render()?;
            harness.canopy.with_root_view(|ctx| {
                let layout = ctx.layout_of(frame.into()).expect("frame layout");
                assert_eq!(layout.min_height, Some(3));
                assert_eq!(layout.max_height, Some(3));
                assert_eq!(layout.padding, Frame::new().layout().padding);
                let view = ctx.view_of(frame.into()).expect("frame view");
                assert_eq!(view.outer_size(), Size::new(20, 3));
                assert_eq!(view.content_size(), Size::new(18, 1));
                assert_eq!(view.content_origin(), Point { x: 1, y: 1 });
            });
            assert!(harness.tbuf().contains_text("retained"));
            assert!(harness.tbuf().contains_text("╰──────────────────╯"));
        }

        harness
            .canopy
            .with_root_context(|ctx| ctx.clear_layout_override_of(frame.into()))?;
        harness.render()?;
        harness.canopy.with_root_view(|ctx| {
            assert_eq!(ctx.layout_of(frame.into()), Some(Frame::new().layout()));
            let view = ctx.view_of(frame.into()).expect("frame view");
            assert_eq!(view.outer_size(), Size::new(20, 8));
            assert_eq!(view.content_size(), Size::new(18, 6));
        });
        assert!(harness.tbuf().contains_text("retained"));
        Ok(())
    }
}
