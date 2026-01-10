//! Launch the cedit example.

use std::{env, error::Error, fs, path::Path, process, result::Result};

use canopy::{
    Canopy, Loader,
    backend::crossterm::{RunloopOptions, runloop_with_options},
};
use canopy_examples::cedit::{Ed, setup_bindings};
use canopy_widgets::Root;

/// Run the cedit example.
fn main() -> Result<(), Box<dyn Error>> {
    let filename = match env::args().nth(1) {
        Some(filename) => filename,
        None => {
            eprintln!("Usage: cedit <filename>");
            return Ok(());
        }
    };

    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;
    Ed::load(&mut cnpy)?;
    setup_bindings(&mut cnpy);

    let contents = fs::read_to_string(&filename)?;
    let extension = file_extension(&filename);
    let app_id = cnpy.core.create_detached(Ed::new(&contents, &extension));
    Root::install_with_inspector(&mut cnpy.core, app_id, false)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}

/// Return a lowercase file extension hint for syntax selection.
fn file_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "txt".to_string())
}
