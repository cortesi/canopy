//! End-to-end keyboard interaction tests for the Todo example.

#[cfg(test)]
mod tests {
    use anyhow::Result as AnyResult;
    use canopy::{event::key::KeyCode, prelude::*, testing::harness::Harness};
    use canopy_widgets::List;
    use todo::{TodoEntry, create_app, store};

    fn add(h: &mut Harness, text: &str) -> Result<()> {
        h.key('a')?;
        for ch in text.chars() {
            h.key(ch)?;
        }
        h.key(KeyCode::Enter)
    }

    fn del_first(h: &mut Harness) -> Result<()> {
        h.key('g')?;
        h.key('d')
    }

    fn del_no_nav(h: &mut Harness) -> Result<()> {
        h.key('d')
    }

    fn list_len(h: &mut Harness) -> usize {
        h.canopy
            .with_root_context(|ctx| {
                ctx.with_unique_descendant::<List<TodoEntry>, _>(|list, _| Ok(list.len()))
            })
            .expect("list node missing")
    }

    /// Build an app over a fresh in-memory database.
    fn app() -> AnyResult<Harness> {
        let canopy = create_app(":memory:")?;
        let mut h = Harness::from_canopy(canopy, Size::new(100, 100))?;
        h.render()?;
        Ok(h)
    }

    #[test]
    fn add_item_via_script() -> AnyResult<()> {
        let mut h = app()?;

        h.key('a')?;
        h.key('h')?;
        h.key('i')?;
        h.key(KeyCode::Enter)?;
        assert_eq!(list_len(&mut h), 1);
        let todos = store::get()?.todos()?;
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].item.trim(), "hi");
        Ok(())
    }

    #[test]
    fn add_item_with_char_newline() -> AnyResult<()> {
        let mut h = app()?;

        h.key('a')?;
        h.key('h')?;
        h.key('i')?;
        // A raw newline is not the modal's confirm key, so nothing is added.
        h.key('\n')?;
        assert_eq!(list_len(&mut h), 0);
        Ok(())
    }

    #[test]
    fn add_item_via_pty() -> AnyResult<()> {
        let mut h = app()?;

        add(&mut h, "item_one")?;
        add(&mut h, "item_two")?;
        add(&mut h, "item_three")?;
        assert_eq!(list_len(&mut h), 3);
        del_first(&mut h)?;
        assert!(h.tbuf().contains_text("item_two"));
        del_first(&mut h)?;
        del_first(&mut h)?;
        assert_eq!(list_len(&mut h), 0);
        Ok(())
    }

    #[test]
    fn single_item_add_remove() -> AnyResult<()> {
        let mut h = app()?;

        add(&mut h, "solo")?;
        assert_eq!(list_len(&mut h), 1);
        del_first(&mut h)?;
        assert_eq!(list_len(&mut h), 0);
        Ok(())
    }

    #[test]
    fn delete_after_moving_focus() -> AnyResult<()> {
        let mut h = app()?;
        add(&mut h, "first")?;
        add(&mut h, "second")?;
        h.key('j')?;
        h.key('d')?;
        assert_eq!(list_len(&mut h), 1);
        assert!(h.tbuf().contains_text("first"));
        Ok(())
    }

    #[test]
    fn delete_middle_keeps_rest() -> AnyResult<()> {
        let mut h = app()?;
        add(&mut h, "first")?;
        add(&mut h, "second")?;
        add(&mut h, "third")?;
        h.key('j')?;
        h.key('j')?;
        h.key('d')?;
        assert_eq!(list_len(&mut h), 2);
        Ok(())
    }

    #[test]
    fn delete_first_without_nav() -> AnyResult<()> {
        let mut h = app()?;
        add(&mut h, "a1")?;
        add(&mut h, "a2")?;
        add(&mut h, "a3")?;
        del_no_nav(&mut h)?;
        del_no_nav(&mut h)?;
        assert_eq!(list_len(&mut h), 1);
        Ok(())
    }

    #[test]
    fn focus_moves_with_navigation() -> AnyResult<()> {
        let mut h = app()?;
        add(&mut h, "one")?;
        add(&mut h, "two")?;
        // A step down and back up returns the selection to where it started, so the
        // delete removes the same item it would have without navigating.
        h.key('j')?;
        h.key('k')?;
        h.key('d')?;
        assert_eq!(list_len(&mut h), 1);
        let todos = store::get()?.todos()?;
        assert_eq!(todos.len(), 1);
        assert!(todos[0].item.contains("two"));
        Ok(())
    }

    #[test]
    fn delete_first_keeps_second_visible() -> AnyResult<()> {
        let mut h = app()?;
        add(&mut h, "first")?;
        add(&mut h, "second")?;
        h.key('g')?; // Go to first item
        h.key('d')?; // Delete first item

        // After deletion, we still have one item
        assert_eq!(list_len(&mut h), 1);

        // Check that the database still has the right item
        let todos = store::get()?.todos()?;
        assert_eq!(todos.len(), 1);
        assert!(todos[0].item.contains("second"));
        Ok(())
    }
}
