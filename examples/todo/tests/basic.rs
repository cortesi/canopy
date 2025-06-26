use anyhow::Result;
use canopy::tutils::run_root;
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
