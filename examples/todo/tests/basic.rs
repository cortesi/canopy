use anyhow::Result;
use canopy::tutils::{run_root, run_root_with_size};
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

fn add(
    h: &mut canopy::tutils::Harness<'_>,
    tr: &mut canopy::backend::test::TestRender,
    root: &mut Todo,
    text: &str,
) -> canopy::Result<()> {
    use canopy::event::key::KeyCode;
    h.key_timeout(root, 'a', TIMEOUT)?;
    h.render_timeout(tr, root, TIMEOUT)?;
    for ch in text.chars() {
        h.key_timeout(root, ch, TIMEOUT)?;
    }
    h.key_timeout(root, KeyCode::Enter, TIMEOUT)?;
    h.render_timeout(tr, root, TIMEOUT)?;
    Ok(())
}

fn del_first(
    h: &mut canopy::tutils::Harness<'_>,
    tr: &mut canopy::backend::test::TestRender,
    root: &mut Todo,
    expected_next: Option<&str>,
) -> canopy::Result<()> {
    h.key_timeout(root, 'g', TIMEOUT)?;
    h.key_timeout(root, 'd', TIMEOUT)?;
    h.render_timeout(tr, root, TIMEOUT)?;
    Ok(())
}

fn del_no_nav(
    h: &mut canopy::tutils::Harness<'_>,
    tr: &mut canopy::backend::test::TestRender,
    root: &mut Todo,
    expected_next: Option<&str>,
) -> canopy::Result<()> {
    h.key_timeout(root, 'd', TIMEOUT)?;
    h.render_timeout(tr, root, TIMEOUT)?;
    Ok(())
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
    let path = db_path("pty");
    open_store(path.to_str().unwrap()).unwrap();
    run_root(Todo::new().unwrap(), |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, TIMEOUT)?;
        add(h, tr, root, "item_one")?;
        add(h, tr, root, "item_two")?;
        add(h, tr, root, "item_three")?;
        del_first(h, tr, root, Some("item_two"))?;
        del_first(h, tr, root, Some("item_three"))?;
        del_first(h, tr, root, None)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_reverse_via_pty() {
    let path = db_path("rev");
    open_store(path.to_str().unwrap()).unwrap();
    run_root(Todo::new().unwrap(), |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, TIMEOUT)?;
        add(h, tr, root, "one")?;
        add(h, tr, root, "two")?;
        add(h, tr, root, "three")?;
        h.key_timeout(root, 'j', TIMEOUT)?;
        h.key_timeout(root, 'j', TIMEOUT)?;
        h.render_timeout(tr, root, TIMEOUT)?;
        del_first(h, tr, root, Some("two"))?;
        del_first(h, tr, root, Some("one"))?;
        del_first(h, tr, root, None)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn single_item_add_remove() {
    let path = db_path("single");
    open_store(path.to_str().unwrap()).unwrap();
    run_root(Todo::new().unwrap(), |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, TIMEOUT)?;
        add(h, tr, root, "solo")?;
        del_first(h, tr, root, None)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_after_moving_focus() {
    let path = db_path("move_del");
    open_store(path.to_str().unwrap()).unwrap();
    run_root(Todo::new().unwrap(), |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, TIMEOUT)?;
        add(h, tr, root, "first")?;
        add(h, tr, root, "second")?;
        h.key_timeout(root, 'j', TIMEOUT)?;
        h.render_timeout(tr, root, TIMEOUT)?;
        h.key_timeout(root, 'd', TIMEOUT)?;
        h.render_timeout(tr, root, TIMEOUT)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_middle_keeps_rest() {
    let path = db_path("del_middle");
    open_store(path.to_str().unwrap()).unwrap();
    run_root(Todo::new().unwrap(), |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, TIMEOUT)?;
        add(h, tr, root, "first")?;
        add(h, tr, root, "second")?;
        add(h, tr, root, "third")?;
        h.key_timeout(root, 'j', TIMEOUT)?;
        h.render_timeout(tr, root, TIMEOUT)?;
        h.key_timeout(root, 'j', TIMEOUT)?;
        h.render_timeout(tr, root, TIMEOUT)?;
        h.key_timeout(root, 'd', TIMEOUT)?;
        h.render_timeout(tr, root, TIMEOUT)?;
        assert_eq!(root.content.child.len(), 2);
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_first_without_nav() {
    let path = db_path("del_first");
    open_store(path.to_str().unwrap()).unwrap();
    run_root(Todo::new().unwrap(), |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, TIMEOUT)?;
        add(h, tr, root, "a1")?;
        add(h, tr, root, "a2")?;
        add(h, tr, root, "a3")?;
        del_no_nav(h, tr, root, Some("a2"))?;
        del_no_nav(h, tr, root, Some("a3"))?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn focus_moves_with_navigation() {
    let path = db_path("nav");
    open_store(path.to_str().unwrap()).unwrap();
    run_root(Todo::new().unwrap(), |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, TIMEOUT)?;
        add(h, tr, root, "one")?;
        add(h, tr, root, "two")?;
        h.key_timeout(root, 'j', TIMEOUT)?;
        h.render_timeout(tr, root, TIMEOUT)?;
        h.key_timeout(root, 'k', TIMEOUT)?;
        h.render_timeout(tr, root, TIMEOUT)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn delete_first_keeps_second_visible() {
    let path = db_path("del_first_second");
    open_store(path.to_str().unwrap()).unwrap();
    run_root(Todo::new().unwrap(), |h, tr, root| {
        style(h.canopy());
        bind_keys(h.canopy());
        h.render_timeout(tr, root, TIMEOUT)?;
        add(h, tr, root, "first")?;
        add(h, tr, root, "second")?;
        h.key_timeout(root, 'd', TIMEOUT)?;
        h.render_timeout(tr, root, TIMEOUT)?;
        Ok(())
    })
    .unwrap();
}
