//! A small application under `Root`, for the toast and notice log tests.

use std::{sync::Arc, time::Duration};

use canopy::{
    Canopy, CanopyBuilder, Context, ContextExt, EventOutcome, NodeId, NodeName, Register, Setup,
    ViewContext, Widget, derive_commands,
    error::Result,
    geom::{PointI32, Size},
    input::{Event, ModalBindings, ModalOptions, ModalToken, key, mouse},
    layout::{Direction, Layout},
    runtime::{NoticePost, TurnInput},
    testing::{ManualClock, harness::Harness},
};

use crate::Root;

/// An application that counts the clicks it takes, and holds a modal of
/// its own, added as it mounts, as an application dialog is.
#[derive(Default)]
pub struct App {
    /// Clicks that reached the application.
    pub(crate) clicks: usize,
    /// The application's modal.
    cover: Option<NodeId>,
    /// The open modal's token.
    token: Option<ModalToken>,
}

#[derive_commands]
impl App {
    /// Open the application's modal.
    #[command]
    fn cover(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let cover = self.cover.expect("the cover is mounted");
        self.token = Some(ctx.open_modal(ModalOptions {
            owner: ctx.node_id(),
            modal: cover,
            initial_focus: cover,
            dim_target: None,
            bindings: ModalBindings::Application,
        })?);
        Ok(())
    }

    /// Close the application's modal.
    #[command]
    fn uncover(&mut self, ctx: &mut dyn Context) -> Result<()> {
        match self.token.take() {
            Some(token) => ctx.close_modal(token),
            None => Ok(()),
        }
    }
}

impl Widget for App {
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.cover = Some(ctx.add_modal(Cover)?.into());
        Ok(())
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(m) = event
            && m.action == mouse::Action::Down
        {
            self.clicks += 1;
            return Ok(EventOutcome::Handle);
        }
        Ok(EventOutcome::Ignore)
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("app")
    }
}

impl Register for App {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()
    }
}

/// A modal that fills the window with `#`.
struct Cover;

impl Widget for Cover {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, render: &mut canopy::render::Render, ctx: &dyn ViewContext) -> Result<()> {
        render.fill("text", ctx.view().outer_rect_local(), '#')
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("cover")
    }
}

/// An application with a band of rows at its bottom, as an editor is.
#[derive(Default)]
pub struct BandApp {
    /// The band, after the application mounts.
    pub(crate) band: Option<NodeId>,
}

impl Widget for BandApp {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let app = ctx.node_id();
        ctx.add_child(app, Filler)?;
        self.band = Some(ctx.add_child(app, Band)?.into());
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("band_app")
    }
}

/// The space above the band.
struct Filler;

impl Widget for Filler {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
}

/// A band of `=` rows, three high until a test overrides its height.
struct Band;

impl Widget for Band {
    fn layout(&self) -> Layout {
        Layout::row().flex_horizontal(1).fixed_height(3)
    }

    fn render(&mut self, render: &mut canopy::render::Render, ctx: &dyn ViewContext) -> Result<()> {
        render.fill("text", ctx.view().outer_rect_local(), '=')
    }

    fn name(&self) -> NodeName {
        NodeName::convert("band")
    }
}

/// Build the band application under `Root` on a screen of `size`, and return
/// it with the band's node.
pub fn band_app(size: Size) -> Result<(Harness, NodeId)> {
    let mut canopy: Canopy = CanopyBuilder::new().configure(Root::register).build()?;
    Root::new().install(&mut canopy, BandApp::default())?;
    canopy.set_clock_for_testing(Arc::new(ManualClock::new()))?;
    let mut harness = Harness::from_canopy(canopy, size)?;
    harness.render()?;
    let band = harness.with_unique(|app: &mut BandApp, _| Ok(app.band.expect("mounted")))?;
    Ok((harness, band))
}

/// Build the application under `Root` on a screen of `size`, with a clock
/// that the test moves.
pub fn app(size: Size) -> Result<(Harness, Arc<ManualClock>)> {
    let mut canopy: Canopy = CanopyBuilder::new()
        .configure(|setup| {
            Root::register(setup)?;
            App::register(setup)
        })
        .build()?;
    Root::new().install(&mut canopy, App::default())?;
    let clock = Arc::new(ManualClock::new());
    canopy.set_clock_for_testing(Arc::clone(&clock))?;
    let mut harness = Harness::from_canopy(canopy, size)?;
    harness.render()?;
    Ok((harness, clock))
}

/// Post `post` and draw the frame it changes.
pub fn notify(harness: &mut Harness, post: NoticePost) -> Result<()> {
    harness.canopy.notify(post);
    harness.render()
}

/// Move the clock on by `millis`, deliver the wake the driver would, and
/// draw.
pub fn advance(harness: &mut Harness, clock: &ManualClock, millis: u64) -> Result<()> {
    clock.advance(Duration::from_millis(millis))?;
    harness.canopy.turn(TurnInput::Wake)?;
    harness.render()
}

/// Return the screen rows.
pub fn rows(harness: &Harness) -> Vec<String> {
    harness.tbuf().lines()
}

/// Press the left button at a screen cell.
pub fn click(harness: &mut Harness, x: i32, y: i32) -> Result<()> {
    harness.mouse(mouse::MouseEvent {
        action: mouse::Action::Down,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: PointI32 { x, y },
    })
}
