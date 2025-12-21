use canopy::{
    Canopy, Context, Layout, Loader, command, derive_commands,
    error::Result,
    geom::Expanse,
    node::Node,
    state::{NodeState, StatefulNode},
    widgets::Text,
};

#[derive(canopy::StatefulNode)]
/// Demo node that displays placeholder text.
pub struct TextDisplay {
    /// Node state.
    state: NodeState,
    /// Text widget.
    text: Text,
}

#[derive_commands]
impl Default for TextDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl TextDisplay {
    /// Construct a new text display demo.
    pub fn new() -> Self {
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
    /// Trigger a redraw.
    pub fn redraw(&mut self, _ctx: &mut dyn Context) {}
}

impl Node for TextDisplay {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        self.text.layout(l, sz)?;
        let vp = self.text.vp();
        self.wrap(vp)?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.text)
    }
}

impl Loader for TextDisplay {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
    }
}
