//! Launch the test_text example.

use std::{error::Error, result::Result as StdResult};

use canopy::{Canopy, Loader, backend::crossterm::runloop, widgets::Root};
use canopy_examples::test_text::TextDisplay;

/// Run the test_text example.
pub fn main() -> StdResult<(), Box<dyn Error>> {
    let mut cnpy = Canopy::new();
    cnpy.add_commands::<Root<TextDisplay>>();
    TextDisplay::load(&mut cnpy);

    cnpy.bind_key('q', "root", "root::quit()")?;
    cnpy.bind_key('r', "textdisplay", "textdisplay::redraw()")?;

    let root = Root::new(TextDisplay::new());
    runloop(cnpy, root)?;
    Ok(())
}
