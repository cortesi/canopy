use crate::{
    Context, ViewContext, command, derive_commands, error::Result, geom::Rect, render::Render,
    state::NodeName, widget::Widget,
};

/// A tab control managing a set of nodes with titles.
pub struct Tabs {
    /// Tab titles.
    tabs: Vec<String>,
    /// Active tab index.
    active: usize,
}

#[derive_commands]
impl Tabs {
    /// Construct tabs with the provided titles.
    pub fn new<I>(tabs: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        Self {
            active: 0,
            tabs: tabs.into_iter().map(|s| s.as_ref().to_string()).collect(),
        }
    }

    /// Select the next tab.
    #[command]
    pub fn next(&mut self, _c: &mut dyn Context) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    /// Select the previous tab.
    #[command]
    pub fn prev(&mut self, _c: &mut dyn Context) {
        if !self.tabs.is_empty() {
            self.active = (self.active.wrapping_sub(1)) % self.tabs.len();
        }
    }
}

impl Widget for Tabs {
    fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
        if self.tabs.is_empty() {
            return Ok(());
        }

        for (i, rect) in ctx
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

    fn name(&self) -> NodeName {
        NodeName::convert("tabs")
    }
}
