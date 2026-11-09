# Review: Canopy's API and its users

## Description

This is a review of the Canopy workspace against its applications, at
`8ef27b63` on 2026-11-10. It covers the public API as a caller reads it in
the captures in `api/`, the runtime, the widgets, the Markdown and options
crates, the automation tooling, and the tests. `ncode api --check` passed, so
the captures match the tree. Each item was checked in the code at its cited
lines, and each count was made in the workspace and the applications.

The applications are fh (`../fh`), verber
(`../../private/verber-mono/verber`), epea (`../../private/epea`), and
markkit (`../../private/markkit`). Backward compatibility is not a
constraint. A change takes the cleaner design, whatever the number of call
sites, and the applications adjust after it. The counts below are evidence of
need, not of cost.

Each item under Changes can be implemented without a further decision. Where
a change contains a choice, the choice is stated. Items that need a design
choice are under Decisions, each with the answer this review recommends.

Paths written from `core/` are under `crates/canopy/src/`. Line numbers are
those of the tree at the review commit. Every API change ends with `ncode
api`. The Execution Plan at the end lists each change, and each part of a
change that has several, as one item to check off.

### Assessment

The runtime crate is coherent: sealed contexts with typed extension traits,
one result type for a state change, one value for a command call. Its
problems are at the edges: a conversion and two modules that serve one
caller, docs that drifted, and two routing defects. `canopy-widgets` is where
conventions vary, because each widget settled its own names for the same
ideas. `canopy-markdown` publishes three layers at once: the reader, the
pipeline that a host paints with, and the worker protocol between them. The
first two have consumers, and the third has none. `canopy-xmark` keeps the
shape of its upstream, as its plan requires.

Three items are defects with an effect a user can meet: C54, where the
default options of the Markdown reader start worker threads that fh and epea
discard, C24, where a long menu cannot scroll, and C25, where the spinner of
a diff view turns only while its host polls. D16 is a soundness defect in the
one `unsafe` block of the repository.

### Coverage

- The captures use default features. They omit `canopy::testing`, the
  `testing` module of canopy-markdown, and the `escape`, `github`, and `html`
  modules of canopy-xmark. The API reading did not cover those. See D7.
- Five `Unnameable` entries. Three are the sealed supertraits of `Context`,
  `ViewContext`, and `Inject`, which are deliberate. `CoreOwner` is a
  crate-private trait that `Canopy` implements, in no public signature.
  `ZoomDirection` is a real gap, and C29 closes it.
- The review ran no build, test, clippy, or Miri. Every statement that a
  change compiles comes from reading signatures. Nothing was measured, so the
  review makes no performance claim.
- Usage counts are text searches over Rust sources, and over Luau where a
  command is concerned.

## Changes

### Documentation

#### C1: One doc comment for each public module and re-export

A `///` comment on a `pub mod` declaration joins the module's own `//!`
comment, so the captures show two leading sentences.

- `core/mod.rs:6` puts `Keyed child collection helpers.` on `pub mod
  commands`, above its real doc at `:7`. Delete line 6. `commands.rs` has no
  `//!`, so line 7 stays.
- `core/mod.rs:9` (`cursor`), `:31` (`status`), `core/style/mod.rs:4`
  (`effects`), and `core/event/mod.rs:1` (`key`) repeat or contradict the
  module's `//!`. Delete the outer line.
- `crates/canopy-widgets/src/lib.rs` has a `///` on twelve modules that have
  their own `//!` (lines 14, 28, 34, 44, 48, 63, 67, 74, 80, 88, 107, 124).
  Delete the outer lines.
- `core/script/mod.rs:35` puts `Base canopy scripting API declarations and
  native registration.` on a `pub use` group, so eleven re-exported items
  carry that doc in `api/canopy.rs` in place of their own. Delete the line.
  `base_api.rs:1` already has it.

#### C2: The docs say what the code does

- `docs/getting-started.md:307-308` says a `LaunchMode::Api` prints the Luau
  API. `LaunchMode` has `Run` and `HeadlessMcp`
  (`crates/canopy-mcp/src/launch.rs:23-33`), and each binary prints the API
  itself.
- `docs/agent-loop.md:21-22` says the third `launch` argument is a
  `RunOptions`, and names API-output modes. `launch` takes two arguments
  (`launch.rs:41`).
- `docs/fixtures.md:25` says the reported reset policy is always the declared
  one. A live session reports `ResetPolicy::External`
  (`crates/canopy-mcp/src/metadata.rs:241-242`). Line 70 says `cargo xtask
  smoke` finds every `.canopyctl.toml`: it lists tracked files
  (`xtask/src/main.rs:402-405`). Line 71 names two suites, and five are
  tracked.
- `docs/markdown.md:137` names `AlertTone::of`, which does not exist. Lines
  347-348 say the crate does not need the `graphics` feature of
  `canopy-widgets`, which its manifest enables (`Cargo.toml:21`).
- `docs/architecture.md:39-41` and `:89-91` list the public modules of
  `canopy-widgets` and omit `chart`, `fuzzy`, `menu`, `regex_search`,
  `table`, and `view_search`. The first list also omits `mosaic`.
- `docs/architecture.md:258-259` says every layout pass ends with
  `validate_invariants`. It runs in debug builds
  (`core/world/layout_driver/mod.rs:48`).
- `docs/architecture.md:71-72` says `Canopy` holds only operations that
  cannot live on a context or on `Setup`. It also holds the facade that
  tests and tools use (D15). Say so.
- `docs/options.md:222-225` omits the toast of `toggle` and `cycle`
  (`crates/canopy-options/src/widget.rs:193,215`).
- `docs/scripting.md:441` names a builder method `config(path)`. The method
  is `script_file`.
- `docs/styles.md:121` names `Context::set_style`, which C5 removes.

#### C3: Doc comments describe their items

- `ChildSlot` reads `A typed slot for slots.` and `slot!` reads `Define a
  typed slot for slots.` (`core/context.rs:32,55`). Say a typed key for a
  keyed child. Both examples import `canopy::{ChildSlot, Widget, slot}`
  (`:40,60`), and `ChildSlot` is at `canopy::tree::ChildSlot` only
  (`lib.rs:70-75`). Doctests are off in every manifest, so nothing compiles
  the examples. Correct the import.
- The `StyleMap::rules` example imports `canopy::style::solarized`, which
  does not exist, and uses `Color` without an import (`core/style/mod.rs:737`).
- The crate docs list seven modules under `# Module Organization`
  (`lib.rs:14`). The crate also publishes `commands`, `error`, `path`,
  `status`, `style`, `terminal`, and `text`. List them.
- `ViewContext` says it is available "during render and measure"
  (`context.rs:105`), and `measure` takes no context. `node_id` says "the
  node currently being rendered" (`:110`), and contexts exist in events and
  polls.
- `Context::edit_structure` says rollback restores "diagnostic requests"
  (`context.rs:709`). `TreeStateSnapshot` (`core/world/mod.rs:162-183`) has
  none, and it restores the modals, which the doc omits.
- `Context::remove_after_dispatch` says "Remove the current widget
  incarnation" and takes a node (`context.rs:780`). Say the node's current
  incarnation.
- `Error::Invalid` and `Error::NotFound` are fatal when a poll or a handler
  returns them (`core/error.rs:446-447`), and their docs do not say so
  (`:268-273`). verber builds `Error::Invalid` 43 times and fh 26. Say on
  each variant that it is a runtime error, and that an application's own
  failure is `Error::app` (C6).
- `Error::Parse` has its doc after its `#[error]` attribute
  (`core/error.rs:326-328`). `Path::new` says "from a slice" and takes an
  iterator (`core/path.rs:32-33`).
- `host_prepare` names `canopy.flush`; the function is `prepare`
  (`core/script/base_api.rs:2141`, `:386`). The `bind` docs omit `MenuAction`
  (`:647-648`, `:1748`, `:1799-1800`, `:1826-1827`).

#### C4: Getting started matches the hello example

`docs/getting-started.md` says its code comes from `examples/hello`. Of its
eight Rust blocks, the second has drifted: line 134 adds
`.with_style("status_bar/text")`, which `examples/hello/src/lib.rs:72` does
not have. Decision: the page follows the example. Remove the call from the
page, and add a test to `examples/hello/tests` that reads the page, dedents
each Rust block, applies the one documented substitution of the smoke path
(`join("smoke")` to `join("../../smoke")`), and asserts that the block is in
`src/lib.rs`, `src/main.rs`, or `tests/smoke.rs`.

### Runtime API

#### C5: Remove `Context::set_style` and `Render::restyle`

`Context::set_style` (`core/context.rs:893-896`) has no caller in any
application or script. Its doc says to prefer `set_theme`. Remove it. Two
runtime tests call it (`core/world/tests.rs:211`,
`core/world/change_tests.rs:60`) and change with it, and `StyleChange`
(`core/style/mod.rs:61-66`) then has one variant and goes too.

`Render::restyle` (`core/render/mod.rs:404-406`) has no caller anywhere.
Remove it with `TermBuf::restyle_grapheme` (`core/termbuf/mod.rs:448`), which
only it and three tests use.

`commands::SerdeArg` (`core/commands.rs:529-530`) has one internal caller and
the derive does not use it. Make it `pub(crate)`, and move the one test that
names it (`crates/canopy/tests/it/commands.rs:11,226`) into the crate.

#### C6: `Error::app` constructs the application error

`Error::App(Box<dyn Error + Send + Sync>)` is the notice-class variant for an
application's own failure. It is built by hand 69 times: 25 in the workspace,
35 in fh, 7 in epea, and 2 in verber. `canopy_mcp::Error::app`
(`crates/canopy-mcp/src/error.rs:43`) is the model. Add `Error::app(error:
impl Into<Box<dyn Error + Send + Sync>>) -> Self`, so a caller writes
`.map_err(Error::app)`.

#### C7: `Constraint::limit`

Twenty match arms pick a fallback for `Constraint::Unbounded` and treat
`AtMost` and `Exact` alike: ten in canopy-widgets (`list_view.rs:890`,
`confirm.rs:625`, `font_banner.rs:172`, `inspector/logs.rs:119`,
`text.rs:254`, `diff_view.rs:1202,1215`, `table_view/mod.rs:927`,
`editor/widget.rs:1655`, `big_text/mod.rs:583-586`), one in the runtime
(`core/world/layout_driver/mod.rs:798-802`), two in the examples, six in
verber, and one in markkit. Add `Constraint::limit(self) -> Option<u32>`,
which returns the bound of `AtMost` and `Exact`, and `None` for `Unbounded`.
`notice_log.rs:180` and `help/binding_list.rs:221` treat the two bounds
differently and do not change.

#### C8: `CanvasContext` carries the glyphs

`MeasureConstraints` carries `glyphs` so a widget measures as it draws
(`crates/canopy/src/layout.rs:765`). `CanvasContext` holds only its children
(`layout.rs:821-847`), so a widget whose canvas depends on the mosaic tier
reads the glyphs in `render`, compares them with the tier it used, and wakes
itself to lay out again: `verber-conversation/src/widget.rs:474-480`,
`reasoning.rs:191`, and `crates/canopy-markdown/src/widget/mod.rs:3261-3270`.
Give `CanvasContext` a `glyphs()` accessor, filled as
`MeasureConstraints::glyphs` is. `CanvasContext::new` has three callers, all
in the runtime.

#### C9: `ViewContext::status_of`

Six sites read another widget's status items with the same lines:
`crates/canopy-widgets/src/status_line.rs:75-86`, fh
`canopy-fileselect/src/status.rs:67-73,123-127,135-139`, fh
`fh-audio/src/app.rs:2592-2597`, and epea `src/app.rs:1484`. Add
`status_of(&self, node: NodeId) -> Result<Vec<StatusItem>>`. It is a required
method of `ViewContext`, implemented once in `NodeCtx` (`context.rs:1130`).
It cannot be a default method of `ViewContextExt`, because `Widget::status`
takes `&dyn ViewContext` and the extension trait is implemented for unsized
types.

#### C10: `ContextExt::update` for setters

188 calls to `with_widget_mut` take a closure that ignores its context and
ends in `Ok(())`: 85 in the workspace, 57 in verber, 41 in fh, and 5 in
epea. Each sets a field or calls a setter, and none uses `?`. Add

```rust
fn update<W, R>(&mut self, node: impl Into<NodeId>, f: impl FnOnce(&mut W) -> R) -> Result<R>
```

to `ContextExt`. It calls `with_widget_mut`, so it invalidates layout as that
does.

#### C11: `BindingOptions` has constructors

Every native binding builds `BindingOptions` as a literal of six fields, of
which `show_in_help: true` and `source: None` never vary. Seventeen sites
install a framework group this way: `confirm.rs:499-512`,
`list_view.rs:135-148`, `menu.rs:528-544`, `picker.rs:1006-1022`,
`answers.rs:404-423`, `button.rs:1214-1225`, `root.rs:569-583`,
`canopy-options/src/panel.rs:318-331`,
`canopy-markdown/src/inspector.rs:1142-1154`, gyms `shell.rs:221-234`, fh
`commander/pickers.rs:672-690`, and six loops in verber
(`verber/src/keys.rs:656-668`, `grants.rs:444-463`, `models/form.rs:251-263`,
`models/card.rs:501-513`, `models/mod.rs:1991-2006`,
`verber-conversation/src/question_dialog.rs:259-271`).

The sites differ in what they bind: commands, intents, a phase for each key,
or mouse and key input together. A helper that takes a table of keys and
commands fits seven of them, and one general enough for all is `Setup::bind`
itself. Decision: give the struct constructors, and leave `Setup::bind` as
the one call.

```rust
impl BindingOptions {
    /// Shown in help, with no path, phase, or source.
    pub fn new(tier: BindingTier, description: impl Into<String>) -> Self;
    pub fn with_path(self, path: PathFilter) -> Self;
    pub fn with_phase(self, phase: BindingPhase) -> Self;
    pub fn hidden(self) -> Self;
}
```

#### C12: `Rect` has public edges, and `Line::indent` goes

`Rect::right` and `Rect::bottom` are private `u64` helpers
(`crates/canopy-geom/src/rect.rs:32-39`), and 34 sites write
`tl.x.saturating_add(w)` or its twin by hand, for example
`crates/canopy/src/layout.rs:842-843`, `core/termbuf/mod.rs:407,431-432`,
`canopy-widgets/src/list_view.rs:856`, and `menu.rs:615`. Make `right` and
`bottom` public accessors that saturate to `u32`, and rename the two `u64`
helpers that `contains_point` and `contains_rect` use.

`Line::indent` (`line.rs:39`) has no caller but its own test. Remove both.

#### C13: Remove `From<mpsc::RecvError> for Error`

`core/error.rs:487-491` converts a closed `std::sync::mpsc` channel to
`Error::Driver`, a fatal class. The one use is `AutomationHandle::request`
(`core/canopy/mod.rs:271`). The impl is public, so `rx.recv()?` in an
application callback compiles and reports the application's closed channel
as a failure of the turn driver, which ends the run. No application relies
on it. Map the error at `mod.rs:271`, and remove the impl.

#### C14: `canopy::style::animation` is private

`core/style/mod.rs:1` declares `pub mod animation`, and `:19` exports its
five types at `canopy::style`. Each type has two paths, and no consumer uses
the longer one. Make the module private.

#### C15: `canopy::format` moves to `canopy_widgets::format`

`canopy::format` holds formats of ages, counts, sizes, durations, and paths
below a home directory (`core/format/mod.rs`, 410 lines, `std` only). Nothing
in the runtime crate uses it. Its callers are `canopy-widgets`
(`notice_log.rs`), `canopy-options` (`file.rs`), fh, and verber, and each
depends on `canopy-widgets`. Move the module there.

#### C16: An enum's script label is `label`

A stable scripting label is `label()` on `CommandStatus`, `BindingAction`,
`BindingActionKind`, `BindingPhase`, `BindingTier`, `RouteTraceKind`,
`NoticeLevel`, and `NoticeSource`, and `as_str()` on `CommandRequirement` and
`ScriptErrorKind`. The reverse is `BindingPhase::parse` and
`NoticeLevel::from_label`. Rename the three outliers, so these enums have
`label` and `from_label`, and `as_str` is the accessor of a newtype over a
string, such as `IntentName`.

### Runtime defects

#### C17: `focused_within` answers containment alone

`Core::focused_within` requires the focused node to be a focus candidate
with a view (`core/world/focus.rs:108-112`), and candidacy reads the widget's
cell (`focus.rs:522-527`). Inside the focused widget's own `render` or
callback the cell is borrowed, so `focused_within(self)` returns `None` while
`is_focused()` is true. The doc promises containment only
(`core/context.rs:199-201`), and the prediction branch
(`context.rs:1238-1243`) applies no candidacy. Make `Core::focused_within`
return the focus when `root` contains it. A test calls it from the focused
widget's `render`. `NodeCtx::is_focused`, `is_on_focus_path`, and
`focused_within` (`context.rs:1215-1243`) then all derive from
`focused_node()`.

#### C18: Intent dispatch uses the route focus

Routing selects an intent with the route focus
(`core/canopy/routing.rs:296-298,345`), and `dispatch_action_on_node`
computes `self.focus.unwrap_or(self.root)` again (`core/select.rs:181`). A
widget earlier on the route that moves focus and returns `Ignore` makes the
navigation fallback disagree with the selection that admitted it, and can
trip the `debug_assert_eq!` at `routing.rs:852-856`. The reader of
canopy-markdown and fh's browser can both produce the case. Pass the route
focus in from the one caller (`routing.rs:849`), with a test.

#### C19: Two reachable panics in styling

- `render_pass` expects the root style to resolve solid
  (`core/canopy/rendering.rs:252-255`). `Canopy::style_mut().rules().fg("",
  gradient).apply()` puts a gradient in the root rule, and the next frame
  panics. Resolve the root style at the top left cell of the screen.
- `GradientSpec::stops` and `Animation::stops` are public fields
  (`core/style/mod.rs:241`, `animation.rs:162`). An emptied list panics at
  `mod.rs:377` and at `animation.rs:369,393`. The constructors already
  replace an empty list with white (`mod.rs:253-256`, `animation.rs:182-184`).
  Decision: make both fields private with a `stops()` accessor, so the
  constructors' rule always holds.

#### C20: The Luau surface agrees with itself

- `KeyRouteOutcome.action` is declared as `script`, `command`, or `intent`
  (`core/script/defs.rs:705-709`), and the emitter writes
  `BindingActionKind::label`, which includes `menu` (`records.rs:916-919`).
  Declare the four, as `action_field()` does (`defs.rs:375-381`). The field
  stays optional.
- `parse_bind_options` and `parse_unbind_selector` read their fields one by
  one and ignore the rest (`core/script/base_api.rs:676-724`, `:727-739`).
  `keymap`, `set_cursor_look`, `set_motion`, and `notify` reject an unknown
  field through `option_fields` (`:742-764`). Reject unknown fields in both.
- `node_id_from_value` writes `kind = type_mismatch` as bare fields with no
  payload (`core/script/bridge.rs:275-290`), so Luau sees `type_mismatch` and
  Rust sees `canopy_error`. Build the payload as the other errors do.
- A `BindingId` crosses to Luau as `i64` (`base_api.rs:1109`, then a float at
  `:1838`) and as `UInt` (`records.rs:306,649,872,909`), and comes back from
  either (`:1496-1497`). Emit `UInt` everywhere, and read it with one
  function.

### Runtime internals

#### C21: `PublicationWatch` has one lock accessor

`PublicationWatch` writes `.lock().expect("publication lock poisoned")`
eight times (`core/canopy/turn.rs:204,208,218,247,260,280,289,297`). Add one
`state()` accessor.

#### C22: Lint attributes in the runtime

`core/mod.rs:1` carries `#![allow(clippy::new_without_default)]` over the
whole core tree. Add the `Default` impls, or narrow the attribute to the
types that need it with `#[expect]` and a reason.
`core/style/color.rs:390,406,499,536` and `core/event/key.rs:49,57,65,73`
use `#[allow]`. Use `#[expect]` with a reason, as the four
`multiple_inherent_impl` attributes do. An `#[expect]` warns when its lint
does not fire, so check each with a clippy run.

#### C23: One helper for each repeated scripting pattern

- `with_current_canopy(..)?; Ok(ret_none())` is written 18 times in
  `core/script/base_api.rs`. Add a unit twin of `host_value`
  (`core/script/dispatch.rs:56-62`).
- Node handles are validated through the live canopy five times
  (`base_api.rs:951,967,1167,1486,2182`), and `read_node_id` and
  `read_opt_node_id` (`:941-972`) are near copies. One helper.
- `host_find_identity` builds a read-only context by hand (`:1342-1343`)
  where `with_root_view` serves.

### Widgets

#### C24: `MenuList` scrolls

`MenuList::canvas` returns the view (`crates/canopy-widgets/src/menu.rs:561-563`),
and `render` paints from the first item whatever the scroll offset
(`menu.rs:616`). `Dialog::add` says a large body scrolls
(`dialog.rs:76-77`), but a canvas that never exceeds the view cannot move, so
the rows past the view are cut and the selection moves onto them.
`PickerList::canvas` (`picker.rs:1051-1055`) does it right. Fix: canvas
`Size::new(view.w, rows.max(view.h))`, and paint through `label_rows` from
`view.scroll.y`. `ModeHelp` mounts a `MenuList` without selection
(`help/mode.rs:43`) that accepts no intents (`menu.rs:588-590`), and it is
the likelier overflow, so that list must scroll too. A test opens a menu
taller than its dialog and selects the last row.

#### C25: `DiffView` drives its own spinner on the driver clock

`set_loading` stores `Instant::now()`, `render` reads `elapsed()`, and `poll`
returns the spinner period while loading (`diff_view.rs:334-336`,
`:1171-1174`, `:1194-1197`). Nothing starts the poll, so the spinner turns
only while a host polls: fh ticks for it (`fh/src/changes.rs:67,999-1000`).
The wall clock defeats `ManualClock`, and the poll ignores `motion_active`.
`SearchProgress` is the model (`search_progress.rs:108-117,169-179`): it
keeps a wake handle from `on_mount`, wakes in `set`, dates the run in `poll`
from `ctx.now()`, and polls only while motion is active. Make `DiffView` do
the same. `set_loading` keeps its signature. fh's test at `changes.rs:1197`
asserts a poll under a harness without motion, and changes with it.

#### C26: `ClickTracker` takes the time from its caller

`ClickTracker::count` reads `Instant::now()` (`click.rs:40`), so the expiry
of the multi-click threshold cannot be tested without a real wait. Pass `now`
from `ctx.now()` in `Editor` (`editor/widget.rs:1037`) and `Terminal`
(`terminal.rs:476`, with the time carried from `on_event`). Add tests for a
double click, a triple click, a click after the threshold, and a click at
another location.

#### C27: One width measure, one wrapper, one segmenter

- `terminal.rs:645` centers the exit message by `chars().count()`. Use
  `text::width`, as `docs/architecture.md:91-93` requires.
- Six sites call `textwrap::wrap` directly: `help/binding_list.rs:454,501,
  620,630,640` and `diff_view.rs:1138`. Use `canopy::text::wrap_with` with
  `break_hyphens: true`, which keeps today's output, and drop the `textwrap`
  dependency (`Cargo.toml:25`). The toast wrap (`toast.rs:176-258`) differs
  in behaviour and stays.
- Four files import `unicode_segmentation`: `table/flow.rs:21`,
  `help/binding_list.rs:20`, `fuzzy.rs:24`, and `tests/vim_diff.rs:25`. Use
  `canopy::text::graphemes` and `grapheme_indices`, and drop the dependency
  (`Cargo.toml:26`). `flow.rs:294` takes the last cluster with `next_back`
  and needs `last`.

#### C28: `#[expect]` in canopy-widgets

`input.rs:473`, `editor/widget.rs:1266`, `chart.rs:498`, `help/mod.rs:132`,
and `fuzzy.rs:159` use `#[allow]` where the rest of the crate uses
`#[expect]` with a reason. Switch them, and check each with a clippy run.

#### C29: Each widget has one path

- `Editor` and `Terminal` are at `canopy_widgets::editor::Editor` and
  `canopy_widgets::terminal::Terminal` only, against the rule at
  `docs/architecture.md:38-40` that widgets are at the root. Decision: the
  root is the one path of a widget, and a module holds its supporting types.
  Export both at the root, and remove the module paths.
- `Menu`, `MenuItem`, `MenuList`, and `MenuRow` resolve at the root and at
  `canopy_widgets::menu::` (`lib.rs:64,149`). `pub mod menu` keeps
  `RowLayout`, `render_row`, and the style rules only.
- `ImageView::zoom` and `call_zoom` take `ZoomDirection`
  (`image_view/mod.rs:29,709`), which no path names. Export it beside
  `ImageView`.
- `TableCopy` (`table/mod.rs:1533`) is in no public signature, and only
  `table_view` uses it. Make it `pub(crate)`.

#### C30: A host registers a widget one way

- `Tabs`, `Columns`, `Dropdown<T>`, `Selector<T>`, and `List<W, K>` carry
  commands and implement no `Register` outside tests, so hosts write
  `add_commands` by hand (`examples/gyms/src/style.rs:1103-1105`,
  `list.rs:246-247`, `term.rs:447`, `intervals.rs:248`,
  `examples/todo/src/lib.rs:480`). Add the impls. A generic impl works, as
  `impl<T> Register for PickerList<T>` shows (`picker.rs:966`).
- `Editor::register` adds the commands and registers the clear intent
  (`editor/widget.rs:1751-1755`). gyms calls `add_commands::<Editor>()` in
  its place at `src/editor.rs:282`, `widget_editor.rs:75`, and
  `style.rs:1109`, which compiles and skips the intent. Call
  `Editor::register`. fh does the same for `ImageView`
  (`canopy-fileselect/src/lib.rs:1509`).
- `Terminal` (`terminal.rs:270`) and `Menu` (`menu.rs:281`) carry
  `#[derive_commands]` with no `#[command]`. Remove the attribute, and the
  `add_commands::<Menu>()` call at `menu.rs:490`.

The rule for a caller is then one sentence: call `register` on each widget
type that the application mounts.

#### C31: Names and shapes agree across sibling widgets

- `Editor::with_config(text, config)` is the only constructor
  (`editor/widget.rs:306`), and `with_*` is a builder everywhere else. Rename
  it `Editor::new`.
- `Picker::filter()` returns the node of the filter field (`picker.rs:235`),
  and the command `PickerList::filter()` returns the filter text
  (`picker.rs:615`). Rename the first `Picker::field()`, and return
  `TypedId<Input>`, as `SearchBar::field()` does.
- `Picker::set_items` and `update_items` take `title: &str`, and the same
  methods of `PickerList` take `label: String` for the same value
  (`picker.rs:131,146,623`). All four take `title: impl Into<String>`.
- `MenuList::set_items` takes `selected: Option<usize>` and unwraps it to 0
  (`menu.rs:402-405`). Take `usize`, as `Menu::set_items` does.

#### C32: Remove `ImageView::set_preview_path`

`ImageView::set_preview_path` (`image_view/mod.rs:700`) has no caller in any
application. fh decodes on a worker with `DecodedImage::read` and calls
`set_preview` (`canopy-fileselect/src/view_loader.rs:275-284,635`). Remove
it, and rewrite the six tests of the crate that use it.

#### C33: `Dialog::set_title`

`Dialog::frame(ctx, id)` followed by `with_widget_mut(frame, set_title)` is
written at `help/mode.rs:86-90` and six times in verber
(`verber/src/grants.rs:579-583,709-713`, `keys.rs:441-445`,
`models/form.rs:155-159`, `models/card.rs:420-424`,
`verber-conversation/src/question_dialog.rs:172-176`). Add
`Dialog::set_title(ctx, dialog, title)`.

#### C34: `PickerList::page` counts rows the view shows

`PickerList::page` moves the cursor by `view_rect().h` items
(`picker.rs:557-561`). The rows include a heading for each group
(`picker.rs:729`), so with groups a page passes more items than the view
shows. fh and verber both have grouped pickers. Count the items among the
rows of one view height, with a test.

#### C35: Two small additions the applications use

- `StatusLine::with_margin` keeps columns clear at the end only
  (`status_line.rs:68-74`). verber pads the start by finding the line with
  `unique_descendant` and a layout override (`verber/src/app.rs:1086-1095`).
  Add a start margin.
- The loop that paints mosaic cells with a pen that applies the effect stack
  is written three times: `image_view/mod.rs:546-568`,
  `crates/canopy-markdown/src/presentation/mod.rs:2052-2085`, and
  `markkit-canopy/src/lib.rs:210-244`. Add `mosaic::put_cell(render, bounds,
  at, fg, bg, ch)` outside the `graphics` feature, since markkit builds the
  crate without default features, and call it from all three.

#### C36: Text positions without an editor

`Editor::position_of_offset` and `offset_of_position`
(`editor/widget.rs:1313,1319`) need an `Editor`. fh-audio has its own pair on
a `&str` (`fh-audio/src/view.rs:98,120`), epea has a byte-identical copy
(`epea/src/view.rs:49,71`), and the two have 32 callers. Add
`editor::position_of_offset(text, offset)` and `editor::offset_of_position(text,
position)` as free functions that share the buffer's logic
(`text_buffer/buffer.rs:116-129`). The methods keep their rope lookups. The
copies count the `\r` of a CRLF as a column, which the buffer does not: the
functions follow the buffer.

#### C37: `paged::PageError` is an error

`PageError` (`paged/mod.rs:107-113`) is the error of `Paged::apply`, and it
implements neither `Display` nor `Error`, so `?` does not carry it into
`canopy::error::Error`. Implement both. `Paged::new` asserts on its sizes
(`mod.rs:279-282`) with no `# Panics` section. Add it.

#### C38: Style rule functions share one name

Three functions have the signature that `Setup::widget_styles` takes:
`table::style_rules` (`table/mod.rs:1597`), `menu::styles` (`menu.rs:215`),
and `highlight::palette_style_rules` (`highlight.rs:71`). Rename the last two
`style_rules`.

#### C39: Reads take `&dyn ViewContext`

`Picker::selected` and `SearchBar::query` call only `with_widget`, and both
demand `&dyn Context` (`picker.rs:182`, `search_bar.rs:142`), so a widget
cannot call them from `render` or `status`. Sibling reads take `&dyn
ViewContext`: `Confirm::is_open`, `Pane::has_keys`, and `StatusLine::items`.
Take `&dyn ViewContext` in both, and in `Options::is_open`
(`crates/canopy-options/src/widget.rs:339`) and `Markdown::state` (C47). A
caller that holds a `Context` still compiles, as
`canopy-markdown/src/widget/slot.rs:827` shows.

#### C40: `canopy_widgets` re-exports `image`

`ImageView::new`, `ImageView::set_image`, `image_cells`, and `read_preview`
take or return `image::RgbaImage`, and `canopy_markdown::host::Resource::Image`
holds one. A caller must add an `image` dependency of its own at a matching
version: `canopy-markdown`, gyms, and fh each pin `0.25.10`. `syntect` is in
the same position, and the crate re-exports it whole (`highlight.rs:166`).
Add `pub use image` under the `graphics` feature. Replace the glob `pub use
canopy_mosaic::*` (`mosaic.rs:9`) with the names that callers use: `Cell`,
`Family`, and `Tier`.

#### C41: `screenshot` moves to `canopyctl`

`canopy_widgets::screenshot` draws a frame to a PNG. It uses no item of the
crate, it embeds four font files (`screenshot/mod.rs:27-30`), and its one
consumer is `canopyctl` (`src/gallery.rs:23`), which depends on
`canopy-widgets` for it and turns on `graphics` to get it. Every `graphics`
user compiles the module and its fonts. Move the module and its assets to
`canopyctl`.

#### C42: `diff::Mode` is `DiffLayout`

`diff::Mode { Unified, SideBySide }` is set through `DiffView::with_strategy`,
`set_strategy`, and `strategy` (`diff_view.rs:569-588`), and its docs call it
the display layout. Rename the type `DiffLayout` and the methods
`with_layout`, `set_layout`, and `layout`.

#### C43: Focus opt-in is `with_focusable(bool)`

`Container::focusable()` and `Pane::focusable()` take no argument,
`Text::with_focusable(bool)` and `ImageView::with_focus(bool)` take one
(`api/canopy-widgets.rs:1095,2408,3896,1667`). Use `with_focusable(bool)` on
all four. `Container::set_focusable` stays.

#### C44: A call that reports an event is `with_on_<event>`

`Input`, `SearchBar`, `TabBar`, `Tabs`, and `TerminalConfig` take the call
that they post to their owner with `with_on_change`, `with_on_submit`,
`with_on_cancel`, `with_on_select`, and `with_on_exit`. `List::with_command`
appends the row index to its call, which is the shape of
`Tabs::with_on_change` under the name of `Button::with_command`, which
appends nothing. Rename `List::with_command` to `with_on_activate`,
`ColumnChart::with_command` to `with_on_change`, `Picker::set_commands(accept,
cancel)` to `set_on_accept` and `set_on_cancel`, and `MenuList::set_close` to
`set_on_close`. `Button::with_command` stays, because the button runs its
command.

#### C45: `ConfirmRequest` builders take the `with_` prefix

`ConfirmRequest` has nine public fields and seven builder methods without a
prefix, four of which share a name with a field: `arm_after`, `closing`,
`message_width`, and `tone` (`api/canopy-widgets.rs:1001-1056`). `MenuRow`
and `EditorConfig` have public fields and `with_*` methods. Give the seven
methods the prefix, so `dim` becomes `with_dim_target`.

### Markdown

#### C46: The inspector reports application failures as notices

`Markdown` maps its failures through `invalid()` to `Error::App`
(`crates/canopy-markdown/src/widget/mod.rs:256-257`), which is a notice. The
inspector builds `Error::Invalid` at nine sites (`inspector.rs:828,830,979,
1026,1182,1187,1204,1252,1304`), which is fatal outside a command. Three
reach a poll: `:979` through `Markdown::receive`, and `:1026` and `:1304`
inside `Inspector::poll`. Use the one helper at all nine, and assert
`is_notice()` in `table_republication_rejects_a_different_dataset`
(`inspector.rs:2430`).

#### C47: Sibling commands of `Markdown` agree

- `set_mode` (`widget/mod.rs:782-796`) does not check the active view, so
  `i` in the Source view puts the hidden rendered surface in Interact, and
  the state then reports `interact`. Decision: in the Source view,
  `interact_status` (`:2209-2215`) reports the command disabled, and
  `set_mode` changes nothing, as `select_block_at` does (`:830`). fh calls
  `reader.interact` with no view check (`canopy-fileselect/src/view_pane.rs:383`)
  and needs no change.
- The condition at `:783` relies on operator precedence. Write it
  `self.slot.is_open() || (mode == Mode::Interact && self.inspector.is_some())`,
  which is what two tests assert (`slot_tests.rs:1041-1079`).
- `select_text` (`:1093-1099`) returns an error in the Source view and has
  no `enabled` gate, while `activate` and `next_object` carry
  `rendered_action_status`. `v` is bound on the source path
  (`bindings.luau:18`), so the error is a notice that a user meets. Give it
  the gate.
- `state(&self, ctx: &mut dyn Context)` (`:2344`) is a read that takes a
  mutable context. `queries::state` takes `&dyn ViewContext`
  (`queries.rs:101`). Take `&dyn ViewContext`.

#### C48: One public path for each Markdown item

`pub mod options` and the root both export `Follow`, `MarkdownOptions`,
`Mode`, and `ViewType` (`lib.rs:17,38`). `DocumentInput`, `FinishOutcome`,
and `MarkdownSource` are at the root and in their modules (`lib.rs:39-40`).
Every application imports the first six from the root, and only the bench
uses a module path. Decision: the root is the path of what a reader host
uses.

- `options` becomes private.
- `DocumentInput` and `FinishOutcome` are arguments of root `Markdown`
  methods. They move out of the public `source_store` and stay at the root.
- Nobody imports `MarkdownSource` from the root. Drop that re-export, so its
  path is `source_view::MarkdownSource`.

#### C49: Two repeated derivations in the reader

- The reading offset of a grapheme in a run is written in `hit`
  (`presentation/mod.rs:914-918`), `paint` (`:1071-1077`), and
  `surface::display_position` (`surface.rs:1622-1626`). Add
  `PreparedRun::reading_range(byte, len)`, which serves the three.
- `status_items` (`widget/mod.rs:2878-2942`) and `update_chrome`
  (`:2944-3035`) each derive the partial reason, the new byte count, and the
  action or link. Three helper methods feed both.

#### C50: The reader's tests share their helpers

`service`, `settle`, and `reader` are each written two or three times
(`widget/tests.rs:65,86`, `cursor_tests.rs:70,81`, `slot_tests.rs:69`,
`edit_props.rs:58,71`), and `.script("markdown-defaults",
"markdown.default_bindings()")` fourteen times in six files. Put them in one
`#[cfg(test)]` support module under `widget`. It cannot be `testing.rs`,
which is public under the `testing` feature and cannot reach the private
fields that the helpers use.

#### C51: `presentation::style_rules` is private

`canopy_markdown::register_styles` installs three rule sets (`lib.rs:46-50`).
`presentation::style_rules` is the first, it is public, and its doc tells a
host to pass it to `Setup::widget_styles`. A host that does so gets the
Markdown roles without the syntax and table styles. No consumer calls it.
verber, which paints documents itself, calls `register_styles`. Make it
`pub(crate)`.

#### C52: `url` leaves the public surface

`url::Url` is in three public signatures: `document::Link::resolve`,
`document::resolve_link`, and `ResolvedLink::External`
(`api/canopy-markdown.rs:1377,1512,1565`). No consumer, bench, or fuzz target
names any of the three. What a host sees is strings:
`MarkdownOptions::base_url: Option<String>` and `HostAction::OpenUrl(String)`.
Make the three items `pub(crate)`.

#### C53: The two `SourceError` types have two names

`source_store::SourceError` (`source_store.rs:53`) is the error of an
append, and `source_view::SourceError` (`source_view.rs:69`) is the error of
the source view. The capture has to spell the second by its full path.
Decision: rename the first `StoreError`.

#### C54: `MarkdownOptions::default()` starts no threads

`MarkdownOptions::default()` calls `MarkdownExecutor::default()`
(`options.rs:131-134`), which spawns the worker threads, two by default, and
panics when a spawn fails (`executor.rs:624-643`). The field's doc tells a
host to clone one executor across readers, and a host sets it with struct
update syntax, which builds the default first:

- fh `canopy-fileselect/src/host.rs:1432-1448` writes `..Default::default()`
  and then replaces `options.executor`. Each call to `reader_options` starts
  and drops a pool.
- epea `src/app.rs:1392-1401` sets `executor` and ends with
  `..MarkdownOptions::default()`, with the same effect.

Decision: `MarkdownExecutor::default()` builds the service without workers
(`without_workers`, used by `try_new`), and `register` (`executor.rs:708`)
starts them when none run. A default that nobody mounts then costs nothing,
and no signature changes. `new` and `try_new` start their workers at once, as
they do now. A test asserts that a default executor runs no worker before its
first registration.

#### C55: `SourceStore` constructors say what they make

`static_text`, `static_with_limit`, `stream`, and `with_limit` build a store
(`api/canopy-markdown.rs:2478-2492`). `with_limit` builds a receiving store,
which its name does not say. `SourceStore::replace` takes `receiving: bool`,
while `Markdown::replace` takes `DocumentInput`, which has `Complete` and
`Receiving`. Rename the four `complete`, `complete_with_limit`, `receiving`,
and `receiving_with_limit`, and make `replace` take a `DocumentInput`.

### Options

#### C56: Small fixes in canopy-options

- `SetError::Kind` is declared and displayed
  (`crates/canopy-options/src/model.rs:475-480,489`) and constructed nowhere.
  A kind mismatch is `SetError::Invalid` (`model.rs:699`). Remove the
  variant.
- The panel pages by a fixed 10 rows (`panel.rs:1328`) while `render` derives
  the visible row count (`panel.rs:1548`), so a panel under 10 rows skips
  rows that it never showed. Page by the visible rows.
- `store_error` (`widget.rs:404-407`) carries a lint attribute that it does
  not need as `fn store_error(error: impl Display) -> Error`, which is how
  canopy-markdown writes `invalid`.

#### C57: The options derive rejects what it ignores

- `min`, `max`, and `step` on a leaf that is not an integer pass the derive
  (`crates/canopy-options-derive/src/lib.rs:360-379`) and are ignored
  (`canopy-options/src/model.rs:610-626`). `inline` with `key` on a group
  drops the key (`model.rs:318-330`) and can report a false clash
  (`lib.rs:327-329`). `inline` on a leaf is ignored (`model.rs:637-645`). A
  variant attribute other than `key` or `label` is ignored
  (`lib.rs:558-574`). Each becomes a compile error with a trybuild case in
  `crates/canopy-options/tests/ui`. The integer check needs a marker trait
  that the derive asserts, as `assert_leaf` is asserted (`lib.rs:414-416`).
- "min is above max" and "step must be positive" use `Span::call_site()`
  (`lib.rs:150,153`), so the error lands on the derive. The default
  expression of a field is emitted without a span (`lib.rs:306-308`). Span
  both on the attribute and the field type.
- `expand_group` carries `#[allow(too_many_lines)]` (`lib.rs:268-271`) for a
  lint that the workspace does not enable. Delete the attribute.

#### C58: `canopy_options::Layout` is `OptionFiles`

`canopy_options::Layout` is the set of option files of a run. In a TUI
application `Layout` is `canopy::layout::Layout`, and a file that needs both
must rename one. `Layout::discover(app_dir, config_home, start, home,
project_root)` takes four paths in a row, three of them `Option<&Path>`
(`api/canopy-options.rs:398-404`), so two swapped arguments compile. Rename
the type `OptionFiles`, and make `discover` take a struct with those five
named fields.

### Automation and tooling

#### C59: Each example builds its factory once

hello builds `AppFactory::new(AppMetadata { .. }, ..)` in `src/main.rs:72-78`
and again in `tests/smoke.rs:15-23`. todo has `make_factory`
(`src/main.rs:55-71`) and builds the factory again in `tests/smoke.rs:17-26`.
gyms builds it three times (`src/main.rs:249-255`,
`src/tests/smoke.rs:13-19,31-37`). fh, verber, and epea each expose `pub fn
factory` from their library. Move the factory of hello and todo into the
library, and give gyms one function in `main.rs`, which is its only crate.
`docs/getting-started.md:280-285,379-386` prints both copies and changes with
it.

#### C60: `canopyctl` and `canopy-term` fixes

- `canopyctl run` without `--pty` binds a ready socket
  (`crates/canopyctl/src/main.rs:471`) and reads it only with a fixture
  (`:483-486`). `ReadySignal` has no `Drop`, and the command ends with
  `exit` while the signal is alive, so the socket file stays. Bind the
  signal only when `--fixture` is given, and implement `Drop` for
  `ReadySignal`, which also removes the file when the child exits first
  (`crates/canopy-term/src/lib.rs:350-352`).
- `ResolvedCommand::to_command` strips the launching package's Cargo
  variables only when the program's file stem is `cargo`, `ncode`, `edev`,
  or `canopyctl` (`crates/canopyctl/src/config.rs:309-316`), while the
  terminal path strips them for every program
  (`canopy-term/src/lib.rs:404-416`). Strip for every program.
- The Cargo variable filter is written three times: `xtask/src/cargo_env.rs`,
  `crates/canopyctl/src/cargo_env.rs`, and inline in `canopy-term`. xtask
  must keep its copy, because it is a leaf. Export the predicate from
  `canopy-term`, and remove canopyctl's copy.
- The live-session defaults `120` and `40` (`canopyctl/src/main.rs:356-358`)
  repeat `ScreenSize::default()` (`canopy-mcp/src/metadata.rs:53-60`). Use
  the default.

#### C61: `canopy-mcp` fixes

- `canopy_mcp::Error::Command` (`error.rs:17`) is constructed only through
  `?` in two tests (`script.rs:1177`, `server/mod.rs:634`). Convert at those
  two lines, and remove the variant and its `From` impl.
- `examples/todo/.canopyctl.toml` has no `[smoke]` section, so its suite runs
  with no timeout and without `fail_fast`, unlike hello and gyms. Add the
  section that hello has.
- `examples/todo/smoke/with_items/delete_item.luau:1-14` rebuilds the screen
  text from `snapshot().cells`. Call `canopy.screen_text()`.

#### C62: `canopy-derive` fixes

- `derive_commands` ignores its attribute arguments (`lib.rs:34-35`), so
  `#[derive_commands(anything)]` compiles. Reject a non-empty argument
  stream with a spanned error.
- The generated code names the runtime crate as `canopy::` about a hundred
  times (`codegen.rs` 65, `lib.rs` 34, `parse.rs` 2), where `::std::` is
  qualified. The runtime also declares `pub mod canopy` in its `core`
  (`core/mod.rs:42`), which an unqualified path can reach. Emit `::canopy::`.
- A method with two `#[command]` attributes keeps the last
  (`parse.rs:172-210`). Report an error.

#### C63: xtask runs a child command one way

`run_command` (`xtask/src/main.rs:384-400`) and `run_cargo_command`
(`:424-445`) print the same two messages. The second calls the first. The
matches in `soak.rs:92-105` and `luau_grammar.rs:87-110` print their own
messages and stay.

#### C64: CI checks what the repository has

- The format step names 10 packages (`.github/workflows/ci.yml:42-45`), and
  the workspace has 17. Format the workspace.
- `cargo xtask big-text --check` and `cargo xtask xmark-specs --check` take
  no input and run nowhere. Add both to CI.

#### C65: Manifests

- `image = "0.25.10"` is declared in five manifests, `fontdue = "0.9.4"` in
  three, and `syn`, `quote`, and `proc-macro2` in both derive crates. Move
  them to `[workspace.dependencies]`.
- Seven publishable manifests have no `description`. Add one to each.
- `crates/canopy-term/Cargo.toml` has no `[lib]` section, so it lacks the
  `doctest = false` that the other ten library crates set.

### Tests

#### C66: Harness helpers for the waits that suites write

verber defines `wait_for_text`, `wait_for_no_text`, and `wait_for_notice`
(`verber/tests/app.rs:197,227,252`) and calls them 402 times. It has a
second `wait_for_text` in `verber-conversation/src/widget.rs:1488`, and fh
has one in `fh/src/audio_tests/by_hand.rs:55`. Add the three to `Harness`
(`core/testing/harness.rs`), over `wait_until` (`:299`), `contains_text`
(`testing/buf.rs:88`), and `Canopy::notices`. Decision: they panic with the
screen in the message under `#[track_caller]`, as verber's do, so a failure
names the test line.

#### C67: Three tests sit with the code they test

- The mouse spec test (`core/canopy/leave_tests.rs:200-210`) tests
  `event/mouse.rs` alone. Move it to the `mod tests` there (`mouse.rs:196`).
- The pure test of `Layout::validate`
  (`core/world/layout_driver/tests.rs:1400-1404`) moves to
  `crates/canopy/src/layout.rs`.
- The three reveal tests and their helper (`layout_driver/tests.rs:695-795`)
  move to `core/world/scroll/tests.rs`, where reveal is implemented.

#### C68: Tests for promises the architecture makes

`docs/architecture.md` states each of these, and no test pins it:

- A failed edit unmounts the widgets it mounted in reverse order (`:168-169`).
  The one fault test has a single completed mount (`core/world/tests.rs:1379`).
- A mount that failed succeeds when it is tried again. The existing test
  counts the attempts only (`crates/canopy/tests/it/tree.rs:42`).
- A scrollbar thumb keeps at least one cell. The clamp is `.max(1)` at
  `crates/canopy-geom/src/linesegment.rs:67`, and no test rounds to zero
  first.
- `FocusGained` reaches a widget through a turn. Paste and `FocusLost` have
  tests, and `FocusGained` has none.
- `validate_semantic_keys` (`core/world/semantic.rs:92`) rejects a bad key.
  Its two tests are positive.

#### C69: Missing widget tests

`border.rs` and `run_paint.rs` have no test. `click.rs` has none, and can
have them after C26. `Dropdown::handle_click` (`dropdown.rs:147`) has none.
Add them.

#### C70: `examples/gyms/src/tests/names.rs` keeps one test

Three of its four tests scan sources and a rename table to guard the rename
of `3bce2a15`, which is complete. Delete them. The fourth,
`renamed_styles_resolve_under_their_new_names` (`names.rs:142-168`), asserts
that four gym styles resolve to styles of their own. Keep it, under a name
that says so.

#### C71: Property tests for the spec parsers

`Key::parse_spec` (`core/event/key.rs:250-257`) and `Mouse::parse_spec`
(`core/event/mouse.rs:107-148`) take the text of a user's keymap. Add a
proptest for each, in the crate, that an arbitrary string does not panic and
that `Display` parses back to the same value. The crate already uses
proptest (`core/inputmap/tests.rs:1219`).

## Decisions

Each needs a choice. Each has the answer this review recommends.

### D1: `TableReader` and `TableView` share names and not types

Both show a read-only table. `TableReader::cursor` returns
`Option<(usize, usize)>` and `TableView::cursor` returns `Option<CellKey>`.
`set_cursor` takes a context in one and not in the other, `move_cursor` has
an `extend` flag in one, and matches arrive through `set_matches` in one and
`set_overlay` in the other (`api/canopy-widgets.rs:3389-3760`). fh uses only
`TableReader`, and the workspace only `TableView`. The names do not say
which to choose. Recommended: keep both, state the difference in the first
line of each doc, and align `cursor` and `set_cursor` on the index pair, with
the keyed forms named `cursor_key` and `set_cursor_key`.

### D2: Search differs between `Editor`, `DiffView`, and `TableReader`

Each has `set_matches`, `search_next`, `clear_search`, `search_matches`, and
`search_position`. `Editor` has three ways to install matches: `set_matches`,
`restore_matches`, and `set_matches_preserving_position`
(`api/canopy-widgets.rs:4524,4598,4601`), which differ in the anchor and in
whether they reveal. `DiffView` lacks `restore_matches` and `current_match`,
and its search methods are not commands, so `DiffView::clear_search` takes a
context that it does not use (`diff_view.rs:460`). fh branches on the widget
type to search (`canopy-fileselect/src/search.rs:224,236`). Recommended:
give `DiffView` the two methods and the commands, and fold the two
installers of `Editor` that do not reveal into one that takes the anchor. A
shared trait can follow once the three agree.

`Editor::move_cursor` takes `canopy::tree::FocusDirection`
(`api/canopy-widgets.rs:4463`), the enum of focus traversal, where `Next`
and `Prev` mean nothing for a caret. Recommended:
`canopy::layout::ScrollDirection`, which has the four directions and is a
command type.

### D3: Styled runs are unnamed tuples

`ItemLabel::runs` returns `Vec<(&str, &str)>`, `BigText::set_runs` and
`with_runs` take `(S, T)` pairs, and `canopy::render::Render::runs` takes the
same pair. The order is style, then text. The docs of `BigText` do not say
so, and swapped arguments compile. `canopy::status::StatusSpan { style, text
}` already names the pair. Recommended: one `Span` type in `canopy::text`
that the four take, which `StatusSpan` becomes.

### D4: "Selected" getters disagree

`selected()` returns an item on `Dropdown`, `PickerList`, and `Paged`, and an
index on `MenuList` (`api/canopy-widgets.rs:1437,2686,5437,2263`).
`Dropdown::selected_index` and `Selector::selected_index` return `usize`,
with 0 for an empty selector, which a test asserts (`selector.rs:415`), and
`List::selected_index` and `Paged::selected_index` return `Option<usize>`.
`Dropdown::new` rejects an empty list, and `Selector::new` accepts one.
Recommended: `selected()` is the item and `selected_index()` is
`Option<usize>` on every one, so `MenuList::selected` becomes
`selected_index` and `Selector::selected_index` returns an `Option`.

### D5: `paged` has generic names and nine bare `bool` results

`canopy_widgets::paged` uses only `std`, and its one consumer, fh, renames
on import (`Page as ListPage`, `canopy-fileselect/src/flat.rs:33-34`). Its
types are `Page`, `Direction`, `Status`, `Request`, `Token`, `Seek`, and
`Capabilities`, and nine methods of `Paged` return a `bool` whose meaning
differs between them (`api/canopy-widgets.rs:5366-5434`). Recommended:
prefix the names that other modules also use (`PageDirection`, `PageStatus`,
`PageRequest`), and say in each doc what its `bool` reports.

### D6: The worker protocol and the search of canopy-markdown

`WorkHandle`, `WorkRequest`, `PreparedView`, the other lane types of
`executor`, and the whole `search` module are public. Their only user
outside the crate is the bench (`benches/qualification.rs:27,469-500,638`).
They draw `PresentationSnapshot`, `PreparedSource`, `SourceOptions`, and the
search types into public signatures, including
`MarkdownSource::find_matches` and `MarkdownExecutor::register`. What
applications use is the `MarkdownExecutor` type, `MarkdownExecutor::manual`
(27 calls in the tests of fh and epea), `ExecutorOptions::default()`, and the
`ManualExecutor` drivers.

`Markdown::set_search(ctx, query, regex: bool)` maps `true` to a smart-case
regex and `false` to a case-sensitive literal (`widget/mod.rs:1369-1385`).
`SearchQuery`, `SearchMode`, and `SearchCase` are public, and no reader
method accepts them, so a host cannot ask for a literal search without case.

Recommended: export `SearchQuery`, `SearchMode`, and `SearchCase` at the
root, and make `set_search` take a `SearchQuery` in Rust, with the flat form
for the command. Put the lane types, `register`, and the rest of `search`
behind the `testing` feature, and give the bench `required-features =
["testing"]`, which its manifest and its README lack today.

### D7: The captures omit the feature-gated surfaces

`api/index.md` says `Capture features: default`. The `testing` module of
`canopy` is the surface that sixteen manifests across the workspace and the
applications enable, and API review cannot see it. The same holds for
`canopy_markdown::testing` and for the three modules behind the `html`
feature of `canopy-xmark`, one of which, `github`, is the only HTML writer
that fh calls. `workspace.metadata.nanocode` sets `features = "all"`.
Recommended: capture with all features, if `ncode api` can. This review did
not find how its feature set is chosen.

### D8: xmark comments need a pass that the parser does not make

`Options::ENABLE_COMMENTS` only drops what is left of a comment. The text
must first go through `hide_comments`, and `Options::obsidian()` turns the
flag on. Each caller makes the pass by hand:
`canopy-markdown/src/document.rs:1954`, `fh-markdown/src/render.rs:36`, and
`canopy-xmark/src/sections.rs:58`. The obvious call,
`Parser::new_ext(text, Options::obsidian())`, shows `%%` comments.
`canopy_xmark::fenced_code` (`src/code.rs:26`) makes that call without the
pass. Whether it then reports a fenced block inside a comment was not
tested. Recommended: one function that makes the pass and returns the masked
text with its parser, and a line in the doc of `Parser::new_ext`.

### D9: An edit of the Markdown source has three shapes

`BlockEdit { epoch, range, text }`, `units::Splice { range, text }`, and
`Markdown::apply_edit(ctx, epoch, from, to, text)`
(`api/canopy-markdown.rs:47-54,140-147,2671-2676`) are one edit. fh-audio
unpacks a range into `from` and `to` (`src/app.rs:919,1795`), and gyms
applies a `BlockEdit` by hand with `replace_range`, which `Splice::apply`
does. Recommended: `BlockEdit { epoch, splice: Splice }`, and a Rust method
`apply_splice(ctx, epoch, Splice)` beside the flat command.

### D10: `canopy::text` measures columns in two integer types

`text::width`, `wrap`, and `wrap_with` use `u32`, as `geom` does.
`slice_by_columns`, `truncate_end`, `truncate_start`, `split_end`,
`split_start`, `cell_width`, `tab_width`, `grapheme_width`, `expand_tabs`,
and `Truncate::apply` use `usize` (`core/text.rs:77-305`). Widgets and
applications cast between the two at about forty call sites, and verber
defines three helpers only to convert (`verber-conversation/src/question.rs:1111-1123`).
The text buffer and the editor layout index columns by `usize`
(`text_buffer/buffer.rs:558,570`, `editor/layout.rs:449,698`), so a change
to `u32` moves some casts there. Recommended: `u32` for every column count,
since a column is a cell and cells are `u32` in `geom`. Byte offsets stay
`usize`.

### D11: `Layout::fill().into()` overrides every field

`Layout::fill().into()` and
`LayoutOverride::new().flex_horizontal(1).flex_vertical(1)` read as one
override, and they differ. `From<Layout> for LayoutOverride` overrides every field
(`crates/canopy/src/layout.rs:433-453`): it sets the direction to a column,
clears padding, gap, alignment, and bounds, and so replaces the child's own
layout. The second sets width and height only. Applications write the first
63 times and the second 21 times, mostly to say "fill the parent".
Recommended: add `LayoutOverride::fill()` for width and height only, use it
at the 21 builder sites, and check each `.into()` site for whether it means
to replace the child's layout. Remove the `From` impl if none does.

### D12: Binding queries sit on `Context`

`available_bindings`, `menu_bindings`, `transient_mode`, `current_event`,
`current_mouse_event`, and `wake_handle` take `&self` and sit on `Context`
(`core/context.rs:599-832`), so a widget cannot call them from `render`. No
consumer works around this: each caller holds a `Context`, and
`current_event` is always `None` in `render`. Recommended: move the three
binding queries to `ViewContext`, where a status line that shows the waiting
menu would read them, and leave the rest. Low value.

### D13: Hidden nodes lose their scroll offset

`clear_layout` sets a hidden node's scroll to zero
(`core/world/layout_driver/mod.rs:771`). A node that shows again starts at
the top, and fh restores the offset by hand in two places that say so
(`canopy-fileselect/src/side.rs:91-103`, `view_pane.rs:510-516`). The
architecture calls the view a cache, and the scroll offset is state.
Recommended: keep the offset. The change has three parts beside the one
line. `validate_cached_state` (`core/world/tree.rs:515-548`) must
skip the scroll bound for a node with a hidden ancestor, not only a hidden
node. A scroll operation on a hidden node must not clamp against its empty
canvas (`layout_driver/mod.rs:892-905`). Two tests assert today's behaviour
and change (`layout_driver/tests.rs:1151-1183`,
`canopy-widgets/src/scrolling_tests.rs:623`).

### D14: Focus after a modal ends

A widget that wants the focus after a modal closes cannot say so while the
modal is open, because `set_focus` outside the modal fails
(`core/world/focus.rs:61-63`). The applications work around it in four
places. One owns the modal: fh `commander/pickers.rs:310-313,344-351` closes
it and posts a call that moves the focus, which is deterministic and already
works. The other three wait on a modal that the widget does not own: fh
`canopy-fileselect/src/view.rs:411-438`, and a 100 ms poll on `modal_admits`
in fh-audio (`src/app.rs:1200-1218`) and its copy in epea
(`src/app.rs:645-664`). A call that closes a modal and names the next focus
would serve only the first. Recommended: add a wake for the moment a node is
admitted again, as `wake_on_modal_close` wakes an owner, which removes the
two timed polls.

### D15: The `Canopy` facade

`glyphs`, `set_glyphs`, `set_theme`, `enter_mode`, `leave_mode`, `notify`,
`notice_log`, `available_bindings`, `menu_bindings`, and `set_motion` exist
on `Canopy` and on a context (`core/canopy/mod.rs:372-704`). Tests and tools
use the facade heavily: `notices()` 46 times, `available_bindings` 38, and
`enter_mode` 37. Recommended: keep it, and say so in the architecture (C2).
`Setup::set_theme`, `Setup::set_glyphs`, and `Setup::set_motion`
(`core/canopy/setup.rs:175-204`) have no caller anywhere and can go.
`Setup::cursor_looks` has no caller, but it is the only entry to cursor
rules and the twin of `Setup::widget_styles`, so it stays.
`CanopyBuilder::script_file` and `project_script_root` have no application
caller, and they are documented and tested. Their removal is a product
choice.

### D16: The script bridge makes two live `&mut Canopy`

The one `unsafe` block (`core/script/bridge.rs:75-79`) makes a `&mut Canopy`
from a pointer that a guard pushed. Its SAFETY comment argues that the
pointee is alive, which is true. It does not argue exclusivity, which is
false:

- `host_send_key` pushes the pointer from `canopy` and then calls
  `canopy.key(..)` on the same reference (`base_api.rs:1432-1436`). Three
  more sites have the shape (`:1449-1452`, `:1510-1512`, `:1565-1566`). The
  use of `canopy` after the push invalidates the pointer under the aliasing
  rules.
- The outer host function and every routing frame hold `&mut Canopy` across
  the nested call, and use it afterwards, while the nested side writes
  through the second reference.
- `with_reentrant_canopy` is a safe function that hands out `&mut Canopy`.
  Called inside its own closure, it yields two.

This review did not run Miri and has not shown a miscompilation.
Recommended: lend the reference at the frame that re-enters. Push the
pointer inside `execute_binding_with_scope` (`core/canopy/routing.rs:953-963`)
from `&mut *self`, with a guard that holds `PhantomData<&'a mut Canopy>`, so
`self` is frozen until the guard drops, and delete the four pushes in host
functions. The nested reference is then a child of the innermost live
borrow.

### D17: A fatal error becomes a notice after a Luau hop

`From<Error> for RuntimeError` and `to_canopy_error`
(`core/script/errors.rs:57-69,165-175`) turn `Internal`, `Invalid`, and
`NodeNotFound` into `ScriptStructured`, which `is_notice` treats as a notice
(`core/error.rs:419-427`). So a runtime invariant failure inside a host
function that a Luau binding calls is recorded as a notice. The kind cannot
decide the class: `Error::script(..)`, the error a script raises itself, has
kind `Canopy` and must be a notice (`error.rs:505`). Recommended: carry the
class and do not infer it. Add a
`fatal` flag to the error payload, set from `!err.is_notice()` where the
error crosses into Luau, as the timeout already crosses. Low priority.

### D18: A widget learns that another node's view changed

fh, epea, and two verber widgets record a size in `render`, wake, and
rearrange in `poll` (`fh-audio/src/app.rs:2546-2564`,
`verber/src/stats/mod.rs:1233-1269`, `apps.rs:1719-1741`). Three of the four
cases watch a different node, so a layout hook on the widget's own size would
not serve them. `Context::observe_scroll(node)` (`core/context.rs:850`) already wakes
an observer when another node's offset changes. Recommended: a sibling
`observe_view(node)`, which wakes the observer when the node's view changes
and removes the recording in `render`. C8 removes the glyph case.

### D19: `PollLifetime::Attachment` has no user

No widget in the workspace or the four applications overrides
`poll_lifetime`, and all 33 calls to `wake_handle` pass `PollLifetime::Node`.
The second lifetime costs a `Widget` method, an argument of
`Context::wake_handle`, generation updates on four tree-edit paths
(`core/world/tree.rs:104,186,702,754,838`), and a paragraph of the
architecture. Recommended: remove it, so `wake_handle()` takes no argument.

### D20: `List` checks have no user

`with_checks`, five check methods, and the commands `toggle`, `check_all`,
and `clear_checks` (`crates/canopy-widgets/src/list_view.rs:179-220,456-488`)
have no consumer. The commands are registered on every `List` and do
nothing without checks, and `checked_keys` is in the runtime snapshot and
the Luau record. Recommended: remove them, unless a list with checks is
planned.

### D21: A streamed document without a widget

verber rebuilds the reader's pipeline as a value
(`verber-conversation/src/markdown.rs:80-162`): a `SourceStore`, a parse for
each text change, and a presentation for each geometry change, in about 80
lines. Recommended: `canopy_markdown::StreamedDocument` with `sync`,
`finish`, and `prepare(GeometryKey)`, and a `PresentationSnapshot` method
that says whether a row is blank, which verber also writes
(`transcript.rs:581-586`). Only verber needs it, so it is low in priority.

### D22: The release set

`cargo-release` names `canopy` alone (`Cargo.toml:11`), and eleven more
crates are publishable. Nothing can publish while the sibling path crates
are unpublished (`Cargo.toml:88-101`). Recommended: list every library crate
in the release set when publishing starts.

### D23: `cargo xtask score --check` in CI

Nothing runs the mosaic score check. Its inputs are in the repository, and
its tolerance is 0.1% (`crates/mosaic-score/src/report.rs:334`). Whether the
scores stay inside it on both CI platforms, and how long the run takes, is
not known. Recommended: try it on one platform, and keep it if it is stable.

## Smaller observations

These are low in value. None is a recommendation to act now.

- `canopy_markdown` re-exports `canopy_xmark::Alignment` for one field of
  `MarkdownTable`, and `impl From<BlockQuoteKind> for AlertTone` is public
  while `BlockQuoteKind` is not re-exported. A private function serves its
  one caller.
- `PaintOverlays::images` is a public field whose only mutator is
  `pub(crate)` (`presentation/mod.rs:363,411`). No consumer supplies an
  image to a document that it paints itself.
- `OptionsHandle::subscribe` has no caller outside the crate's tests, and a
  subscriber made with it lives as long as the store. Followers subscribe
  through `subscribe_while` and are pruned.
- `canopy_markdown::testing::spec_examples` reads
  `CARGO_MANIFEST_DIR/../canopy-xmark/specs`, which the xmark package
  excludes, so it fails in a registry install (`testing.rs:331-334`).
- `canopy_xmark::utils` is public and holds two items that the root also
  exports. Upstream has the same module.
- `Parser::new_with_limits` and `new_resumed` have no form with default
  callbacks, and `Limits` has `NONE` and no `Default`.
- `font::FontRenderer` is public, and its only public method is `new`.
  `FontBanner::new(text, Font)` would hide it.
- Some types lack a derive that their siblings have: `PartialEq` on
  `EditorConfig`, `HighlightSpan`, and `MenuItem`, `Default` on
  `editor::WrapMode` and `PaneKeys`, and `Eq` and `Hash` on xmark's
  `Alignment` and `LinkType`.
- `View::hactive` and `View::vactive` compute the parts of a scrollbar
  track. Their one caller outside the runtime is `scrollbar.rs:348-349`, and
  their names do not say what they return.
- `canopy::text` has five ways to truncate: `truncate_end`, `truncate_start`,
  `split_end`, `split_start`, and `Truncate::apply`.
- `canopy_mcp::ScriptOutcome::outcome` holds an `EvalReport`, whose
  `outcome` holds the result, so a caller writes `script.outcome.outcome`.
- `TermHost::launch` and `TermHost::start` differ in who chooses the socket,
  and the names do not say so.
- `canopy_mosaic::clear_into` panics on a raster that is not a multiple of
  8, while the other three matchers take a `RasterView` that was validated.
- `Scroll::horizontal` and `Scroll::both` (`scroll.rs:46,53`) have no test.
- `canopy-xmark` has no fuzz target of its own. It parses arbitrary document
  bytes, and the Markdown targets reach it only through short inputs.
- `examples/todo` is not published, and nothing outside its library names
  `todo::options`. It can be a private module.

## Execution Plan

Each item is one change, or one part of a change that has several. The
stages are in dependency order. Documentation comes after the API stages, so
that it describes the API as it ends. Decisions are not in the plan: one
joins it when its choice is made.

### Stage 1: Runtime API

- [x] C5: Remove `Context::set_style` and `StyleChange`, with the two tests
      that call it (`core/world/tests.rs`, `core/world/change_tests.rs`).
- [x] C5: Remove `Render::restyle` and `TermBuf::restyle_grapheme`.
- [x] C5: Make `commands::SerdeArg` `pub(crate)`, and move its test from
      `crates/canopy/tests/it/commands.rs` into the crate.
- [x] C6: Add `Error::app`, and use it at the 25 workspace sites.
- [x] C7: Add `Constraint::limit`, and use it at the 13 workspace sites.
- [x] C8: Give `CanvasContext` a `glyphs()` accessor, and use it in
      `canopy-markdown/src/widget/mod.rs`.
- [x] C9: Add `ViewContext::status_of`, and use it in
      `canopy-widgets/src/status_line.rs`.
- [x] C10: Add `ContextExt::update`, and use it at the 85 workspace sites.
- [x] C11: Add `BindingOptions::new`, `with_path`, `with_phase`, and
      `hidden`, and use them at the ten workspace sites.
- [x] C12: Make `Rect::right` and `Rect::bottom` public `u32` accessors, and
      use them at the 31 workspace sites.
- [x] C12: Remove `Line::indent` and its test.
- [x] C13: Remove `From<mpsc::RecvError> for Error`, and map the error in
      `AutomationHandle::request`.
- [x] C14: Make `core/style/animation` a private module.
- [x] C15: Move `core/format` to `canopy_widgets::format`, with its tests,
      and change the imports in `canopy-widgets` and `canopy-options`.
- [x] C16: Rename `CommandRequirement::as_str` and `ScriptErrorKind::as_str`
      to `label`, and `BindingPhase::parse` to `from_label`.
- [x] Run `ncode test -p canopy -p canopy-geom -p canopy-widgets -p
      canopy-options` and `ncode api`. Review the capture diff.

### Stage 2: Runtime defects and internals

- [x] C17: Make `Core::focused_within` answer containment alone, with a test
      that calls it from the focused widget's `render`.
- [x] C17: Derive `NodeCtx::is_focused`, `is_on_focus_path`, and
      `focused_within` from `focused_node()`.
- [x] C18: Pass the route focus to `dispatch_action_on_node`, with a test.
- [x] C19: Resolve the root style at the top left cell, with a test that
      sets a gradient root.
- [x] C19: Make `GradientSpec::stops` and `Animation::stops` private, with a
      `stops()` accessor.
- [x] C20: Declare the four values of `KeyRouteOutcome.action`.
- [x] C20: Reject unknown fields in `parse_bind_options` and
      `parse_unbind_selector`, with a test for each.
- [x] C20: Build the error payload in `node_id_from_value`, with a test that
      Rust sees `type_mismatch`.
- [x] C20: Emit a `BindingId` as `UInt` everywhere, and read it with one
      function.
- [x] C21: Add `PublicationWatch::state()`, and use it at the eight sites.
- [x] C22: Add `Default` impls or narrow `new_without_default` in
      `core/mod.rs`.
- [x] C22: Use `#[expect]` with a reason in `core/style/color.rs` and
      `core/event/key.rs`.
- [x] C23: Add the unit twin of `host_value`, and use it at the 18 sites.
- [x] C23: Validate node handles with one helper, and fold `read_node_id`
      and `read_opt_node_id`.
- [x] C23: Use `with_root_view` in `host_find_identity`.
- [x] Run `ncode test -p canopy`, `ncode tidy`, and `cargo xtask smoke`.

### Stage 3: Widget defects

- [x] C24: Give `MenuList` a canvas of its rows, and paint from the scroll
      offset, with a test of a menu taller than its dialog.
- [x] C24: Make the `MenuList` of `ModeHelp` scroll.
- [x] C25: Drive the `DiffView` spinner from a wake handle and `ctx.now()`,
      and poll only while motion is active, with a test under `ManualClock`.
- [x] C26: Pass `now` to `ClickTracker::count` from `Editor` and `Terminal`.
- [x] C26: Add tests for a double click, a triple click, a click after the
      threshold, and a click at another location.
- [x] C34: Count items among the rows of one view height in
      `PickerList::page`, with a test of a grouped picker.
- [x] Run `ncode test -p canopy-widgets`.

### Stage 4: Widget API

- [x] C27: Use `text::width` for the exit message in `terminal.rs`.
- [x] C27: Use `canopy::text::wrap_with` at the six `textwrap::wrap` sites,
      and drop the `textwrap` dependency.
- [x] C27: Use `canopy::text::graphemes` and `grapheme_indices` in
      `table/flow.rs`, `help/binding_list.rs`, `fuzzy.rs`, and
      `tests/vim_diff.rs`, and drop the `unicode-segmentation` dependency.
- [x] C28: Use `#[expect]` at the five `#[allow]` sites, and check each with
      clippy.
- [x] C29: Export `Editor` and `Terminal` at the root only.
- [x] C29: Keep `Menu`, `MenuItem`, `MenuList`, and `MenuRow` at the root
      only.
- [x] C29: Export `ZoomDirection` beside `ImageView`.
- [x] C29: Make `TableCopy` `pub(crate)`.
- [x] C30: Implement `Register` for `Tabs`, `Columns`, `Dropdown<T>`,
      `Selector<T>`, and `List<W, K>`, and use it in gyms and todo.
- [x] C30: Call `Editor::register` at the three gyms sites.
- [x] C30: Remove `#[derive_commands]` from `Terminal` and `Menu`, and the
      `add_commands::<Menu>()` call.
- [x] C31: Rename `Editor::with_config` to `Editor::new`.
- [x] C31: Rename `Picker::filter` to `Picker::field`, returning
      `TypedId<Input>`.
- [x] C31: Take `title: impl Into<String>` in `set_items` and `update_items`
      of `Picker` and `PickerList`.
- [x] C31: Take `selected: usize` in `MenuList::set_items`.
- [x] C32: Remove `ImageView::set_preview_path`, and rewrite its six tests.
- [x] C33: Add `Dialog::set_title`, and use it in `help/mode.rs`.
- [x] C35: Add a start margin to `StatusLine`.
- [x] C35: Add `mosaic::put_cell` outside the `graphics` feature, and call
      it from `image_view` and from `canopy-markdown/src/presentation`.
- [x] C36: Add `editor::position_of_offset` and `editor::offset_of_position`
      over the buffer's logic, with tests that include a CRLF line.
- [x] C37: Implement `Display` and `Error` for `PageError`.
- [x] C37: Add a `# Panics` section to `Paged::new`.
- [x] C38: Rename `menu::styles` and `highlight::palette_style_rules` to
      `style_rules`.
- [x] C39: Take `&dyn ViewContext` in `Picker::selected`, `SearchBar::query`,
      and `Options::is_open`.
- [x] C40: Add `pub use image` under the `graphics` feature.
- [x] C40: Replace the glob re-export of `canopy_mosaic` with `Cell`,
      `Family`, and `Tier`.
- [x] C41: Move `screenshot` and its font assets to `canopyctl`, and drop
      the `graphics` feature from its `canopy-widgets` dependency if nothing
      else needs it.
- [x] C42: Rename `diff::Mode` to `DiffLayout`, and the three methods to
      `with_layout`, `set_layout`, and `layout`.
- [x] C43: Use `with_focusable(bool)` on `Container`, `Pane`, `Text`, and
      `ImageView`.
- [x] C44: Rename `List::with_command` to `with_on_activate`, and
      `ColumnChart::with_command` to `with_on_change`.
- [x] C44: Rename `Picker::set_commands` to `set_on_accept` and
      `set_on_cancel`, and `MenuList::set_close` to `set_on_close`.
- [x] C45: Give the seven `ConfirmRequest` builders the `with_` prefix.
- [x] Run `ncode test -p canopy-widgets -p canopy-markdown -p canopy-options
      -p canopyctl`, `cargo xtask smoke`, and `ncode api`. Review the capture
      diff.

### Stage 5: Markdown

- [x] C54: Start the workers of a default `MarkdownExecutor` at its first
      registration, with a test that a default executor runs no worker
      before it.
- [x] C46: Use the `invalid` helper at the nine `Error::Invalid` sites of
      the inspector, and assert `is_notice()` in
      `table_republication_rejects_a_different_dataset`.
- [x] C47: Disable `interact` and make `set_mode` change nothing in the
      Source view, with a test.
- [x] C47: Write the condition of `set_mode` with explicit grouping.
- [x] C47: Gate `select_text` with `rendered_action_status`.
- [x] C47: Take `&dyn ViewContext` in `Markdown::state`.
- [x] C48: Make `options` a private module.
- [x] C48: Move `DocumentInput` and `FinishOutcome` out of `source_store`,
      so the root is their one path.
- [x] C48: Drop the root re-export of `MarkdownSource`.
- [x] C49: Add `PreparedRun::reading_range`, and use it in `hit`, `paint`,
      and `display_position`.
- [x] C49: Share the three derivations of `status_items` and
      `update_chrome`.
- [x] C50: Put `service`, `settle`, `reader`, and the defaults script in one
      `#[cfg(test)]` support module under `widget`.
- [x] C51: Make `presentation::style_rules` `pub(crate)`.
- [x] C52: Make `Link::resolve`, `resolve_link`, and `ResolvedLink`
      `pub(crate)`.
- [x] C53: Rename `source_store::SourceError` to `StoreError`.
- [x] C55: Rename the four `SourceStore` constructors, and make `replace`
      take a `DocumentInput`.
- [x] Run `ncode test -p canopy-markdown`, `cargo xtask bench-check`, and
      `ncode api`. Review the capture diff.

### Stage 6: Options

- [x] C56: Remove `SetError::Kind`.
- [x] C56: Page the panel by its visible rows, with a test of a short panel.
- [x] C56: Take `impl Display` in `store_error`, and remove its attribute.
- [x] C57: Reject `min`, `max`, and `step` on a leaf that is not an integer,
      with a trybuild case.
- [x] C57: Reject `inline` with `key` on a group, and `inline` on a leaf,
      with a trybuild case for each.
- [x] C57: Reject a variant attribute other than `key` or `label`, with a
      trybuild case.
- [x] C57: Span the limit errors on their attribute, and the default
      expression on the field type.
- [x] C57: Delete the `too_many_lines` attribute of `expand_group`.
- [x] C58: Rename `canopy_options::Layout` to `OptionFiles`, and make
      `discover` take a struct of named paths.
- [x] Run `ncode test -p canopy-options -p canopy-options-derive -p todo`
      and `ncode api`. Review the capture diff.

### Stage 7: Automation and tooling

- [x] C59: Move the factory of hello into its library, and use it in
      `main.rs` and `tests/smoke.rs`.
- [x] C59: Use `make_factory` of todo from its library in `main.rs` and
      `tests/smoke.rs`.
- [x] C59: Build the factory of gyms in one function of `main.rs`.
- [x] C60: Bind the ready socket only with `--fixture`, and implement `Drop`
      for `ReadySignal`.
- [x] C60: Strip the Cargo variables for every program in
      `ResolvedCommand::to_command`.
- [x] C60: Export the Cargo variable predicate from `canopy-term`, and
      remove the copy in `canopyctl`.
- [x] C60: Use `ScreenSize::default()` for the live-session size.
- [x] C61: Remove `canopy_mcp::Error::Command`, and convert at its two test
      sites.
- [x] C61: Add the `[smoke]` section to `examples/todo/.canopyctl.toml`.
- [x] C61: Call `canopy.screen_text()` in `delete_item.luau`.
- [x] C62: Reject arguments to `derive_commands`, with a trybuild case.
- [x] C62: Emit `::canopy::` in the generated code.
- [x] C62: Report an error for two `#[command]` attributes on one method,
      with a trybuild case.
- [x] C63: Make `run_cargo_command` call `run_command`.
- [x] C64: Format the whole workspace in CI.
- [x] C64: Run `cargo xtask big-text --check` and `cargo xtask xmark-specs
      --check` in CI.
- [x] C65: Move `image`, `fontdue`, `syn`, `quote`, and `proc-macro2` to
      `[workspace.dependencies]`.
- [x] C65: Add a `description` to the seven manifests that lack one.
- [x] C65: Set `doctest = false` in `crates/canopy-term/Cargo.toml`.
- [x] Run `ncode test`, `ncode tidy`, `cargo xtask smoke`, and `ncode api`.

### Stage 8: Tests

- [x] C66: Add `wait_for_text`, `wait_for_no_text`, and `wait_for_notice`
      to `Harness`, with a test of each.
- [x] C67: Move the mouse spec test to `core/event/mouse.rs`.
- [x] C67: Move the pure `Layout::validate` test to
      `crates/canopy/src/layout.rs`.
- [x] C67: Move the three reveal tests and their helper to
      `core/world/scroll/tests.rs`.
- [x] C68: Test that a failed edit unmounts in reverse order.
- [x] C68: Test that a mount that failed succeeds when it is tried again.
- [x] C68: Test that a scrollbar thumb keeps at least one cell.
- [x] C68: Test that `FocusGained` reaches a widget through a turn.
- [x] C68: Test that `validate_semantic_keys` rejects a bad key.
- [x] C69: Add tests for `border.rs` and `run_paint.rs`.
- [x] C69: Add a test for `Dropdown::handle_click`.
- [x] C70: Delete the three rename guards of
      `examples/gyms/src/tests/names.rs`, and rename the test that stays.
- [x] C71: Add a proptest for `Key::parse_spec`.
- [x] C71: Add a proptest for `Mouse::parse_spec`.
- [x] Run `ncode test`.

### Stage 9: Documentation

- [x] C1: Delete `core/mod.rs:6` and the outer doc lines of `cursor`,
      `status`, `effects`, and `key`.
- [x] C1: Delete the outer doc lines of the twelve modules in
      `crates/canopy-widgets/src/lib.rs`.
- [x] C1: Delete the doc line on the `pub use` group at
      `core/script/mod.rs:35`.
- [x] C2: Correct `docs/getting-started.md` and `docs/agent-loop.md` on
      `LaunchMode` and `launch`.
- [x] C2: Correct `docs/fixtures.md` on the reset policy and on what
      `cargo xtask smoke` finds.
- [x] C2: Correct `docs/markdown.md` on `AlertTone` and the `graphics`
      feature.
- [x] C2: Correct `docs/architecture.md` on the module lists,
      `validate_invariants`, and the `Canopy` facade.
- [x] C2: Correct `docs/options.md`, `docs/scripting.md`, and
      `docs/styles.md`.
- [x] C3: Correct the docs and examples of `ChildSlot` and `slot!`.
- [x] C3: Correct the `StyleMap::rules` example.
- [x] C3: List every public module in the crate docs of `canopy`.
- [x] C3: Correct the docs of `ViewContext`, `node_id`, `edit_structure`,
      and `remove_after_dispatch`.
- [x] C3: Say on `Error::Invalid` and `Error::NotFound` that they are fatal
      runtime errors.
- [x] C3: Correct the docs of `Error::Parse`, `Path::new`, `host_prepare`,
      and `bind`.
- [x] C4: Remove the drifted call from `docs/getting-started.md`, and update
      the page for C59.
- [x] C4: Add the test in `examples/hello/tests` that checks each Rust block
      of the page against the example.
- [x] Run `ncode test -p hello`, `ncode tidy`, and `ncode api`. Review the
      capture diff.

### Stage 10: Applications

- [x] Adjust fh to the changed API, and replace its workarounds: the
      spinner tick (C25), the status reads (C9), the text positions (C36),
      and the executor default (C54).
- [x] Adjust verber, and replace its copies: the dialog titles (C33), the
      start margin (C35), and the harness waits (C66).
- [x] Adjust epea, and replace its copy of the text positions (C36).
- [x] Adjust markkit, and call `mosaic::put_cell` (C35).
