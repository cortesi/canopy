use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use canopy::{
    error::Result as CanopyResult, event::key::KeyCode, testing::harness::Harness,
    widgets::list::List,
};
use todo::{Todo, TodoItem, open_store, setup_app};

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

fn add(h: &mut Harness, text: &str) -> CanopyResult<()> {
    h.key('a')?;
    for ch in text.chars() {
        h.key(ch)?;
    }
    h.key(KeyCode::Enter)?;
    // h.expect_highlight(text);
    Ok(())
}

fn del_first(h: &mut Harness, _next: Option<&str>) -> CanopyResult<()> {
    h.key('g')?;
    h.key('d')?;
    // if let Some(txt) = next {
    //     h.expect_highlight(txt);
    // }
    Ok(())
}

fn del_no_nav(h: &mut Harness, _next: Option<&str>) -> CanopyResult<()> {
    h.key('d')?;
    // if let Some(txt) = next {
    //     h.expect_highlight(txt);
    // }
    Ok(())
}

fn list_len(h: &mut Harness) -> usize {
    let list_id = h
        .canopy
        .core
        .nodes
        .iter()
        .find(|(_, node)| node.name == "list")
        .map(|(id, _)| id)
        .expect("list node not found");
    h.with_widget(list_id, |list: &mut List<TodoItem>| list.len())
}

fn app(path: &str) -> Result<Harness> {
    open_store(db_path(path).to_str().unwrap())?;
    let mut h = Harness::new(Todo::new()?)?;
    setup_app(&mut h.canopy);
    h.render()?;
    Ok(h)
}

#[test]
#[ignore]
fn add_item_via_script() -> Result<()> {
    let mut h = app("script")?;

    h.key('a')?;
    h.key('h')?;
    h.key('i')?;
    use canopy::event::key::KeyCode;
    h.key(KeyCode::Enter)?;
    assert_eq!(list_len(&mut h), 1);
    let todos = todo::store::get().todos().unwrap();
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
#[ignore]
fn add_item_via_pty() -> Result<()> {
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
#[ignore]
fn delete_reverse_via_pty() -> Result<()> {
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
#[ignore]
fn single_item_add_remove() -> Result<()> {
    let mut h = app("single")?;

    add(&mut h, "solo")?;
    del_first(&mut h, None)?;
    Ok(())
}

#[test]
#[ignore]
fn delete_after_moving_focus() -> Result<()> {
    let mut h = app("move_del")?;
    add(&mut h, "first")?;
    add(&mut h, "second")?;
    h.key('j')?;
    h.key('d')?;
    Ok(())
}

#[test]
#[ignore]
fn delete_middle_keeps_rest() -> Result<()> {
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
#[ignore]
fn delete_first_without_nav() -> Result<()> {
    let mut h = app("del_first")?;
    add(&mut h, "a1")?;
    add(&mut h, "a2")?;
    add(&mut h, "a3")?;
    del_no_nav(&mut h, Some("a2"))?;
    del_no_nav(&mut h, Some("a1"))?;
    Ok(())
}

#[test]
#[ignore]
fn focus_moves_with_navigation() -> Result<()> {
    let mut h = app("nav")?;
    add(&mut h, "one")?;
    add(&mut h, "two")?;
    h.key('j')?;
    h.key('k')?;
    Ok(())
}

#[test]
#[ignore]
fn delete_first_keeps_second_visible() -> Result<()> {
    let mut h = app("del_first_second")?;
    add(&mut h, "first")?;
    add(&mut h, "second")?;
    h.key('g')?; // Go to first item
    h.key('d')?; // Delete first item

    // After deletion, we still have one item
    assert_eq!(list_len(&mut h), 1);

    // Check that the database still has the right item
    let todos = todo::store::get().todos()?;
    assert_eq!(todos.len(), 1);
    assert!(todos[0].item.contains("second"));
    Ok(())
}
