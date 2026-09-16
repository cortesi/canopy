# Activation, Buttons, and Mouse Bindings

## Description

Route button activation through the binding registry. Mouse clicks, focused
keyboard activation, and accelerators should invoke the same `Button::press`
command. Compose confirmation dialogs from real buttons so they share this
behavior, command eligibility, and focus handling.

Keys and mouse already share `Canopy::route_input_inner` and `InputMap`.
The gap is narrower: `Button::handle_click` consumes left-button presses before
an after-widget binding can run. `ConfirmBody` paints answers without child
nodes or click handling. Mouse bindings exist in `canopy.bindings()`, but
`available_bindings()` and contextual help currently expose only keys.

Keep the existing command and routing architecture. No generic activation trait,
second dispatcher, or replacement binding registry is needed. This is a proposed
change, checked against the current Canopy tree and sibling `fh` consumer.
Implementation and runtime acceptance remain outstanding.

## Changes

### C1: Activation is a command on a node

Keep `Button::press` and `with_command`. The stored action supplies arguments and
an optional exact target for each button instance. Removing it would move that
work into application bindings without removing the need for instance state.
Remove `handle_click` and the button's event override only when C2 is installed.

Make the declared eligibility of `press` reflect the configured action, using
the same status provider as widget semantics and disabled styling. Today `press`
delegates to `Context::dispatch`, which checks the target command's eligibility,
but `press` itself has no status declaration. Otherwise help would describe a
disabled button's activation command as enabled. Preserve direct `press` as a
no-op for a button without an action, and preserve direct command errors.

Keep disabled user activation inert and consumed. In the routed declarative
command executor, check availability within the event scope before dispatch and
consume a disabled winner without invoking it or falling through. Its reason
remains available through discovery. This policy also applies to other
declarative command bindings. Do not suppress errors from executed commands or
opaque script callbacks. Recheck at dispatch rather than trusting the last frame.

The original proposal to propagate disabled clicks would be a regression:
`backend/crossterm.rs` propagates `turn(work)` errors out of the terminal loop.
An ordinary click on a disabled control must not terminate the application.

Implement `Button::accept_focus` for buttons with configured actions. Leave
decorative, actionless buttons out of keyboard traversal. Disabled actions can
retain focus so users can inspect their reason. On enabled mouse activation,
focus the button before invoking the action, never afterward: the action can
close the modal, remove the button, or deliberately move focus. Direct and
keyboard calls do not implicitly steal focus.

Primary paths: `canopy-widgets/src/button.rs`, `canopy/src/core/help.rs`, and
`canopy/src/core/canopy/routing.rs`, all under `crates/`.

### C2: Default activation bindings

Provide described, declarative bindings for unmodified `LeftDown`, `Enter`, and
`Space`, targeting `button::press` from the route node. Match
`**/button/**/` so clicks on the label, border, and button area resolve to the
containing button. Use `after_widget`, retaining widget-first behavior by
default. Modified clicks require their own bindings, an explicit change from
`handle_click`, which currently ignores modifiers.

Use ordinary application defaults and explicit exclusive-modal bindings. Keep
the existing resolver. `bind_framework` accepts only `Exclusive(group)`, whose
records apply while its modal is active. Application bindings cannot override
or unbind that group.

Add `Loader for Button`, register its commands and a
`button.default_bindings()` script, and install those defaults in
application setup before user configuration. Registration alone does not execute
the script. Migrate stock entry points and test harness setup explicitly, including
users that previously needed no `Button` command registration for clicking.
Repeated loading must not duplicate records or undo user overrides.

For an exclusive modal, the owner explicitly installs the same three activation
bindings in its own group with a dialog-specific path. Do not admit unrelated
application defaults through an exclusive modal. In `fh`, extend
`CONFIRM_BINDINGS` in `../fh/crates/fh/src/commander.rs`.

This approach makes ordinary activation replaceable and removable through the
existing API. It retains protected controls where the host already requests an
exclusive modal. Do not add a protected framework fallback tier.

### C3: Mouse bindings admit `before_widget`

Remove both restrictions: `validate_phase` rejects registration and
`RoutedInput::allows_pre_event_binding` prevents early mouse dispatch. Keep
`after_widget` as the default. C3 can land independently of C1 and C2.

At each route node, resolve one winner, then execute its declared phase. Do not
search for a second binding when the winner is disabled or after a widget
consumes the event. Phases are local to each node in the bubbling route: an
ancestor's early binding does not precede a descendant's widget handler.

Keep hit testing, modal admission, capture ownership, node-local coordinates,
and wheel default actions unchanged. A matching early binding intentionally
preempts the widget at that node, including a terminal or scrollbar. Existing
after-widget configurations preserve their order. Verify these interactions in
`core/canopy/tests.rs`, `core/inputmap/tests.rs`, and script tests.

### C4: Availability includes mouse bindings

Add `BindingSnapshot::mouse_bindings`, leaving `bindings` key-only. Each mouse
record carries its normalized input and the same description, owner, scope,
route, phase, command availability, and source metadata as a key record.
Rust's input type is `event::mouse::Mouse`; `MouseSpec` is the script-facing type.
Reuse the existing resolver and factor shared snapshot construction.

Return one winner per normalized mouse input, using existing scope, path
specificity, and insertion-order rules. Sorting the resulting list is only for
stable presentation. It requires no new precedence rule or `Ord` on mouse specs.

The requested node, or current focus when omitted, is the hypothetical mouse
route start. This reports bindings for that context, not the pointer position
or every button in the application. As with keys, discovery cannot predict
whether an `on_event` handler will consume an after-widget binding. Capture and
hit testing still determine the actual route. Document this limit explicitly.

Update `core/script/records.rs`, `defs.rs`, and `base_api.rs` together with the
Rust snapshot. Extend `canopy-widgets/src/help/binding_list.rs` to display both
lists and disabled reasons. Preserve the help modal's captured origin context.
Use a canonical mouse input label consistently in script output and help.

### C5: Accelerators are real bindings

Add `Button::with_accelerator(char)` and a `roles::BUTTON_KEY` paint role beneath
the button's component and state layers. Use a small private label renderer if
needed, retaining the existing border and centering composition. General rich
text support is outside this change.

Highlight the first matching label character without changing its spelling.
For the initial API, match ASCII letters without case and other characters
exactly. Use grapheme boundaries and display-cell widths for painting and
clipping. A missing match leaves the label unchanged. Do not uppercase Unicode
text into a one-cell rectangle. Test wide and combining text, repeated letters,
and clipped labels.

The builder configures the visible mnemonic. The host or dialog explicitly
registers the key binding, its
scope, description, and target. The mnemonic is declared presentation metadata,
not a live view of arbitrary user rebinding. Document that limitation and keep
stock annotations and bindings together at their declaration site.

For `Confirm`, register static `y` and `n` bindings to separate `confirm::yes`
and `confirm::no` commands. Resolve the containing dialog from the input route,
then dispatch `Button::press` to its stored answer child. Each forwarding command
exposes that child's eligibility. Separate commands fit the current status API,
whose providers receive the widget and context but not invocation arguments.
This selects the right answer regardless of focus without storing a particular
dialog instance's node ID in a global binding.

Do not register bindings automatically from `on_mount` or infer a pane from a
path. Explicit registration avoids adding per-instance binding ownership,
collision handling, and cleanup on hide, detach, removal, and remount.

### C6: `Confirm` composes buttons

Replace the painted answers in `ConfirmBody` with a message and a centered row
of two `Button` children. Keep the rounded borders, spacing, shared background,
and start-truncated message. Use layout measurement for the message and button
row, with an explicit three-row button height and label-based width. Composition
still needs bounded, cell-aware label painting; it does not eliminate clipping.

Provide explicit Yes and No `CommandCall` configuration before opening the
dialog, with optional exact host targets. Store each action on its button.
The host continues to own the question's business state and modal token, and
its answer commands close the dialog. `Confirm` must not also close it.

Keep `Confirm::body()` returning the body node. Add a separate initial-focus
accessor that returns the configured answer button after mounting. Changing
`body()` to return Yes would silently change an existing API's meaning.
Add a configurable default answer, with No as the default; a host can select
Yes deliberately. Reopening applies that choice rather than retaining old focus.

Migrate `fh`'s bookmark dialog to configure both actions, use the new focus
accessor, and route `y`/`n` through the corresponding button. Keep `Esc` as
cancellation independent of answer focus. Preserve the modal's outside-click
barrier and prevent clicks in the gap or frame from answering.

Replace obsolete `/confirm/button/label` and `/confirm/key` painting rules with
the composed button roles. Add `/button/key` defaults and preserve focused and
disabled state styling. Update `style/palette.rs`, theme goldens, and
`docs/styles.md`; do not hand-edit generated captures.

### C7: Focus moves between answers

Bind `Left` and `Right` to spatial focus movement, `Tab` to Next, and
`BackTab` to Prev, matching the terminal's Shift-Tab representation. Implement
the dialog command with
`focus_move(FocusScope::Node(dialog), direction)`. Register these bindings in
the same admitted group as its activation bindings. Do not depend on Root's
application bindings inside an exclusive modal.

Preserve existing traversal semantics: Tab and reverse Tab wrap; spatial
movement stops at the edge. Enter and Space activate the focused button, while
`y` and `n` select an answer regardless of focus. Validate click-to-focus before
dispatch, focus restoration on close, nested modals, and repeated opens.
Tiny layouts must not focus or activate invisible answer nodes, leak input to
the underlying application, or prevent cancellation.

## Execution Plan

All implementation items remain unchecked. Preserve C1–C7 when revising this
plan, and record validation at each completed stage.

### Stage 1: Confirm design and enable mouse phases

- [x] Confirm C2: ordinary defaults and explicit exclusive-modal bindings.
- [x] Confirm C5: explicit accelerator registration with mnemonic highlighting.
- [x] Implement C3 in registration and shared routing.
- [x] Test early and late mouse bindings, winner precedence, coordinates,
  capture, modal isolation, and wheel fallback, including Luau registration.

Stage 1 also brought forward C4's canonical mouse label. `Mouse`'s `Display`
wrote `Left Down`, which `parse_spec` could not read back, and help splits
several inputs on a space. It now writes the spec it parses, `Ctrl+LeftDown`,
and `parse_spec` accepts `+` between modifiers as a key spec already does.

### Stage 2: Deliver button activation through bindings

- [x] Implement C1 and C2 together: eligibility, focus, disabled-input handling,
  loader/default setup, and removal of direct click handling.
- [x] Migrate stock consumers, examples, and harness loaders. Prove ordinary
  defaults and exclusive modal activation, plus override/unbind behavior.
- [x] Retain real mouse-event tests through the runtime. Cover label and border
  hits, sibling buttons, modifiers, non-activation events, disabled status
  changes, direct command errors, and actions that remove their own button.
- [x] Test disabled declarative bindings without fallback or execution. Prove
  action failures and opaque script errors still propagate and event scopes
  are restored after success, skipped activation, and failure.

`Root::load` loads `Button` and `root.default_bindings()` runs
`button.default_bindings()`, so every stock entry point that takes the root
defaults activates buttons. `HarnessBuilder::bindings` gives a test the same
step, since loading registers the script but does not run it. The decorative
buttons in the terminal demo keep consuming their clicks, because `press` on a
button with no action is an enabled no-op.

### Stage 3: Expose mouse activation in discovery and help

- [x] Implement C4 across Rust snapshots, Luau records/declarations, and help.
- [x] Test context-specific mouse winners, modes and exclusive groups, disabled
  reasons, stable ordering, captured help context, and unchanged key fields.
- [x] Update binding and routing contracts in `docs/scripting.md` and
  `docs/architecture.md`; regenerate affected API and script artifacts.

`AvailableBinding` took an input type parameter rather than gaining a twin, so
one definition and one conversion serve both lists. Its `key` field is now
`input`. Help sorts mouse rows after every key category and merges an input into
the action it shares with a key, so activation reads as one row.

### Stage 4: Compose and validate confirmation dialogs

- [ ] Implement C5–C7: mnemonic rendering and bindings, answer composition,
  configured actions/default focus, and scoped navigation.
- [ ] Test both answers by mouse, Enter, Space, and accelerator; test Esc,
  disabled actions, focus restoration, repeated opens, and nested modals.
- [ ] Preserve geometry/style tests for both borders, shared background, long
  messages, Unicode labels, tiny views, and clicks outside answer bounds.
- [ ] Migrate `../fh/crates/fh/src/commander.rs` and its bookmark tests. Verify
  exactly one action per activation, correct stored path, cancellation, and
  retained modal isolation. Include this sibling consumer in acceptance.
- [ ] Update durable style/API documentation and regenerate theme/API captures.
  Run focused checks for each change, then Canopy's `ncode test`, `ncode tidy`,
  `ncode api --check`, and `cargo xtask smoke`. Run fh's required checks and smoke
  suite from its own checkout, then `git diff --check` in both repositories.
