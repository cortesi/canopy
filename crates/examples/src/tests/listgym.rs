use canopy::{error::Result, prelude::*, testing::harness::Harness};
use canopy_widgets::{List, Panes};

use crate::listgym::{ListEntry, ListGym};

fn list_len(harness: &mut Harness) -> Result<usize> {
    harness.with_root_context(|_root: &mut ListGym, ctx| {
        ctx.with_unique_descendant::<List<ListEntry>, _>(|list, _| Ok(list.len()))
    })
}

fn list_selected_index(harness: &mut Harness) -> Result<Option<usize>> {
    harness.with_root_context(|_root: &mut ListGym, ctx| {
        ctx.with_unique_descendant::<List<ListEntry>, _>(|list, _| Ok(list.selected_index()))
    })
}

fn panes_column_count(harness: &mut Harness) -> Result<usize> {
    harness.with_root_context(|_root: &mut ListGym, ctx| {
        ctx.with_unique_descendant::<Panes, _>(|_panes, ctx| Ok(ctx.children().len()))
    })
}

fn list_count(harness: &mut Harness) -> Result<usize> {
    harness.with_root_context(|_root: &mut ListGym, ctx| {
        let view = ctx as &dyn canopy::ViewContext;
        Ok(view.all_in_tree::<List<ListEntry>>().len())
    })
}

fn focused_list_index(harness: &mut Harness) -> Result<Option<usize>> {
    harness.with_root_context(|_root: &mut ListGym, ctx| {
        let focused = (ctx as &dyn ViewContext).focused_descendant::<List<ListEntry>>();
        let lists = (ctx as &dyn ViewContext).descendants_of_type::<List<ListEntry>>();
        Ok(focused.and_then(|focused_id| {
            let focused_id = canopy::NodeId::from(focused_id);
            lists
                .iter()
                .position(|id| canopy::NodeId::from(*id) == focused_id)
        }))
    })
}

#[test]
fn test_listgym_initial_state() -> Result<()> {
    let root = ListGym::new();
    let mut harness = Harness::new(root)?;
    harness.render()?;

    let len = list_len(&mut harness)?;
    assert_eq!(len, 10);

    Ok(())
}

#[test]
fn test_harness_script_with_list_navigation() -> Result<()> {
    let mut harness = Harness::new(ListGym::new())?;
    harness.render()?;

    let initial_selected = list_selected_index(&mut harness)?;

    // Navigate using list commands (these are loaded by the List type).
    harness.script("list.select_last()")?;

    let selected = list_selected_index(&mut harness)?;

    assert!(selected > initial_selected);

    Ok(())
}

#[test]
fn test_listgym_adds_and_deletes_columns() -> Result<()> {
    let mut harness = Harness::new(ListGym::new())?;
    harness.render()?;

    let initial_cols = panes_column_count(&mut harness)?;

    harness.script("list_gym.add_column()")?;
    let after_add = panes_column_count(&mut harness)?;
    assert_eq!(after_add, initial_cols + 1);

    harness.script("list_gym.delete_column()")?;
    let after_delete = panes_column_count(&mut harness)?;
    assert_eq!(after_delete, initial_cols);

    Ok(())
}

#[test]
fn test_listgym_add_item_command() -> Result<()> {
    let mut harness = Harness::new(ListGym::new())?;
    harness.render()?;

    let before = list_len(&mut harness)?;
    harness.script("list_gym.add_item()")?;
    assert_eq!(list_len(&mut harness)?, before + 1);

    harness.script("list_gym.add_column()")?;
    harness.script("list_gym.add_item()")?;

    Ok(())
}

#[test]
fn test_listgym_tabs_between_columns() -> Result<()> {
    let mut harness = Harness::new(ListGym::new())?;
    harness.render()?;

    harness.script("list_gym.add_column()")?;
    let lists = list_count(&mut harness)?;
    assert_eq!(lists, 2);
    assert_eq!(focused_list_index(&mut harness)?, Some(1));

    harness.script("panes.focus_column(1)")?;
    assert_eq!(focused_list_index(&mut harness)?, Some(0));

    harness.script("panes.focus_column(-1)")?;
    assert_eq!(focused_list_index(&mut harness)?, Some(1));

    Ok(())
}
