use anyhow::Result;
use canopy::tutils::{run_root, run_root_with_size, spawn_workspace_bin, PtyApp};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use todo::{bind_keys, open_store, style, Todo};

const WAIT: Duration = Duration::from_millis(500);

fn temp_db(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "{}_{}.db",
        name,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ))
}

fn spawn_todo(path: &std::path::Path) -> PtyApp {
    let mut app = spawn_workspace_bin("todo", &[path.to_str().unwrap()]).unwrap();
    app.expect("todo", Duration::from_millis(100)).ok();
    app
}

fn add(app: &mut PtyApp, text: &str) {
    app.send("a").unwrap();
    app.send(text).unwrap();
    app.send("\r").unwrap();
    app.expect(text, WAIT).unwrap();
    std::thread::sleep(WAIT);
}

fn assert_present(app: &mut PtyApp, text: &str) {
    app.expect(text, WAIT).unwrap();
}

fn select_first(app: &mut PtyApp) {
    app.send("g").unwrap();
}

fn select_next(app: &mut PtyApp) {
    app.send("j").unwrap();
}

fn delete_current(app: &mut PtyApp, _next: Option<&str>) {
    app.send("d").unwrap();
    std::thread::sleep(WAIT);
}

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

    let mut app = spawn_todo(&db_path);

    fn del(app: &mut PtyApp, expected_next: Option<&str>) {
        select_first(app);
        delete_current(app, expected_next);
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
    let db_path = temp_db("todo_test_rev");
    open_store(db_path.to_str().unwrap()).unwrap();

    let mut app = spawn_todo(&db_path);

    add(&mut app, "one");
    add(&mut app, "two");
    add(&mut app, "three");

    select_first(&mut app);
    select_next(&mut app);
    select_next(&mut app);
    delete_current(&mut app, Some("two"));
    assert_present(&mut app, "one");
    assert_present(&mut app, "two");

    delete_current(&mut app, Some("one"));
    assert_present(&mut app, "one");

    delete_current(&mut app, None);

    app.send("q").unwrap();
    app.wait_eof(Duration::from_secs(2)).unwrap();
}

#[test]
fn single_item_add_remove_via_pty() {
    let db_path = temp_db("todo_test_single");
    open_store(db_path.to_str().unwrap()).unwrap();

    let mut app = spawn_todo(&db_path);

    add(&mut app, "only");
    select_first(&mut app);
    delete_current(&mut app, None);

    // ensure still responsive when list empty
    app.send("j").unwrap();
    app.send("k").unwrap();
    app.send("d").unwrap();

    add(&mut app, "again");
    select_first(&mut app);
    delete_current(&mut app, None);

    app.send("q").unwrap();
    app.wait_eof(Duration::from_secs(2)).unwrap();
}
