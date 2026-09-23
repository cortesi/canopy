//! The gyms' Luau smoke suite, run in-process.

use std::path::PathBuf;

use canopy::error::{Error, Result};
use canopy_mcp::{AppFactory, AppMetadata, Error as McpError, ResetPolicy, SuiteConfig, run_suite};

use crate::Demo;

#[test]
fn luau_smoke_suite_passes() -> Result<()> {
    let suite_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("smoke");
    let factory = AppFactory::new(
        AppMetadata {
            app: "gyms".into(),
            reset: ResetPolicy::Isolated,
        },
        || Demo::Listgym.build(false).map_err(McpError::app),
    );
    let result = run_suite(&factory, &SuiteConfig::new(suite_dir))
        .map_err(|error| Error::App(Box::new(error)))?;
    assert!(result.success(), "{result:#?}");
    assert_eq!(result.scripts.len(), 1, "all checked-in smoke scripts ran");
    Ok(())
}
