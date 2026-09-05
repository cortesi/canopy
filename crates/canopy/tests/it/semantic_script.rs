//! Semantic lookup remains independent of decorative ancestors.

#[cfg(test)]
mod tests {
    use canopy::{Canopy, NodeId, Widget, commands::ArgValue, error::Result};

    struct Marker;
    impl Widget for Marker {}

    #[test]
    fn script_keys_are_scoped_and_follow_wrapping() -> Result<()> {
        let mut canopy = Canopy::new();
        let (first, first_item, second) = canopy.with_root_context(|ctx| {
            let root = ctx.root_id();
            let first = NodeId::from(ctx.add_child(Marker)?);
            let second = NodeId::from(ctx.add_child(Marker)?);
            let first_item = NodeId::from(ctx.add_child_to(first, Marker)?);
            let second_item = NodeId::from(ctx.add_child_to(second, Marker)?);
            ctx.set_semantic_key(first, root, "first")?;
            ctx.set_semantic_key(second, root, "second")?;
            ctx.set_semantic_key(first_item, first, "item")?;
            ctx.set_semantic_key(second_item, second, "item")?;
            Ok((first, first_item, second))
        })?;
        let source = r#"
            local first = canopy.find_key("first")
            local second = canopy.find_key("second")
            if first == nil or second == nil then error("missing scope") end
            local first_item = canopy.find_key("item", first)
            local second_item = canopy.find_key("item", second)
            if first_item == nil or second_item == nil then error("missing item") end
            canopy.assert(first_item ~= second_item)
            canopy.assert(canopy.find_key("item") == nil)
            local identity = canopy.node_info(first_item).semantic_identity
            if identity == nil then error("missing identity") end
            canopy.assert(identity.key == "item")
            return true
        "#;
        assert_eq!(canopy.eval_script_value(source)?, ArgValue::Bool(true));
        canopy.with_root_context(|ctx| {
            ctx.edit_structure(&mut |ctx| {
                let wrapper = NodeId::from(ctx.create_detached(Marker)?);
                ctx.detach(first_item)?;
                ctx.attach(wrapper, first_item)?;
                ctx.attach(first, wrapper)
            })
        })?;
        assert_eq!(canopy.eval_script_value(source)?, ArgValue::Bool(true));
        canopy.with_root_context(|ctx| {
            ctx.clear_semantic_key(first_item)?;
            ctx.detach(first_item)?;
            ctx.attach(second, first_item)?;
            assert_eq!(ctx.find_key(first, "item")?, None);
            Ok(())
        })?;
        Ok(())
    }
}
