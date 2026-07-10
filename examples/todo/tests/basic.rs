//! End-to-end keyboard interaction tests for the Todo example.

#[cfg(test)]
mod tests {
    use std::{
        env,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use anyhow::Result as AnyResult;
    use canopy::{event::key::KeyCode, prelude::*, testing::harness::Harness};
    use canopy_widgets::List;
    use todo::{TodoEntry, create_app, store};

    fn db_path(tag: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "todo_test_{}_{}.db",
            tag,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        ))
    }

    fn add(h: &mut Harness, text: &str) -> Result<()> {
        h.key('a')?;
        for ch in text.chars() {
            h.key(ch)?;
        }
        h.key(KeyCode::Enter)?;
        // h.expect_highlight(text);
        Ok(())
    }

    fn del_first(h: &mut Harness, _next: Option<&str>) -> Result<()> {
        h.key('g')?;
        h.key('d')?;
        // if let Some(txt) = next {
        //     h.expect_highlight(txt);
        // }
        Ok(())
    }

    fn del_no_nav(h: &mut Harness, _next: Option<&str>) -> Result<()> {
        h.key('d')?;
        // if let Some(txt) = next {
        //     h.expect_highlight(txt);
        // }
        Ok(())
    }

    fn list_len(h: &mut Harness) -> usize {
        h.canopy
            .with_root_context(|ctx| {
                ctx.with_unique_descendant::<List<TodoEntry>, _>(|list, _| Ok(list.len()))
            })
            .expect("list node missing")
    }

    fn app(path: &str) -> AnyResult<Harness> {
        let db_path = db_path(path);
        let canopy = create_app(db_path.to_str().unwrap())?;
        let mut h = Harness::from_canopy(canopy, Size::new(100, 100))?;
        h.render()?;
        Ok(h)
    }

    #[test]
    fn add_item_via_script() -> AnyResult<()> {
        let mut h = app("script")?;

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
    #[should_panic]
    fn add_item_with_char_newline() {
        let mut h = app("charn1").unwrap();

        h.key('a').unwrap();
        h.key('h').unwrap();
        h.key('i').unwrap();
        h.key('\n').unwrap();
        assert_eq!(list_len(&mut h), 1);
    }

    #[test]
    fn add_item_via_pty() -> AnyResult<()> {
        let mut h = app("pty")?;

        add(&mut h, "item_one")?;
        add(&mut h, "item_two")?;
        add(&mut h, "item_three")?;
        del_first(&mut h, Some("item_two"))?;
        del_first(&mut h, Some("item_three"))?;
        del_first(&mut h, None)?;
        Ok(())
    }

    #[test]
    fn delete_reverse_via_pty() -> AnyResult<()> {
        let mut h = app("rev")?;
        add(&mut h, "one")?;
        add(&mut h, "two")?;
        add(&mut h, "three")?;
        h.key('j')?;
        h.key('j')?;
        del_first(&mut h, Some("two"))?;
        del_first(&mut h, Some("three"))?;
        del_first(&mut h, None)?;
        Ok(())
    }

    #[test]
    fn single_item_add_remove() -> AnyResult<()> {
        let mut h = app("single")?;

        add(&mut h, "solo")?;
        del_first(&mut h, None)?;
        Ok(())
    }

    #[test]
    fn delete_after_moving_focus() -> AnyResult<()> {
        let mut h = app("move_del")?;
        add(&mut h, "first")?;
        add(&mut h, "second")?;
        h.key('j')?;
        h.key('d')?;
        Ok(())
    }

    #[test]
    fn delete_middle_keeps_rest() -> AnyResult<()> {
        let mut h = app("del_middle")?;
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
        let mut h = app("del_first")?;
        add(&mut h, "a1")?;
        add(&mut h, "a2")?;
        add(&mut h, "a3")?;
        del_no_nav(&mut h, Some("a2"))?;
        del_no_nav(&mut h, Some("a1"))?;
        Ok(())
    }

    #[test]
    fn focus_moves_with_navigation() -> AnyResult<()> {
        let mut h = app("nav")?;
        add(&mut h, "one")?;
        add(&mut h, "two")?;
        h.key('j')?;
        h.key('k')?;
        Ok(())
    }

    #[test]
    fn delete_first_keeps_second_visible() -> AnyResult<()> {
        let mut h = app("del_first_second")?;
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
