use canopy::{
    Context, ReadContext, Widget, command, derive_commands, error::Result, render::Render,
    state::NodeName,
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

    /// Select a tab by signed offset.
    /// @param delta Signed tab delta. Positive moves forward and negative moves backward.
    #[command]
    pub fn select_by(&mut self, _c: &mut dyn Context, delta: i32) {
        if self.tabs.is_empty() {
            return;
        }
        let len = self.tabs.len() as i32;
        self.active = (self.active as i32 + delta).rem_euclid(len) as usize;
    }
}

impl Widget for Tabs {
    fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        if self.tabs.is_empty() {
            return Ok(());
        }

        let rects = ctx
            .view()
            .view_rect_local()
            .split_horizontal(self.tabs.len() as u32)?;
        for (i, rect) in rects.iter().enumerate() {
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
