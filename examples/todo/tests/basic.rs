use anyhow::Result;
use canopy::tutils::run_root;
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
        h.render(tr, root)?;
        h.key(root, 'a')?;
        Ok(())
    })?;
    Ok(())
}
