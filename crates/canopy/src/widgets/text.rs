use crate::core as canopy;
use crate::core::{
    Context, Layout, Node, NodeState, Render, Result, StatefulNode, command, derive_commands,
    geom::Expanse,
};

/// Multiline text widget with wrapping and scrolling.
#[derive(crate::core::StatefulNode)]
pub struct Text {
    /// Node state.
    pub state: NodeState,
    /// Raw text content.
    pub raw: String,
    /// Wrapped lines cache.
    lines: Option<Vec<String>>,
    /// Optional fixed width for wrapping.
    fixed_width: Option<u32>,
    /// Cached size of the wrapped content.
    current_size: Expanse,
}

#[derive_commands]
impl Text {
    /// Construct a text widget with raw content.
    pub fn new(raw: &str) -> Self {
        Self {
            state: NodeState::default(),

            raw: raw.to_owned(),
            lines: None,
            fixed_width: None,
            current_size: Expanse::default(),
        }
    }
    /// Add a fixed width, ignoring fit parameters
    pub fn with_fixed_width(mut self, width: u32) -> Self {
        self.fixed_width = Some(width);
        self
    }

    #[command]
    /// Scroll to the top-left corner.
    pub fn scroll_to_top(&mut self, c: &mut dyn Context) {
        c.scroll_to(self, 0, 0);
    }

    #[command]
    /// Scroll down by one line.
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        c.scroll_down(self);
    }

    #[command]
    /// Scroll up by one line.
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        c.scroll_up(self);
    }

    #[command]
    /// Scroll left by one column.
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        c.scroll_left(self);
    }

    #[command]
    /// Scroll right by one column.
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        c.scroll_right(self);
    }

    #[command]
    /// Page down in the viewport.
    pub fn page_down(&mut self, c: &mut dyn Context) {
        c.page_down(self);
    }

    #[command]
    /// Page up in the viewport.
    pub fn page_up(&mut self, c: &mut dyn Context) {
        c.page_up(self);
    }
}

impl Node for Text {
    fn layout(&mut self, _l: &Layout, s: Expanse) -> Result<()> {
        let w = if let Some(w) = self.fixed_width {
            w
        } else {
            s.w
        };
        // Only resize if the width has changed
        if self.current_size.w != w {
            let mut split: Vec<String> = vec![];
            for i in textwrap::wrap(&self.raw, w as usize) {
                split.push(format!("{:width$}", i, width = w as usize))
            }
            self.current_size = Expanse {
                w,
                h: split.len() as u32,
            };
            self.lines = Some(split);
        }
        let cs = self.current_size;
        self.fit_size(cs, s);
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, rndr: &mut Render) -> Result<()> {
        let vo = self.vp().view();
        if let Some(lines) = self.lines.as_ref() {
            for i in 0..vo.h {
                let line_idx = (vo.tl.y + i) as usize;
                if line_idx < lines.len() {
                    let line = &lines[line_idx];
                    let start_char = vo.tl.x as usize;
                    
                    let start_byte = line.char_indices()
                        .nth(start_char)
                        .map(|(i, _)| i)
                        .unwrap_or(line.len());
                    
                    let out = &line[start_byte..];
                    rndr.text("text", vo.line(i), out)?;
                }
            }
        }
        Ok(())
    }
}
