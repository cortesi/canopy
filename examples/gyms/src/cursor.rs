//! Cursor gym: every cursor role, motion, and shape.
//!
//! Each row declares a cursor. The focused row holds the primary cursor, which
//! moves; every other row shows a steady secondary cursor.

use canopy::{
    CanopyBuilder, Context, ContextExt, NodeName, ViewContext, Widget,
    error::Result,
    geom::{Line, Point},
    layout::Layout,
    render::{
        Render,
        cursor::{self, CursorMotion, CursorRequest, CursorShape},
    },
    rgb,
};

/// Width of the label column.
const LABEL_WIDTH: u32 = 18;
/// Sample text beside each label.
const SAMPLE: &str = "type here";
/// Column of the cursor within the sample text.
const CURSOR_COLUMN: u32 = 5;

/// One cursor sample: a label and a declaration.
struct Sample {
    /// Row label.
    label: &'static str,
    /// Cursor role.
    role: &'static str,
    /// Shape that replaces the role's shape.
    shape: Option<CursorShape>,
    /// Motion that replaces the role's motion.
    motion: Option<CursorMotion>,
}

/// The samples, one per row.
const SAMPLES: [Sample; 9] = [
    Sample {
        label: "text",
        role: cursor::TEXT,
        shape: None,
        motion: None,
    },
    Sample {
        label: "vi insert",
        role: cursor::VI_INSERT,
        shape: None,
        motion: None,
    },
    Sample {
        label: "vi normal",
        role: cursor::VI_NORMAL,
        shape: None,
        motion: None,
    },
    Sample {
        label: "vi visual",
        role: cursor::VI_VISUAL,
        shape: None,
        motion: None,
    },
    Sample {
        label: "inactive",
        role: cursor::INACTIVE,
        shape: None,
        motion: None,
    },
    Sample {
        label: "terminal",
        role: cursor::TERMINAL,
        shape: None,
        motion: None,
    },
    Sample {
        label: "underline",
        role: cursor::TEXT,
        shape: Some(CursorShape::Underline),
        motion: None,
    },
    Sample {
        label: "pulse",
        role: cursor::TEXT,
        shape: None,
        motion: Some(CursorMotion::PULSE),
    },
    Sample {
        label: "steady",
        role: cursor::TEXT,
        shape: None,
        motion: Some(CursorMotion::Steady),
    },
];

/// Demo node that stacks the cursor samples.
#[derive(Default)]
pub struct CursorGym;

impl CursorGym {
    /// Construct the cursor gym.
    pub fn new() -> Self {
        Self
    }
}

impl Widget for CursorGym {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let mut rows = Vec::with_capacity(SAMPLES.len());
        for sample in &SAMPLES {
            rows.push(ctx.create_detached(CursorSample { sample })?.into());
        }
        ctx.set_children(ctx.node_id(), rows.clone())?;
        if let Some(first) = rows.first() {
            ctx.set_focus(*first)?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("cursor_gym")
    }
}

/// One row: a label, sample text, and a cursor in it.
struct CursorSample {
    /// The sample this row shows.
    sample: &'static Sample,
}

impl Widget for CursorSample {
    fn layout(&self) -> Layout {
        Layout::row().flex_horizontal(1).fixed_height(1)
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();
        r.fill("", rect, ' ')?;
        let origin = view.content_origin();
        let label = if ctx.is_focused() {
            "cursorgym/label/focused"
        } else {
            "cursorgym/label"
        };
        r.text(
            label,
            Line::new(origin.x, origin.y, LABEL_WIDTH),
            self.sample.label,
        )?;
        let x = origin.x.saturating_add(LABEL_WIDTH);
        r.text(
            "cursorgym/sample",
            Line::new(x, origin.y, SAMPLE.len() as u32),
            SAMPLE,
        )?;

        let mut request = CursorRequest::new(self.sample.role);
        if self.sample.role == cursor::TERMINAL {
            // A child terminal program chooses its own cursor color.
            request = request.with_color(rgb!("#e5c07b"));
        }
        if let Some(shape) = self.sample.shape {
            request = request.with_shape(shape);
        }
        if let Some(motion) = self.sample.motion {
            request = request.with_motion(motion);
        }
        let location = Point {
            x: x.saturating_add(CURSOR_COLUMN),
            y: origin.y,
        };
        r.cursor(location, request);
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("cursor_sample")
    }
}

/// Focus keys, and the root's defaults.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()
canopy.keymap({
    { key = "Tab", description = "Next sample", action = command.root.focus("next") },
    { key = "BackTab", description = "Previous sample", action = command.root.focus("prev") },
    { key = "j", description = "Next sample", action = command.root.focus("down") },
    { key = "k", description = "Previous sample", action = command.root.focus("up") },
})
"#;

/// Queue this demo's styles and bindings.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder
        .configure(|setup| {
            setup.widget_styles(|palette, rules| {
                rules
                    .fg("cursorgym/label", palette.muted_fg)
                    .fg("cursorgym/label/focused", palette.accent)
                    .fg("cursorgym/sample", palette.fg)
                    .apply();
            });
            Ok(())
        })
        .script("cursorgym", DEFAULT_BINDINGS)
}
