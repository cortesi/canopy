use canopy_core as canopy;

use canopy_core::{
    Context, Node, NodeState, Render, Result, StatefulNode, command, derive_commands,
};

/// A tab control managing a set of nodes with titles.
#[derive(canopy_core::StatefulNode)]
pub struct Tabs {
    pub state: NodeState,
    pub tabs: Vec<String>,
    pub active: usize,
}

#[derive_commands]
impl Tabs {
    pub fn new(tabs: Vec<String>) -> Self {
        Tabs {
            state: NodeState::default(),
            active: 0,
            tabs,
        }
    }

    /// Select the next tab.
    #[command]
    pub fn next(&mut self, _c: &mut dyn Context) {
        self.active = (self.active + 1) % self.tabs.len();
    }

    /// Select the previous tab.
    #[command]
    pub fn prev(&mut self, _c: &mut dyn Context) {
        self.active = (self.active.wrapping_sub(1)) % self.tabs.len();
    }
}

impl Node for Tabs {
    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        for (i, rect) in self
            .vp()
            .view()
            .split_horizontal(self.tabs.len() as u32)?
            .iter()
            .enumerate()
        {
            let styl = if i == self.active {
                "tab/active"
            } else {
                "tab/inactive"
            };
            let (text, end) = rect.carve_hend(1);
            r.text(styl, text.line(0), &self.tabs[i])?;
            r.text("", end.line(0), " ")?;
        }
        Ok(())
    }
}
