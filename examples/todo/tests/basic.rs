use anyhow::Result;
use canopy::tutils::{run_root, run_root_with_size, spawn_workspace_bin};
use todo::{bind_keys, open_store, style, Todo};

use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TIMEOUT: Duration = Duration::from_millis(100);

fn db_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "todo_test_{}_{}.db",
        tag,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    ))
}

fn spawn_app(tag: &str) -> canopy::tutils::PtyApp {
    let path = db_path(tag);
    open_store(path.to_str().unwrap()).unwrap();
    let mut app = spawn_workspace_bin("todo", &[path.to_str().unwrap()]).unwrap();
    // Give cargo run more time to start up
    app.expect("todo", TIMEOUT).ok();
    app
}

fn quit(mut app: canopy::tutils::PtyApp) {
    app.send("q").unwrap();
    app.wait_eof(TIMEOUT).unwrap();
}

fn expect_highlight(app: &mut canopy::tutils::PtyApp, text: &str) {
    app.expect(text, TIMEOUT).unwrap();
    app.expect("\x1b[38;", TIMEOUT).unwrap();
}

fn add(app: &mut canopy::tutils::PtyApp, text: &str) {
    app.send("a").unwrap();
    app.send(text).unwrap();
    app.send("\r").unwrap();
    expect_highlight(app, text);
}

fn del_first(app: &mut canopy::tutils::PtyApp, expected_next: Option<&str>) {
    app.send("g").unwrap();
    app.send("d").unwrap();
    if let Some(txt) = expected_next {
        expect_highlight(app, txt);
    }
}

fn del_no_nav(app: &mut canopy::tutils::PtyApp, expected_next: Option<&str>) {
    app.send("d").unwrap();
    if let Some(txt) = expected_next {
        expect_highlight(app, txt);
    }
}

#[test]
fn add_item_via_script() -> Result<()> {
    let path = db_path("script");
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
    let path = db_path("seed");
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
    let path = db_path("charnl");
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
    let mut app = spawn_app("pty");

    add(&mut app, "item_one");
    add(&mut app, "item_two");
    add(&mut app, "item_three");

    del_first(&mut app, Some("item_two"));
    del_first(&mut app, Some("item_three"));
    del_first(&mut app, None);

    // App should still respond after deleting the last item
    quit(app);
}

#[test]
fn delete_reverse_via_pty() {
    let mut app = spawn_app("rev");

    add(&mut app, "one");
    add(&mut app, "two");
    add(&mut app, "three");

    app.send("j").unwrap();
    app.send("j").unwrap();
    del_first(&mut app, Some("two"));
    del_first(&mut app, Some("one"));
    del_first(&mut app, None);

    quit(app);
}

#[test]
fn single_item_add_remove() {
    let mut app = spawn_app("single");

    add(&mut app, "solo");
    del_first(&mut app, None);

    quit(app);
}

#[test]
fn delete_after_moving_focus() {
    let mut app = spawn_app("move_del");

    add(&mut app, "first");
    add(&mut app, "second");

    app.send("j").unwrap();
    app.expect("second", Duration::from_secs(2)).unwrap();
    app.send("d").unwrap();
    expect_highlight(&mut app, "first");

    quit(app);
}

#[test]
fn delete_middle_keeps_rest() {
    let mut app = spawn_app("del_middle");

    add(&mut app, "first");
    add(&mut app, "second");
    add(&mut app, "third");

    app.send("j").unwrap();
    expect_highlight(&mut app, "second");
    app.send("d").unwrap();
    expect_highlight(&mut app, "first");
    app.expect("third", Duration::from_secs(2)).unwrap();

    quit(app);
}

#[test]
fn delete_first_without_nav() {
    let mut app = spawn_app("del_first");

    add(&mut app, "a1");
    add(&mut app, "a2");
    add(&mut app, "a3");

    del_no_nav(&mut app, Some("a2"));
    del_no_nav(&mut app, Some("a3"));

    quit(app);
}

#[test]
fn focus_moves_with_navigation() {
    let mut app = spawn_app("nav");

    add(&mut app, "one");
    add(&mut app, "two");

    app.send("j").unwrap();
    expect_highlight(&mut app, "two");

    app.send("k").unwrap();
    expect_highlight(&mut app, "one");

    quit(app);
}

#[test]
fn delete_first_keeps_second_visible() {
    let mut app = spawn_app("del_first_second");

    // Add two items
    app.send("a").unwrap();
    app.send("first").unwrap();
    app.send("\r").unwrap();
    expect_highlight(&mut app, "first");

    app.send("a").unwrap();
    app.send("second").unwrap();
    app.send("\r").unwrap();
    // Focus returns to first item
    expect_highlight(&mut app, "first");

    // Delete the first item without moving focus
    app.send("d").unwrap();
    expect_highlight(&mut app, "second");

    quit(app);
}
