use canopy::{backend::crossterm::runloop, *};
use canopy_examples::test_text::TextDisplay;

pub fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut cnpy = Canopy::new();
    cnpy.add_commands::<Root<TextDisplay>>();
    TextDisplay::load(&mut cnpy);

    cnpy.bind_key('q', "root", "root::quit()")?;
    cnpy.bind_key('r', "textdisplay", "textdisplay::redraw()")?;

    let root = Root::new(TextDisplay::new());
    runloop(cnpy, root)?;
    Ok(())
}
