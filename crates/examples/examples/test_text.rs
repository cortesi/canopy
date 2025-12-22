//! Launch the test_text example.

use std::{error::Error, result::Result as StdResult};

use canopy::{Canopy, Loader, backend::crossterm::runloop, widgets::Root};
use canopy_examples::test_text::TextDisplay;

/// Run the test_text example.
pub fn main() -> StdResult<(), Box<dyn Error>> {
    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy);
    TextDisplay::load(&mut cnpy);

    cnpy.bind_key('q', "root", "root::quit()")?;
    cnpy.bind_key('r', "text_display", "text_display::redraw()")?;

    let app_id = cnpy.core.add(TextDisplay::new());
    Root::install(&mut cnpy.core, app_id)?;
    runloop(cnpy)?;
    Ok(())
}
