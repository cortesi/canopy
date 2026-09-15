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

Measurement is an infallible widget hook. A widget returns a fixed content size
or asks layout to wrap visible children. Layout may measure a widget several
times in one pass.

Canvas calculation is also infallible. It returns the scrollable content extent,
which is at least the content size. Layout clamps scroll after every pass.

`Context::scroll_into_view()` queues a content rectangle to reveal after canvas
sizing, before views are published. This lets a widget expand and reveal a row
in the same turn. The latest request wins and explicit scrolling cancels it.
Requests survive hidden, detached, and zero-sized views, but widget replacement
clears them. A successful nonempty layout consumes each request once.

Hidden nodes and `Display::None` nodes do not participate in visible layout.
Layout clears their subtree caches.

Layout errors must surface. Re-entrant widget access and missing nodes must not
become zero measurements or fallback canvases.

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
