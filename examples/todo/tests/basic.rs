use anyhow::Result;
use canopy::tutils::{run_root, run_root_with_size, spawn_workspace_bin};
use std::time::Duration;
use todo::{bind_keys, open_store, style, Todo};

#[test]
fn add_item_via_script() -> Result<()> {
    let path = std::env::temp_dir().join(format!(
        "todo_test_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ));
    open_store(path.to_str().unwrap())?;
    run_root(Todo::new()?, |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, Duration::from_secs(1))?;
        h.key_timeout(root, 'a', Duration::from_secs(1))?;
        h.render_timeout(tr, root, Duration::from_secs(1))?;
        h.key_timeout(root, 'h', Duration::from_secs(1))?;
        h.key_timeout(root, 'i', Duration::from_secs(1))?;
        use canopy::event::key::KeyCode;
        h.key_timeout(root, KeyCode::Enter, Duration::from_secs(1))?;
        h.render_timeout(tr, root, Duration::from_secs(1))?;
        assert_eq!(root.content.child.len(), 1);
        let todos = todo::store::get().todos().unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].item.trim(), "hi");
        Ok(())
    })?;
    Ok(())
}

#[test]
fn render_seeded_item() {
    use canopy::geom::Expanse;
    let path = std::env::temp_dir().join(format!(
        "todo_test_seed_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ));
    open_store(path.to_str().unwrap()).unwrap();
    todo::store::get().add_todo("seeded").unwrap();
    run_root_with_size(Todo::new().unwrap(), Expanse::new(20, 5), |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, Duration::from_secs(1)).unwrap();
        assert!(tr.contains_text("seeded"));
        Ok(())
    })
    .unwrap();
}

#[test]
#[should_panic]
fn add_item_with_char_newline() {
    let path = std::env::temp_dir().join(format!(
        "todo_test_charnl_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    ));
    open_store(path.to_str().unwrap()).unwrap();
    run_root(Todo::new().unwrap(), |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, Duration::from_secs(1)).unwrap();
        h.key_timeout(root, 'a', Duration::from_secs(1)).unwrap();
        h.render_timeout(tr, root, Duration::from_secs(1)).unwrap();
        h.key_timeout(root, 'h', Duration::from_secs(1)).unwrap();
        h.key_timeout(root, 'i', Duration::from_secs(1)).unwrap();
        h.key_timeout(root, '\n', Duration::from_secs(1)).unwrap();
        h.render_timeout(tr, root, Duration::from_secs(1)).unwrap();
        assert_eq!(root.content.child.len(), 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn add_item_via_pty() {
    let db_path = std::env::temp_dir().join(format!(
        "todo_test_pty_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ));
    open_store(db_path.to_str().unwrap()).unwrap();

    let mut app = spawn_workspace_bin("todo", &[db_path.to_str().unwrap()]).unwrap();
    app.expect("todo", Duration::from_millis(100)).ok();

    fn add(app: &mut canopy::tutils::PtyApp, text: &str) {
        app.send("a").unwrap();
        app.send(text).unwrap();
        app.send("\r").unwrap();
        app.expect(text, Duration::from_millis(200)).unwrap();
    }

    fn del(app: &mut canopy::tutils::PtyApp, expected_next: Option<&str>) {
        app.send("g").unwrap();
        app.send("d").unwrap();
        if let Some(txt) = expected_next {
            app.expect(txt, Duration::from_millis(200)).unwrap();
        }
    }

    add(&mut app, "item_one");
    add(&mut app, "item_two");
    add(&mut app, "item_three");

    del(&mut app, Some("item_two"));
    del(&mut app, Some("item_three"));
    del(&mut app, None);

    // App should still respond after deleting the last item
    app.send("q").unwrap();
    app.wait_eof(Duration::from_secs(2)).unwrap();
}

#[test]
fn delete_reverse_via_pty() {
    let db_path = std::env::temp_dir().join(format!(
        "todo_test_rev_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    ));
    open_store(db_path.to_str().unwrap()).unwrap();

    let mut app = spawn_workspace_bin("todo", &[db_path.to_str().unwrap()]).unwrap();
    app.expect("todo", Duration::from_millis(100)).ok();

    fn add(app: &mut canopy::tutils::PtyApp, text: &str) {
        app.send("a").unwrap();
        app.send(text).unwrap();
        app.send("\r").unwrap();
        app.expect(text, Duration::from_millis(200)).unwrap();
    }

    fn del(app: &mut canopy::tutils::PtyApp, expected_next: Option<&str>) {
        app.send("g").unwrap();
        app.send("d").unwrap();
        if let Some(txt) = expected_next {
            app.expect(txt, Duration::from_millis(200)).unwrap();
        }
    }

    add(&mut app, "one");
    add(&mut app, "two");
    add(&mut app, "three");

    app.send("j").unwrap();
    app.send("j").unwrap();
    del(&mut app, Some("two"));
    del(&mut app, Some("one"));
    del(&mut app, None);

    app.send("q").unwrap();
    app.wait_eof(Duration::from_secs(2)).unwrap();
}

#[test]
fn single_item_add_remove() {
    let db_path = std::env::temp_dir().join(format!(
        "todo_test_single_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    ));
    open_store(db_path.to_str().unwrap()).unwrap();

    let mut app = spawn_workspace_bin("todo", &[db_path.to_str().unwrap()]).unwrap();
    app.expect("todo", Duration::from_millis(100)).ok();

    fn add(app: &mut canopy::tutils::PtyApp, text: &str) {
        app.send("a").unwrap();
        app.send(text).unwrap();
        app.send("\r").unwrap();
        app.expect(text, Duration::from_millis(200)).unwrap();
    }

    fn del(app: &mut canopy::tutils::PtyApp, expected_next: Option<&str>) {
        app.send("g").unwrap();
        app.send("d").unwrap();
        if let Some(txt) = expected_next {
            app.expect(txt, Duration::from_millis(200)).unwrap();
        }
    }

    add(&mut app, "solo");
    del(&mut app, None);

    app.send("q").unwrap();
    app.wait_eof(Duration::from_secs(2)).unwrap();
}
