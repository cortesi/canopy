//! End-to-end Luau smoke-suite test for the Todo example.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use anyhow::Result;
    use canopy_mcp::{ResetPolicy, SuiteConfig, run_suite};

    #[test]
    fn luau_smoke_suite_passes() -> Result<()> {
        let suite_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("smoke");
        let result = run_suite(
            &todo::factory(":memory:".into(), None),
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
