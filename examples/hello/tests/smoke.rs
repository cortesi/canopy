//! End-to-end Luau smoke-suite test for the Hello example.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use anyhow::Result;
    use canopy_mcp::{
        AppFactory, AppMetadata, Error as McpError, ResetPolicy, SuiteConfig, run_suite,
    };

    #[test]
    fn luau_smoke_suite_passes() -> Result<()> {
        let suite_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("smoke");
        let factory = AppFactory::new(
            AppMetadata {
                app: "hello".into(),
                reset: ResetPolicy::Isolated,
            },
            // The suite never mounts a user script root, so a run cannot
            // depend on or modify developer configuration.
            || hello::create_app(None).map_err(McpError::app),
        );
        let result = run_suite(&factory, &SuiteConfig::new(suite_dir))?;
        assert!(result.success(), "{result:#?}");
        assert_eq!(result.scripts.len(), 1, "all checked-in smoke scripts ran");
        assert!(result.scripts.iter().all(|script| {
            script.outcome.metadata.app == "hello"
                && script.outcome.metadata.reset == ResetPolicy::Isolated
        }));
        Ok(())
    }
}
