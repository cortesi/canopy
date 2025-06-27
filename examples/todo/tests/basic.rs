use anyhow::Result;
use canopy::tutils::{run_root, run_root_with_size};
use todo::{bind_keys, open_store, style, Todo};

use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
