use canopy::{backend::crossterm::runloop, derive_commands, widgets::Text, *};

#[derive(StatefulNode)]
struct TextDisplay {
    state: NodeState,
    text: Text,
}

#[derive_commands]
impl TextDisplay {
    fn new() -> Self {
        let paragraph = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod \
                        tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
                        quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
                        consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse \
                        cillum dolore eu fugiat nulla pariatur.\
                        Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod \
                        tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
                        quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
                        consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse \
                        cillum dolore eu fugiat nulla pariatur.\
                        Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod \
                        tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
                        quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
                        consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse \
                        cillum dolore eu fugiat nulla pariatur.
                        ";

        Self {
            state: NodeState::default(),
            text: Text::new(paragraph),
        }
    }

    #[command]
    fn redraw(&mut self, ctx: &mut dyn Context) {
        ctx.taint_tree(self);
    }
}

impl Node for TextDisplay {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        self.text.layout(l, sz)?;
        let vp = self.text.vp();
        l.wrap(self, vp)?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.text)
    }
}

pub fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut cnpy = Canopy::new();
    cnpy.add_commands::<Root<TextDisplay>>();
    cnpy.add_commands::<TextDisplay>();

    cnpy.bind_key('q', "root", "root::quit()")?;
    cnpy.bind_key('r', "textdisplay", "textdisplay::redraw()")?;

    let root = Root::new(TextDisplay::new());
    runloop(cnpy, root)?;
    Ok(())
}
