//! Launch the textgym example.

use std::{error::Error, process, result::Result};

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    prelude::*,
};
use canopy_examples::textgym::TextGym;
use canopy_widgets::Root;

/// Run the textgym example.
fn main() -> Result<(), Box<dyn Error>> {
    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;
    TextGym::load(&mut cnpy)?;

    cnpy.bind_key('q', "root", "root::quit()")?;
    cnpy.bind_key('r', "text_gym", "text_gym::redraw()")?;

    let app_id = cnpy.core.create_detached(TextGym::new());
    Root::install(&mut cnpy.core, app_id)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}
