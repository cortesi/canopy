# Review: Canopy against its users

## Description

This is a review of the Canopy workspace against its two applications, fh
(`../fh`) and verber (`../../private/verber-mono/verber`), made on
2026-11-05. It covers the public API, the runtime, the widgets, the Markdown
and options crates, the automation tooling, and the tests. Six parallel
reviews read the code, and every item below was then checked at its cited
lines. Each change is one that a maintainer can implement without a further
decision. Items that need a design choice are under Decisions, with the choice
this review recommends.

The baseline is good. Clippy is clean across the workspace with every feature
and target. The 3744 tests pass in about 15 seconds under nextest. `ncode api
--check` passes. A keyed reconciliation of 1,000 rows takes 0.6 ms, so the
full-arena snapshot that each tree edit takes is not a cost worth chasing. The
changes here remove duplication, tighten the public surface, fix documentation
that has drifted from the code, repair the defects that the applications work
around, and add the tests that the architecture promises but the suite lacks.

API changes migrate fh and verber in the same change, and refresh `api/`.
Line numbers are those of the tree at the review date. Paths under
`crates/canopy/src/` are written from `core/` where the crate is clear.

## Changes

### Documentation and captures

#### C1: One doc comment for each public module and re-export

A `///` comment on a `pub mod` declaration joins the module's own `//!`
comment, so the API captures show two leading sentences, one of them stale:

- `crates/canopy/src/core/mod.rs:6-7` puts `Keyed child collection helpers.`
  and `Command definition and dispatch.` on `pub mod commands`. The first is
  a leftover.
- `core/mod.rs:9` (`cursor`), `core/mod.rs:31` (`status`), `core/style/mod.rs:4`
  (`effects`), and `core/event/mod.rs:1` (`key`) repeat what the module's
  `//!` says.
- `crates/canopy-widgets/src/lib.rs` puts a `///` on `chart`, `diff`,
  `editor`, `fuzzy`, `highlight`, `menu`, `mosaic`, `paged`, `regex_search`,
  `screenshot`, `table`, and `view_search`, each of which has its own `//!`.
- `crates/canopy/src/core/script/mod.rs:35` puts `Base canopy scripting API
  declarations and native registration.` on a `pub use` group, so eleven
  re-exported items (`AutomationCallback`, `AutomationHandle`, `EvalId`,
  `EvalOutcome`, `EvalRequest`, `EvalTicket`, `ScriptJournalEntry`,
  `ScriptOrigin`, `ScriptTrust`, `Fixture`, `FixtureInfo`) carry that wrong
  doc in `api/canopy.rs`.

Decision: the module file's `//!` is the one doc. Remove the outer `///`
lines, and move the `script/mod.rs:35` line to `mod base_api`, which already
has it. `ncode api` then shows one doc for each module and the right doc for
each re-export.

#### C2: The docs say what the code does

Each of these sentences names something the code does not have:

- `docs/getting-started.md:307` says a `LaunchMode::Api` prints the Luau API.
  `LaunchMode` has `Run` and `HeadlessMcp` only
  (`crates/canopy-mcp/src/launch.rs:23-33`). Every binary prints the API
  itself before it picks a mode. Say so.
- `docs/agent-loop.md:21` says the third `launch` argument is a `RunOptions`.
  `launch` takes two arguments, and `RunOptions` is a field of
  `LaunchMode::Run` (`launch.rs:41`).
- `docs/fixtures.md:25` says the reported reset policy is always the declared
  one. A live session reports `ResetPolicy::External` whatever the app
  declares (`crates/canopy-mcp/src/metadata.rs:235-242`, with its reason in a
  comment, and a test at `script.rs:1008`). Describe the live rule.
- `docs/fixtures.md:70` says `cargo xtask smoke` discovers every
  `.canopyctl.toml`. It lists tracked files only (`xtask/src/main.rs:404`).
- `docs/markdown.md:137` names `AlertTone::of`. The conversion is `impl
  From<BlockQuoteKind> for AlertTone` (`crates/canopy-markdown/src/document.rs:785`).
- `docs/markdown.md:347-348` says the crate does not need the `graphics`
  feature of `canopy-widgets`. Its manifest requires it
  (`crates/canopy-markdown/Cargo.toml:21`).
- `docs/architecture.md:38-43` and `:89-91` list the public modules of
  `canopy-widgets` as `editor`, `highlight`, `font`, `terminal`, `scrollbar`,
  `diff`, `list`, `paged`, and `mosaic`. The crate also publishes `chart`,
  `fuzzy`, `menu`, `regex_search`, `screenshot`, `table`, and `view_search`,
  and every one has a consumer. List them.
- `docs/architecture.md:258-259` says every layout pass ends with
  `validate_invariants`. It runs under `cfg!(debug_assertions)` only
  (`core/world/layout_driver/mod.rs:46-50`). Say debug builds.
- `docs/options.md:216-219` says only `set` toasts. `toggle` and `cycle`
  toast too (`crates/canopy-options/src/widget.rs:157,179`).
- `core/script/mod.rs:715-723` documents `LuauHost::logs` and `assertions` as
  the most recent evaluation's, but only the synchronous `execute` path
  publishes them (`mod.rs:1024`). The doc describes that path.

#### C3: Doc comments describe their items

- `ChildSlot` reads `A typed slot for slots.` and `slot!` reads `Define a
  typed slot for slots.` (`core/context.rs:32,55`). Say a typed key for a
  keyed child.
- The `StyleMap::rules` example imports `canopy::style::solarized`, which does
  not exist, and uses `Color` without importing it
  (`core/style/mod.rs:736-745`).
- `ViewContext` says it is available "during render and measure"
  (`context.rs:105-108`), but `measure` takes no context. `node_id` says "the
  node currently being rendered" (`context.rs:110`), but contexts exist in
  events and polls too.
- `Context::edit_structure` says rollback restores "diagnostic requests"
  (`context.rs:671`). `TreeStateSnapshot` (`core/world/mod.rs:153-174`) holds
  none.
- `Context::remove_after_dispatch` says "Remove the current widget
  incarnation" and takes a node (`context.rs:742-748`).
- `Error::Parse` has its doc comment after its `#[error]` attribute
  (`core/error.rs:322-324`). `Path::new` says "from a slice" and takes an
  iterator (`core/path.rs:32`).
- `host_prepare` names `canopy.flush` (`core/script/base_api.rs:2127`), and
  the `bind` docs omit `MenuAction` (`:633-634, 1785-1786, 1812-1813`).

#### C4: Getting started matches the hello example

`docs/getting-started.md` says every code block comes from `examples/hello`.
Of its eight Rust blocks, one has drifted: the `on_mount` block adds
`.with_style("status_bar/text")` to the status text, which
`examples/hello/src/lib.rs:72` does not. Two more match only after
indentation, and one changes `join("smoke")` to `join("../../smoke")` for the
external layout the page teaches.

Decision: fix the drift, and add a test to `examples/hello/tests` that reads
the page, dedents each Rust block, applies the one documented substitution of
the smoke path, and asserts that the block is a substring of `src/lib.rs`,
`src/main.rs`, or `tests/smoke.rs`. The test keeps the page honest.

### Runtime API

#### C5: Remove `Context::set_style` and `Render::restyle`

`Context::set_style` (`core/context.rs:858`) has no caller outside the
runtime: not canopy-widgets, fh, verber, or the Luau API. Its one use is a
runtime test (`core/world/tests.rs:210`), and its doc says to prefer
`set_theme`. Remove it with `StyleChange::Map` (`core/canopy/rendering.rs:360`).
`Render::restyle` (`core/render/mod.rs:406`) has no caller anywhere, tests
included. Remove it. `SerdeArg` (`core/commands.rs:526`) has one internal
caller and the derive does not use it. Make it `pub(crate)`.

#### C6: `Error::app` constructs the application error

`Error::App(Box<dyn Error + Send + Sync>)` is the notice-class variant for an
application's own failure, and it is built 50 times as
`Error::App(Box::new(..))` or `Error::App(message.into())` across the
workspace and both applications. `canopy_mcp::Error::app` is the model.
Add `Error::app(error: impl Into<Box<dyn Error + Send + Sync>>) -> Self`
and use it at those sites.

#### C7: `LayoutOverride::fill`

`Layout::fill().into()` and `LayoutOverride::new().flex_horizontal(1).flex_vertical(1)`
spell one override. The workspace and both applications write the pair 77
times (fh alone at `canopy-fileselect/src/mount.rs:119,196,207,212`,
`host.rs:217,355`, `canopy-speech/src/voice_pane.rs:287,292`,
`fh-audio/src/app.rs:1850,1860,1873,1892`). Add `LayoutOverride::fill()`,
the twin of `Layout::fill()`, and use it.

#### C8: `Constraint::limit`

Eight match arms in canopy-widgets and three in verber and markkit pick a
fallback for `Constraint::Unbounded` (`list_view.rs:890`, `confirm.rs:535`,
`font_banner.rs:172`, `inspector/logs.rs:119`, `notice_log.rs:180`,
`text.rs:254`, `diff_view.rs:1176,1188`). Add
`Constraint::limit(self) -> Option<u32>`, which returns the bound of `AtMost`
and `Exact` and `None` for `Unbounded`, so a caller writes
`c.width.limit().unwrap_or(n)`.

#### C9: `CanvasContext` carries the glyphs

`MeasureConstraints` carries `glyphs` so a widget measures as it draws
(`docs/architecture.md:635-637`). `CanvasContext` does not
(`api/canopy.rs:3376-3387`), so a widget whose canvas depends on the mosaic
tier reads the glyphs in `render`, compares them with the tier it used, and
wakes itself to lay out again
(`verber-conversation/src/widget.rs:470-478`, `reasoning.rs:191`). Give
`CanvasContext` a `glyphs()` accessor filled from `Core::glyphs`, as
`MeasureConstraints::glyphs` is.

#### C10: `ViewContextExt::status_of`

`StatusLine::items` (`crates/canopy-widgets/src/status_line.rs:77-81`) and
four fh sites (`canopy-fileselect/src/status.rs:61,117,129`,
`fh-audio/src/app.rs:1967`) read another widget's status items with the same
four lines of `with_widget_dyn` and `Widget::status`. Add
`ViewContextExt::status_of(&self, node) -> Result<Vec<StatusItem>>` and use
it in all five.

#### C11: `ContextExt::update` for setters

Of fh's 307 `with_widget_mut` calls, 41 take a closure that ignores its
context and ends in `Ok(())`. In verber, 70 of 140 do. Each sets a field or
calls a setter. Add

```rust
fn update<W, R>(&mut self, node: impl Into<NodeId>, f: impl FnOnce(&mut W) -> R) -> Result<R>
```

on `ContextExt`, which calls `with_widget_mut` and so still invalidates
layout. The stock widgets use it where it fits, and the applications migrate
at their pace.

#### C12: `Setup::bind_framework` installs a table of keys

Five widgets of canopy-widgets install their framework group with the same
loop over `(key, description, call)` triples, each building a six-field
`BindingOptions` with `tier: Framework(group)`, `phase: Some(AfterWidget)`,
`show_in_help: true`, `source: None`, and a `**/name/**/` path
(`answers.rs:342`, `confirm.rs:421-434`, `list_view.rs:136`, `menu.rs:531`,
`picker.rs:903`). verber copies the loop six more times
(`verber/src/keys.rs:656`, `grants.rs:444`, `models/form.rs:251`,
`models/card.rs:501`, `models/mod.rs:1992`,
`verber-conversation/src/question_dialog.rs:259`). Add

```rust
fn bind_framework(&mut self, group: FrameworkBindingGroup, path: &str,
                  keys: &[(&str, &str, CommandCall)]) -> Result<()>
```

to `Setup`, and use it in all eleven.

#### C13: `canopy::text` measures columns in `u32`

`text::width` returns `u32` and `wrap` takes `u32`, as `geom` does. But
`slice_by_columns`, `truncate_end`, `truncate_start`, `split_end`,
`split_start`, `cell_width`, `tab_width`, and `grapheme_width` use `usize`
for columns. fh converts about sixty times, verber sixteen
(`verber-conversation/src/transcript.rs:514,828,1881,2782`, `excerpt.rs:80,92`,
`question.rs:1105-1118`, which defines three helpers only to convert). Both
applications then reach for `textwrap` and `unicode-segmentation` directly
where `canopy::text` already serves. Change those eight functions to take and
return `u32` columns. Byte offsets stay `usize`.

#### C14: Public rectangle edges and consistent constructors in `canopy-geom`

- `Rect::right` and `Rect::bottom` are private `u64` helpers
  (`crates/canopy-geom/src/rect.rs:32-39`) while `RectI32::{left, top, right,
  bottom}` are public. 28 sites write `tl.x.saturating_add(w)` or its twin
  by hand. Examples: `canopy/src/layout.rs:842`, `termbuf/mod.rs:407,431`,
  `render/mod.rs:471`, and `canopy-widgets/src/{status_bar.rs:236,
  status_line.rs:210, list_view.rs:856, menu.rs:614, sparkline.rs:188,
  button.rs:248}`. Make `right` and `bottom` public saturating `u32`
  accessors, and rename the two `u64` helpers that `contains_point` and
  `contains_rect` use.
- `Line::indent` (`line.rs:39`) has no caller anywhere. Its callers went with
  the confirmation dialog rewrite (`ce8cf844`). Remove it.
- `Rect::new` is `const fn`, `RectI32::new`, `Line::new`, and `Size::rect`
  are not. Make them `const`.

#### C15: Read-only queries live on `ViewContext`

`Context::available_bindings(&self)` (`core/context.rs:585`) and
`Context::current_event(&self)` (`:719`) take `&self` and read the core
(`core/help.rs:156`, `core/world/mod.rs:256`), but they sit on `Context`, so a
widget cannot list bindings or read the event from `render`. Move both to
`ViewContext`. `Context::end_toast` returns `bool` (`:791`) while every
sibling mutation returns `ChangeOutcome`. Return `ChangeOutcome`.

#### C16: Hidden nodes keep their scroll offset

`clear_layout` sets a hidden node's scroll to zero
(`core/world/layout_driver/mod.rs:766-778`). A node that shows again starts
at the top, so fh saves and restores offsets or reveals the selection again
in five places (`canopy-fileselect/src/side.rs:89-97`, `view_pane.rs:386`,
`find.rs:400-406`, `view_watch.rs:201,217`, `directory_watch.rs:153-168`).
The architecture calls the view a cache, and the scroll offset is state.

Decision: `clear_layout` keeps `scroll`. The hidden node's `view` is still
cleared. `validate_cached_state` (`core/world/tree.rs:504-537`) skips the
scroll bound for a hidden node, because its canvas and content are zero while
it is hidden. When the node shows again, `update_canvas` clamps the offset to
the new canvas as it does today. A test hides a scrolled node, shows it, and
reads the same offset.

#### C17: Close a modal and name the focus that follows

`close_modal` restores the focus the modal replaced
(`core/world/modal.rs:381-408`), and `set_focus` to a node outside the open
modal fails (`core/world/focus.rs:58-64`). A widget that closes a modal and
wants the focus elsewhere cannot say so in one call. fh works around it three
ways:

- a pending field and `request_poll` (`canopy-fileselect/src/view.rs:408-424`),
- a 100 ms poll that retries `set_focus` until the modal is gone
  (`fh-audio/src/app.rs:819-831`),
- a posted call that runs after the close
  (`fh/src/commander/pickers.rs:290-294, 324-332`).

Add `Context::close_modal_focusing(token, node)`. It queues the
close as `close_modal` does, and when the close runs, it focuses `node` in
place of the restored ancestry when `node` is live and admitted. Tests cover
a live node, a removed node, and a node inside another open modal.

### Runtime defects

#### C18: `focused_within` answers containment alone

`Core::focused_within` requires the focused node to be a focus candidate with
a view (`core/world/focus.rs:107-112`), and candidacy reads the widget's
cell (`focus.rs:522-527`). Inside the focused widget's own `render` or
callback the cell is borrowed, so `focused_within(self)` returns `None`
although the widget has focus. `is_focused` applies no such rule, the doc
promises only containment (`core/context.rs:199-201`), and the prediction
branch of `NodeCtx::focused_within` (`context.rs:1200-1203`) applies none
either. verber calls it in `render` (`verber-conversation/src/editor.rs:372`)
and in a callback (`call.rs:201`). Make `Core::focused_within` return the
focus when `root` contains it, and nothing else. The focus is valid by the
runtime's invariants. A test calls it from the focused widget's `render`.

#### C19: Intent dispatch uses the route focus

Routing selects an intent with the route focus
(`core/canopy/routing.rs:296-298`, prediction `:383`), but
`dispatch_action_on_node` recomputes `focus = self.focus.unwrap_or(self.root)`
(`core/select.rs:171-175`). A widget earlier on the route that moves focus
and returns `Ignore` makes the navigation fallback disagree with the
selection that admitted it. Pass the route focus in.

#### C20: Two reachable panics in styling

- `render_pass` expects the root style to resolve solid
  (`core/canopy/rendering.rs:250-253`). `Canopy::style_mut().rules().fg("",
  gradient).apply()` merges a gradient into the root rule
  (`core/style/mod.rs:765-771, 799-803`) and the next frame panics. Resolve
  the root style at the top left cell of the screen instead.
- `GradientSpec::stops` and `Animation::stops` are public fields
  (`core/style/mod.rs:236-248`, `animation.rs:143`). `color_for_ratio` indexes
  `stops[0]` (`mod.rs:377`) and `expect`s at `:386`, so an emptied list
  panics at emission. An empty list resolves to `Color::Black`, and the
  constructors keep a `debug_assert!` that the list is not empty.

#### C21: The Luau surface agrees with itself

- `KeyRouteOutcome.action` is declared as `script`, `command`, or `intent`
  (`core/script/defs.rs:703-706`), but `records.rs:900` emits
  `BindingActionKind::label`, which includes `menu`. `action_field()`
  (`defs.rs:375-381`) has the right list. Use it.
- `canopy.bind` reads its option fields one by one and ignores the rest
  (`core/script/base_api.rs:662-710`), while `keymap`, `set_cursor_look`,
  `set_motion`, and `notify` reject an unknown field (`:1894-1897, 728-750`).
  Reject unknown fields in `bind` too.
- `node_id_from_value` writes `kind = type_mismatch` as bare fields without a
  `CanopyErrorPayload` (`core/script/bridge.rs:281-288`), so Luau sees
  `type_mismatch` and Rust sees `canopy_error`. Build the payload as the other
  errors do.

#### C22: Redundant invalidations

`Event::Resize` invalidates paint and then `set_screen_size` invalidates
layout (`core/canopy/routing.rs:929-930`). `replace_root` invalidates paint
after a tree edit that already invalidated layout
(`core/canopy/mod.rs:345-346`). Drop the first of each pair.

### Runtime internals

#### C23: `Path` shares its components

`Core::path_of` (`core/world/tree.rs:1056-1077`) calls `to_string` on each
`NodeName`, and `Path::new` (`core/path.rs:33-41`) calls it again, although
`NodeName` holds an `Arc<str>` (`path.rs:321-324`). A path is built per route
start, per route step (`core/world/modal.rs:89` clones it again), per
`explain_key` step (`core/keyroute.rs:169`), and per `matching_nodes`
candidate (`context.rs:260`). Store `Arc<str>` components, and give `Path` a
crate-private constructor from node names. `Path::new` and `pop` keep their
signatures.

#### C24: One helper for each repeated runtime sequence

- `op.target(..)` followed by `clamp_scroll(&mut target, content, canvas)`
  appears five times (`core/world/scroll/mod.rs:94-95, 108-109, 134-135,
  245-246`, `core/world/layout_driver/mod.rs:717-718`). Add
  `Node::clamped_scroll(target)`. `outcome(changed)` (`scroll/mod.rs:393-400`)
  and two hand-written `if changed { Changed } else { Unchanged }`
  (`tree.rs:1046-1052`, `scroll/mod.rs:110-114`) become `impl From<bool> for
  ChangeOutcome`.
- `push_notice_source`, a dispatch boundary, and `pop_notice_source` are
  written four times in `core/canopy/routing.rs:386-390, 682-713, 776-779,
  845-851` and once in `rendering.rs:64-70`. Add
  `Canopy::with_notice_source(source, f)`.
- `PublicationWatch` writes `.lock().expect("publication lock poisoned")`
  seven times (`core/canopy/turn.rs:167,171,181,210,223,243,251`). Add one
  `state()` accessor.
- `NodeCtx::is_focused`, `is_on_focus_path`, and `focused_within` each branch
  on `focus_override` (`core/context.rs:1177-1205`). Derive the three from
  `focused_node()`.
- `MountedWidget::identity` exists (`core/world/tree.rs:66-69`), and
  `tree.rs:316` repeats `Rc::as_ptr(..) as usize`. Call it.
- `route_input_inner` is 239 lines (`core/canopy/routing.rs:254-493`). Split
  it at its three stages: intent offers (`:298-371`), widget dispatch with
  the prediction check (`:372-420`), and fallback binding, default action,
  and bubble (`:422-482`).

#### C25: Avoidable work on hot paths

- `key_for` runs a full `explain_key` for every admitted key
  (`core/help.rs:131-149`). Filter `input_map.bindings()` by
  `target.matches(&record.action)` first, and explain those keys alone.
  `candidate_inputs` sorts with `sort_by_key(ToString::to_string)`
  (`core/inputmap/mod.rs:907`), one allocation per comparison. Use
  `sort_by_cached_key`.
- A mouse event hit-tests the tree up to three times: `routing.rs:737` for
  `Leave`, `:177` for the modal check, and `:191` for the route start.
  Compute the node under the pointer once in `mouse` and pass it down.
- `visible_children` allocates a `Vec` three times per node per layout pass
  (`core/world/layout_driver/mod.rs:390,520,688`), and `clear_layout` clones
  `children` per hidden node (`:773`). Collect once per node, and iterate by
  index as `update_views` does.
- `Render::cursor` clones the effect stack before it checks that a frame
  exists (`core/render/mod.rs:247-250`). `Render::with_origin` copies
  `self.effects` to a `Vec` although it is a `&'a [Effect]` (`:314-316`).
  `Core::set_children_inner` clones the `Vec` it owns and then consumes the
  original (`core/world/tree.rs:828,833`). Fix each.
- `text::expand_tabs` segments with `s.graphemes(true)` (`core/text.rs:156`)
  where the crate's own `graphemes()` skips segmentation between ASCII
  characters.

#### C26: Lint attributes in the runtime

`core/mod.rs:1` carries `#![allow(clippy::new_without_default)]` over the
whole core tree. Add `Default` impls or narrow the attribute to the types that
need it, with `#[expect]` and a reason. `core/style/color.rs:390,406,499,536`
use `#[allow(clippy::many_single_char_names)]`. Use `#[expect]` with a reason,
as the four `multiple_inherent_impl` attributes do.

#### C27: One helper for each repeated scripting pattern

- `with_current_canopy(..)?; Ok(ret_none())` is written 20 times in
  `core/script/base_api.rs`. Add a unit twin of `host_value`
  (`core/script/dispatch.rs:56-62`).
- Node handles are validated through the live canopy five times
  (`base_api.rs:936-938, 952-954, 1153, 1472, 2168`), and `read_node_id` and
  `read_opt_node_id` (`:927-958`) are near copies. One helper.
- A `BindingId` crosses to Luau as `i64` (`:1095`), as `UInt` (`records.rs:306,
  649, 854, 891`), and comes back from `Int` or `UInt` (`:1482-1483`). Emit
  `UInt` everywhere, and read it with one function.
- Mouse events are built twice (`:1491-1509`, `:1551-1560`), tagged records
  decoded twice (`:1129-1164`, `:1444-1475`), and numeric fields parsed twice
  (`:866-881`, `:2226-2236`).
- `binding_info_to_arg` and `available_binding_to_arg` share nine field writes
  (`records.rs:297-331`, `:648-679`). `FixtureInfo` and `ScriptJournalEntry`
  are built by hand (`:264-280, 750-799`) although both derive `Serialize`
  and `screen_capture` already goes through `SerdeArg` (`:567`).
- `find_node` and `find_nodes` (`base_api.rs:1345-1366`) repeat the body of
  `ViewContext::find_nodes` (`core/context.rs:231-234`), and `host_set_focus`,
  `host_focus_move`, and `host_find_identity` build a `CoreContext` by hand
  (`:1378-1380, 1391-1393, 1328-1329`) where `with_root_context` serves.
- `register_framework_declarations` (`defs.rs:95-319`) and
  `register_observation_info` (`defs.rs:467-752`) are flat alias lists of 225
  and 286 lines. Split them by record family, as `register_snapshot_info`
  (`:755`) already is.

### Widgets

#### C28: `MenuList` scrolls

`MenuList::measure` asks for one row per item, `canvas` returns the view,
and `render` paints from the first item regardless of the scroll offset
(`crates/canopy-widgets/src/menu.rs:555-562, 610-620`). When the dialog
clamps the list, the rows past the view are cut, the selection moves onto
them, and `cursor.reveal` cannot move a canvas that never exceeds the view.
`PickerList::canvas` (`picker.rs:944-948`) and `NoticeList::canvas`
(`notice_log.rs:162`) do it right. Fix: canvas `Size::new(view.w,
rows.max(view.h))`, and paint through `label_rows` from `view.scroll.y`. A
test opens a menu taller than its dialog and selects the last row. fh's file
switcher uses `Menu`.

#### C29: `DiffView` drives its own spinner on the driver clock

`set_loading` stores `Instant::now()`, `render` reads `elapsed()`, and
`poll` returns the spinner period while loading
(`crates/canopy-widgets/src/diff_view.rs:323-325, 1143-1152, 1168-1171`).
Nothing starts the poll, so the spinner turns only while a host polls: fh
ticks every 50 ms for it (`fh/src/changes.rs:40, 843-847`). The wall clock
also defeats `ManualClock`, and the poll ignores `motion_active`.
`SearchProgress` (`search_progress.rs:73-84, 133-144`) is the model: it keeps
a wake handle from `on_mount`, wakes in `set`, reads `ctx.now()`, and polls
only while motion is active. Make `DiffView` do the same. `set_loading` takes
the context.

#### C30: `ClickTracker` takes the time from the context

`ClickTracker::count` reads `Instant::now()` (`crates/canopy-widgets/src/click.rs:40`),
so multi-click detection in `Editor` (`editor/widget.rs:1000`) and `Terminal`
(`terminal.rs:347`) cannot run under the test clock, and `click.rs` has no
tests. Pass `now` from `ctx.now()`, and add tests for a double click, a
triple click, a click after the threshold, and a click at another location.

#### C31: One width measure, one wrapper, one segmenter

- `terminal.rs:523` centers the exit message by `chars().count()`. Use
  `text::width`, as `docs/architecture.md:91-93` requires.
- `help/binding_list.rs:352,361,370` and `diff_view.rs:1112` call
  `textwrap::wrap` directly, and `toast.rs:212-256` wraps by hand with a
  three-line limit and a marker. `canopy::text::wrap_with` with
  `WrapOptions { paragraphs: true, max_lines: Some(3), .. }` is that toast
  wrap. Route all five through `canopy::text`, and drop the `textwrap`
  dependency of canopy-widgets (`Cargo.toml:25`).
- `table/flow.rs:21` and `editor/vi/mod.rs:14` import
  `unicode_segmentation` where `flow.rs:179,260,699` already use
  `canopy::text::grapheme_indices`. Replace the two calls and drop the
  dependency (`Cargo.toml:26`). `icu_segmenter` in the editor layout is a
  line breaker and stays.

#### C32: Lock poisoning and lint attributes in canopy-widgets

- `inspector/logs.rs:53,194,238` call `.lock().unwrap()` in library code. A
  writer that panics on another thread poisons the mutex and takes the
  inspector down with it. Use `unwrap_or_else(PoisonError::into_inner)`, as
  `Watch` and `request_slot` do.
- `input.rs:473`, `editor/widget.rs:1237`, and `chart.rs:498` use `#[allow]`
  where the rest of the crate uses `#[expect]` (`confirm.rs:617`,
  `diff_view.rs:737,801,1052`, `run_paint.rs:39`, `table/mod.rs:1384`).
  Switch them, so a lint that stops firing is noticed.

#### C33: Every widget at the root once

- `Editor` and `Terminal` live only at `canopy_widgets::editor::Editor` and
  `canopy_widgets::terminal::Terminal` (`api/canopy-widgets.rs:4334, 6134`),
  against the rule at `docs/architecture.md:38`. Export both at the root. The
  modules keep their supporting types.
- `Menu`, `MenuItem`, `MenuList`, and `MenuRow` resolve at the root and at
  `canopy_widgets::menu::` (`lib.rs:64,151`). Follow the `list` pattern: the
  widgets live in a private module, and `pub mod menu` re-exports
  `RowLayout`, `render_row`, and `styles` only.
- `ImageView::zoom` and `call_zoom` take `ZoomDirection`, which no path
  names (the capture header lists it as unnameable). Export it beside
  `ImageView`.
- `TableCopy` (`table/mod.rs:1532`) appears in no public signature and only
  `table_view` uses it. Make it `pub(crate)`.

#### C34: Registration is complete and honest

- `Tabs` (`tabs.rs:202`) and `Columns` (`columns.rs:51`) carry commands but
  implement no `Register`, so hosts register them by hand
  (`examples/gyms/src/style.rs:1105`, `list.rs:247`). Add the impls.
- `Terminal` (`terminal.rs:171`) and `Menu` (`menu.rs:277`) carry
  `#[derive_commands]` with no `#[command]` outside tests. Remove the
  attribute and the `add_commands::<Menu>()` call at `menu.rs:489`.

#### C35: Names and shapes agree across sibling widgets

- `Editor::with_config(text, config)` is the only constructor
  (`editor/widget.rs:307`). Rename it `Editor::new(text, config)`.
- `Picker::filter()` returns the field's node (`picker.rs:229`) and
  `PickerList::filter()` returns the filter text as a command
  (`picker.rs:506`), which fh calls. Rename the first `Picker::field()`, as
  `SearchBar::field()` is named.
- `Selector::selected_index` returns `usize` and 0 for an empty selector
  (`selector.rs:103-105`). `List`, `MenuList`, `PickerList`, and `Paged`
  return `Option`. Return `Option<usize>`.
- `Picker::set_items` takes `title: &str` and `PickerList::set_items` takes
  `label: String` for one value (`picker.rs:122, 511`). Both take `impl
  Into<String>` and call it `title`.
- `Menu::set_items` takes `selected: usize` and `MenuList::set_items` takes
  `Option<usize>` (`menu.rs:308, 401`). Both take `Option<usize>`.
- `Dropdown::confirm` and `cancel` are the only commands in the crate without
  a context parameter (`dropdown.rs:126,152`). Add it.
- The context parameter is named `c` in 44 methods, `context` in 54, and
  `ctx` elsewhere. Name it `ctx` everywhere.

#### C36: Remove public items nothing uses

`Frame::with_scrollbar_glyphs` and `Columns::with_scrollbar_glyphs`
(`frame.rs:53`, `columns.rs:93`) have no caller, and `ScrollbarGlyphs` has
one value, `THIN`, so nothing else can be passed. `ImageView::set_preview_path`
(`image_view/mod.rs:698`) has no caller. fh decodes with
`DecodedImage::read(..).fit(..)` on a worker and calls `set_preview`. Remove
the three.

#### C37: One panel shell and one notice row

- `Help` (`help/mod.rs:22-72`), `NoticeLog` (`notice_log.rs:38-93`), and
  `ModeHelp` (`help/mode.rs:30-45,110-124`) are each a unit struct whose
  `install` builds `Dialog::new().with_title(..).with_max_width(72)
  .with_margin(Edges::symmetric(2, 1)).add(..)`, finds its list by
  `unique_descendant`, pushes one layer in `render`, and swallows mouse
  events in `on_event`. One `Panel` type with a layer name and a body
  replaces the three.
- `Dialog::frame(ctx, id)` followed by `with_widget_mut(frame, set_title)`
  appears at `menu.rs:294-300,351`, `help/mode.rs:87-100`,
  `confirm.rs:387-390`, and `picker.rs:269,694-697`. Add
  `Dialog::set_title(ctx, dialog, title)`.
- `toast.rs` `Shown` (152-195) and `notice_log.rs` `LogRow` (96-125) hold the
  same `message`, `count`, and `level`, with the same `count_width`, once in
  `u32` and once in `usize`, and the same level-to-style match
  (`toast.rs:293-297`, `notice_log.rs:217-221`). One `NoticeRow` with
  `count_width()` and `level_part()` serves both.

#### C38: Modal helpers that `Confirm` has and its siblings lack

`Confirm::open(ctx, confirm, owner, request)` builds the `ModalOptions`,
opens the modal, and stores the token in the dialog (`confirm.rs:244-267`).
`Menu` and `Picker` have no such helper, so fh builds the options by hand
(`canopy-fileselect/src/switcher.rs:81-90`, `fh/src/commander/pickers.rs:423-465`),
and verber copies the three-step dance seven times
(`verber/src/keys.rs:297-320`, `grants.rs:500-535,687-700`,
`models/form.rs:83-105`, `models/card.rs:409-445`, `models/mod.rs:494-562`,
`verber-conversation/src/question_dialog.rs:63-85`). `Confirm` also lacks the
close half: hosts keep the token themselves (`fh/src/commander/dialogs.rs:36,111,146`,
`fh-audio/src/app.rs:164,335,342`). Add `Menu::open(ctx, menu, owner) ->
Result<ModalToken>`, `Picker::open(ctx, picker, owner)`, and `close(ctx)` on
all three, which closes the stored token once and ignores a dialog that is
not open.

#### C39: `Markdown::navigate` is public

`TableReader::navigate(ctx, NavIntent)` is public. The Markdown reader has
the same method on its surface (`crates/canopy-markdown/src/surface.rs:653`)
but exposes it only through `on_intent`, so fh dispatches a navigation by
its string name through `with_widget_dyn_mut`
(`canopy-fileselect/src/view_pane.rs:978-987`). Add `Markdown::navigate(&mut
self, ctx, NavIntent) -> Result<()>` beside the reader's other commands.

#### C40: `PickerList::page` counts rows the view shows

`PickerList::page` moves the cursor by `view_rect().h` items
(`picker.rs:450-454`), but the rows include group headings
(`picker.rs:338-344`), so with groups a page skips more items than the view
shows. Count the items among the rows of one view height.

#### C41: Small widget options the applications ask for

- `StatusLine::with_margin` keeps columns clear at the end only
  (`status_line.rs:68-72`). verber pads the start by finding the line inside
  its `StatusBar` with `unique_descendant` and a layout override
  (`verber/src/app.rs:1078-1095`). Add a start margin.
- `Columns` without a frame always draws a track under its thumbs
  (`columns.rs:99-108`). verber's plan asks for a bare thumb so its
  conversation can drop its `Frame`. Add `Columns::with_track(bool)`.
- `ImageView` paints its mosaic cells with a per-cell pen that applies the
  effect stack and resolves the ground (`image_view/mod.rs:524-565`), and
  markkit's `Logo` copies the loop (`markkit-canopy/src/lib.rs:170-236`).
  Move it to `mosaic::paint_cells(render, origin, cells, ground)` and call it
  from `ImageView`.

#### C42: Text positions and byte shifts without a widget

- `Editor::offset_of_position` and `position_of_offset` need an `Editor`.
  fh-audio reimplements both on a `&str` (`fh-audio/src/view.rs:49-84`) for
  text it has not loaded into an editor. Add `editor::offset_of_position(text,
  position)` and `editor::position_of_offset(text, offset)` as free
  functions, and make the methods call them.
- `crates/canopy-markdown/src/widget/mod.rs:84-85` says a host shifts a
  source byte through an edit itself. fh wrote the walk
  (`canopy-fileselect/src/view_watch.rs:285-319`, with tests), and every host
  that watches a file needs it. Move it to `canopy_markdown::shift_byte(old,
  new, byte)` with its tests, and have fh call it.

### Markdown reader

#### C43: The inspector reports application failures as notices

`Markdown` maps its failures through `invalid()` to `Error::App`
(`crates/canopy-markdown/src/widget/mod.rs:273-275`), which is a notice. The
inspector builds `Error::Invalid` at nine sites (`inspector.rs:753,755,901,948,
1103,1108,1125,1173,1227`), which is fatal outside a command
(`core/error.rs:440`). Two reach a poll: `publish_table`'s error (`:901`)
propagates through `Markdown::receive` (`widget/mod.rs:2559`) to `poll`
(`:3396-3404`), and `publish_matches` maps to `Invalid` inside
`Inspector::poll` (`:1227`). Use the one `invalid()` helper in the inspector
too, and add a test that a failed publish in the inspector's poll records a
notice and keeps the application running.

#### C44: Hosts can supply an image

`PaintOverlays::images` is a public field of type `ImagePixels`
(`presentation/mod.rs:330`), whose only mutator `insert` is `pub(crate)`
(`:378`). A host that paints documents itself, as verber does, cannot supply
an image. Make `insert` public.

#### C45: Sibling commands of `Markdown` agree

- `set_mode` (`widget/mod.rs:810-831`) never checks `self.active`, so `i` in
  the Source view flips the hidden rendered surface into Interact. Check the
  active view as `select_text` does.
- `select_text` (`:1120-1130`) returns an error for the Source view but has no
  `enabled` gate, while `activate` and `next_object` carry
  `rendered_action_status`. Give it the gate.
- `state(&self, ctx: &mut dyn Context)` (`:2367`) is a read-only query that
  takes a mutable context. `queries::state` takes `&dyn ViewContext`. Take
  `&dyn Context`, as `Options::is_open` does.
- `WorkHandle::acknowledge` is `acknowledge_document` with the document of
  the previous presentation (`executor.rs:772-803`). The widget calls one and
  the inspector the other. Keep `acknowledge_document` and fold the other.

#### C46: One public path for each Markdown item

`pub mod options` and the root both export `Follow`, `MarkdownOptions`,
`Mode`, and `ViewType` (`lib.rs:17,38`). `source_store::{DocumentInput,
FinishOutcome}` and `source_view::MarkdownSource` are at the root too
(`lib.rs:39-40`), and verber imports from both paths side by side
(`verber-conversation/src/markdown.rs:14-19`). Decision: `options` becomes
private, and the root keeps its four types. `DocumentInput` and
`FinishOutcome` live in `source_store` only. `MarkdownSource` is a widget and
stays at the root. fh and verber migrate.

#### C47: Internal duplication in the presentation and the widget

- `prepare_image` and `prepare_embed` carry `#[allow(too_many_arguments)]`
  and take the tuple `(inset, card_inset, body, rails)`
  (`presentation/mod.rs:1778-1792, 1860-1872`). A `BlockFrame` struct with
  those four fields removes both attributes.
- The reading offset of a grapheme in a run, `if run.text.len() ==
  range.len() { range.start + byte } else { range.start }`, is written in
  `hit` (`:876-883`), `paint` (`:1026-1040`), and `surface::display_position`
  (`surface.rs:1336-1341`). Add `PreparedRun::reading_offset(byte)`.
- `last_submitted` is an anonymous six-tuple (`widget/mod.rs:180`, built at
  `:2435-2442`) that repeats `WorkTags` (`executor.rs:103-120`) without
  `source_options` and `limits`, so `set_source_options` and `set_centered`
  bump `generation` to force a resubmission. Store a `WorkTags`.
- `status_items` (`:2919-2983`) and `update_chrome` (`:2985-3076`) each
  derive the partial flag, the new byte count, and the action or link. One
  struct of those facts feeds both.
- `prepare_block` is 378 lines (`presentation/mod.rs:1387-1764`). Its table
  arm (`:1443-1540`) and heading arm (`:1627-1734`) are self-contained
  functions.

#### C48: Markdown tests share a builder and a cached corpus

- `service`, `settle`, and `reader` are each written twice
  (`widget/tests.rs:64-106`, `cursor_tests.rs:60-87`, `slot_tests.rs:69-74`,
  `edit_props.rs:71-76`), and `.script("markdown-defaults",
  "markdown.default_bindings()")` fourteen times across six files. Put one
  reader harness builder in `testing.rs`, and use it.
- `spec_examples()` reads and extracts the whole canopy-xmark spec corpus
  from disk at each of its six call sites in `units/tests.rs`
  (`testing.rs:331-372`). Cache it in a `OnceLock`.

### Options

#### C49: Subscribers that expire leave the store

`subscribe_root` only pushes (`crates/canopy-options/src/store.rs:602-607`),
and nothing removes a subscriber. `Follower::follow` subscribes at each mount
(`follow.rs:85-99`) with a closure over a `WidgetStamp`. Once the widget is
gone, `with_widget_mut_or_post` returns `Delivery::Expired` and the closure
stays, running on every change for the life of the store. fh and verber
mount followers per widget against one shared store
(`fh/canopy-fileselect/src/lib.rs:315,326`, `verber/src/app.rs:720,1056`).
Decision: a subscriber callback returns whether it is still live, and
`notify` drops the ones that are not. A test mounts, removes, and changes,
and counts the subscribers.

#### C50: Small fixes in canopy-options

- `SetError::Kind` is declared and displayed (`model.rs:475-480,489`) but
  constructed nowhere. Remove it.
- The panel pages by a fixed 10 rows (`panel.rs:1278`) while `render`
  derives the visible row count (`panel.rs:1502`). Page by the visible rows.
- `widget.rs:46-49` puts `#[allow(needless_pass_by_value)]` on the whole
  command impl, where the workspace convention is per method
  (`canopy-widgets/src/input.rs:473`). Move it to the `String`-taking
  commands. `store_error` (`widget.rs:331`) needs no attribute as
  `fn store_error(error: impl Display) -> Error`, which is how canopy-markdown
  writes `invalid`.

#### C51: The options derive rejects what it ignores

- `min`, `max`, and `step` on a leaf that is not an integer pass the derive
  (`crates/canopy-options-derive/src/lib.rs:331-350`) and are ignored at
  runtime (`canopy-options/src/model.rs:611-614,626`). `inline` with `key` on
  a group drops the key (`model.rs:320-325`). A variant attribute other than
  `key` or `label` is ignored (`lib.rs:529-542`). Each becomes a compile
  error, modeled on `assert_leaf` (`lib.rs:385-387`), with a trybuild case.
- "min is above max" and "step must be positive" use `Span::call_site()`
  (`lib.rs:143,146`), so the error lands on the derive rather than the
  attribute. A `skip` field of a type without `Default` fails on a
  `Default::default()` emitted at the call site (`lib.rs:277`). Span both on
  the attribute and the field type.
- `expand_group` carries `#[allow(too_many_lines)]` (`lib.rs:238-241`,
  body `:242-495`). Split the per-field emission (`:272-412`) from the final
  `quote!`. `OptionAttrs` carries `#[allow(struct_excessive_bools)]`
  (`lib.rs:72-75`) because `skip`, `flatten`, and `inline` are three
  exclusive roles as three bools, with `skip` winning silently (`:281`). One
  enum removes the attribute and the ambiguity.

### Automation and tooling

#### C52: `AppFactory::new` takes any error

Every factory closure ends in `.map_err(McpError::app)`: hello
`main.rs:77`, todo `main.rs:67-68`, gyms `main.rs:254`, fh `lib.rs:384`,
verber `lib.rs:232`, which first wraps an `anyhow::Error` in
`CanopyError::Invalid` to get there, and the five smoke tests. Make
`AppFactory::new` generic:

```rust
pub fn new<F, E>(metadata: AppMetadata, factory: F) -> Self
where F: Fn() -> Result<Canopy, E> + Send + Sync + 'static,
      E: Into<Box<dyn Error + Send + Sync>>
```

and wrap the error as `Error::App`, which is what every caller does today.
`anyhow::Error` converts, so verber's closure returns its own result.

#### C53: Each example builds its factory once

hello, todo, and gyms each build `AppFactory::new(AppMetadata { .. }, ..)`
in `main.rs` and again in their smoke test (`hello/src/main.rs:72-78` and
`tests/smoke.rs:15-23`, `todo/src/main.rs:55-71` and `tests/smoke.rs:17-26`,
`gyms/src/main.rs:249-255` and `src/tests/smoke.rs:13-37`). fh and verber
expose `pub fn factory(..) -> AppFactory` from their library and share it.
Move each example's factory into its library, and have `main.rs` and the test
call it. `docs/getting-started.md` then teaches the shape the applications
use.

#### C54: `canopyctl` and `canopy-term` fixes

- `canopyctl run` without `--pty` binds a ready socket at `tmp/cctl-*.ready`
  (`crates/canopyctl/src/main.rs:487`) and consumes it only with a fixture
  (`:499`). `ReadySignal` has no `Drop`, and only `wait` removes the file
  (`crates/canopy-term/src/ready.rs:46-54`). Implement `Drop` for
  `ReadySignal`, which removes the path, and simplify `wait`.
- `ResolvedCommand::to_command` strips the launching package's Cargo
  variables only when the program's file stem is `cargo`, `ncode`, `edev`, or
  `canopyctl` (`crates/canopyctl/src/config.rs:304-310`), while the terminal
  path strips them for every program (`canopy-term/src/lib.rs:411-422`). A
  shell wrapper around cargo inherits the wrong identity on one path only.
  Strip for every program.
- The Cargo variable filter is written three times: `xtask/src/cargo_env.rs`,
  `crates/canopyctl/src/cargo_env.rs` (both say "kept in sync by hand"), and
  inline in `canopy-term/src/lib.rs:411-422`. xtask must keep its copy. Export
  the filter from canopy-term and remove canopyctl's copy.
- The live-session defaults `120` and `40` (`canopyctl/src/main.rs:356-357`)
  repeat `ScreenSize::default()` (`canopy-mcp/src/metadata.rs:52-57`). Use
  the default.
- `canopyctl run` prints the socket to stderr without `--pty` (`main.rs:496`)
  and to stdout with it (`main.rs:539`). The docs say the host prints its
  socket. Print to stdout in both.
- `TermHost::eval` returns a JSON value and `screen` a vector of rows, so
  gyms and fh repeat `== Value::Bool(true)` and `.join("\n")`
  (`examples/gyms/tests/terminal.rs:48,57`, fh `tests/terminal.rs`). Add
  `TermHost::eval_bool` and `screen_text`.

#### C55: `canopy-mcp` fixes

- `canopy_mcp::Error::Command` and its `From` impl (`error.rs:17`) are never
  constructed. Remove the variant.
- `examples/todo/.canopyctl.toml` has no `[smoke]` section, so its suite runs
  with no timeout and `fail_fast = false`, unlike hello and gyms. Add the
  section hello has.
- `examples/todo/smoke/with_items/delete_item.luau:1-15` rebuilds
  `canopy.screen_text()` from `snapshot().cells`. Call `screen_text`.

#### C56: `canopy-derive` fixes

- `derive_commands` ignores its attribute arguments (`lib.rs:35`), so
  `#[derive_commands(anything)]` compiles. Reject a non-empty argument stream
  with a spanned error.
- The generated code names the runtime crate as `canopy::` in all 45
  `quote!` blocks (`codegen.rs`, `lib.rs`), while `::std::` is qualified.
  canopy itself relies on `extern crate self as canopy` (`canopy/src/lib.rs:25`),
  so `::canopy::` resolves there too. Use the leading `::`, as
  canopy-options-derive does with `::canopy_options`.
- `expand_command_arg` and `expand_command_enum` emit the same `CommandType`
  impl skeleton (`lib.rs:150-168` and `:281-295`). One helper takes the alias
  body.
- `parse_command_macro_args` keeps the last of two `#[command(..)]`
  attributes on one method (`parse.rs:172-210`). Error on the second.

#### C57: Gyms declare their demos once

- `Demo::configure` and `Demo::build` are two parallel 20-arm matches over
  the same variants (`examples/gyms/src/main.rs:261-331`). Each demo
  describes itself once: a register function, a bindings function, and a
  constructor.
- 19 `binding_setup` functions have three shapes (`builder.script(name,
  BINDINGS)`, the same plus one `widget_styles`, and two register calls).
  One helper in `main.rs` takes the name, the bindings, and an optional style
  rule set.
- The gallery list lives twice: `xtask/src/main.rs:179-194` and
  `examples/gyms/src/tests/smoke.rs:56-85`, hand-synced. Decision: each
  gallery script names its gym and arguments in a first-line comment, `--
  gym: imgview ../../.assets/shyness.jpg`. xtask and the test read that line,
  so there is one source.

#### C58: xtask runs a child command one way

`run_command` (`xtask/src/main.rs:384-400`), `run_cargo_command`
(`:424-445`), and `soak.rs:76-105` each match a child status and print the
same two messages. `run_cargo_command` builds the command and calls
`run_command`, and soak does the same.

#### C59: CI checks what the repository has

`.github/workflows/ci.yml` is red by design until the sibling crates publish,
and its steps should still be right:

- The format step lists 10 of 17 crates (`:42-45`). Add `canopy-markdown`,
  `canopy-mosaic`, `mosaic-score`, `canopy-options`, `canopy-options-derive`,
  `canopy-term`, and `canopy-xmark`.
- Add `cargo xtask score --check`, which `CLAUDE.md` asks for after matcher
  changes and nothing runs.
- Add the freshness checks `cargo xtask big-text --check`, `unicode-mosaics
  --check`, and `xmark-specs --check`.

#### C60: Manifests

- `image = "0.25.10"` is declared in five manifests, `fontdue = "0.9.4"` in
  three, and `syn`, `quote`, and `proc-macro2` in both derive crates. Move
  them to `[workspace.dependencies]`.
- `crates/canopyctl/Cargo.toml:27-28` declares `tokio` with `io-util` as a
  dev-dependency, a feature its normal dependency already has. Remove the
  entry.
- `canopy`, `canopy-derive`, `canopy-geom`, `canopy-mcp`, `canopy-widgets`,
  `canopyctl`, and `todo` have no `description`. Add one each.
- `canopy-term/Cargo.toml` is the one library without `[lib] doctest =
  false`, and `ncode audit` flags it. Add it.
- No crate names a `rust-version`, while CI pins 1.96.1 and the code uses
  let chains. Add `rust-version = "1.96"` to `[workspace.package]`.

### Tests

#### C61: Harness helpers for the waits every suite writes

verber's tests define `wait_for_text`, `wait_for_no_text`, `wait_for_notice`,
and `screen` over `Harness::wait_until` (`verber/tests/app.rs:196-262,
2729-2733`), and the same shapes recur in fh and the gyms. Add
`Harness::wait_for_text(timeout, text)`, `wait_for_gone(timeout, text)`, and
`wait_for_notice(timeout, text)`, whose failure message includes the screen
and the notices.

#### C62: The runtime's tests use the harness and share their widgets

- `Harness` and `HarnessBuilder` (`core/testing/harness.rs`) have no user
  among the runtime's own tests. 27 tests set a screen size and prepare by
  hand (`core/canopy/testing.rs:166-170`, `motion_tests.rs:92-101`,
  `observe_tests.rs:71-82`, `turn_cleanup_tests.rs:61-71`,
  `snapshot_tests.rs:41-43`, `rendering_tests.rs:31-34`, `leave_tests.rs:57-78`,
  `pointer_tests.rs:97-130`, `post_tests.rs:322-345`, `canopy/tests.rs` at six
  sites, and `core/testing/contracts.rs:113-121`). `HarnessBuilder::clock`
  and `motion` already exist for the two that need them. Use the builder.
- A focusable leaf widget is defined 16 times, a poll counter 4 times, a
  mouse event builder 4 times, and a `BindingOptions` literal 12 times across
  `core/canopy/tests.rs`, `post_tests.rs`, `pointer_tests.rs`,
  `observe_tests.rs`, `motion_tests.rs`, `world/tests.rs`,
  `scroll/tests.rs`, and `inputmap/tests.rs`. `core/world/test_support.rs`
  already holds `LayoutWidget` and `simple_widget`, and `testing/ttree.rs`
  a shared recording `State`. Add the focusable leaf, the poll counter, the
  event builders, and a binding options builder there, and use them. Typed
  widget reads through `with_widget_dyn_mut` and `downcast_mut` at ten sites
  in `canopy/tests.rs` and `post_tests.rs` become `Harness::with_widget`.
- `termbuf/tests.rs:279-303 RecBackend` repeats `testing/backend.rs
  TestRender`, `termbuf/tests.rs:339-383 ReplayBackend` is a subset of
  `ModelBackend` (`:642-690`) with one use, and `rendering_tests.rs:81-129
  FailingBackend` repeats `termbuf/tests.rs:305-337`. Keep one of each.

#### C63: Runtime tests sit with the code they test

Each of these tests moves to the module it tests:

- The poll, wake, and turn-batch suite at `core/canopy/testing.rs:115-386`
  moves to a test module of `turn.rs`.
- The `script_api` and `check_script` tests at `core/canopy/tests.rs:2461-2500,
  3238-3248` move to `script/tests.rs`.
- The `widget_styles` test at `canopy/tests.rs:3198-3215` moves to
  `style/mod.rs`.
- The `Mouse::parse_spec` and `Display` test at `leave_tests.rs:200-210` moves
  to `event/mouse.rs`.
- The `Layout::validate` tests at `layout_driver/tests.rs:1401-1404, 797-809`
  move to `src/layout.rs`.
- The reveal tests at `layout_driver/tests.rs:695-795` move to
  `world/scroll/tests.rs`.
- The `CrosstermRender` test at `termbuf/tests.rs:209-215` moves to
  `backend/crossterm/output.rs`.

Then split `core/canopy/tests.rs` (3280 lines) by concern: automation, mouse,
key bindings, notices and hooks, modes, route analysis, and intents, as its
sections already read.

#### C64: Tests for the promises the architecture makes

Each of these sentences of `docs/architecture.md` has no test, and each is one
test:

- `:168` "unwinds completed mounts in reverse order": two completed mounts,
  then a failure, and the unmount order.
- `:169-170` "a failed mount can run again on a later attachment attempt": a
  widget that fails once and succeeds on the next attach.
- `:462-463, 472` "a view with a newer stamp keeps its offset but still clips
  the area" and "replacement clears requests and stamps": read the stamp
  after a replacement.
- `:510` "a thumb is at least one cell long": assert `active.h >= 1` for a
  canvas far larger than its track.
- `:577-578` "if a pre-render hook marks layout dirty, Canopy runs layout
  again": a hook that changes a layout, and the frame that follows.
- `:701-703` "layout settles before each mouse event that follows another
  event": one `TurnInput::Events` batch with a key that moves a node and a
  click at its new place.
- `:795-797` "a release mismatch is treated as a decline": the three
  `#[should_panic]` tests at `core/canopy/tests.rs:1965-1976, 3174-3176` run
  only in debug builds. Add a release-mode assertion that the route continues
  and the trace records the mismatch.
- `:890-891` paste and focus-change events reach widgets: `FocusGained` and
  `FocusLost` delivered to the focused widget.
- `:955-956, 959-960` focus recovery falls back to the first focusable node,
  and layout moves focus off a node that received no area.
- `:263-264` the semantic-key index invariant: a negative test of
  `validate_semantic_keys` (`core/world/semantic.rs:92`).

#### C65: Shared widget test scaffolding and the missing widget tests

- A root that mounts a subtree and reports node ids through
  `Rc<RefCell<Vec<NodeId>>>` is written as `Scene` in `scrolling_tests.rs:30-47`,
  `Host` in `pane.rs:155-182`, and again in `container.rs:178-206` and
  `tabs.rs:480-560`, with focusable leaf stubs beside each.
  `notice_test_support.rs` is already a shared test module. Add `Scene`,
  `Build`, and `FocusLeaf` to it.
- `click.rs`, `border.rs`, `run_paint.rs`, `Scroll::horizontal` and `both`,
  and `Dialog`'s margin and click swallowing have no direct tests. Add them.
  `dropdown.rs:268-361` sets private fields to reach states its commands
  should reach, and its collapsed click (`dropdown.rs:135-148`) has no test.
  Drive it through `toggle`, `select_by`, and `confirm`, and test the click.
- Twelve assertions in `inputmap/tests.rs:418, 554-604` compare
  `registry_status(..).label()` strings although `RegistryStatus` is an enum
  that `:989-991` already matches. Match the variants.

#### C66: Large widget files move their tests to a child module

`button.rs` (tests from line 407 of 1329), `confirm.rs` (585 of 1602),
`root.rs` (590 of 1474), `picker.rs` (1085 of 2541), `list_view.rs` (934 of
1866), `chart.rs` (755 of 1319), `column_chart.rs` (739 of 1419),
`diff_view.rs` (1303 of 1801), `terminal.rs` (950 of 1575),
`image_view/mod.rs` (797 of 1428), and `toast.rs` (343 of 703) hold their
tests inline, while `big_text`, `paged`, `table`, `table_view`,
`table_reader`, `help`, and `editor` keep them in a child `tests.rs` under
`#[cfg(test)] mod tests;`. Move the inline modules to the child layout. The
tests keep private access. `highlight.rs` holds its `syntect` module at lines
172-1657 behind a feature gate: make it `highlight/syntect.rs`, so the gate
sits on one `mod` line.

## Decisions

These need a choice that this review does not make for the implementer. Each
has the recommended answer.

- **The `Canopy` facade repeats `Context` and `Setup`.** `set_glyphs`,
  `glyphs`, `set_theme`, `enter_mode`, `leave_mode`, `notify`, `notices`,
  `available_bindings`, and `set_motion` exist on `Canopy` and on a context
  or `Setup` (`core/canopy/mod.rs:365-688`), against the rule at
  `docs/architecture.md:71-72`. `Canopy::palette`, `cursor_looks`,
  `Setup::cursor_looks`, `Setup::set_theme`, `Setup::set_glyphs`,
  `Setup::set_motion`, `CanopyBuilder::script_file`, and
  `project_script_root` have no consumer. Recommended: keep the facade for
  tests and tools, and remove the eight items with no consumer after fh and
  verber confirm they do not plan to use the script roots.
- **A post-layout hook.** fh changes state in `render` because that is the
  first place the laid-out size is known (`fh-audio/src/app.rs:1935-1944`),
  and three verber widgets record their size in `render`, wake, and
  rearrange in `poll` (`verber/src/stats/mod.rs:1236-1271`,
  `apps.rs:1694-1716`, `verber-conversation/src/widget.rs:470-478`). The
  architecture forbids mutation in `render`. Recommended: add
  `Widget::on_layout(&mut self, size: Size, ctx: &mut dyn Context)`, called
  after the views publish for a node whose content size changed, inside a
  dispatch boundary, with layout run again when it invalidates. C9 removes
  the glyph half of the problem on its own.
- **The reentrant script bridge.** The one `unsafe` block
  (`core/script/bridge.rs:75-79`) makes a second `&mut Canopy` from a raw
  pointer while the outer `ContextMut` guard and the routing call's
  `&mut self` are on the stack. The SAFETY comment argues liveness, not
  exclusivity. Recommended: state the exclusivity assumption in the comment
  now, and evaluate passing the live `&mut Canopy` down through the scope
  context, which removes the block.
- **Fatal errors after a Luau hop.** `From<Error> for RuntimeError` and
  `to_canopy_error` (`core/script/errors.rs:57-69, 164-174`) turn `Internal`,
  `Invalid`, and `NodeNotFound` into `ScriptStructured`, which `is_notice`
  treats as a notice for every kind but timeout, cancel, and busy
  (`core/error.rs:417-422`). A runtime invariant failure inside a host
  function that a Luau binding calls is recorded as a notice. Recommended:
  `is_notice` returns false for `ScriptStructured` of kind `Canopy`.
- **A `Send` notice sender.** verber wraps `Watch` three times to carry
  notices from worker threads, and drains the queues in two polls
  (`verber/src/feed.rs:572-690`, `verber-conversation/src/feed.rs:199-258`,
  `verber/src/stats/source/mod.rs:71-125`). Recommended: a
  `runtime::NoticeSender` from `Context::notice_sender()`, `Send + Sync`,
  whose `post` queues the notice and wakes the driver, which records it at
  the start of the next turn.
- **Styled wrapping.** verber wraps runs of `(style, text)` twice
  (`verber-conversation/src/transcript.rs:1681-1760`,
  `verber/src/stats/timeline.rs:542-600`), and canopy wraps plain text only.
  Recommended: `text::wrap_runs(runs, width, options)`, and a `Text` that
  takes runs.
- **Request slot lanes.** fh keeps three hand-rolled slots beside three
  uses of `request_slot` because it needs a low-priority prefetch lane
  (`canopy-fileselect/src/view_loader.rs:111-202`, `find_worker.rs:110-220`,
  `fh/src/history/mod.rs:119-230`). Recommended: `SlotSender::prefetch`, a
  second lane the worker takes when the first is empty.
- **A streamed document without a widget.** verber rebuilds the reader's
  pipeline as a value (`verber-conversation/src/markdown.rs:79-203`): a
  `SourceStore`, a parse per text change, a presentation per geometry change.
  Recommended: `canopy_markdown::StreamedDocument` with `sync`, `finish`, and
  `prepare(GeometryKey)`, plus `PreparedRow::text()` and `is_blank()`, which
  verber also writes (`transcript.rs:565-610, 971-987`).
- **The executor and search surface.** `WorkHandle`, `WorkRequest`,
  `PreparedView`, and the whole `search` module of canopy-markdown are public
  with no consumer but the crate's bench. Recommended: keep
  `MarkdownExecutor::{new, manual, shutdown}`, `ExecutorOptions`, and
  `ManualExecutor` public, and move the rest behind the `testing` feature,
  which the bench enables.
- **`set_mode` precedence.** `mode == Mode::Interact && self.inspector.is_some()
  || self.slot.is_some()` (`canopy-markdown/src/widget/mod.rs:811`) refuses to
  leave Interact while a slot is open and allows it over an inspector.
  Recommended: parenthesize as `mode == Interact && (inspector || slot)`, to
  match `select_text`, and add the test either way.
- **Two shapes for a Luau node id.** A live call returns `ArgValue::Node`
  (`core/script/value.rs:155-158`) and a marshaled call returns a `{type,
  token}` map (`value.rs:318`, `bridge.rs:257-264`). Recommended: document
  the two, since the token is for reports and the userdata for calls.
- **Luau and Rust surfaces differ.** Luau alone has `send_key`, `send_click`,
  `prepare`, `screen_text`, `capture`, `diagnostic_dump`, `bind`, `keymap`,
  `unbind`, and `clear_bindings`. Rust alone has `apply_fixture`, the motion
  and cursor look getters, `set_theme`, `end_toast`, `focus_first`, `exit`,
  `path_of`, and `is_attached`. Recommended: add `apply_fixture` to Luau and
  leave the rest, which are automation on one side and application plumbing
  on the other.
- **`PollLifetime::Attachment`.** No widget in the workspace, fh, or verber
  overrides `poll_lifetime`, so the attachment generations serve the
  runtime's own tests. Recommended: keep it. It is documented and tested, and
  its cost is a field.
- **Fuzzy match positions.** `fuzzy::rank` returns ranks only, so fh runs
  nucleo beside it to highlight matches (`canopy-fileselect/src/find.rs:122`,
  `find_worker/fuzzy.rs:194-217`). Recommended: `fuzzy::rank_with_positions`.
- **The `Error::Invalid` class.** Applications reach for `Error::Invalid` as
  their own error (verber builds it 43 times), and it is fatal when a poll
  returns it. C6 gives them `Error::app`. Recommended: keep the classes, and
  document `Invalid` and `NotFound` as runtime errors on the variants.
- **Release set.** `ncode audit` reports eleven publishable crates absent
  from the `cargo-release` setting, which names `canopy` alone. Nothing can
  publish while the sibling path crates are unpublished. Recommended: list
  every library crate in the release set when publishing starts, and leave
  the setting until then.
- **`examples/gyms/src/tests/names.rs`.** The three tests guard a completed
  rename and test nothing a user sees. Recommended: delete the file.
- **`List` checks.** `with_checks` and the check methods of `List` have no
  consumer and no non-test use. Recommended: keep them, as they are a
  documented capability.
- **Fuzz targets.** The three targets cover the editor and the Markdown
  units. `canopy-xmark`, `Key::parse_spec`, `Mouse::parse_spec`,
  `canopyctl::replay::parse_replay`, and `canopy_term::parse_input` take
  untrusted input and have none. Recommended: add one target each for the
  two spec parsers and the replay parser.
- **Live replay of an isolated recording.** Because a live session reports
  `External`, a replay recorded headless against an `Isolated` app reports a
  `reset` mismatch when replayed live (`canopyctl/src/replay.rs:98-102`).
  Recommended: compare `reset` only between two recordings of the same
  execution mode.

## Execution Plan

### Stage 1: Documentation, captures, and manifests

- [ ] C1, C2, C3, C4, C60. Run `ncode api`, `ncode tidy`, and `ncode test -p
      hello`.

### Stage 2: Runtime API and defects

- [ ] C5, C6, C7, C8, C9, C10, C11, C12, C14, C15, with tests in
      `crates/canopy` and `crates/canopy-geom`. Migrate the stock widgets to
      C7, C8, C10, C11, C12.
- [ ] C13, with the callers in canopy-widgets and canopy-markdown. Migrate fh
      and verber in the same change.
- [ ] C16, C17, C18, C19, C20, C21, C22, with a test for each defect.
- [ ] Run `ncode test -p canopy -p canopy-geom`, `ncode api`, and review the
      capture diff.

### Stage 3: Runtime internals

- [ ] C23, C24, C25, C26, C27. Run `ncode test -p canopy` and `cargo xtask
      bench-check`.

### Stage 4: Widget defects and hygiene

- [ ] C28, C29, C30, C31, C32, C40, with a test for each defect.
- [ ] C33, C34, C35, C36, C37, C38, C41. Migrate fh and verber.
- [ ] C39, C42, with the fh call sites.
- [ ] Run `ncode test -p canopy-widgets -p canopy-markdown`, `ncode api`,
      `cargo xtask feature-check`, and the fh and verber tests.

### Stage 5: Markdown and options

- [ ] C43, C44, C45, C46, C47, C48.
- [ ] C49, C50, C51, with a test for the subscriber pruning and a trybuild
      case for each new derive error.
- [ ] Run `ncode test -p canopy-markdown -p canopy-options -p
      canopy-options-derive`, `ncode api`, and the fh and verber tests.

### Stage 6: Tooling and tests

- [ ] C52, C53, C54, C55, C56, C57, C58, C59. Migrate fh and verber to C52.
- [ ] C61, C62, C63, C64, C65, C66.
- [ ] Run `ncode test`, `cargo xtask smoke`, `cargo xtask score --check`,
      `ncode api --check`, `ncode tidy`, and `ncode audit`.
