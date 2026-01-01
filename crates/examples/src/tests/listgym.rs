use canopy::{Loader, NodeId, error::Result, testing::harness::Harness, widgets::List};

use crate::listgym::{ListEntry, ListGym};

fn panes_id(harness: &Harness) -> NodeId {
    harness
        .find_node("list_gym/*/panes")
        .expect("panes not initialized")
}

fn list_id(harness: &Harness) -> NodeId {
    harness
        .find_node("list_gym/*/frame/list")
        .expect("list not initialized")
}

fn column_list_ids(harness: &Harness) -> Vec<NodeId> {
    harness.find_nodes("list_gym/*/frame/list")
}

fn create_test_harness() -> Result<Harness> {
    let root = ListGym::new();
    let mut harness = Harness::new(root)?;

    // Load the commands so scripts can find them.
    ListGym::load(&mut harness.canopy);

    Ok(harness)
}

#[test]
fn test_listgym_creates_and_renders() -> Result<()> {
    let root = ListGym::new();
    let mut harness = Harness::new(root)?;

    // Test that we can render without crashing.
    harness.render()?;

    Ok(())
}

#[test]
fn test_listgym_initial_state() -> Result<()> {
    let root = ListGym::new();
    let mut harness = Harness::new(root)?;
    harness.render()?;

    let list_node = list_id(&harness);
    let mut len = 0;
    harness.with_widget(list_node, |list: &mut List<ListEntry>| {
        len = list.len();
    });

    assert_eq!(len, 10);

    Ok(())
}

#[test]
fn test_listgym_with_harness() -> Result<()> {
    let mut harness = Harness::builder(ListGym::new()).size(80, 20).build()?;

    // Test that we can render with a specific size.
    harness.render()?;

    // The harness should have created a render buffer.
    let _buf = harness.buf();

    Ok(())
}

#[test]
fn test_harness_script_method() -> Result<()> {
    let mut harness = create_test_harness()?;
    harness.render()?;

    // Test that we can execute a simple print script.
    harness.script("print(\"Hello from script\")")?;

    Ok(())
}

#[test]
fn test_harness_script_with_list_navigation() -> Result<()> {
    let mut harness = create_test_harness()?;
    harness.render()?;

    let list_node = list_id(&harness);
    let mut initial_selected = None;
    harness.with_widget(list_node, |list: &mut List<ListEntry>| {
        initial_selected = list.selected_index();
    });

    // Navigate using list commands (these are loaded by the List type).
    harness.script("list::select_last()")?;

    let mut selected = None;
    harness.with_widget(list_node, |list: &mut List<ListEntry>| {
        selected = list.selected_index();
    });

    assert!(selected > initial_selected);

    Ok(())
}

#[test]
fn test_listgym_adds_and_deletes_columns() -> Result<()> {
    let mut harness = create_test_harness()?;
    harness.render()?;

    let panes_id = panes_id(&harness);
    let initial_cols = harness
        .canopy
        .core
        .node(panes_id)
        .expect("panes node missing")
        .children()
        .len();

    harness.script("list_gym::add_column()")?;
    let after_add = harness
        .canopy
        .core
        .node(panes_id)
        .expect("panes node missing")
        .children()
        .len();
    assert_eq!(after_add, initial_cols + 1);

    harness.script("list_gym::delete_column()")?;
    let after_delete = harness
        .canopy
        .core
        .node(panes_id)
        .expect("panes node missing")
        .children()
        .len();
    assert_eq!(after_delete, initial_cols);

    Ok(())
}

#[test]
fn test_listgym_add_item_command() -> Result<()> {
    let mut harness = create_test_harness()?;
    harness.render()?;

    harness.script("list_gym::add_item()")?;
    harness.script("list_gym::add_column()")?;
    harness.script("list_gym::add_item()")?;

    Ok(())
}

#[test]
fn test_listgym_tabs_between_columns() -> Result<()> {
    let mut harness = create_test_harness()?;
    harness.render()?;

    harness.script("list_gym::add_column()")?;
    let lists = column_list_ids(&harness);
    assert_eq!(lists.len(), 2);
    assert!(harness.canopy.core.is_on_focus_path(lists[1]));

    harness.script("list_gym::next_column()")?;
    assert!(harness.canopy.core.is_on_focus_path(lists[0]));

    harness.script("list_gym::prev_column()")?;
    assert!(harness.canopy.core.is_on_focus_path(lists[1]));

    Ok(())
}
