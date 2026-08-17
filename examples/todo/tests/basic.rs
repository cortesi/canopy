//! End-to-end keyboard interaction tests for the Todo example.

#[cfg(test)]
mod tests {
    use anyhow::Result as AnyResult;
    use canopy::{event::key::KeyCode, prelude::*, testing::harness::Harness};
    use canopy_widgets::List;
    use tempfile::TempDir;
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

    /// Build an app over a fresh database. The returned directory owns the database file and
    /// removes it when the test ends.
    fn app() -> AnyResult<(Harness, TempDir)> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().join("todo.db");
        let canopy = create_app(db_path.to_str().expect("database path is utf-8"))?;
        let mut h = Harness::from_canopy(canopy, Size::new(100, 100))?;
        h.render()?;
        Ok((h, dir))
    }

    #[test]
    fn add_item_via_script() -> AnyResult<()> {
        let (mut h, _db) = app()?;

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
    #[should_panic(expected = "assertion `left == right` failed")]
    fn add_item_with_char_newline() {
        let (mut h, _db) = app().expect("app builds");

        h.key('a').unwrap();
        h.key('h').unwrap();
        h.key('i').unwrap();
        h.key('\n').unwrap();
        assert_eq!(list_len(&mut h), 1);
    }

    #[test]
    fn add_item_via_pty() -> AnyResult<()> {
        let (mut h, _db) = app()?;

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
    fn delete_reverse_via_pty() -> AnyResult<()> {
        let (mut h, _db) = app()?;
        add(&mut h, "one")?;
        add(&mut h, "two")?;
        add(&mut h, "three")?;
        h.key('j')?;
        h.key('j')?;
        del_first(&mut h)?;
        assert_eq!(list_len(&mut h), 2);
        del_first(&mut h)?;
        del_first(&mut h)?;
        assert_eq!(list_len(&mut h), 0);
        Ok(())
    }

    #[test]
    fn single_item_add_remove() -> AnyResult<()> {
        let (mut h, _db) = app()?;

        add(&mut h, "solo")?;
        assert_eq!(list_len(&mut h), 1);
        del_first(&mut h)?;
        assert_eq!(list_len(&mut h), 0);
        Ok(())
    }

    #[test]
    fn delete_after_moving_focus() -> AnyResult<()> {
        let (mut h, _db) = app()?;
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
        let (mut h, _db) = app()?;
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
        let (mut h, _db) = app()?;
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
        let (mut h, _db) = app()?;
        add(&mut h, "one")?;
        add(&mut h, "two")?;
        // A step down and back up returns the selection to where it started, so the delete
        // removes the same item it would have without navigating.
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
        let (mut h, _db) = app()?;
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
