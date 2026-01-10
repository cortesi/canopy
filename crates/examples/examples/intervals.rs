//! Launch the intervals example.

use std::process;

use canopy::{
    Canopy, Loader,
    backend::crossterm::{RunloopOptions, runloop_with_options},
    error::Result,
};
use canopy_examples::intervals::{Intervals, setup_bindings};
use canopy_widgets::Root;

/// Run the intervals example.
fn main() -> Result<()> {
    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;
    Intervals::load(&mut cnpy)?;
    setup_bindings(&mut cnpy);

    let app_id = cnpy.core.create_detached(Intervals::new());
    Root::install(&mut cnpy.core, app_id)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}
