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
facade traits and their handle types: `Canopy`, `Widget`, `Context`,
`ViewContext`, `Render`, `NodeName`, `View`, the capability context traits,
typed node IDs, and the command macros. Value libraries live in their modules:
`canopy::geom`, `canopy::layout`, `canopy::style`, `canopy::event`,
`canopy::path`, `canopy::script`, `canopy::error`, `canopy::help`,
`canopy::cursor`, `canopy::text`, and `canopy::render` (backend interfaces).
Each item has one canonical location, and there is no prelude.

`Canopy` owns `Core` and the style map. Its fields are private. Apps install root
widgets with helpers such as `Root::install`, mutate styles through
`Canopy::style_mut()`, and use `Canopy` methods for scripting, fixtures, input
modes, rendering, and automation.

Lower-level runtime state is crate-private. `Core`, `inputmap`, and raw arena
mutation are not reachable from app code, and `script` and the backend modules
expose only what the stable surface above needs.

Path-oriented APIs use `Path`, `PathFilter`, and `NodeName`. Literal path
components must be valid node names. Raw script path strings are validated at the
Luau boundary before matching.

## Widget capabilities

`canopy-widgets` enables its complete bundle by default. Basic forms can set
`default-features = false`; Input, List, Root, and Help remain available. The
shared `canopy_widgets::text_buffer` module is independent of Editor. Editor
does not re-export text-buffer types; it uses `display_width` internally when
enabled.

| Feature | Additional widgets and dependencies |
| --- | --- |
| `editor` | Editor and Syntect syntax highlighting |
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
It also restores root, focus, mouse capture, focus recovery hints, exit requests,
pending style changes, and pending diagnostic requests.
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
on detach. Reattachment starts a new attachment generation. Hiding ends neither
lifetime. `Widget::poll_lifetime()` defaults to node lifetime, preserving detached
terminal polling. Attachment polling initializes again after reattachment.

Each `Widget::poll` result replaces its previous timer. `Some(delay)` schedules
another poll, with delays below one millisecond rounded up to let the adapter
sleep. `None` cancels scheduled polling, including when an early node wake
triggered the callback. Explicit wakes remain immediate.

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
keys, focus, mouse capture, lifecycle flags, layout caches, computed view caches,
the semantic-key index, and pending diagnostic targets.

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

Removing or replacing siblings is allowed. Removing the focused node recovers
focus immediately. Removing the mouse-capture node clears capture immediately.
Removed `NodeId`s become invalid immediately.

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

Scrolling and revealing are separate. `Context::scroll_to()`, `scroll_by()`,
and `scroll_to_of()` move a view at once. `Context::reveal_area()`,
`reveal_anchor()`, and `reveal_node()` queue requests that run after layout
settles the geometry they show, so a widget can change content and reveal it in
the same turn. `reveal_anchor()` asks `Widget::reveal_anchor()` for a rectangle
using the final content size. The editor reveals its cursor this way.

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

Hidden nodes and `Display::None` nodes do not participate in visible layout.
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
`LayoutOverride::fixed_height()`.

Painting and input resolve the same tracks. Wheel input on a track scrolls the
target with `Context::scroll_to_of` only when the step can move. A press starts
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
advances only after output and backend flush succeed. Any backend failure
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

`ChangeSet` tracks layout, paint, cursor, and observation invalidation. Mutable
widget access and accepted runtime mutations record the required work. Failed
mutations retain invalidation for state they changed. Read-only automation does
not request a redraw. `Canopy::render` remains an explicit preparation and emission
path for isolated rendering tests. `Canopy::flush` prepares pending changes for
native callers that need snapshots or geometry before the next turn.

Poll deadlines belong to the driver. There is no eager scheduler thread per
application. Adapters wait on terminal input, runtime notifications, and
`Canopy::next_deadline()` with fair ready-source selection. Tests can install
`testing::ManualClock` before initialization, advance it, then deliver `Work::Wake`.

## Event Routing

Input arrives as typed events. `Core` owns one flat `InputMap` with complete
records for application and framework bindings. Each record contains its
normalized input, owner, scope, path matcher, description, source, target, and
insertion order. Application targets call Luau functions or dispatch stored
commands. Framework targets dispatch commands.

The resolver checks the framework binding group admitted by the top modal scope
first. Without one, it checks the global scope, active modes from newest to
oldest, and then the default scope. A transient mode ends that search, so a key
it does not bind resolves to nothing. Path specificity and insertion order select
a winner within one scope. The binding phase chooses dispatch before widget
input or after the widget ignores it. The default phase is `after_widget`, and
the path filter has no effect on the phase. Mouse bindings run after ignored
widget input.

Key routing and `available_bindings` call the same resolver at each node in the
focus-to-root route. Availability returns an owned snapshot with one effective
winner per normalized key. It does not include mouse bindings. Diagnostic
binding output uses the same registry and reports why records are active,
shadowed, blocked by the top modal's framework group, or unmatched.

Mouse events go to the capture node when capture is active; otherwise hit-
testing chooses the target.

Routing visits each admitted node from the target toward the root. At each node
the widget receives the event, then a matching binding runs, then the runtime
applies the input's default action. The first of these that acts ends the
route; otherwise the route continues to the parent. An ancestor binding runs
only after every descendant declines. The route stops at the modal owner and
never applies a default action outside the modal region.

Wheel input is the only input with a default action. It scrolls the node by
the step that `event::mouse::Action::scroll_delta()` returns. The runtime
computes the clamped destination first. A step that cannot move declines, so
the wheel reaches the nearest ancestor that can move, and the node's pending
reveal survives. The route trace records each applied action as
`RoutePhase::DefaultAction`. Widgets that give the wheel another meaning, such
as a terminal that reports mouse input to its program, handle the event
themselves. Command scopes expose the originating event and target.

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
`Canopy::register_mode_hook` registers a function that runs against the root
context before a frame whenever the mode stack has changed. Root uses one to
list the keys of a transient mode in a panel that overlays the main pane. The
help modal covers that panel, and hides it while help is open.

## Focus and Mouse Capture

Focus is `Option<NodeId>`. A valid focus node exists and is attached to the root.
After removal, recovery prefers the next focusable node, then the previous node,
then a focusable ancestor.

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
errors, layout failures, render failures, and runloop misuse.

Panics are for tests and impossible internal bugs. A panic in public library code
needs a clear invariant and a test around the surrounding behavior.

Do not hide internal errors behind harmless defaults. If layout cannot measure a
node, canvas computation cannot access a widget, or the runloop has consumed its
event receiver, return a typed error with enough context to debug the phase and
node.
