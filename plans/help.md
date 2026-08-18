# SPEC: Contextual key-binding help

## Description

Replace Canopy's experimental help path with contextual binding discovery that uses the same
precedence rules as input routing. `Root` opens a synchronous, isolated modal for the pre-help
focus. The modal shows each effective binding once, explains fallback bindings accurately, and
restores the prior context when it closes. This is a clean break with no compatibility layer.

## User Experience

- Every Root-based demo binds `?` to `root.toggle_help()` as a global application binding.
- `?` opens help before an editor, terminal, input, or active application mode can consume it.
- The modal lists bindings for the focus and mode stack that existed before help opened.
- `Up`, `Down`, `k`, and `j` scroll by one line. `Esc` closes. `?` also toggles the modal closed.
- `PageUp`, `PageDown`, `Space`, `Home`, `End`, `g`, `G`, the mouse wheel, and the scrollbar are
  secondary controls.
- Closing restores the exact prior focus when it is still valid. A stale focus falls back within
  its prior pane before Canopy selects the first visible application focus target.
- Long text wraps. The panel does not need horizontal scrolling and remains safe down to a 1-by-1
  terminal.

The important behavior changes are:

| Current behavior | Proposed behavior |
| --- | --- |
| Help reconstructs matches with rules that differ from routing. | Routing and discovery use one ranking implementation. |
| Snapshot delivery uses global pending slots and render polling. | `Root::show_help` captures and installs an owned snapshot synchronously. |
| Help focus does not block loose application bindings. | A Root-owned exclusive frame blocks all application binding tiers. |
| Closing selects the first application node. | Closing restores the saved focus or a defined pane-local fallback. |
| The panel has a fixed 50-by-20 size. | The panel uses bounded flex sizing and safe clipping. |

## Binding Model

Move `InputMap` into `Core`, next to `CommandSet`. `Context` can then query and control binding
resolution without a callback into `Canopy`. Keep the input map outside structural tree-edit
rollback. A failed widget-tree edit must not revert application bindings or exclusive tokens.

Make one binding record the source of routing and introspection metadata:

```text
BindingRecord
  id: BindingId
  input: InputSpec
  owner: application | framework(group)
  scope: global | mode(name) | default | exclusive(group)
  path_matcher: PathMatcher             # compiled and original filter
  description: String                   # required user-facing text
  source: String?                       # diagnostic provenance
  target: script(LuauFunctionId) | command(CommandInvocation)
  insertion_id: u64
```

Application bindings use script targets. Root installs help controls as framework-owned command
targets. Application clear, unbind, script invalidation, and reload operations cannot remove
framework records. Script callback cleanup removes only script targets. Framework groups and
records are not available through application mutation APIs.

Add these Rust-only interfaces:

```text
Canopy::bind_framework(
  group: FrameworkBindingGroup,
  input: InputSpec,
  path: &str,
  description: &str,
  command: CommandInvocation,
) -> Result<BindingId>

Context::push_exclusive_bindings(
  group: FrameworkBindingGroup,
) -> Result<ExclusiveFrameToken>

Context::pop_exclusive_bindings(token: ExclusiveFrameToken) -> Result<()>

Context::take_mouse_capture() -> Result<Option<NodeId>>

Context::restore_mouse_capture(node: NodeId) -> Result<ChangeOutcome>

Canopy::unbind(id: BindingId) -> Result<bool>
```

`bind_framework` creates a `framework(group)` record in `exclusive(group)` scope. An exclusive
frame considers records only when their owner and scope use its group. These interfaces are public
Rust APIs because `Root` is in `canopy-widgets`. They are absent from Luau and generated scripting
records. Root is their only initial caller.

Re-registering an identical framework record returns its existing ID. A conflicting record with
the same group, input, and path returns an error. This makes repeated `Root::load` calls idempotent
without allowing ambiguous framework controls. Routing dispatches a command target from the
resolved route node with the same command scope that a script target receives.

`Canopy::unbind` becomes fallible because the complete registry exposes framework IDs. It returns
an error for a framework-owned ID and `Ok(false)` for a missing application ID. Input selectors and
`clear_bindings` inspect and remove application records only. Startup-script rollback restores only
application records and releases only script targets. It does not replace framework records or the
exclusive-frame stack.

Resolution has these policies:

1. If an exclusive frame exists, only records in the newest frame's exclusive group participate.
2. Otherwise, resolve the global tier, active modes from newest to oldest, and the default mode.
3. Within one scope, use `PathMatch::score()` and then latest insertion.
4. At each route node, classify an anchored winner as `before_widget` and another winner as
   `after_ignore`.
5. Stop at the first route node with a winner, exactly as `route_input` does.

A global binding must use a start- and end-anchored path filter. The demo help binding uses
`/root/**/`, so it wins on the initial focus path. Active modes cannot shadow it. This tier is
necessary even though the current demos do not use application modes. Active modes resolve before
the default mode and could otherwise replace the help key.

Exclusive frames use a separate runtime stack and opaque tokens. Each frame records the `NodeId`
that pushed it. Popping a token removes only its frame. Application `set_mode`, `pop_mode`, clear,
and reload operations do not alter this stack.

Successful detach, removal, or root replacement removes frames whose owner is no longer attached.
A failed tree edit keeps them because the owner is restored. While help is open, its `root.help`
group blocks global, active, and default bindings even when no help binding matches.

Use one internal candidate-ranking function for all consumers. Routing selects the first winner
without allocating a diagnostic list. Contextual help projects one winner per normalized key.
Diagnostics can enumerate the same ranked candidates and mark the winner. They explain whether
mode, path, insertion order, an earlier route node, or an exclusive group shadowed each loser.

## Scripting Interface

Use the compatibility break to make application bindings self-documenting:

```luau
canopy.bind("?", {
    description = "Show key bindings",
    path = "/root/**/",
    tier = "global",
}, function()
    root.toggle_help()
end)
```

- `canopy.bind` and `canopy.bind_mouse` take a required options table and a callback.
- `description` is required, non-empty text. Remove `desc`.
- `path` and `mode` remain optional. `tier = "global"` is optional and cannot be combined with a
  named mode.
- Invalid input fails during script evaluation and includes the binding source location.
- Delete `bind_with` and `bind_mouse_with` instead of keeping aliases.
- Use a separate selector type for unbind operations. An unbind selector does not include a
  description.
- Keep captured script source in `source`. Never use source text as a fallback description.

`canopy.bindings()` returns the complete key and mouse registry, including owner and target kind,
for diagnostics. Application mutation APIs reject framework-owned records.

## Contextual Introspection

Replace the borrowed and owned help types with one owned snapshot:

```text
BindingSnapshot
  focus: NodeId
  focus_path: Path
  active_modes: [String]                # resolution order
  exclusive_group: FrameworkBindingGroup?
  bindings: [AvailableBinding]

AvailableBinding
  id: BindingId
  key: Key
  description: String
  owner: application | framework(group)
  scope: global | mode | default | exclusive(group)
  mode: String?
  path_filter: String
  route_path: Path
  phase: before_widget | after_ignore
  source: String?
```

For each normalized key in an eligible scope, availability starts at the captured focus. It walks
to the root and applies the routing rules above. It returns only the first effective binding for
that key. Inactive, lower-priority, and shadowed records stay available through
`canopy.bindings()`. They do not appear in the contextual snapshot.

Expose the operation as `Canopy::available_bindings(Option<NodeId>)`,
`Context::available_bindings(Option<NodeId>)`, and Luau `canopy.available_bindings(node?)`. It
always describes the specified node or current focus with the live mode and exclusive-frame state.
While help is open, the no-argument form therefore describes the live help context and names the
`root.help` group. An explicit application node can still produce no bindings because the
exclusive help frame blocks application tiers. The modal uses its owned pre-open snapshot.
Automation that needs that application snapshot captures it before opening help.

If no node and no current focus exist, use the root node. Reject an explicit missing or detached
node instead of returning a partial snapshot.

The diagnostic dump also prints the active exclusive group. Candidate diagnostics use `blocked by
exclusive group` when the frame makes a global, mode, or default record ineligible. An empty live
snapshot must therefore distinguish an isolated context from a context with no registered keys.

Mouse bindings are not part of this focus-derived snapshot. Their availability depends on pointer
position and hit testing. Command availability remains a separate introspection product.

## Root Modal Lifecycle

Replace independent help booleans and global snapshot slots with Root-owned state:

```text
HelpState
  Closed
  Open {
    origin_focus: NodeId?
    origin_pane: NodeId?
    exclusive_token: ExclusiveFrameToken
  }
```

`Root::show_help` performs one synchronous transaction:

1. If help is already open, return successfully without changing the snapshot or scroll.
2. Capture the current focus, containing pane, mode stack, and owned binding snapshot.
3. Install the snapshot on `BindingList` and reset its scroll to the top.
4. Push the `root.help` exclusive frame and keep its token.
5. Take and clear any existing mouse capture.
6. Make the overlay visible, dim `MainPane`, and focus `BindingList`.

`take_mouse_capture` clears and returns the current capture. `restore_mouse_capture` rejects a
missing or detached node. If any open step fails, remove the new exclusive frame, restore the
prior mouse capture, visibility, and focus. Restore the snapshot and scroll position, and leave the
state closed. A successful open cancels the old mouse gesture permanently. Closing help does not
restore its capture.

The flow does not need a request, response, drain, render poll, observed flag, or second render
pass. It works for a routed binding, a normal script command, automation, and tests. The snapshot
is ready before `show_help` returns.

`hide_help` hides and clears the overlay. It removes only the stored exclusive token and restores
the origin. If the origin is not attached, visible, and focusable, it tries the saved application
or inspector pane. It then tries visible `MainPane`. `toggle_help` closes an open modal. Quit uses
the same close path.

Successful Root removal uses the exclusive owner's liveness cleanup. Failed Root removal leaves
the Root and its exclusive frame intact. Reopening always captures a fresh snapshot.

Root installs the `root.help` framework group once when it loads. The exclusive frame makes only
these `/root/help/**/` bindings eligible:

- `Up` and `k`: scroll one line up.
- `Down` and `j`: scroll one line down.
- `PageUp`: scroll one viewport up.
- `PageDown` and `Space`: scroll one viewport down.
- `Home` and `g`: go to the first row.
- `End` and `G`: go to the last row.
- `Esc`: close help.
- `?`: toggle help closed.

These records use command targets, not Luau callbacks. They are absent from the main list because
capture occurs before the exclusive frame becomes active. The fixed footer documents them. The
application owns the global help trigger, so loading Root never chooses a trigger key.

## Modal Presentation

Keep `HelpOverlay` as Root's final stack child. It fills the terminal, paints an opaque modal
surface, and is the top hit-test target. Only `MainPane` receives the dim effect.

```text
HelpOverlay [name: help]
  Modal [name: modal]
    Frame "Key bindings" [name: frame]
      HelpPanel [name: help_panel]
        BindingList [name: binding_list]
        ControlFooter [name: help_footer]
```

Root's node name remains `root`. These names make `/root/help/**/` the authoritative filter for
all focused descendants of the overlay.

The frame fills small screens and is capped near 72 columns by 28 rows on larger screens. Use a
one-cell screen margin when space permits. Do not set a minimum size that can exceed the terminal.
At very small sizes, omit navigation groups that do not fit in the footer, keep the close guide
visible, and reserve the remaining area for the list.

The primary list shows `before_widget` bindings. A labeled, muted section shows `after_ignore`
bindings with text such as
`When the focused widget does not handle the key`. This preserves discoverability without
claiming that a stateful widget will ignore the event. Do not add a static widget-consumption API
or execute widget handlers during introspection.

Sort rows by lowercase letters, uppercase letters, digits, arrows, special keys, and modifier
chords. Use display-cell width. Use aligned key and description columns at normal widths. Put the
description on an indented continuation line at narrow widths. Wrap all descriptions and context
text. Show `No key bindings in this context` when both sections are empty.

Keep the header and footer fixed. `BindingList` owns the exact scroll canvas. It clamps scroll after
content and viewport changes and shows a compact vertical position indicator. It handles mouse
wheel input and indicator clicks without propagation.

The enclosing `Frame` is decorative. Its direct child is `HelpPanel`. Thus, the list does not use
`Frame`'s direct-child scrollbar or drag state. The footer shows the available subset of
`Up/k  Down/j  PgUp/PgDn  Home/End  ?/Esc close`.

## Success Criteria

- Resolver tables prove global, stacked-mode, path, insertion, route-node, and phase precedence by
  comparing availability with route traces. An active mode cannot shadow the global `?` binding.
- Root tests prove synchronous capture from binding and non-binding commands. They cover exact
  focus restore, stale-origin fallback, idempotence, token balance, and all failure paths.
- Isolation tests prove that background bindings, widgets, and a previously captured mouse target
  receive no input while help is open. A failed open restores the prior capture.
- Buffer tests cover normal, narrow, tiny, empty, long, wide-key, scroll, and resize states without
  overflow or panic.
- Every Root-based demo and Todo installs exactly one global `?` trigger. Terminal and editor demos
  open help from a consuming widget. They scroll, close, restore focus, and accept the next input.

## Scope Boundaries

- This facility lists bindings, not commands. A command palette is a separate capability.
- The contextual modal does not list mouse bindings.
- Discovery does not execute widget handlers or promise that an `after_ignore` binding will run.
- The exclusive-frame API is Rust-only, and Root is its first caller. General script-controlled
  modal isolation needs a second consumer and a separate design.
- The demos intentionally reserve literal `?` for help in all focused widgets. Production
  applications own their trigger and can choose a different global key.
- Canopy does not infer or require a help trigger. Root supplies the commands and framework modal
  controls. Application binding configuration decides whether and how users open it.
- Remove the experimental help types, pending snapshot protocol, `Ctrl+/` default, old binding
  functions, and old generated surfaces. Do not retain aliases or parallel paths.

## Execution Plan

### Stage 1 - Replace the binding registry and scripting contract

- [x] Define `BindingOwner`, `BindingScope`, `BindingTarget`, `BindingRecord`,
  `FrameworkBindingGroup`, and `ExclusiveFrameToken` in
  `crates/canopy/src/core/inputmap/mod.rs`.
- [x] Store `InputMap` in `Core` in `crates/canopy/src/core/world/mod.rs`. Keep it out of
  `TreeStateSnapshot` in `crates/canopy/src/core/world/tree.rs`.
- [x] Preserve application-only startup rollback and Luau target release in
  `crates/canopy/src/core/canopy/mod.rs`.
- [x] Add idempotent `Canopy::bind_framework`, fallible `Canopy::unbind`, and application-only
  selector and clear behavior in `crates/canopy/src/core/canopy/mod.rs`.
- [x] Dispatch `BindingTarget::Command` and `BindingTarget::Script` through the same event command
  scope in `crates/canopy/src/core/canopy/routing.rs`.
- [x] Add exclusive-frame, capture-transfer, and owner-liveness operations in
  `crates/canopy/src/core/context.rs`, `crates/canopy/src/core/world/focus.rs`, and
  `crates/canopy/src/core/testing/dummyctx.rs`.
- [x] Prune exclusive frames only after successful detach, removal, or root replacement in
  `crates/canopy/src/core/world/tree.rs`.
- [x] Replace the scripting parser and declarations in
  `crates/canopy/src/core/script/base_api.rs`, `defs.rs`, and `records.rs`.
- [x] Require `description`, support `tier = "global"`, retain `path` and `mode`, and delete
  `bind_with`, `bind_mouse_with`, and `desc`.
- [x] Keep `canopy.bindings()` complete. Expose owner, scope, description, source, and target kind.
- [x] Migrate binding scripts in `crates/canopy-widgets/src/root.rs`, `help/mod.rs`,
  `inspector/mod.rs`, and `editor/tests.rs`.
- [x] Migrate callers in `crates/canopy/src/core/canopy/tests.rs`,
  `crates/canopy/src/core/script/tests.rs`, and `crates/canopy/tests/it/{commands,script,viewport}.rs`.
- [x] Migrate `crates/canopy-mcp/src/server.rs` and `crates/examples/examples/widget.rs`.
- [x] Migrate `crates/examples/src/{chargym,editorgym,focusgym,fontgym,framegym,imgview,intervals,listgym,pager,stylegym,termgym,textgym,widget_editor}.rs`.
- [x] Migrate `examples/todo/src/lib.rs` without adding the help trigger until Stage 4.
- [x] Update the clean-break binding contract and examples in `docs/scripting.md`.
- [x] Add registry tables for normalization, tier order, replacement, and framework idempotence.
- [x] Test mutation isolation, token order, owner cleanup, and startup rollback in
  `crates/canopy/src/core/inputmap/tests.rs` and `crates/canopy/src/core/world/tests.rs`.
- [x] Add script tests for required descriptions, invalid option combinations, source metadata,
  removed functions, and framework-ID rejection in `crates/canopy/src/core/script/tests.rs` and
  `crates/canopy/tests/it/script.rs`.
- [x] Add route tests that compare script and command targets in
  `crates/canopy/src/core/canopy/tests.rs`.
- [x] Prove removal of the old API with
  `rg 'bind_with|bind_mouse_with|desc\s*=' crates examples docs`.
- [x] Run `cargo nextest run -p canopy -p canopy-widgets -p canopy-mcp -p canopy-examples -p todo`.
- [x] Run `cargo xtask luau` and inspect the canonical preamble and generated declaration tail.
- [x] Run `cargo xtask api`. Review every changed file under `api-surface/`.
- [x] Run `git diff --check` and review the coherent registry migration.
- [ ] Commit the coherent registry migration.

### Stage 2 - Share resolution and make Root modal state synchronous

- [x] Implement one allocation-free candidate ranker and one diagnostic enumerator in
  `crates/canopy/src/core/inputmap/mod.rs`.
- [x] Make `route_input` consume the shared winner and phase result in
  `crates/canopy/src/core/canopy/routing.rs`.
- [x] Replace `HelpSnapshot`, `OwnedHelpSnapshot`, and related binding types with
  `BindingSnapshot` and `AvailableBinding` in `crates/canopy/src/core/help.rs`.
- [x] Implement `Canopy::available_bindings` and diagnostic loser reasons in
  `crates/canopy/src/core/canopy/mod.rs`.
- [x] Implement `Context::available_bindings` in `crates/canopy/src/core/context.rs` and
  `crates/canopy/src/core/testing/dummyctx.rs`.
- [x] Replace `help_snapshot` with `available_bindings` in
  `crates/canopy/src/core/script/base_api.rs`, `defs.rs`, `records.rs`, and `mod.rs`.
- [x] Remove pending help request, snapshot, and observed state from
  `crates/canopy/src/core/world/{mod.rs,tree.rs,tests.rs}`.
- [x] Remove help fulfillment and second-render polling from
  `crates/canopy/src/core/canopy/{mod.rs,routing.rs,rendering.rs}`.
- [x] Replace `help_active` with `HelpState` and make `Root::show_help` transactional in
  `crates/canopy-widgets/src/root.rs`.
- [x] Install the owned snapshot directly on `BindingList`. Restore focus by saved pane policy.
- [x] Register the idempotent `root.help` command bindings after `Help::load` in
  `crates/canopy-widgets/src/root.rs`.
- [x] Remove `help.default_bindings()`, the `Ctrl+/` trigger, and the Luau help-control script from
  `crates/canopy-widgets/src/{root.rs,help/mod.rs}`.
- [x] Keep a minimal `BindingList` renderer in `crates/canopy-widgets/src/help/mod.rs` until Stage 3.
- [x] Add resolver parity tables for global, stacked-mode, path, insertion, route-node, phase, and
  exclusive precedence in `crates/canopy/src/core/inputmap/tests.rs` and canopy routing tests.
- [x] Add Rust and Luau availability tests for fallback, invalid nodes, modes, and isolation.
- [x] Test complete registry output and diagnostic reasons.
- [x] Add Root tests for synchronous entry, idempotence, focus restoration, and stale fallback.
- [x] Test capture transfer, failure compensation, token balance, and Root removal.
- [x] Update `docs/agent-loop.md` to use `canopy.available_bindings()`.
- [x] Update `docs/architecture.md` for Core-owned bindings, exclusive frames, and synchronous help.
- [x] Prove protocol deletion with
  `rg 'help_snapshot|pending_help|OwnedHelp|HelpBinding' crates docs`.
- [x] Run `cargo nextest run -p canopy -p canopy-widgets`.
- [x] Run `cargo xtask luau` and `cargo xtask api`. Review declaration and Rust surface changes.
- [x] Run `git diff --check` and review the coherent lifecycle migration.
- [ ] Commit the coherent lifecycle migration.

### Stage 3 - Build the responsive, scrollable help presentation

- [x] Keep overlay construction and public exports in `crates/canopy-widgets/src/help/mod.rs`.
- [x] Implement `HelpPanel` and `ControlFooter` in
  `crates/canopy-widgets/src/help/panel.rs`.
- [x] Implement row layout, phase sections, wrapping, scroll commands, wheel handling, and the
  clickable position indicator in `crates/canopy-widgets/src/help/binding_list.rs`.
- [x] Give the help nodes the exact names `help`, `modal`, `frame`, `help_panel`, `binding_list`,
  and `help_footer`.
- [x] Keep `Frame` decorative. Do not route help scrolling through its direct-child scrollbar.
- [x] Fill the overlay, cap large panels, use optional margins, and avoid unsafe minimum sizes.
- [x] Use display-cell widths and the specified deterministic key ordering.
- [x] Add `help/overlay`, `help/panel`, `help/key`, `help/label`, `help/fallback`, `help/footer`,
  `help/footer/key`, `help/footer/label`, and `help/indicator` rules in
  `crates/canopy/src/core/style/palette.rs`.
- [x] Remove obsolete `help/content` style rules after every renderer uses the new names.
- [x] Add focused layout and scroll unit tests in `crates/canopy-widgets/src/help/tests.rs`.
- [x] Add exact buffer tests for normal, narrow, 1-by-1, empty, long, wide-key, scrolled, and resized
  states in `crates/canopy-widgets/src/help/tests.rs`.
- [x] Test that wheel and indicator input is consumed and cannot reach the dimmed application.
- [x] Run `cargo nextest run -p canopy-widgets -p canopy`.
- [x] Run `cargo xtask api` and review `api-surface/canopy-widgets.rs` and `api-surface/canopy.rs`.
- [x] Run `git diff --check` and review the presentation.
- [ ] Commit the presentation.

### Stage 4 - Integrate applications and prove the complete facility

- [x] Add one shared global `?` binding installer in `crates/examples/src/lib.rs`.
- [x] Call it once from `crates/examples/examples/demo.rs` and
  `crates/examples/examples/widget.rs` after `Root::load`.
- [x] Remove Focusgym's local help binding from `crates/examples/src/focusgym.rs`.
- [x] Add exactly one global `/root/**/` help binding to `examples/todo/src/lib.rs`.
- [x] Add `crates/examples/src/tests/help.rs` and register it in
  `crates/examples/src/tests/mod.rs`.
- [x] Prove Termgym and WidgetEditor open from consuming widgets, scroll, close, restore focus, and
  accept the next input.
- [x] Prove both demo launchers install one global `?` record and no duplicate trigger.
- [x] Add `examples/todo/smoke/help_modal.luau` for open, inspection, scrolling, close, and restored
  input through the real smoke launcher.
- [x] Update `docs/scripting.md`, `docs/architecture.md`, and `docs/agent-loop.md` with final names,
  ownership rules, availability semantics, and automation examples.
- [x] Run `cargo xtask luau` to type-check every tracked Luau source.
- [x] Run `cargo xtask smoke` to execute the Todo help scenario.
- [x] Run `cargo xtask api`. Review all seven checked-in files under `api-surface/`.
- [x] Run `cargo xtask ci` with pinned nextest `0.9.99`, Ruskel `0.0.11`, and nightly
  `2026-07-01` available.
- [x] Run `git diff --check` and prove no obsolete API or protocol names remain with `rg`.
- [x] Review the full requirement-to-test mapping and inspect the final diff.
- [ ] Commit the facility.
