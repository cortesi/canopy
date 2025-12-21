use crate::core as canopy;
use crate::core::{
    Context, Node, NodeState, Render, Result, StatefulNode, command, derive_commands,
};

/// A tab control managing a set of nodes with titles.
#[derive(crate::core::StatefulNode)]
pub struct Tabs {
    /// Node state.
    pub state: NodeState,
    /// Tab titles.
    pub tabs: Vec<String>,
    /// Active tab index.
    pub active: usize,
}

#[derive_commands]
impl Tabs {
    /// Construct tabs with the provided titles.
    pub fn new(tabs: Vec<String>) -> Self {
        Self {
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
