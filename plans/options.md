# Options affordances

## Description

`canopy-options` is now Canopy's options manager. fh is its only consumer:
the commander (`fh/src/commander/mod.rs`) and the browser
(`canopy-fileselect/src/options.rs`) each follow a group of one store. Their
use shows four kinds of friction that Canopy can remove:

- A widget that follows options must call `Follower::take_pending` in its
  `poll`. The follower applies a change through `with_widget_mut`, which
  fails when the widget is busy, such as when its own command writes an
  option. The follower then sets a pending flag and wakes the widget, and
  the change lands at the poll. Each follower thus needs a poll hook, and a
  widget that forgets one misses changes.
- fh copies Canopy enums to make them options: `GlyphSet` copies
  `canopy::render::Glyphs`, and `LineNumbering` copies
  `canopy_widgets::editor::LineNumbers`. Each copy has a `From` impl back.
  The orphan rule stopped fh from implementing `OptionEnum` for them.
  `canopy-options` now lives in Canopy, so it can implement the trait itself.
- A widget without a host store builds one inline:
  `Follower::new(Store::<T>::default().handle())`. fh does this twice.
- Canopy has no consumer of its own, so only fh's tests and smoke suites
  exercise the panel and the `options` commands. The workspace also warns
  that the `canopy-options` dependency entry is unused.

This plan adds a deferred widget call to the runtime, option impls for
Canopy's enums, and a standalone follower. It also adopts options in the
todo example and documents them. fh migrates in the same changes.

## Changes

### C1: Deferred widget calls

The runtime gets a typed call that runs on a widget now, or later if the
widget is busy. The call targets one incarnation of a widget, through a
stamp:

```rust
/// A widget node and the incarnation it held when the stamp was taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetStamp { node: NodeId, incarnation: u64 }

/// How a deferred widget call went.
pub enum Delivery { Ran, Posted, Expired }

// ViewContext
fn widget_stamp(&self, node: NodeId) -> Result<WidgetStamp>;

// ContextExt
fn with_widget_mut_or_post<W: Widget + 'static>(
    &mut self,
    widget: WidgetStamp,
    f: impl FnOnce(&mut W, &mut dyn Context) -> Result<()> + 'static,
) -> Result<Delivery>;
```

- A stamp whose node is gone, or holds another incarnation, gives
  `Expired`, and `f` does not run. This is not an error.
- If the widget's cell is free, `f` runs at once, as with
  `with_widget_mut`, and its error returns to the caller.
- If the cell is out because the widget is on the call stack, the call
  joins the completion queue (`core/world/completion.rs`) as a new
  `CompletionRequest`. The request keeps the same fields as a
  `PostRequest`: the origin node, the target node and its incarnation, the
  event in scope, and the notice source. It runs as the outermost dispatch
  boundary completes, at the same time as a call from `Context::post`, and
  in the event that it was queued in.
- A queued call follows the rules of a posted call. A target that is gone
  or replaced when the call would run drops it. A failure stops the drain.
  The failure shows as a notice only at a notice boundary, such as input
  or a poll (`Canopy::with_notice_boundary`). A direct `with_context` call,
  a command, or a script gets the error back.
- A queued call uses one slot of `MAX_COMPLETION_REQUESTS` and one of
  `MAX_POSTED_CALLS`.
- A type mismatch fails at once, as with `with_widget_mut`. Only a busy
  cell defers the call. A sealed batch refuses the call, as it refuses a
  posted call.
- The dyn-safe core is a `Context` method that takes a boxed
  `FnOnce(&mut dyn Widget, &mut dyn Context) -> Result<()>`. The typed
  method in `ContextExt` downcasts, as `with_widget_mut` does.

Every production entry point that can queue a call opens a dispatch
boundary: `with_context`, `replace_root`, polls, input routing, and the
render sweep's mounts. A child that a callback mounts inherits the
boundary of its caller. The drain does not run as a widget callback
returns, because `on_mount` can run inside an open tree edit. A test covers
a call queued at mount through each entry point.

`Follower` then uses the new call:

- `Follower::follow` takes the widget's stamp, and subscribes with
  `with_widget_mut_or_post`. A change reaches a busy widget as its
  dispatch completes. A widget that is gone or replaced has an expired
  stamp, and the follower ignores the change, as it does now.
- The store runs every subscriber and joins their failures in one
  `options:` notice (`Shared::notify`). A deferred apply runs outside that
  loop, so its failure would stop the completion drain and drop the
  unrelated work behind it. The follower thus posts a deferred apply's
  failure as an `options:` error notice and returns success to the drain.
- `Follower::take_pending` and the `pending` flag go. The follower keeps
  its wake handle only for `wake_on_close`, so a widget still polls as the
  options panel closes.
- fh: `Commander::poll` and `FileSelect` drop their pending checks.
  `FileSelect::poll_options` goes.

### C2: Options for Canopy's enums

`canopy-options` implements `OptionEnum`, `LeafValue`, and `OptionField` for
`canopy::render::Glyphs` and `canopy_widgets::editor::LineNumbers`. The
traits belong to `canopy-options`, so the orphan rule allows this, and
neither enum's crate depends on `canopy-options`. The derive cannot apply
to a foreign type, so a private declarative macro in `canopy-options`
writes the impls from a list of variants and calls `leaf_field!`.

- The labels are those that the derive gives: the variant names in lower
  case, `full`, `blocks`, `none`, `absolute`, and `relative`.
- `Glyphs` declares `Full` before `Blocks`, and `Glyphs::ALL` follows. This
  is an intentional change of public order. The panel then shows the
  default first, as fh's `GlyphSet` does now. `OptionEnum::ALL` promises
  declaration order, so the declaration and `ALL` move together.
- The impl takes `OptionEnum::from_name` from the trait's default, which
  ignores case. `Glyphs::from_name` matches case exactly, so the impl does
  not delegate to it.
- `LineNumbers` derives `Default`, with `None` as the default, because a
  leaf needs a default.
- The variant names do not change, so option files that fh wrote before
  this change still load. Tests load fh-shaped files with
  `glyphs = "full"` and `line_numbers = "relative"`. They also check the
  schema names, the labels, the defaults, and round trips through a value
  and a file.
- fh: `GlyphSet` and `LineNumbering` go, with their `From` impls, the
  `GlyphSet` re-export in `fh/src/lib.rs`, and the `LineNumbering` matches
  in `canopy-fileselect/src/lib.rs`. The fields take `Glyphs` and
  `LineNumbers`, and `context.set_glyphs(options.display.glyphs)` needs no
  conversion.

### C3: A standalone follower

```rust
impl<G: OptionGroup + Send + Sync> Follower<G> {
    pub fn standalone() -> Self;
}
```

The follower follows a store of its own, which has no files. Each call
makes a separate store. A widget uses it until a host gives it a handle.
Tests check the default values and that two standalone followers do not
share changes.

- fh: `canopy_fileselect::options::own_options` and the commander's
  inline store use it. `own_options` goes.

### C4: The todo example, and documents

The todo example (`examples/todo`) declares a small group of options:

- `list.order`: a choice of oldest first or newest first. Today the store
  reads in id order (`store.rs`), so the newest-first order reverses the
  list. A change of order keeps the selected todo, by its id.
- `list.wrap`: a bool. On, an entry wraps as it does now. Off, an entry
  measures one row and shows one line, cut at the width. A change updates
  every entry and lays the list out again.

The todo wires the options as fh does:

- The app builder calls `Options::register`, and the app wraps itself in
  `Options`.
- `Todo` follows the group with a `Follower`. It builds its entries from
  the follower's values in `on_mount`, so a loaded file sets the first
  frame. It subscribes once, after that.
- The global file is `options.toml` in the config home. A run without a
  config home, such as headless MCP, keeps no files. The todo has no local
  files, because it has no project directory.
- The default configuration binds `o` to `options.open()`.

Tests and documents:

- A smoke suite `examples/todo/smoke/with_items/options.luau` runs with the
  item fixture. It sets each option with `options.set` and checks the
  list, with an item long enough to wrap. It opens and closes the panel,
  and it checks `options.source`.
- Native tests load `options.toml` from a temporary `ConfigHome`, and check
  that no config home gives no files. They check the order and the kept
  selection, a wrap change on existing entries, and the `o` binding.
- `docs/options.md` describes how an app uses options:
  - the derives and their attributes, and the leaf kinds
  - the layers and the option files
  - handles and narrowing
  - the follower pattern
  - the `Options` widget, its commands, and the panel

  The crate docs in `canopy-options/src/lib.rs` point to it.
- `AGENTS.md` names the todo as the example of options.
- The workspace warning about the unused `canopy-options` entry ends,
  because the todo uses it.

### Not in this plan

- A one-call install that does both `Options::register` and the wrap in
  `Options::new`. It saves one line in each app.
- Typed group paths from the derive. These would replace strings such as
  the name in `narrow("", …)` and the paths in `revert("listing")` and
  `live_at("listing")`. This needs a design of its own.
- A `Layout` built from canopy-mcp's `ConfigHome`. fh's `options::load` is
  ten lines, and it needs fh's repository root.

## Execution Plan

fh depends on Canopy by path. A stage that changes both commits Canopy
first. Stages 1 and 3 change both repositories. Stages 2 and 4 change only
Canopy.

### Stage 1: Canopy's enums and the standalone follower

- [x] C2: the impls for `Glyphs` and `LineNumbers`, the `Glyphs` order, and
  `Default` for `LineNumbers`, with their tests.
- [x] C3: `Follower::standalone`, with its tests.
- [x] Run the Canopy tests and `ncode api`. Commit Canopy.
- [x] fh: replace `GlyphSet`, `LineNumbering`, and `own_options`. Run fh's
  tests and `ncode api`, and check that the copies are gone from the API.
  Commit fh.

### Stage 2: Deferred widget calls in the runtime

- [x] C1: `WidgetStamp`, `Delivery`, `widget_stamp`, the `Context` method,
  the `CompletionRequest` variant, and
  `ContextExt::with_widget_mut_or_post`.
- [x] Core tests:
  - [x] A free widget runs the call at once. A busy widget runs it as the
    dispatch completes, in the event that it was queued in.
  - [x] A type mismatch fails at once. A removed or replaced widget gives
    `Expired` at admission, and drops a queued call.
  - [x] A failed dispatch discards the calls it queued.
  - [x] A call queued at mount runs, through `replace_root`, the render
    sweep, and a widget callback. A failed mount discards it.
  - [x] A sealed batch refuses the call. Mixed and recursive calls hit
    the limits.
  - [x] A failure is a notice at an input boundary, and an error from
    `with_context`.
- [x] Run the Canopy tests, `cargo xtask smoke`, and `ncode api`. Commit.

### Stage 3: The follower uses deferred calls

- [x] C1: `Follower::follow` takes the stamp and uses
  `with_widget_mut_or_post`. Remove `take_pending` and the pending flag.
  A deferred apply's failure is an `options:` notice.
- [x] Tests in `canopy-options`:
  - [x] A widget's command writes an option through another handle, and
    the widget follows that option. The widget shows the change after the
    dispatch, with no poll.
  - [x] Two writes in one dispatch apply once, through `take_change`.
  - [x] A removed or replaced widget ignores changes, with no notice.
  - [x] A deferred apply's failure is a notice, and other queued work
    still runs.
  - [x] The panel close still polls the widget.
- [x] Run the Canopy tests, `cargo xtask smoke`, and `ncode api`. Commit
  Canopy.
- [x] fh: remove the pending checks from `Commander::poll` and
  `FileSelect`, and remove `poll_options`. Run fh's tests, smoke suites,
  and `ncode api`. Commit fh.

### Stage 4: The todo example and documents

- [x] C4: the todo's options, the registration, the `Options` wrapper, the
  follower, the `o` binding, the native tests, and the smoke suite.
- [x] `docs/options.md`, the crate docs, and `AGENTS.md`.
- [x] Run `ncode tidy`, `ncode test`, `cargo xtask smoke`, and `ncode api`
  in Canopy. Commit.
- [x] Run `ncode tidy` and `ncode test` in both repositories on the final
  state.
