//! End-to-end keyboard interaction tests for the Todo example.

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use anyhow::Result as AnyResult;
    use canopy::{event::key::KeyCode, prelude::*, testing::harness::Harness};
    use canopy_widgets::{Input, List};
    use todo::{TodoEntry, create_app_with_store, store::Store};

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
    fn app() -> AnyResult<(Harness, Store)> {
        let store = Store::open(":memory:")?;
        let canopy = create_app_with_store(store.clone(), None)?;
        let mut h = Harness::from_canopy(canopy, Size::new(100, 100))?;
        h.render()?;
        Ok((h, store))
    }

    #[test]
    fn add_item_via_script() -> AnyResult<()> {
        let (mut h, store) = app()?;

        h.key('a')?;
        h.key('h')?;
        h.key('i')?;
        h.key(KeyCode::Enter)?;
        assert_eq!(list_len(&mut h), 1);
        let todos = store.todos()?;
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].item.trim(), "hi");
        Ok(())
    }

    #[test]
    fn add_item_with_char_newline() -> AnyResult<()> {
        let (mut h, _store) = app()?;

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
        let (mut h, _store) = app()?;

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
        let (mut h, _store) = app()?;

        add(&mut h, "solo")?;
        assert_eq!(list_len(&mut h), 1);
        del_first(&mut h)?;
        assert_eq!(list_len(&mut h), 0);
        Ok(())
    }

    #[test]
    fn delete_after_moving_focus() -> AnyResult<()> {
        let (mut h, _store) = app()?;
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
        let (mut h, _store) = app()?;
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
        let (mut h, _store) = app()?;
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
        let (mut h, store) = app()?;
        add(&mut h, "one")?;
        add(&mut h, "two")?;
        // A step down and back up returns the selection to where it started, so the
        // delete removes the same item it would have without navigating.
        h.key('j')?;
        h.key('k')?;
        h.key('d')?;
        assert_eq!(list_len(&mut h), 1);
        let todos = store.todos()?;
        assert_eq!(todos.len(), 1);
        assert!(todos[0].item.contains("two"));
        Ok(())
    }

    #[test]
    fn delete_first_keeps_second_visible() -> AnyResult<()> {
        let (mut h, store) = app()?;
        add(&mut h, "first")?;
        add(&mut h, "second")?;
        h.key('g')?; // Go to first item
        h.key('d')?; // Delete first item

        // After deletion, we still have one item
        assert_eq!(list_len(&mut h), 1);

        // Check that the database still has the right item
        let todos = store.todos()?;
        assert_eq!(todos.len(), 1);
        assert!(todos[0].item.contains("second"));
        Ok(())
    }

    fn records(store: &Store) -> AnyResult<Vec<(i64, String)>> {
        Ok(store.todos()?.into_iter().map(|row| (row.id, row.item)).collect())
    }

    fn list_state(h: &mut Harness) -> Result<(usize, Option<usize>)> {
        h.canopy.with_root_context(|ctx| {
            ctx.with_unique_descendant::<List<TodoEntry>, _>(|list, _| {
                Ok((list.len(), list.selected_index()))
            })
        })
    }

    fn modal_focused(h: &Harness) -> bool {
        h.canopy.with_root_view(|ctx| {
            ctx.focused_node().is_some_and(|node| {
                ctx.node_type_id(node) == Some(TypeId::of::<Input>())
            })
        })
    }

    #[test]
    fn two_apps_keep_commands_and_fixtures_isolated() -> AnyResult<()> {
        let (mut first, first_store) = app()?;
        let (mut second, second_store) = app()?;

        add(&mut first, "first app")?;
        add(&mut second, "second app")?;
        add(&mut first, "first extra")?;
        del_first(&mut second)?;
        assert_eq!(records(&first_store)?.iter().map(|(_, text)| text.as_str()).collect::<Vec<_>>(), ["first app", "first extra"]);
        assert!(records(&second_store)?.is_empty());
        assert_eq!(list_state(&mut first)?, (2, Some(1)));
        assert_eq!(list_state(&mut second)?, (0, None));
        assert!(first.tbuf().contains_text("first app"));
        assert!(first.tbuf().contains_text("first extra"));
        assert!(!modal_focused(&first));
        assert!(!modal_focused(&second));

        // Apply every fixture to both apps while preserving the other app's state.
        for fixture in ["modal_open", "empty", "with_items"] {
            let second_rows = records(&second_store)?;
            let second_list = list_state(&mut second)?;
            let second_modal = modal_focused(&second);
            first.canopy.apply_fixture(fixture)?;
            first.render()?;
            assert_eq!(records(&second_store)?, second_rows);
            assert_eq!(list_state(&mut second)?, second_list);
            assert_eq!(modal_focused(&second), second_modal);
            let count = if fixture == "empty" { 0 } else { 3 };
            assert_eq!(first_store.todos()?.len(), count);
            assert_eq!(list_state(&mut first)?, (count, (count != 0).then_some(0)));
            assert_eq!(modal_focused(&first), fixture == "modal_open");

            let first_rows = records(&first_store)?;
            let first_list = list_state(&mut first)?;
            let first_modal = modal_focused(&first);
            second.canopy.apply_fixture(fixture)?;
            second.render()?;
            assert_eq!(records(&first_store)?, first_rows);
            assert_eq!(list_state(&mut first)?, first_list);
            assert_eq!(modal_focused(&first), first_modal);
            assert_eq!(records(&second_store)?, first_rows);
            assert_eq!(list_state(&mut second)?, first_list);
            assert_eq!(modal_focused(&second), fixture == "modal_open");
        }

        first.key('a')?;
        first.key('x')?;
        second.canopy.apply_fixture("empty")?;
        second.render()?;
        assert!(modal_focused(&first));
        assert!(!modal_focused(&second));
        first.key(KeyCode::Enter)?;
        assert_eq!(first_store.todos()?.last().unwrap().item, "x");
        assert_eq!(list_state(&mut first)?, (4, Some(3)));
        assert!(second_store.todos()?.is_empty());
        assert_eq!(list_state(&mut second)?, (0, None));
        Ok(())
    }

}
