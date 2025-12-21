//! Launch the intervals example.

use canopy::{backend::crossterm::runloop, *};
use canopy_examples::intervals::{Intervals, setup_bindings};

/// Run the intervals example.
pub fn main() -> Result<()> {
    let mut cnpy = Canopy::new();
    Root::<Intervals>::load(&mut cnpy);
    setup_bindings(&mut cnpy);

    let root = Root::new(Intervals::new());
    runloop(cnpy, root)?;
    Ok(())
}
