use anyhow::Result;
use canopy::tutils::run_root;
use canopy::tutils::run_root_with_size;
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
use canopy::tutils::spawn_workspace_bin;

#[test]
#[ignore]
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

    let items = ["one", "two", "three"];
    for item in items.iter() {
        app.send("a").unwrap();
        app.send(item).unwrap();
        app.send_line("").unwrap();
        app.expect(item, Duration::from_millis(500)).unwrap();
    }

    for _ in 0..items.len() {
        app.send("d").unwrap();
    }

    app.send("q").unwrap();
    app.wait_eof(Duration::from_secs(2)).unwrap();

    open_store(db_path.to_str().unwrap()).unwrap();
    assert!(todo::store::get().todos().unwrap().is_empty());
}


#[test]
fn add_delete_via_harness() -> Result<()> {
    let path = std::env::temp_dir().join(format!(
        "todo_test_roundtrip_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    ));
    open_store(path.to_str().unwrap()).unwrap();
    run_root(Todo::new()?, |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, Duration::from_secs(1))?;
        for text in ["one", "two", "three"] {
            h.key_timeout(root, 'a', Duration::from_secs(1))?;
            h.render_timeout(tr, root, Duration::from_secs(1))?;
            for ch in text.chars() {
                h.key_timeout(root, ch, Duration::from_secs(1))?;
            }
            use canopy::event::key::KeyCode;
            h.key_timeout(root, KeyCode::Enter, Duration::from_secs(1))?;
            h.render_timeout(tr, root, Duration::from_secs(1))?;
            assert!(tr.contains_text(text));
        }
        for text in ["three", "two", "one"] {
            h.key_timeout(root, 'd', Duration::from_secs(1))?;
            h.render_timeout(tr, root, Duration::from_secs(1))?;
            assert!(!tr.contains_text(text));
        }
        Ok(())
    })?;
    open_store(path.to_str().unwrap()).unwrap();
    assert!(todo::store::get().todos().unwrap().is_empty());
    Ok(())
}

