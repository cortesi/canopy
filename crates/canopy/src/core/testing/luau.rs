//! Typecheck assertions for tracked Luau sources.

use crate::{Canopy, error::Result};

/// Assert that a Luau source typechecks against a canopy app's command surface.
///
/// The panic message carries every diagnostic, one per line, so a failure names the offending
/// line and column without a second run.
pub fn assert_typechecks(canopy: &mut Canopy, source_name: &str, source: &str) -> Result<()> {
    let result = canopy.check_script(source_name, source)?;
    assert!(
        result.is_ok(),
        "{}",
        result
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(())
}
