use anyhow::Result;
use canopy::tutils::Harness;
use todo::{bind_keys, open_store, style, Todo};

use std::time::{SystemTime, UNIX_EPOCH};

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

fn add(h: &mut Harness<Todo>, text: &str) -> canopy::Result<()> {
    use canopy::event::key::KeyCode;
    h.key('a')?;
    for ch in text.chars() {
        h.key(ch)?;
    }
    h.key(KeyCode::Enter)?;
    h.expect_highlight(text);
    Ok(())
}

fn del_first(h: &mut Harness<Todo>, next: Option<&str>) -> canopy::Result<()> {
    h.key('g')?;
    h.key('d')?;
    if let Some(txt) = next {
        h.expect_highlight(txt);
    }
    Ok(())
}

fn del_no_nav(h: &mut Harness<Todo>, next: Option<&str>) -> canopy::Result<()> {
    h.key('d')?;
    if let Some(txt) = next {
        h.expect_highlight(txt);
    }
    Ok(())
}

#[test]
fn add_item_via_script() -> Result<()> {
    let path = db_path("script");
    open_store(path.to_str().unwrap())?;
    let mut h = Harness::new(Todo::new()?)?;
    style(h.canopy());
    bind_keys(h.canopy());
    h.render()?;
    h.key('a')?;
    h.key('h')?;
    h.key('i')?;
    use canopy::event::key::KeyCode;
    h.key(KeyCode::Enter)?;
    assert_eq!(h.root().content.child.len(), 1);
    let todos = todo::store::get().todos().unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].item.trim(), "hi");
    Ok(())
}

#[test]
fn render_seeded_item() {
    use canopy::geom::Expanse;
    let path = db_path("seed");
    open_store(path.to_str().unwrap()).unwrap();
    todo::store::get().add_todo("seeded").unwrap();
    let mut h = Harness::with_size(Todo::new().unwrap(), Expanse::new(20, 5)).unwrap();
    style(h.canopy());
    bind_keys(h.canopy());
    h.render().unwrap();
    h.expect_contains("seeded");
}

#[test]
#[should_panic]
fn add_item_with_char_newline() {
    let path = db_path("charnl");
    open_store(path.to_str().unwrap()).unwrap();
    let mut h = Harness::new(Todo::new().unwrap()).unwrap();
    style(h.canopy());
    bind_keys(h.canopy());
    h.render().unwrap();
    h.key('a').unwrap();
    h.key('h').unwrap();
    h.key('i').unwrap();
    h.key('\n').unwrap();
    assert_eq!(h.root().content.child.len(), 1);
}

#[test]
#[ignore]
fn add_item_via_pty() {
    let path = db_path("pty");
    open_store(path.to_str().unwrap()).unwrap();
    let mut h = Harness::new(Todo::new().unwrap()).unwrap();
    style(h.canopy());
    bind_keys(h.canopy());
    h.render().unwrap();
    add(&mut h, "item_one").unwrap();
    add(&mut h, "item_two").unwrap();
    add(&mut h, "item_three").unwrap();
    del_first(&mut h, Some("item_two")).unwrap();
    del_first(&mut h, Some("item_three")).unwrap();
    del_first(&mut h, None).unwrap();
}

#[test]
#[ignore]
fn delete_reverse_via_pty() {
    let path = db_path("rev");
    open_store(path.to_str().unwrap()).unwrap();
    let mut h = Harness::new(Todo::new().unwrap()).unwrap();
    style(h.canopy());
    bind_keys(h.canopy());
    h.render().unwrap();
    add(&mut h, "one").unwrap();
    add(&mut h, "two").unwrap();
    add(&mut h, "three").unwrap();
    h.key('j').unwrap();
    h.key('j').unwrap();
    del_first(&mut h, Some("two")).unwrap();
    del_first(&mut h, Some("three")).unwrap();
    del_first(&mut h, None).unwrap();
}

#[test]
fn single_item_add_remove() {
    let path = db_path("single");
    open_store(path.to_str().unwrap()).unwrap();
    let mut h = Harness::new(Todo::new().unwrap()).unwrap();
    style(h.canopy());
    bind_keys(h.canopy());
    h.render().unwrap();
    add(&mut h, "solo").unwrap();
    del_first(&mut h, None).unwrap();
}

#[test]
fn delete_after_moving_focus() {
    let path = db_path("move_del");
    open_store(path.to_str().unwrap()).unwrap();
    let mut h = Harness::new(Todo::new().unwrap()).unwrap();
    style(h.canopy());
    bind_keys(h.canopy());
    h.render().unwrap();
    add(&mut h, "first").unwrap();
    add(&mut h, "second").unwrap();
    h.key('j').unwrap();
    h.key('d').unwrap();
}

#[test]
fn delete_middle_keeps_rest() {
    let path = db_path("del_middle");
    open_store(path.to_str().unwrap()).unwrap();
    let mut h = Harness::new(Todo::new().unwrap()).unwrap();
    style(h.canopy());
    bind_keys(h.canopy());
    h.render().unwrap();
    add(&mut h, "first").unwrap();
    add(&mut h, "second").unwrap();
    add(&mut h, "third").unwrap();
    h.key('j').unwrap();
    h.key('j').unwrap();
    h.key('d').unwrap();
    assert_eq!(h.root().content.child.len(), 2);
}

#[test]
fn delete_first_without_nav() {
    let path = db_path("del_first");
    open_store(path.to_str().unwrap()).unwrap();
    let mut h = Harness::new(Todo::new().unwrap()).unwrap();
    style(h.canopy());
    bind_keys(h.canopy());
    h.render().unwrap();
    add(&mut h, "a1").unwrap();
    add(&mut h, "a2").unwrap();
    add(&mut h, "a3").unwrap();
    del_no_nav(&mut h, Some("a2")).unwrap();
    del_no_nav(&mut h, Some("a1")).unwrap();
}

#[test]
fn focus_moves_with_navigation() {
    let path = db_path("nav");
    open_store(path.to_str().unwrap()).unwrap();
    let mut h = Harness::new(Todo::new().unwrap()).unwrap();
    style(h.canopy());
    bind_keys(h.canopy());
    h.render().unwrap();
    add(&mut h, "one").unwrap();
    add(&mut h, "two").unwrap();
    h.key('j').unwrap();
    h.key('k').unwrap();
}

#[test]
fn delete_first_keeps_second_visible() {
    let path = db_path("del_first_second");
    open_store(path.to_str().unwrap()).unwrap();
    let mut h = Harness::new(Todo::new().unwrap()).unwrap();
    style(h.canopy());
    bind_keys(h.canopy());
    h.render().unwrap();
    add(&mut h, "first").unwrap();
    add(&mut h, "second").unwrap();
    h.key('g').unwrap(); // Go to first item
    h.key('d').unwrap(); // Delete first item

    // After deletion, we still have one item
    assert_eq!(h.root().content.child.len(), 1);

    // Check that the database still has the right item
    let todos = todo::store::get().todos().unwrap();
    assert_eq!(todos.len(), 1);
    assert!(todos[0].item.contains("second"));
}
