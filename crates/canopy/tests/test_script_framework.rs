//! Integration tests for the framework Luau API surface.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Context, EventOutcome, Loader, NodeId, ReadContext, Widget, command,
        derive_commands,
        error::Result,
        event::{Event, mouse},
        layout::Layout,
        render::Render,
        testing::harness::Harness,
    };

    struct ApiLeaf {
        value: i32,
    }

    #[derive_commands]
    impl ApiLeaf {
        fn new() -> Self {
            Self { value: 0 }
        }

        #[command]
        fn set(&mut self, value: i32) {
            self.value = value;
        }

        #[command]
        fn get(&self) -> i32 {
            self.value
        }
    }

    impl Widget for ApiLeaf {
        fn render(&mut self, _frame: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
            Ok(())
        }

        fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
            match event {
                Event::Mouse(mouse::MouseEvent {
                    action: mouse::Action::Down,
                    button: mouse::Button::Left,
                    ..
                }) => {
                    self.value = 21;
                    Ok(EventOutcome::Handle)
                }
                Event::Mouse(mouse::MouseEvent {
                    action: mouse::Action::ScrollDown,
                    ..
                }) => {
                    self.value = 22;
                    Ok(EventOutcome::Handle)
                }
                _ => Ok(EventOutcome::Ignore),
            }
        }

        fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
            true
        }
    }

    impl Loader for ApiLeaf {
        fn load(c: &mut Canopy) -> Result<()> {
            c.add_commands::<Self>()
        }
    }

    struct ApiRoot;

    impl Widget for ApiRoot {
        fn layout(&self) -> Layout {
            Layout::row()
        }

        fn render(&mut self, _frame: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
            Ok(())
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let left = ctx.add_child(ApiLeaf::new())?;
            let right = ctx.add_child(ApiLeaf::new())?;
            ctx.set_layout_of(left, Layout::fill())?;
            ctx.set_layout_of(right, Layout::fill())?;
            let _ = ctx.set_focus(left.into());
            Ok(())
        }
    }

    impl Loader for ApiRoot {
        fn load(c: &mut Canopy) -> Result<()> {
            ApiLeaf::load(c)
        }
    }

    fn leaf_ids(harness: &Harness) -> Vec<NodeId> {
        harness.find_nodes("api_root/api_leaf")
    }

    fn leaf_values(harness: &mut Harness) -> Vec<i32> {
        leaf_ids(harness)
            .into_iter()
            .map(|node| harness.with_widget::<ApiLeaf, _>(node, |leaf| leaf.value))
            .collect()
    }

    #[test]
    fn framework_functions_are_available_from_luau() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot).size(20, 5).build()?;
        harness.render()?;

        harness.script(
            r#"
            local root = canopy.root()
            local root_info = canopy.node_info(root)
            canopy.assert(root_info.name == "api_root", "root node should expose the widget name")
            canopy.assert(canopy.parent(root) == nil, "root should not have a parent")

            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.assert(#leaves == 2, "expected two focusable leaves")
            local first = leaves[1]
            local second = leaves[2]

            canopy.assert(
                canopy.find_node("api_root/api_leaf") == first,
                "find_node should return the first matching leaf"
            )
            canopy.assert(canopy.parent(first) == root, "leaf parent should be the root")

            local children = canopy.children(root)
            canopy.assert(#children == 2, "root should expose both children")
            canopy.assert(children[1] == first, "first child should match the first leaf")
            canopy.assert(children[2] == second, "second child should match the second leaf")

            canopy.assert(canopy.focused() ~= nil, "a leaf should be focused after mount")
            canopy.set_focus(first)
            canopy.assert(canopy.focused() == first, "focus should move to the first leaf")
            canopy.assert(api_leaf.get() == 0, "focused dispatch should hit the first leaf")

            canopy.cmd_on(second, "api_leaf::set", 9)
            canopy.assert(canopy.cmd_on(second, "api_leaf::get") == 9, "cmd_on should target a node")
            canopy.assert(api_leaf.get() == 0, "focused dispatch should remain on the first leaf")

            canopy.set_focus(second)
            canopy.assert(canopy.focused() == second, "focus should move to the second leaf")

            canopy.focus_prev()
            canopy.assert(canopy.focused() == first, "focus_prev should move back to the first leaf")
            canopy.focus_next()
            canopy.assert(canopy.focused() == second, "focus_next should move to the second leaf")
            canopy.set_focus(first)
            canopy.focus_dir("Right")
            canopy.assert(canopy.focused() == second, "focus_dir right should move to the second leaf")

            canopy.send_click(1, 1)
            canopy.assert(
                canopy.cmd_on(first, "api_leaf::get") == 21,
                "send_click should dispatch a left click to the target node"
            )
            canopy.send_scroll("Down", 1, 1)
            canopy.assert(
                canopy.cmd_on(first, "api_leaf::get") == 22,
                "send_scroll should dispatch a scroll event to the target node"
            )
        "#,
        )?;

        assert_eq!(leaf_values(&mut harness), vec![22, 9]);
        Ok(())
    }

    #[test]
    fn luau_bindings_replace_unbind_and_clear_correctly() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot).size(20, 5).build()?;
        harness.render()?;

        harness.canopy.eval_script(
            r#"
            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.set_focus(leaves[1])

            canopy.bind_with("x", { desc = "old" }, function() api_leaf.set(3) end)
            canopy.bind_with("x", { desc = "new" }, function() api_leaf.set(7) end)

            local transient = canopy.bind("u", function() api_leaf.set(99) end)
            canopy.unbind(transient)
        "#,
        )?;

        harness.script(r#"canopy.send_key("x")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        harness.script(r#"canopy.send_key("u")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        harness.canopy.eval_script(
            r#"
            canopy.bind("z", function() api_leaf.set(15) end)
            canopy.unbind_key("z")
        "#,
        )?;
        harness.script(r#"canopy.send_key("z")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        harness.canopy.eval_script(
            r#"
            canopy.bind("c", function() api_leaf.set(21) end)
            canopy.clear_bindings()
        "#,
        )?;
        harness.script(r#"canopy.send_key("c")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        Ok(())
    }
}
