//! Launch the test_text example.

use std::{error::Error, result::Result};

use canopy::{Canopy, Loader, backend::crossterm::runloop};
use canopy_examples::test_text::TextDisplay;
use canopy_widgets::Root;

/// Run the test_text example.
fn main() -> Result<(), Box<dyn Error>> {
    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy);
    TextDisplay::load(&mut cnpy);

    cnpy.bind_key('q', "root", "root::quit()")?;
    cnpy.bind_key('r', "text_display", "text_display::redraw()")?;

    let app_id = cnpy.core.create_detached(TextDisplay::new());
    Root::install(&mut cnpy.core, app_id)?;
    runloop(cnpy)?;
    Ok(())
}
