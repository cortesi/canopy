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
    use super::backend::TestRender;
    use crate::{
        Canopy, FocusManager, ReadContext, derive_commands,
        error::Result,
        geom::Expanse,
        layout::{Direction, Layout},
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
        fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
            if ctx.children().is_empty() {
                r.fill("blue", ctx.view().outer_rect_local(), 'x')?;
            }
            Ok(())
        }

        fn layout(&self) -> Layout {
            if self.horizontal {
                Layout::fill().direction(Direction::Row)
            } else {
                Layout::fill()
            }
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
        let left = canopy.core.create_detached(Block::new(false));
        let right = canopy.core.create_detached(Block::new(false));
        canopy
            .core
            .set_children(canopy.core.root, vec![left, right])?;

        canopy.core.set_layout_of(left, Layout::fill())?;
        canopy.core.set_layout_of(right, Layout::fill())?;

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
        let left = canopy.core.create_detached(Block::new(false));
        let right = canopy.core.create_detached(Block::new(false));
        canopy
            .core
            .set_children(canopy.core.root, vec![left, right])?;

        canopy.core.set_layout_of(left, Layout::fill())?;
        canopy.core.set_layout_of(right, Layout::fill())?;

        canopy.set_root_size(Expanse::new(20, 10))?;
        canopy.render(&mut tr)?;
        tr.text.lock().unwrap().text.clear();

        canopy.core.focus_next(canopy.core.root);
        canopy.render(&mut tr)?;
        assert!(tr.buf_empty());

        Ok(())
    }
}
