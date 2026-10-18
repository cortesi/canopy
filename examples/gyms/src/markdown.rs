//! Markdown with deterministic stream controls and a recording host.

use std::{sync::Arc, time::Duration};

use canopy::{
    CanopyBuilder, Context, ContextExt, NodeName, Register, Setup, Widget, derive_commands,
    error::Result, layout::Layout, style::themes, tree::ChildSlot,
};
use canopy_markdown::{
    DocumentInput, FinishOutcome, Markdown, MarkdownOptions, Mode, ViewType,
    host::{HostAction, HostOutcome, HostReply, HostRequest, Resource, ResourceKind},
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

/// Gym controls installed in addition to the standard reader bindings. Plain
/// letters the reader leaves free open which-key menus; `a` stays one key
/// because a stream is appended repeatedly.
const BINDINGS: &str = r#"
root.default_bindings()
canopy.keymap({
    path = "markdown_gym",
    { key = "a", description = "Append next stream chunk", action = command.markdown_gym.append_chunk() },
    { key = "s", description = "Stream", action = canopy.menu("stream") },
    { key = "d", description = "Document", action = canopy.menu("document") },
    { key = "p", description = "Presentation", action = canopy.menu("presentation") },
})
canopy.keymap({
    mode = "stream",
    { key = "s", description = "Start a controlled stream", action = command.markdown_gym.begin_stream() },
    { key = "a", description = "Append next chunk", action = command.markdown_gym.append_chunk() },
    { key = "f", description = "Finish", action = command.markdown_gym.finish() },
    { key = "x", description = "Abort", action = command.markdown_gym.abort() },
})
canopy.keymap({
    mode = "document",
    { key = "f", description = "Full fixture", action = command.markdown_gym.replace() },
    { key = "e", description = "Exact CRLF source", action = command.markdown_gym.exact_source() },
    { key = "l", description = "Over the syntax limit", action = command.markdown_gym.syntax_limit() },
    { key = "p", description = "Partial source prefix", action = command.markdown_gym.partial_source() },
    { key = "u", description = "Unsafe link", action = command.markdown_gym.unavailable() },
})
canopy.keymap({
    mode = "presentation",
    { key = "t", description = "Light / dark theme", action = command.markdown_gym.theme() },
    { key = "g", description = "ASCII / Unicode glyphs", action = command.markdown_gym.glyphs() },
})
"#;
/// Host requests drained and acknowledged per collection.
const REQUEST_BATCH: usize = 16;
/// Recorded host requests retained for queries.
const REQUEST_LOG: usize = 64;

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
            local_links: true,
            resources: true,
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
    /// Record a lifecycle state and publish it to the status line.
    fn set_state(
        &mut self,
        ctx: &mut dyn Context,
        state: impl Into<String>,
        receiving: bool,
    ) -> Result<()> {
        self.state = state.into();
        self.receiving = receiving;
        self.show_status(ctx)
    }
    /// Replace the document in a new epoch with both presentations available.
    fn load(
        &mut self,
        ctx: &mut dyn Context,
        input: DocumentInput,
        state: impl Into<String>,
    ) -> Result<()> {
        let receiving = matches!(input, DocumentInput::Receiving(_));
        reader(ctx, |md, ctx| {
            md.set_source_only(ctx, false, None)?;
            md.replace(ctx, input).map(|_| ())
        })?;
        self.next_chunk = usize::from(receiving);
        self.set_state(ctx, state, receiving)
    }
    /// Mount a reader over the complete fixture with the current options and
    /// region.
    fn mount_reader(&self, ctx: &mut dyn Context) -> Result<()> {
        let node = ctx.add_slot::<ReaderSlot>(
            ctx.node_id(),
            Markdown::new(FIXTURE.into(), self.options())?,
        )?;
        ctx.set_layout_override(node.into(), flex_row(1))
    }
    /// Return the most recent recorded action that `pick` accepts.
    fn last_action(&self, pick: impl Fn(&HostAction) -> Option<&String>) -> String {
        self.requests
            .iter()
            .rev()
            .find_map(|request| pick(&request.action))
            .cloned()
            .unwrap_or_default()
    }

    /// A worker/publication readiness predicate, independent of frame timing.
    #[command]
    pub fn ready(&self, ctx: &mut dyn Context) -> Result<bool> {
        reader(ctx, |md, _| Ok(md.ready() && !md.resources_pending()))
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
        self.load(
            ctx,
            DocumentInput::Receiving(STREAM[0].into()),
            format!("receiving 1/{}", STREAM.len()),
        )
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
        let state = format!("receiving {}/{}", self.next_chunk, STREAM.len());
        self.set_state(ctx, state, true)
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
        self.set_state(ctx, "finished", false)
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
        self.set_state(ctx, "aborted", false)
    }
    #[command]
    /// Replace the document with the complete rich fixture in a new epoch.
    pub fn replace(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.load(
            ctx,
            DocumentInput::Complete(FIXTURE.into()),
            "complete fixture",
        )
    }
    /// Show exact CRLF, a tab, and a final newline for Source copy checks.
    #[command]
    pub fn exact_source(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.load(
            ctx,
            DocumentInput::Complete(EXACT_SOURCE.into()),
            "exact CRLF fixture",
        )?;
        reader(ctx, |md, ctx| md.set_view_type(ctx, ViewType::Source))
    }
    /// A large code payload deliberately exceeds automatic syntax preparation.
    #[command]
    pub fn syntax_limit(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let payload = "let plain_above_syntax_cap = 42;\n".repeat(9000);
        let source = format!("# Syntax limit\n\n```rust\n{payload}```\n");
        self.load(ctx, DocumentInput::Complete(source), "syntax limit fixture")
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
        self.load(
            ctx,
            DocumentInput::Complete(prefix),
            "partial source prefix",
        )?;
        reader(ctx, |md, ctx| md.set_source_only(ctx, true, Some(status)))
    }
    /// A dangerous scheme stays inspectable but cannot publish an Open action.
    #[command]
    pub fn unavailable(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.load(
            ctx,
            DocumentInput::Complete(
                "# Unavailable actions\n\n[Unsafe destination](javascript:alert%281%29)\n".into(),
            ),
            "unavailable browser action",
        )?;
        reader(ctx, |md, ctx| md.set_view_type(ctx, ViewType::Rendered))
    }
    #[command]
    /// Whether the current host policy allows switching presentations.
    pub fn rendered_available(&self, ctx: &mut dyn Context) -> Result<bool> {
        reader(ctx, |md, ctx| md.can_toggle_view_type(ctx))
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
    /// Reset the fixture with the other geometry preset. Focus moves to the
    /// new reader when the old one held it.
    #[command]
    pub fn glyphs(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.ascii = !self.ascii;
        let focused = self.focused(ctx)?;
        if let Some(node) = ctx.child_slot_of(ctx.node_id(), <ReaderSlot as ChildSlot>::KEY) {
            ctx.remove_subtree(node)?;
        }
        self.mount_reader(ctx)?;
        if focused {
            self.focus_reader(ctx)?;
        }
        self.set_state(ctx, "complete fixture", false)
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
            let requests = md.take_requests(REQUEST_BATCH);
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
            let excess = self.requests.len().saturating_sub(REQUEST_LOG);
            self.requests.drain(..excess);
            self.show_status(ctx)?;
        }
        Ok(())
    }
    /// Answer each resource request without files: images with the gym's
    /// own landscape, embeds with a short note, and link checks with found,
    /// except for a link to `Nowhere`.
    fn serve_resources(&self, ctx: &mut dyn Context) -> Result<()> {
        reader(ctx, |md, ctx| {
            for request in md.take_resource_requests() {
                let resource = match request.kind {
                    ResourceKind::Image => Resource::Image(landscape()),
                    ResourceKind::Note => Resource::Text(EMBEDDED_NOTE.into()),
                    ResourceKind::Link if request.target.path == "Nowhere" => Resource::Missing,
                    ResourceKind::Link => Resource::Found,
                };
                md.supply_resource(ctx, request.id, resource)?;
            }
            Ok(())
        })
    }
    #[command]
    /// Number of requests retained in the bounded recording log.
    pub fn host_actions(&self) -> usize {
        self.requests.len()
    }
    #[command]
    /// Most recently recorded complete copy payload.
    pub fn last_copy(&self) -> String {
        self.last_action(|action| match action {
            HostAction::CopyText(text) => Some(text),
            HostAction::OpenUrl(_) | HostAction::OpenLocal(_) => None,
        })
    }
    #[command]
    /// Most recently recorded resolved browser destination.
    pub fn last_url(&self) -> String {
        self.last_action(|action| match action {
            HostAction::OpenUrl(url) => Some(url),
            HostAction::CopyText(_) | HostAction::OpenLocal(_) => None,
        })
    }
    #[command]
    /// Path of the most recently recorded local link follow.
    pub fn last_local(&self) -> String {
        self.last_action(|action| match action {
            HostAction::OpenLocal(link) => Some(&link.path),
            HostAction::CopyText(_) | HostAction::OpenUrl(_) => None,
        })
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
        self.mount_reader(ctx)?;
        self.show_status(ctx)
    }
    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        self.collect_host(ctx)?;
        self.serve_resources(ctx)?;
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

/// The note the gym supplies for every embed; an embed shows the section its
/// anchor names.
const EMBEDDED_NOTE: &str = "# Verber\n\nAn embedded note.\n\n## Plans\n\nShip the trajectory, then the embeds. ^ship\n\nKeep the reader quiet.\n\n## Later\n\nNot embedded.\n";

/// A small landscape: sky over hills, with a sun, drawn without a file.
fn landscape() -> Arc<image::RgbaImage> {
    Arc::new(image::RgbaImage::from_fn(192, 96, |x, y| {
        let (fx, fy) = (x as f32 / 192.0, y as f32 / 96.0);
        let hill = 0.62 + 0.12 * (fx * 7.0).sin() + 0.06 * (fx * 17.0).cos();
        let sun = (fx - 0.72).powi(2) * 4.0 + (fy - 0.3).powi(2) < 0.012;
        let rgb = if sun {
            [250, 214, 120]
        } else if fy > hill {
            let shade = 1.0 - (fy - hill) * 1.2;
            [
                (52.0 * shade) as u8,
                (120.0 * shade) as u8,
                (88.0 * shade) as u8,
            ]
        } else {
            [
                (120.0 + 90.0 * fy) as u8,
                (140.0 + 60.0 * fy) as u8,
                (210.0 - 20.0 * fy) as u8,
            ]
        };
        image::Rgba([rgb[0], rgb[1], rgb[2], 255])
    }))
}
