# Canopy Structural Review

## Description

This review combines four passes over Canopy and its main consumer, fh:

- **Deslop**: structural simplifications.
- **Clear wins**: work that can be done without further decisions.
- **API review**: a review of the `api/` captures in both repositories.
- **Conceptual pass**: terminology and structure.

Eight area surveys read the source: six in Canopy and two in fh. Every
high-impact claim was then checked against the code.

**Status.** The owner accepted every recommendation on 2026-09-23. This is a
clean break: no compatibility shims, aliases, deprecations, or migration
paths. Canopy (at `a4d81897`) and fh (at `fac382f`) migrate together, stage by
stage. fh covers three crates: the fh app, the `canopy-fileselect` widget
crate, and `fh-git`.

Changes C39 to C51 come from the fh pass and are marked **New**. They follow
the same acceptance, but the owner can veto any of them individually. The fh
pass also adjusted several earlier changes; each adjustment is marked
"Adjusted (fh)" in its change.

### Assessment

The core model is sound, and its documentation is unusually strong:

- A retained arena tree with read and write contexts.
- One turn driver.
- One route resolver shared by dispatch, discovery, analysis, and help.
- A Luau surface generated from command metadata.

`docs/architecture.md` states real contracts, and the code mostly keeps them.

Ten days of fast growth (about 30,000 inserted lines since `c8da09f4`) have
left eight kinds of debt:

1. **Vocabulary collisions.** Several words carry several meanings:
   - "action" has 9 meanings.
   - "scope" has 7.
   - "slot" has 4, and "key" has 5.
   - "render" has about 9.
   - "work" has 3, and "semantic" has 2.
   - fh adds more: "hidden" for dotfiles, "publish" for pane sync, and
     "prepare" for a precomputed diff.

   Some concepts also have two names, such as `RoutePhase::PreEventBinding`
   versus `BindingPhase::BeforeWidget`. These are the main source of lost
   clarity.
2. **Parallel mechanisms for one concept.**
   - Three command-call types.
   - Two checklist implementations.
   - Two notions of hidden.
   - Four ways to build children.
   - Five copies of scroll commands.
   - Seven hand-written modal-bounded route walks, which already disagree.
   - Two ways for widgets to ship bindings: Luau default-binding scripts and
     Rust framework groups.
3. **Surfaces grew by doubling.**
   - `Context` has 70 methods, and `ViewContext` has 45.
   - Both passed the thresholds in `docs/api-budget.md`, and each crossing was
     justified after the fact.
   - The main cause is the self and `_of` method pairs.
4. **Setup and runtime are blurred.** Seven `Canopy` registration methods fail
   at runtime once the API is finalized. `Canopy::new` is a second,
   testing-only copy of `CanopyBuilder`.
5. **"Legacy" surfaces in an unpublished project.**
   - `canopy.cmd` and `canopy.cmd_on`, with their table-as-named-arguments
     rule.
   - `canopy.screen`.
6. **Divergent widget conventions.**
   - There are four styling patterns, and style paths leak into consumers.
   - Several generic names sit at the crate root.
7. **Documentation drift.**
   - The docs describe testing-only methods as app API.
   - The `getting-started.md` code blocks no longer match `examples/hello`.
   - `docs/api-budget.md` has become a changelog of after-the-fact
     justifications, and its thresholds constrain nothing (C38 retires it).
8. **Missing capabilities at the consumer boundary.** fh re-implements what
   Canopy should provide:
   - A complete text field: canopy-fileselect wraps `Input` three times.
   - Dialog framing and modal keymaps: fh rebuilds the Confirm and Picker
     keymaps in about 130 lines.
   - Recoverable command failures: a failing command bound to a key ends the
     app, so fh turns every error into a footer message by hand.
   - Wake-on-send workers: three fh workers poll their channels every 50 ms.
   - A config-home helper.
   - A key-label lookup: status hints type "ctrl-g" beside the real binding.

### Verified defects

These were confirmed by reading the code. No reproduction tests were run.

- Under an `Application` modal, a transient mode cuts off default-tier bindings
  and never pops (C1).
- A cancelled script reports kind `canopy` through the Luau error payload
  (C2).
- `Confirm::set_default_answer` takes a type that callers cannot name (C3).
- Selector measures checkbox glyphs as a fixed 4 columns (C3).
- DiffView has no theme paths in any built-in theme (C3).
- `Columns` can focus a node that refuses focus (C3).
- In fh, the style `fh/diff/loading` has no rule, and DiffPane's six
  `#[command]` methods are never registered (C50).
- In canopy-fileselect, the columns dialog does not swallow clicks on its
  margin, unlike Confirm and Picker (C32).

### Evidence and coverage

- **Captures:** `ncode api --check` passed in Canopy; all eight captures are
  current. fh's captures were read but not refreshed.
- **What the captures omit:**
  - Canopy's `testing` feature: `Canopy::new`, `finalize_api`,
    `require_startup_global`, `invalidate_script_modules`, and nine more
    methods, plus the whole `canopy::testing` module.
  - Crates with no library target: canopyctl and both xtasks.
- **Use counts:** these come from `rg` over both repositories. They are
  approximate to about ±1 and are not compiler-checked.
- **Real consumers:** fh usage counts as evidence that an API is needed. Several
  items that looked unused inside Canopy stay for that reason:
  - `ModalBindings::FrameworkWithActions`
  - List checks
  - most Picker methods
  - most DiffView constructors
  - `wake_handle`
  - `bind_framework`
- **Not run:** no builds, tests, or runtime checks were run for this review.
  Performance statements are from code structure only; C18 measures before it
  acts.
- **Out of scope:** `ruau` and `fh-git` internals were not reviewed; fh-git does
  not depend on Canopy.

### Resolved decisions

| Change | Resolution |
| --- | --- |
| C1 | Transient modes work inside `Application` modals. |
| C5 | A typed `Setup` phase; `Loader` becomes `Register`. Runtime restyling stays. |
| C8, C10 | "Tier" and "intent" become the binding vocabulary; the clear intent is `canopy.clear`. |
| C12 | `canopy::input` absorbs `event`. |
| C13 | The node-addressed context rule. |
| C14 | Drop `compose`; `KeyedChildren` moves into canopy-widgets. |
| C17 | One `ScrollOp` enum with counted lines and pages. |
| C19, C20 | render, prepare, emit, and flush each keep one meaning; `TurnInput`, `PollLifetime`. |
| C26, C28 | The trimmed Luau lookup set, and every wire rename. |
| C30 | List checks survive; Selector returns to single choice. |
| C31 | Shared part roles, one `style::themes` module, and palette-driven widget styles. |
| C34 | Core navigation intents cover cursor and view movement. |
| C36 | Gyms move to `examples/gyms`, and fh's config model becomes Canopy's helper (C41). |
| C38 | Retire `docs/api-budget.md`, and move its lasting rules to `architecture.md`. |

## Vocabulary

This table is the target vocabulary. Each term keeps one meaning. The renames
live in the changes named in the last column.

| Term | One meaning | Replaces or retires | Change |
| --- | --- | --- | --- |
| node | An arena entry holding one widget plus its tree, layout, and view state | "Core arena" in public docs | C4 |
| slot | A named, unique child position (`ChildSlot`, `slot!`) | `child_keys`, `keyed()`, `DuplicateChildKey` | C4 |
| widget cell | The storage for a node's boxed widget, taken out during mutable callbacks | `WidgetSlotGuard`, `WidgetSlotPolicy` | C4 |
| identity | An application key that is unique within a scope subtree (`NodeIdentity`) | "semantic key", `SemanticIdentity`, `set_semantic_key` | C4 |
| semantics | A widget's published observations: role, label, value (`WidgetSemantics`) | `ChangeSet.observation` | C4, C18 |
| scope | Only a subtree that bounds an operation (`FocusScope`, identity scope) | `BindingScope`, "modal scope", `CommandScopeFrame` | C7, C8, C10 |
| key | A keyboard key, or an item key in a keyed collection | slot names, identity keys | C4 |
| hidden | A node and its subtree are out of layout, render, hit testing, and focus. This is the only such notion. | `Display::None`, `Layout::hidden()`; fileselect "hidden" for dot entries (becomes "dotfiles") | C15, C50 |
| base layout, layout override, effective layout | `Widget::layout()`; a parent's persistent per-field settings; the override applied to the base | `set_layout`, `with_layout`, which silently pin fields | C15 |
| viewport | The part of a canvas shown in a content box | snapshot "viewport", which is really the screen size | C4 |
| scroll, reveal | An immediate viewport move; a deferred request to show an area, anchor, or node | none | C17 |
| event | One input occurrence (`Event`) | none | C12 |
| input spec | The key or mouse pattern that a binding matches (`InputSpec`) | none | C12 |
| binding action | What a binding runs: a command call, a script function, or an intent (the Luau `action` field) | `BindingTarget`, `BindingTargetKind` | C10 |
| tier | A binding's resolution layer, in order: framework group, then global, then modes (newest first), then default | `BindingScope`, `BindingOwner`, Luau `scope` output | C8 |
| framework group | Framework-owned bindings that only a modal admits | "exclusive group" | C8 |
| mode | An entry on the mode stack. A transient mode takes one key. | "input mode" (`push_input_mode`, `input_mode()`) | C11 |
| phase | `before_widget` or `after_widget`: when a binding runs relative to its node's widget | `RoutePhase` (becomes trace kind) | C9 |
| route, route trace, route explanation | The walk from focus or the hit node toward the root, bounded by the modal owner; what dispatch did; what dispatch would do | `canopy::help`, `canopy::keyroute` placement | C9, C12 |
| intent | A registered name that the route offers to widgets. The first widget that accepts it consumes the key; otherwise it falls through. Examples: `canopy.clear` and the navigation intents. | "widget action", `WidgetActionName`, `accepts_action`, `canopy.text.clear` | C10, C34 |
| command | A typed operation that a widget type owns (`owner::name`), resolved by owner name | none | C6 |
| command call | A command id, its arguments, and an optional target. This is the only such type. | `CommandAction`, `CommandInvocation` | C6 |
| origin | The node a call dispatches from when it has no explicit target: the root for evaluations, the route node for bindings | script "anchor", `EvalRequest.anchor` | C4, C27 |
| activation | A widget's primary command (Button press, List activate) | `WidgetSemantics::action_status`, `Confirm::set_actions` | C10 |
| default action | The runtime response to input that nothing handles (wheel scroll) | none | none |
| modal | An open modal region with an owner, initial focus, dimming, and admission | `InteractionToken`, `world/interaction.rs` | C10 |
| setup | Registration before finalization: commands, bindings, intents, fixtures, styles | `Canopy::register_*` at runtime, `Loader` | C5 |
| turn, turn input | One bounded driver step; what drives it | `Work` | C20 |
| poll, poll lifetime, wake | A widget's scheduled or woken callback; how long scheduling lasts; a request for one poll | `WorkLifetime`, `WorkStamp` | C20 |
| invalidate | Record pending frame work: layout includes paint, and paint includes snapshot | public `ChangeSet`, `render_pending` | C18 |
| prepare, render, publish, emit, flush | prepare: build a frame. render: widget painting only. publish: make a snapshot current. emit: write to a backend. flush: backend bytes only. | `Canopy::flush`, `pre_render`, `post_render`, `TermBuf::render`; `PreparedDiff` (becomes `DiffModel`); fileselect `publish` (becomes `sync_panes`) | C19, C46, C50 |
| frame buffer, snapshot | `TermBuf`; the immutable published `FrameSnapshot` that owns its buffer | `Canopy::buf()`, `FrameSnapshot.cells` | C19 |
| evaluation, source | One top-level Luau run (`EvalId`); the Luau text it runs | MCP `script` field | C28 |
| script journal, replay file | The in-app bounded history of runs; a `canopy.replay/1` envelope | "journal" for replay files | C28 |
| outcome, report | The runtime result (`EvalOutcome`); the serialized automation result | `ScriptEvalOutcome` | C28 |
| reset policy, fixture | A declared domain-state lifetime (isolated or external); a named setup function, reported separately | `ResetPolicy::Fixture` | C28 |
| selection, checked, focus | The cursor row a collection's navigation moves; membership in a multi-mark set; keyboard focus (core only) | Selector `selected` for checked and `focused` for the cursor; `selected_keys` reporting checks | C30 |
| text width | The terminal columns a string occupies (`canopy::text::width`) | direct `UnicodeWidthStr::width`, and "columns" for width | C32 |
| notice | A recoverable failure reported to the user and to scripts, while the app keeps running | fatal binding errors, hand-built footer error messages | C39 |
| config home | The directory holding a user's `init.luau`. It is mounted only when `init.luau` exists, and never written unless the app opts in. | per-app home resolution, `ensure_user_config` | C41 |
| active, pressed, highlight, mark | Takes the keyboard; a button being pressed; syntax colouring; a position annotation | button layer `active`, `highlight_matches` | C31, C33 |
| border, frame, dialog | Plain box chrome; a titled border that owns scrollbars; a framed question or panel | module `boxed`, node `box` | C32 |
| layer, part role | A pushed component name, equal to the node name; a bare painted part (`text`, `border`) | per-widget role aliases | C31 |

## Changes

Each change is tagged with its kind and stage:

- **Clear win**: can be implemented unattended.
- **Decided**: needed an owner decision, now resolved.
- **Defect**: a verified bug.
- **New**: added by the fh pass.

"Adjusted (fh)" marks a design change that fh evidence required.

The ten items in the deslop batch are marked with ★: C6, C7, C9, C13, C15,
C16, C18, C24, C25, C32.

### Defects

#### C1: Transient mode inside an application modal

*Defect. Stage 1.*

Code disagrees on when a transient mode is active:

- Routing and analysis treat it as active only when no modal is open:
  `routing.rs:437`, `keyroute.rs:141`.
- The resolver treats it as active unless a framework group is active:
  `inputmap/mod.rs:682-699`, the `registry_status` checks near `:803`,
  `help.rs:161`, and `help/mode.rs:117`.

Under an `Application` modal, such as the todo adder or a Picker overlay, a
pushed transient mode causes three problems:

- The resolver stops at the mode, so default-tier bindings are unreachable.
- No key pops the mode, because routing takes the normal path.
- The mode panel still shows the mode as waiting.

There is a related duplication: seven hand-written modal-bounded route walks
use different bounds (`keyroute.rs:180,275,446,481`, `help.rs:189`,
`routing.rs:349,475`). `route_transient_key` ignores the modal owner and
admission; `explain_transient_key` checks them.

**Proposal:**

- Add `Core::effective_transient_mode()`, returning a mode unless a framework
  group is active.
- Add one `Core::route(start)` iterator that yields `(NodeId, Path)` and stops
  at the modal owner.
- Use both in routing, analysis, discovery, help, and the mode panel.
- Add a test that pushes a transient mode inside an `Application` modal.

**Decided:** transient modes work inside `Application` modals, as the
resolver already assumes. A framework-group modal suspends them.

#### C2: One script error-kind mapping

*Defect; clear win. Stage 1.*

`From<&Error> for CanopyErrorPayload` ends in `_ => Canopy`
(`script/errors.rs:116`). As a result, `Error::ScriptCancelled` reaches Luau as
kind `canopy`, while MCP reclassifies it separately as `ScriptCancelled`
(`canopy-mcp/src/script.rs:907-941`). Timeout and cancellation each have two
representations: a typed variant, and `ScriptStructured { kind }`. `ScriptBusy`
is built as a structured error in eight places.

**Proposal:**

- Add one exhaustive `Error::script_kind()`, with no wildcard. Both the Luau
  payload and MCP use it.
- Add a typed `Error::ScriptBusy`.
- Remove the `script_structured(NodeDetached)` duplicate
  (`script/invocation.rs:258-262`).

#### C3: Small verified defects and contract gaps

*Defect; clear wins except where noted. Stage 1.*

- **`Confirm::set_default_answer(Answer)` cannot be called outside the crate.**
  `Answer` is `pub` in a private module and is not re-exported
  (`canopy-widgets/src/lib.rs:79`). Re-export it.
- **Selector mis-measures custom glyphs.** `Selector::content_size` adds a fixed
  `+ 4` for the checkbox and ignores `with_glyphs` widths
  (`selector.rs:203-212`). Measure the glyphs.
- **DiffView has no theme coverage.** No built-in theme defines a `diff/*` path
  (`style/palette.rs`, `themes.golden`), so added and removed rows render in
  default colours. Derive them from palette roles (C31).
- **`BindingTargetKind::of` is unusable from outside.** It is `pub`, but its
  argument type `BindingTarget` cannot be named outside the crate
  (`api/canopy.rs:4275`). Make it `pub(crate)`.
- **Two widgets can focus a node that refuses focus.** `Columns`
  (`columns.rs:86-90`) and listgym fall back to `first_leaf`, and `set_focus`
  does not check acceptance (`world/focus.rs:35-59`). The next layout then
  moves focus to another pane. Use `focus_first(FocusScope::Node(pane))`.
- **The `clear_bindings` doc is wrong.** It says "Remove every binding from
  every mode" (`base_api.rs:530`). The function removes application bindings
  only, and it also clears the mode stack. Fix the doc.
- **`set_children` silently detaches omitted children** and leaves them in the
  arena (`tree.rs:913-918`). Reject omissions: a caller that wants a child out
  detaches or removes it explicitly.
- **`pre_render` checks only `hidden`** (`rendering.rs:86`). A `Display::None`
  subtree is still mounted, polled, and counted as focus-seen. C15 removes the
  second notion.

### Conceptual model

#### C4: Terminology renames that no other change covers

*Clear win. Stage 4.*

- **Slot means a named child position only.**
  - `Node::child_keys` becomes `slots`.
  - `Error::DuplicateChildKey` becomes `DuplicateSlot`. (`ChildBuilder` and
    `ChildConfig`, which report the same condition as `Invalid`, go with C14.)
  - Widget storage becomes the widget cell: `WidgetSlotGuard` becomes
    `WidgetCellGuard`, and `scroll::Slot` becomes `RevealScope`.
- **Identity gets one word.**
  - `set_semantic_key` becomes `set_identity`, and `clear_semantic_key`
    becomes `clear_identity`.
  - `SemanticIdentity` becomes `NodeIdentity`, and
    `NodeSnapshot.semantic_identity` becomes `identity`.
  - "Semantics" then means only `WidgetSemantics`.
- **The script dispatch node becomes "origin".** Replace the script "anchor"
  with "origin". Command docs already say "route origin", and this frees
  "anchor" for `reveal_anchor`.
- **"Viewport" means the scroll window only.** `FrameSnapshot.viewport` becomes
  `size`. "Screen size" replaces "root size" in `set_root_size`. The MCP
  `viewport` wire field becomes `screen` (C28).
- **Stale docs:** fix the `Widget` and `NodeId` docs that cite the private
  "Core arena", and the `Loader` doc ("recursive initialization").

#### C5: Setup, then run

*Decided. Stage 3.*

Registration only works before API finalization. There are seven checks:
`ensure_api_unfinalized` in `core/canopy/mod.rs` at 620, 650, 792, 823, 1045
and 1152, plus a builder reset at `builder.rs:138-140`. Yet the registration
methods live on `Canopy` and fail at runtime.

`Canopy::new` duplicates `CanopyBuilder` for tests. It also brings five
`_inner` wrapper pairs, and `HarnessBuilder::build` re-implements the builder
sequence (`testing/harness.rs:79-103`).

`Loader` compounds the problem:

- Its doc is wrong: "recursive initialization of themselves and their
  children".
- It has 57 in-workspace impls plus 3 in fh, and 13 are empty. They exist only
  because `Harness` requires `W: Loader`.

**Proposal:**

- `CanopyBuilder::configure(|setup: &mut Setup| ...)` passes a setup handle.
  It owns these methods:
  - `add_commands`, `register_default_bindings`, `register_fixture`, and
    `register_startup_script`
  - intent registration (C10)
  - `register_mode_hook`
  - `bind` (C11)
  - `set_render_limits`
  - the initial theme and widget style layers (C31)
- Rename `Loader::load(&mut Canopy)` to `Register::register(&mut Setup)`.
- Delete `ensure_api_unfinalized`, `is_api_finalized`, and the MCP guard at
  `canopy-mcp/src/metadata.rs:128`.
- Retire `Canopy::new`. Tests build through `CanopyBuilder`, and `Harness`
  wraps it.
- Drop the `W: Loader` bound and the empty impls.
- **Adjusted (fh):**
  - Styles stay runtime state: themes switch at runtime through
    `Context::set_style` and `Canopy::style_mut`, and 22 fileselect test sites
    restyle after build.
  - `Harness::from_canopy` stays, because it wraps an app's own builder; fh
    uses it 10 times.
  - Add `HarnessBuilder::register::<W>()`. It replaces the implicit
    registration that 51 `FileSelect` harnesses get from the `W: Loader`
    bound today.

**Cost:**

- About 60 `Register` impls change signature, mechanically.
- About 120 `Canopy::new()` test sites move to a builder helper.

**Decided:** adopt the `Setup` handle, and rename `Loader` to `Register`.

#### C6: One command-call type ★

*Clear win. Stage 3.*

Three types describe one idea:

- `CommandCall` stores a `&'static CommandSpec` but reads only its id
  (`commands.rs:1087,1112`).
- `CommandAction` holds an invocation plus an optional target (`:817`).
- `CommandInvocation` holds an id plus arguments (`:685`).

Every boundary converts immediately:

- `CommandCall::action()` is called at `canopy/mod.rs:1000,1013`,
  `button.rs:117`, and `list.rs:262`.
- There are 48 `.invocation()` sites that then pass a target separately.
- `target.unwrap_or(CommandTarget::From(node))` is repeated six times.

The Luau userdata named `CommandCall` already wraps the Rust `CommandAction`
(`script/bridge.rs:183`). fh uses none of these conversions.

**Proposal:**

- Keep one type: `CommandCall { id, args, target: Option<CommandTarget> }`,
  with `with_target` and `target_or(origin)`.
- `InvokeFn` takes `&CommandArgs`.
- `Context::dispatch`, `ContextExt::dispatch_exact`, and
  `ViewContext::command_status` take `&CommandCall`. An omitted target means
  From the current node.
- Delete `CommandAction`, `CommandInvocation`, `.action()`, and
  `.invocation()`.
- Rename `BindingCommand.action` to `call`.

#### C7: Take list rows out of core dispatch ★

*Clear win. Stage 3.*

These items encode one canopy-widgets concept in core, and only tests consume
them:

- `ListRowContext`
- `CommandRequirement::ListRow`
- `CommandScopeFrame.list_row`
- `Context::current_list_row`
- `Context::dispatch_scoped`
- the derive's hard-coded injection list (`canopy-derive/src/parse.rs:143`)
- two Luau literal types (`script/defs.rs:378,818`)

The tests are `list.rs:1354`, `canopy-widgets/tests/command_targets.rs:49`,
and a derive test. List already appends the row index as a user argument
(`list.rs:43`). fh uses none of these.

**Proposal:**

- Delete the list-row injection, `dispatch_scoped`, and `current_list_row`.
- Make `CommandScopeFrame` a private stack of `Option<Event>`.
- Make `current_mouse_event` a provided method derived from `current_event`.
- Seal `Inject`: the derive selects injectable parameters by type name, so no
  outside implementation can work (`parse.rs:131-145`).

`Context` loses 2 to 3 methods.

#### C8: Binding tiers replace scope and owner

*Decided. Stage 3.*

Today the invariant "owner is `Framework(g)` exactly when scope is
`Exclusive(g)`" is built at registration (`inputmap/mod.rs:515-519`) and then
checked on every read (`:714`, `:737`, `:754`). `bind_framework` takes the
group twice (`root.rs:385-395`). Luau writes the concept as `tier = "global"`
but reads it back as `scope`.

"Exclusive" is also wrong for `FrameworkWithActions`, which admits application
intents.

**Proposal:**

- Replace both types with one enum whose variant order is the actual
  resolution order, and delete `BindingOwner`:

  ```rust
  enum BindingTier {
      Framework(FrameworkBindingGroup),
      Global,
      Mode(String),
      Default,
  }
  ```

- Rename "exclusive group" to "framework group" everywhere:
  - `BindingSnapshot.exclusive_group`
  - `RegistryStatus::{BlockedByExclusive, InactiveExclusive}`
  - the Luau output field `scope`, which becomes `tier`
- Stop `candidates()` from cloning the mode name on every query
  (`inputmap/mod.rs:690`).

This touches about 150 sites. fh has 3 `bind_framework` calls (2 in the
commander, 1 in canopy-fileselect) and 8 Luau reads of `exclusive_group`.

#### C9: Phase, route outcome, and trace carry their state explicitly ★

*Clear win. Stage 3.*

**Phase.** `Option<BindingPhase>` is used to signal "this is a widget action".
It appears in five records, and three `debug_assert!(false)` arms exist only
because impossible combinations can be typed (`keyroute.rs:213`,
`routing.rs:558,861`).

- Store the phase non-optionally. An intent is stored as `BeforeWidget`.
- `BindingOptions.phase` stays optional as user input, and an explicit
  `after_widget` on an intent is an error.
- `KeyRouteStep` gets one `binding: Option<StepBinding { id, kind, phase }>`
  in place of three coupled options.

**Route outcome.** `RouteOutcome` has eight variants, and five repeat
`{binding, node, path}`. Five-way or-patterns recur at `keyroute.rs:304,456,530`,
`records.rs:890`, and `routing.rs:867`. `KeyExpectation::matches` already
groups the outcomes into two classes. Replace them with:

```rust
RouteOutcome {
    Binding(RouteWinner),
    Transient(RouteWinner),
    Widget { node, path },
    TransientDismiss,
    Unhandled,
}

RouteWinner { binding, node, path, kind, phase }
```

**Trace kind.** Rename `RoutePhase` to `RouteTraceKind`:

- Use the binding vocabulary: `BeforeWidgetBinding`, `AfterWidgetBinding`,
  `OfferIntent`, `Widget`, `DefaultAction`, `Bubble`, `RunBinding`,
  `Handled`, `Unhandled`, `Start`.
- Use snake_case labels: `RoutePhase` labels are kebab-case today
  (`canopy/mod.rs:174`).
- Type the Luau field as a literal union.

Intent offers currently trace as `PreEventBinding` (`routing.rs:205,225`).

#### C10: Widget actions become intents, and "action" means what a binding runs

*Decided. Stage 4.*

The mechanism is sound and justified:

- A key such as `ctrl-x` "clear" reaches whichever widget on the route can
  consume it.
- A dormant intent falls through to the next candidate.
- A disabled command still consumes its key. Button relies on this
  (`routing.rs:585-590`).

Commands cannot express this without losing nominal dispatch. The mechanism
has one intent, `canopy.text.clear`, with five implementers:

- `Input` and `PickerList` in this workspace.
- Three widgets in fh `canopy-fileselect`.

fh also uses `ModalBindings::FrameworkWithActions` (`commander.rs:1170`).
Keep all of it.

The word "action" is the problem. It has nine meanings:

- `CommandAction`
- widget actions
- the Luau keymap `action` field
- `BindingTarget` docs
- `mouse::Action`
- `WidgetSemantics::action_status`
- Button docs
- `Confirm::set_actions`
- default action

**Proposal:**

- Widget actions become **intents**, following the Shortcuts, Intents, and
  Actions model:
  - `IntentName` and `IntentSpec`.
  - `Widget::accepts_intent` and `Widget::on_intent`.
  - `Setup::register_intent`.
  - `ModalBindings` becomes the admission type from C42:
    `Framework { groups, intents }`, which replaces both `Framework(group)`
    and `FrameworkWithActions`.
  - `RouteOutcome` kind `Intent`.
- **Adjusted (fh):**
  - The clear intent is `canopy.clear`, not `canopy.text.clear`.
    canopy-fileselect accepts it to reset find, search, order, columns, and
    dotfiles (`lib.rs:2069-2082`), which is not text.
  - Widgets register the intents they implement in their own `Register` impl.
    Today only the apps register the name (fh `lib.rs:113-118`, todo), while
    canopy-widgets defines and implements it. Registering an existing name
    again is idempotent.
- "Action" means only what a binding runs:
  - `BindingTarget` becomes `BindingAction`, and `BindingTargetKind` becomes
    `BindingActionKind`.
  - This frees "target" for `CommandTarget`.
- `WidgetSemantics::action_status` becomes `activation_status`.
- `Confirm::set_actions` becomes `set_commands`.
- Keep `mouse::Action` (namespaced) and "default action" (a standard term).
- Rename the modal types:
  - `InteractionToken` becomes `ModalToken`.
  - `world/interaction.rs` becomes `world/modal.rs`.
  - `InteractionState` becomes `ModalStack`, and `interaction_admits` becomes
    `modal_admits`.
  - Docs say "modal", not "modal scope".
- Intents also replace the per-owner scroll and page commands that five
  widgets copy today (C34).

This touches fh in about 30 places, all mechanical: 3 widget impls in
canopy-fileselect, the fh registration, `FrameworkWithActions` at
`commander.rs:1170`, and 3 `InteractionToken` sites.

#### C11: One binding entry point and one mode vocabulary

*Decided. Stage 3.*

**Entry points.**

- `Canopy::bind_command` and `bind_widget_action` have identical bodies
  (`canopy/mod.rs:1005-1039`) and no non-test consumers anywhere.
- Their Luau twins are also identical (`base_api.rs:1024-1057`).

Replace them with one `Setup::bind(input, options, BindingAction)`. With C8,
`bind_framework` becomes the Framework-tier case of the same call.

**Modes.** Three spellings of mode naming coexist:

- Rust: `input_mode`, `set_input_mode`, `push_input_mode`,
  `push_transient_input_mode`, `pop_input_mode`.
- Luau: `canopy.input_mode()` next to `set_mode`, `push_mode`, and
  `pop_mode`.
- `InputMap`: `current_mode`.

Use `mode`, `set_mode`, `push_mode`, `push_transient_mode`, and `pop_mode`
everywhere. The Rust methods have one outside consumer (canopyctl). fh uses
only the Luau getter `canopy.input_mode()`, at 27 sites in its smoke scripts
and tests.

**Luau.** `canopy.keymap` covers `canopy.bind_mouse`, which has 2 shipped uses
in stylegym. Delete `bind_mouse` and move those uses to `canopy.keymap`. Keep
`canopy.bind` for one key binding; it has about 86 uses.

#### C12: A module map for the `canopy` crate

*Decided. Stage 4.*

`architecture.md` says the root holds facade traits and handle types. In
practice the root re-exports about 55 items, including:

- eval, journal, and trust types
- 12 binding types
- frame, snapshot, and wake types
- turn inputs and invalidation

Binding discovery sits in `canopy::help`, which shares its name with the
`canopy_widgets::help` widget module and has a stale doc. Route analysis sits
in `canopy::keyroute`, which has no doc.

The API capture prints 55 private paths such as `crate::core::inputmap::BindingId`.

**Proposal:**

| Module | Holds |
| --- | --- |
| root | `Canopy`, `CanopyBuilder`, `Setup`, `Widget`, `Context`, `ContextExt`, `ViewContext`, `ViewContextExt`, `Register`, `NodeId`, `TypedId`, `NodeName`, `EventOutcome`, `ChangeOutcome`, the derive macros, `slot!`, `rgb!` |
| `canopy::input` | Today's `event` module (`Event`, `key`, `mouse`); `InputSpec`; binding registry types (C8, C9, C10); modal types; discovery (today's `help`); analysis (today's `keyroute`); trace |
| `canopy::tree` | `ChildSlot`, `NodeIdentity`, `FocusDirection`, `FocusScope` |
| `canopy::layout` | Adds `View`, `RevealAlign`, `ScrollAxis`, `ScrollMark`, and `ScrollOp` (C17) |
| `canopy::script` | Adds `EvalId`, `EvalRequest`, `EvalTicket`, `EvalOutcome`, `AutomationHandle`, `AutomationCallback`, `ScriptJournalEntry`, `ScriptOrigin`, `ScriptTrust`, `Fixture`, `FixtureInfo` |
| `canopy::runtime` | `TurnInput`, `TurnOutcome`, `FrameId`, `FrameSnapshot`, `NodeSnapshot`, `WidgetSemantics`, `NodeWakeHandle`, `WakeOutcome`, `PollLifetime`, `wake_channel` and `WakeSender` (C43), and `Notice` (C39) |
| `canopy::render` | `Render`, `RenderBackend`, `NopBackend`, `TermBuf`, `Cell`, `RenderLimits`, and `cursor` |

The following are clear wins with no layout change:

- **Fix the capture leaks.** In the files that define public items, import
  public types by their public path. Add a check that fails when `api/` shows
  `crate::core::`.
- Move `NodeName` into `path.rs`, and delete `core/state.rs`.
- Rename `widget/mod.rs` to `widget.rs`.

**Decided:** `canopy::input` absorbs `event`, so every input concept has one
home. There is no `canopy::event` module afterward.

Flattening `core/` is rejected for now; see "Considered and rejected".

### Context and tree surface

#### C13: Node-addressed context API ★

*Decided; mechanical. Stage 3.*

Of 15 self and `_of` pairs, the node form has more uses in 9, and it can always
express the self form. `list.rs:348` already writes
`is_on_focus_path_of(ctx.node_id())`. Odd singletons remain:

- `is_attached_of`
- `set_layout_override_of`
- `scroll_outcome_of`
- `add_child_to`
- `add_slot_to`

**Rule:**

- Every operation names its node.
- Self forms remain only for:
  - `node_id()`
  - `view()`, which has 54 uses in render code
  - `is_focused()`
  - operations on the caller's own viewport and input: scroll, reveal, mouse
    capture, `wake_handle`, and `request_poll` (C43)
- Noun queries keep `_of`: `view_of`, `layout_of`, `children_of`, `parent_of`,
  `path_of`.
- Predicates and verbs drop the suffix and take the node first:
  - `is_attached(node)`, `is_on_focus_path(node)`
  - `set_hidden(node, bool)`, `set_children(parent, ..)`
  - `add_child(parent, W)`, `add_slot::<K>(parent, W)`
  - `get_slot::<K>(parent)`, `with_slot::<K>(parent, f)`

**Removals:**

- `ViewContext`: `layout()`, `children()`, the self `child_slot`, the self
  `is_on_focus_path`, and `is_focused_of`.
- `Context`: the self `set_hidden`, `with_layout`, and the self
  `set_children`.
- `ContextExt`: the self `set_layout`, `add_child`, `add_slot`, `get_slot`,
  and `get_or_create_slot`, plus `has_slot`.

`get_slot` returns `Result<TypedId>` with a `NotFound` that names `K::KEY`.
10 of its 11 non-test callers unwrap it immediately.

**Result:** combined with C14, C16, and C17:

- `Context` plus `ContextExt` go from 70 to about 47 methods.
- `ViewContext` plus `ViewContextExt` go from 45 to about 27.

**Cost:** about 150 sites in this workspace and about 50 in fh (41 in
canopy-fileselect, 7 in the fh crate), mostly sed-able.

**Migration note (fh):** replace `is_focused_of(owner)` with
`focused_node() == Some(owner)`, not `is_on_focus_path(owner)`. The focus-path
form is true while a descendant field holds focus, which breaks the listing's
focus emphasis (`listing.rs:557`).

#### C14: A minimal tree-construction set

*Decided. Stage 3.*

Today there are four idioms for building children:

1. Immediate add.
2. Build detached, then attach. `Root::install`, `Help::install`, and
   `ModeHelp::install` build trees by hand this way.
3. Build detached, then attach in a batch with `set_children_of`.
4. `compose` with `ChildBuilder` and `ChildConfig`: 2 sites in todo, and none
   in fh.

**Proposal:**

- `Context` keeps the object-safe core: `create_detached_boxed`, `attach`,
  `attach_slot`, `detach`, `set_children`, `remove_subtree`,
  `remove_after_dispatch`, and `edit_structure`.
- `ContextExt` keeps the sugar: `create_detached`, `add_child`, `add_slot`,
  and `get_or_create_slot`.
- Delete `add_child_to_boxed`, `add_child_to_slot_boxed`, and
  `Canopy::create_detached`.

**Decided:**

- Drop `compose`, `ChildBuilder`, `ChildConfig`, and `attach_composed`. Set
  identity after attaching.
- Move `KeyedChildren` into canopy-widgets as List's reconciler. It uses only
  public `Context` methods (`create_detached`, `edit_structure`,
  `remove_subtree`, `set_children_of`, `type_id_of`). Only List and a bench
  use it, and fh does not.

#### C15: One notion of hidden and one layout setter ★

*Clear win. Stage 3.*

**Hidden.** `Node::hidden` and `Display::None` are two states.

- Seven sites test `hidden || display == None`: `layout_driver/mod.rs` at
  141, 181, 683, and 956; `focus.rs:451`; `snapshot.rs:91`;
  `rendering.rs:169`.
- They already drift: `pre_render` checks only `hidden` (C3), and script
  records report only `hidden`.
- No widget returns `Display::None`. Only two example sites use
  `Layout::hidden()`, and fh uses neither.

Delete `Display`, `Layout::display`, and `Layout::hidden()`.

**Layout setters.** Four setters apply three persistence rules:

- `set_layout(_of)` pins every field (`context.rs:793`).
- `with_layout(_of)` pins only the fields the closure changed
  (`tree.rs:115-128`, `layout.rs:393-439`).
- `set_layout_override_of` replaces the override.

Six of the eight `with_layout` sites assign a whole layout, so they pin less
than the code suggests.

**Proposal:**

- Keep only `set_layout_override(node, LayoutOverride)`, plus
  `impl From<Layout> for LayoutOverride` for full pinning.
- Delete `set_layout`, `set_layout_of`, `with_layout`, `with_layout_of`, and
  `record_changes`.

fh has 5 affected sites. One needs care: `columns.rs:780` assigns a whole
layout inside `with_layout_of`, which today pins only the changed fields. Pin
only the fields it means to set.

#### C16: Typed lookups and focus helpers ★

*Clear win. Stage 3.*

There are 19 lookup methods, and most have 0 to 2 non-test uses. canopy-widgets
uses none of the type-based searches. The search scope varies: `*_in_tree`
starts at the root, `*_descendant` at the current node, and `first_leaf` at an
explicit root.

**Proposal:**

- Add `descendants::<W>(root) -> impl Iterator<Item = TypedId<W>>`. It replaces
  `all_in_tree`, `first_in_tree`, `descendants_of_type`, `focused_descendant`,
  `focused_or_first_descendant`, and `children_of_type`.
- `unique_descendant` takes a root. Keep `with_unique_descendant`, which tests
  use 33 times and fh uses too.
- Delete `unique_child`, `try_with_unique_descendant`, `find_one`, `preorder`,
  `locate`, `first_leaf`, and `focusable_leaves` (see the C3 focus fix).
- Rename `focused_leaf` to `focused_within(root)`. It returns any focused
  candidate, not a leaf.
- Make `find_nodes_matching` and `find_node_matching` crate-private; only the
  script host uses them.

#### C17: Scroll operations

*Decided. Stage 3.*

There are nine scroll methods. The same `FocusDirection`-to-`scroll_*` match
appears four times (`text.rs:113`, `diff_view.rs:305`, `list.rs:684`,
`examples/lib.rs:80`), and the same page match three times. Scroll commands also
borrow `FocusDirection`, whose `Next` and `Prev` alias `Down` and `Up`.

**Proposal:**

- Add a scroll operation enum, with a `ScrollDirection` `CommandEnum`:

  ```rust
  enum ScrollOp {
      To(Point),
      By(i32, i32),
      Lines(ScrollDirection, u32),
      Pages(ScrollDirection, u32),
  }
  ```

- Provide `Context::scroll(op)` and `Context::scroll_node(node, op)`, which
  replaces `scroll_to_of`.
- Provide `ViewContext::scroll_outcome(node, op)`.
- Keep the saturating `u32` page arithmetic and its test
  (`context.rs:597-606,1380`).
- **Adjusted (fh):**
  - Pages carry a count, because fh pages by signed counts.
  - Core owns one overlap policy: a page keeps one line of overlap. fh's
    listing uses that overlap (`lib.rs:543-549,394-396`), while Canopy's
    `page(delta)` uses only the sign and moves a full height.
  - `scroll_node` deletes fh's throwaway-callback workaround
    `with_preview_view` (`lib.rs:1804-1818`).

**Decided:** the `ScrollOp` enum. fh has about 20 affected sites, including a
fifth copy of the direction match (`diff_pane.rs:198-205`).

### Runtime and rendering

#### C18: Invalidation is internal ★

*Clear win. Stage 2 for the internals, Stage 3 for the public part.*

`Core::with_widget_dyn_mut` invalidates layout before every mutable callback
(`world/mod.rs:282-285`). As a result:

- `Context::invalidate_layout` is redundant. All 16 calls run inside a
  mutable callback: 7 in Canopy and 9 in canopy-fileselect. The fh pass
  checked each one.
- `ChangeSet.cursor` always equals `paint`, and `observation` is set on every
  invalidation (`change.rs:43-57`).
- Frame code reads only `.layout` and `is_pending()`
  (`rendering.rs:274,288`).
- `render_pending` is a second dirty flag set in 9 places.
- `ChangeSet` and `Invalidation` have no uses outside the crate.

**Proposal:**

- Make `ChangeSet` and `Invalidation` `pub(crate)`.
- Collapse the flags to one ordered level: `Semantics < Paint < Layout`.
- Replace `render_pending` with `invalidate(Paint)`.
- Delete `Context::invalidate_layout` and its 16 calls. Delete the whole
  block at canopy-fileselect `lib.rs:1772-1775`, which exists only to call it.
- Rename "observation" to "snapshot".
- **Invariant to document in `architecture.md`:** any mutable callback repaints
  the whole frame. canopy-fileselect relies on this: `FooterStatus` reads
  `FileSelect` during render, and `ListingColumn` reads another node's focus.
- **Measure first (fh):** every poll and mutable callback schedules a full
  layout. fh polls every 50 ms during Git work and every 250 ms while a
  terminal is open. Before narrowing the implicit level to Paint with an
  explicit layout escalation, measure the per-turn layout cost on fh's tree
  with the `core` bench. Change the default only if that cost is material.
  C43 already removes most of those polls.

#### C19: Frame pipeline vocabulary and buffer ownership

*Decided. Stage 4.*

"Render" means widget painting, a forced prepare-and-emit (`Canopy::render`),
a full emit (`TermBuf::render`), the output sink, a mount sweep
(`pre_render`), a cursor overlay (`post_render`), a dirty flag, and size limits.
"Flush" means both prepare (`Canopy::flush`, Luau `canopy.flush()`) and backend
output (`RenderBackend::flush`).

Buffers are also copied more than needed:

- Each frame copies all cells into the snapshot (`snapshot.rs:121`), and each
  emit clones the buffer (`rendering.rs:335`).
- `FrameSnapshot.cells` is a flattened `TermBuf`, so consumers redo the index
  math (`records.rs:23-27`).
- Frames flush twice: `TermBuf::diff` and `TermBuf::render` flush, and then
  `emit_frame` flushes again.

**Proposal:**

- Keep "render" for widget painting only.
- Rename `pre_render` to `mount_pending` and `post_render` to
  `overlay_cursor`.
- The crate-private `Canopy::flush` becomes `prepare`.
- `FrameSnapshot` holds an `Arc<TermBuf>`. Delete `Canopy.termbuf`,
  `Canopy::buf()`, and the per-frame copies.
- Flush once per frame.
- Make `TermBuf::{new_with_limits, fill_with, text_with, overlay_cursor}` and
  `StyleManager` crate-private, and add `StyleMap::resolve(path)`.

**Decided:**

- Luau `canopy.flush()` becomes `canopy.prepare()`: 26 uses in 14 Canopy
  files, plus 73 in fh.
- `TermBuf::render` and `diff` become `emit` and `emit_diff`.

#### C20: Turn and poll naming

*Decided. Stage 4.*

"Work" names three things:

- `Work`, a turn's input (`turn.rs:78`).
- `WorkLifetime` and `WorkStamp`, which govern poll scheduling (`wake.rs`).
  `Widget::poll_lifetime()` already says "poll".
- "Pending work" in the `ChangeSet` docs.

**Proposal:**

- `Work` becomes `TurnInput`, and its `Input` variant becomes `Events`.
- `WorkLifetime` becomes `PollLifetime`, and `WorkStamp` becomes `PollOwner`.
- In `launch`, move `RunOptions` into `LaunchMode::Run { mcp_socket, options }`,
  since it matters only for `Run`.
- **Adjusted (fh):**
  - Delete `LaunchMode::Api`. Apps print `factory.script_api()` themselves; fh
    already does this so it can page and highlight the output.
  - `launch` returns `ExitCode`, replacing `process::exit(code)` in all three
    apps.

fh has 7 affected sites.

#### C21: Error variants

*Clear win. Stage 3.*

- Merge `Invalid` and `InvalidOperation`: they have 109 and 59 constructions,
  both map to one script kind, and no pattern separates them.
- Merge `Invariant` into `Internal`. `testing/grid.rs` misuses `Invariant` for
  test failures.
- Fix the stale `ParseError` "marker type" doc.
- Rename `RunLoop` to `Driver`, since the run loop is now the turn driver.

#### C22: Runtime internals

*Clear win except where noted. Stage 2.*

- **Split `backend/crossterm.rs`** (1,732 lines) into modules for input
  translation and coalescing, terminal session, output, and the run loop.
- **One run-loop entry point.** `terminal::runloop` has 0 callers. Keep
  `runloop(canopy, options)`.
- **One wait loop.** `HeadlessEval::run` (`turn.rs:642-664`) re-implements the
  crossterm work selection (`crossterm.rs:253-302`) with different fairness.
  Share one selector.
- **Remove duplication:**
  - poll-owner construction (`rendering.rs:48-61`, `teardown.rs:56-75`)
  - evaluation-completion bookkeeping (`turn.rs:488-502,603-614`)
  - the redundant `published ||` checks
  - the first-frame prepare and emit written outside the loop
    (`crossterm.rs:922-928`)
- **Split `canopy/mod.rs`** (1,646 lines). Move the script lifecycle (lines
  547-1470) to `canopy/scripting.rs`, move `RoutePhase` to `routing.rs`, and
  group the 30 fields into script, journal, and frame parts.
- **Testing module:**
  - Build `HarnessBuilder` on `CanopyBuilder` (with C5).
  - Rename `Harness::with_root_context` to `with_root_widget_context`.
  - Move `testing/driver.rs` next to `turn_testing.rs`.
  - Replace the unexplained requirement IDs R06 to R27 in `contracts.rs`.
- **Fix the stale `bench_layout`.** It has timed a setter since `9098ff55`
  (`benches/core.rs:218`).
- **Delete `DummyContext`** (344 lines, a second `Context` implementation). Its
  users switch to a builder-made context; fh does not use it. This also
  removes the default bodies of `open_modal`, `close_modal`, and
  `modal_is_open`.
- **Delete `Context::request_diagnostic_dump`** and `Root::dump_diagnostics`.
  The request `eprintln!`s onto the live screen (`rendering.rs:315-317`), and
  its only requester is unbound. Keep Luau `canopy.diagnostic_dump()`.

### Tree and input internals

#### C23: Tree internals

*Clear win. Stage 2.*

- **Duplicated tree code:**
  - Unlinking a child from its parent is written three times (`tree.rs:806`,
    `:903`, `:964`).
  - Attach validation is written twice (`tree.rs:742-768`, `:860-889`).
  - `remove_subtree_inner` and `replace_subtree_inner` share their plan and
    hooks but use different focus-hint strategies. Unify them.
- **Duplicated focus code.** "Prefer nodes with a view" is written five times
  in `focus.rs` (313, 321, 340, 373, 379).
- **Attachment generations cost O(n²·depth).**
  `refresh_attachment_generations` walks every node and its ancestors on each
  attach, detach, or set (`tree.rs:84`), so building n nodes costs
  O(n²·depth). Recompute only the moved subtree.
- **Duplicate guard.** The one at `teardown.rs:101-105` repeats the one in
  `enqueue_completion`.
- **Confusing names:**
  - `ensure_invariants` repairs focus and capture (`focus.rs:274`), while
    `validate_invariants` checks structure. Rename the first to
    `repair_focus_and_capture`.
  - Rename `teardown.rs` to `completion.rs`.
- **Misplaced code.** `focus_acceptance` belongs in `focus.rs`.
- **Repeated plumbing.** One private helper can replace the four one-shot
  `&mut dyn FnMut` wrappers and their 10 `Internal("consumed")` errors.
- **Asymmetric access.** `with_widget` takes a `TypedId` and reports a stale id
  as `Internal`; `with_widget_mut` reports `NodeTypeMismatch`.
  - Both take `impl Into<NodeId>` and check the type at runtime.
  - Document `with_widget` as the read path. fh makes 9 read-only calls
    through `with_widget_mut` (`commander.rs:353,496,555,597,895,1027,1197,1214,1267`),
    and every one of them invalidates.
- **Unused method.** `take_mouse_capture` has no non-test callers anywhere.
  Delete it.
- **Hook parameter names.** Use one name for the `&dyn ViewContext` parameter
  across `Widget` hooks. `view: Size` means the content size, so rename it
  `content`.
- **Focus recovery (fh):**
  - Tree edits inside a callback skip the node whose cell is taken, because
    `accepts_focus` treats a taken cell as unfocusable (`widget_access.rs:160`).
    Defer focus repair for such edits until the cell returns;
    `callback_depth` already exists.
  - `detach` computes no recovery hint (`tree.rs:955-959`). Make it record the
    same hint that removal does.
  - Both gaps force hand-written repair in fh (`commander.rs:1294-1300`) and
    canopy-fileselect (`lib.rs:1360-1369`).

#### C24: Input internals ★

*Clear win. Stage 2.*

- Use the single `Core::route` iterator from C1 in place of the seven walks.
- **Stop per-candidate cloning.** `select_key_binding` and `resolve_match`
  clone the description and target of every candidate at every node, and
  discovery calls them for each key × route depth. Return `&BindingRecord`.
- **Simplify `CommandAvailability`.** Drop its lifetime; all specs are
  `&'static`. `BindingCommand` then embeds it instead of copying three fields
  (`help.rs:116-125`).
- **Take names by type.** `WidgetActionCatalog::contains_name(&str)` validates
  and allocates again; take a validated name.
- **Fold single-use wrappers:**
  - `Canopy::predict_key_outcome`
  - `Core::close_modal`
  - the `WidgetActionSpec` accessors, which are used only in their own file
- **Free commands.** `CommandDispatchKind::Free` and `CommandResolution::Free`
  are built only by tests; the derive always emits `Node`. Delete them, along
  with `CommandError::WrongOwner.expected: Option`, which then always holds an
  owner.
- **One `call_*` example.** `termgym.rs:396,399` use `cmd_x().call()` where
  `call_x()` exists.

### Scripting and automation

#### C25: One Luau command story ★

*Clear win. Stage 5.*

Luau has three call forms, and two of them disagree:

- Generated `owner.command(...)` functions decode a single table as named
  arguments when its keys match the parameter names (`dispatch.rs:20-95`).
- The generated constructors `command.owner.x(...)` are positional
  (`base_api.rs:2404-2414`).

After finalization every source is strict-typechecked, so the inference can
fire only when the first parameter already accepts a table. In that case it
guesses wrong; the `{ options = "dark" }` example in `scripting.md` shows the
collision.

`canopy.cmd` and `canopy.cmd_on` have 0 non-test uses in this workspace and in
fh.

**Proposal:**

- `owner.command(...)` is positional and dispatches from the origin.
- Delete `canopy.cmd`, `canopy.cmd_on`, `run_script_command`, and the
  inference code (about 90 lines).
- Rewrite the `scripting.md` Commands section as one story:
  - `owner.command(...)` runs now.
  - `command.owner.command(...)` builds a value.
  - `call_exact`, `call_from`, and `call_focus` give explicit targets.
  - `call_named` gives named arguments.
- Drop the word "legacy".

#### C26: Luau observation and lookup surface

*Decided. Stage 5.*

**Screen queries.**

- `canopy.screen` has 0 uses.
- `screen_cells` returns exactly `flush(); snapshot().cells`. It has 1 test
  use in Canopy and 2 in fh's `smoke/columns.luau`.
- `screen_text` is canonical: 6 `.luau`, 9 Rust, and 9 docs in Canopy, plus
  39 in fh.
- Todo's `delete_item.luau` rebuilds `screen_text` by hand to avoid the
  "legacy" call.

Delete `screen` and `screen_cells`. Document `screen_text()` as the text of
the published frame after a prepare. Fix the bootstrap guide text in
`canopy-mcp/src/script.rs:27-31`.

**Node lookup.** There are 13 functions. Keep:

- `root`, `focused`
- `find_node`, `find_nodes`
- `find_identity`
- `node_info` (add a `parent` field)
- `snapshot().nodes`

Delete these, which have 0 uses in `.luau` and in fh:

- `parent`, `children`
- `tree` and its `TreeNode` type
- `node_at`
- `wait_for_node`, which resolves from focus while `resolve` resolves from
  the origin
- `wait_for_screen_text`

Use `wait_for(fn)` in their place.

**Other renames.**

- Replace `focus_next`, `focus_prev`, and `focus_dir` with
  `move_focus(dir)`.
- `fixtures()` becomes `canopy.fixtures()`; it is the only base global outside
  `canopy`.
- `canopy.input_mode()` becomes `canopy.mode()` (C11).

**Decided:**

- The lookup set above.
- **Adjusted (fh):** `screen_region` and `node_region` fold into one
  `screen_text(target?)`, where the target is a rect or a node. All 5 of fh's
  crops are addressed by node, so a node form stays.
- `resolve` becomes `canopy.target(owner)`, because it previews dispatch
  target resolution.

#### C27: Eval plumbing

*Clear win except where noted. Stage 5.*

- **`EvalRequest.anchor` is always the root.** All 13 constructions pass
  `root_id()`, and the live MCP path makes a UI round-trip partly to fetch it
  (`canopy-mcp/src/script.rs:612-620`). Add `EvalRequest::new(source)` with a
  `timeout` builder, and drop the field.
- **Logs and assertions have three channels:** `EvalOutcome`, a copy-back into
  the host buffer (`turn.rs:490-492`), and the journal.
  - Headless MCP reads the copy-back, while live MCP reads the outcome.
  - The copy-back also overwrites ambient callback output, and ambient output
    grows without bound in a live app.
  - Proposal: use `EvalOutcome` everywhere, and have the journal record from
    the invocation. Delete the copy-back, bound or drop the ambient buffer, and
    put `take_script_logs` and `take_script_assertions` behind `testing`.
- **MCP typechecks every script twice** (`check_script` then compile) and makes
  three UI round-trips on the live path.
  - `EvalOutcome` gains `diagnostics`, and one `outcome_from` serves headless
    and live.
  - Delete `TypecheckGate` and the test-only `evaluate_live` copy
    (`script.rs:505-534`). Test `validate_live_request` directly.
- **`LuauHost::execute` takes a `timeout` that is `None` at every production
  caller.** Drop the parameter and its abort branch.
- **Gate `eval_script` behind `testing`.** It has 0 production callers and
  about 150 test uses.
- **Warnings.** `EvalOutcome.diagnostics` carries errors now. Warnings follow
  when ruau's prepare step exposes them; that ruau change is outside this
  plan.

#### C28: Automation records and wire vocabulary

*Decided; wire changes with no compatibility. Stage 5.*

- **`ResetPolicy::Fixture` is derived state.** No app declares it, and the
  derivation rule is written three times (`canopy-mcp/src/script.rs:792`,
  `metadata.rs:270`, `canopyctl/src/replay.rs:87`). Use
  `ResetPolicy { Isolated, External }` plus `ExecutionMetadata.fixture`.
- **"Journal" means two things.** `canopyctl eval --journal-out` writes a
  replay file. Rename the flag to `--replay-out`. Drop "Replayable" from the
  `ScriptJournalEntry` docs, and drop its unused serde derives and
  `ScriptOrigin::Other`.
- **`ScriptEvalOutcome` becomes `EvalReport`.** "Outcome" then means the
  runtime result.
- **Smaller wire cleanups:**
  - Drop `SuiteScript.fixture`, which duplicates `request.fixture`.
  - Drop `BootstrapResponse.default_target`, which is always `"root"`.
  - Rename "evaluator" to "factory" (`server/mod.rs:58`, `EvaluatorPeer`).
- **Remove the MCP `fixtures` tool.** Bootstrap and Luau already list
  fixtures, and the headless tool builds a whole app to answer. Delete
  `AppFactory::fixtures` and `Session::fixtures`; `canopyctl fixtures` reads
  bootstrap.
- **Rename wire fields:**
  - `session_id` becomes `instance_id`, since "session" is canopyctl's client
    connection.
  - MCP `script` becomes `source`.
  - MCP `viewport` becomes `screen`.
- **Rename the builder source.** `CanopyBuilder::bindings(name, src)` runs
  arbitrary Luau, so it becomes `script(name, src)`, with
  `ScriptOrigin::Build`. `config(path)` becomes `script_file(path)`.

#### C29: Script internals

*Clear win. Stage 2.*

- **`base_api.rs` duplication:**
  - Extract `action_from_value`; `plan_keymap_entry` copies `read_action`
    (`:1993` vs `:1767`).
  - Fold the three `install_*_binding` functions into one.
  - Derive `Clone` for `ScriptAction`.
  - Use the existing `canopy_context()` helper at its three inline copies.
  - Let `send_click` reuse the `send_drag` loop.
- **`value.rs` has two copies of one conversion policy.**
  `table_to_arg_value` and `marshaled_table_to_arg_value` duplicate sequence
  and map assembly. One generic assembler makes the documented "one policy" true
  by construction.
- **Move the `LuauState` constructor and helpers** from `bridge.rs:20-42` into
  `script/mod.rs`.
- **Type the diagnostic severity.** `ScriptCheckDiagnostic.severity` becomes an
  enum that serializes to the same strings. Keep one of `is_ok` and
  `has_errors`.
- **Visibility (dropped).** These `pub` items already sit in private
  modules, so they are unreachable from outside. The workspace lint
  `clippy::redundant_pub_crate` requires plain `pub` there, so no narrowing
  applies.
- **Delete the `script_native_modules` plumbing.** Only a `#[cfg(test)]`
  method fills it. Also delete `startup_requirements` and
  `require_startup_global`, which only the testing feature reaches.

### Widgets and styling

#### C30: One checklist

*Decided. Stage 6.*

Selector (`a21d5f06`) and List checks (`71696f66`) landed one commit apart,
with contradictory vocabulary:

| Operation | Selector | List |
| --- | --- | --- |
| uncheck all | `Selector::clear` | `List::clear_checks` |
| check all | `select_all` | `check_all` |
| empty the collection | — | `List::clear` |
| name of the checked set | "selected" | "checks" |
| name of the cursor | "focused" | "selected" |
| report checks to automation | a joined `value` | `selected_keys` |

fh uses List checks (`canopy-fileselect/src/columns.rs`, `Selectable::set_checked`).
Neither repo uses the Selector checklist.

**Proposal:**

- Keep List checks.
- Return Selector to single choice.
- Standardize the vocabulary: `toggle`, `check_all`, `clear_checks`;
  "checked" for membership and "selection" for the cursor.
- **Adjusted (fh):** split `WidgetSemantics.selected_keys` into
  `selected_keys` (the cursor) and `checked_keys` (the checked set). Today a
  checked List reports its checks as `selected_keys` (`list.rs:836-848`), so
  one field changes meaning with the list's mode.

**Decided:** keep List checks, and return Selector to single choice.

#### C31: One styling rule

*Decided. Stage 6.*

There are four patterns today:

1. Push a layer and paint bare roles: Button, Input, Picker.
2. Paint `<name>/<part>` with no layer: Frame, Tabs, Columns, Selector,
   Dropdown, Editor, List, StatusBar.
3. Both at once, so the resolver probes `x/x/part` first: `confirm/message`
   under `confirm`, and `help/*` under `help`.
4. A prefix that differs from the node name: DiffView paints `diff/*` on node
   `diff_view`, KeyHint paints `status_bar/*`, Border's node is `box`.

**Rule:**

- A widget's style prefix is its node name.
- Leaves, composites, and surfaces push that layer and paint bare part roles.
- Containers around foreign content push nothing and paint `<name>/<part>`.
- Never both.
- Cursor rows use `roles::selection(active)`.

**Fixes:**

- Remove the double prefixes. Resolved paths do not change.
- StatusBar pushes `status_bar`, so plain `Text` resolves correctly. This
  deletes the hard-coded `"status_bar/text"` in hello, todo, and the examples.
- **Adjusted (fh): hints follow the binding.** Replace free-text `KeyHint`
  keys with `KeyHint::for_command(call, label)` and `KeyHint::for_intent(name,
  label)`. Both resolve the key label through binding discovery at render.
  Add Luau `canopy.key_for(command_or_intent)` for scripted hints and
  messages.
  - Today the hint `"ctrl-g"` is typed separately from the binding `Ctrl+g`
    in hello, todo, the examples, and fh (`default_config.luau:31`).
  - fh's trash message `"· u undo"` hard-codes a key too
    (`commander.rs:633`).
  - This deletes canopy-fileselect's `set_footer_hint` command.
- Selector and Dropdown use `roles::selection`. This changes the golden file.
- Add DiffView theme paths (C3), and delete fh's own `diff/*` rules
  (`lib.rs:238-244`).
- Replace `StyleBuilder` with chaining methods on `PartialStyle`.
- Examples push `WidgetState::Selected` instead of a raw `"selected"`.

**Decided:**

- `roles` holds shared part names in core: `TEXT`, `BACKGROUND`, `BORDER`,
  `KEY`, `PROMPT`, `CURSOR`, `TITLE`, `THUMB`, plus `selection(active)`.
  Delete the per-widget aliases (`BUTTON_LABEL`, `INPUT_TEXT`, and the rest).
- One `style::themes` module holds `default_dark`, `dracula`, `gruvbox_dark`,
  `solarized_dark`, and `solarized_light`, with a public `Palette`. Apps use
  palette roles instead of colour constants. Delete the public colour constants
  in `style::default` and `style::solarized`.
  - Four of five consumers rename `style::default` on import, and
    `StyleMap::default()` is not the default theme.
- DiffView colours come from palette roles: added rows use `green`, removed
  rows `red`, headers `accent`, and gaps `muted_fg`. These match fh's current
  choices, except that fh uses blue for headers.
- **Adjusted (fh): widget crates style themselves from the palette.** Add
  `Setup::widget_styles(|palette: &Palette, rules| ...)`, layered under the
  theme, so switching themes keeps widget styles.
  - Today canopy-fileselect's `install_styles` hard-codes `style::default`
    constants. It must run after fh sets the theme, which is why it cannot
    live in `Loader` (fh `lib.rs:227-246`).
  - It also restyles Canopy's Picker for the whole app from a crate that
    never mounts a Picker. `FileSelect::register` installs its own styles,
    and the picker styles move to fh.
  - `Palette` fields replace the public colour constants for fh's 8 named
    colours.

#### C32: Shared widget parts ★

*Clear win except where noted. Stage 6.*

- **A private `RowCursor`** replaces cursor and reveal code copied across
  Selector, Dropdown, and PickerList. The reveal helpers, click-to-row, and
  visible-row loops are near-identical (`selector.rs:173-302`,
  `dropdown.rs:109-215`).
- **A public `Dialog` widget** provides the layer, margin, measured titled
  Frame, and margin click swallowing. It is used as
  `Dialog::new().with_title(..).with_max_width(..)`.
  - Confirm and Picker duplicate these, including their constants
    (`confirm.rs:23-25,281-355`, `picker.rs:35-39,209-278`). Help and ModeHelp
    use it too.
  - **Adjusted (fh):** it is public, not private, because canopy-fileselect
    builds the columns dialog by hand (`columns.rs:776-839`). That dialog
    misses the margin click swallowing, and its 11 style rules restate the
    theme's panel dialog (`lib.rs:2201-2238`).
  - It paints `dialog/<part>`, so one rule set in the theme styles every
    dialog. Today the theme repeats the frame rules for picker, confirm, and
    help.
- **Help surfaces.** Delete the vestigial `HelpPanel` (`help/panel.rs`). Make
  `ModeHelp` the overlay widget; it currently misuses `Center` with
  `Align::End` (`help/mode.rs:98-106`).
- **One width helper.** Add `canopy::text::width(&str) -> u32` and
  `canopy::text::cell_width` for tabs.
  - The name is not "columns", which already names the `Columns` widget, List
    columns, and fh's metadata columns.
  - Eight sites measure with `UnicodeWidthStr::width` while rendering uses
    grapheme width.
  - The saturating `u32` conversion is rewritten about 10 times in
    canopy-widgets and 11 times in canopy-fileselect.
  - Then drop the direct `unicode-width` dependency from canopy-widgets.
- **One axis type.** `scrollbar::Axis` duplicates `canopy::ScrollAxis`
  (`scrollbar.rs:19-24,449-454`).
- **A top-level `highlight` module.** The `Highlighter` trait and
  `HighlightSpan` have no editor dependency. Share one highlighted-run painter
  between the Editor line loop and `DiffView::draw_code`. This removes 18
  `#[cfg(feature = "editor")]` in DiffView.
- **No `editor` feature (decided during Stage 2).** Only `SyntectHighlighter`
  needs syntect and two-face, which bring the Oniguruma C library and about
  10 MB of rlibs, mostly bundled syntax data. The Editor, `text_buffer`, the
  `Highlighter` trait, and DiffView's highlighting hooks compile
  unconditionally. A narrow default `syntax` feature gates only the
  syntect-backed highlighter, which removes nearly all 26 feature gates.
- **A public run painter (fh).** Expose that painter as
  `Render::runs(line, base, runs)`, with each run's foreground laid over the
  base style. canopy-fileselect paints styled runs by hand in its footer
  (`lib.rs:1920-1941`) and its Git cell (`listing.rs:434-463`).
- **One click threshold** for Editor (500 ms) and Terminal (400 ms).
- **Border naming.** Module `boxed`, type `Border`, and node `"box"` become
  `border` everywhere.
- **Container presets.** Replace `Center` and `Pad` with
  `Container::center()` and `Container::padded(edges)`. Both hold nothing but
  a `Layout`, and fh uses `Center` once.

#### C33: Widget naming and visibility

*Mostly clear win. Stage 6.*

- **Module rule.**
  - Every widget is exported at the root exactly once.
  - A module is `pub` only when it carries a family of supporting types:
    `editor`, `highlight`, `font`, `terminal`, `scrollbar`, `diff`.
  - No item has two paths. Today `Scrollbar` and `THIN` have two, and
    `ImageView` appears under `font::`.
- **Generic root names get homes:**
  - `Scope` becomes `diff::Scope`, and `Strategy` becomes `diff::Mode`.
  - `Truncate` moves to `canopy::text`.
  - `AutoKey` becomes `list::AutoKey`.
  - `THIN` and the box glyph constants become associated constants.
  - `wrap` becomes `ContextExt::wrap_node`, because "wrap" already means text
    wrapping.
- **Consistent methods:**
  - `show(items, ..)` on Selector and Picker becomes `set_items`.
  - `with_command` is used for every stored command call; List's
    `with_on_activate` changes.
  - `TerminalConfig::with_command(argv)` becomes `with_program`.
  - `Editor::highlight_matches` becomes `set_matches`.
- **Must-use.** Enable `clippy::return_self_not_must_use` workspace-wide.
- **Empty impls.** Remove empty `#[derive_commands]` on Center, Pad, Border,
  and Frame.
- **Trim unused methods.** Remove only methods unused in both repos:
  `DiffView::{set_diff, set_scope, set_strategy, strategy}`, the `Frame` and
  `Columns` `scrollbar_glyphs()` getters, and `Confirm::body()`.
- **Stale comments.** Fix `list.rs:925-927`, `selector.rs:353-354`, and the
  help-footer comment at `binding_list.rs:348`. Also remove the double cfg
  branch at `root.rs:341-350`.
- **Narrow `text_buffer`.** Make it crate-private. Re-export only `TextRange`
  and `TextPosition`, under `editor`: `Editor::set_matches` needs them, and
  canopy-fileselect's search uses them (`search.rs:20`).
- **Rename the `Label` item trait** to `ItemLabel`.
- **One Editor interaction field.** `EditorConfig` carries three interaction
  modes in two booleans. Replace them with `interaction: {Edit, View, Display}`.

#### C34: Navigation and scroll command parity

*Decided. Stage 6, after C10.*

The cursor widgets expose different command sets:

| Widget | Navigation commands |
| --- | --- |
| List, Logs | `select_first`, `select_last`, `select_by`, `page`, `scroll` |
| Selector | `select_by`, `select_first`, `select_last` |
| PickerList | `select_by`, `page` |
| Dropdown | `select_by` |

`Tabs::select_by` wraps, while every other `select_by` clamps. Scroll and page
commands are defined five times.

**Proposal:**

- **One cursor command set.** Give every cursor widget the same clamped set:
  `select_by`, `select_first`, `select_last`, and `page`.
- **Tabs.** The wrapping `Tabs::select_by` becomes `Tabs::cycle`.
- **Navigation intents.** Core registers built-in navigation intents:
  `canopy.nav.up`, `down`, `left`, `right`, `page_up`, `page_down`, `first`,
  and `last`. Apps do not register them.
  - At each route node, a widget that accepts the intent handles it:
    - A cursor widget (List, Selector, PickerList, Dropdown) moves its
      selection.
    - canopy-fileselect accepts them only while find is active. This replaces
      its hard-coded `find_command` keys (`find.rs:447-460`), which help cannot
      show and users cannot rebind, and the 12-key × 5-state agreement test
      (`lib.rs:2474-2512`).
  - Otherwise the runtime applies a default action when that node's view can
    move, as the wheel does today. A view that cannot move declines, and the
    route continues.
  - Delete the `scroll`, `page`, and `scroll_to` commands on Text, List,
    DiffView, Logs, BindingList, and the examples, and the four duplicated
    direction matches (C17). Cursor widgets keep their `select_*` commands for
    scripts and buttons.
  - One keymap entry then serves every scrollable and cursor widget.
  - **Adjusted (fh):** intents act only on the focus route. fh's keys that
    scroll the preview or diff pane from the listing remain app commands.
    These call `scroll_node` (C17), since those panes are not on the route
    (`commander.rs:393-454`).

### Repository, tooling, and docs

#### C35: Repository hygiene and manifests

*Clear win. Stage 1.*

- **Stray file.** Delete the tracked root file `Error`. It holds one line,
  `---- CommandError-`, committed by accident in `c8da09f4`.
- **Resolver.** Set `resolver = "3"`. The root has `"2"`, while
  getting-started and nano-rust use 3.
- **CI.** The fmt list omits `hello`. The blocker comment names `itty-script`,
  which is gone, and omits `musq`.
- **Manifests:**
  - Put `version` and `authors` in `[workspace.package]`.
  - Put internal crates in `[workspace.dependencies]`.
  - Set `publish = false` on examples.
  - Drop the stray `version` on path dependencies.
- **Local cleanup.** Remove the stale ignored
  `crates/canopy/tmp/canopy-script-framework-config-*`.

#### C36: Examples structure

*Decided. Stage 7.*

**Two example locations with no rule.** `crates/examples` (package
`canopy-examples`) is a library of 13 gyms plus two binaries. It has a
777-line API capture, and agents cannot drive it.
`examples/{hello,todo}` are driveable apps with smoke suites.

- Rule: `crates/` holds framework crates and tooling; `examples/` holds apps.
- Move the gyms to `examples/gyms` as one binary. This drops the capture and
  lets dead-code lints see the modules.
- Route the gyms through `canopy_mcp::launch` with a `.canopyctl.toml` and a
  smoke script, so agents can drive them like the other examples.

**The `widget` showcase binary.** Its recorder scripts were deleted
(`5c688915`), so the README gifs cannot be regenerated. The binary mostly
repeats the gyms. Delete it with `src/widget/` and `assets/tiger.jpg`. The
README gifs stay until they are recorded again from the gyms, which is outside
this plan.

**Duplicated bindings.** `crates/examples/src/lib.rs:21-30` re-registers
Ctrl+g, which `root.default_bindings()` already provides. Several gyms and todo
re-bind `q`. Every binding script starts with `root.default_bindings()`.

**Two config models.** Hello and todo teach different ones: a user script root
with first-run setup, versus `--config FILE`. **Decided:** user script roots,
through the Canopy helper in C41, which follows fh's model. Convert hello and
todo to it. **Duplicated bindings (fh):** fh's `default_config.luau:22-28`
repeats Root's Ctrl+g binding line for line; delete it with the rest.

#### C37: Tooling

*Clear win. Stage 2.*

**Tidy hooks.** They run three xtasks on every `ncode tidy`.

- `feature-check` step 2, `cargo check --workspace --all-targets`, repeats
  ncode's build.
- `checks` runs two tests that `ncode test` already runs; only its Luau
  inventory (`xtask/src/main.rs:148-170`) is unique.
- `bench-check` adds only linking.

Proposal:

- Drop the duplicated step.
- Move the inventory into a test.
- Run bench-check in CI only.
- Trim the hooks to `feature-check` alone.

**xtask fixes.**

- The `Checks` doc wrongly mentions API skeletons.
- Rename `run_default_check` to match `FeatureCheck`.
- Smoke discovery walks the filesystem; use `git ls-files '*.canopyctl.toml'`
  like the inventory.
- Keep both `cargo_env.rs` copies, since the xtask leaf rule forbids sharing.
  Add a cross-reference comment to each.
- **fh's xtask (fh).** Delete it (118 lines), its workspace member, and its
  `.cargo` alias.
  - Its only task runs `canopyctl smoke`, and canopyctl already strips Cargo's
    package environment when it spawns Cargo (`canopyctl/src/config.rs:231-241`).
  - Its `cargo_env.rs` is a third byte-identical copy.
  - fh's docs then say `canopyctl smoke`.

#### C38: Documentation realignment, and retiring `api-budget.md`

*Clear win. Stages 1 and 7.*

**Retire `docs/api-budget.md` (Stage 1).** It no longer does its job:

- **The thresholds constrain nothing.** They are advisory, and no tool reads
  them.
  - Every surface that crossed its threshold kept growing, each time with a
    paragraph of justification: `Context` went from 67 to 70 methods against
    60, `ViewContext` from 43 to 45 against 34, and `canopy-widgets.rs` from
    2,037 to 3,028 lines against 2,050.
  - Other thresholds sit far from reality, such as `canopy.rs` at 5,031 lines
    against 8,826.
- **It is a changelog.** Of the 8 commits that touched it after it was
  created, 7 only appended a growth justification. Under the project
  conventions, those reasons belong in commit messages.
- **Nothing depends on it.** No doc, code, or tool references it; even the
  README index omits it.
- **The real control is elsewhere.** The `api/` captures and their reviewed
  diffs already show every surface change, and every stage of this plan
  reviews them. A fresh baseline after this plan would be arbitrary, because
  Stages 3 to 6 reshape every budgeted surface.

Move its four lasting pieces into `architecture.md`, then delete the file:

- **Surface rules**, in the "Public API Surface" section:
  - `Canopy` holds only operations that cannot live on a context or on
    `Setup`.
  - A new context query or mutation replaces or generalizes an existing one,
    in the node-addressed form (C13).
- **Why scroll and reveal stay separate.** Explicit scrolling moves a view at
  once, a reveal waits for layout, and a scrollbar owner scrolls a node other
  than itself.
- **Why intents are registered at setup.** Names must exist before script
  finalization, and the same catalog validates bindings and renders the Luau
  API.
- **Accepted dependency coupling.** `EvalTicket::completion` exposes
  `futures::channel::oneshot::Receiver` directly.

The Editor rule "editing policy on the editor, buffer mechanics on the buffer"
moves to the `editor` module docs, since `text_buffer` becomes crate-private
(C33).

**Stage 1 facts:**

- `architecture.md`:
  - `Canopy::flush` (`:372`) and `next_deadline` (`:377`) are crate-private.
  - The module list omits `commands`, `keyroute`, `terminal`, and `testing`.
- `scripting.md` describes testing-only or crate-private items as app API:
  - `finalize_api`, `register_script_module`, `require_startup_global`, and
    `invalidate_script_modules`.
  - "Low-level `Canopy` setup APIs remain available", although `Canopy::new`
    is testing-only.
  - The stale type list at `:17-24`; point to `canopy.api()` instead.
  - Provisional rows "stay in `bindings`" (`:279`); they move to
    `provisional_bindings`.
- `fixtures.md`: "only checked-in suite", but hello has one too.
- `getting-started.md`:
  - The `Widget` block no longer matches `examples/hello`: it uses
    `Layout::fill()` and shows a stale "? help" status line.
  - The dependency block omits `default-features = false`.
- `agent-loop.md:101-108` asserts that a fixture item that never existed is
  deleted, so the check always passes.
- `README.md`: the doc index omits `styles.md`, and nothing says how to run the
  gyms.
- Delete `plans/mouse-activation.md`; everything durable is already in
  `architecture.md` and `styles.md`.
- Add the nearest-cell thumb rounding sentence to `architecture.md`
  Scrollbars, then delete `plans/scroll.md`. Its sub-cell and gutter proposals
  are not scheduled.

**Stage 7 realignment:**

- Rewrite the "Public API Surface" section of `architecture.md` for C12. Keep
  the surface rules above.
- Rewrite the `scripting.md` Commands section for C25.
- Update styles for C31.
- Add a short repository map to `AGENTS.md`:
  - the sibling path dependencies itty, ruau, tmcp, and musq
  - the fh consumer
  - `cargo xtask smoke`
  - `ncode api` for capture review
- Update fh's `architecture.md`, `README.md`, and `find.md` (C50).

### Capabilities the fh pass exposed

#### C39: Recoverable command failures

*New. Stage 3.*

A failing command bound to a key ends the run loop: the terminal adapter
propagates `turn` errors, and routing avoids that only for disabled commands
(`routing.rs:583-587`). fh therefore turns every failure into a footer message
by hand:

- the settle path (`commander.rs:853-856`)
- trash, undo, and delete
- picker loads (`:1141-1149`)
- watch failures

It also discards errors on the poll path (`:938,946,1370`), and wraps I/O
errors as `Error::Invalid` (`:1538-1540`).

**Proposal:**

- **Split failures into two classes.**
  - A command's own failure (`CommandError::Exec`, or an app error) is a
    **notice**.
  - Runtime invariant, backend, and layout failures stay fatal.
- **Record notices.** The turn keeps notices in a bounded queue.
  - `Canopy::notices()` and Luau `canopy.notices()` read the queue.
  - The route trace records each notice.
  - `Root` shows the newest notice in its status bar.
  - Poll failures also become notices.
- **Scripts are unchanged.** A command that fails inside a script call still
  raises a script error. Only failures from input bindings and polls become
  notices.
- **Add `Error::App(Box<dyn Error + Send + Sync>)`.** App code then returns its
  own errors unwrapped, and C2's `script_kind` maps this variant to `app`.
- **fh deletes its hand-built error-to-footer plumbing.**

**Risk:** tests that expect a failing binding to fail its turn must assert on
notices instead.

#### C40: A complete text field

*New. Stage 6.*

`Input` has no Home, End, delete, paste, runtime prompt or label, or owner
notifications. canopy-fileselect wraps it three times: `field.rs`, `FindField`
(`find.rs:490-635`), and `SearchField` (`search.rs:38-147`). The wrappers work
around the gaps:

- They synthesize Home and End by repeating left and right (`field.rs:30-38`).
- They turn paste into key events (`:44-47`).
- They rebuild the `Input` to change its prompt (`find.rs:535-547`).
- They reach their owner through 8 `with_widget_mut` chains.

Canopy has two more line editors: the PickerList filter
(`picker.rs:317-424,693-735`) and the Editor vi prompt.

**Proposal:**

- `Input` gains Home, End, delete, paste, `set_prompt`, and `set_label`.
- Owner notifications become stored command calls:
  - `with_on_change(call)`, which appends the value as the last argument, the
    way List appends the row index.
  - `with_on_submit(call)` and `with_on_cancel(call)`.
- fileselect's fields become plain `Input` nodes that target the existing
  `FileSelect` commands. This also fixes their C31 layer violations.
- PickerFilter becomes an `Input`. This removes the duplicated
  `picker/filter/*` theme paths (`palette.rs:244-258`).

**Effect:** about −220 lines in fh, −60 in Canopy, and +80 in `Input`.

**Risk:** the fields' "take every key" and Enter rules move into owner
commands.

#### C41: A config-home helper

*New. Stage 5. Resolves C36's config model.*

Each app resolves and loads its user configuration differently:

- **hello** writes defaults on first run: `ensure_user_config`,
  `create_new_file`, `indent` (`lib.rs:134-173`). It has the flags
  `--no-config` and `--config-home`, and the variable `HELLO_CONFIG_HOME`.
- **todo** takes `--config FILE`.
- **fh** has the cleanest model:
  - It resolves the home from the flag, then `FH_CONFIG_HOME`, then
    `$HOME/.fh`, and rejects `--no-config` together with `--config-home`
    (`main.rs:76,226-233`).
  - It mounts no root in headless mode, and errors on an explicit home there
    (`main.rs:132-151`).
  - It mounts the root only when `init.luau` exists. Otherwise it runs the
    built-in defaults as a startup script with the same `setup()` contract, so
    one file serves as both (`lib.rs:76-78,119-121`).
  - It never writes. Users bootstrap with `fh --default-config`.

**Proposal:** adopt fh's model in `canopy-mcp`, next to `launch`.

- `ConfigHome::resolve(app, flag, no_config, &mode)` implements the
  resolution order and the hermetic rule for headless modes.
- `CanopyBuilder::user_config(home, defaults)` mounts `@user` when `init.luau`
  exists, and otherwise runs `defaults` as the startup script.
- The helper never writes. An opt-in `write_defaults(home)` with `create_new`
  semantics replaces hello's first-run code.
- hello, todo, and fh adopt it. `.canopyctl.toml` files drop `--no-config`,
  because headless mode is hermetic by construction.

#### C42: Widgets ship their keymaps, and modals admit groups

*New. Stage 6.*

A framework modal admits one group, so hosts re-declare the keymaps of the
widgets inside it:

- fh re-binds Confirm's 11 inputs (`commander.rs:1455-1535`). It maps Down to
  Next but has no Up to Prev.
- fh re-binds 12 Picker and PickerList inputs under a hand-written path
  (`:1402-1453`).
- canopy-fileselect re-binds List navigation (`lib.rs:2116-2162`).

Canopy's Confirm tells its owner to do this (`confirm.rs:35-41`), while Help
ships its own group. Widgets thus have two ways to ship bindings, with no rule
between them: Luau default-binding scripts and Rust framework groups.

**Proposal:**

- **One rule for shipping bindings.**
  - A widget that runs inside modals ships a framework group from its
    `Register` impl: `Confirm::BINDINGS`, `Picker::BINDINGS`, and
    `List::BINDINGS`. The groups use the navigation intents from C34 where they
    apply.
  - An inline widget keeps a default-binding script at the application tier,
    as Root and Button do.
- **One admission type** (lands in Stage 4 with C10). `ModalBindings` becomes
  either `Application` or an admission of framework groups plus intents. A host
  admits its own group plus the groups of the widgets inside the modal:

  ```rust
  enum ModalBindings {
      Application,
      Framework {
          groups: &'static [FrameworkBindingGroup],
          intents: &'static [&'static str],
      },
  }
  ```

- **Open helpers.** Add `Confirm::open(ctx, request) -> ModalToken`, and
  `Picker::set_commands(accept, cancel)`. fh keeps only its own `d` binding.

**Effect:** about −130 lines in fh's commander, and −45 in canopy-fileselect.

#### C43: Event-driven background work

*New. Stage 4, with the widget part in Stage 6.*

**Workers poll their channels.** Three workers drain `try_recv` on 50 ms polls:
`git_status.rs:139`, `changes.rs:150`, and fileselect `find_worker.rs:218`.
Only fh's watcher wakes its owner (`commander.rs:983-996`).

- Add `canopy::runtime::wake_channel(handle, capacity)`. It returns a
  `WakeSender<T>` that wakes its owner on send, plus the receiver. It adds
  nothing to `Context`.
- fh's three polling workers switch to it, and so does its watcher, which
  wakes by hand today. Polling then stops while idle.

**Widgets wake themselves with throwaway handles.** They create a handle just
to wake their own node (`commander.rs:1238,387-389`, `find.rs:283`).

- Add `Context::request_poll()`. It is one of the allowed self forms in C13.

**Terminal exits are found by polling.** fh's `reap_terminals` checks every
terminal every 250 ms (`commander.rs:1262-1303`), although Terminal already
knows when its child exits (`terminal.rs:486-506,629`).

- Add `TerminalConfig::with_on_exit(call)`, and delete the reaping poll.

#### C44: The command derive surface

*New. Stage 3.*

- **`CommandEnum` emits snake_case names to Luau.** Today Luau sees variant
  names as written, such as `"Down"` (`canopy-derive/src/lib.rs:233-236`), and
  parsing already ignores case. This matches C9's snake_case labels.
  - fh derives `CommandEnum` for `Column`, `Order`, `SortDirection`,
    `WidthPolicy`, and `FindMode`.
  - That deletes the hand-written `ToArgValue` and `parse` code
    (`columns.rs:48`, `lib.rs:909,969`).
  - It also replaces five `toggle_*` commands with one `toggle_column(Column)`
    (`columns.rs:736-764`).
- **Generated functions inherit the method's visibility.** Today the derive
  always emits `pub` `call_*` and `cmd_*` functions (`codegen.rs:633,651`).
  canopy-fileselect has 122 of them, filling 360 of the 964 lines of its
  capture.
- **`cmd_*` becomes `spec_*`,** because it returns a `CommandSpec`.
  `canopy.cmd` goes in C25.

#### C45: Every widget predicts its keys

*New. Stage 3.*

`Widget::key_outcome` returns `Option<EventOutcome>`. The `None` default exists
only for widgets written before prediction existed.

- 31 of the 36 canopy-widgets implementations, and all 5 in fh, return
  `Some(Ignore)`.
- No first-party widget returns `None`.

The whole unknown-prediction machinery serves only widgets that skip the
method: `provisional_bindings`, `key_prediction_gaps`, `KeyPredictionGap`, and
`RouteCertainty`, 64 references in all.

**Proposal:**

- `key_outcome` returns `EventOutcome`, with a default of `Ignore`.
- Delete the provisional-binding and gap machinery. Every analysis becomes
  exact.
- Routing compares each key's actual `on_event` result with the widget's
  prediction. On a mismatch it records a trace entry and fails a debug
  assertion, so tests catch a widget that forgot to predict. The
  `KeyDispatchDivergence` machinery already records divergences for checked
  dispatch.
- Delete the boilerplate implementations: 31 in canopy-widgets and 5 in fh.

#### C46: DiffView becomes complete, and DiffPane goes

*New. Stage 6.*

fh's DiffPane (377 lines) wraps DiffView to add what it lacks:

- It embeds DiffView as a field, not a node, so DiffView's own scroll and page
  commands are unreachable.
- It redefines six `#[command]` methods that are never registered: there is no
  `add_commands::<DiffPane>` (`diff_pane.rs:106`).
- `change_heads` (`:306-325`) re-encodes DiffView's row model.
- Its spinner duplicates canopy-fileselect's (`find.rs:25`).

**Proposal:**

- DiffView gains next and previous change commands, `set_message`, and a
  loading state.
- canopy-widgets adds a `Spinner`.
- fh mounts DiffView directly and deletes about 250 lines.
- `PreparedDiff` becomes `DiffModel`, because "prepare" means building a frame
  (C19). `from_prepared` becomes `DiffView::new(model)`, and `set_prepared`
  becomes `set_model`.

#### C47: The test harness drives real turns

*New. Stage 3.*

fh's tests use about 110 lines of sleep loops that call private `poll_git` and
`reap_terminals` directly (`commander.rs:1789-1806,2844-2864,3027-3043,3248-3279`).
These bypass the real wake and poll wiring. `answer()` reads button labels at
fixed cell offsets (`:1690-1714`).

**Proposal:**

- Add `Harness::wait_until(timeout, predicate)`. It runs real turns under the
  manual or the real clock, and is the Rust counterpart of Luau `wait_for`.
- Add `Harness::with_unique::<W>(f)`.
- Rename `Harness::with_root_context` to `with_root_widget_context` (C22).

#### C48: Nested modals (design spike)

*New. Stage 6. Needs a spike before it is adopted.*

A modal must sit inside its owner's subtree, and the owner must be admitted
(`interaction.rs:164-176`). Asking a question over a Picker therefore costs fh
a lot of workaround code:

- It keeps two Confirm instances (`commander.rs:171-173,1356-1364`).
- Picker grows `add_overlay` and `open_overlay` only for this case
  (`picker.rs:150-195`).
- Hosts hide modal nodes by hand (`:1354,1363`).
- fh mirrors the modal stack in its own tokens (`:1115-1119`), although Canopy
  already closes younger modals (`interaction.rs:228-240`).

**Direction:**

- Root owns one overlay layer.
- `open_modal` accepts a modal node in that layer whose owner the current top
  modal admits.
- `add_modal(W) -> TypedId` attaches a hidden node there.

The spike must prove the dimming, admission, focus restoration, and structural
rollback invariants. Only then delete `Picker::add_overlay`, `open_overlay`,
and fh's second Confirm.

### fh structure and terminology

#### C49: fh structure

*New. Stage 2.*

- **Split `commander.rs`** (3,279 lines: 1,551 of code and 1,728 of tests):
  - `commander/mod.rs`: the struct, thin command delegates, and `Widget` and
    `Register`.
  - `terminals.rs`, `dialogs.rs`, `pickers.rs`, and `git_view.rs`.
  - The trash operations move into `trash.rs`, and shared helpers move into
    `tests/support.rs`.
  - Commands stay in one derive block as thin delegates, because the derive
    emits one `CommandNode` per type.
- **Split canopy-fileselect `lib.rs`** (5,169 lines):
  - `lib.rs` keeps the struct, the command facade, `Widget`, and `Register`.
  - New modules: `mount.rs`, `navigate.rs`, `preview.rs`, `view.rs`,
    `host.rs`, `style.rs`, and `path.rs`.
  - `columns.rs` splits into a metadata model and a dialog.
  - The tests split along the same lines.
- **Read through the read path.** fh switches its 9 read-only `with_widget_mut`
  calls to `with_widget` (C23), and stores `TypedId` handles.
- **Shrink fh's library surface:**
  - `fh::factory(path, roots)` is shared by `main.rs` and `tests/smoke.rs`.
  - After C41, the library exports `UserRoots`, `factory`, `default_config`,
    `user_config_root`, and `write_luau`.
  - Drop the `#![deny(unsafe_code)]` lines that the workspace lint already
    covers.
- **Shrink canopy-fileselect's surface.** `ChangeLabel::runs` becomes
  `pub(crate)`. The `Picker` and `PickerList` aliases become `PositionPicker`
  and `PositionPickerList`, so they stop shadowing Canopy's names.
- **Local deslop:**
  - Remove the duplicated heading and metadata style rules
    (`lib.rs:2241-2261`).
  - Merge the two consecutive dialog callbacks (`lib.rs:1053-1058`).

#### C50: fh terminology, docs, and defects

*New. Stages 1, 4, and 7.*

**Defects (Stage 1):**

- Add a rule for `fh/diff/loading`, which has none (`diff_pane.rs:47`).
- Drop `derive_commands` from DiffPane, whose commands are never registered.
  C46 later removes DiffPane.

**Renames (Stage 4):**

- Dot entries: `ListingView.hidden`, `ListingOptions.hidden`, `toggle_hidden`,
  and `show_hidden` say "dotfiles".
- `FileSelect::publish` and `publish_find` become `sync_panes` and `sync_find`.
- canopy-fileselect's `Sizing` becomes `WidthPolicy`; it shadows
  `layout::Sizing`.
- The node `find_glob`, which also serves the rg and jump fields, becomes
  `find_field`.
- `watch.rs`'s private `Context` becomes `WatchContext`.
- The test data in `fixtures/tree` moves to `testdata/tree`, because it is not
  a Canopy fixture.
- fh's `Prepared` and `set_prepared` follow `DiffModel` (C46).
  `Commander::prepare` gets a name for the question it builds.
- The columns dialog layer `"columns"` becomes `columns_dialog` under C31's
  rule. It collides with Canopy's `Columns` parts today.

**Docs (Stage 7):**

- fh `architecture.md`:
  - "modal scope" and "exclusive": lines 131-132, 223, 232-235, 248, 276-283,
    and 339-342.
  - "slot" for the overlay: line 274.
  - "widget action": lines 65-66.
  - "input mode": line 44.
  - `canopy.flush()`: line 371.
  - The stale footer statement: lines 100-102.
  - API mode through `launch`: line 424.
- `README.md:59` ("widget action").
- `find.md:201` (the node name).

#### C51: fh adopts the new capabilities

*New. Stage 6.*

This is the fh side of C31 to C48. Each item deletes fh code that works around
a Canopy gap:

- The fields become `Input` nodes (C40): about −220 lines.
- The columns dialog becomes a `Dialog` and gains margin click swallowing
  (C32): about −70 lines.
- The Confirm, Picker, and List keymaps go (C42): about −175 lines.
- `CommandEnum` derives and `toggle_column(Column)` (C44): about −70 lines.
- The find keys become navigation intents (C34): about −50 lines.
- The workers wake on send, `request_poll` replaces the self-wake handles, and
  terminals report exit (C43).
- DiffPane goes, and fh mounts DiffView directly (C46): about −250 lines.
- Footer hints resolve through `KeyHint::for_command`, and `set_footer_hint`
  goes (C31).
- canopy-fileselect styles itself through `Setup::widget_styles`, and the
  picker styles move to fh (C31).
- Notices replace the hand-built error messages (C39).
- Configuration goes through the C41 helper.

### Considered and rejected

- **Fully unifying intents and commands.** Commands need nominal dispatch, and
  disabled commands must keep consuming their key. Intents need polymorphic,
  fall-through dispatch. C10 renames and trims instead.
- **Deleting `FrameworkWithActions`, List checks, the Picker overlay and filter
  methods, and the DiffView prepared-content constructors.** fh uses them, so
  they carry real needs. C10, C30, C42, C46, and C48 reshape them instead.
- **Merging `canopy-geom` into `canopy`.** Only `canopy::geom` consumes it,
  but the crate boundary keeps geometry a dependency-free leaf, and users
  never see the crate name.
- **Splitting scripting out of `canopy`.** The Luau surface is generated from
  command metadata and is part of the core contract.
- **Flattening `core/` or renaming `world/` to `core/`.** That means about 90
  file moves, and C12's import-path fix removes the visible symptom. Revisit
  after Stage 4.
- **One reveal method with an enum target.** The three targets differ: a
  canvas rectangle, an anchor the widget computes after layout, and a node in
  its ancestor views. The reasoning moves to `architecture.md` with C38.
- **Renaming `ViewContext` to `ReadContext`.** 393 references, with no gain
  once the vocabulary is documented.
- **Merging `eval` and `eval_with_cancellation`.** Both are thin.
- **A single `canopy.call(target, id, ...)`.** A leading optional target plus
  varargs types badly in Luau.
- **Deleting `BackendControl`.** Its two test implementations exercise session
  capability handling.
- **Renaming `event::mouse::Action`.** It is namespaced and standard; it has
  174 sites.
- **Merging Border into Frame, folding Dropdown into Selector, and replacing
  StatusBar with a row container.** Each has real, distinct behaviour.
- **Sharing `cargo_env` through a crate.** It would break the xtask leaf
  rule.
- **Commands spread across several impl blocks (fh).** It would let
  `FileSelect` and `Commander` split their single derive blocks. Sub-structs
  with thin command delegates (C49) are simpler than merging derive output.
- **Clamping scrolls lazily at the next layout (fh).** fh's docs tell scripts
  to prepare between loading a preview and scrolling it. `reveal_area` already
  covers "change content and show it in the same turn".
- **Narrowing implicit invalidation to Paint now (fh).** The saving is
  unmeasured, and widgets that change size would need an audit. C18 measures
  first, and C43 removes most of the polls that cause it.
- **A Canopy `Style::sgr()` helper for fh's `--default-config` highlighting.**
  It has one consumer.
- **Keeping styles setup-only.** fh and the gyms switch themes at runtime
  (C5).

## Execution Plan

Every stage migrates Canopy and fh together. A stage is done when all of the
following pass:

- `ncode check` and `ncode test` in both repositories.
- `ncode api` in both repositories, with the capture diffs reviewed.
- `cargo xtask smoke` in Canopy, and `canopyctl smoke` in fh.

Each stage also updates the docs it touches.

### Stage 1: Defects, hygiene, and factual docs

- [x] C1:
  - Add `Core::effective_transient_mode` and `Core::route`, and use them in
    `canopy/routing.rs`, `keyroute.rs`, `help.rs`, and
    `canopy-widgets/src/help/mode.rs`.
  - Add a regression test for a transient mode under an `Application` modal.
  - Keep framework-group modals unchanged; all three fh modals are of that
    kind.
- [x] C2: add an exhaustive `Error::script_kind()`, use it from
  `script/errors.rs` and `canopy-mcp/src/script.rs`, and add a typed
  `ScriptBusy`.
- [x] C3:
  - Re-export `Answer`.
  - Measure Selector glyphs.
  - Make `BindingTargetKind::of` `pub(crate)`.
  - Replace the `first_leaf` fallbacks with `focus_first` (`columns.rs`,
    `listgym.rs`).
  - Fix the `clear_bindings` doc.
  - Reject omitted children in `set_children`.
  - Add the palette-based DiffView theme paths.
- [x] C35: delete `Error`, set resolver 3, fix CI drift, unify manifests, and
  clean the stale tmp directory.
- [x] C38: retire `docs/api-budget.md`. Move its surface rules, the
  scroll-and-reveal and intent-catalog reasons, and the dependency-coupling
  note into `architecture.md`, then delete the file. Doing this first spares
  every later stage from updating it.
- [x] C38 Stage 1 facts:
  - Fix `architecture.md`, `scripting.md`, `fixtures.md`,
    `getting-started.md`, `agent-loop.md`, and `README.md`.
  - Delete `plans/mouse-activation.md`.
  - Fold the scroll rounding sentence into `architecture.md`, and delete
    `plans/scroll.md`.
- [x] C50 defects (fh): add the `fh/diff/loading` rule, and drop
  `derive_commands` from DiffPane.

### Stage 2: Internal simplification, with no public API change

- [x] C18 internals:
  - Collapse `ChangeSet` to one level and replace `render_pending`.
  - Document the whole-frame repaint invariant.
  - Measure per-turn layout cost on fh's tree.
- [x] C22:
  - Split `backend/crossterm.rs` and `canopy/mod.rs`.
  - Share one work selector with `HeadlessEval`.
  - Remove the duplicates.
  - Fix `bench_layout`.
  - Tidy the testing module.
  - Delete `request_diagnostic_dump`.
- [x] C23:
  - Tree internals, including the subtree-only attachment refresh.
  - The symmetric `with_widget` and `with_widget_mut`.
  - Deferred focus repair inside callbacks, and a recovery hint on `detach`.
- [x] C24: one route walk, borrowed candidates, `CommandAvailability` without
  a lifetime, and no free commands.
- [x] C29: consolidate `base_api.rs` and `value.rs`, move the `bridge.rs`
  helpers, narrow visibility, and delete the native-module and startup-global
  plumbing.
- [x] C32 internal parts: `RowCursor`, `HelpPanel` removal, `ScrollAxis`
  reuse, the click threshold, and the shared highlighted-run painter. Replace
  the `editor` feature with a narrow `syntax` feature.
- [x] C37:
  - Trim the tidy hooks.
  - Move the Luau inventory into a test.
  - Apply the xtask fixes.
  - Delete fh's xtask.
- [x] C49 (fh):
  - Split `commander.rs`, canopy-fileselect's `lib.rs`, and `columns.rs`.
  - Switch the read-only callbacks to `with_widget`.
  - Add `fh::factory`, and shrink both public surfaces.

### Stage 3: Core API consolidation

- [ ] C6: one `CommandCall`. Update the derive codegen, `Context::dispatch`,
  `command_status`, Button, List, routing, and canopy-fileselect's two sites.
- [ ] C7: remove list-row injection, `dispatch_scoped`, and
  `current_list_row`, and seal `Inject`.
- [ ] C44: snake_case `CommandEnum`, generated-function visibility, and
  `spec_*`. fh derives its enums and adds `toggle_column(Column)`. Lowercase the
  enum strings in Luau, including 6 sites in fh's `default_config.luau`.
- [ ] C8, C9: `BindingTier`, a non-optional stored phase, `StepBinding`,
  `RouteWinner`, and `RouteTraceKind` with snake_case labels. Update the Luau
  records, and fh's 8 `exclusive_group` reads.
- [ ] C45: `key_outcome` returns `EventOutcome`. Delete the provisional and
  gap machinery, add the prediction check to routing, and delete the
  boilerplate implementations in both repositories.
- [ ] C11:
  - `Setup::bind`, which absorbs `bind_framework` (3 fh sites).
  - Mode renames in Rust and Luau, including fh's 27 `input_mode()` sites.
  - Delete `bind_mouse`.
- [ ] C5:
  - The `Setup` handle and the `Register` trait. Retire `Canopy::new`.
  - `HarnessBuilder::register::<W>()`, keeping `Harness::from_canopy`.
  - Migrate fh's 3 `Loader` impls, the style installers, and 61 harness
    builders.
- [ ] C47: `Harness::wait_until` and `with_unique`. Replace fh's sleep loops
  and private poll calls.
- [ ] C39: notices, `Error::App`, and Root's notice display. Delete fh's
  error-to-footer plumbing.
- [ ] C13: apply the node-addressed rule to the four context traits. Migrate
  about 150 Canopy sites and about 50 fh sites, using
  `focused_node() == Some(id)` for `is_focused_of`.
- [ ] C14: remove the boxed add variants, `Canopy::create_detached`, and
  `compose`, and move `KeyedChildren` into canopy-widgets.
- [ ] C15: delete `Display` and the `set_layout` and `with_layout` families.
  Add `From<Layout> for LayoutOverride`, and fix `columns.rs:780` pinning.
- [ ] C16: the `descendants::<W>` iterator, removal of the unused lookups, and
  the `focused_within` rename.
- [ ] C17: `ScrollOp` with counted lines and pages and a one-line page overlap.
  Replace the five direction matches, and delete fileselect's
  `with_preview_view`.
- [ ] C18 public part: `ChangeSet` and `Invalidation` are already crate-private
  (done in Stage 2). Delete `invalidate_layout` and its 16 calls, including fileselect's
  `lib.rs:1772-1775` block.
- [ ] C21: merge the error variants, and rename `RunLoop` to `Driver`.

### Stage 4: Vocabulary and module homes

- [ ] C4: slot and widget-cell renames, identity renames, script origin, and
  screen size.
- [ ] C10:
  - Rename widget actions to intents across both repositories, and rename the
    clear intent to `canopy.clear`.
  - Widgets register the intents they implement.
  - Also `BindingAction`, `activation_status`, `ModalToken`, and
    `world/modal.rs`.
- [ ] C19:
  - Pipeline renames, `Arc<TermBuf>` snapshots, and one flush per frame.
  - Luau `canopy.prepare()` replaces `canopy.flush()`: 26 Canopy sites and 73
    fh sites.
  - Narrow `TermBuf` and `StyleManager` visibility, and add
    `StyleMap::resolve`.
- [ ] C20: `TurnInput` and `PollLifetime`. `RunOptions` moves into
  `LaunchMode::Run`, `LaunchMode::Api` goes, and `launch` returns `ExitCode`.
- [ ] C43 runtime part: `wake_channel` and `Context::request_poll`. fh's three
  polling workers and its self-wake handles switch over.
- [ ] C12: create the module map in `crates/canopy/src/lib.rs`. Fix the
  `crate::core::` capture leaks, add a check that fails on them, and move
  `NodeName` to `path.rs`. Migrate about 25 fh import blocks.
- [ ] C50 renames (fh): dotfiles, `sync_panes`, `WidthPolicy`, `find_field`,
  `WatchContext`, `testdata`, and `columns_dialog`.
- [ ] Rewrite the "Public API Surface" section of `architecture.md`.

### Stage 5: Scripting and automation surface

- [ ] C25: positional owner functions. Delete `cmd`, `cmd_on`, and the
  inference code, and rewrite the `scripting.md` Commands section.
- [ ] C26:
  - Delete `screen`, `screen_cells`, and the unused lookups.
  - Add `screen_text(target?)`, `move_focus`, `canopy.fixtures()`, and
    `canopy.target`.
  - Migrate the smoke scripts in both repositories, including fh's 2
    `screen_cells`, 5 `node_region`, and 1 `fixtures()` sites.
- [ ] C27: `EvalRequest::new`, one log channel, one typecheck, and a
  `testing`-gated `eval_script`. Delete `evaluate_live` and the `execute`
  timeout parameter.
- [ ] C28:
  - `ResetPolicy { Isolated, External }` plus `ExecutionMetadata.fixture`.
  - `EvalReport`, `--replay-out`, `instance_id`, `source`, and `screen`.
  - Remove the fixtures tool, and rename builder `script` and `script_file`.
  - Update fh's `tests/smoke.rs` and its harness `bindings()` call.
- [ ] C41: `ConfigHome` and `CanopyBuilder::user_config` in canopy-mcp.
  Convert hello, todo, and fh, and drop `--no-config` from the
  `.canopyctl.toml` files.

### Stage 6: Widgets and styling

- [ ] C30: keep List checks, return Selector to single choice, standardize the
  check vocabulary, and split `checked_keys` from `selected_keys`.
- [ ] C31:
  - Apply the styling rule, and remove the double prefixes.
  - StatusBar pushes its layer, and KeyHint resolves keys through bindings.
    Add Luau `canopy.key_for`.
  - `roles` becomes shared part names, `style::themes` has a public `Palette`,
    and `Setup::widget_styles` is added.
  - `PartialStyle` gains chaining methods, and `StyleBuilder` goes.
  - Update `themes.golden`.
- [ ] C32 public parts: public `Dialog`, `Render::runs`, `text::width` and
  `cell_width`, the `highlight` module, Border naming, and the Container
  presets.
- [ ] C33: the module visibility rule, homes for the root names, method
  renames, the must-use lint, unused-method trims, `ItemLabel`, the Editor
  interaction field, and `text_buffer` narrowing.
- [ ] C40: a complete `Input` with command notifications. PickerFilter becomes
  an `Input`.
- [ ] C42: widget framework groups (`Confirm`, `Picker`, `List`), the
  multi-group `ModalBindings`, `Confirm::open`, and `Picker::set_commands`.
- [ ] C34: the cursor command set, `Tabs::cycle`, and the navigation intents
  with the runtime scroll default.
- [ ] C43 widget part: `TerminalConfig::with_on_exit`.
- [ ] C46: complete DiffView, add `Spinner`, and rename `DiffModel`.
- [ ] C48: run the nested-modal spike. Adopt the overlay layer only if its
  invariants hold.
- [ ] C51: fh adopts C31 to C48 and deletes its workaround code.

### Stage 7: Repository structure and docs

- [ ] C36:
  - Move the gyms to `examples/gyms`, driven through `launch` with a
    `.canopyctl.toml`.
  - Delete the `widget` showcase.
  - Make binding scripts start with `root.default_bindings()`, in both
    repositories.
- [ ] C38 Stage 7: rewrite the "Public API Surface" section of
  `architecture.md` around the final module map and the surface rules. Update
  `styles.md`, and add the `AGENTS.md` map.
- [ ] C50 docs (fh): update `architecture.md`, `README.md`, and `find.md` to
  the vocabulary.
- [ ] Final validation: all captures are current in both repositories, and
  the checks, tests, and smoke suites pass in both.
