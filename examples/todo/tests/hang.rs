use canopy::tutils::spawn_workspace_bin;
use std::time::Duration;
use todo::open_store;

#[test]
#[should_panic]
fn add_item_via_pty() {
    let db_path = std::env::temp_dir().join(format!(
        "todo_test_pty_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    ));
    open_store(db_path.to_str().unwrap()).unwrap();

    let mut app = spawn_workspace_bin("todo", &[db_path.to_str().unwrap()]).unwrap();
    app.expect("todo", Duration::from_millis(100)).ok();

    app.send("a").unwrap();
    app.send("hi").unwrap();
    app.send_line("").unwrap();
    app.send("q").unwrap();

    app.wait_eof(Duration::from_secs(2)).unwrap();
}
