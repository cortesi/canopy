//! Launch the intervals example.

use canopy::{Canopy, Loader, backend::crossterm::runloop, error::Result, widgets::Root};
use canopy_examples::intervals::{Intervals, setup_bindings};

/// Run the intervals example.
pub fn main() -> Result<()> {
    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy);
    Intervals::load(&mut cnpy);
    setup_bindings(&mut cnpy);

    let app_id = cnpy.core.add(Intervals::new());
    Root::install(&mut cnpy.core, app_id)?;
    runloop(cnpy)?;
    Ok(())
}
