//! Markdown with deterministic stream controls and a recording host.

use std::time::Duration;

use canopy::{
    CanopyBuilder, Context, ContextExt, NodeName, Register, Setup, Widget, derive_commands,
    error::Result,
    layout::{Layout, LayoutOverride},
    style::themes,
    tree::ChildSlot,
};
use canopy_markdown::{
    DocumentInput, FinishOutcome, Markdown, MarkdownOptions, Mode, ViewType,
    host::{HostAction, HostOutcome, HostReply, HostRequest},
    presentation::GlyphPreset,
};
use canopy_widgets::Text;

use crate::{fixed_row, flex_row};

/// Comprehensive complete document shared by the gym and its tests.
pub const FIXTURE: &str = include_str!("../fixtures/markdown.md");
/// Exact-byte fixture covering CRLF, tabs, Unicode, and final newline.
pub const EXACT_SOURCE: &str = "# Exact source\r\n\r\n\tlet label = \"界\";\r\nfinal line\r\n";
/// Controlled append boundaries including open fences and trailing tables.
const STREAM: &[&str] = &[
    "# Stream log\n\nEarlier content remains readable while output arrives. Inspect the [guide](https://example.com/stream).\n\n## Earlier context\n\nA stable first paragraph.\n\nA second paragraph stays above the live output.\n\nA third paragraph gives the reader room to pause.\n\nA fourth paragraph is accepted before the code arrives.\n\nThe fifth note preserves its own source identity.\n\nThe sixth note remains independently selectable.\n\nThe seventh note makes the early context scrollable.\n\nThe eighth note stays above the live tail.\n\nThe ninth note is accepted in this same chunk.\n\nThe tenth note can be inspected while more text arrives.\n\nThe eleventh note leaves room for a paused viewport.\n\nThe twelfth note precedes the receiving code card.\n\n",
    "## Build output\n\n```rust\nlet revision = 1;\n",
    "publish(revision);\n```\n\n## Live table\n\n| Phase | State | Notes |\n| --- | --- | --- |\n| Parse | ready | A trailing table stays literal |\n",
    "| Paint | queued | 界 and 👩‍💻 stay intact |\n",
    "\n## Published tail\n\nThe preceding table now has a following block and publishes a grid.\n\n",
    "All chunks are accepted. Finish or abort explicitly to record the final outcome.\n",
];

canopy::slot!(pub(crate) ReaderSlot: Markdown);
canopy::slot!(StatusSlot: Text);

/// Gym controls installed in addition to the standard reader bindings.
const BINDINGS: &str = r#"
root.default_bindings()
canopy.keymap({
    path = "markdown_gym",
    { key = "alt-a", description = "Append next stream chunk", action = command.markdown_gym.append_chunk() },
    { key = "alt-f", description = "Finish stream", action = command.markdown_gym.finish() },
    { key = "alt-x", description = "Abort stream", action = command.markdown_gym.abort() },
    { key = "alt-r", description = "Replace with complete fixture", action = command.markdown_gym.replace() },
    { key = "alt-s", description = "Begin controlled stream", action = command.markdown_gym.begin_stream() },
    { key = "alt-w", description = "Toggle narrow reader", action = command.markdown_gym.narrow() },
    { key = "alt-t", description = "Toggle light theme", action = command.markdown_gym.theme() },
    { key = "alt-u", description = "Toggle ASCII decorations", action = command.markdown_gym.glyphs() },
})
"#;

/// Access the mounted document controller in its own context.
fn reader<R>(
    ctx: &mut dyn Context,
    f: impl FnOnce(&mut Markdown, &mut dyn Context) -> Result<R>,
) -> Result<R> {
    ctx.with_slot::<ReaderSlot, _>(ctx.node_id(), f)
}

/// A real reader with an explicit, non-effecting clipboard/browser host.
pub struct MarkdownGym {
    /// Next controlled chunk to append.
    next_chunk: usize,
    /// Whether the controlled stream still accepts append or finish.
    receiving: bool,
    /// Human-readable lifecycle outcome for the host status line.
    state: String,
    /// Whether the reader uses a fixed 40-cell width.
    narrow: bool,
    /// Whether newly mounted geometry uses portable decorations.
    ascii: bool,
    /// Whether the live palette uses the light theme.
    light: bool,
    /// Recorded host requests, retained without external side effects.
    requests: Vec<HostRequest>,
}
impl Default for MarkdownGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl MarkdownGym {
    /// Construct the complete fixture with a recording host.
    pub fn new() -> Self {
        Self {
            next_chunk: 0,
            receiving: false,
            state: "complete fixture".into(),
            narrow: false,
            ascii: false,
            light: false,
            requests: Vec::new(),
        }
    }

    /// Build reader options with an inert recording host.
    fn options(&self) -> MarkdownOptions {
        MarkdownOptions {
            clipboard: true,
            browser: true,
            base_url: Some("https://example.com/docs/".into()),
            glyphs: if self.ascii {
                GlyphPreset::Ascii
            } else {
                GlyphPreset::Unicode
            },
            coalesce: Duration::ZERO,
            ..Default::default()
        }
    }
    /// Publish lifecycle and host-action counts to the fixed status line.
    fn show_status(&self, ctx: &mut dyn Context) -> Result<()> {
        let text = format!(
            "Markdown · {} · {} · recording host: {} actions",
            self.state,
            if self.ascii { "ASCII" } else { "Unicode" },
            self.requests.len()
        );
        ctx.with_slot::<StatusSlot, _>(ctx.node_id(), |status, _| {
            status.set_text(text);
            Ok(())
        })
    }
    /// Choose the full-width or narrow reader region.
    fn reader_layout(&self) -> LayoutOverride {
        let mut layout = flex_row(1);
        if self.narrow {
            layout = layout.fixed_width(40);
        }
        layout
    }

    /// A worker/publication readiness predicate, independent of frame timing.
    #[command]
    pub fn ready(&self, ctx: &mut dyn Context) -> Result<bool> {
        reader(ctx, |md, _| Ok(md.ready()))
    }
    #[command]
    /// Latest source revision accepted by the reader.
    pub fn accepted_revision(&self, ctx: &mut dyn Context) -> Result<u64> {
        reader(ctx, |md, _| Ok(md.accepted_revision()))
    }
    #[command]
    /// Source revision currently displayed by the active surface.
    pub fn displayed_revision(&self, ctx: &mut dyn Context) -> Result<u64> {
        reader(ctx, |md, _| Ok(md.displayed_revision()))
    }
    #[command]
    /// Current source epoch, retained through appends and view switches.
    pub fn epoch(&self, ctx: &mut dyn Context) -> Result<u64> {
        reader(ctx, |md, _| Ok(md.epoch()))
    }
    #[command]
    /// Currently active document presentation.
    pub fn view(&self, ctx: &mut dyn Context) -> Result<ViewType> {
        reader(ctx, |md, _| Ok(md.view_type()))
    }
    #[command]
    /// Whether the active reading surface holds focus.
    pub fn focused(&self, ctx: &mut dyn Context) -> Result<bool> {
        reader(
            ctx,
            |md, ctx| Ok(md.navigation_node() == ctx.focused_node()),
        )
    }
    #[command]
    /// Focus the active reading surface explicitly.
    pub fn focus_reader(&self, ctx: &mut dyn Context) -> Result<()> {
        reader(ctx, |md, ctx| {
            if let Some(node) = md.navigation_node() {
                ctx.set_focus(node)?;
            }
            Ok(())
        })
    }
    #[command]
    /// Whether a Markdown inspector is mounted in the root overlay.
    #[allow(
        clippy::needless_pass_by_ref_mut,
        reason = "Command dispatch requires a mutable Context"
    )]
    pub fn inspector_open(&self, ctx: &mut dyn Context) -> Result<bool> {
        let mut pending = vec![ctx.root_id()];
        while let Some(node) = pending.pop() {
            let path = ctx.path_of(ctx.root_id(), node).to_string();
            if path.split('/').any(|part| part == "markdown_inspector") {
                return Ok(true);
            }
            pending.extend(ctx.children_of(node));
        }
        Ok(false)
    }
    /// Whether the mounted inspector's current publication is ready for input.
    #[command]
    pub fn inspector_ready(&self, ctx: &mut dyn Context) -> Result<bool> {
        reader(ctx, |md, ctx| md.inspector_ready(ctx))
    }

    /// Start a new receiving epoch at the first controlled chunk.
    #[command]
    pub fn begin_stream(&mut self, ctx: &mut dyn Context) -> Result<()> {
        reader(ctx, |md, ctx| {
            md.set_source_only(ctx, false, None)?;
            md.replace(ctx, DocumentInput::Receiving(STREAM[0].into()))
                .map(|_| ())
        })?;
        self.next_chunk = 1;
        self.receiving = true;
        self.state = "receiving 1/6".into();
        self.show_status(ctx)
    }
    /// Accept exactly one known chunk. This command never advances a clock.
    #[command]
    pub fn append_chunk(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if !self.receiving {
            return Ok(());
        }
        let Some(chunk) = STREAM.get(self.next_chunk) else {
            return Ok(());
        };
        reader(ctx, |md, ctx| md.append(ctx, chunk).map(|_| ()))?;
        self.next_chunk += 1;
        self.state = format!("receiving {}/{}", self.next_chunk, STREAM.len());
        self.show_status(ctx)
    }
    #[command]
    /// Complete the current stream without accepting additional bytes.
    pub fn finish(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if !self.receiving {
            return Ok(());
        }
        reader(ctx, |md, ctx| {
            md.finish(ctx, FinishOutcome::Completed).map(|_| ())
        })?;
        self.state = "finished".into();
        self.receiving = false;
        self.show_status(ctx)
    }
    #[command]
    /// Retain accepted bytes and finish with an explicit abort outcome.
    pub fn abort(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if !self.receiving {
            return Ok(());
        }
        reader(ctx, |md, ctx| {
            md.finish(ctx, FinishOutcome::Aborted("Controlled gym abort".into()))
                .map(|_| ())
        })?;
        self.state = "aborted".into();
        self.receiving = false;
        self.show_status(ctx)
    }
    #[command]
    /// Replace the document with the complete rich fixture in a new epoch.
    pub fn replace(&mut self, ctx: &mut dyn Context) -> Result<()> {
        reader(ctx, |md, ctx| {
            md.set_source_only(ctx, false, None)?;
            md.replace(ctx, DocumentInput::Complete(FIXTURE.into()))
                .map(|_| ())
        })?;
        self.next_chunk = 0;
        self.receiving = false;
        self.state = "complete fixture".into();
        self.show_status(ctx)
    }
    /// Show exact CRLF, a tab, and a final newline for Source copy checks.
    #[command]
    pub fn exact_source(&mut self, ctx: &mut dyn Context) -> Result<()> {
        reader(ctx, |md, ctx| {
            md.set_source_only(ctx, false, None)?;
            md.replace(ctx, DocumentInput::Complete(EXACT_SOURCE.into()))?;
            md.set_view_type(ctx, ViewType::Source)
        })?;
        self.state = "exact CRLF fixture".into();
        self.receiving = false;
        self.show_status(ctx)
    }
    /// A large code payload deliberately exceeds automatic syntax preparation.
    #[command]
    pub fn syntax_limit(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let payload = "let plain_above_syntax_cap = 42;\n".repeat(9000);
        let source = format!("# Syntax limit\n\n```rust\n{payload}```\n");
        reader(ctx, |md, ctx| {
            md.set_source_only(ctx, false, None)?;
            md.replace(ctx, DocumentInput::Complete(source)).map(|_| ())
        })?;
        self.state = "syntax limit fixture".into();
        self.receiving = false;
        self.show_status(ctx)
    }
    /// Demonstrate a host-limited prefix without presenting it as a complete
    /// parse.
    #[command]
    pub fn partial_source(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let prefix = FIXTURE.chars().take(240).collect::<String>();
        let status = format!(
            "Partial file · retained bytes 0..{} · Rendered unavailable",
            prefix.len()
        );
        reader(ctx, |md, ctx| {
            md.replace(ctx, DocumentInput::Complete(prefix))?;
            md.set_source_only(ctx, true, Some(status))
        })?;
        self.state = "partial source prefix".into();
        self.receiving = false;
        self.show_status(ctx)
    }
    /// A dangerous scheme stays inspectable but cannot publish an Open action.
    #[command]
    pub fn unavailable(&mut self, ctx: &mut dyn Context) -> Result<()> {
        reader(ctx, |md, ctx| {
            md.set_source_only(ctx, false, None)?;
            md.replace(
                ctx,
                DocumentInput::Complete(
                    "# Unavailable actions\n\n[Unsafe destination](javascript:alert%281%29)\n"
                        .into(),
                ),
            )?;
            md.set_view_type(ctx, ViewType::Rendered)
        })?;
        self.state = "unavailable browser action".into();
        self.receiving = false;
        self.show_status(ctx)
    }
    #[command]
    /// Whether the current host policy allows switching presentations.
    pub fn rendered_available(&self, ctx: &mut dyn Context) -> Result<bool> {
        reader(ctx, |md, ctx| md.can_toggle_view_type(ctx))
    }
    #[command]
    /// Toggle the reader between full width and a 40-cell region.
    pub fn narrow(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.narrow = !self.narrow;
        if let Some(node) = ctx.child_slot_of(ctx.node_id(), <ReaderSlot as ChildSlot>::KEY) {
            ctx.set_layout_override(node, self.reader_layout())?;
        }
        Ok(())
    }
    #[command]
    /// Toggle the live palette between dark and light.
    pub fn theme(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.light = !self.light;
        ctx.set_theme(if self.light {
            themes::solarized_light()
        } else {
            themes::default_dark()
        });
        Ok(())
    }
    /// Reset the fixture with the other geometry preset.
    #[command]
    pub fn glyphs(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.ascii = !self.ascii;
        if let Some(node) = ctx.child_slot_of(ctx.node_id(), <ReaderSlot as ChildSlot>::KEY) {
            ctx.remove_subtree(node)?;
        }
        let node = ctx.add_slot::<ReaderSlot>(
            ctx.node_id(),
            Markdown::new(FIXTURE.into(), self.options())?,
        )?;
        ctx.set_layout_override(node.into(), self.reader_layout())?;
        self.state = "complete fixture".into();
        self.next_chunk = 0;
        self.receiving = false;
        self.show_status(ctx)
    }

    #[command]
    /// Search the active readable projection with a literal query.
    pub fn search(&self, ctx: &mut dyn Context, query: String) -> Result<()> {
        reader(ctx, |md, ctx| md.set_search(ctx, query, false))
    }
    #[command]
    /// Enter object interaction mode through the public controller.
    pub fn interact(&self, ctx: &mut dyn Context) -> Result<()> {
        reader(ctx, |md, ctx| md.set_mode(ctx, Mode::Interact))
    }
    #[command]
    /// Retained match count for the active presentation.
    pub fn matches(&self, ctx: &mut dyn Context) -> Result<usize> {
        reader(ctx, |md, _| Ok(md.search_matches()))
    }
    #[command]
    /// Record pending immutable host requests and acknowledge them once.
    pub fn collect_host(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let requests = reader(ctx, |md, ctx| {
            let requests = md.take_requests(16);
            for request in &requests {
                md.complete_request(
                    ctx,
                    HostReply {
                        id: request.id,
                        outcome: HostOutcome::Success,
                    },
                )?;
            }
            Ok(requests)
        })?;
        if !requests.is_empty() {
            self.requests.extend(requests);
            if self.requests.len() > 64 {
                self.requests.drain(..self.requests.len() - 64);
            }
            self.show_status(ctx)?;
        }
        Ok(())
    }
    #[command]
    /// Number of requests retained in the bounded recording log.
    pub fn host_actions(&self) -> usize {
        self.requests.len()
    }
    #[command]
    /// Most recently recorded complete copy payload.
    pub fn last_copy(&self) -> String {
        self.requests
            .iter()
            .rev()
            .find_map(|r| {
                if let HostAction::CopyText(text) = &r.action {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default()
    }
    #[command]
    /// Most recently recorded resolved browser destination.
    pub fn last_url(&self) -> String {
        self.requests
            .iter()
            .rev()
            .find_map(|r| {
                if let HostAction::OpenUrl(url) = &r.action {
                    Some(url.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default()
    }
    #[command]
    /// Displayed revision pinned by the most recent host action.
    pub fn last_revision(&self) -> u64 {
        self.requests.last().map_or(0, |r| r.revision)
    }
}

impl Widget for MarkdownGym {
    fn name(&self) -> NodeName {
        NodeName::convert("markdown_gym")
    }
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let status =
            ctx.add_slot::<StatusSlot>(ctx.node_id(), Text::new("").with_style("markdown/status"))?;
        ctx.set_layout_override(status.into(), fixed_row(1))?;
        let controls=ctx.add_child(ctx.node_id(),Text::new("Alt: s stream · a append · f finish · x abort · r replace · w width · t theme · u glyphs"))?;
        ctx.set_layout_override(controls.into(), fixed_row(1))?;
        let node = ctx.add_slot::<ReaderSlot>(
            ctx.node_id(),
            Markdown::new(FIXTURE.into(), self.options())?,
        )?;
        ctx.set_layout_override(node.into(), self.reader_layout())?;
        self.show_status(ctx)
    }
    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        self.collect_host(ctx)?;
        Ok(Some(Duration::from_millis(16)))
    }
}
impl Register for MarkdownGym {
    fn register(setup: &mut Setup) -> Result<()> {
        Markdown::register(setup)?;
        setup.add_commands::<Self>()
    }
}
/// Add the gym's explicit stream and presentation control bindings.
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.script("markdowngym", BINDINGS)
}
