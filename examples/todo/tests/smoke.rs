use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use canopy_mcp::{Error as McpError, SuiteConfig, run_suite};
use todo::create_app;

fn db_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "todo_smoke_{}_{}.db",
        tag,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_millis(),
    ))
}

#[test]
fn luau_smoke_suite_passes() -> Result<()> {
    let db_path = db_path("suite");
    let suite_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("smoke");
    let result = run_suite(
        {
            let db_path = db_path.clone();
            move || {
                if db_path.exists() {
                    fs::remove_file(&db_path)?;
                }
                create_app(db_path.to_str().expect("utf-8 db path")).map_err(McpError::app)
            }
        },
        &SuiteConfig::new(suite_dir),
    )?;
    assert!(result.success(), "{result:#?}");
    Ok(())
}
