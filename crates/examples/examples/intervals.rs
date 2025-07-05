use canopy::{backend::crossterm::runloop, *};
use canopy_examples::intervals::{setup_bindings, Intervals};

pub fn main() -> Result<()> {
    let mut cnpy = Canopy::new();
    Root::<Intervals>::load(&mut cnpy);
    setup_bindings(&mut cnpy);

    let root = Root::new(Intervals::new());
    runloop(cnpy, root)?;
    Ok(())
}
