use canopy::tutils::{spawn_bin, Eof};
use std::time::Duration;

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

    let mut p = spawn_bin("todo", &[db_path.to_str().unwrap()]).expect("spawn");

    p.set_expect_timeout(Some(Duration::from_millis(100)));
    let _ = p.expect("todo");

    p.send("a").expect("send a");
    p.send("hi").expect("send hi");
    p.send_line("").expect("send enter");
    p.send("q").expect("send q");

    p.set_expect_timeout(Some(Duration::from_secs(2)));
    p.expect(Eof).expect("process eof");
}
