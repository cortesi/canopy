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

/// Run one checked-in script of a directory against a fresh headless instance
/// of a demo.
fn run_script(demo: Demo, dir: &str, script: &str) -> Result<()> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir);
    let factory = AppFactory::new(
        AppMetadata {
            app: "gyms".into(),
            reset: ResetPolicy::Isolated,
        },
        move || demo.build(false).map_err(McpError::app),
    );
    let config = SuiteConfig {
        scripts: vec![dir.join(script)],
        ..SuiteConfig::new(&dir)
    };
    let result = run_suite(&factory, &config).map_err(|error| Error::App(Box::new(error)))?;
    assert!(result.success(), "{result:#?}");
    assert_eq!(result.scripts.len(), 1);
    Ok(())
}

/// Run one checked-in gym script against a fresh headless instance of a demo.
fn run_gym_script(demo: Demo, script: &str) -> Result<()> {
    run_script(demo, "scripts", script)
}

/// Every gallery script runs against the demo that `cargo xtask gallery`
/// captures it from.
#[test]
fn the_gallery_scripts_run() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scripts = [
        (Demo::Stylegym, "stylegym.luau"),
        (Demo::Chartgym, "chartgym.luau"),
        (
            Demo::Cedit {
                file: root.join("../../crates/canopy-widgets/src/spinner.rs"),
            },
            "cedit.luau",
        ),
        (Demo::Editorgym, "editorgym.luau"),
        (Demo::Fontgym, "fontgym.luau"),
        (
            Demo::Imgview {
                file: root.join("../../.assets/shyness.jpg"),
            },
            "imgview.luau",
        ),
    ];
    for (demo, script) in scripts {
        run_script(demo, "gallery", script)?;
    }
    Ok(())
}

#[test]
fn the_cursor_gym_script_passes() -> Result<()> {
    run_gym_script(Demo::Cursorgym, "cursorgym.luau")
}

#[test]
fn the_motion_gym_script_passes() -> Result<()> {
    run_gym_script(Demo::Motiongym, "motiongym.luau")
}

#[test]
fn the_chart_gym_script_passes() -> Result<()> {
    run_gym_script(Demo::Chartgym, "chartgym.luau")
}
