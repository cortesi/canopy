//! Script observation reads retain one immutable publication until an explicit
//! flush.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, ViewContext, Widget, WidgetSemantics,
        commands::ArgValue,
        derive_commands,
        error::Result,
        geom::{Line, Size},
        layout::Layout,
        render::Render,
    };

    struct Label {
        text: String,
    }

    #[derive_commands]
    impl Label {
        #[command]
        fn rename(&mut self, text: String) {
            self.text = text;
        }
    }

    impl Widget for Label {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn render(&mut self, render: &mut Render, _view: &dyn ViewContext) -> Result<()> {
            render.text("text", Line::new(0, 0, 8), &self.text)
        }

        fn semantics(&self, _view: &dyn ViewContext) -> Result<WidgetSemantics> {
            Ok(WidgetSemantics {
                label: Some(self.text.clone()),
                ..WidgetSemantics::default()
            })
        }
    }

    #[test]
    fn snapshot_records_stay_detached_across_mutation_and_flush() -> Result<()> {
        let mut canopy = Canopy::new();
        canopy.add_commands::<Label>()?;
        canopy.replace_root(Label { text: "old".into() })?;
        assert!(canopy.snapshot().is_none());
        canopy.set_root_size(Size::new(8, 2))?;
        assert_eq!(
            canopy.eval_script(
                r#"
            local old = canopy.snapshot()
            if old == nil then error("first eval must prepare") end
            canopy.assert(old.nodes[1].semantics.label == "old")
            canopy.call_exact(canopy.root(), "label::rename", "new")
            local pending = canopy.snapshot()
            if pending == nil then error("publication disappeared") end
            canopy.assert(pending.frame_id == old.frame_id)
            canopy.assert(pending.nodes[1].semantics.label == "old")
            canopy.flush()
            local fresh = canopy.snapshot()
            if fresh == nil then error("flush must publish") end
            canopy.assert(fresh.frame_id > old.frame_id)
            canopy.assert(fresh.nodes[1].semantics.label == "new")
            canopy.assert(fresh.cells[1][1].text == "n")
            canopy.assert(old.nodes[1].semantics.label == "old")
            canopy.assert(old.cells[1][1].text == "o")
            fresh.nodes[1].semantics.label = "local change"
            local again = canopy.snapshot()
            if again == nil then error("publication disappeared") end
            canopy.assert(again.nodes[1].semantics.label == "new")
            canopy.assert(again.frame_id == fresh.frame_id)
            return true
        "#
            )?,
            ArgValue::Bool(true)
        );
        Ok(())
    }

    #[test]
    fn snapshot_is_nil_until_a_viewport_can_be_published() -> Result<()> {
        let mut canopy = Canopy::new();
        assert_eq!(
            canopy.eval_script("return canopy.snapshot() == nil")?,
            ArgValue::Bool(true)
        );
        Ok(())
    }
}
