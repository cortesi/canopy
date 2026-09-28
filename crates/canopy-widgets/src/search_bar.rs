//! A one-row search field, with what the search found beside it.

use canopy::{
    Context, ContextExt, NodeName, TypedId, ViewContext, ViewContextExt, Widget,
    commands::CommandCall,
    error::{Error, Result},
    geom::{Line, Size},
    layout::{Layout, LayoutOverride, MeasureConstraints, Measurement},
    render::Render,
    style::WidgetState,
    text,
};

use crate::input::{Input, ValueExposure};

/// Prompt of a search bar that names none.
const PROMPT: &str = " / ";
/// Label of the field of a search bar that names none.
const LABEL: &str = "Search";

/// A one-row bar that takes a search query: a prompt, the query field, and on
/// the right what the search found, such as `3 of 12`.
///
/// The bar runs no search. Its owner runs the query on what it searches, such
/// as the text of an [`Editor`](crate::editor::Editor) or the rows of a list,
/// and says what it found with [`SearchBar::set_status`]. The bar posts the
/// query after each edit through [`SearchBar::with_on_change`]. Enter and Esc
/// post the calls set by [`SearchBar::with_on_submit`] and
/// [`SearchBar::with_on_cancel`].
///
/// [`SearchBar::open`] shows the bar with a field that takes the keyboard,
/// and [`SearchBar::close`] hides it. An owner that keeps a query
/// after Enter gives the keyboard back to what it searches and leaves the bar
/// open, so the query and its status stay in sight.
///
/// The bar pushes the `search_bar` layer, and the `focused` layer while the
/// field has the keyboard. Its ground is `background`, the field takes the
/// input styles, and the status paints `status`, or `status/none` when the
/// search found nothing.
pub struct SearchBar {
    /// Node name, which bindings and automation match.
    name: NodeName,
    /// Text before the query.
    prompt: String,
    /// Label of the field in semantic snapshots.
    label: String,
    /// Call posted with the query after each edit.
    on_change: Option<CommandCall>,
    /// Call posted by Enter.
    on_submit: Option<CommandCall>,
    /// Call posted by Esc.
    on_cancel: Option<CommandCall>,
    /// The query field, after the bar mounts.
    field: Option<TypedId<Input>>,
    /// What the search found, after the field.
    status: Option<TypedId<Status>>,
}

impl Default for SearchBar {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchBar {
    /// Construct a closed bar with the prompt ` / `.
    pub fn new() -> Self {
        Self {
            name: NodeName::convert("search_bar"),
            prompt: PROMPT.to_owned(),
            label: LABEL.to_owned(),
            on_change: None,
            on_submit: None,
            on_cancel: None,
            field: None,
            status: None,
        }
    }

    /// Name the node, so bindings and automation can tell bars apart. The
    /// style layer stays `search_bar`.
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = NodeName::convert(name);
        self
    }

    /// Replace the label of the field in semantic snapshots, `Search` by
    /// default.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Replace the text before the query.
    #[must_use]
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }

    /// Post `call` with the query appended after each edit, as
    /// [`Input::with_on_change`] does.
    #[must_use]
    pub fn with_on_change(mut self, call: CommandCall) -> Self {
        self.on_change = Some(call);
        self
    }

    /// Post `call` when Enter is pressed in the field.
    #[must_use]
    pub fn with_on_submit(mut self, call: CommandCall) -> Self {
        self.on_submit = Some(call);
        self
    }

    /// Post `call` when Esc is pressed in the field.
    #[must_use]
    pub fn with_on_cancel(mut self, call: CommandCall) -> Self {
        self.on_cancel = Some(call);
        self
    }

    /// Return the query field, or an error before the bar mounts.
    pub fn field(&self) -> Result<TypedId<Input>> {
        self.field
            .ok_or_else(|| Error::Invalid("the search bar is not mounted".into()))
    }

    /// Return the query.
    pub fn query(&self, ctx: &dyn Context) -> Result<String> {
        ctx.with_widget(self.field()?, |field: &Input| Ok(field.value().to_owned()))
    }

    /// Put `query` in the field without posting a change, as an owner does
    /// when the query comes from elsewhere.
    pub fn set_query(&mut self, ctx: &mut dyn Context, query: &str) -> Result<()> {
        ctx.with_widget_mut(self.field()?, |field: &mut Input, _| {
            field.set_value(query);
            Ok(())
        })
    }

    /// Show the bar with `query` in the field, and give the field the
    /// keyboard. An owner that holds a query from an earlier search passes it,
    /// so the operator can refine it; the status stays until the owner sets
    /// it.
    pub fn open(&mut self, ctx: &mut dyn Context, query: &str) -> Result<()> {
        let field = self.field()?;
        self.set_query(ctx, query)?;
        ctx.set_hidden(ctx.node_id(), false)?;
        ctx.set_focus(field.into())?;
        Ok(())
    }

    /// Hide the bar, and forget its query and its status.
    pub fn close(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.set_query(ctx, "")?;
        self.set_status(ctx, "", true)?;
        ctx.set_hidden(ctx.node_id(), true)?;
        Ok(())
    }

    /// Show what the search found, such as `3 of 12`. `found` says whether it
    /// found anything, which sets the style of the status. Empty text shows
    /// nothing.
    pub fn set_status(
        &mut self,
        ctx: &mut dyn Context,
        text: impl Into<String>,
        found: bool,
    ) -> Result<()> {
        let status = self
            .status
            .ok_or_else(|| Error::Invalid("the search bar is not mounted".into()))?;
        let text = text.into();
        ctx.with_widget_mut(status, |status: &mut Status, _| {
            status.text = text;
            status.found = found;
            Ok(())
        })
    }
}

impl Widget for SearchBar {
    fn layout(&self) -> Layout {
        Layout::row().flex_horizontal(1).fixed_height(1)
    }

    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        render.push_layer("search_bar");
        if self
            .field
            .is_some_and(|field| ctx.is_on_focus_path(field.into()))
        {
            render.push_layer(WidgetState::Focused.layer());
        }
        render.fill("background", ctx.view().outer_rect_local(), ' ')
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let node = ctx.node_id();
        let mut field = Input::new("")
            .with_prompt(self.prompt.clone())
            .with_label(self.label.clone())
            .with_value_exposure(ValueExposure::Public);
        if let Some(call) = self.on_change.take() {
            field = field.with_on_change(call);
        }
        if let Some(call) = self.on_submit.take() {
            field = field.with_on_submit(call);
        }
        if let Some(call) = self.on_cancel.take() {
            field = field.with_on_cancel(call);
        }
        let field = ctx.add_child(node, field)?;
        ctx.set_layout_override(field.into(), LayoutOverride::new().flex_horizontal(1))?;
        let status = ctx.add_child(node, Status::default())?;
        self.field = Some(field);
        self.status = Some(status);
        ctx.set_hidden(node, true)?;
        Ok(())
    }

    fn name(&self) -> NodeName {
        self.name.clone()
    }
}

/// What a search found, at the right of a search bar.
#[derive(Default)]
pub struct Status {
    /// The text.
    text: String,
    /// Whether the search found anything.
    found: bool,
}

impl Widget for Status {
    fn layout(&self) -> Layout {
        Layout::column()
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let width = if self.text.is_empty() {
            0
        } else {
            text::width(&self.text).saturating_add(2)
        };
        c.clamp(Size::new(width, 1))
    }

    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let area = ctx.view().outer_rect_local();
        if area.w == 0 || self.text.is_empty() {
            return Ok(());
        }
        let style = if self.found { "status" } else { "status/none" };
        render.text(style, Line::new(0, 0, area.w), &format!(" {} ", self.text))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("search_status")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        Register, Setup, commands::CommandTarget, derive_commands, input::key::KeyCode,
        layout::Direction, testing::harness::Harness,
    };

    use super::*;

    /// A host that records what its search bar posts.
    #[derive(Default)]
    struct Host {
        /// The bar, after the host mounts.
        bar: Option<TypedId<SearchBar>>,
        /// Queries, submits, and cancels, in order.
        events: Vec<String>,
    }

    #[derive_commands]
    impl Host {
        /// Records one query.
        /// @param query The query.
        #[command]
        fn changed(&mut self, mut query: String) {
            query.insert_str(0, "change ");
            self.events.push(query);
        }

        /// Records Enter.
        #[command]
        fn submitted(&mut self) {
            self.events.push("submit".to_owned());
        }

        /// Records Esc.
        #[command]
        fn cancelled(&mut self) {
            self.events.push("cancel".to_owned());
        }
    }

    impl Widget for Host {
        fn layout(&self) -> Layout {
            Layout::fill().direction(Direction::Column)
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let owner = CommandTarget::Exact(ctx.node_id());
            let bar = ctx.add_child(
                ctx.node_id(),
                SearchBar::new()
                    .with_name("find_bar")
                    .with_on_change(Self::spec_changed().call().with_target(owner))
                    .with_on_submit(Self::call_submitted().with_target(owner))
                    .with_on_cancel(Self::call_cancelled().with_target(owner)),
            )?;
            self.bar = Some(bar);
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("host")
        }
    }

    impl Register for Host {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()
        }
    }

    /// Builds a host with its bar open.
    fn opened() -> Result<Harness> {
        let mut harness = Harness::builder(Host::default())
            .register::<Host>()
            .size(30, 3)
            .build()?;
        harness.render()?;
        assert!(!harness.tbuf().contains_text(" / "), "a closed bar hides");
        harness.with_root_widget_context(|host: &mut Host, ctx| {
            let bar = host.bar.expect("mounted");
            ctx.with_widget_mut(bar, |bar: &mut SearchBar, ctx| bar.open(ctx, ""))
        })?;
        harness.render()?;
        Ok(harness)
    }

    #[test]
    fn the_bar_posts_each_edit_enter_and_escape_to_its_owner() -> Result<()> {
        let mut harness = opened()?;
        assert!(
            harness.tbuf().contains_text(" / "),
            "an open bar shows its prompt"
        );
        assert_eq!(
            harness.find_nodes("**/find_bar")?.len(),
            1,
            "the bar takes its name"
        );
        harness.type_text("ab")?;
        harness.key(KeyCode::Enter)?;
        harness.key(KeyCode::Esc)?;
        let events = harness.with_root_widget(|host: &mut Host| host.events.clone());
        assert_eq!(events, ["change a", "change ab", "submit", "cancel"]);
        let query = harness.with_root_widget_context(|host: &mut Host, ctx| {
            let bar = host.bar.expect("mounted");
            ctx.with_widget(bar, |bar: &SearchBar| bar.query(ctx))
        })?;
        assert_eq!(query, "ab");
        Ok(())
    }

    #[test]
    fn reopening_with_a_held_query_lets_the_operator_refine_it() -> Result<()> {
        let mut harness = opened()?;
        harness.with_root_widget_context(|host: &mut Host, ctx| {
            let bar = host.bar.expect("mounted");
            ctx.with_widget_mut(bar, |bar: &mut SearchBar, ctx| bar.open(ctx, "ab"))
        })?;
        harness.type_text("c")?;
        let events = harness.with_root_widget(|host: &mut Host| host.events.clone());
        assert_eq!(events, ["change abc"], "typing continues the held query");

        // A query set from elsewhere posts no change.
        harness.with_root_widget_context(|host: &mut Host, ctx| {
            let bar = host.bar.expect("mounted");
            ctx.with_widget_mut(bar, |bar: &mut SearchBar, ctx| bar.set_query(ctx, "xyz"))
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text(" / xyz"));
        let events = harness.with_root_widget(|host: &mut Host| host.events.clone());
        assert_eq!(events, ["change abc"]);
        Ok(())
    }

    #[test]
    fn the_status_shows_at_the_right_and_says_when_nothing_matched() -> Result<()> {
        let mut harness = opened()?;
        let status = |harness: &mut Harness, text: &str, found: bool| -> Result<()> {
            harness.with_root_widget_context(|host: &mut Host, ctx| {
                let bar = host.bar.expect("mounted");
                ctx.with_widget_mut(bar, |bar: &mut SearchBar, ctx| {
                    bar.set_status(ctx, text, found)
                })
            })?;
            harness.render()
        };
        status(&mut harness, "2 of 5", true)?;
        let row = harness.tbuf().lines()[0].clone();
        assert!(row.trim_end().ends_with("2 of 5"), "{row:?}");
        let style_of = |harness: &Harness, needle: char| {
            harness
                .canopy
                .snapshot()
                .expect("frame")
                .buffer
                .cells()
                .iter()
                .find(|cell| cell.ch == needle)
                .map(|cell| cell.style.fg)
                .expect("the status renders")
        };
        let found = style_of(&harness, '2');
        status(&mut harness, "no 9", false)?;
        assert_ne!(
            style_of(&harness, '9'),
            found,
            "no match takes its own style"
        );

        // Closing hides the bar and forgets the query.
        harness.with_root_widget_context(|host: &mut Host, ctx| {
            let bar = host.bar.expect("mounted");
            ctx.with_widget_mut(bar, |bar: &mut SearchBar, ctx| bar.close(ctx))
        })?;
        harness.render()?;
        assert!(!harness.tbuf().contains_text(" / "));
        Ok(())
    }
}
