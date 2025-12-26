/// Backend utilities for tests.
pub mod backend;
/// Buffer testing utilities.
pub mod buf;
/// Dummy context for tests.
pub mod dummyctx;
/// Grid test helpers.
pub mod grid;
/// Harness for node testing.
pub mod harness;
/// Render helpers for tests.
pub mod render;
/// Test tree helpers.
pub mod ttree;

#[cfg(test)]
mod tests {
    use taffy::style::{Dimension, Display, FlexDirection, Style};

    use super::backend::TestRender;
    use crate::{
        Canopy, ViewContext, derive_commands,
        error::Result,
        geom::{Expanse, Rect},
        render::Render,
        state::NodeName,
        widget::Widget,
    };

    struct Block {
        horizontal: bool,
    }

    #[derive_commands]
    impl Block {
        fn new(horizontal: bool) -> Self {
            Self { horizontal }
        }
    }

    impl Widget for Block {
        fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
            if ctx.children().is_empty() {
                r.fill("blue", ctx.view(), 'x')?;
            }
            Ok(())
        }

        fn configure_style(&self, style: &mut Style) {
            style.display = Display::Flex;
            style.flex_direction = if self.horizontal {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            };
        }

        fn name(&self) -> NodeName {
            NodeName::convert("block")
        }
    }

    #[test]
    fn block_renders() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut canopy = Canopy::new();

        canopy.core.set_widget(canopy.core.root, Block::new(true));
        let left = canopy.core.add(Block::new(false));
        let right = canopy.core.add(Block::new(false));
        canopy
            .core
            .set_children(canopy.core.root, vec![left, right])?;

        canopy.core.build(left).style(|style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        });
        canopy.core.build(right).style(|style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        });

        canopy.set_root_size(Expanse::new(20, 10))?;
        canopy.render(&mut tr)?;
        assert!(!tr.buf_empty());
        Ok(())
    }

    #[test]
    fn render_on_focus_change() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut canopy = Canopy::new();

        canopy.core.set_widget(canopy.core.root, Block::new(true));
        let left = canopy.core.add(Block::new(false));
        let right = canopy.core.add(Block::new(false));
        canopy
            .core
            .set_children(canopy.core.root, vec![left, right])?;

        canopy.core.build(left).style(|style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        });
        canopy.core.build(right).style(|style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        });

        canopy.set_root_size(Expanse::new(20, 10))?;
        canopy.render(&mut tr)?;
        tr.text.lock().unwrap().text.clear();

        canopy.core.focus_next(canopy.core.root);
        canopy.render(&mut tr)?;
        assert!(tr.buf_empty());

        Ok(())
    }
}
