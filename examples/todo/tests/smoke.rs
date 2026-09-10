//! End-to-end Luau smoke-suite test for the Todo example.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use anyhow::Result;
    use canopy_mcp::{
        AppFactory, AppMetadata, Error as McpError, ResetPolicy, SuiteConfig, run_suite,
    };
    use todo::{create_app, store::Store};

    #[test]
    fn luau_smoke_suite_passes() -> Result<()> {
        let suite_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("smoke");
        let result = run_suite(
            &AppFactory::new(
                AppMetadata {
                    app: "todo".into(),
                    reset: ResetPolicy::Isolated,
                },
                || {
                    let store = Store::open(":memory:").map_err(McpError::app)?;
                    create_app(store, None).map_err(McpError::app)
                },
            ),
            &SuiteConfig::new(suite_dir),
        )?;
        assert!(result.success(), "{result:#?}");
        assert!(
            result
                .scripts
                .iter()
                .all(|script| script.outcome.metadata.app == "todo"
                    && script.outcome.metadata.reset == ResetPolicy::Isolated)
        );
        Ok(())
    }
}
