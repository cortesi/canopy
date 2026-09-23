# Canopy Architecture

Canopy is a terminal UI runtime. `Core` owns the node arena, layout, focus, mouse
capture, commands, and input bindings. `Canopy` owns `Core`, the style map,
rendering, polling, scripting, and fixtures. Widgets own local state. They use
context traits; they do not keep arena references.

Treat this file as the current contract. If it disagrees with code, fix one before
adding behavior.

## Public API Surface

Application code imports core types from the crate root and domain types from
their module; it selects `canopy_widgets` types directly. The root holds the
facade traits and their handle types: `CanopyBuilder`, `Setup`, `Register`,
`Canopy`, `Widget`, `Context`, `ViewContext`, `Render`, `NodeName`, `View`, the
capability context traits, typed node IDs, and the command macros. Value
libraries live in their modules:
`canopy::geom`, `canopy::layout`, `canopy::style`, `canopy::event`,
`canopy::path`, `canopy::commands`, `canopy::script`, `canopy::error`,
`canopy::help`, `canopy::keyroute`, `canopy::cursor`, `canopy::text`,
`canopy::render` (backend interfaces), and `canopy::terminal` (the Crossterm
run loop). The `testing` feature adds `canopy::testing`. Each item has one
canonical location, and there is no prelude.

An application runs in two phases. `CanopyBuilder::configure` passes a `Setup`
handle, which owns every registration: commands, bindings, widget actions,
startup scripts, fixtures, mode hooks, render limits, and the initial styles.
Each type registers what it needs in a `Register` impl. The builder then
finalizes the API and returns a `Canopy`, which has no registration methods.

`Canopy` is the running application. It owns `Core` and the style map, and its
fields are private. Apps install root widgets with helpers such as
`Root::install`, restyle at runtime through `Canopy::style_mut()` or
`Context::set_style`, and use `Canopy` methods for scripting, fixtures, input
modes, rendering, and automation. The `testing` feature adds hooks that tests
need and the builder cannot express: a manual clock, the event receiver, script
module invalidation, the journal limit, and layout timing.

Lower-level runtime state is crate-private. `Core`, `inputmap`, and raw arena
mutation are not reachable from app code, and `script` and the backend modules
expose only what the stable surface above needs.

Path-oriented APIs use `Path`, `PathFilter`, and `NodeName`. Literal path
components must be valid node names. Raw script path strings are validated at the
Luau boundary before matching.

Two rules keep the surface small. `Canopy` holds only operations that cannot
live on a context. A new context query or mutation replaces or generalizes an
existing one. The generated captures in `api/` record the surface, and every
public API change is reviewed as a diff of those captures.

`EvalTicket::completion` exposes `futures::channel::oneshot::Receiver`
directly. Evaluation completion is a single-consumer event with that receiver's
polling and cancellation semantics, so a wrapper would add surface without
changing the contract.

## Widget capabilities

`canopy-widgets` enables its complete bundle by default. Basic forms can set
`default-features = false`; Input, List, Root, Help, and Editor remain
available. The shared `canopy_widgets::text_buffer` module is independent of
Editor. Editor does not re-export text-buffer types; it uses `display_width`
internally.

Editor and DiffView highlight through any `editor::highlight::Highlighter` a
host supplies; neither needs a feature for that. `syntax` adds the built-in
Syntect-backed `SyntectHighlighter`, so only its dependencies and detection
tables are optional.

| Feature | Additional widgets and dependencies |
| --- | --- |
| `syntax` | `SyntectHighlighter` and its Syntect dependencies |
| `terminal-widget` | Terminal and `itty-core` |
| `graphics` | `ImageView` (crate-root re-export), fonts, `image`, and `fontdue` |
| `devtools` | Inspector and tracing subscriber support |

Without `devtools`, Root creates no Inspector nodes or Inspector commands.
Core scripting and the Crossterm adapter remain available in every profile.

## Tree Model

`Core` stores `Node`s in a `NodeArena<Node>` over `SlotMap<RawNodeId, Node>`. The
wrapper keeps the raw slotmap key private, so apps cannot forge `NodeId`s. A
`NodeId` is valid only while its node remains in the arena. Removed IDs are
invalid for application code, scripts, bindings, and tests.

The root node always exists. It has no parent and anchors the attached tree. A
node is attached when its parent chain reaches the root without cycles. Detached
nodes may exist during assembly or reparenting. The runtime does not render,
hit-test, or focus them.

A node stores a parent, ordered children, keyed children, a widget slot, layout
and view caches, and mount and polling flags. Parent links, child lists, and keys
must agree: parents list their children, children point back, and keys point only
at direct children.

## Structural Edits

`Context::edit_structure` runs its closure immediately. If the closure returns
an error, the runtime restores its structural checkpoint. A failed nested edit
restores its own checkpoint, even when the enclosing edit handles the error.

The checkpoint captures every arena node, including detached nodes. It restores
node metadata, topology, child keys, layout and view caches, and lifecycle flags.
It also restores root, focus, mouse capture, deferred focus repairs, exit
requests, and pending style changes.
Layout metadata includes the widget base layout and persistent override.

The checkpoint shares widget slots with the live arena. Widget-owned mutations
survive rollback. Binding registration and external effects, including database
writes, are also outside this guarantee. Callers must compensate those effects
or make them safe to repeat.

Rollback unwinds completed mounts in reverse order before restoring the checkpoint.
Cleanup hooks must be safe to repeat. A failed mount can run again on a later
attachment attempt with its previous widget mutations still present.

## Node Lifecycle

Nodes start detached. Attaching a subtree under an attached parent mounts its
unmounted nodes in pre-order.

Removing a subtree runs `pre_remove` in pre-order, runs `on_unmount` in
post-order, then deletes the nodes. Every `NodeId` in the subtree becomes invalid.

Replacing a subtree deletes descendants first, then replaces the target widget.
The node keeps its ID and resets mount and polling state. Its widget incarnation
changes, so old poll callbacks and wake handles cannot reach the replacement.

Detaching clears the parent link but preserves mounted widgets. Reattachment does
not repeat completed mount hooks. Layout caches refresh when the subtree returns.

Runtime-managed work declares `WorkLifetime::Node` or `WorkLifetime::Attachment`.
Node lifetime ends on widget replacement or removal. Attachment lifetime also ends
on detach. Reattachment starts a new attachment generation. An edit updates the
generations of the subtree it moves and no others. Hiding ends neither
lifetime. `Widget::poll_lifetime()` defaults to node lifetime, preserving detached
terminal polling. Attachment polling initializes again after reattachment.

Each successful `Widget::poll` result replaces its previous timer.
`Ok(Some(delay))` schedules another poll, with delays below one millisecond
rounded up to let the adapter sleep. `Ok(None)` cancels scheduled polling,
including when an early node wake triggered the callback. A poll that fails
with a notice keeps polling at the delay the last success requested; any other
failure is fatal. Explicit wakes remain immediate.

`Context::wake_handle(lifetime)` creates a `Send + Sync` `NodeWakeHandle` for the
current widget incarnation. Attachment handles require an attached owner.
`wake()` returns `Queued`, `Coalesced`, or `Expired` and requests one owner poll.
Producers keep results in their own bounded channels. The runtime coalesces wakes
per owner and stores no producer-result queue.

Structural success commits lifetime expiration and cancels obsolete polling.
Provisional registrations allow workers started by mount hooks to wake before
commit. Rollback removes provisional registrations and preserves valid earlier
wakes. Successful edits also retire modal scopes whose owner or modal widget is
detached or replaced.

## Invariants

`Core::validate_invariants()` checks invariants that do not mutate widgets. Every
layout pass ends with it, so any tree mutation that reaches layout is checked.
`Core` is crate-private, so only canopy's own tests call it directly.

It checks the root, widget slots, reciprocal links, duplicate children, cycles,
keys, focus, mouse capture, lifecycle flags, attachment generations, layout
caches, computed view caches, and the semantic-key index.

It does not run layout. Run layout before using screen coordinates.

## Widget Access

The runtime has three widget access modes.

Read access borrows a widget immutably. Layout refresh, measurement, canvas
calculation, cursor lookup, focus checks, and script node inspection use this
mode. Nested read access is allowed.

Render access borrows a widget mutably while holding only a shared `Core`
reference. This lets widgets render local cached state without mutating the tree.
It is separate from read access so render-only borrowing is visible in code.

Mutation callback access temporarily removes the widget from its node slot and
passes `&mut Core` to the callback. Event, mount, unmount, poll, command, and
test helper callbacks use this mode. Nested access to the same widget fails
instead of aliasing the widget.

`ViewContextExt::with_widget` is the typed read path. It borrows in place and
does not invalidate. `ContextExt::with_widget_mut` takes the widget cell for a
mutation callback and invalidates layout. Both check the node's widget type at
runtime and fail with `NodeTypeMismatch` for another type.

All widget access failures include the operation, node ID, node path, and source
error. Slot take and restore are safe code through `WidgetSlotGuard`. The only
`unsafe` in the runtime is the reentrant script bridge.

## Callback Mutation

Callback mutation is immediate. A widget callback can create, attach, detach,
hide, focus, capture, scroll, restyle, and dispatch during the callback. Later
code in the same callback observes the new state.

A callback cannot remove or replace the active callback subtree. Removing or
replacing the current node fails. Removing or replacing an ancestor that contains
the current node also fails. Canopy checks this before running lifecycle hooks,
so a rejected edit does not partially run `pre_remove` or `on_unmount`.

Removing or replacing siblings is allowed. Removing the mouse-capture node
clears capture immediately. Removed `NodeId`s become invalid immediately.

Focus repair after an edit inside a callback waits until the outermost callback
returns every widget cell. A widget whose cell a callback holds cannot answer
`accept_focus`, so an immediate repair would skip it, and often that widget is
the one that should take focus back. When the edit takes the focused node out of
the tree, focus clears at once and the edit records its recovery candidates.
Once the cells return, recovery picks among them, unless the callback focused
another node in the meantime. Focus that stays in the tree on a node that can
no longer hold it moves only when the cells return.

Use `Context::remove_after_dispatch(node)` when an active callback must remove
itself or an ancestor. The bounded FIFO records widget incarnations and drains
after successful outer dispatch, once active widget slots are restored. Admission
fails when the batch reaches capacity. A failed nested dispatch discards requests
added since its checkpoint. A failed outer dispatch discards its remaining batch.

Missing or replaced targets are harmless. A lifecycle veto stops the drain and
discards its tail. Earlier successful removals remain committed. Cleanup hooks
cannot enqueue another removal. Direct removal retains the restrictions above.

## Layout

Layout starts at the root with the terminal size. Each node gets an outer
rectangle relative to its parent's content origin. Padding produces content size.
The parent's direction, sizing, gap, alignment, display, and overflow settings
place its children.

`Layout::validate()` checks author-facing layout contracts: min must not exceed
max, flex weights must be non-zero, and padding arithmetic must not overflow.
The engine still uses saturating arithmetic internally so invalid or extreme
geometry does not panic.

Fixed outer sizes use `fixed_width()` and `fixed_height()`, which encode fixed
size as equal min and max constraints. There is no separate fixed-size enum.

`Layout::max_width_fraction()` bounds an outer width by a `Fraction` of the
parent's width budget: the parent's content width, less the gaps between its
displayed children in a row. The root uses the screen width. The parent
resolves the fraction once and applies the same bound when it measures the
child and when it allocates the child's final width, so a child's reduced share
never shrinks the bound again. The bound rounds down, joins `max_width` by
taking the smaller, and yields to `min_width`. A measurement without a width
bound reports the child's natural width, and the fraction applies once the
parent has a finite allocation.

Measurement is an infallible widget hook. A widget returns a fixed content size
or asks layout to wrap visible children. Layout may measure a widget several
times in one pass.

Canvas calculation is also infallible. It returns the scrollable content extent,
which is at least the content size. Layout clamps scroll after every pass.

Scrolling and revealing are separate. `Context::scroll()` and `scroll_node()`
apply a `ScrollOp` and move a view at once. A `ScrollOp` scrolls to an offset,
by an offset, or by a count of lines or pages. A page is the view extent less
one line, so consecutive pages keep one line of overlap. `Context::reveal_area()`,
`reveal_anchor()`, and `reveal_node()` queue requests that run after layout
settles the geometry they show, so a widget can change content and reveal it in
the same turn. `reveal_anchor()` asks `Widget::reveal_anchor()` for a rectangle
using the final content size. The editor reveals its cursor this way.

Neither operation can express the other: a scroll moves now, and a reveal waits
for layout. A scrollbar owner scrolls a node other than itself, so
`scroll_node()` takes a node. The three reveals take different targets: a
rectangle of the node's canvas, an anchor the widget computes after layout, and
a node in its ancestor views.

Each node holds one local request, an area or an anchor, and one request to
reveal the node in its ancestor views. A later call replaces its slot. Every
scroll and reveal takes an increasing call-order stamp, and each view records
the stamp of the last scroll or reveal it accepted, including one that needed
no movement. A focus change queues a nearest-edge reveal of the focused node.

After measurement and canvas sizing, layout publishes views and runs focus
recovery. It then applies the pending requests in call order. A local request
moves its own view. A node request starts with the node's outer rectangle in its
parent's canvas. At each view out to the active modal region or the root, it
moves the view, clips the area to what the view now shows, and translates the
result into the next canvas through the view's position and padding. A view
with a newer stamp keeps its offset but still clips the area. Layout then
publishes the moved views.

`RevealAlign::Nearest` makes the smallest move and keeps a view that already
lies inside a larger area. `RevealAlign::Center` centers each axis on which the
area is shorter than the view. A request waits while its node is hidden,
detached, zero-sized, or outside the active modal region. The stamps stop a
returning request from overriding newer intent. Removal discards requests,
replacement clears requests and stamps, and structural rollback restores them
with the node.

`Scroll` is a container whose canvas spans its children on the axes it
scrolls. Those axes measure children without a bound, and the other axis stays
bounded. Content that must grow uses measured or fixed sizing on a scrolling
axis; flex children share only the space that remains.

Hidden nodes do not participate in visible layout.
Layout clears their subtree caches.

Layout errors must surface. Re-entrant widget access and missing nodes must not
become zero measurements or fallback canvases.

## Scrollbars

Each node owns its canvas and scroll offset. Layout allocates the viewport, the
canvas defines the scroll range, and the widgets around a node display and
control that state. A node never draws its own scrollbar.

A widget that returns true from `Widget::owns_scrollbars()` displays positions
for its subtree. `Frame` is one. `scrollbar::scroll_target()` resolves the node
an owner draws for one axis. It starts at the owner's children and visits the
settled views. It skips nodes with no visible content, which include hidden and
display-suppressed nodes. It stops at a nested owner, so no node has two owners
on one axis, and it returns a node whose canvas exceeds its content on that
axis without descending further. Siblings combine: one result is unique, more
than one is ambiguous, and an ambiguous subtree makes every enclosing result
ambiguous. An ambiguous or empty result draws nothing. The target's viewport is
its content rectangle, clipped by every ancestor's content and by the screen.

A frame draws the vertical target on its right border and the horizontal target
on its bottom border. The track covers only the border cells beside the target's
viewport, so corners, tab bars, headers, and footers stay plain. A track exists
only when the target reaches the border: every node from the target up to the
frame's child ends at its parent's content edge on that side. A sidebar beside
the target therefore removes the vertical track.

A thumb covers whole cells. Both of its ends round to the nearest cell, and a
thumb is at least one cell long. A drag inverts the same rounding, so the thumb
stays under the pointer and its last position reaches the final offset.

`Columns` also owns scrollbars. It places panes in a row with a one-cell gap
after each pane and one trailing column, and draws a divider in each of those
cells. Each divider displays the vertical target in the pane on its left: the
pane itself when it overflows, or the node that target resolution finds within
the pane. The adjacency and projection rules above apply to the divider, so a
pane's header and footer rows keep plain divider lines. `Columns` draws no
horizontal tracks; horizontal overflow scrolls through wheel input.

Layout-only grouping uses `Container`, which supplies a row, column, stack, or
any other layout and has no other behavior. A parent sets each child's sizing
through layout overrides such as `LayoutOverride::flex_vertical()` and
`LayoutOverride::fixed_height()`. `Context::set_layout_override` is the one layout
setter. It replaces the node's whole override, and
`LayoutOverride::from(layout)` pins every field of a complete layout.

Painting and input resolve the same tracks. Wheel input on a track scrolls the
target with `Context::scroll_node` only when the step can move. A press starts
a drag that stores the target, the track, and the pointer's offset within the
thumb. Each later event and render resolves the track again. A drag continues
while the same target and track exist, against the target's current range. It
ends when either changes. It also ends when another node takes mouse capture,
without releasing that node's capture. Rendering cannot release capture, so a
render that finds the drag invalid draws no active thumb, and the next event
releases capture.

## Rendering

Rendering consumes current layout and view state. Canopy renders visible nodes in
tree order into an offscreen buffer and applies the cursor overlay. Published
snapshots and backend output use separate buffers. Observation can refresh the
published snapshot without changing the backend diff baseline. The baseline
advances only after output and backend flush succeed. Each emitted frame flushes
the backend once. Any backend failure
invalidates it, so the next attempt repaints in full. A failed write may have
already changed the terminal, making a repeated diff unsafe.

Widgets draw through `Render` in local coordinates. The runtime clips to the view,
translates to terminal coordinates, and applies style effects.

Mouse events reach a widget relative to its content origin, before scroll. The
location is signed: a point in padding above or left of the content is negative,
and a captured drag can lie anywhere. `View::outer_point()`,
`View::viewport_point()`, and `View::content_point()` return the cell under the
pointer in each coordinate space, or nothing when the pointer is outside it. A
widget hit-tests in the same space it paints.

`TermBuf` owns grapheme writes. It stores a base cell plus continuation cells for
wide graphemes, clips text by display columns, and clears stale continuation
cells when narrower text overwrites wider text.

Diff rendering must produce the same terminal state as a full repaint. Tests
replay diff operations into an in-memory backend and compare the resulting screen
with full render output.

If a pre-render hook marks layout dirty, Canopy runs layout again before
rendering. Rendering must not rely on stale views.

## Runtime Turns

`Canopy::turn(Work)` drives input, background wakes, evaluation start or cancellation,
and explicit preparation. `TurnOutcome` reports the published `FrameId`, evaluation
admission, unticketed completion, and exit status. Ticket-backed evaluations complete
through their `EvalTicket`. Crossterm, headless evaluation, and the test harness use
this driver.

`Work::Input` carries the input events that arrived together. The turn dispatches
them in order and prepares one frame, so a burst of input costs one render. Layout
settles before each mouse event that follows another event, so hit testing sees
current geometry. The terminal adapter drains waiting input into a bounded batch
and collapses consecutive pointer moves, and drags with the same button, to the
latest position.

Dispatch completion restores widget slots and applies queued removals before layout.
The driver services bounded native automation work, due polls, node wakes, and
ready VM segments. It then prepares layout, paints, and publishes changed state.
Evaluation tickets complete after preparation. Publication wakes parked predicates
for a later turn.

Headless evaluation drives these turns on Ruau's cached blocking runtime and
yields between ready turns. Each VM poll has an instruction quantum, independent
of the total gas and timeout limits. Synchronous script callbacks also use this
runtime bridge. No foreign executor or unconstrained Tokio scope drives scripts.
The MCP transport owns only factory handles and results; bounded blocking workers
own headless applications. Live evaluation stays on the UI thread, and dropping
a ticket wakes that driver to cancel its work.

The runtime records pending frame work as one invalidation level. The levels are
ordered: layout includes paint, paint includes the snapshot, and semantics
republishes the snapshot only. A mutation raises the level to what it needs and
never lowers it. Settling layout between batched events runs only when layout is
pending. Frame preparation runs when any level is pending, and it lays out and
paints the whole tree. Failed mutations retain invalidation for state they
changed. Read-only automation does not request a redraw. `Canopy::render`
prepares a frame and emits it to a given backend; headless MCP evaluation and
rendering tests use it. Scripts prepare pending changes with `canopy.flush()`
when they need snapshots or geometry before the next turn.

Every mutable widget callback invalidates layout before it runs, so any
mutable callback repaints the whole frame. There is no per-node damage
tracking. Widgets can therefore read other nodes while they render, and still
repaint when a callback changes what they read. canopy-fileselect relies on
this: its footer reads the file selector during render, and its listings read
another node's focus. Any narrower invalidation must keep this guarantee.

Poll deadlines belong to the driver. There is no eager scheduler thread per
application. The terminal adapter, headless evaluation, and the test
harness share one work selector. It waits on adapter input, runtime
notifications, and the next driver deadline, and it takes ready sources in
rotating order, so a source that stays ready cannot starve the others. Tests can
install `testing::ManualClock` before initialization, advance it, then deliver
`Work::Wake`. `Harness::wait_until` runs real turns through the same selector
until a condition holds, which is the native counterpart of Luau
`canopy.wait_for`.

## Event Routing

Input arrives as typed events. `Core` owns one flat `InputMap` with complete
records for application and framework bindings. Each record contains its
normalized input, tier, path matcher, description, source, target, phase, and
insertion order. Application targets call Luau functions or dispatch stored
commands. Framework targets dispatch commands.

A `BindingTier` is a binding's resolution layer, and its variant order is the
resolution order: `Framework(group)`, `Global`, `Mode(name)`, then `Default`.
The resolver checks the framework group that the top modal admits first.
Without one, it checks the global tier, active modes from newest to oldest, and
then the default tier. A transient mode ends that search, so a key it does not
bind resolves to nothing. Path specificity and insertion order select a winner
within one tier. `Setup::bind` installs a binding in any tier: a framework
tier is registered idempotently and takes a command or a widget action, and an
application tier replaces any binding with the same tier, input, and path.
Scripts install only the three application tiers.

The binding phase chooses dispatch before widget input or after the widget
ignores it. The default phase is `after_widget`, and the path filter has no
effect on the phase. Key and mouse bindings take either phase. The phase
belongs to the winner at one route node, so an early binding on an ancestor
still runs after every descendant declines.

An application can also bind a key to a named widget action instead of a
command or callback. The application registers each action name in its catalog
with `Setup::register_widget_action`. The catalog validates binding names and
renders the application's Luau API; a name promises a bindable operation, not a
consumer in every state. At each node the resolver ranks candidates, and an
action candidate is eligible only when that node's widget accepts the action in
its current state. A dormant action falls through to the next candidate at the
same node, so it never shadows a usable binding. An eligible action runs before
the node's raw key handler, so its stored phase is always `before_widget`, and
an explicit `after_widget` on one is an error. A release mismatch from a
widget that breaks its acceptance promise is treated as a decline, recorded in
the route trace, and never aborts the application. In a transient mode the
mode pops before its action runs, and an action without a consumer dismisses
the mode the way an unbound key does. A framework modal can admit named
application actions with `ModalBindings::FrameworkWithActions`, which admits
only the listed action names after its framework group and only while a widget
inside the modal accepts them.

Every widget predicts its keys. `Widget::key_outcome` returns the
`EventOutcome` that `on_event` would return for the same key and pre-event
state, and its default, `Ignore`, suits a widget that handles no keys. Routing
reads the prediction before each key reaches a widget and compares it with the
actual result. A mismatch records a `RouteTraceKind::Widget` entry and fails a
debug assertion, so tests catch a widget that forgot to predict. A widget that
cannot be read, because it is running the callback that asks, predicts `Ignore`
during analysis.

Routing and `available_bindings` call the same selection at each node in the
focus-to-root route. Availability returns an owned snapshot with one effective
winner per normalized key in `bindings`, and one per normalized mouse input in
`mouse_bindings`. An action row needs an accepting widget, while an ordinary
target claims the key when the route reaches it. Every row carries the same
description, tier, route, target kind, phase, command availability, and source.
The snapshot also names the framework group the top modal admits.

The mouse route starts at the requested node, as a click on it would. The
pointer's position plays no part, and hit testing and capture still choose the
real target. Key discovery asks each widget's `key_outcome` along the route:
`Handle` hides the after-widget bindings at that node and above, and `Ignore`
continues. An action without an accepting consumer does not appear. Diagnostic
binding output uses the same registry and reports why records are active,
dormant, shadowed, blocked by the top modal's framework group, or unmatched.

`Canopy::explain_key` walks the same route without acting and returns an owned
`KeyRouteExplanation`. Each examined step records its node, path, widget
prediction, and the binding selected there as one `StepBinding` of id, target
kind, and phase. The outcome is a `RouteOutcome`: `Binding` or `Transient` with
a `RouteWinner`, `Widget`, `TransientDismiss`, or `Unhandled`. A `RouteWinner`
names the binding, its node and path, its target kind, and its phase. The
explanation is exact for the current state, but it is advisory. Routing still
resolves and dispatches one node at a time, because an ignored widget can change
the tree, focus, or bindings before the route reaches an ancestor.
`available_bindings` projects its key results from the same analysis, so
discovery and explanation cannot disagree.

`Canopy::route_trace` records what routing actually did. Each entry has a
`RouteTraceKind`: `Start`, `BeforeWidgetBinding`, `OfferIntent`, `Widget`,
`AfterWidgetBinding`, `RunBinding`, `DefaultAction`, `Bubble`, `Handled`,
`Unhandled`, or `Notice`. A widget action offer traces as `OfferIntent`, and a
`Notice` entry ends a route whose binding or widget handler failed with a
notice. Labels are snake_case, such as `before_widget_binding`.

`Canopy::send_key_checked` analyzes the route, rejects a mismatched expectation
before delivery, then guards each normal route step and stops before an
unexpected consumer acts. It never selects a target from the stored analysis,
and it is not a transaction: earlier widgets may already have observed the key.

One label names an input everywhere. `Mouse` writes the spec `parse_spec`
reads back, such as `Ctrl+LeftDown`, and it carries no space, so help can
separate several inputs with one.

Mouse events go to the capture node when capture is active; otherwise hit-
testing chooses the target.

Routing visits each admitted node from the target toward the root. At each node
the widget receives the event, then a matching binding runs, then the runtime
applies the input's default action. The first of these that acts ends the
route; otherwise the route continues to the parent. An ancestor binding runs
only after every descendant declines. The route stops at the modal owner and
never applies a default action outside the modal region. Paste and focus-change
events take the same route from the focus to the modal owner, offered to
widgets only.

Wheel input is the only input with a default action. It scrolls the node by
the step that `event::mouse::Action::scroll_delta()` returns. The runtime
computes the clamped destination first. A step that cannot move declines, so
the wheel reaches the nearest ancestor that can move, and the node's pending
reveal survives. The route trace records each applied action as
`RouteTraceKind::DefaultAction`. Widgets that give the wheel another meaning, such
as a terminal that reports mouse input to its program, handle the event
themselves. A command that runs while an event is handled can take that event
as an injected `Event` or `MouseEvent` parameter.

A declarative binding whose command is not available consumes its input without
running it. The route stops there rather than offering the input to an ancestor,
so a control the user saw as unavailable cannot be acted on in its place.
Availability is read inside the event scope at dispatch, not taken from the last
rendered frame, and the route trace records the reason. An invoked command's own
failure, or an opaque script callback's, becomes a notice that consumes the
input; see [Notices](#notices).

Widget activation is a command like any other. `Button::press` runs the action a
button was built with, and `button.default_bindings()` binds unmodified
`LeftDown`, `Enter`, and `Space` to it under `**/button/**/`, so a click on the
label or the border resolves to the button containing it. The records are
ordinary application bindings that a configuration can replace or unbind.
`root.default_bindings()` installs them. A modal that admits only a framework
group must bind activation in that group itself.

Root captures a help snapshot before it opens a modal with `ModalOptions` and
`ModalBindings::Framework(HELP_BINDINGS)`. The scope dims the main pane, and the
help overlay draws only its panel, so the dimmed application stays visible
around it. The scope admits only Root-owned help controls until the modal
closes. Root then calls `close_modal` with the returned
`InteractionToken` and restores the original focus when that node remains live.
Removing or replacing a subtree retires modal scopes whose owner or modal widget
no longer belongs to the tree.

Key routing gives a transient mode the next key before any widget sees it. It
pops the mode, then runs the binding that the key resolved to, if there is one.
`Setup::register_mode_hook` registers a function that runs against the root
context before a frame whenever the mode stack has changed. Root uses one to
list the keys of a transient mode in a panel that overlays the main pane. The
help modal covers that panel, and hides it while help is open.
`Setup::register_notice_hook` does the same for the shown notice. Root uses
one to show the newest notice as one row at the bottom of the main pane.

## Focus and Mouse Capture

Focus is `Option<NodeId>`. A valid focus node exists, is attached to the root,
is not hidden, and accepts focus. Removal, replacement, and detachment record
the same recovery candidates when they take the focused node out of the tree.
Recovery prefers the next focusable node after the subtree, then the previous
node, then a focusable ancestor, then the first focusable node.

Structural changes check focus without views, because a node added or shown
since the last layout has none. A widget can focus such a node, for example in
`on_mount` or when it shows a page. Layout checks focus again after it publishes
views, and moves focus off a node that received no area.

Mouse capture is also `Option<NodeId>`. A valid capture node exists and is
attached to the root. Detaching or removing it clears capture.

Widgets define focusability. Directional focus depends on computed view
rectangles, so it depends on layout.

## Scripting Ownership

Scripts share the runtime state used by native Rust code. A script callback may
touch the tree only while Canopy has installed an execution context for that
callback. Canopy must restore the context when the callback returns an error.

Script-owned IDs, function handles, and binding handles are runtime capabilities.
They remain valid only while the app, node, script host, and registry entry remain
alive.

MCP and live automation cross the event-loop boundary. Work submitted from another
thread must marshal back to the UI thread before touching `Canopy` or `Core`.
`AutomationHandle::submit_eval` queues an `EvalRequest` and returns an `EvalTicket`.
Await its completion outside the UI thread. The queue applies bounded backpressure.

Only one top-level evaluation may be active. Another evaluation or module reload
receives `ScriptBusy`. Input and bounded native automation continue during parked
evaluations. A detached invocation holds no application, widget, or VM borrow
between polls. Each resumed segment restores its original script anchor.

## Failure and Panic Policy

Public Canopy APIs report expected failures with `Result` or `Option`: invalid
node IDs, invalid tree edits, re-entrant widget access, script errors, command
errors, layout failures, render failures, and runloop misuse. Application code
returns its own failures, such as I/O errors, unwrapped as `Error::App`, whose
script kind is `app`.

### Notices

A failure is either a notice or fatal, and `Error::is_notice` is the one rule
that decides:

- A command's failure (`Error::Command`), an application error
  (`Error::App`), and an error a script raises are notices.
- A widget operation failure takes the class of its source.
- Timeouts, cancellation, and runtime invariant, backend, render, and layout
  failures are fatal.

The runtime applies the rule where input and background work fail: a binding's
command or callback, a widget's event or intent handler, and a widget's poll.
It records a notice and keeps running. The failed input counts as consumed, and
the route trace ends with a `Notice` entry. A failed poll keeps polling at the
delay its last successful poll requested. A fatal failure still returns from
`Canopy::turn`, which ends the terminal run loop. A script call is not input,
so it raises every failure as a script error; a key that a script sends is
input, so a failure it causes is a notice.

`Canopy::notices` and Luau `canopy.notices()` return the newest 32 notices,
oldest first. A `Notice` holds the message, the failure's `ScriptErrorKind`, a
`NoticeSource` (`Binding`, `Widget`, or `Poll`), and the node when known. The
newest notice is shown from its record until the next key, paste, or mouse
action other than a bare move. `ViewContext::notice` returns it while it is
shown, and a notice hook runs whenever that changes, before the frame after a
record, and before the dismissing input routes. Root's hook shows the notice as
one row at the bottom of the main pane, styled `root/notice`, and hides the row
otherwise, so the row takes no clicks while it is empty.

A failed command changes no structure. Its completion boundary drops the
removals and modal closes it queued, while changes to widget state stand. A
command that closes a dialog and then does fallible work therefore does the
work first, so a failure leaves the dialog open and consistent.

Panics are for tests and impossible internal bugs. A panic in public library code
needs a clear invariant and a test around the surrounding behavior.

Do not hide internal errors behind harmless defaults. If layout cannot measure a
node, canvas computation cannot access a widget, or the runloop has consumed its
event receiver, return a typed error with enough context to debug the phase and
node.
